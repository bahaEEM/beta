// SPDX-License-Identifier: AGPL-3.0-or-later
//! Core world state: units and terrain.

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub struct UnitId(pub u32);

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub struct PlayerId(pub u8);

/// World position in metres. x grows east, y grows south (row direction of the height map).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance(self, other: Vec2) -> f64 {
        (self.x - other.x).hypot(self.y - other.y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum UnitState {
    #[default]
    Healthy,
    /// Mobility halved.
    Damaged,
    Destroyed,
}

/// Rules of engagement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stance {
    /// Fire at will.
    #[default]
    Free,
    /// Fire only for a while after being shot at.
    Return,
    /// Hold fire until a target is close, then strike from hiding (K-142).
    Ambush,
    /// Never fire.
    Hold,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Unit {
    pub id: UnitId,
    pub owner: PlayerId,
    pub type_id: String,
    pub pos: Vec2,
    pub heading_rad: f64,
    pub hp: f64,
    pub state: UnitState,
    /// Radar/jammer switched on (only meaningful for emitter types).
    #[serde(default)]
    pub emitting: bool,
    /// Ground speed during the last tick.
    #[serde(default)]
    pub speed_mps: f64,
    #[serde(default)]
    pub fuel: f64,
    #[serde(default)]
    pub ammo: u32,
    /// Supply stock (supply units only).
    #[serde(default)]
    pub supply_fuel: f64,
    #[serde(default)]
    pub supply_ammo: u32,
    #[serde(default)]
    pub stance: Stance,
    /// Tick this unit was last shot at.
    #[serde(default)]
    pub under_fire_tick: Option<u32>,
    /// Carrier this unit rides in (infantry in an IFV): hidden, protected, cannot act.
    #[serde(default)]
    pub carried_by: Option<UnitId>,
    /// Game seconds of setting up still to go (K-120): 0 = deployed, the type's
    /// `deploy_s` = fully packed for the march.
    #[serde(default)]
    pub packed_s: f64,
    /// Turns in a row that ended without any supply in reach (K-134).
    #[serde(default)]
    pub unsupplied_turns: u32,
    /// Suppression from incoming fire, 0..=1 (K-143).
    #[serde(default)]
    pub suppression: f64,
    /// Tick of this unit's last shot; it gives the position away for a while (K-140).
    #[serde(default)]
    pub last_fired_tick: Option<u32>,
    /// Decoys: the type the enemy takes this unit for (K-144), copied from the unit type.
    #[serde(default)]
    pub decoy_of: Option<String>,
}

impl Unit {
    /// Set up and working (always true for types that need no deployment).
    pub fn deployed(&self) -> bool {
        self.packed_s <= 0.0
    }

    /// Radar/jammer actually on the air: switched on and set up.
    pub fn radiating(&self) -> bool {
        self.emitting && self.deployed()
    }

    /// On the battlefield: alive and not riding inside another unit.
    pub fn active(&self) -> bool {
        self.state != UnitState::Destroyed && self.carried_by.is_none()
    }
}

impl Unit {
    /// A fresh, fully supplied unit of type `t`.
    pub fn new(
        id: UnitId,
        owner: PlayerId,
        t: &crate::data::UnitType,
        pos: Vec2,
        heading_rad: f64,
    ) -> Self {
        Self {
            id,
            owner,
            type_id: t.id.clone(),
            pos,
            heading_rad,
            hp: t.hp,
            fuel: t.fuel_l,
            ammo: t.ammo,
            supply_fuel: t.supply_fuel_l,
            supply_ammo: t.supply_ammo,
            decoy_of: t.decoy_of.clone(),
            ..Self::default()
        }
    }
}

/// Ground cover of a map sample (K-025). The byte values are the `terrain.bin` format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Terrain {
    #[default]
    Open,
    Forest,
    Settlement,
    Water,
}

impl Terrain {
    fn from_byte(b: u8) -> Self {
        match b {
            1 => Terrain::Forest,
            2 => Terrain::Settlement,
            3 => Terrain::Water,
            _ => Terrain::Open,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum WorldError {
    BadMapSize { expected: usize, got: usize },
    BadCellSize,
    DuplicateUnit(UnitId),
}

impl fmt::Display for WorldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorldError::BadMapSize { expected, got } => {
                write!(f, "height map needs {expected} samples, got {got}")
            }
            WorldError::BadCellSize => write!(f, "cell size must be finite and > 0"),
            WorldError::DuplicateUnit(id) => write!(f, "duplicate unit id {}", id.0),
        }
    }
}

impl std::error::Error for WorldError {}

/// Regular grid of heights (metres). Sample (i, j) sits at world (i * cell, j * cell);
/// samples are row-major: `heights[j * width + i]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeightMap {
    width: usize,
    height: usize,
    cell_size_m: f64,
    heights: Vec<f32>,
    /// Road class per sample (0 = none); empty = no road data.
    #[serde(default)]
    roads: Vec<u8>,
    /// Ground cover per sample (`Terrain` bytes); empty = all open land.
    #[serde(default)]
    terrain: Vec<u8>,
    /// Region id per sample (K-063); 0 = none. Empty = no region data.
    #[serde(default)]
    regions: Vec<u8>,
}

/// A 1x1 flat placeholder, only used until the real map is attached after loading a save.
impl Default for HeightMap {
    fn default() -> Self {
        Self {
            width: 1,
            height: 1,
            cell_size_m: 1.0,
            heights: vec![0.0],
            roads: Vec::new(),
            terrain: Vec::new(),
            regions: Vec::new(),
        }
    }
}

impl HeightMap {
    pub fn new(
        width: usize,
        height: usize,
        cell_size_m: f64,
        heights: Vec<f32>,
    ) -> Result<Self, WorldError> {
        if !(cell_size_m.is_finite() && cell_size_m > 0.0) {
            return Err(WorldError::BadCellSize);
        }
        let expected = width * height;
        if expected == 0 || heights.len() != expected {
            return Err(WorldError::BadMapSize {
                expected,
                got: heights.len(),
            });
        }
        Ok(Self {
            width,
            height,
            cell_size_m,
            heights,
            roads: Vec::new(),
            terrain: Vec::new(),
            regions: Vec::new(),
        })
    }

    /// Attaches a road grid (one class byte per sample, same layout as the heights).
    pub fn with_roads(mut self, roads: Vec<u8>) -> Result<Self, WorldError> {
        if roads.len() != self.heights.len() {
            return Err(WorldError::BadMapSize {
                expected: self.heights.len(),
                got: roads.len(),
            });
        }
        self.roads = roads;
        Ok(self)
    }

    /// Attaches a ground cover grid (one `Terrain` byte per sample, same layout as the heights).
    pub fn with_terrain(mut self, terrain: Vec<u8>) -> Result<Self, WorldError> {
        if terrain.len() != self.heights.len() {
            return Err(WorldError::BadMapSize {
                expected: self.heights.len(),
                got: terrain.len(),
            });
        }
        self.terrain = terrain;
        Ok(self)
    }

    /// Attaches a region grid (one region id per sample, same layout as the heights).
    pub fn with_regions(mut self, regions: Vec<u8>) -> Result<Self, WorldError> {
        if regions.len() != self.heights.len() {
            return Err(WorldError::BadMapSize {
                expected: self.heights.len(),
                got: regions.len(),
            });
        }
        self.regions = regions;
        Ok(self)
    }

    /// Region id of the sample nearest to a world position; 0 = none (water, outside
    /// the map or no region data).
    pub fn region_at(&self, pos: Vec2) -> u32 {
        let (gx, gy) = (
            (pos.x / self.cell_size_m).round(),
            (pos.y / self.cell_size_m).round(),
        );
        if gx < 0.0 || gy < 0.0 || gx >= self.width as f64 || gy >= self.height as f64 {
            return 0;
        }
        let k = gy as usize * self.width + gx as usize;
        u32::from(self.regions.get(k).copied().unwrap_or(0))
    }

    /// Ground cover of sample (i, j); open land outside the map or without cover data.
    pub fn terrain(&self, i: usize, j: usize) -> Terrain {
        if i >= self.width || j >= self.height {
            return Terrain::Open;
        }
        Terrain::from_byte(self.terrain.get(j * self.width + i).copied().unwrap_or(0))
    }

    /// Ground cover of the sample nearest to a world position.
    pub fn terrain_at(&self, pos: Vec2) -> Terrain {
        let (gx, gy) = (
            (pos.x / self.cell_size_m).round(),
            (pos.y / self.cell_size_m).round(),
        );
        if gx < 0.0 || gy < 0.0 {
            return Terrain::Open;
        }
        self.terrain(gx as usize, gy as usize)
    }

    /// Coast: a land sample with water among its eight neighbours.
    pub fn is_coast(&self, i: usize, j: usize) -> bool {
        self.terrain(i, j) != Terrain::Water
            && (-1isize..=1).any(|dj| {
                (-1isize..=1).any(
                    |di| match (i.checked_add_signed(di), j.checked_add_signed(dj)) {
                        (Some(ni), Some(nj)) => self.terrain(ni, nj) == Terrain::Water,
                        _ => false,
                    },
                )
            })
    }

    /// Road class of sample (i, j); 0 outside the map or without road data.
    pub fn road(&self, i: usize, j: usize) -> u8 {
        if i >= self.width || j >= self.height {
            return 0;
        }
        self.roads.get(j * self.width + i).copied().unwrap_or(0)
    }

    /// Road class of the sample nearest to a world position.
    pub fn road_at(&self, pos: Vec2) -> u8 {
        let (gx, gy) = (
            (pos.x / self.cell_size_m).round(),
            (pos.y / self.cell_size_m).round(),
        );
        if gx < 0.0 || gy < 0.0 {
            return 0;
        }
        self.road(gx as usize, gy as usize)
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn cell_size_m(&self) -> f64 {
        self.cell_size_m
    }

    /// Height of sample (i, j), clamped to the map edge.
    pub fn sample(&self, i: usize, j: usize) -> f64 {
        let i = i.min(self.width - 1);
        let j = j.min(self.height - 1);
        self.heights.get(j * self.width + i).copied().unwrap_or(0.0) as f64
    }

    /// Bilinear height at a world position; positions outside the map are clamped to the edge.
    pub fn height_at(&self, pos: Vec2) -> f64 {
        let max_x = (self.width - 1) as f64;
        let max_y = (self.height - 1) as f64;
        let gx = (pos.x / self.cell_size_m).clamp(0.0, max_x);
        let gy = (pos.y / self.cell_size_m).clamp(0.0, max_y);
        let (i, j) = (gx.floor() as usize, gy.floor() as usize);
        let (fx, fy) = (gx - i as f64, gy - j as f64);
        let top = self.sample(i, j) * (1.0 - fx) + self.sample(i + 1, j) * fx;
        let bottom = self.sample(i, j + 1) * (1.0 - fx) + self.sample(i + 1, j + 1) * fx;
        top * (1.0 - fy) + bottom * fy
    }
}

/// Whole simulation state. Invariant: `units` is sorted by id with no duplicates.
/// The map is not serialised (saves stay small); whoever loads a save re-attaches it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct World {
    pub tick: u32,
    units: Vec<Unit>,
    #[serde(skip)]
    pub map: HeightMap,
}

impl World {
    pub fn new(map: HeightMap) -> Self {
        Self {
            tick: 0,
            units: Vec::new(),
            map,
        }
    }

    pub fn add_unit(&mut self, unit: Unit) -> Result<(), WorldError> {
        match self.units.binary_search_by_key(&unit.id, |u| u.id) {
            Ok(_) => Err(WorldError::DuplicateUnit(unit.id)),
            Err(idx) => {
                self.units.insert(idx, unit);
                Ok(())
            }
        }
    }

    /// Units in ascending id order.
    pub fn units(&self) -> &[Unit] {
        &self.units
    }

    /// Mutable units in ascending id order. Do not change `id`.
    pub fn units_mut(&mut self) -> &mut [Unit] {
        &mut self.units
    }

    /// Mutable units together with the (read-only) map.
    pub fn units_and_map_mut(&mut self) -> (&mut [Unit], &HeightMap) {
        (&mut self.units, &self.map)
    }

    pub fn unit(&self, id: UnitId) -> Option<&Unit> {
        let idx = self.units.binary_search_by_key(&id, |u| u.id).ok()?;
        self.units.get(idx)
    }

    pub fn unit_mut(&mut self, id: UnitId) -> Option<&mut Unit> {
        let idx = self.units.binary_search_by_key(&id, |u| u.id).ok()?;
        self.units.get_mut(idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map_2x2() -> HeightMap {
        // (0,0)=0 (1,0)=10 / (0,1)=20 (1,1)=30, cell 50 m
        HeightMap::new(2, 2, 50.0, vec![0.0, 10.0, 20.0, 30.0]).unwrap()
    }

    fn unit(id: u32) -> Unit {
        Unit {
            id: UnitId(id),
            owner: PlayerId(0),
            type_id: "infantry".into(),
            pos: Vec2::default(),
            heading_rad: 0.0,
            hp: 100.0,
            state: UnitState::Healthy,
            fuel: 1e6,
            ammo: 1000,
            ..Default::default()
        }
    }

    #[test]
    fn g1_height_at_samples_and_bilinear() {
        let m = map_2x2();
        assert_eq!(m.height_at(Vec2::new(0.0, 0.0)), 0.0);
        assert_eq!(m.height_at(Vec2::new(50.0, 50.0)), 30.0);
        assert_eq!(m.height_at(Vec2::new(25.0, 0.0)), 5.0);
        assert_eq!(m.height_at(Vec2::new(25.0, 25.0)), 15.0);
    }

    #[test]
    fn g1_height_at_clamps_outside_map() {
        let m = map_2x2();
        assert_eq!(m.height_at(Vec2::new(-100.0, -100.0)), 0.0);
        assert_eq!(m.height_at(Vec2::new(1e6, 1e6)), 30.0);
    }

    #[test]
    fn g1_height_map_rejects_bad_input() {
        assert!(HeightMap::new(2, 2, 50.0, vec![0.0; 3]).is_err());
        assert!(HeightMap::new(0, 0, 50.0, vec![]).is_err());
        assert!(HeightMap::new(1, 1, 0.0, vec![0.0]).is_err());
    }

    #[test]
    fn k025_terrain_cover_and_coast() {
        // 3x2: open forest water / settlement open open
        let m = HeightMap::new(3, 2, 50.0, vec![0.0; 6])
            .unwrap()
            .with_terrain(vec![0, 1, 3, 2, 0, 0])
            .unwrap();
        assert_eq!(m.terrain(0, 0), Terrain::Open);
        assert_eq!(m.terrain(1, 0), Terrain::Forest);
        assert_eq!(m.terrain(2, 0), Terrain::Water);
        assert_eq!(m.terrain_at(Vec2::new(10.0, 60.0)), Terrain::Settlement);
        assert_eq!(m.terrain(9, 9), Terrain::Open, "outside the map");
        assert!(m.is_coast(1, 0) && m.is_coast(1, 1) && m.is_coast(2, 1));
        assert!(!m.is_coast(0, 0), "two cells from the water");
        assert!(!m.is_coast(2, 0), "water itself is not coast");
        // No cover data: all open; wrong size rejected.
        let bare = HeightMap::new(3, 2, 50.0, vec![0.0; 6]).unwrap();
        assert_eq!(bare.terrain(2, 0), Terrain::Open);
        assert!(bare.with_terrain(vec![0; 5]).is_err());
    }

    #[test]
    fn g1_units_kept_in_id_order() {
        let mut w = World::new(map_2x2());
        for id in [5, 1, 3, 2] {
            w.add_unit(unit(id)).unwrap();
        }
        let ids: Vec<u32> = w.units().iter().map(|u| u.id.0).collect();
        assert_eq!(ids, [1, 2, 3, 5]);
        assert_eq!(
            w.add_unit(unit(3)),
            Err(WorldError::DuplicateUnit(UnitId(3)))
        );
        assert_eq!(w.unit(UnitId(5)).map(|u| u.id), Some(UnitId(5)));
        assert!(w.unit(UnitId(4)).is_none());
    }
}
