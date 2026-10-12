// SPDX-License-Identifier: AGPL-3.0-or-later
//! F3-2 tests: aircraft and air defence (K-100…K-104).

use crate::event::Event;
use crate::indirect::FireTarget;
use crate::movement::{plan_route, MoveOrder};
use crate::turn::tests::{catalog, game, rules, unit};
use crate::turn::{plan_phase, step_tick, Order};
use crate::world::{HeightMap, PlayerId, Stance, UnitId, Vec2};
use std::collections::BTreeMap;

fn flat() -> HeightMap {
    HeightMap::new(800, 400, 50.0, vec![0.0; 320_000]).unwrap()
}

/// A 500 m high wall across x = 10 km.
fn wall() -> HeightMap {
    let (w, h) = (800, 400);
    let heights = (0..w * h)
        .map(|k| if (k % w) == 200 { 500.0 } else { 0.0 })
        .collect();
    HeightMap::new(w, h, 50.0, heights).unwrap()
}

fn quality(g: &crate::turn::Game, player: u8, unit: u32) -> u8 {
    g.tracks
        .get(PlayerId(player), UnitId(unit))
        .map_or(0, |t| t.quality)
}

#[test]
fn k100_aircraft_fly_straight_over_terrain() {
    let g = game(wall(), vec![unit(1, 0, "uav", 5000.0, 5000.0)]);
    let order = MoveOrder {
        unit: UnitId(1),
        waypoints: vec![Vec2::new(15000.0, 5000.0)],
    };
    assert_eq!(
        plan_route(&g.world, &catalog(), &order).unwrap(),
        vec![Vec2::new(15000.0, 5000.0)]
    );
    // A tank cannot climb the wall at all.
    let g = game(wall(), vec![unit(1, 0, "mbt", 5000.0, 5000.0)]);
    let order = MoveOrder {
        unit: UnitId(1),
        waypoints: vec![Vec2::new(15000.0, 5000.0)],
    };
    assert!(plan_route(&g.world, &catalog(), &order).is_err());
}

#[test]
fn k101_only_anti_air_weapons_hit_aircraft() {
    // Enemy UAV 1 km from a tank and an AA vehicle (both see it optically).
    let mut g = game(
        flat(),
        vec![
            unit(1, 0, "mbt", 5000.0, 5000.0),
            unit(2, 0, "shorad", 5100.0, 5000.0),
            unit(9, 1, "uav", 6000.0, 5000.0),
        ],
    );
    let mut shooters = Vec::new();
    for _ in 0..30 {
        for e in step_tick(&mut g, &catalog(), &rules()) {
            if let Event::ShotFired {
                shooter,
                target: UnitId(9),
            } = e
            {
                shooters.push(shooter);
            }
        }
    }
    assert!(!shooters.is_empty());
    assert!(
        shooters.iter().all(|&s| s == UnitId(2)),
        "only the AA vehicle fires at the UAV: {shooters:?}"
    );
}

#[test]
fn k101_anti_air_does_not_engage_ground_targets() {
    let mut g = game(
        flat(),
        vec![
            unit(2, 0, "shorad", 5000.0, 5000.0),
            unit(9, 1, "mbt", 5500.0, 5000.0),
        ],
    );
    g.world.unit_mut(UnitId(9)).unwrap().stance = Stance::Hold;
    for _ in 0..30 {
        let ev = step_tick(&mut g, &catalog(), &rules());
        assert!(!ev.iter().any(|e| matches!(
            e,
            Event::ShotFired {
                shooter: UnitId(2),
                ..
            }
        )));
    }
}

#[test]
fn k102_air_radar_sees_aircraft_ground_radar_does_not() {
    // UAV 15 km away, hovering (speed 0): air radar locks it, ground radar does not.
    let mut g = game(
        flat(),
        vec![
            unit(1, 0, "shorad", 1000.0, 5000.0),
            unit(2, 0, "gsr", 1000.0, 6000.0),
            unit(9, 1, "uav", 16000.0, 5000.0),
        ],
    );
    let on = Order::SetEmitter {
        units: vec![UnitId(1)],
        on: true,
    };
    plan_phase(
        &mut g,
        &catalog(),
        &rules(),
        &BTreeMap::from([(PlayerId(0), vec![on])]),
    );
    step_tick(&mut g, &catalog(), &rules());
    assert_eq!(quality(&g, 0, 9), rules().sensor.radar.quality);
    let mut g2 = game(
        flat(),
        vec![
            unit(2, 0, "gsr", 1000.0, 6000.0),
            unit(9, 1, "uav", 16000.0, 5000.0),
        ],
    );
    let on = Order::SetEmitter {
        units: vec![UnitId(2)],
        on: true,
    };
    plan_phase(
        &mut g2,
        &catalog(),
        &rules(),
        &BTreeMap::from([(PlayerId(0), vec![on])]),
    );
    let go = Order::Move {
        units: vec![UnitId(9)],
        waypoints: vec![Vec2::new(16000.0, 9000.0)],
    };
    plan_phase(
        &mut g2,
        &catalog(),
        &rules(),
        &BTreeMap::from([(PlayerId(1), vec![go])]),
    );
    step_tick(&mut g2, &catalog(), &rules());
    assert_eq!(
        quality(&g2, 0, 9),
        0,
        "ground surveillance radar ignores aircraft even when they move"
    );
}

#[test]
fn k103_shells_do_not_hit_aircraft() {
    let mut g = game(
        flat(),
        vec![
            unit(1, 0, "spg", 1000.0, 5000.0),
            unit(9, 1, "uav", 13000.0, 5000.0),
        ],
    );
    let start_hp = g.world.unit(UnitId(9)).unwrap().hp;
    let fire = Order::FireMission {
        units: vec![UnitId(1)],
        target: FireTarget::Point(Vec2::new(13000.0, 5000.0)),
    };
    plan_phase(
        &mut g,
        &catalog(),
        &rules(),
        &BTreeMap::from([(PlayerId(0), vec![fire])]),
    );
    let mut impacts = 0;
    for _ in 0..200 {
        impacts += step_tick(&mut g, &catalog(), &rules())
            .iter()
            .filter(|e| matches!(e, Event::ShellImpact { .. }))
            .count();
    }
    assert!(impacts > 0);
    assert_eq!(
        g.world.unit(UnitId(9)).unwrap().hp,
        start_hp,
        "the UAV overhead is untouched"
    );
}

#[test]
fn k104_altitude_sees_over_ridges() {
    // Enemy infantry behind the wall: a ground recon vehicle cannot see it, a UAV can.
    let g = game(
        wall(),
        vec![
            unit(1, 0, "recon", 8000.0, 5000.0),
            unit(9, 1, "infantry", 11000.0, 5000.0),
        ],
    );
    assert_eq!(quality(&g, 0, 9), 0);
    let g = game(
        wall(),
        vec![
            unit(1, 0, "uav", 8000.0, 5000.0),
            unit(9, 1, "infantry", 11000.0, 5000.0),
        ],
    );
    assert_eq!(quality(&g, 0, 9), 3);
}
