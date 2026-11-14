// SPDX-License-Identifier: AGPL-3.0-or-later
//! K-150…K-155: radio emissions, radio silence, anti-radiation munitions, counter-battery
//! radar and counter-fire, drones without a data link.

use crate::comms::connected_units;
use crate::event::Event;
use crate::indirect::FireTarget;
use crate::turn::tests::{catalog, game, rules, unit};
use crate::turn::{plan_phase, step_tick, Game, Order, RejectReason, Rejection};
use crate::visibility::{contacts, Position};
use crate::world::{HeightMap, PlayerId, Stance, Unit, UnitId, Vec2};
use std::collections::BTreeMap;

/// 40 x 10 km, flat, no roads.
fn flat() -> HeightMap {
    HeightMap::new(800, 200, 50.0, vec![0.0; 160_000]).unwrap()
}

/// The same with a road along y = 100 m.
fn with_road() -> HeightMap {
    let mut roads = vec![0u8; 160_000];
    roads[2 * 800..3 * 800].fill(2);
    flat().with_roads(roads).unwrap()
}

fn order(g: &mut Game, player: u8, o: Order) -> Vec<Rejection> {
    let p = PlayerId(player);
    plan_phase(g, &catalog(), &rules(), &BTreeMap::from([(p, vec![o])]))
        .remove(&p)
        .unwrap()
}

fn mv(id: u32, x: f64, y: f64) -> Order {
    Order::Move {
        units: vec![UnitId(id)],
        waypoints: vec![Vec2::new(x, y)],
    }
}

fn run(g: &mut Game, ticks: u32) -> Vec<Event> {
    (0..ticks)
        .flat_map(|_| step_tick(g, &catalog(), &rules()))
        .collect()
}

fn quality(g: &Game, player: u8, target: u32) -> u8 {
    g.tracks
        .get(PlayerId(player), UnitId(target))
        .map_or(0, |t| t.quality)
}

fn relink(g: &mut Game) {
    g.connected = connected_units(&g.world, &catalog(), &rules(), &g.hq_players);
}

fn with<F: FnOnce(&mut Unit)>(mut u: Unit, f: F) -> Unit {
    f(&mut u);
    u
}

#[test]
fn k150_units_that_get_orders_transmit_and_esm_hears_them() {
    let r = rules();
    let bearing = r.sensor.esm.bearing_quality;
    // Blue: HQ, a tank that gets an order, a tank that does not. Red: one ESM vehicle.
    let mut g = game(
        flat(),
        vec![
            unit(1, 0, "hq", 1000.0, 5000.0),
            unit(2, 0, "mbt", 5000.0, 5000.0),
            unit(3, 0, "mbt", 5000.0, 8000.0),
            unit(9, 1, "esm", 30000.0, 5000.0),
        ],
    );
    run(&mut g, 1);
    assert_eq!(
        (quality(&g, 1, 1), quality(&g, 1, 2)),
        (0, 0),
        "nobody on the air"
    );
    assert!(order(&mut g, 0, mv(2, 6000.0, 5000.0)).is_empty());
    run(&mut g, 1);
    assert_eq!(quality(&g, 1, 2), bearing, "the tank acknowledges");
    assert_eq!(quality(&g, 1, 1), bearing, "the HQ sent the order");
    assert_eq!(quality(&g, 1, 3), 0, "no order, no transmission");
    let c = contacts(&g.tracks, &g.world, &r, PlayerId(1));
    assert!(c
        .iter()
        .all(|c| matches!(c.position, Position::Bearing { .. })));
    // The transmission is short.
    run(&mut g, r.ticks(r.comms.radio_emit_s, 0));
    assert_eq!((quality(&g, 1, 1), quality(&g, 1, 2)), (0, 0));

    // On the HQ's road wire the order travels by cable: nothing to hear.
    let mut g = game(
        with_road(),
        vec![
            unit(1, 0, "hq", 1000.0, 100.0),
            unit(2, 0, "mbt", 5000.0, 100.0),
            unit(9, 1, "esm", 30000.0, 5000.0),
        ],
    );
    assert!(order(&mut g, 0, mv(2, 6000.0, 100.0)).is_empty());
    run(&mut g, 1);
    assert_eq!((quality(&g, 1, 1), quality(&g, 1, 2)), (0, 0));
}

#[test]
fn k151_silent_units_keep_quiet_take_no_orders_and_relay_nothing() {
    // HQ, a tank 9 km out, infantry 4 km further that only the tank can relay to.
    let mut g = game(
        flat(),
        vec![
            unit(1, 0, "hq", 1000.0, 5000.0),
            unit(2, 0, "mbt", 10000.0, 5000.0),
            unit(3, 0, "infantry", 14000.0, 5000.0),
            unit(9, 1, "esm", 30000.0, 5000.0),
        ],
    );
    let silence = |on| Order::SetSilence {
        units: vec![UnitId(2)],
        on,
    };
    assert!(g.connected[&PlayerId(0)].contains(&UnitId(3)));
    assert!(order(&mut g, 0, silence(true)).is_empty());
    assert!(g.world.unit(UnitId(2)).unwrap().silent);
    run(&mut g, 1);
    assert_eq!(
        (quality(&g, 1, 1), quality(&g, 1, 2)),
        (0, 0),
        "not a sound"
    );
    relink(&mut g);
    assert!(!g.connected[&PlayerId(0)].contains(&UnitId(2)));
    assert!(!g.connected[&PlayerId(0)].contains(&UnitId(3)), "no relay");
    let rejected = order(&mut g, 0, mv(2, 12000.0, 5000.0));
    assert_eq!(rejected[0].reason, RejectReason::NoComms);
    // Breaking the silence is the one order it takes; then it is back on the net.
    assert!(order(&mut g, 0, silence(false)).is_empty());
    relink(&mut g);
    assert!(order(&mut g, 0, mv(2, 12000.0, 5000.0)).is_empty());
    assert!(g.connected[&PlayerId(0)].contains(&UnitId(3)));
}

/// Blue launcher and two ESM vehicles; a red air-defence vehicle radiating while it
/// drives north. Returns the game after the munition was launched at it.
fn harpy_launched() -> Game {
    let mut g = game(
        flat(),
        vec![
            unit(1, 0, "harpy", 1000.0, 5000.0),
            unit(2, 0, "esm", 12000.0, 1000.0),
            unit(3, 0, "esm", 12000.0, 9000.0),
            with(unit(9, 1, "shorad", 16000.0, 2000.0), |u| u.emitting = true),
        ],
    );
    run(&mut g, 1);
    assert_eq!(quality(&g, 0, 9), rules().sensor.esm.triangulated_quality);
    assert!(order(&mut g, 1, mv(9, 16000.0, 9500.0)).is_empty());
    let mission = Order::FireMission {
        units: vec![UnitId(1)],
        target: FireTarget::Unit(UnitId(9)),
    };
    assert!(order(&mut g, 0, mission).is_empty());
    let launched = (0..200).any(|_| {
        step_tick(&mut g, &catalog(), &rules())
            .iter()
            .any(|e| matches!(e, Event::ShellFired { shooter: UnitId(1) }))
    });
    assert!(launched);
    g.missions.clear(); // one munition is enough
    g
}

fn impact(g: &mut Game) -> Vec2 {
    for _ in 0..400 {
        for e in step_tick(g, &catalog(), &rules()) {
            if let Event::ShellImpact { pos, .. } = e {
                return pos;
            }
        }
    }
    panic!("the munition never came down");
}

#[test]
fn k152_loitering_munition_homes_on_a_radiating_emitter() {
    // The radar stays on: the munition follows the moving vehicle and hits it.
    let mut g = harpy_launched();
    let aimed_at = g.shells[0].pos;
    assert_eq!(g.shells[0].homing, Some(UnitId(9)));
    let pos = impact(&mut g);
    let target = g.world.unit(UnitId(9)).unwrap();
    assert_eq!(pos, target.pos);
    assert!(pos.distance(aimed_at) > 500.0, "it drove on meanwhile");
    assert!(target.hp < 80.0);

    // Radar switched off in time: the munition comes down where the radar was heard.
    let mut g = harpy_launched();
    let aimed_at = g.shells[0].pos;
    g.world.unit_mut(UnitId(9)).unwrap().emitting = false;
    assert_eq!(impact(&mut g), aimed_at);
    assert_eq!(g.world.unit(UnitId(9)).unwrap().hp, 100.0);
}

/// Blue howitzer shelling a point; red counter-battery radar, a red howitzer on 'return'
/// and one on 'free'. Returns the game and the events of 500 ticks.
fn artillery_duel(radar_on: bool) -> (Game, Vec<Event>) {
    let mut g = game(
        flat(),
        vec![
            unit(1, 0, "spg", 1000.0, 5000.0),
            with(unit(2, 1, "cb_radar", 20000.0, 5000.0), |u| {
                u.emitting = radar_on
            }),
            with(unit(3, 1, "spg", 25000.0, 5000.0), |u| {
                u.stance = Stance::Return
            }),
            unit(4, 1, "spg", 25000.0, 6000.0),
        ],
    );
    let mission = Order::FireMission {
        units: vec![UnitId(1)],
        target: FireTarget::Point(Vec2::new(12000.0, 5000.0)),
    };
    assert!(order(&mut g, 0, mission).is_empty());
    let events = run(&mut g, 500);
    (g, events)
}

#[test]
fn k153_counter_battery_radar_locates_firing_guns() {
    let (g, events) = artillery_duel(true);
    let fired = |id: u32| {
        events
            .iter()
            .filter(|e| matches!(e, Event::ShellFired { shooter } if *shooter == UnitId(id)))
            .count()
    };
    assert!(fired(1) > 0);
    let fix = g.gun_fixes.iter().find(|f| f.gun == UnitId(1)).unwrap();
    assert_eq!(
        (fix.player, fix.pos),
        (PlayerId(1), Vec2::new(1000.0, 5000.0))
    );
    // Red sees the gun as a last-known position, not as a live track.
    let c = contacts(&g.tracks, &g.world, &rules(), PlayerId(1));
    assert_eq!(c.len(), 1);
    assert_eq!((c[0].unit, c[0].quality), (UnitId(1), 0));
    assert!(matches!(c[0].position, Position::Area { .. }));

    // Radar off: nothing is located, nobody shoots back.
    let (g, events) = artillery_duel(false);
    assert!(g.gun_fixes.is_empty());
    assert!(!events
        .iter()
        .any(|e| matches!(e, Event::ShellFired { shooter } if *shooter != UnitId(1))));
}

#[test]
fn k154_guns_on_return_shoot_back_at_located_guns() {
    let (mut g, events) = artillery_duel(true);
    let shooters: Vec<u32> = events
        .iter()
        .filter_map(|e| match e {
            Event::ShellFired { shooter } => Some(shooter.0),
            _ => None,
        })
        .collect();
    assert!(shooters.contains(&3), "the gun on 'return' answers");
    assert!(!shooters.contains(&4), "the gun on 'free' has no mission");
    // The answer lands on the blue gun, which never moved.
    assert!(g.world.unit(UnitId(1)).unwrap().hp < 100.0);

    // An old fix is forgotten.
    g.missions.clear();
    for f in &mut g.gun_fixes {
        f.tick = 0;
    }
    run(&mut g, 1);
    assert!(g.gun_fixes.is_empty());
}

#[test]
fn k155_drones_without_a_data_link_fly_home() {
    let build = |airfield: bool, jammer_on: bool| {
        let mut units = vec![
            unit(1, 0, "hq", 1000.0, 5000.0),
            unit(3, 0, "uav", 20000.0, 5000.0),
            with(unit(9, 1, "jammer", 20500.0, 5000.0), |u| {
                u.emitting = jammer_on
            }),
        ];
        if airfield {
            units.push(unit(2, 0, "airfield", 3000.0, 2000.0));
        }
        game(flat(), units)
    };
    let no_orders = BTreeMap::new();
    // Jammed: the drone takes no orders and turns for the airfield on its own.
    let mut g = build(true, true);
    assert!(!g.connected[&PlayerId(0)].contains(&UnitId(3)));
    assert_eq!(
        order(&mut g, 0, mv(3, 30000.0, 5000.0))[0].reason,
        RejectReason::NoComms
    );
    assert_eq!(g.routes[&UnitId(3)], vec![Vec2::new(3000.0, 2000.0)]);
    let before = g.world.unit(UnitId(3)).unwrap().pos.x;
    run(&mut g, 10);
    assert!(g.world.unit(UnitId(3)).unwrap().pos.x < before);
    // No airfield: back to the HQ.
    let mut g = build(false, true);
    plan_phase(&mut g, &catalog(), &rules(), &no_orders);
    assert_eq!(g.routes[&UnitId(3)], vec![Vec2::new(1000.0, 5000.0)]);
    // With a link it stays where it was sent.
    let mut g = build(true, false);
    plan_phase(&mut g, &catalog(), &rules(), &no_orders);
    assert!(!g.routes.contains_key(&UnitId(3)));
}
