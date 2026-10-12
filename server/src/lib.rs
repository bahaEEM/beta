// SPDX-License-Identifier: AGPL-3.0-or-later
//! Authoritative match server: one match, two players, WebSocket (JSON + zstd).
//!
//! Each connection runs in its own task; a single match task owns the game state and
//! receives everything over a channel. Resolution runs on `spawn_blocking`.

mod convert;
pub mod setup;

use futures_util::{SinkExt, StreamExt};
use protocol::{self as p, ClientMsg, ServerMsg};
use setup::{save_game, SaveFile};
use sim::data::{Rules, UnitCatalog};
use sim::turn::{group_routes, plan_phase, resolve_phase, validate_orders, Game, Order};
use sim::visibility::{contacts, filter_for_player, SeenTickEvent};
use sim::world::PlayerId;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{sleep_until, Instant};
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

pub const PLAYERS: usize = 2;

/// Everything needed to run (or resume) a match.
pub struct MatchSetup {
    pub match_id: String,
    pub map_id: String,
    pub saves_dir: PathBuf,
    pub catalog: UnitCatalog,
    pub rules: Rules,
    pub game: Game,
    /// Player names by player id (filled when resuming).
    pub players: [Option<String>; PLAYERS],
}

type Tx = mpsc::UnboundedSender<Message>;

enum ToMatch {
    Hello {
        name: String,
        conn: u64,
        tx: Tx,
        reply: oneshot::Sender<Result<u8, String>>,
    },
    Client {
        conn: u64,
        player: u8,
        msg: ClientMsg,
    },
    Gone {
        conn: u64,
        player: u8,
    },
    Resolved(Box<Resolved>),
}

struct Resolved {
    game: Game,
    turn: u32,
    events: [Vec<SeenTickEvent>; PLAYERS],
    took: Duration,
}

/// Accepts connections forever and runs the match.
pub async fn serve(listener: TcpListener, setup: MatchSetup) -> Result<(), BoxError> {
    save_game(&setup.saves_dir, &save_file(&setup))?;
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(Match::new(setup, tx.clone()).run(rx));
    let mut next_conn = 0u64;
    loop {
        let (stream, addr) = match listener.accept().await {
            Ok(s) => s,
            Err(e) => {
                warn!("accept failed: {e}");
                continue;
            }
        };
        next_conn += 1;
        let conn = next_conn;
        let to_match = tx.clone();
        tokio::spawn(async move {
            if let Err(e) = connection(stream, conn, to_match).await {
                info!("connection {conn} ({addr}) ended: {e}");
            }
        });
    }
}

fn save_file(s: &MatchSetup) -> SaveFile {
    SaveFile {
        match_id: s.match_id.clone(),
        map_id: s.map_id.clone(),
        players: s.players.clone(),
        game: s.game.clone(),
    }
}

fn binary(msg: &ServerMsg) -> Option<Message> {
    match p::encode(msg) {
        Ok(bytes) => Some(Message::Binary(bytes.into())),
        Err(e) => {
            error!("cannot encode server message: {e}");
            None
        }
    }
}

async fn connection(
    stream: TcpStream,
    conn: u64,
    to_match: mpsc::UnboundedSender<ToMatch>,
) -> Result<(), BoxError> {
    let ws = tokio_tungstenite::accept_async(stream).await?;
    let (mut sink, mut source) = ws.split();
    let (tx, mut out_rx) = mpsc::unbounded_channel::<Message>();
    let writer = tokio::spawn(async move {
        while let Some(m) = out_rx.recv().await {
            let close = matches!(m, Message::Close(_));
            if sink.send(m).await.is_err() || close {
                break;
            }
        }
    });

    let send_error = |code: &str, message: String| {
        if let Some(m) = binary(&ServerMsg::Error {
            code: code.into(),
            message,
        }) {
            let _ = tx.send(m);
        }
        let _ = tx.send(Message::Close(None));
    };

    // The first message must be Hello with our protocol version.
    let player = loop {
        let Some(frame) = source.next().await else {
            return Ok(());
        };
        let Message::Binary(bytes) = frame? else {
            continue; // pings etc.
        };
        match p::decode::<ClientMsg>(&bytes) {
            Ok(ClientMsg::Hello {
                protocol_version,
                player_name,
            }) => {
                if let Err(e) = p::check_version(protocol_version) {
                    send_error("version_mismatch", e.to_string());
                    break None;
                }
                let (reply, answer) = oneshot::channel();
                to_match.send(ToMatch::Hello {
                    name: player_name,
                    conn,
                    tx: tx.clone(),
                    reply,
                })?;
                match answer.await? {
                    Ok(id) => break Some(id),
                    Err(reason) => {
                        send_error("join_refused", reason);
                        break None;
                    }
                }
            }
            Ok(_) => send_error("hello_expected", "first message must be hello".into()),
            Err(e) => send_error("bad_message", e.to_string()),
        }
    };

    if let Some(player) = player {
        while let Some(frame) = source.next().await {
            let Ok(Message::Binary(bytes)) = frame else {
                if frame.is_err() {
                    break;
                }
                continue;
            };
            match p::decode::<ClientMsg>(&bytes) {
                Ok(msg) => to_match.send(ToMatch::Client { conn, player, msg })?,
                Err(e) => {
                    if let Some(m) = binary(&ServerMsg::Error {
                        code: "bad_message".into(),
                        message: e.to_string(),
                    }) {
                        let _ = tx.send(m);
                    }
                }
            }
        }
        to_match.send(ToMatch::Gone { conn, player })?;
    }
    drop(tx);
    let _ = writer.await;
    Ok(())
}

#[derive(PartialEq)]
enum Phase {
    Lobby,
    Planning,
    Resolving,
    Finished,
}

#[derive(Default)]
struct Slot {
    name: Option<String>,
    conn: Option<(u64, Tx)>,
    orders: Vec<Order>,
    ready: bool,
}

struct Match {
    match_id: String,
    map_id: String,
    saves_dir: PathBuf,
    catalog: Arc<UnitCatalog>,
    rules: Arc<Rules>,
    game: Game,
    slots: [Slot; PLAYERS],
    phase: Phase,
    deadline: Option<Instant>,
    /// Planning time left while paused for a disconnected player.
    remaining: Duration,
    to_self: mpsc::UnboundedSender<ToMatch>,
}

impl Match {
    fn new(s: MatchSetup, to_self: mpsc::UnboundedSender<ToMatch>) -> Self {
        let [a, b] = s.players;
        let finished = s.game.result.is_some();
        Self {
            match_id: s.match_id,
            map_id: s.map_id,
            saves_dir: s.saves_dir,
            catalog: Arc::new(s.catalog),
            remaining: Duration::from_secs(u64::from(s.rules.turn.planning_secs)),
            rules: Arc::new(s.rules),
            game: s.game,
            slots: [
                Slot {
                    name: a,
                    ..Slot::default()
                },
                Slot {
                    name: b,
                    ..Slot::default()
                },
            ],
            phase: if finished {
                Phase::Finished
            } else {
                Phase::Lobby
            },
            deadline: None,
            to_self,
        }
    }

    async fn run(mut self, mut rx: mpsc::UnboundedReceiver<ToMatch>) {
        info!(match_id = %self.match_id, turn = self.game.turn, "match ready, waiting for players");
        loop {
            let deadline = self.deadline;
            tokio::select! {
                msg = rx.recv() => match msg {
                    Some(m) => self.handle(m),
                    None => break,
                },
                _ = async { if let Some(d) = deadline { sleep_until(d).await } }, if deadline.is_some() => {
                    info!(turn = self.game.turn, "planning time is up");
                    self.start_resolve();
                }
            }
        }
    }

    fn send(&self, player: usize, msg: &ServerMsg) {
        if let (Some((_, tx)), Some(m)) = (
            self.slots.get(player).and_then(|s| s.conn.as_ref()),
            binary(msg),
        ) {
            let _ = tx.send(m);
        }
    }

    fn all_connected(&self) -> bool {
        self.slots.iter().all(|s| s.conn.is_some())
    }

    fn handle(&mut self, msg: ToMatch) {
        match msg {
            ToMatch::Hello {
                name,
                conn,
                tx,
                reply,
            } => {
                let result = self.join(name, conn, tx);
                let joined = result.as_ref().ok().copied();
                let _ = reply.send(result);
                if let Some(p) = joined {
                    self.after_join(usize::from(p));
                }
            }
            ToMatch::Client { conn, player, msg } => {
                let p = usize::from(player);
                if self
                    .slots
                    .get(p)
                    .and_then(|s| s.conn.as_ref())
                    .is_some_and(|(c, _)| *c == conn)
                {
                    self.client(p, msg);
                }
            }
            ToMatch::Gone { conn, player } => {
                let p = usize::from(player);
                let Some(slot) = self.slots.get_mut(p) else {
                    return;
                };
                if slot.conn.as_ref().is_some_and(|(c, _)| *c == conn) {
                    slot.conn = None;
                    warn!(player, "player disconnected; match waits");
                    if let Some(d) = self.deadline.take() {
                        self.remaining = d.saturating_duration_since(Instant::now());
                    }
                }
            }
            ToMatch::Resolved(r) => self.finish_resolve(*r),
        }
    }

    /// Same name = reconnect to that slot; otherwise the first free slot.
    fn join(&mut self, name: String, conn: u64, tx: Tx) -> Result<u8, String> {
        let idx = match self
            .slots
            .iter()
            .position(|s| s.name.as_deref() == Some(name.as_str()))
        {
            Some(i) if self.slots.get(i).is_some_and(|s| s.conn.is_some()) => {
                return Err(format!("player '{name}' is already connected"))
            }
            Some(i) => i,
            None => self
                .slots
                .iter()
                .position(|s| s.name.is_none())
                .ok_or_else(|| "match is full".to_string())?,
        };
        let slot = self.slots.get_mut(idx).ok_or("no slot")?;
        slot.name = Some(name.clone());
        slot.conn = Some((conn, tx));
        info!(player = idx, %name, "player joined");
        u8::try_from(idx).map_err(|e| e.to_string())
    }

    fn after_join(&mut self, p: usize) {
        self.send(
            p,
            &ServerMsg::Welcome {
                player_id: p as u8,
                map_id: self.map_id.clone(),
            },
        );
        match self.phase {
            Phase::Lobby if self.all_connected() => self.begin_planning(),
            Phase::Planning => {
                if self.all_connected() && self.deadline.is_none() {
                    self.deadline = Some(Instant::now() + self.remaining);
                }
                self.send(p, &self.turn_start(p));
            }
            Phase::Finished => self.send(p, &self.game_over()),
            _ => {} // lobby with one player, or resolving: TurnStart follows
        }
    }

    fn begin_planning(&mut self) {
        self.phase = Phase::Planning;
        self.remaining = Duration::from_secs(u64::from(self.rules.turn.planning_secs));
        self.deadline = self
            .all_connected()
            .then(|| Instant::now() + self.remaining);
        for s in &mut self.slots {
            s.orders.clear();
            s.ready = false;
        }
        info!(turn = self.game.turn, "planning phase");
        for p in 0..PLAYERS {
            self.send(p, &self.turn_start(p));
        }
    }

    fn turn_start(&self, p: usize) -> ServerMsg {
        let player = PlayerId(p as u8);
        let left = self.deadline.map_or(self.remaining, |d| {
            d.saturating_duration_since(Instant::now())
        });
        let own_units = self
            .game
            .world
            .units()
            .iter()
            .filter(|u| u.owner == player)
            .map(|u| p::OwnUnit {
                id: u.id.0,
                type_id: u.type_id.clone(),
                pos: convert::vec2(u.pos),
                heading_rad: u.heading_rad,
                hp: u.hp,
                state: convert::state(u.state),
                emitting: u.emitting,
                fuel: u.fuel,
                ammo: u.ammo,
                supply_fuel: u.supply_fuel,
                supply_ammo: u.supply_ammo,
                stance: convert::stance(u.stance),
                connected: !self.game.hq_players.contains(&player)
                    || self
                        .game
                        .connected
                        .get(&player)
                        .is_some_and(|c| c.contains(&u.id)),
                carried_by: u.carried_by.map(|c| c.0),
            })
            .collect();
        let contacts = contacts(&self.game.tracks, &self.game.world, &self.rules, player)
            .iter()
            .map(convert::contact)
            .collect();
        ServerMsg::TurnStart {
            turn: self.game.turn,
            deadline_secs: left.as_secs_f64().ceil() as u32,
            view: p::PlayerView {
                own_units,
                contacts,
                kbg_budget: self.rules.kbg.budget_per_turn,
                zones: self.game.zones.iter().map(convert::zone).collect(),
                scores: self.scores(),
            },
        }
    }

    fn client(&mut self, p: usize, msg: ClientMsg) {
        let player = PlayerId(p as u8);
        match msg {
            ClientMsg::Hello { .. } => {}
            ClientMsg::SubmitOrders { turn, orders } => {
                if self.phase != Phase::Planning || turn != self.game.turn {
                    self.send(
                        p,
                        &ServerMsg::OrderRejected {
                            reason: format!("not planning turn {turn}"),
                        },
                    );
                    return;
                }
                let orders: Vec<Order> = orders.iter().map(convert::order).collect();
                let v = validate_orders(&self.game, &self.catalog, &self.rules, player, &orders);
                for r in &v.rejected {
                    let ids: Vec<String> =
                        r.order.units().iter().map(|u| u.0.to_string()).collect();
                    self.send(
                        p,
                        &ServerMsg::OrderRejected {
                            reason: format!("units {}: {}", ids.join(","), r.reason),
                        },
                    );
                }
                self.send(
                    p,
                    &ServerMsg::OrdersAccepted {
                        turn,
                        kbg_spent: v.kbg_spent,
                        kbg_budget: self.rules.kbg.budget_per_turn,
                    },
                );
                if let Some(slot) = self.slots.get_mut(p) {
                    slot.orders = orders
                        .into_iter()
                        .filter(|o| !v.rejected.iter().any(|r| r.order == *o))
                        .collect();
                }
            }
            ClientMsg::RequestPreview { units, waypoints } => {
                let ids = convert::units(&units);
                let wps: Vec<_> = waypoints.into_iter().map(convert::from_vec2).collect();
                let mine = ids
                    .iter()
                    .all(|&id| self.game.world.unit(id).is_some_and(|u| u.owner == player));
                let result = if mine {
                    group_routes(&self.game.world, &self.catalog, &ids, &wps)
                        .map_err(|e| e.to_string())
                } else {
                    Err("unit is not yours".into())
                };
                match result {
                    Ok(routes) => {
                        for (id, path) in routes {
                            let path = path.into_iter().map(convert::vec2).collect();
                            self.send(
                                p,
                                &ServerMsg::PreviewResult {
                                    unit: id.0,
                                    path,
                                    reason: None,
                                },
                            );
                        }
                    }
                    Err(reason) => {
                        for &unit in &units {
                            let reason = Some(reason.clone());
                            self.send(
                                p,
                                &ServerMsg::PreviewResult {
                                    unit,
                                    path: vec![],
                                    reason,
                                },
                            );
                        }
                    }
                }
            }
            ClientMsg::Ready => {
                if self.phase == Phase::Planning {
                    if let Some(slot) = self.slots.get_mut(p) {
                        slot.ready = true;
                    }
                    if self.slots.iter().all(|s| s.ready) {
                        self.start_resolve();
                    }
                }
            }
        }
    }

    fn start_resolve(&mut self) {
        self.phase = Phase::Resolving;
        self.deadline = None;
        let mut game = self.game.clone();
        let (catalog, rules) = (Arc::clone(&self.catalog), Arc::clone(&self.rules));
        let orders: BTreeMap<PlayerId, Vec<Order>> = self
            .slots
            .iter()
            .enumerate()
            .map(|(i, s)| (PlayerId(i as u8), s.orders.clone()))
            .collect();
        let to_self = self.to_self.clone();
        let (dir, match_id, map_id) = (
            self.saves_dir.clone(),
            self.match_id.clone(),
            self.map_id.clone(),
        );
        let players = [0, 1].map(|i| self.slots.get(i).and_then(|s| s.name.clone()));
        tokio::task::spawn_blocking(move || {
            let started = Instant::now();
            let turn = game.turn;
            plan_phase(&mut game, &catalog, &rules, &orders);
            let start_tracks = game.tracks.clone();
            let events = resolve_phase(&mut game, &catalog, &rules);
            let filtered = [0u8, 1].map(|p| {
                filter_for_player(&events, &start_tracks, &game.world, &rules, PlayerId(p))
            });
            let took = started.elapsed();
            let save = SaveFile {
                match_id,
                map_id,
                players,
                game,
            };
            if let Err(e) = save_game(&dir, &save) {
                error!("saving turn {} failed: {e}", save.game.turn);
            }
            let game = save.game;
            let _ = to_self.send(ToMatch::Resolved(Box::new(Resolved {
                game,
                turn,
                events: filtered,
                took,
            })));
        });
    }

    fn finish_resolve(&mut self, r: Resolved) {
        info!(
            turn = r.turn,
            took_ms = r.took.as_secs_f64() * 1000.0,
            "resolution done"
        );
        self.game = r.game;
        for (p, events) in r.events.iter().enumerate() {
            let events = events.iter().map(convert::event).collect();
            self.send(
                p,
                &ServerMsg::Resolution {
                    turn: r.turn,
                    events,
                },
            );
        }
        if self.game.result.is_some() {
            self.phase = Phase::Finished;
            info!(result = ?self.game.result, scores = ?self.scores(), "match over");
            for p in 0..PLAYERS {
                self.send(p, &self.game_over());
            }
        } else {
            self.begin_planning();
        }
    }

    fn scores(&self) -> Vec<u32> {
        (0..PLAYERS as u8)
            .map(|p| self.game.scores.get(&PlayerId(p)).copied().unwrap_or(0))
            .collect()
    }

    fn game_over(&self) -> ServerMsg {
        ServerMsg::GameOver {
            winner: self.game.result.and_then(|r| r.winner).map(|p| p.0),
            scores: self.scores(),
        }
    }
}
