// SPDX-License-Identifier: AGPL-3.0-or-later
//! F1-5 tests: radar, ESM bearings/triangulation, jamming, emitter orders.

use crate::event::TickEvent;
use crate::sensor::{update_tracks, TrackTable};
use crate::turn::tests::{catalog, game, rules, unit};
use crate::turn::{plan_phase, step_tick, validate_orders, Order, RejectReason};
use crate::visibility::{filter_for_player, Position, SeenEvent};
use crate::world::{HeightMap, PlayerId, UnitId, Vec2};
use std::collections::BTreeMap;

fn flat() -> HeightMap {
    HeightMap::new(500, 500, 50.0, vec![0.0; 250_000]).unwrap()
}

fn quality(g: &crate::turn::Game, player: u8, unit: u32) -> u8 {
    g.tracks
        .get(PlayerId(player), UnitId(unit))
        .map_or(0, |t| t.quality)
}

fn emit(g: &mut crate::turn::Game, ids: &[u32]) {
    let order = Order::SetEmitter {
        units: ids.iter().map(|&i| UnitId(i)).collect(),
        on: true,
    };
    let owner = g.world.unit(UnitId(ids[0])).unwrap().owner;
    let r = plan_phase(
        g,
        &catalog(),
        &rules(),
        &BTreeMap::from([(owner, vec![order])]),
    );
    assert!(r[&owner].is_empty());
}

fn go(g: &mut crate::turn::Game, id: u32, to: Vec2) {
    let owner = g.world.unit(UnitId(id)).unwrap().owner;
    let order = Order::Move {
        units: vec![UnitId(id)],
        waypoints: vec![to],
    };
    plan_phase(
        g,
        &catalog(),
        &rules(),
        &BTreeMap::from([(owner, vec![order])]),
    );
}

#[test]
fn f1_5_radar_sees_moving_targets_only_when_on() {
    // Radar at x=1000, enemy tank 10 km east: far beyond optics.
    let mut g = game(
        flat(),
        vec![
            unit(1, 0, "gsr", 1000.0, 5000.0),
            unit(2, 1, "mbt", 11000.0, 5000.0),
        ],
    );
    go(&mut g, 2, Vec2::new(11000.0, 9000.0));
    step_tick(&mut g, &catalog(), &rules());
    assert_eq!(quality(&g, 0, 2), 0, "radar is off");
    emit(&mut g, &[1]);
    step_tick(&mut g, &catalog(), &rules());
    assert_eq!(
        quality(&g, 0, 2),
        rules().sensor.radar.quality,
        "moving target seen"
    );
    // Stop the tank: a stationary target is invisible to the radar.
    plan_phase(
        &mut g,
        &catalog(),
        &rules(),
        &BTreeMap::from([(
            PlayerId(1),
            vec![Order::Stop {
                units: vec![UnitId(2)],
            }],
        )]),
    );
    step_tick(&mut g, &catalog(), &rules());
    step_tick(&mut g, &catalog(), &rules());
    assert_eq!(quality(&g, 0, 2), 0);
}

#[test]
fn f1_5_enemy_jammer_degrades_radar() {
    let mut g = game(
        flat(),
        vec![
            unit(1, 0, "gsr", 1000.0, 5000.0),
            unit(2, 1, "mbt", 11000.0, 5000.0),
            unit(3, 1, "jammer", 12000.0, 9000.0),
        ],
    );
    emit(&mut g, &[1]);
    go(&mut g, 2, Vec2::new(11000.0, 9000.0));
    step_tick(&mut g, &catalog(), &rules());
    let clear = quality(&g, 0, 2);
    emit(&mut g, &[3]);
    step_tick(&mut g, &catalog(), &rules());
    let r = rules().sensor.radar;
    assert_eq!(clear, r.quality);
    assert_eq!(quality(&g, 0, 2), r.quality - r.jam_quality_drop);
}

#[test]
fn f1_5_one_bearing_reveals_direction_not_position() {
    // ESM 30 km from an emitting enemy radar (an air-defence vehicle: radiates on the move).
    let mut g = game(
        flat(),
        vec![
            unit(1, 0, "esm", 1000.0, 1000.0),
            unit(2, 1, "shorad", 20000.0, 20000.0),
        ],
    );
    emit(&mut g, &[2]);
    go(&mut g, 2, Vec2::new(20000.0, 23000.0));
    let start = g.tracks.clone();
    let mut events = Vec::new();
    for tick in 1..=20 {
        for event in step_tick(&mut g, &catalog(), &rules()) {
            events.push(TickEvent { tick, event });
        }
    }
    assert_eq!(quality(&g, 0, 2), rules().sensor.esm.bearing_quality);
    let seen = filter_for_player(&events, &start, &g.world, &rules(), PlayerId(0));
    assert!(seen.iter().any(|e| matches!(
        &e.event,
        SeenEvent::TrackGained {
            position: Position::Bearing { .. },
            ..
        }
    )));
    for e in &seen {
        match &e.event {
            SeenEvent::UnitMoved { unit, .. } => {
                assert_ne!(*unit, UnitId(2), "bearing-only unit moved event leaked")
            }
            SeenEvent::TrackGained { position, .. } => {
                assert!(matches!(position, Position::Bearing { .. }))
            }
            _ => {}
        }
    }
    let c = crate::visibility::contacts(&g.tracks, &g.world, &rules(), PlayerId(0));
    assert!(matches!(c[0].position, Position::Bearing { .. }));
}

#[test]
fn f1_5_two_crossing_bearings_triangulate() {
    let mut g = game(
        flat(),
        vec![
            unit(1, 0, "esm", 1000.0, 1000.0),
            unit(2, 0, "esm", 1000.0, 20000.0),
            unit(3, 1, "gsr", 20000.0, 10000.0),
        ],
    );
    emit(&mut g, &[3]);
    step_tick(&mut g, &catalog(), &rules());
    assert_eq!(quality(&g, 0, 3), rules().sensor.esm.triangulated_quality);
    // Radar switched off: ESM hears nothing.
    let off = Order::SetEmitter {
        units: vec![UnitId(3)],
        on: false,
    };
    plan_phase(
        &mut g,
        &catalog(),
        &rules(),
        &BTreeMap::from([(PlayerId(1), vec![off])]),
    );
    step_tick(&mut g, &catalog(), &rules());
    assert_eq!(quality(&g, 0, 3), 0);
}

#[test]
fn f1_5_set_emitter_rejects_non_emitters() {
    let g = game(flat(), vec![unit(1, 0, "mbt", 1000.0, 1000.0)]);
    let order = Order::SetEmitter {
        units: vec![UnitId(1)],
        on: true,
    };
    let v = validate_orders(&g, &catalog(), &rules(), PlayerId(0), &[order]);
    assert_eq!(v.rejected[0].reason, RejectReason::NotEmitter);
}

#[test]
fn f1_5_destroyed_units_neither_emit_nor_listen() {
    let mut g = game(
        flat(),
        vec![
            unit(1, 0, "esm", 1000.0, 1000.0),
            unit(2, 1, "gsr", 20000.0, 20000.0),
        ],
    );
    emit(&mut g, &[2]);
    g.world.unit_mut(UnitId(2)).unwrap().state = crate::world::UnitState::Destroyed;
    let mut t = TrackTable::default();
    update_tracks(&g.world, &catalog(), &rules(), &mut t);
    assert!(t.get(PlayerId(0), UnitId(2)).is_none());
}
