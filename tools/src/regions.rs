// SPDX-License-Identifier: AGPL-3.0-or-later

//! `tools regions --osm <overpass.json> --map-dir <dir> [--min-dist 2000] [--extra name:x:y]...`
//!
//! Splits the land into named regions (K-063). Region centres are the named OSM `place`
//! nodes on land, taken in the order town, village, suburb, hamlet, neighbourhood (then
//! by name) and skipped when closer than `--min-dist` metres to an already chosen centre;
//! `--extra` centres (sim metres) are always taken. Every land cell belongs to the nearest
//! centre. Ids run north to south. Writes
//! - `regions.toml`: id, name, kind, centre, cell count, bordering regions and the
//!   regions reachable directly by road,
//! - `regions.bin`: u8 region id per sim cell (0 = water),
//! - `regions_lines.json`: border segments per pair of regions, for drawing.

use crate::cover::{load_sim_map, LatLon, SimMap};
use crate::Res;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

/// Centres keep this distance from the map edge.
const EDGE_MARGIN_M: f64 = 300.0;

#[derive(Deserialize)]
struct Overpass {
    elements: Vec<Element>,
}

#[derive(Deserialize)]
struct Element {
    #[serde(flatten)]
    at: Option<LatLon>,
    #[serde(default)]
    tags: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct Region {
    id: u8,
    name_tr: String,
    kind: String,
    x: f64,
    y: f64,
    cells: usize,
    neighbours: Vec<u8>,
    road_links: Vec<u8>,
}

#[derive(Serialize)]
struct RegionsToml {
    region: Vec<Region>,
}

fn place_rank(kind: &str) -> Option<u8> {
    ["town", "village", "suburb", "hamlet", "neighbourhood"]
        .iter()
        .position(|k| *k == kind)
        .map(|r| r as u8)
}

/// `name:x:y` in sim metres.
fn parse_extra(s: &str) -> Res<(String, [f64; 2])> {
    let mut parts = s.rsplitn(3, ':');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(y), Some(x), Some(name)) if !name.is_empty() => {
            Ok((name.to_string(), [x.parse()?, y.parse()?]))
        }
        _ => Err(format!("--extra wants name:x:y, got '{s}'").into()),
    }
}

/// Chosen centres as (name, kind, position), sorted north to south.
fn pick_centres(
    map: &SimMap,
    data: &Overpass,
    min_dist_m: f64,
    extras: &[String],
) -> Res<Vec<(String, String, [f64; 2])>> {
    let (xmax, ymax) = ((map.w - 1) as f64 * map.cell, (map.h - 1) as f64 * map.cell);
    let usable = |p: [f64; 2]| {
        p[0] >= EDGE_MARGIN_M
            && p[1] >= EDGE_MARGIN_M
            && p[0] <= xmax - EDGE_MARGIN_M
            && p[1] <= ymax - EDGE_MARGIN_M
            && !map.is_water_at(p)
    };
    let mut chosen = Vec::new();
    for e in extras {
        let (name, p) = parse_extra(e)?;
        if !usable(p) {
            return Err(format!("--extra '{name}' is on water or off the map").into());
        }
        chosen.push((name, "point".to_string(), p));
    }
    let mut places: Vec<(u8, &str, &str, [f64; 2])> = data
        .elements
        .iter()
        .filter_map(|el| {
            let kind = el.tags.get("place")?;
            // Whole metres: stable ordering and tidy data.
            let p = map.project(el.at.as_ref()?).map(f64::round);
            Some((
                place_rank(kind)?,
                el.tags.get("name")?.as_str(),
                kind.as_str(),
                p,
            ))
        })
        .filter(|c| usable(c.3))
        .collect();
    places.sort_by(|a, b| {
        (a.0, a.1)
            .cmp(&(b.0, b.1))
            .then(a.3[0].total_cmp(&b.3[0]))
            .then(a.3[1].total_cmp(&b.3[1]))
    });
    for (_, name, kind, p) in places {
        let far = chosen.iter().all(|c: &(String, String, [f64; 2])| {
            (p[0] - c.2[0]).hypot(p[1] - c.2[1]) >= min_dist_m
        });
        if far {
            // Two places may share a name (a village and a quarter of the next town).
            let taken = chosen.iter().any(|c| c.0 == name);
            let name = match (taken, kind) {
                (false, _) => name.to_string(),
                (true, "suburb" | "neighbourhood") => format!("{name} Mahallesi"),
                (true, _) => format!("{name} 2"),
            };
            chosen.push((name, kind.to_string(), p));
        }
    }
    chosen.sort_by(|a, b| a.2[1].total_cmp(&b.2[1]).then(a.2[0].total_cmp(&b.2[0])));
    if chosen.is_empty() || chosen.len() > 255 {
        return Err(format!("{} region centres; need 1..=255", chosen.len()).into());
    }
    Ok(chosen)
}

type Borders = BTreeMap<(u8, u8), Vec<[f64; 4]>>;

/// Adds one cell-edge segment to the border of regions `a` and `b`, extending the last
/// segment when it continues in a straight line.
fn add_border(borders: &mut Borders, a: u8, b: u8, seg: [f64; 4]) {
    let list = borders.entry((a.min(b), a.max(b))).or_default();
    match list.last_mut() {
        Some(last)
            if last[2] == seg[0]
                && last[3] == seg[1]
                && ((last[0] == last[2] && seg[0] == seg[2])
                    || (last[1] == last[3] && seg[1] == seg[3])) =>
        {
            last[2] = seg[2];
            last[3] = seg[3];
        }
        _ => list.push(seg),
    }
}

pub fn run(osm: &Path, map_dir: &Path, min_dist_m: f64, extras: &[String]) -> Res<String> {
    let map = load_sim_map(map_dir)?;
    let data: Overpass = serde_json::from_str(&fs::read_to_string(osm)?)?;
    let centres = pick_centres(&map, &data, min_dist_m, extras)?;
    let (w, h, cell) = (map.w, map.h, map.cell);

    // Nearest centre per land cell; ties go to the lower id.
    let mut grid = vec![0u8; w * h];
    let mut cells = vec![0usize; centres.len() + 1];
    for j in 0..h {
        for i in 0..w {
            if map.is_water(i, j) {
                continue;
            }
            let (x, y) = (i as f64 * cell, j as f64 * cell);
            let mut best = (f64::MAX, 0u8);
            for (k, c) in centres.iter().enumerate() {
                let d = (x - c.2[0]).powi(2) + (y - c.2[1]).powi(2);
                if d < best.0 {
                    best = (d, k as u8 + 1);
                }
            }
            grid[j * w + i] = best.1;
            cells[usize::from(best.1)] += 1;
        }
    }

    let at = |i: usize, j: usize| grid[j * w + i];
    let mut borders = Borders::new();
    // Vertical edges (between west/east neighbours), column by column so runs merge.
    for i in 0..w - 1 {
        for j in 0..h {
            let (a, b) = (at(i, j), at(i + 1, j));
            if a != b && a != 0 && b != 0 {
                let x = (i as f64 + 0.5) * cell;
                let (y0, y1) = ((j as f64 - 0.5) * cell, (j as f64 + 0.5) * cell);
                add_border(&mut borders, a, b, [x, y0.max(0.0), x, y1]);
            }
        }
    }
    // Horizontal edges (between north/south neighbours), row by row.
    for j in 0..h - 1 {
        for i in 0..w {
            let (a, b) = (at(i, j), at(i, j + 1));
            if a != b && a != 0 && b != 0 {
                let y = (j as f64 + 0.5) * cell;
                let (x0, x1) = ((i as f64 - 0.5) * cell, (i as f64 + 0.5) * cell);
                add_border(&mut borders, a, b, [x0.max(0.0), y, x1, y]);
            }
        }
    }

    // Road links: a road cell touching a road cell of another region.
    let mut links: BTreeSet<(u8, u8)> = BTreeSet::new();
    let roads_path = map_dir.join("roads.bin");
    if roads_path.exists() {
        let roads = fs::read(roads_path)?;
        if roads.len() != w * h {
            return Err("roads.bin: size does not match map.toml".into());
        }
        for j in 0..h {
            for i in 0..w {
                if roads[j * w + i] == 0 || at(i, j) == 0 {
                    continue;
                }
                for (ni, nj) in [
                    (i + 1, j),
                    (i, j + 1),
                    (i + 1, j + 1),
                    (i.wrapping_sub(1), j + 1),
                ] {
                    if ni < w && nj < h && roads[nj * w + ni] > 0 {
                        let (a, b) = (at(i, j), at(ni, nj));
                        if a != b && b != 0 {
                            links.insert((a.min(b), a.max(b)));
                        }
                    }
                }
            }
        }
    }

    let others = |pairs: &mut dyn Iterator<Item = (u8, u8)>, id: u8| -> Vec<u8> {
        pairs
            .filter_map(|(a, b)| match (a == id, b == id) {
                (true, _) => Some(b),
                (_, true) => Some(a),
                _ => None,
            })
            .collect()
    };
    let region: Vec<Region> = centres
        .iter()
        .enumerate()
        .map(|(k, (name, kind, p))| {
            let id = k as u8 + 1;
            Region {
                id,
                name_tr: name.clone(),
                kind: kind.clone(),
                x: p[0],
                y: p[1],
                cells: cells[usize::from(id)],
                neighbours: others(&mut borders.keys().copied(), id),
                road_links: others(&mut links.iter().copied(), id),
            }
        })
        .collect();
    let n = region.len();
    let header = format!(
        "# Generated by `tools regions`. Region centres are OSM place nodes (min. {min_dist_m} m\n\
         # apart) plus --extra points; every land cell belongs to the nearest centre.\n\
         # regions.bin: u8 region id per sim cell, row-major like sim_height.bin, 0 = water.\n\
         # regions_lines.json: {{\"borders\": [{{\"a\": id, \"b\": id, \"segments\": [[x0, y0, x1, y1]]}}]}},\n\
         # sim metres. neighbours = regions sharing a border; road_links = regions joined\n\
         # directly by a road. x, y in sim metres (x east, y south).\n\
         # Source: OpenStreetMap via Overpass API, file {src}.\n\
         # License: (c) OpenStreetMap contributors, ODbL 1.0 (https://www.openstreetmap.org/copyright)\n\n",
        src = osm.file_name().map_or("?".into(), |f| f.to_string_lossy()),
    );
    let lines: Vec<serde_json::Value> = borders
        .iter()
        .map(|((a, b), segs)| serde_json::json!({ "a": a, "b": b, "segments": segs }))
        .collect();
    fs::write(
        map_dir.join("regions.toml"),
        header + &toml::to_string(&RegionsToml { region })?,
    )?;
    fs::write(map_dir.join("regions.bin"), &grid)?;
    fs::write(
        map_dir.join("regions_lines.json"),
        serde_json::to_string(&serde_json::json!({ "borders": lines }))?,
    )?;
    Ok(format!(
        "{n} regions, {} borders, {} road links -> {}",
        borders.len(),
        links.len(),
        map_dir.display()
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::roads::utm_forward;

    #[test]
    fn k063_regions_split_land_by_nearest_place() {
        let dir = std::env::temp_dir().join(format!("k063_regions_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let (e0, n0) = utm_forward(38.0, 34.0, 33.0);
        // 40x20 cells of 50 m; the two easternmost columns are sea.
        fs::write(
            dir.join("map.toml"),
            format!(
                "width = 40\nheight = 20\ncell_size_m = 50\norigin_x = {e0}\norigin_y = {n0}\n\
                 heights = \"sim_height.bin\"\nsea_level_m = 950\n"
            ),
        )
        .unwrap();
        let heights: Vec<u8> = (0..800)
            .flat_map(|k| (if k % 40 >= 38 { 950.0f32 } else { 1000.0 }).to_le_bytes())
            .collect();
        fs::write(dir.join("sim_height.bin"), heights).unwrap();
        // A road along row 10 from the west edge to column 30.
        let mut roads = vec![0u8; 800];
        roads[10 * 40..10 * 40 + 31].fill(2);
        fs::write(dir.join("roads.bin"), roads).unwrap();
        let ll = |x: f64, y: f64| {
            let (mut lat, mut lon) = (38.0, 34.0);
            for _ in 0..5 {
                let (e, n) = utm_forward(lat, lon, 33.0);
                lon += (e0 + x - e) / 87_800.0;
                lat += (n0 - y - n) / 111_000.0;
            }
            format!("\"lat\": {lat}, \"lon\": {lon}")
        };
        // West village and east town on the road; a hamlet too close to the town (dropped),
        // a nameless village, a village in the sea and a non-place node (all ignored).
        let json = format!(
            r#"{{"elements": [
              {{"type": "node", {}, "tags": {{"place": "village", "name": "Batıköy"}}}},
              {{"type": "node", {}, "tags": {{"place": "town", "name": "Doğukent"}}}},
              {{"type": "node", {}, "tags": {{"place": "hamlet", "name": "Yakın"}}}},
              {{"type": "node", {}, "tags": {{"place": "village"}}}},
              {{"type": "node", {}, "tags": {{"place": "village", "name": "Islak"}}}},
              {{"type": "node", {}, "tags": {{"amenity": "cafe", "name": "Kahve"}}}},
              {{"type": "way", "tags": {{"landuse": "forest"}}, "geometry": []}}
            ]}}"#,
            ll(500.0, 500.0),
            ll(1500.0, 500.0),
            ll(1600.0, 500.0),
            ll(1000.0, 400.0),
            ll(1925.0, 500.0),
            ll(700.0, 700.0),
        );
        let osm = dir.join("places.json");
        fs::write(&osm, json).unwrap();
        // The extra point sits in the north-west, so it gets id 1.
        let extras = ["Tepe:500:300".to_string()];
        run(&osm, &dir, 150.0, &extras).unwrap();

        let toml = fs::read_to_string(dir.join("regions.toml")).unwrap();
        let v: toml::Value = toml::from_str(&toml).unwrap();
        let regions = v["region"].as_array().unwrap();
        let field = |k: usize, f: &str| regions[k][f].clone();
        assert_eq!(regions.len(), 3, "{toml}");
        let names: Vec<_> = (0..3).map(|k| field(k, "name_tr")).collect();
        // North to south, then west to east.
        assert_eq!(
            names
                .iter()
                .map(|n| n.as_str().unwrap())
                .collect::<Vec<_>>(),
            ["Tepe", "Batıköy", "Doğukent"]
        );
        assert_eq!(field(0, "kind").as_str(), Some("point"));
        assert_eq!(field(2, "kind").as_str(), Some("town"));
        let ids = |k: usize, f: &str| -> Vec<i64> {
            field(k, f)
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_integer().unwrap())
                .collect()
        };
        assert_eq!(ids(1, "neighbours"), [1, 3]);
        assert_eq!(ids(1, "road_links"), [3], "the road joins village and town");
        assert!(ids(0, "road_links").is_empty());

        let g = fs::read(dir.join("regions.bin")).unwrap();
        let at = |i: usize, j: usize| g[j * 40 + i];
        assert_eq!((at(10, 3), at(10, 12), at(30, 10)), (1, 2, 3));
        // Halfway between village (x = 500) and town (x = 1500): the tie goes to the lower id.
        assert_eq!((at(19, 15), at(20, 15), at(21, 15)), (2, 2, 3));
        assert_eq!((at(38, 10), at(39, 0)), (0, 0), "sea belongs to nobody");
        let total: i64 = (0..3)
            .map(|k| field(k, "cells").as_integer().unwrap())
            .sum();
        assert_eq!(total, 38 * 20);

        let lines: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join("regions_lines.json")).unwrap())
                .unwrap();
        let borders = lines["borders"].as_array().unwrap();
        let b23 = borders.iter().find(|b| b["a"] == 2 && b["b"] == 3).unwrap();
        // One merged straight segment at x = 1025 down to the south edge.
        let last = b23["segments"].as_array().unwrap().last().unwrap().clone();
        assert_eq!(
            (last[0].as_f64(), last[2].as_f64()),
            (Some(1025.0), Some(1025.0))
        );
        assert_eq!(last[3].as_f64(), Some(975.0));
        assert!(parse_extra("no-coordinates").is_err());

        // A quarter that shares its name with a village gets a suffix.
        let json = format!(
            r#"{{"elements": [
              {{"type": "node", {}, "tags": {{"place": "suburb", "name": "Aynı"}}}},
              {{"type": "node", {}, "tags": {{"place": "village", "name": "Aynı"}}}}
            ]}}"#,
            ll(500.0, 500.0),
            ll(1500.0, 500.0),
        );
        fs::write(&osm, json).unwrap();
        run(&osm, &dir, 150.0, &[]).unwrap();
        let toml = fs::read_to_string(dir.join("regions.toml")).unwrap();
        assert!(
            toml.contains("name_tr = \"Aynı Mahallesi\"\nkind = \"suburb\""),
            "{toml}"
        );
        assert!(
            toml.contains("name_tr = \"Aynı\"\nkind = \"village\""),
            "{toml}"
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}
