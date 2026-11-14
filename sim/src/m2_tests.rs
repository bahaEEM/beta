// SPDX-License-Identifier: AGPL-3.0-or-later
//! K-120…K-123: deployable systems, structures, medical units.

use crate::event::Event;
use crate::logistics::{end_of_turn, Zone};
use crate::turn::tests::{catalog, game, rules, unit};
use crate::turn::{plan_phase, resolve_phase, step_tick, Game, Order};
use crate::world::{HeightMap, PlayerId, UnitId, UnitState, Vec2};
use std::collections::BTreeMap;

fn flat() -> HeightMap {
    HeightMap::new(400, 200, 50.0, vec![0.0; 80_000]).unwrap()
}

fn order(g: &mut Game, o: Order) -> Vec<crate::turn::Rejection> {
    let owner = g.world.unit(o.units()[0]).unwrap().owner;
    plan_phase(g, &catalog(), &rules(), &BTreeMap::from([(owner, vec![o])]))
        .remove(&owner)
        .unwrap()
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

fn mv(id: u32, x: f64, y: f64) -> Order {
    Order::Move {
        units: vec![UnitId(id)],
        waypoints: vec![Vec2::new(x, y)],
    }
}

#[test]
fn k120_deployable_packs_before_moving_and_sets_up_after_halting() {
    let mut g = game(
        flat(),
        vec![
            unit(1, 0, "spg", 1000.0, 1000.0),
            unit(2, 0, "mbt", 1000.0, 2000.0),
        ],
    );
    let deploy_s = catalog().get("spg").unwrap().deploy_s;
    let ticks = rules().ticks(deploy_s, 1);
    assert!(deploy_s > 0.0 && g.world.unit(UnitId(1)).unwrap().deployed());
    assert!(order(&mut g, mv(1, 9000.0, 1000.0)).is_empty());
    assert!(order(&mut g, mv(2, 9000.0, 2000.0)).is_empty());
    run(&mut g, 1);
    assert!(
        g.world.unit(UnitId(2)).unwrap().pos.x > 1000.0,
        "tanks just go"
    );
    assert!(!g.world.unit(UnitId(1)).unwrap().deployed(), "packing up");
    run(&mut g, ticks - 3);
    assert_eq!(
        g.world.unit(UnitId(1)).unwrap().pos.x,
        1000.0,
        "still packing"
    );
    run(&mut g, 4);
    let x = g.world.unit(UnitId(1)).unwrap().pos.x;
    assert!(x > 1000.0, "packed: on the move");
    assert_eq!(g.world.unit(UnitId(1)).unwrap().packed_s, deploy_s);

    // Halting starts the set-up; it takes the same time.
    order(
        &mut g,
        Order::Stop {
            units: vec![UnitId(1)],
        },
    );
    run(&mut g, ticks - 2);
    assert!(!g.world.unit(UnitId(1)).unwrap().deployed());
    run(&mut g, 4);
    assert!(g.world.unit(UnitId(1)).unwrap().deployed());
}

#[test]
fn k120_not_deployed_means_no_fire_no_radar_no_esm() {
    // Blue: radar (1), MG team (2), ESM (3), all still setting up. Red: a tank driving
    // inside radar range (4), infantry in front of the MG (5), an emitting air-defence
    // vehicle for the ESM (6).
    let mut g = game(
        flat(),
        vec![
            unit(1, 0, "gsr", 1000.0, 1000.0),
            unit(2, 0, "mg_team", 1000.0, 5000.0),
            unit(3, 0, "esm", 1000.0, 9000.0),
            unit(4, 1, "mbt", 9000.0, 1000.0),
            unit(5, 1, "infantry", 1500.0, 5000.0),
            unit(6, 1, "shorad", 15000.0, 9000.0),
        ],
    );
    for id in [1, 6] {
        g.world.unit_mut(UnitId(id)).unwrap().emitting = true;
    }
    for id in [4, 5] {
        g.world.unit_mut(UnitId(id)).unwrap().stance = crate::world::Stance::Hold;
    }
    for (id, kind) in [(1, "gsr"), (2, "mg_team"), (3, "esm")] {
        g.world.unit_mut(UnitId(id)).unwrap().packed_s = catalog().get(kind).unwrap().deploy_s;
    }
    order(&mut g, mv(4, 9000.0, 9000.0));
    let mg_ticks = rules().ticks(catalog().get("mg_team").unwrap().deploy_s, 1);
    let early = run(&mut g, mg_ticks - 2);
    assert!(
        !early.iter().any(|e| matches!(e, Event::ShotFired { .. })),
        "the machine gun is not set up yet"
    );
    assert_eq!(quality(&g, 0, 4), 0, "radar mast still down");
    assert_eq!(quality(&g, 0, 6), 0, "ESM antennas still down");

    let radar_ticks = rules().ticks(catalog().get("gsr").unwrap().deploy_s, 1);
    let late = run(&mut g, radar_ticks);
    assert!(late.iter().any(|e| matches!(
        e,
        Event::ShotFired {
            shooter: UnitId(2),
            ..
        }
    )));
    assert_eq!(quality(&g, 0, 4), rules().sensor.radar.quality);
    assert_eq!(quality(&g, 0, 6), rules().sensor.esm.bearing_quality);

    // A packed radar is silent: the enemy ESM cannot hear it, even when switched on.
    let mut g = game(
        flat(),
        vec![
            unit(1, 0, "gsr", 1000.0, 1000.0),
            unit(2, 1, "esm", 9000.0, 1000.0),
        ],
    );
    g.world.unit_mut(UnitId(1)).unwrap().emitting = true;
    run(&mut g, 1);
    assert_eq!(quality(&g, 1, 1), rules().sensor.esm.bearing_quality);
    order(&mut g, mv(1, 1000.0, 9000.0));
    run(&mut g, 1);
    assert_eq!(quality(&g, 1, 1), 0);
}

#[test]
fn k120_teams_riding_in_a_carrier_come_out_packed() {
    let mut g = game(
        flat(),
        vec![
            unit(1, 0, "ifv", 1000.0, 1000.0),
            unit(2, 0, "mg_team", 1050.0, 1000.0),
        ],
    );
    assert!(order(
        &mut g,
        Order::Embark {
            units: vec![UnitId(2)],
            carrier: UnitId(1)
        }
    )
    .is_empty());
    run(&mut g, 1);
    let deploy_s = catalog().get("mg_team").unwrap().deploy_s;
    assert_eq!(g.world.unit(UnitId(2)).unwrap().packed_s, deploy_s);
}

#[test]
fn k121_structures_do_not_move() {
    let mut g = game(flat(), vec![unit(1, 0, "radar_tower", 1000.0, 1000.0)]);
    assert_eq!(order(&mut g, mv(1, 2000.0, 1000.0)).len(), 1);
    run(&mut g, 5);
    assert_eq!(
        g.world.unit(UnitId(1)).unwrap().pos,
        Vec2::new(1000.0, 1000.0)
    );
}

fn zone(id: u32, x: f64, owner: u8) -> Zone {
    Zone {
        id,
        name_tr: format!("Bölge {id}"),
        center: Vec2::new(x, 1000.0),
        radius_m: 300.0,
        links: Vec::new(),
        owner: Some(PlayerId(owner)),
        base: None,
        progress: None,
    }
}

#[test]
fn k122_structures_change_hands_with_their_region() {
    // Region 1 = west half, region 2 = east half of a 400x200 map.
    let regions: Vec<u8> = (0..80_000).map(|k| 1 + u8::from(k % 400 >= 200)).collect();
    let map = flat().with_regions(regions).unwrap();
    let mut g = game(
        map,
        vec![
            unit(1, 0, "infantry", 3000.0, 1000.0),
            unit(2, 1, "depot", 8000.0, 6000.0), // far from the centre, same region
            unit(3, 1, "radar_tower", 3100.0, 1000.0), // destroyed: stays a red wreck
            unit(4, 1, "supply", 3000.0, 6000.0), // not a structure
            unit(5, 1, "port", 12000.0, 1000.0), // region 2
            unit(6, 1, "mbt", 19000.0, 9000.0),
        ],
    );
    g.world.unit_mut(UnitId(3)).unwrap().state = UnitState::Destroyed;
    g.world.unit_mut(UnitId(4)).unwrap().stance = crate::world::Stance::Hold;
    g.zones = vec![zone(1, 3000.0, 1), zone(2, 12000.0, 1)];
    resolve_phase(&mut g, &catalog(), &rules());
    assert_eq!(g.zones[0].owner, Some(PlayerId(0)));
    let owner = |id: u32| g.world.unit(UnitId(id)).unwrap().owner.0;
    assert_eq!(owner(2), 0, "the depot comes with the region");
    assert_eq!((owner(3), owner(4), owner(5)), (1, 1, 1));
    assert_eq!(quality(&g, 0, 2), 0, "no enemy track of an own unit");

    // Without a region grid the capture circle decides.
    let mut g = game(
        flat(),
        vec![
            unit(1, 0, "infantry", 3000.0, 1000.0),
            unit(2, 1, "depot", 3200.0, 1000.0),
            unit(3, 1, "depot", 3400.0, 1000.0),
        ],
    );
    g.zones = vec![zone(1, 3000.0, 1)];
    resolve_phase(&mut g, &catalog(), &rules());
    let owner = |id: u32| g.world.unit(UnitId(id)).unwrap().owner.0;
    assert_eq!((owner(2), owner(3)), (0, 1));
}

#[test]
fn k123_medical_units_heal_infantry_at_the_end_of_the_turn() {
    let mut g = game(
        flat(),
        vec![
            unit(1, 0, "ambulance", 1000.0, 1000.0),
            unit(2, 0, "infantry", 1100.0, 1000.0),
            unit(3, 0, "infantry", 1000.0, 3000.0), // out of reach
            unit(4, 0, "mbt", 1050.0, 1000.0),      // not infantry
            unit(5, 1, "infantry", 1000.0, 1100.0), // enemy
            unit(6, 0, "field_hospital", 9000.0, 1000.0),
            unit(7, 0, "atgm", 9100.0, 1000.0),
            unit(8, 0, "infantry", 9000.0, 1100.0), // dead
        ],
    );
    for id in [2, 3, 4, 5, 7, 8] {
        let u = g.world.unit_mut(UnitId(id)).unwrap();
        u.hp = 30.0;
        u.state = UnitState::Damaged;
    }
    {
        let dead = g.world.unit_mut(UnitId(8)).unwrap();
        dead.hp = 0.0;
        dead.state = UnitState::Destroyed;
    }
    // Nobody is in supply here; keep attrition (K-134) out of the picture.
    let mut r = rules();
    r.supply.attrition_after_turns = u32::MAX;
    let heal = catalog().get("ambulance").unwrap().heal_hp_per_turn;
    let hp = |g: &Game, id: u32| {
        let u = g.world.unit(UnitId(id)).unwrap();
        (u.hp, u.state)
    };
    end_of_turn(&mut g, &catalog(), &r);
    assert_eq!(hp(&g, 2), (30.0 + heal, UnitState::Damaged));
    for id in [3, 4, 5] {
        assert_eq!(hp(&g, id), (30.0, UnitState::Damaged), "unit {id}");
    }
    // The hospital heals completely (the team's own maximum).
    let atgm_hp = catalog().get("atgm").unwrap().hp;
    assert_eq!(hp(&g, 7), (atgm_hp, UnitState::Healthy));
    assert_eq!(hp(&g, 8), (0.0, UnitState::Destroyed));
    // A second turn lifts the infantry above the damaged threshold.
    end_of_turn(&mut g, &catalog(), &r);
    end_of_turn(&mut g, &catalog(), &r);
    assert_eq!(hp(&g, 2), (30.0 + 3.0 * heal, UnitState::Healthy));
}
