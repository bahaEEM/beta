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
                    ["infantry", "mbt", "atgm", "ifv", "recon"].contains(&u.type_id.as_str())
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
