// SPDX-License-Identifier: AGPL-3.0-or-later
//! Core world state: units and terrain.

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct UnitId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Unit {
    pub id: UnitId,
    pub owner: PlayerId,
    pub type_id: String,
    pub pos: Vec2,
    pub heading_rad: f64,
    pub hp: f64,
    pub state: UnitState,
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
        })
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct World {
    pub tick: u32,
    units: Vec<Unit>,
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
