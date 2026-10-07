// SPDX-License-Identifier: AGPL-3.0-or-later

//! `tools roads --osm <overpass.json> --map-dir <dir>`
//!
//! Reads an Overpass `out geom` JSON of highway ways, projects WGS84 to UTM zone 36N
//! (EPSG:32636) and then to sim metres (x = E - origin_x, y = origin_y - N, so sim sample
//! (i, j) of `map.toml` is at (i * cell, j * cell)). Writes `roads.bin` (u8 class per sim
//! cell), `roads_lines.json` (clipped, simplified polylines) and `roads.toml` (summary).

use crate::Res;
use serde::Deserialize;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

/// Sampling step along a segment when rasterising.
const RASTER_STEP_M: f64 = 10.0;
/// Polyline simplification for drawing: drop points closer than this to the last kept one.
const SIMPLIFY_M: f64 = 25.0;

#[derive(Deserialize)]
struct Overpass {
    #[serde(default)]
    osm3s: Option<Osm3s>,
    elements: Vec<Element>,
}

#[derive(Deserialize)]
struct Osm3s {
    timestamp_osm_base: Option<String>,
}

#[derive(Deserialize)]
struct Element {
    #[serde(default)]
    tags: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    geometry: Vec<LatLon>,
}

#[derive(Deserialize)]
struct LatLon {
    lat: f64,
    lon: f64,
}

#[derive(Deserialize)]
struct MapToml {
    width: usize,
    height: usize,
    cell_size_m: f64,
    origin_x: f64,
    origin_y: f64,
}

/// 0 = none, 1 = track/service, 2 = minor, 3 = major.
fn road_class(highway: &str) -> u8 {
    match highway {
        "motorway" | "trunk" | "primary" | "secondary" => 3,
        "tertiary" | "unclassified" | "residential" => 2,
        "track" | "service" => 1,
        _ => 0,
    }
}

/// WGS84 -> UTM (zone with central meridian `lon0_deg`, northern hemisphere), Krüger
/// series to n^4 (Karney 2011, "Transverse Mercator with an accuracy of a few nanometers",
/// eqs. 7, 9, 35; sub-mm within a UTM zone). Returns (easting, northing) in metres.
pub fn utm_forward(lat_deg: f64, lon_deg: f64, lon0_deg: f64) -> (f64, f64) {
    const A: f64 = 6_378_137.0;
    const F: f64 = 1.0 / 298.257_223_563;
    const K0: f64 = 0.9996;
    let n = F / (2.0 - F);
    let (n2, n3, n4) = (n * n, n * n * n, n * n * n * n);
    let big_a = A / (1.0 + n) * (1.0 + n2 / 4.0 + n4 / 64.0);
    let alpha = [
        n / 2.0 - 2.0 * n2 / 3.0 + 5.0 * n3 / 16.0 + 41.0 * n4 / 180.0,
        13.0 * n2 / 48.0 - 3.0 * n3 / 5.0 + 557.0 * n4 / 1440.0,
        61.0 * n3 / 240.0 - 103.0 * n4 / 140.0,
        49_561.0 * n4 / 161_280.0,
    ];
    let phi = lat_deg.to_radians();
    let lam = (lon_deg - lon0_deg).to_radians();
    let e = 2.0 * n.sqrt() / (1.0 + n); // first eccentricity
    let t = (phi.sin().atanh() - e * (e * phi.sin()).atanh()).sinh();
    let xi = t.atan2(lam.cos());
    let eta = (lam.sin() / (1.0 + t * t).sqrt()).atanh();
    let (mut x, mut y) = (eta, xi);
    for (j, a) in alpha.iter().enumerate() {
        let k = 2.0 * (j + 1) as f64;
        x += a * (k * xi).cos() * (k * eta).sinh();
        y += a * (k * xi).sin() * (k * eta).cosh();
    }
    (500_000.0 + K0 * big_a * x, K0 * big_a * y)
}

/// Liang-Barsky: clip segment a-b to [0, xmax] x [0, ymax].
fn clip(a: [f64; 2], b: [f64; 2], xmax: f64, ymax: f64) -> Option<([f64; 2], [f64; 2])> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [
        (-dx, a[0]),
        (dx, xmax - a[0]),
        (-dy, a[1]),
        (dy, ymax - a[1]),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                t0 = t0.max(r);
            } else {
                t1 = t1.min(r);
            }
        }
    }
    (t0 <= t1).then(|| {
        let at = |t: f64| [a[0] + t * dx, a[1] + t * dy];
        (at(t0), at(t1))
    })
}

/// Splits a polyline into the pieces that lie inside the map rectangle.
fn clip_polyline(pts: &[[f64; 2]], xmax: f64, ymax: f64) -> Vec<Vec<[f64; 2]>> {
    let mut out: Vec<Vec<[f64; 2]>> = Vec::new();
    let mut cur: Vec<[f64; 2]> = Vec::new();
    for w in pts.windows(2) {
        match clip(w[0], w[1], xmax, ymax) {
            Some((p, q)) => {
                if cur.last() != Some(&p) {
                    if cur.len() >= 2 {
                        out.push(std::mem::take(&mut cur));
                    }
                    cur = vec![p];
                }
                cur.push(q);
            }
            None => {
                if cur.len() >= 2 {
                    out.push(std::mem::take(&mut cur));
                }
                cur.clear();
            }
        }
    }
    if cur.len() >= 2 {
        out.push(cur);
    }
    out
}

/// Keeps the first and last point and every point >= `SIMPLIFY_M` from the last kept one.
fn simplify(pts: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut out: Vec<[f64; 2]> = Vec::new();
    for (k, &p) in pts.iter().enumerate() {
        let far = out
            .last()
            .is_none_or(|q| (p[0] - q[0]).hypot(p[1] - q[1]) >= SIMPLIFY_M);
        if far || k + 1 == pts.len() {
            if k + 1 == pts.len() && !far && out.len() >= 2 {
                out.pop();
            }
            out.push(p);
        }
    }
    out
}

pub fn run(osm: &Path, map_dir: &Path) -> Res<String> {
    let map: MapToml = toml::from_str(&fs::read_to_string(map_dir.join("map.toml"))?)?;
    let data: Overpass = serde_json::from_str(&fs::read_to_string(osm)?)?;
    let (w, h, cell) = (map.width, map.height, map.cell_size_m);
    if w == 0 || h == 0 || !(cell.is_finite() && cell > 0.0) {
        return Err("map.toml: width, height and cell_size_m must be > 0".into());
    }
    let (xmax, ymax) = ((w - 1) as f64 * cell, (h - 1) as f64 * cell);

    let mut grid = vec![0u8; w * h];
    let mut lines = Vec::new();
    let mut ways = [0usize; 4];
    for el in &data.elements {
        let class = el.tags.get("highway").map_or(0, |hw| road_class(hw));
        if class == 0 {
            continue;
        }
        let pts: Vec<[f64; 2]> = el
            .geometry
            .iter()
            .map(|g| {
                let (e, n) = utm_forward(g.lat, g.lon, 33.0);
                [e - map.origin_x, map.origin_y - n]
            })
            .collect();
        let pieces = clip_polyline(&pts, xmax, ymax);
        if !pieces.is_empty() {
            ways[usize::from(class)] += 1;
        }
        for piece in pieces {
            for s in piece.windows(2) {
                let len = (s[1][0] - s[0][0]).hypot(s[1][1] - s[0][1]);
                let steps = (len / RASTER_STEP_M).ceil().max(1.0) as usize;
                for k in 0..=steps {
                    let t = k as f64 / steps as f64;
                    let x = s[0][0] + t * (s[1][0] - s[0][0]);
                    let y = s[0][1] + t * (s[1][1] - s[0][1]);
                    // Clipped to [0, xmax] so the rounded index is always in range.
                    let i = ((x / cell).round() as usize).min(w - 1);
                    let j = ((y / cell).round() as usize).min(h - 1);
                    let c = &mut grid[j * w + i];
                    *c = (*c).max(class);
                }
            }
            let pts: Vec<[f64; 2]> = simplify(&piece)
                .iter()
                .map(|p| [(p[0] * 10.0).round() / 10.0, (p[1] * 10.0).round() / 10.0])
                .collect();
            lines.push(serde_json::json!({ "class": class, "points": pts }));
        }
    }

    let mut cells = [0usize; 4];
    for &c in &grid {
        cells[usize::from(c)] += 1;
    }
    let date = data
        .osm3s
        .and_then(|o| o.timestamp_osm_base)
        .unwrap_or_else(|| "unknown".to_string());
    let mut toml = String::new();
    writeln!(
        toml,
        "# Generated by `tools roads`. roads.bin: u8 per sim cell, row-major, {w}x{h} like\n\
         # sim_height.bin. 0 = none, 1 = track/service, 2 = minor (tertiary/unclassified/\n\
         # residential), 3 = major (secondary and above). roads_lines.json: polylines in sim\n\
         # metres (x east, y south from sample (0, 0)), simplified to >= {SIMPLIFY_M} m spacing.\n\
         width = {w}\n\
         height = {h}\n\
         cells = \"roads.bin\"\n\
         lines = \"roads_lines.json\"\n\
         road_cells = {rc}\n\
         source = \"OpenStreetMap via Overpass API, file {src}\"\n\
         osm_timestamp = \"{date}\"\n\
         license = \"(c) OpenStreetMap contributors, ODbL 1.0 (https://www.openstreetmap.org/copyright)\"\n\
         \n\
         [class_1_track_service]\nways = {w1}\ncells = {c1}\n\
         \n\
         [class_2_minor]\nways = {w2}\ncells = {c2}\n\
         \n\
         [class_3_major]\nways = {w3}\ncells = {c3}",
        rc = cells[1] + cells[2] + cells[3],
        src = osm.file_name().map_or("?".into(), |f| f.to_string_lossy()),
        w1 = ways[1],
        w2 = ways[2],
        w3 = ways[3],
        c1 = cells[1],
        c2 = cells[2],
        c3 = cells[3],
    )?;

    fs::write(map_dir.join("roads.bin"), &grid)?;
    fs::write(
        map_dir.join("roads_lines.json"),
        serde_json::to_string(&serde_json::json!({ "lines": lines }))?,
    )?;
    fs::write(map_dir.join("roads.toml"), toml)?;
    Ok(format!(
        "{} lines, road cells by class 1/2/3: {}/{}/{} -> {}",
        lines.len(),
        cells[1],
        cells[2],
        cells[3],
        map_dir.display()
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Snyder 1987 (USGS PP 1395) eqs. 8-9 to 8-13: an independent series to cross-check
    /// the Krüger implementation.
    fn snyder(lat: f64, lon: f64, lon0: f64) -> (f64, f64) {
        let a = 6_378_137.0f64;
        let f = 1.0 / 298.257_223_563;
        let e2 = f * (2.0 - f);
        let ep2 = e2 / (1.0 - e2);
        let k0 = 0.9996;
        let p = lat.to_radians();
        let nn = a / (1.0 - e2 * p.sin().powi(2)).sqrt();
        let t = p.tan().powi(2);
        let c = ep2 * p.cos().powi(2);
        let aa = (lon - lon0).to_radians() * p.cos();
        let m = a
            * ((1.0 - e2 / 4.0 - 3.0 * e2 * e2 / 64.0 - 5.0 * e2.powi(3) / 256.0) * p
                - (3.0 * e2 / 8.0 + 3.0 * e2 * e2 / 32.0 + 45.0 * e2.powi(3) / 1024.0)
                    * (2.0 * p).sin()
                + (15.0 * e2 * e2 / 256.0 + 45.0 * e2.powi(3) / 1024.0) * (4.0 * p).sin()
                - (35.0 * e2.powi(3) / 3072.0) * (6.0 * p).sin());
        let x = k0
            * nn
            * (aa
                + (1.0 - t + c) * aa.powi(3) / 6.0
                + (5.0 - 18.0 * t + t * t + 72.0 * c - 58.0 * ep2) * aa.powi(5) / 120.0);
        let y = k0
            * (m + nn
                * p.tan()
                * (aa * aa / 2.0
                    + (5.0 - t + 9.0 * c + 4.0 * c * c) * aa.powi(4) / 24.0
                    + (61.0 - 58.0 * t + t * t + 600.0 * c - 330.0 * ep2) * aa.powi(6) / 720.0));
        (500_000.0 + x, y)
    }

    #[test]
    fn f1_2_tm_projects_hasandag_summit() {
        let (e, n) = utm_forward(38.1275, 34.1667, 33.0);
        // Independent formula agrees to centimetres.
        let (se, sn) = snyder(38.1275, 34.1667, 33.0);
        assert!(
            (e - se).abs() < 0.05 && (n - sn).abs() < 0.05,
            "{e} {n} vs {se} {sn}"
        );
        // Fixed reference (Snyder series, this file; checked to agree with the Python
        // reimplementation used while writing F1-2): 602257.24 E, 4220604.42 N.
        assert!((e - 602_257.24).abs() < 0.1 && (n - 4_220_604.42).abs() < 0.1);
        // docs/maps.md rounds the summit to 602250 / 4220600 (~8 m off, box centre).
        assert!((e - 602_250.0).abs() < 10.0 && (n - 4_220_600.0).abs() < 10.0);
        // Central meridian maps to the false easting.
        assert!((utm_forward(38.0, 33.0, 33.0).0 - 500_000.0).abs() < 1e-6);
    }

    #[test]
    fn f1_2_rasterises_synthetic_osm() {
        let dir = std::env::temp_dir().join(format!("f1_2_roads_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        // Origin chosen so that sim (0, 0) sits on a known lat/lon.
        let (e0, n0) = utm_forward(38.0, 34.0, 33.0);
        fs::write(
            dir.join("map.toml"),
            format!(
                "width = 10\nheight = 10\ncell_size_m = 50\norigin_x = {e0}\norigin_y = {n0}\n"
            ),
        )
        .unwrap();
        // Lat/lon of sim metres (x, y), found by a few Newton steps on utm_forward.
        let ll = |x: f64, y: f64| {
            let (mut lat, mut lon) = (38.0, 34.0);
            for _ in 0..5 {
                let (e, n) = utm_forward(lat, lon, 33.0);
                lon += (e0 + x - e) / 87_800.0;
                lat += (n0 - y - n) / 111_000.0;
            }
            format!("{{\"lat\": {lat}, \"lon\": {lon}}}")
        };
        // Major road along row 2 from x=0 to x=450 (whole row); track down column 5 from
        // row 0 to row 2 ending on it; a path (unknown class) that must be ignored; a minor
        // road entering from outside the map (x=-200) to (100, 400).
        let json = format!(
            r#"{{"osm3s": {{"timestamp_osm_base": "2026-01-01T00:00:00Z"}}, "elements": [
              {{"type": "way", "tags": {{"highway": "secondary"}}, "geometry": [{}, {}]}},
              {{"type": "way", "tags": {{"highway": "track"}}, "geometry": [{}, {}]}},
              {{"type": "way", "tags": {{"highway": "path"}}, "geometry": [{}, {}]}},
              {{"type": "way", "tags": {{"highway": "residential"}}, "geometry": [{}, {}]}}
            ]}}"#,
            ll(0.0, 100.0),
            ll(450.0, 100.0),
            ll(250.0, 0.0),
            ll(250.0, 100.0),
            ll(0.0, 450.0),
            ll(450.0, 450.0),
            ll(-200.0, 400.0),
            ll(100.0, 400.0),
        );
        let osm = dir.join("osm.json");
        fs::write(&osm, json).unwrap();
        run(&osm, &dir).unwrap();

        let g = fs::read(dir.join("roads.bin")).unwrap();
        assert_eq!(g.len(), 100);
        let at = |i: usize, j: usize| g[j * 10 + i];
        for i in 0..10 {
            assert_eq!(at(i, 2), 3, "row 2 col {i}");
        }
        assert_eq!(at(5, 0), 1);
        assert_eq!(at(5, 1), 1);
        assert_eq!(at(5, 2), 3); // crossing keeps the max class
        assert_eq!(at(5, 3), 0);
        assert_eq!((at(0, 8), at(2, 8), at(3, 8), at(0, 9)), (2, 2, 0, 0));
        assert_eq!(g.iter().filter(|&&c| c > 0).count(), 10 + 2 + 3);

        let lines: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join("roads_lines.json")).unwrap())
                .unwrap();
        let lines = lines["lines"].as_array().unwrap();
        assert_eq!(lines.len(), 3);
        // The minor road is clipped at the west edge.
        let p0 = &lines[2]["points"][0];
        assert_eq!(lines[2]["class"], 2);
        assert!(p0[0].as_f64().unwrap().abs() < 0.01);

        let toml = fs::read_to_string(dir.join("roads.toml")).unwrap();
        assert!(toml.contains("road_cells = 15"), "{toml}");
        assert!(toml.contains("ODbL 1.0"));
        assert!(toml.contains("2026-01-01T00:00:00Z"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn f1_2_simplify_drops_close_points() {
        let s = simplify(&[
            [0.0, 0.0],
            [10.0, 0.0],
            [30.0, 0.0],
            [40.0, 0.0],
            [70.0, 0.0],
        ]);
        assert_eq!(s, vec![[0.0, 0.0], [30.0, 0.0], [70.0, 0.0]]);
        // A last point close to the previous kept one replaces it.
        let s = simplify(&[[0.0, 0.0], [30.0, 0.0], [40.0, 0.0]]);
        assert_eq!(s, vec![[0.0, 0.0], [40.0, 0.0]]);
    }
}
