// SPDX-License-Identifier: AGPL-3.0-or-later
//! K-130…K-134: supply networks, production, carriers, region supply, attrition.

use crate::logistics::{end_of_turn, Zone};
use crate::supply::{zone_at, Networks};
use crate::turn::tests::{catalog, game, rules, unit};
use crate::turn::Game;
use crate::world::{HeightMap, PlayerId, Unit, UnitId, UnitState, Vec2};

fn flat() -> HeightMap {
    HeightMap::new(400, 200, 50.0, vec![0.0; 80_000]).unwrap()
}

/// Zones in a row along y = 1000, 3 km apart: zone k at x = 3000 * k.
fn zone(id: u32, owner: Option<u8>, links: &[u32]) -> Zone {
    Zone {
        id,
        name_tr: format!("Bölge {id}"),
        center: Vec2::new(3000.0 * f64::from(id), 1000.0),
        radius_m: 500.0,
        links: links.to_vec(),
        owner: owner.map(PlayerId),
        base: None,
        progress: None,
    }
}

/// A unit standing in zone `zone` (dx metres east of its centre) with the given stock.
fn at(id: u32, owner: u8, kind: &str, zone: u32, dx: f64, stock: (f64, u32)) -> Unit {
    let mut u = unit(id, owner, kind, 3000.0 * f64::from(zone) + dx, 1000.0);
    (u.supply_fuel, u.supply_ammo) = stock;
    u
}

fn stock(g: &Game, id: u32) -> (f64, u32) {
    let u = g.world.unit(UnitId(id)).unwrap();
    (u.supply_fuel, u.supply_ammo)
}

fn tank(g: &Game, id: u32) -> (f64, u32) {
    let u = g.world.unit(UnitId(id)).unwrap();
    (u.fuel, u.ammo)
}

fn drain(g: &mut Game, id: u32) {
    let u = g.world.unit_mut(UnitId(id)).unwrap();
    u.fuel = 0.0;
    u.ammo = 0;
}

#[test]
fn k130_networks_follow_road_links_through_own_regions() {
    // 1 - 2 - 3 - 4 by road; blue holds 1, 2 and 4, red holds 3 (and cuts 4 off).
    let mut g = game(
        flat(),
        vec![
            at(1, 0, "logistics_base", 1, 0.0, (1000.0, 100)),
            at(2, 0, "depot", 4, 0.0, (0.0, 0)),
            at(3, 1, "depot", 3, 0.0, (500.0, 50)),
        ],
    );
    g.zones = vec![
        zone(1, Some(0), &[2]),
        zone(2, Some(0), &[3]),
        zone(3, Some(1), &[4]),
        zone(4, Some(0), &[]),
        zone(5, None, &[4]),
    ];
    let nets = Networks::new(&g.world, &g.zones, &catalog());
    let supplied: Vec<bool> = (0..5).map(|z| nets.zone_supplied(&g.world, z)).collect();
    // Zone 2 shares the base's stock over the road; zone 4 has an empty depot of its own;
    // neutral zone 5 belongs to no network.
    assert_eq!(supplied, [true, true, true, false, false]);
    assert_eq!(
        zone_at(&g.world.map, &g.zones, Vec2::new(6200.0, 1100.0)),
        Some(1)
    );
    assert_eq!(
        zone_at(&g.world.map, &g.zones, Vec2::new(6000.0, 5000.0)),
        None
    );

    // Taking zone 3 joins everything: the far depot now counts as one pool with the base.
    g.zones[2].owner = Some(PlayerId(0));
    let nets = Networks::new(&g.world, &g.zones, &catalog());
    assert!(nets.zone_supplied(&g.world, 3));
}

#[test]
fn k131_base_produces_and_overflow_fills_linked_storages() {
    let cat = catalog();
    let base = cat.get("logistics_base").unwrap().clone();
    let depot = cat.get("depot").unwrap().clone();
    let mut g = game(
        flat(),
        vec![
            // Base 100 l and 10 rounds short of full; linked depot empty; cut-off depot empty.
            at(
                1,
                0,
                "logistics_base",
                1,
                0.0,
                (base.supply_fuel_l - 100.0, base.supply_ammo - 10),
            ),
            at(2, 0, "depot", 2, 0.0, (0.0, 0)),
            at(3, 0, "depot", 4, 0.0, (0.0, 0)),
        ],
    );
    g.zones = vec![
        zone(1, Some(0), &[2]),
        zone(2, Some(0), &[]),
        zone(3, None, &[2, 4]),
        zone(4, Some(0), &[]),
    ];
    end_of_turn(&mut g, &cat, &rules());
    assert_eq!(stock(&g, 1), (base.supply_fuel_l, base.supply_ammo));
    assert_eq!(
        stock(&g, 2),
        (base.produce_fuel_l - 100.0, base.produce_ammo - 10)
    );
    assert_eq!(stock(&g, 3), (0.0, 0), "no road through own regions");
    assert!(depot.produce_fuel_l == 0.0 && depot.supply_fuel_l > 0.0);

    // A destroyed base makes nothing.
    g.world.unit_mut(UnitId(1)).unwrap().state = UnitState::Destroyed;
    let before = stock(&g, 2);
    end_of_turn(&mut g, &cat, &rules());
    assert_eq!(stock(&g, 2), before);
}

#[test]
fn k132_carriers_load_at_the_network_unload_at_a_cut_off_storage_and_fill_units() {
    let cat = catalog();
    let truck = cat.get("supply").unwrap().clone();
    let mbt = cat.get("mbt").unwrap().clone();
    let mut g = game(
        flat(),
        vec![
            at(1, 0, "logistics_base", 1, 0.0, (20_000.0, 1000)),
            at(2, 0, "supply", 1, 100.0, (0.0, 0)), // empty truck at the base
            at(3, 0, "depot", 3, 0.0, (0.0, 0)),    // cut off: zone 2 is neutral
            at(4, 0, "cargo_plane", 3, -100.0, (2000.0, 100)),
            at(5, 0, "supply", 5, 0.0, (500.0, 10)), // out in neutral land
            at(6, 0, "mbt", 5, 100.0, (0.0, 0)),
            at(7, 0, "mbt", 5, 2000.0, (0.0, 0)), // too far from the truck
            at(8, 1, "mbt", 5, -100.0, (0.0, 0)), // enemy
        ],
    );
    g.zones = vec![
        zone(1, Some(0), &[2]),
        zone(2, None, &[3]),
        zone(3, Some(0), &[]),
        zone(5, None, &[]),
    ];
    for id in [6, 7, 8] {
        drain(&mut g, id);
    }
    let tank_before = g.world.unit(UnitId(5)).unwrap().fuel;
    assert_eq!(tank_before, 1e6, "test trucks have a full tank");
    end_of_turn(&mut g, &cat, &rules());
    // The truck at the base is full; the base made its production first.
    assert_eq!(stock(&g, 2), (truck.supply_fuel_l, truck.supply_ammo));
    let base = cat.get("logistics_base").unwrap();
    assert_eq!(
        stock(&g, 1),
        (
            20_000.0 + base.produce_fuel_l - truck.supply_fuel_l,
            1000 + base.produce_ammo - truck.supply_ammo
        )
    );
    // The plane emptied itself into the cut-off depot.
    assert_eq!(stock(&g, 4), (0.0, 0));
    assert_eq!(stock(&g, 3), (2000.0, 100));
    // The forward truck gave what it had to the tank next to it and nobody else.
    assert_eq!(tank(&g, 6), (500.0, 10));
    assert_eq!(stock(&g, 5), (0.0, 0));
    assert_eq!(tank(&g, 7), (0.0, 0));
    assert_eq!(tank(&g, 8), (0.0, 0));
    assert!(mbt.fuel_l > 500.0);

    // A unit beside a storage with stock is filled up completely.
    let mut g = game(
        flat(),
        vec![
            at(1, 0, "depot", 1, 0.0, (5000.0, 500)),
            at(2, 0, "mbt", 1, 200.0, (0.0, 0)),
        ],
    );
    drain(&mut g, 2);
    end_of_turn(&mut g, &cat, &rules());
    assert_eq!(tank(&g, 2), (mbt.fuel_l, mbt.ammo));
    assert_eq!(
        stock(&g, 1),
        (5000.0 - mbt.fuel_l, 500 - mbt.ammo),
        "taken from the depot"
    );
}

#[test]
fn k133_units_in_a_supplied_region_get_a_share_each_turn() {
    let cat = catalog();
    let mbt = cat.get("mbt").unwrap().clone();
    let share = rules().supply.region_share;
    // Region grid: zone 1 = west half, zone 2 = east half.
    let regions: Vec<u8> = (0..80_000).map(|k| 1 + u8::from(k % 400 >= 200)).collect();
    let mut g = game(
        flat().with_regions(regions).unwrap(),
        vec![
            at(1, 0, "depot", 1, 0.0, (10_000.0, 1000)),
            unit(2, 0, "mbt", 3000.0, 8000.0), // zone 1, far from the depot
            unit(3, 0, "mbt", 15000.0, 8000.0), // zone 2: red
            unit(4, 1, "mbt", 5000.0, 8000.0), // red tank in blue land
        ],
    );
    g.zones = vec![zone(1, Some(0), &[]), zone(2, Some(1), &[])];
    for id in [2, 3, 4] {
        drain(&mut g, id);
    }
    end_of_turn(&mut g, &cat, &rules());
    let expect = (
        mbt.fuel_l * share,
        (f64::from(mbt.ammo) * share).ceil() as u32,
    );
    assert_eq!(tank(&g, 2), expect);
    assert_eq!(tank(&g, 3), (0.0, 0));
    assert_eq!(tank(&g, 4), (0.0, 0));
    assert_eq!(stock(&g, 1), (10_000.0 - expect.0, 1000 - expect.1));
    // A full unit takes nothing.
    let before = stock(&g, 1);
    let full = g.world.unit_mut(UnitId(2)).unwrap();
    (full.fuel, full.ammo) = (mbt.fuel_l, mbt.ammo);
    end_of_turn(&mut g, &cat, &rules());
    assert_eq!(stock(&g, 1), before);
}

#[test]
fn k134_cut_off_ground_units_wither_and_die() {
    let cat = catalog();
    let r = rules();
    let mut g = game(
        flat(),
        vec![
            unit(1, 0, "infantry", 6000.0, 1000.0), // cut off in neutral land
            at(2, 0, "depot", 1, 0.0, (1000.0, 100)),
            unit(3, 0, "infantry", 3100.0, 1000.0), // at the depot
            unit(4, 0, "uav", 9000.0, 5000.0),      // aircraft are exempt
            at(5, 0, "radar_tower", 4, 0.0, (0.0, 0)), // so are structures
            at(6, 0, "supply", 5, 0.0, (0.0, 0)),   // an empty truck feeds nobody
        ],
    );
    g.zones = vec![zone(1, Some(0), &[])];
    let hp = |g: &Game, id: u32| g.world.unit(UnitId(id)).unwrap().hp;
    for _ in 1..r.supply.attrition_after_turns {
        end_of_turn(&mut g, &cat, &r);
    }
    assert_eq!(hp(&g, 1), 100.0, "grace period");
    assert_eq!(
        g.world.unit(UnitId(1)).unwrap().unsupplied_turns,
        r.supply.attrition_after_turns - 1
    );
    end_of_turn(&mut g, &cat, &r);
    let loss = 100.0 * r.supply.attrition_hp_share;
    assert_eq!(hp(&g, 1), 100.0 - loss);
    assert_eq!(hp(&g, 6), 100.0 - loss);
    for id in [3, 4, 5] {
        assert_eq!(hp(&g, id), 100.0, "unit {id}");
    }
    assert_eq!(g.world.unit(UnitId(3)).unwrap().unsupplied_turns, 0);

    // A truck with stock resets the count: the bleeding stops.
    g.world.unit_mut(UnitId(6)).unwrap().supply_ammo = 5;
    g.world.unit_mut(UnitId(6)).unwrap().pos = Vec2::new(6100.0, 1000.0);
    end_of_turn(&mut g, &cat, &r);
    assert_eq!(hp(&g, 1), 100.0 - loss);
    assert_eq!(g.world.unit(UnitId(1)).unwrap().unsupplied_turns, 0);

    // Left alone again it bleeds out completely.
    g.world.unit_mut(UnitId(6)).unwrap().state = UnitState::Destroyed;
    let mut damaged_seen = false;
    for _ in 0..40 {
        end_of_turn(&mut g, &cat, &r);
        damaged_seen |= g.world.unit(UnitId(1)).unwrap().state == UnitState::Damaged;
    }
    assert!(damaged_seen);
    assert_eq!(hp(&g, 1), 0.0);
    assert_eq!(g.world.unit(UnitId(1)).unwrap().state, UnitState::Destroyed);
}
