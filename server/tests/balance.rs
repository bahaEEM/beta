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
                .filter(|u| ["infantry", "mbt", "atgm"].contains(&u.type_id.as_str()))
                .map(|u| u.id)
                .collect();
            let emit: Vec<_> = mine
                .iter()
                .filter(|u| u.type_id == "gsr" || u.type_id == "jammer")
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
            if turn == 1 {
                list.push(Order::SetEmitter {
                    units: emit,
                    on: true,
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
        let mut line = format!("turn {turn:2}: {shots:3} shots |");
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
