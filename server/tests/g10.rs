// SPDX-License-Identifier: AGPL-3.0-or-later
//! G10 Phase 0 gate: Hasandag, 6 infantry + 2 MBT per side, fixed orders, 5 turns.
//! Run the timing report with: cargo test --release -p server --test g10 -- --nocapture
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)] // test code; timing is measured outside the simulation

use futures_util::{SinkExt, StreamExt};
use protocol::{self as p, ClientMsg, ServerMsg};
use server::setup::{load_data, load_map, load_scenario};
use server::{serve, MatchSetup};
use sim::event::TickEvent;
use sim::turn::{plan_phase, step_tick, Game, Order};
use sim::visibility::{filter_for_player, SeenEvent};
use sim::world::{PlayerId, UnitId, Vec2};
use sim::TICKS_PER_TURN;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

const TURNS: u32 = 5;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn new_game() -> (Game, sim::data::UnitCatalog, sim::data::Rules) {
    let (catalog, rules) = load_data(&root().join("data")).unwrap();
    let map = load_map(&root().join("maps/hasandag")).unwrap();
    let scenario = std::fs::read_to_string(root().join("data/scenarios/hasandag.toml")).unwrap();
    let world = load_scenario(&scenario, map, &catalog).unwrap();
    assert_eq!(world.units().len(), 16);
    let game = Game::new(world, &catalog, &rules);
    (game, catalog, rules)
}

/// Fixed orders: every unit heads 2 km towards the enemy from where it stands.
fn orders_for(player: u8, own: &[(u32, f64, f64)]) -> Vec<(u32, f64, f64)> {
    let dx = if player == 0 { 2000.0 } else { -2000.0 };
    own.iter().map(|&(id, x, y)| (id, x + dx, y)).collect()
}

#[test]
fn g10_cheat_oracle_and_resolution_time() {
    let (mut g, catalog, rules) = new_game();
    let mut report = Vec::new();
    for turn in 1..=TURNS {
        let mut orders: BTreeMap<PlayerId, Vec<Order>> = BTreeMap::new();
        for player in [0u8, 1] {
            let own: Vec<_> = g
                .world
                .units()
                .iter()
                .filter(|u| {
                    u.owner == PlayerId(player) && u.state != sim::world::UnitState::Destroyed
                })
                .map(|u| (u.id.0, u.pos.x, u.pos.y))
                .collect();
            let list = orders_for(player, &own)
                .into_iter()
                .map(|(id, x, y)| Order::Move {
                    units: vec![UnitId(id)],
                    waypoints: vec![Vec2::new(x, y)],
                })
                .collect();
            orders.insert(PlayerId(player), list);
        }
        let started = Instant::now();
        plan_phase(&mut g, &catalog, &rules, &orders);
        let start_tracks = g.tracks.clone();
        let mut events = Vec::new();
        let mut snapshots = vec![start_tracks.clone()];
        for tick in 1..=TICKS_PER_TURN {
            for event in step_tick(&mut g, &catalog, &rules) {
                events.push(TickEvent { tick, event });
            }
            snapshots.push(g.tracks.clone());
        }
        g.turn += 1;
        let took = started.elapsed();

        for player in [PlayerId(0), PlayerId(1)] {
            let q = |tick: u32, id: UnitId| {
                snapshots[tick as usize]
                    .get(player, id)
                    .map_or(0, |t| t.quality)
            };
            for e in filter_for_player(&events, &start_tracks, &g.world, &rules, player) {
                let ids: Vec<UnitId> = match e.event {
                    SeenEvent::UnitMoved { unit, .. }
                    | SeenEvent::UnitDamaged { unit, .. }
                    | SeenEvent::UnitDestroyed { unit }
                    | SeenEvent::TrackGained { unit, .. }
                    | SeenEvent::TrackLost { unit, .. } => vec![unit],
                    SeenEvent::ShotFired { shooter, target } => {
                        shooter.into_iter().chain(target).collect()
                    }
                };
                for id in ids {
                    let own = g.world.unit(id).is_some_and(|u| u.owner == player);
                    let seen = match e.event {
                        SeenEvent::TrackLost { .. } => q(e.tick - 1, id) > 0,
                        _ => q(e.tick, id) > 0,
                    };
                    assert!(
                        own || seen,
                        "turn {turn}: {player:?} got unseen unit {id:?} at tick {}",
                        e.tick
                    );
                }
            }
        }
        let shots = events
            .iter()
            .filter(|e| matches!(e.event, sim::event::Event::ShotFired { .. }))
            .count();
        let dead = g
            .world
            .units()
            .iter()
            .filter(|u| u.state == sim::world::UnitState::Destroyed)
            .count();
        report.push(format!(
            "turn {turn}: {:.1} ms, {} events, {shots} shots, {dead} destroyed",
            took.as_secs_f64() * 1000.0,
            events.len()
        ));
        assert!(
            took < Duration::from_secs(1) || cfg!(debug_assertions),
            "turn {turn} took {took:?}"
        );
    }
    println!(
        "G10 resolution report ({}):",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
    for line in report {
        println!("  {line}");
    }
}

/// One full match over WebSocket; returns each player's Resolution messages as JSON bytes.
async fn network_match(run: u32) -> [Vec<Vec<u8>>; 2] {
    let (game, catalog, rules) = new_game();
    let setup = MatchSetup {
        match_id: format!("g10_{run}"),
        map_id: "hasandag".into(),
        saves_dir: std::env::temp_dir().join(format!("beta_g10_{}", std::process::id())),
        catalog,
        rules,
        game,
        players: [None, None],
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { serve(listener, setup).await.unwrap() });

    let mut clients = Vec::new();
    for name in ["ayse", "baha"] {
        let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}"))
            .await
            .unwrap();
        let hello = ClientMsg::Hello {
            protocol_version: p::PROTOCOL_VERSION,
            player_name: name.into(),
        };
        ws.send(Message::Binary(p::encode(&hello).unwrap().into()))
            .await
            .unwrap();
        clients.push(ws);
    }
    let mut out: [Vec<Vec<u8>>; 2] = [Vec::new(), Vec::new()];
    let mut known: [BTreeSet<u32>; 2] = [BTreeSet::new(), BTreeSet::new()];
    let mut player_of = [0u8; 2];
    let mut done = [false; 2];
    while !(done[0] && done[1]) {
        for (i, ws) in clients.iter_mut().enumerate() {
            if done[i] {
                continue;
            }
            let frame = tokio::time::timeout(Duration::from_secs(30), ws.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let Message::Binary(bytes) = frame else {
                continue;
            };
            match p::decode::<ServerMsg>(&bytes).unwrap() {
                ServerMsg::Welcome { player_id, .. } => player_of[i] = player_id,
                ServerMsg::TurnStart { turn, view, .. } => {
                    if turn > TURNS {
                        done[i] = true;
                        continue;
                    }
                    known[i].extend(view.own_units.iter().map(|u| u.id));
                    known[i].extend(view.contacts.iter().map(|c| c.unit));
                    let own: Vec<_> = view
                        .own_units
                        .iter()
                        .filter(|u| u.state != p::UnitState::Destroyed)
                        .map(|u| (u.id, u.pos.x, u.pos.y))
                        .collect();
                    let orders = orders_for(player_of[i], &own)
                        .into_iter()
                        .map(|(unit, x, y)| p::Order::Move {
                            units: vec![unit],
                            waypoints: vec![p::Vec2 { x, y }],
                        })
                        .collect();
                    for msg in [ClientMsg::SubmitOrders { turn, orders }, ClientMsg::Ready] {
                        ws.send(Message::Binary(p::encode(&msg).unwrap().into()))
                            .await
                            .unwrap();
                    }
                }
                ServerMsg::Resolution { turn, events } => {
                    for e in &events {
                        let ids: Vec<u32> = match e.kind {
                            p::EventKind::UnitMoved { unit, .. }
                            | p::EventKind::UnitDamaged { unit, .. }
                            | p::EventKind::UnitDestroyed { unit }
                            | p::EventKind::TrackLost { unit, .. } => vec![unit],
                            p::EventKind::TrackGained { unit, .. } => {
                                known[i].insert(unit);
                                vec![unit]
                            }
                            p::EventKind::ShotFired { shooter, target } => {
                                shooter.into_iter().chain(target).collect()
                            }
                        };
                        for id in ids {
                            assert!(
                                known[i].contains(&id),
                                "turn {turn}: client {i} got unknown unit {id}"
                            );
                        }
                    }
                    out[i].push(serde_json::to_vec(&events).unwrap());
                }
                ServerMsg::OrderRejected { reason } => panic!("order rejected: {reason}"),
                ServerMsg::Error { code, message } => panic!("server error {code}: {message}"),
                ServerMsg::PreviewResult { .. } | ServerMsg::OrdersAccepted { .. } => {}
            }
        }
    }
    server.abort();
    out
}

#[tokio::test]
async fn g10_three_network_runs_are_byte_identical() {
    let first = network_match(1).await;
    assert_eq!(first[0].len(), TURNS as usize);
    for run in 2..=3 {
        let again = network_match(run).await;
        assert!(again == first, "run {run} differs from run 1");
    }
}
