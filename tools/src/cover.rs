// SPDX-License-Identifier: AGPL-3.0-or-later

//! `tools cover --osm <overpass.json> --map-dir <dir>`
//!
//! Writes `terrain.bin`: one byte per sim cell (same layout as `sim_height.bin`),
//! 0 = open, 1 = forest, 2 = settlement, 3 = water. Water is every sample at or below
//! `sea_level_m` of `map.toml`; forest and settlement are OSM polygons (ways and
//! multipolygon relations, Overpass `out geom`) filled with the even-odd rule at cell
//! centres. Settlement wins over forest, water over both.

use crate::roads::utm_forward;
use crate::Res;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub const FOREST: u8 = 1;
pub const SETTLEMENT: u8 = 2;
pub const WATER: u8 = 3;

#[derive(Deserialize)]
struct MapToml {
    width: usize,
    height: usize,
    cell_size_m: f64,
    origin_x: f64,
    origin_y: f64,
    heights: Option<String>,
    sea_level_m: Option<f32>,
}

/// Grid geometry of `map.toml` plus the water mask (empty when the map has no sea).
pub struct SimMap {
    pub w: usize,
    pub h: usize,
    pub cell: f64,
    pub origin_x: f64,
    pub origin_y: f64,
    water: Vec<bool>,
}

impl SimMap {
    pub fn is_water(&self, i: usize, j: usize) -> bool {
        self.water.get(j * self.w + i).copied().unwrap_or(false)
    }

    /// Water at the sample nearest to a point in sim metres (inside the map).
    pub fn is_water_at(&self, p: [f64; 2]) -> bool {
        let i = ((p[0] / self.cell).round() as usize).min(self.w - 1);
        let j = ((p[1] / self.cell).round() as usize).min(self.h - 1);
        self.is_water(i, j)
    }

    /// Cuts a polyline at its points on water; pieces shorter than two points are dropped.
    pub fn split_on_water(&self, pts: Vec<[f64; 2]>) -> Vec<Vec<[f64; 2]>> {
        if self.water.is_empty() {
            return vec![pts];
        }
        pts.split(|&p| self.is_water_at(p))
            .filter(|piece| piece.len() >= 2)
            .map(<[_]>::to_vec)
            .collect()
    }

    /// WGS84 -> sim metres (x east, y south from sample (0, 0)); UTM zone 36N.
    pub fn project(&self, g: &LatLon) -> [f64; 2] {
        let (e, n) = utm_forward(g.lat, g.lon, 33.0);
        [e - self.origin_x, self.origin_y - n]
    }
}

pub fn load_sim_map(map_dir: &Path) -> Res<SimMap> {
    let m: MapToml = toml::from_str(&fs::read_to_string(map_dir.join("map.toml"))?)?;
    if m.width == 0 || m.height == 0 || !(m.cell_size_m.is_finite() && m.cell_size_m > 0.0) {
        return Err("map.toml: width, height and cell_size_m must be > 0".into());
    }
    let water = match (m.sea_level_m, &m.heights) {
        (Some(sea), Some(file)) => {
            let raw = fs::read(map_dir.join(file))?;
            if raw.len() != m.width * m.height * 4 {
                return Err(format!("{file}: size does not match map.toml").into());
            }
            raw.as_chunks::<4>()
                .0
                .iter()
                .map(|c| f32::from_le_bytes(*c) <= sea)
                .collect()
        }
        _ => Vec::new(),
    };
    Ok(SimMap {
        w: m.width,
        h: m.height,
        cell: m.cell_size_m,
        origin_x: m.origin_x,
        origin_y: m.origin_y,
        water,
    })
}

#[derive(Deserialize)]
pub struct LatLon {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Deserialize)]
struct Overpass {
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
    tags: BTreeMap<String, String>,
    /// Ways: the outline.
    #[serde(default)]
    geometry: Vec<LatLon>,
    /// Relations: outline pieces (outer and inner rings, possibly split over several ways).
    #[serde(default)]
    members: Vec<Member>,
}

#[derive(Deserialize)]
struct Member {
    #[serde(default)]
    geometry: Vec<LatLon>,
}

fn cover_class(tags: &BTreeMap<String, String>) -> u8 {
    let tag = |k: &str| tags.get(k).map(String::as_str);
    match (tag("landuse"), tag("natural")) {
        (Some("residential" | "industrial" | "commercial"), _) => SETTLEMENT,
        (Some("forest"), _) | (_, Some("wood")) => FOREST,
        _ => 0,
    }
}

type Edge = ([f64; 2], [f64; 2]);

/// Raises every cell whose centre is inside the edge soup (even-odd) to at least `class`.
fn fill(grid: &mut [u8], map: &SimMap, edges: &[Edge], class: u8) {
    let ys = edges.iter().flat_map(|(a, b)| [a[1], b[1]]);
    let (lo, hi) = ys.fold((f64::MAX, f64::MIN), |(lo, hi), y| (lo.min(y), hi.max(y)));
    if lo > hi {
        return;
    }
    let j0 = (lo / map.cell).ceil().max(0.0) as usize;
    let j1 = (hi / map.cell).floor();
    if j1 < 0.0 {
        return;
    }
    for j in j0..=(j1 as usize).min(map.h - 1) {
        let y = j as f64 * map.cell;
        let mut xs: Vec<f64> = edges
            .iter()
            .filter(|(a, b)| (a[1] <= y) != (b[1] <= y))
            .map(|(a, b)| a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]))
            .collect();
        xs.sort_by(f64::total_cmp);
        for span in xs.as_chunks::<2>().0 {
            let i0 = (span[0] / map.cell).ceil().max(0.0) as usize;
            let i1 = (span[1] / map.cell).floor();
            if i1 < 0.0 {
                continue;
            }
            for i in i0..=(i1 as usize).min(map.w - 1) {
                let c = &mut grid[j * map.w + i];
                *c = (*c).max(class);
            }
        }
    }
}

pub fn run(osm: &Path, map_dir: &Path) -> Res<String> {
    let map = load_sim_map(map_dir)?;
    let data: Overpass = serde_json::from_str(&fs::read_to_string(osm)?)?;
    let mut grid = vec![0u8; map.w * map.h];
    for el in &data.elements {
        let class = cover_class(&el.tags);
        if class == 0 {
            continue;
        }
        let ring = |g: &[LatLon]| -> Vec<[f64; 2]> { g.iter().map(|p| map.project(p)).collect() };
        let mut edges: Vec<Edge> = Vec::new();
        // A lone way is closed implicitly; relation members are ring pieces and are not.
        let way = ring(&el.geometry);
        if let (Some(&first), Some(&last)) = (way.first(), way.last()) {
            edges.extend(way.windows(2).map(|s| (s[0], s[1])));
            edges.push((last, first));
        }
        for m in &el.members {
            edges.extend(ring(&m.geometry).windows(2).map(|s| (s[0], s[1])));
        }
        fill(&mut grid, &map, &edges, class);
    }
    for (k, c) in grid.iter_mut().enumerate() {
        if map.is_water(k % map.w, k / map.w) {
            *c = WATER;
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
    let toml = format!(
        "# Generated by `tools cover`. terrain.bin: u8 per sim cell, row-major, {w}x{h} like\n\
         # sim_height.bin. 0 = open, 1 = forest (landuse=forest, natural=wood), 2 = settlement\n\
         # (landuse=residential/industrial/commercial), 3 = water (height <= sea_level_m).\n\
         width = {w}\n\
         height = {h}\n\
         cells = \"terrain.bin\"\n\
         open_cells = {}\n\
         forest_cells = {}\n\
         settlement_cells = {}\n\
         water_cells = {}\n\
         source = \"OpenStreetMap via Overpass API, file {src}\"\n\
         osm_timestamp = \"{date}\"\n\
         license = \"(c) OpenStreetMap contributors, ODbL 1.0 (https://www.openstreetmap.org/copyright)\"\n",
        cells[0],
        cells[1],
        cells[2],
        cells[3],
        w = map.w,
        h = map.h,
        src = osm.file_name().map_or("?".into(), |f| f.to_string_lossy()),
    );
    fs::write(map_dir.join("terrain.bin"), &grid)?;
    fs::write(map_dir.join("terrain.toml"), toml)?;
    Ok(format!(
        "cells open/forest/settlement/water: {}/{}/{}/{} -> {}",
        cells[0],
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

    fn flat_map(n: usize) -> SimMap {
        SimMap {
            w: n,
            h: n,
            cell: 50.0,
            origin_x: 0.0,
            origin_y: 0.0,
            water: Vec::new(),
        }
    }

    fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Edge> {
        let p = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
        (0..4).map(|k| (p[k], p[(k + 1) % 4])).collect()
    }

    #[test]
    fn k025_fill_is_even_odd_and_keeps_the_higher_class() {
        let map = flat_map(10);
        let mut grid = vec![0u8; 100];
        // Forest over cells 1..=6 with a hole over 3..=4; partly outside the map to the west.
        let mut edges = square(-500.0, 40.0, 310.0, 310.0);
        edges.extend(square(140.0, 140.0, 210.0, 210.0));
        fill(&mut grid, &map, &edges, FOREST);
        let at = |g: &[u8], i: usize, j: usize| g[j * 10 + i];
        assert_eq!(at(&grid, 0, 1), FOREST);
        assert_eq!(at(&grid, 6, 6), FOREST);
        assert_eq!(at(&grid, 3, 3), 0, "hole");
        assert_eq!(at(&grid, 4, 4), 0, "hole");
        assert_eq!(at(&grid, 7, 3), 0);
        assert_eq!(at(&grid, 3, 0), 0);
        assert_eq!(grid.iter().filter(|&&c| c == FOREST).count(), 7 * 6 - 4);
        // A village inside the forest wins; forest painted later does not undo it.
        fill(
            &mut grid,
            &map,
            &square(40.0, 40.0, 110.0, 110.0),
            SETTLEMENT,
        );
        fill(&mut grid, &map, &square(-10.0, -10.0, 460.0, 460.0), FOREST);
        assert_eq!(at(&grid, 1, 1), SETTLEMENT);
        assert_eq!(at(&grid, 3, 3), FOREST);
    }

    #[test]
    fn k025_cover_run_marks_water_and_osm_polygons() {
        let dir = std::env::temp_dir().join(format!("k025_cover_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let (e0, n0) = utm_forward(38.0, 34.0, 33.0);
        fs::write(
            dir.join("map.toml"),
            format!(
                "width = 10\nheight = 10\ncell_size_m = 50\norigin_x = {e0}\norigin_y = {n0}\n\
                 heights = \"sim_height.bin\"\nsea_level_m = 950\n"
            ),
        )
        .unwrap();
        // Columns 8 and 9 are sea.
        let heights: Vec<u8> = (0..100)
            .flat_map(|k| (if k % 10 >= 8 { 950.0f32 } else { 1000.0 }).to_le_bytes())
            .collect();
        fs::write(dir.join("sim_height.bin"), heights).unwrap();
        let ll = |x: f64, y: f64| {
            let (mut lat, mut lon) = (38.0, 34.0);
            for _ in 0..5 {
                let (e, n) = utm_forward(lat, lon, 33.0);
                lon += (e0 + x - e) / 87_800.0;
                lat += (n0 - y - n) / 111_000.0;
            }
            format!("{{\"lat\": {lat}, \"lon\": {lon}}}")
        };
        // A wood over rows 1..=2 reaching into the sea; a village on cells (1..=2, 5..=6)
        // given as a relation whose outline is split over two open ways.
        let json = format!(
            r#"{{"elements": [
              {{"type": "way", "tags": {{"natural": "wood"}}, "geometry": [{}, {}, {}, {}, {}]}},
              {{"type": "relation", "tags": {{"landuse": "residential"}}, "members": [
                {{"type": "way", "role": "outer", "geometry": [{}, {}, {}]}},
                {{"type": "way", "role": "outer", "geometry": [{}, {}, {}]}}]}},
              {{"type": "way", "tags": {{"landuse": "farmland"}}, "geometry": [{}, {}, {}]}}
            ]}}"#,
            ll(40.0, 40.0),
            ll(460.0, 40.0),
            ll(460.0, 110.0),
            ll(40.0, 110.0),
            ll(40.0, 40.0),
            ll(40.0, 240.0),
            ll(110.0, 240.0),
            ll(110.0, 310.0),
            ll(110.0, 310.0),
            ll(40.0, 310.0),
            ll(40.0, 240.0),
            ll(0.0, 400.0),
            ll(450.0, 400.0),
            ll(450.0, 450.0),
        );
        let osm = dir.join("cover.json");
        fs::write(&osm, json).unwrap();
        run(&osm, &dir).unwrap();

        let g = fs::read(dir.join("terrain.bin")).unwrap();
        let at = |i: usize, j: usize| g[j * 10 + i];
        assert_eq!(
            (at(1, 1), at(7, 2), at(0, 1), at(1, 3)),
            (FOREST, FOREST, 0, 0)
        );
        assert_eq!((at(8, 1), at(9, 9)), (WATER, WATER));
        assert_eq!((at(1, 5), at(2, 6), at(3, 5)), (SETTLEMENT, SETTLEMENT, 0));
        assert_eq!(at(4, 8), 0, "farmland is open ground");
        let toml = fs::read_to_string(dir.join("terrain.toml")).unwrap();
        assert!(toml.contains("water_cells = 20"), "{toml}");
        assert!(toml.contains("forest_cells = 14"), "{toml}");
        assert!(toml.contains("settlement_cells = 4"), "{toml}");

        let map = load_sim_map(&dir).unwrap();
        let cut = map.split_on_water(vec![[0.0, 0.0], [300.0, 0.0], [400.0, 0.0], [450.0, 0.0]]);
        assert_eq!(cut, vec![vec![[0.0, 0.0], [300.0, 0.0]]]);
        fs::remove_dir_all(&dir).unwrap();
    }
}
