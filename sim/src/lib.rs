// SPDX-License-Identifier: AGPL-3.0-or-later
//! Game rules. Pure, deterministic, no I/O.

pub mod data;
pub mod movement;
pub mod sensor;
pub mod world;

/// Fixed simulation step: 10 ticks per second.
pub const TICK_SECONDS: f64 = 0.1;
/// Resolution phase length: 20 s = 200 ticks.
pub const TICKS_PER_TURN: u32 = 200;
