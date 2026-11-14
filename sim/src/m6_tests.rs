// SPDX-License-Identifier: AGPL-3.0-or-later
//! K-160…K-165: ships, submarines, sonar, naval weapons, landings, supply by sea.

use crate::event::Event;
use crate::logistics::{end_of_turn, Zone};
use crate::movement::{plan_route, MoveError, MoveOrder};
use crate::turn::tests::{catalog, game, rules, unit};
use crate::turn::{plan_phase, step_tick, Game, Order, RejectReason, Rejection};
use crate::visibility::{contacts, Position};
use crate::world::{HeightMap, PlayerId, Stance, Terrain, Unit, UnitId, UnitState, Vec2};
use std::collections::BTreeMap;

/// 20 x 10 km. Land west of x = 5 km; sea east of it, with a land bar at x = 10..10.5 km
/// that leaves a passage south of y = 7.5 km.
fn sea() -> HeightMap {
    let terrain = (0..80_000)
        .map(|k| {
            let (i, j) = (k % 400, k / 400);
            let land = i < 100 || ((200..=210).contains(&i) && j < 150);
            if land {
                0
            } else {
                3
            }
        })
        .collect();
    HeightMap::new(400, 200, 50.0, vec![0.0; 80_000])
        .unwrap()
        .with_terrain(terrain)
        .unwrap()
}

fn with<F: FnOnce(&mut Unit)>(mut u: Unit, f: F) -> Unit {
    f(&mut u);
    u
}

fn held(u: Unit) -> Unit {
    with(u, |u| u.stance = Stance::Hold)
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

fn shots_by(events: &[Event], id: u32) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, Event::ShotFired { shooter, .. } if *shooter == UnitId(id)))
        .count()
}

#[test]
fn k160_ships_sail_on_water_only() {
    let mut g = game(
        sea(),
        vec![
            unit(1, 0, "patrol_boat", 7000.0, 1000.0),
            unit(2, 0, "mbt", 4000.0, 1000.0),
        ],
    );
    let route = |g: &Game, id: u32, x: f64| {
        plan_route(
            &g.world,
            &catalog(),
            &MoveOrder {
                unit: UnitId(id),
                waypoints: vec![Vec2::new(x, 1000.0)],
            },
        )
    };
    // Round the bar through the southern passage, never touching land.
    let r = route(&g, 1, 13000.0).unwrap();
    assert!(r.iter().any(|p| p.y >= 7500.0));
    assert!(r
        .iter()
        .all(|&p| g.world.map.terrain_at(p) == Terrain::Water));
    assert_eq!(
        route(&g, 1, 2000.0),
        Err(MoveError::Unreachable { index: 0 })
    );
    assert_eq!(
        route(&g, 2, 7000.0),
        Err(MoveError::Unreachable { index: 0 })
    );
    assert!(order(&mut g, 0, mv(1, 9000.0, 1000.0)).is_empty());
    run(&mut g, 10);
    let boat = catalog().get("patrol_boat").unwrap().clone();
    let x = g.world.unit(UnitId(1)).unwrap().pos.x;
    let dt = rules().turn.game_seconds_per_tick;
    assert!((x - (7000.0 + 10.0 * dt * boat.speed_offroad_mps)).abs() < 1e-6);
}

#[test]
fn k161_submarines_are_found_only_by_active_sonar() {
    let r = rules();
    let build = |pinging: bool, d: f64| {
        game(
            sea(),
            vec![
                with(unit(1, 0, "patrol_boat", 12000.0, 9000.0), |u| {
                    u.emitting = pinging
                }),
                held(unit(9, 1, "submarine", 12000.0 + d, 9000.0)),
            ],
        )
    };
    // A kilometre away, eyes and radar see nothing.
    assert_eq!(quality(&build(false, 1000.0), 0, 9), 0);
    let g = build(true, 1000.0);
    assert_eq!(quality(&g, 0, 9), r.sensor.sonar.active_quality);
    let c = contacts(&g.tracks, &g.world, &r, PlayerId(0));
    assert_eq!(c[0].type_id.as_deref(), Some("submarine"));
    // Beyond the sonar's range it is gone again.
    assert_eq!(quality(&build(true, 6000.0), 0, 9), 0);
    // The submarine sees the boat through its periscope all the while.
    assert_eq!(quality(&g, 1, 1), r.sensor.optical.fire_quality);
}

#[test]
fn k162_passive_sonar_hears_moving_hulls_and_pings() {
    let r = rules();
    let bearing = r.sensor.esm.bearing_quality;
    // Blue submarine listening; a red patrol boat 13 km east of it (beyond the periscope).
    let mut g = game(
        sea(),
        vec![
            unit(1, 0, "submarine", 6000.0, 9000.0),
            held(unit(9, 1, "patrol_boat", 19000.0, 9000.0)),
        ],
    );
    run(&mut g, 1);
    assert_eq!(quality(&g, 0, 9), 0, "a boat lying still is silent");
    assert!(order(&mut g, 1, mv(9, 19000.0, 8000.0)).is_empty());
    run(&mut g, 1);
    assert_eq!(quality(&g, 0, 9), bearing, "screws turning");
    let c = contacts(&g.tracks, &g.world, &r, PlayerId(0));
    assert!(matches!(c[0].position, Position::Bearing { .. }));

    // A red submarine creeping along is not heard; when it pings, it is. ESM on the
    // shore hears neither.
    let mut g = game(
        sea(),
        vec![
            unit(1, 0, "submarine", 6000.0, 9000.0),
            unit(2, 0, "esm", 4000.0, 9000.0),
            held(unit(9, 1, "submarine", 15000.0, 9000.0)),
        ],
    );
    assert!(order(&mut g, 1, mv(9, 15000.0, 8000.0)).is_empty());
    run(&mut g, 1);
    assert!(g.world.unit(UnitId(9)).unwrap().speed_mps > 0.0);
    assert_eq!(quality(&g, 0, 9), 0, "silent running");
    g.world.unit_mut(UnitId(9)).unwrap().emitting = true;
    run(&mut g, 1);
    assert_eq!(quality(&g, 0, 9), bearing, "the ping gives it away");
    let c = contacts(&g.tracks, &g.world, &r, PlayerId(0));
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].position, {
        let b = g
            .tracks
            .get(PlayerId(0), UnitId(9))
            .unwrap()
            .bearing
            .unwrap();
        Position::Bearing {
            origin: b.origin,
            angle_rad: b.angle_rad,
        }
    });
    assert_eq!(c[0].position.clone(), c[0].position);
    assert_eq!(
        g.tracks
            .get(PlayerId(0), UnitId(9))
            .unwrap()
            .bearing
            .unwrap()
            .origin,
        Vec2::new(6000.0, 9000.0),
        "heard by the submarine, not by the ESM vehicle"
    );
}

#[test]
fn k163_naval_weapons_engage_their_own_kind_of_target() {
    // Coastal battery on a radar fix of a moving corvette, far beyond sight.
    let mut g = game(
        sea(),
        vec![
            with(unit(1, 0, "coastal_battery", 4000.0, 9000.0), |u| {
                u.emitting = true
            }),
            held(unit(9, 1, "corvette", 18000.0, 9500.0)),
        ],
    );
    assert!(order(&mut g, 1, mv(9, 18000.0, 8000.0)).is_empty());
    let events = run(&mut g, 60);
    assert!(shots_by(&events, 1) > 0);
    assert!(g.world.unit(UnitId(9)).unwrap().hp < 100.0);

    // A tank on the shore cannot touch a boat it sees perfectly well.
    let mut g = game(
        sea(),
        vec![
            unit(1, 0, "mbt", 4900.0, 5000.0),
            held(unit(9, 1, "patrol_boat", 5600.0, 5000.0)),
        ],
    );
    assert_eq!(quality(&g, 0, 9), rules().sensor.optical.fire_quality);
    assert_eq!(shots_by(&run(&mut g, 60), 1), 0);

    // Torpedoes: a submarine sinks what it sees through the periscope.
    let mut g = game(
        sea(),
        vec![
            unit(1, 0, "submarine", 12000.0, 9000.0),
            held(unit(9, 1, "corvette", 17000.0, 9000.0)),
        ],
    );
    assert!(shots_by(&run(&mut g, 60), 1) > 0);
    assert!(g.world.unit(UnitId(9)).unwrap().hp < 100.0);

    // Depth charges: a pinging patrol boat attacks the submarine it has found; one that
    // does not ping never finds it.
    let hunt = |pinging: bool| {
        let mut g = game(
            sea(),
            vec![
                with(unit(1, 0, "patrol_boat", 12000.0, 9000.0), |u| {
                    u.emitting = pinging
                }),
                held(unit(9, 1, "submarine", 14000.0, 9000.0)),
            ],
        );
        shots_by(&run(&mut g, 60), 1)
    };
    assert!(hunt(true) > 0);
    assert_eq!(hunt(false), 0);
}

#[test]
fn k164_landing_ships_carry_units_from_shore_to_shore() {
    let r = rules();
    let mut g = game(
        sea(),
        vec![
            unit(1, 0, "landing_ship", 5200.0, 5000.0),
            unit(2, 0, "infantry", 4950.0, 5000.0),
            unit(3, 0, "mbt", 4950.0, 5050.0),
            unit(4, 0, "ifv", 4900.0, 5100.0),
        ],
    );
    let embark = |units: &[u32], carrier: u32| Order::Embark {
        units: units.iter().map(|&u| UnitId(u)).collect(),
        carrier: UnitId(carrier),
    };
    let disembark = Order::Disembark {
        units: vec![UnitId(2), UnitId(3)],
    };
    // An IFV does not take a tank; the ship takes both from the beach.
    assert_eq!(
        order(&mut g, 0, embark(&[3], 4))[0].reason,
        RejectReason::CannotEmbark
    );
    assert!(order(&mut g, 0, embark(&[2, 3], 1)).is_empty());
    assert!(order(&mut g, 0, mv(1, 7000.0, 9000.0)).is_empty());
    run(&mut g, 100);
    let ship = g.world.unit(UnitId(1)).unwrap().pos;
    assert_eq!(g.world.unit(UnitId(3)).unwrap().pos, ship, "riding along");
    // No shore in reach out here.
    assert_eq!(
        order(&mut g, 0, disembark.clone())[0].reason,
        RejectReason::CannotEmbark
    );
    // Back at the coast, further south: both go ashore, shaken up.
    assert!(order(&mut g, 0, mv(1, 5100.0, 9000.0)).is_empty());
    run(&mut g, 900);
    assert_eq!(
        g.world.unit(UnitId(1)).unwrap().pos,
        Vec2::new(5100.0, 9000.0)
    );
    assert!(order(&mut g, 0, disembark).is_empty());
    for id in [2, 3] {
        let u = g.world.unit(UnitId(id)).unwrap();
        assert_eq!(u.carried_by, None);
        assert_eq!(u.pos, Vec2::new(4950.0, 9000.0), "nearest land");
        assert_eq!(u.suppression, r.transport.landing_suppression);
    }

    // A ship that goes down takes everyone aboard with it.
    assert!(order(&mut g, 0, embark(&[2, 3], 1)).is_empty());
    let ship = g.world.unit_mut(UnitId(1)).unwrap();
    (ship.hp, ship.state) = (0.0, UnitState::Destroyed);
    run(&mut g, 1);
    for id in [2, 3] {
        assert_eq!(
            g.world.unit(UnitId(id)).unwrap().state,
            UnitState::Destroyed
        );
    }
}

#[test]
fn k165_ports_and_supply_ships_carry_the_supply_network_over_the_sea() {
    let (cat, r) = (catalog(), rules());
    let ship_t = cat.get("supply_ship").unwrap().clone();
    let boat_t = cat.get("patrol_boat").unwrap().clone();
    let stock = |id: u32, x: f64, y: f64, kind: &str, fuel: f64, ammo: u32| {
        with(unit(id, 0, kind, x, y), |u| {
            (u.supply_fuel, u.supply_ammo) = (fuel, ammo)
        })
    };
    let mut g = game(
        sea(),
        vec![
            stock(1, 4000.0, 5000.0, "logistics_base", 50_000.0, 3000),
            stock(2, 4950.0, 5000.0, "port", 10_000.0, 500),
            stock(3, 5100.0, 5000.0, "supply_ship", 0.0, 0),
            with(unit(4, 0, "patrol_boat", 5150.0, 5100.0), |u| {
                (u.fuel, u.ammo) = (15_000.0, 0) // the port alone can make that good
            }),
            // An island port with nothing, and a boat far out at sea.
            stock(5, 10250.0, 2000.0, "port", 0.0, 0),
            unit(6, 0, "patrol_boat", 18000.0, 2000.0),
        ],
    );
    g.zones = vec![Zone {
        id: 1,
        name_tr: "Liman".into(),
        center: Vec2::new(4000.0, 5000.0),
        radius_m: 1500.0,
        links: Vec::new(),
        owner: Some(PlayerId(0)),
        base: None,
        income: 0,
        progress: None,
    }];
    end_of_turn(&mut g, &cat, &r);
    let unit_of = |g: &Game, id: u32| g.world.unit(UnitId(id)).unwrap().clone();
    // At the home port the ship loads and the boat is refuelled and rearmed.
    let ship = unit_of(&g, 3);
    assert_eq!(
        (ship.supply_fuel, ship.supply_ammo),
        (ship_t.supply_fuel_l, ship_t.supply_ammo)
    );
    let boat = unit_of(&g, 4);
    assert_eq!((boat.fuel, boat.ammo), (boat_t.fuel_l, boat_t.ammo));
    // At the island port, which no base feeds, it unloads.
    g.world.unit_mut(UnitId(3)).unwrap().pos = Vec2::new(10550.0, 2000.0);
    end_of_turn(&mut g, &cat, &r);
    let (port, ship) = (unit_of(&g, 5), unit_of(&g, 3));
    let port_t = cat.get("port").unwrap();
    assert_eq!(port.supply_fuel, port_t.supply_fuel_l, "filled to the brim");
    assert_eq!(
        ship.supply_fuel,
        ship_t.supply_fuel_l - port_t.supply_fuel_l
    );
    assert_eq!(port.supply_ammo + ship.supply_ammo, ship_t.supply_ammo);
    // Ships do not wither at sea.
    for _ in 0..5 {
        end_of_turn(&mut g, &cat, &r);
    }
    assert_eq!(unit_of(&g, 6).hp, 100.0);
}
