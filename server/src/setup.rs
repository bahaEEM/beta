// SPDX-License-Identifier: AGPL-3.0-or-later
//! Loading maps, scenarios and save files.

use crate::{BoxError, PLAYERS};
use serde::{Deserialize, Serialize};
use sim::data::{Rules, UnitCatalog};
use sim::logistics::Zone;
use sim::turn::Game;
use sim::world::{HeightMap, PlayerId, Terrain, Unit, UnitId, Vec2, World};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveFile {
    pub match_id: String,
    pub map_id: String,
    pub players: [Option<String>; PLAYERS],
    pub game: Game,
}

/// `<saves>/<match_id>/turn_<n>.json.zst`, n = the turn about to be planned.
pub fn save_game(saves_dir: &Path, save: &SaveFile) -> Result<PathBuf, BoxError> {
    let dir = saves_dir.join(&save.match_id);
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("turn_{}.json.zst", save.game.turn));
    let json = serde_json::to_vec(save)?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, zstd::encode_all(json.as_slice(), 3)?)?;
    fs::rename(&tmp, &path)?; // never leave a half-written save behind
    Ok(path)
}

/// Loads the highest-numbered turn of a match.
pub fn load_latest(saves_dir: &Path, match_id: &str) -> Result<SaveFile, BoxError> {
    let dir = saves_dir.join(match_id);
    let latest = fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let n: u32 = name
                .strip_prefix("turn_")?
                .strip_suffix(".json.zst")?
                .parse()
                .ok()?;
            Some((n, e.path()))
        })
        .max_by_key(|(n, _)| *n)
        .ok_or_else(|| format!("no saves in {}", dir.display()))?;
    let bytes = zstd::decode_all(fs::read(&latest.1)?.as_slice())?;
    Ok(serde_json::from_slice(&bytes)?)
}

#[derive(Deserialize)]
struct MapToml {
    width: usize,
    height: usize,
    cell_size_m: f64,
    heights: String,
}

/// Reads `map.toml` + the f32 LE height grid written by `tools convert`.
pub fn load_map(map_dir: &Path) -> Result<HeightMap, BoxError> {
    let meta: MapToml = toml::from_str(&fs::read_to_string(map_dir.join("map.toml"))?)?;
    let raw = fs::read(map_dir.join(&meta.heights))?;
    let heights = raw
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect();
    let mut map = HeightMap::new(meta.width, meta.height, meta.cell_size_m, heights)?;
    // Optional grids written by `tools roads` / `tools cover` (same layout, one byte per sample).
    let roads = map_dir.join("roads.bin");
    if roads.exists() {
        map = map.with_roads(fs::read(roads)?)?;
    }
    let terrain = map_dir.join("terrain.bin");
    if terrain.exists() {
        map = map.with_terrain(fs::read(terrain)?)?;
    }
    let regions = map_dir.join("regions.bin");
    if regions.exists() {
        map = map.with_regions(fs::read(regions)?)?;
    }
    Ok(map)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScenarioUnit {
    id: u32,
    owner: u8,
    #[serde(rename = "type")]
    type_id: String,
    x: f64,
    y: f64,
    heading_deg: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Scenario {
    unit: Vec<ScenarioUnit>,
    #[serde(default)]
    zone: Vec<ScenarioZone>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScenarioZone {
    id: u32,
    name_tr: String,
    x: f64,
    y: f64,
    radius_m: f64,
    /// Regions joined to this one by road (`road_links` of regions.toml).
    #[serde(default)]
    road_links: Vec<u32>,
    owner: Option<u8>,
    /// Home base of this player (K-061).
    base: Option<u8>,
}

/// New game from `data/scenarios/<map_id>.toml`: starting units and zones (map metres).
pub fn load_game(
    text: &str,
    map: HeightMap,
    catalog: &UnitCatalog,
    rules: &Rules,
) -> Result<Game, BoxError> {
    let scenario: Scenario = toml::from_str(text)?;
    let mut world = World::new(map);
    for u in scenario.unit {
        if usize::from(u.owner) >= PLAYERS {
            return Err(format!("unit {}: owner must be < {PLAYERS}", u.id).into());
        }
        let t = catalog
            .get(&u.type_id)
            .ok_or_else(|| format!("unit {}: unknown type '{}'", u.id, u.type_id))?;
        let pos = Vec2::new(u.x, u.y);
        if !t.class.is_air() && world.map.terrain_at(pos) == Terrain::Water {
            return Err(format!("unit {}: ground unit placed on water", u.id).into());
        }
        world.add_unit(Unit::new(
            UnitId(u.id),
            PlayerId(u.owner),
            t,
            pos,
            u.heading_deg.to_radians(),
        ))?;
    }
    let mut game = Game::new(world, catalog, rules);
    for z in scenario.zone {
        if [z.owner, z.base]
            .iter()
            .flatten()
            .any(|&o| usize::from(o) >= PLAYERS)
            || !(z.radius_m.is_finite() && z.radius_m > 0.0)
        {
            return Err(format!("zone {}: bad owner, base or radius", z.id).into());
        }
        game.zones.push(Zone {
            id: z.id,
            name_tr: z.name_tr,
            center: Vec2::new(z.x, z.y),
            radius_m: z.radius_m,
            links: z.road_links,
            owner: z.owner.map(PlayerId),
            base: z.base.map(PlayerId),
            progress: None,
        });
    }
    Ok(game)
}

/// Loads catalog and rules from the data directory.
pub fn load_data(data_dir: &Path) -> Result<(UnitCatalog, Rules), BoxError> {
    let catalog = sim::data::load_units(&fs::read_to_string(data_dir.join("units.toml"))?)?;
    let rules = sim::data::load_rules(&fs::read_to_string(data_dir.join("rules.toml"))?)?;
    Ok((catalog, rules))
}
