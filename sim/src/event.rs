// SPDX-License-Identifier: AGPL-3.0-or-later
//! Unfiltered simulation events. The server filters them per player before sending.

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
        pos: Vec2,
    },
    TrackLost {
        player: PlayerId,
        unit: UnitId,
        last_known: Vec2,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TickEvent {
    pub tick: u32,
    pub event: Event,
}
