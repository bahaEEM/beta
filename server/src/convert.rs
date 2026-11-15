// SPDX-License-Identifier: AGPL-3.0-or-later
//! sim (already filtered) -> protocol types. No game rules here.

use protocol as p;
use sim::indirect::FireTarget;
use sim::logistics::Zone;
use sim::turn::Order;
use sim::visibility::{Contact, Position, SeenEvent, SeenTickEvent};
use sim::world::{Stance, UnitId, UnitState, Vec2};

pub fn stance(s: Stance) -> p::Stance {
    match s {
        Stance::Free => p::Stance::Free,
        Stance::Return => p::Stance::Return,
        Stance::Hold => p::Stance::Hold,
        Stance::Ambush => p::Stance::Ambush,
    }
}

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
        Position::Bearing { origin, angle_rad } => p::Position::Bearing {
            origin: vec2(origin),
            angle_rad,
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
        p::Order::SetEmitter { units: ids, on } => Order::SetEmitter {
            units: units(ids),
            on: *on,
        },
        p::Order::FireMission { units: ids, target } => Order::FireMission {
            units: units(ids),
            target: match *target {
                p::FireTarget::Point { pos } => FireTarget::Point(from_vec2(pos)),
                p::FireTarget::Unit { unit } => FireTarget::Unit(UnitId(unit)),
            },
        },
        p::Order::Embark {
            units: ids,
            carrier,
        } => Order::Embark {
            units: units(ids),
            carrier: UnitId(*carrier),
        },
        p::Order::Disembark { units: ids } => Order::Disembark { units: units(ids) },
        p::Order::Build {
            units: ids,
            type_id,
        } => Order::Build {
            units: units(ids),
            type_id: type_id.clone(),
        },
        p::Order::Buy { type_id, site } => Order::Buy {
            type_id: type_id.clone(),
            site: UnitId(*site),
        },
        p::Order::SetSilence { units: ids, on } => Order::SetSilence {
            units: units(ids),
            on: *on,
        },
        p::Order::SetStance { units: ids, stance } => Order::SetStance {
            units: units(ids),
            stance: match stance {
                p::Stance::Free => Stance::Free,
                p::Stance::Return => Stance::Return,
                p::Stance::Hold => Stance::Hold,
                p::Stance::Ambush => Stance::Ambush,
            },
        },
    }
}

/// `supplied`: Some for the viewer's own regions only (K-133).
pub fn zone(z: &Zone, supplied: Option<bool>) -> p::ZoneView {
    p::ZoneView {
        id: z.id,
        name_tr: z.name_tr.clone(),
        center: vec2(z.center),
        radius_m: z.radius_m,
        supplied,
        owner: z.owner.map(|o| o.0),
        base: z.base.map(|o| o.0),
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
        SeenEvent::ShellFired { shooter } => p::EventKind::ShellFired { shooter: shooter.0 },
        SeenEvent::ShellImpact { pos, radius_m } => p::EventKind::ShellImpact {
            pos: vec2(*pos),
            radius_m: *radius_m,
        },
        SeenEvent::ZoneCaptured { zone, player } => p::EventKind::ZoneCaptured {
            zone: *zone,
            player: player.0,
        },
    };
    p::VisibleEvent { tick: e.tick, kind }
}
