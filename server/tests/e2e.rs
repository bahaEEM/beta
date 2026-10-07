// SPDX-License-Identifier: AGPL-3.0-or-later
//! G7 end-to-end tests with Rust WebSocket clients.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use futures_util::{SinkExt, StreamExt};
use protocol::{self as p, ClientMsg, ServerMsg};
use server::setup::{load_latest, load_scenario};
use server::{serve, MatchSetup};
use sim::data::{load_rules, load_units};
use sim::turn::Game;
use sim::world::HeightMap;
use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

const SCENARIO: &str = r#"
[[unit]]
id = 1
owner = 0
type = "mbt"
x = 1000.0
y = 1000.0
heading_deg = 0.0

[[unit]]
id = 2
owner = 0
type = "infantry"
x = 1000.0
y = 1200.0
heading_deg = 0.0

[[unit]]
id = 3
owner = 1
type = "mbt"
x = 3500.0
y = 1000.0
heading_deg = 180.0

[[unit]]
id = 4
owner = 1
type = "infantry"
x = 4500.0
y = 4500.0
heading_deg = 180.0
"#;

fn saves_dir(test: &str) -> PathBuf {
    std::env::temp_dir().join(format!("beta_e2e_{test}_{}", std::process::id()))
}

fn setup(test: &str, planning_secs: u32) -> MatchSetup {
    let catalog = load_units(include_str!("../../data/units.toml")).unwrap();
    let mut rules = load_rules(include_str!("../../data/rules.toml")).unwrap();
    rules.turn.planning_secs = planning_secs;
    let map = HeightMap::new(120, 120, 50.0, vec![0.0; 14_400]).unwrap();
    let world = load_scenario(SCENARIO, map, &catalog).unwrap();
    let game = Game::new(world, &catalog, &rules);
    MatchSetup {
        match_id: "test".into(),
        map_id: "flat".into(),
        saves_dir: saves_dir(test),
        catalog,
        rules,
        game,
        players: [None, None],
    }
}

async fn start(setup: MatchSetup) -> (SocketAddr, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        serve(listener, setup).await.unwrap();
    });
    (addr, handle)
}

struct Client(WebSocketStream<MaybeTlsStream<TcpStream>>);

impl Client {
    async fn connect(addr: SocketAddr, name: &str, version: u32) -> Self {
        let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}"))
            .await
            .unwrap();
        let mut c = Client(ws);
        c.send(&ClientMsg::Hello {
            protocol_version: version,
            player_name: name.into(),
        })
        .await;
        c
    }

    async fn send(&mut self, msg: &ClientMsg) {
        self.0
            .send(Message::Binary(p::encode(msg).unwrap().into()))
            .await
            .unwrap();
    }

    async fn recv(&mut self) -> ServerMsg {
        loop {
            let frame = tokio::time::timeout(Duration::from_secs(10), self.0.next())
                .await
                .expect("timed out waiting for server")
                .expect("connection closed")
                .unwrap();
            if let Message::Binary(b) = frame {
                return p::decode(&b).unwrap();
            }
        }
    }

    async fn welcome(&mut self) -> u8 {
        match self.recv().await {
            ServerMsg::Welcome { player_id, .. } => player_id,
            other => panic!("expected welcome, got {other:?}"),
        }
    }

    async fn turn_start(&mut self) -> (u32, p::PlayerView) {
        loop {
            match self.recv().await {
                ServerMsg::TurnStart { turn, view, .. } => return (turn, view),
                ServerMsg::OrderRejected { .. } | ServerMsg::PreviewResult { .. } => {}
                other => panic!("expected turn start, got {other:?}"),
            }
        }
    }

    async fn resolution(&mut self) -> (u32, Vec<p::VisibleEvent>) {
        loop {
            match self.recv().await {
                ServerMsg::Resolution { turn, events } => return (turn, events),
                ServerMsg::OrderRejected { .. } | ServerMsg::PreviewResult { .. } => {}
                other => panic!("expected resolution, got {other:?}"),
            }
        }
    }
}

fn move_order(unit: u32, x: f64, y: f64) -> ClientMsg {
    ClientMsg::SubmitOrders {
        turn: 1,
        orders: vec![p::Order::Move {
            unit,
            waypoints: vec![p::Vec2 { x, y }],
        }],
    }
}

fn event_ids(e: &p::EventKind) -> Vec<u32> {
    match *e {
        p::EventKind::UnitMoved { unit, .. }
        | p::EventKind::UnitDamaged { unit, .. }
        | p::EventKind::UnitDestroyed { unit }
        | p::EventKind::TrackGained { unit, .. }
        | p::EventKind::TrackLost { unit, .. } => vec![unit],
        p::EventKind::ShotFired { shooter, target } => shooter.into_iter().chain(target).collect(),
    }
}

/// Both players connect and play turn 1. Returns the clients and their turn-2 views.
async fn play_turn_one(addr: SocketAddr) -> (Client, Client, p::PlayerView, p::PlayerView) {
    let mut a = Client::connect(addr, "ayse", p::PROTOCOL_VERSION).await;
    assert_eq!(a.welcome().await, 0);
    let mut b = Client::connect(addr, "baha", p::PROTOCOL_VERSION).await;
    assert_eq!(b.welcome().await, 1);
    let (ta, view_a) = a.turn_start().await;
    let (tb, view_b) = b.turn_start().await;
    assert_eq!((ta, tb), (1, 1));

    // Unit 4 is far away and must be unknown to player 0.
    assert!(view_a.contacts.iter().all(|c| c.unit != 4));

    a.send(&ClientMsg::RequestPreview {
        unit: 2,
        waypoints: vec![p::Vec2 {
            x: 1000.0,
            y: 2000.0,
        }],
    })
    .await;
    match a.recv().await {
        ServerMsg::PreviewResult {
            unit: 2,
            path,
            reason: None,
        } => assert!(!path.is_empty()),
        other => panic!("bad preview {other:?}"),
    }
    a.send(&move_order(2, 1000.0, 2000.0)).await;
    b.send(&move_order(1, 0.0, 0.0)).await; // not b's unit
    match b.recv().await {
        ServerMsg::OrderRejected { reason } => assert!(reason.contains("not yours")),
        other => panic!("expected rejection, got {other:?}"),
    }
    a.send(&ClientMsg::Ready).await;
    b.send(&ClientMsg::Ready).await;

    for (client, view) in [(&mut a, &view_a), (&mut b, &view_b)] {
        let (turn, events) = client.resolution().await;
        assert_eq!(turn, 1);
        assert!(!events.is_empty());
        // Every id in the stream is own, already known, or introduced by TrackGained first.
        let mut known: BTreeSet<u32> = view.own_units.iter().map(|u| u.id).collect();
        known.extend(view.contacts.iter().map(|c| c.unit));
        for e in &events {
            if let p::EventKind::TrackGained { unit, .. } = e.kind {
                known.insert(unit);
            }
            for id in event_ids(&e.kind) {
                assert!(known.contains(&id), "unit {id} leaked at tick {}", e.tick);
            }
        }
    }
    let (t2a, view_a2) = a.turn_start().await;
    let (t2b, view_b2) = b.turn_start().await;
    assert_eq!((t2a, t2b), (2, 2));
    let moved = view_a2.own_units.iter().find(|u| u.id == 2).unwrap();
    assert!(moved.pos.y > 1200.0, "unit 2 should have moved south");
    (a, b, view_a2, view_b2)
}

#[tokio::test]
async fn g7_end_to_end_turn() {
    let s = setup("e2e", 45);
    let dir = s.saves_dir.clone();
    let (addr, _server) = start(s).await;
    play_turn_one(addr).await;
    assert!(dir.join("test").join("turn_2.json.zst").exists());
}

#[tokio::test]
async fn g7_resume_after_shutdown() {
    let s = setup("resume", 45);
    let dir = s.saves_dir.clone();
    let (addr, server) = start(s).await;
    let (a, b, view_a, _) = play_turn_one(addr).await;
    server.abort();
    drop((a, b));

    let save = load_latest(&dir, "test").unwrap();
    assert_eq!(save.game.turn, 2);
    let mut s = setup("resume", 45);
    s.game = save.game;
    s.players = save.players;
    let (addr, _server) = start(s).await;
    let mut b = Client::connect(addr, "baha", p::PROTOCOL_VERSION).await;
    assert_eq!(b.welcome().await, 1);
    let mut a = Client::connect(addr, "ayse", p::PROTOCOL_VERSION).await;
    assert_eq!(a.welcome().await, 0);
    let (turn, resumed) = a.turn_start().await;
    assert_eq!(turn, 2);
    assert_eq!(resumed.own_units, view_a.own_units);
}

#[tokio::test]
async fn g7_reconnect_same_name() {
    let (addr, _server) = start(setup("reconnect", 45)).await;
    let mut a = Client::connect(addr, "ayse", p::PROTOCOL_VERSION).await;
    a.welcome().await;
    let mut b = Client::connect(addr, "baha", p::PROTOCOL_VERSION).await;
    b.welcome().await;
    a.turn_start().await;
    b.turn_start().await;
    drop(a);
    // A stranger cannot take the slot while the match waits.
    let mut c = Client::connect(addr, "can", p::PROTOCOL_VERSION).await;
    assert!(matches!(c.recv().await, ServerMsg::Error { code, .. } if code == "join_refused"));

    let mut a = Client::connect(addr, "ayse", p::PROTOCOL_VERSION).await;
    assert_eq!(a.welcome().await, 0);
    let (turn, view) = a.turn_start().await;
    assert_eq!(turn, 1);
    assert_eq!(view.own_units.len(), 2);
    // The match continues normally.
    a.send(&ClientMsg::Ready).await;
    b.send(&ClientMsg::Ready).await;
    assert_eq!(a.resolution().await.0, 1);
}

#[tokio::test]
async fn g7_version_mismatch_rejected() {
    let (addr, _server) = start(setup("version", 45)).await;
    let mut c = Client::connect(addr, "eski", p::PROTOCOL_VERSION + 1).await;
    assert!(matches!(c.recv().await, ServerMsg::Error { code, .. } if code == "version_mismatch"));
}

#[tokio::test]
async fn g7_planning_deadline_starts_resolution() {
    let (addr, _server) = start(setup("deadline", 1)).await;
    let mut a = Client::connect(addr, "ayse", p::PROTOCOL_VERSION).await;
    a.welcome().await;
    let mut b = Client::connect(addr, "baha", p::PROTOCOL_VERSION).await;
    b.welcome().await;
    a.turn_start().await;
    b.turn_start().await;
    // Nobody says Ready; the 1 s deadline must trigger resolution.
    assert_eq!(a.resolution().await.0, 1);
    assert_eq!(b.resolution().await.0, 1);
}
