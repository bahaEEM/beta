// SPDX-License-Identifier: AGPL-3.0-or-later
//! Unfiltered simulation events. The server filters them per player before sending.

use crate::sensor::Bearing;
use crate::world::{PlayerId, UnitId, UnitState, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Event {
    UnitMoved {
        unit: UnitId,
        pos: Vec2,
        heading_rad: f64,
    },
    ShotFired {
        shooter: UnitId,
        target: UnitId,
    },
    UnitDamaged {
        unit: UnitId,
        hp: f64,
        state: UnitState,
    },
    UnitDestroyed {
        unit: UnitId,
    },
    TrackGained {
        player: PlayerId,
        unit: UnitId,
        quality: u8,
        /// True position; never shown to the observer when `bearing` is set.
        pos: Vec2,
        /// Set for bearing-only (single ESM line) tracks.
        bearing: Option<Bearing>,
    },
    TrackLost {
        player: PlayerId,
        unit: UnitId,
        last_known: Vec2,
    },
    /// An indirect-fire round left the tube.
    ShellFired {
        shooter: UnitId,
    },
    /// Public: explosions are seen and heard by everyone.
    ShellImpact {
        pos: Vec2,
        radius_m: f64,
    },
    /// Public: zone flags are visible to everyone.
    ZoneCaptured {
        zone: u32,
        player: PlayerId,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TickEvent {
    pub tick: u32,
    pub event: Event,
}
