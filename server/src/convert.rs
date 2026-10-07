// SPDX-License-Identifier: AGPL-3.0-or-later
//! sim (already filtered) -> protocol types. No game rules here.

use protocol as p;
use sim::turn::Order;
use sim::visibility::{Contact, Position, SeenEvent, SeenTickEvent};
use sim::world::{UnitId, UnitState, Vec2};

pub fn vec2(v: Vec2) -> p::Vec2 {
    p::Vec2 { x: v.x, y: v.y }
}

pub fn from_vec2(v: p::Vec2) -> Vec2 {
    Vec2::new(v.x, v.y)
}

pub fn position(pos: &Position) -> p::Position {
    match *pos {
        Position::Exact(v) => p::Position::Exact { pos: vec2(v) },
        Position::Area { center, radius_m } => p::Position::Area {
            center: vec2(center),
            radius_m,
        },
    }
}

pub fn state(s: UnitState) -> p::UnitState {
    match s {
        UnitState::Healthy => p::UnitState::Healthy,
        UnitState::Damaged => p::UnitState::Damaged,
        UnitState::Destroyed => p::UnitState::Destroyed,
    }
}

pub fn units(ids: &[p::UnitId]) -> Vec<UnitId> {
    ids.iter().copied().map(UnitId).collect()
}

pub fn order(o: &p::Order) -> Order {
    match o {
        p::Order::Move {
            units: ids,
            waypoints,
        } => Order::Move {
            units: units(ids),
            waypoints: waypoints.iter().copied().map(from_vec2).collect(),
        },
        p::Order::Stop { units: ids } => Order::Stop { units: units(ids) },
    }
}

pub fn contact(c: &Contact) -> p::Contact {
    p::Contact {
        unit: c.unit.0,
        quality: c.quality,
        type_id: c.type_id.clone(),
        position: position(&c.position),
        last_seen_tick: c.last_seen_tick,
    }
}

pub fn event(e: &SeenTickEvent) -> p::VisibleEvent {
    let kind = match &e.event {
        SeenEvent::UnitMoved {
            unit,
            position: pos,
            heading_rad,
        } => p::EventKind::UnitMoved {
            unit: unit.0,
            position: position(pos),
            heading_rad: *heading_rad,
        },
        SeenEvent::ShotFired { shooter, target } => p::EventKind::ShotFired {
            shooter: shooter.map(|u| u.0),
            target: target.map(|u| u.0),
        },
        SeenEvent::UnitDamaged { unit, hp, state: s } => p::EventKind::UnitDamaged {
            unit: unit.0,
            hp: *hp,
            state: state(*s),
        },
        SeenEvent::UnitDestroyed { unit } => p::EventKind::UnitDestroyed { unit: unit.0 },
        SeenEvent::TrackGained {
            unit,
            quality,
            position: pos,
        } => p::EventKind::TrackGained {
            unit: unit.0,
            quality: *quality,
            position: position(pos),
        },
        SeenEvent::TrackLost { unit, last_known } => p::EventKind::TrackLost {
            unit: unit.0,
            last_known: position(last_known),
        },
    };
    p::VisibleEvent { tick: e.tick, kind }
}
