// SPDX-License-Identifier: AGPL-3.0-or-later
//! Game rules. Pure, deterministic, no I/O.

pub mod combat;
pub mod data;
pub mod event;
pub mod movement;
pub mod sensor;
pub mod turn;
pub mod visibility;
pub mod world;

#[cfg(test)]
mod ew_tests;

/// Resolution phase length: 20 s = 200 ticks.
pub const TICKS_PER_TURN: u32 = 200;
