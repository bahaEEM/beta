// SPDX-License-Identifier: AGPL-3.0-or-later
//! Balance report (not a pass/fail test): both fronts race for the western lowland on Hasandag for 15 turns.
//! cargo test --release -p server --test balance -- --ignored --nocapture
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use server::setup::{load_data, load_game, load_map};
use sim::turn::{plan_phase, resolve_phase, Order};
use sim::world::{PlayerId, UnitState, Vec2};
use std::collections::BTreeMap;
use std::path::Path;

#[test]
#[ignore]
fn balance_report_hasandag_advance() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let (catalog, rules) = load_data(&root.join("data")).unwrap();
    let map = load_map(&root.join("maps/hasandag")).unwrap();
    let text = std::fs::read_to_string(root.join("data/scenarios/hasandag.toml")).unwrap();
    let mut g = load_game(&text, map, &catalog, &rules).unwrap();
    for turn in 1..=15 {
        let mut orders = BTreeMap::new();
        for p in [0u8, 1] {
            let mine: Vec<_> = g
                .world
                .units()
                .iter()
                .filter(|u| u.owner == PlayerId(p) && u.state != UnitState::Destroyed)
                .collect();
            let front: Vec<_> = mine
                .iter()
                .filter(|u| {
                    ["infantry", "mbt", "atgm", "ifv", "recon", "supply"]
                        .contains(&u.type_id.as_str())
                })
                .map(|u| u.id)
                .collect();
            let emit: Vec<_> = mine
                .iter()
                .filter(|u| ["gsr", "jammer", "shorad"].contains(&u.type_id.as_str()))
                .map(|u| u.id)
                .collect();
            let mut list = Vec::new();
            if turn <= 8 && !front.is_empty() {
                // Both fronts head for the western lowland between the two flank villages.
                list.push(Order::Move {
                    units: front,
                    waypoints: vec![Vec2::new(7500.0, 19500.0)],
                });
            }
            let ids_of = |t: &str| -> Vec<_> {
                mine.iter()
                    .filter(|u| u.type_id == t)
                    .map(|u| u.id)
                    .collect()
            };
            let enemy_base = if p == 0 {
                Vec2::new(19381.0, 27897.0)
            } else {
                Vec2::new(23160.0, 12713.0)
            };
            if turn == 1 {
                list.push(Order::SetEmitter {
                    units: emit,
                    on: true,
                });
                // Drones: recon UAV over the enemy base, armed UAV over the mountain.
                list.push(Order::Move {
                    units: ids_of("uav"),
                    waypoints: vec![enemy_base],
                });
                list.push(Order::Move {
                    units: ids_of("suav"),
                    waypoints: vec![Vec2::new(19850.0, 20100.0)],
                });
            }
            // Artillery and rockets: best-tracked enemy (quality >= 2), lowest id on ties.
            let target = g
                .tracks
                .tracks
                .values()
                .filter(|t| t.observer_player == PlayerId(p) && t.quality >= 2)
                .max_by_key(|t| (t.quality, std::cmp::Reverse(t.target_unit)))
                .map(|t| t.target_unit);
            let guns: Vec<_> = ids_of("spg").into_iter().chain(ids_of("rocket")).collect();
            if let (Some(t), false) = (target, guns.is_empty()) {
                list.push(Order::FireMission {
                    units: guns,
                    target: sim::indirect::FireTarget::Unit(t),
                });
            }
            orders.insert(PlayerId(p), list);
        }
        plan_phase(&mut g, &catalog, &rules, &orders);
        let events = resolve_phase(&mut g, &catalog, &rules);
        let shots = events
            .iter()
            .filter(|e| matches!(e.event, sim::event::Event::ShotFired { .. }))
            .count();
        let shells = events
            .iter()
            .filter(|e| matches!(e.event, sim::event::Event::ShellFired { .. }))
            .count();
        let mut line = format!("turn {turn:2}: {shots:3} shots {shells:2} shells |");
        for p in [0u8, 1] {
            let mut by_type: BTreeMap<&str, (u32, u32, f64)> = BTreeMap::new();
            for u in g.world.units().iter().filter(|u| u.owner == PlayerId(p)) {
                let e = by_type.entry(u.type_id.as_str()).or_default();
                e.1 += 1;
                if u.state != UnitState::Destroyed {
                    e.0 += 1;
                    e.2 += u.hp;
                }
            }
            let parts: Vec<String> = by_type
                .iter()
                .map(|(t, (alive, all, hp))| {
                    format!(
                        "{t} {alive}/{all} ({:.0}hp)",
                        if *alive > 0 {
                            hp / f64::from(*alive)
                        } else {
                            0.0
                        }
                    )
                })
                .collect();
            line += &format!(" P{p}: {} |", parts.join(", "));
        }
        println!("{line}");
    }
}

/// Hit points left (as a share of the start) of each side after `turns` turns.
fn duel(
    catalog: &sim::data::UnitCatalog,
    rules: &sim::data::Rules,
    attacker: (&str, usize),
    defender: (&str, usize),
    gap_m: f64,
    turns: u32,
    forest: bool,
) -> (f64, f64) {
    use sim::world::{HeightMap, Unit, UnitId, World};
    // With `forest` the defender's half of the field (from 500 m in front of it) is wooded.
    let edge = ((4000.0 + gap_m - 500.0) / 50.0) as usize;
    let cover = (0..80_000)
        .map(|k| u8::from(forest && k % 400 >= edge))
        .collect();
    let map = HeightMap::new(400, 200, 50.0, vec![0.0; 80_000])
        .unwrap()
        .with_terrain(cover)
        .unwrap();
    let mut world = World::new(map);
    let mut id = 0;
    let mut start = [0.0, 0.0];
    for (side, (kind, n)) in [attacker, defender].into_iter().enumerate() {
        let t = catalog.get(kind).unwrap();
        for k in 0..n {
            id += 1;
            let x = 4000.0 + gap_m * side as f64;
            let y = 5000.0 + 60.0 * (k as f64 - (n as f64 - 1.0) / 2.0);
            let heading = if side == 0 { 0.0 } else { std::f64::consts::PI };
            let mut u = Unit::new(
                UnitId(id),
                PlayerId(side as u8),
                t,
                Vec2::new(x, y),
                heading,
            );
            u.emitting = true; // radars on: air defence needs a lock
            world.add_unit(u).unwrap();
            start[side] += t.hp;
        }
    }
    let mut g = sim::turn::Game::new(world, catalog, rules);
    let mut rules = rules.clone();
    rules.kbg.budget_per_turn = 100;
    for turn in 0..turns {
        let mut orders = BTreeMap::new();
        if turn == 0 {
            let units = (1..=attacker.1 as u32).map(UnitId).collect();
            let go = Order::Move {
                units,
                waypoints: vec![Vec2::new(4000.0 + gap_m, 5000.0)],
            };
            orders.insert(PlayerId(0), vec![go]);
        }
        plan_phase(&mut g, catalog, &rules, &orders);
        g.result = None;
        resolve_phase(&mut g, catalog, &rules);
        // Supply is not the subject here: no fuel worries, no attrition. Ammunition is not refilled.
        for u in g.world.units_mut() {
            let t = catalog.get(&u.type_id).unwrap();
            (u.fuel, u.unsupplied_turns) = (t.fuel_l, 0);
        }
    }
    let mut left = [0.0, 0.0];
    for u in g.world.units() {
        left[usize::from(u.owner.0)] += u.hp;
    }
    (left[0] / start[0], left[1] / start[1])
}

/// Balance report: equal money on both sides, the row attacks the column over open
/// ground from 2 km, 16 turns, no resupply. Each cell: attacker's % left / defender's % left.
/// cargo test --release -p server --test balance duels -- --ignored --nocapture
#[test]
#[ignore]
fn balance_report_duels() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let (catalog, rules) = load_data(&root.join("data")).unwrap();
    let kinds = [
        "infantry",
        "mg_team",
        "rocket_inf",
        "atgm",
        "recon",
        "ifv",
        "mbt",
    ];
    let budget = 3000.0;
    let count =
        |k: &str| ((budget / f64::from(catalog.get(k).unwrap().cost)).round() as usize).max(1);
    print!("{:>14}", "att \\ def");
    for d in kinds {
        print!("{:>12}", format!("{d} x{}", count(d)));
    }
    println!();
    let mut net: BTreeMap<&str, f64> = BTreeMap::new();
    for a in kinds {
        print!("{:>14}", format!("{a} x{}", count(a)));
        for d in kinds {
            let (la, ld) = duel(
                &catalog,
                &rules,
                (a, count(a)),
                (d, count(d)),
                2000.0,
                16,
                false,
            );
            print!("{:>12}", format!("{:.0}/{:.0}", la * 100.0, ld * 100.0));
            *net.entry(a).or_default() += la - ld;
            *net.entry(d).or_default() += ld - la;
        }
        println!();
    }
    println!("net score per money (higher = too cheap): {net:.1?}");
    println!("defender in forest, 2 km apart (attacker % / defender %):");
    for (a, d) in [
        ("mbt", "infantry"),
        ("mbt", "rocket_inf"),
        ("mbt", "atgm"),
        ("ifv", "infantry"),
        ("ifv", "rocket_inf"),
        ("ifv", "mg_team"),
        ("infantry", "infantry"),
        ("infantry", "mg_team"),
    ] {
        let (la, ld) = duel(
            &catalog,
            &rules,
            (a, count(a)),
            (d, count(d)),
            2000.0,
            16,
            true,
        );
        println!(
            "  {a} x{} -> {d} x{}: {:.0}/{:.0}",
            count(a),
            count(d),
            la * 100.0,
            ld * 100.0
        );
    }
    println!("air and air defence, 6 km apart (attacker % / defender %):");
    for (a, d) in [
        ("helo", "mbt"),
        ("helo", "infantry"),
        ("helo", "shorad"),
        ("suav", "mbt"),
        ("suav", "shorad"),
        ("mbt", "shorad"),
    ] {
        let (la, ld) = duel(
            &catalog,
            &rules,
            (a, count(a)),
            (d, count(d)),
            6000.0,
            8,
            false,
        );
        println!(
            "  {a} x{} -> {d} x{}: {:.0}/{:.0}",
            count(a),
            count(d),
            la * 100.0,
            ld * 100.0
        );
    }
}
