// SPDX-License-Identifier: AGPL-3.0-or-later
//! Client-server messages. Wire format: JSON, then zstd. Independent of sim types;
//! the server converts sim state into these (already visibility-filtered) types.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::fmt;
use std::io::Read;

pub const PROTOCOL_VERSION: u32 = 4;
/// Maximum size of a message's JSON (uncompressed).
pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
const ZSTD_LEVEL: i32 = 3;

pub type UnitId = u32;
pub type PlayerId = u8;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

/// Exact position (track quality >= 2 or own unit) or an approximate area (quality 1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Position {
    Exact {
        pos: Vec2,
    },
    Area {
        center: Vec2,
        radius_m: f64,
    },
    /// Direction only (one ESM bearing): the unit is somewhere along this ray.
    Bearing {
        origin: Vec2,
        angle_rad: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitState {
    Healthy,
    Damaged,
    Destroyed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Order {
    /// Group move; units keep their layout around the waypoints. Cost: 1 KBG per started 12 units.
    Move {
        units: Vec<UnitId>,
        waypoints: Vec<Vec2>,
    },
    /// Cancel routes continuing from earlier turns.
    Stop { units: Vec<UnitId> },
    /// Radar/jammer on or off. Emitting units can be heard by enemy ESM.
    SetEmitter { units: Vec<UnitId>, on: bool },
    /// Rules of engagement: free (fire at will), return (only after being shot at), hold.
    SetStance { units: Vec<UnitId>, stance: Stance },
    /// Artillery/rockets fire at a point or a tracked enemy (track quality >= 2) for this turn.
    FireMission {
        units: Vec<UnitId>,
        target: FireTarget,
    },
    /// Infantry (within 100 m) boards an IFV.
    Embark { units: Vec<UnitId>, carrier: UnitId },
    /// Passengers get out.
    Disembark { units: Vec<UnitId> },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FireTarget {
    Point { pos: Vec2 },
    Unit { unit: UnitId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stance {
    Free,
    Return,
    Hold,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OwnUnit {
    pub id: UnitId,
    pub type_id: String,
    pub pos: Vec2,
    pub heading_rad: f64,
    pub hp: f64,
    pub state: UnitState,
    pub emitting: bool,
    pub fuel: f64,
    pub ammo: u32,
    pub supply_fuel: f64,
    pub supply_ammo: u32,
    pub stance: Stance,
    /// Linked to the HQ: only connected units accept new orders.
    pub connected: bool,
    /// Riding inside this carrier (hidden, cannot act).
    pub carried_by: Option<UnitId>,
    /// Game seconds of setting up still to go; above 0 the unit cannot fire and its
    /// radar, ESM and jammer are off. Always 0 for types that need no deployment.
    pub packed_s: f64,
}

/// An enemy unit as known to this player. `type_id` is only present at quality >= 2.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contact {
    pub unit: UnitId,
    pub quality: u8,
    pub type_id: Option<String>,
    pub position: Position,
    pub last_seen_tick: u32,
}

/// Named region (public map feature), captured by holding the circle at its centre.
/// Capture progress is not sent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ZoneView {
    pub id: u32,
    pub name_tr: String,
    pub center: Vec2,
    pub radius_m: f64,
    pub depot: bool,
    pub owner: Option<PlayerId>,
    /// Home base of this player: an enemy who captures it wins the match.
    pub base: Option<PlayerId>,
}

/// Everything one player is allowed to know at the start of a turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerView {
    pub own_units: Vec<OwnUnit>,
    pub contacts: Vec<Contact>,
    pub kbg_budget: u32,
    pub zones: Vec<ZoneView>,
    /// Regions held per player id.
    pub scores: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    UnitMoved {
        unit: UnitId,
        position: Position,
        heading_rad: Option<f64>,
    },
    /// Ids the player cannot see are `None`.
    ShotFired {
        shooter: Option<UnitId>,
        target: Option<UnitId>,
    },
    UnitDamaged {
        unit: UnitId,
        hp: Option<f64>,
        state: UnitState,
    },
    UnitDestroyed {
        unit: UnitId,
    },
    TrackGained {
        unit: UnitId,
        quality: u8,
        position: Position,
    },
    TrackLost {
        unit: UnitId,
        last_known: Position,
    },
    /// An artillery/rocket round was fired (shooter shown only if visible).
    ShellFired {
        shooter: UnitId,
    },
    /// Public: an explosion.
    ShellImpact {
        pos: Vec2,
        radius_m: f64,
    },
    ZoneCaptured {
        zone: u32,
        player: PlayerId,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisibleEvent {
    pub tick: u32,
    #[serde(flatten)]
    pub kind: EventKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    Hello {
        protocol_version: u32,
        player_name: String,
    },
    SubmitOrders {
        turn: u32,
        orders: Vec<Order>,
    },
    /// One `preview_result` comes back per unit.
    RequestPreview {
        units: Vec<UnitId>,
        waypoints: Vec<Vec2>,
    },
    Ready,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    Welcome {
        player_id: PlayerId,
        map_id: String,
    },
    TurnStart {
        turn: u32,
        deadline_secs: u32,
        view: PlayerView,
    },
    Resolution {
        turn: u32,
        events: Vec<VisibleEvent>,
    },
    /// `path` is empty and `reason` set when no route exists.
    PreviewResult {
        unit: UnitId,
        path: Vec<Vec2>,
        reason: Option<String>,
    },
    /// Reply to `submit_orders` (the accepted part replaces the previous submission).
    OrdersAccepted {
        turn: u32,
        kbg_spent: u32,
        kbg_budget: u32,
    },
    OrderRejected {
        reason: String,
    },
    Error {
        code: String,
        message: String,
    },
    /// Sent after the last Resolution. `winner` is null on a draw.
    GameOver {
        winner: Option<PlayerId>,
        scores: Vec<u32>,
    },
}

#[derive(Debug)]
pub enum ProtoError {
    TooLarge(usize),
    Json(serde_json::Error),
    Zstd(std::io::Error),
    VersionMismatch { ours: u32, theirs: u32 },
}

impl fmt::Display for ProtoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProtoError::TooLarge(n) => {
                write!(f, "message too large: {n} bytes (max {MAX_MESSAGE_BYTES})")
            }
            ProtoError::Json(e) => write!(f, "json: {e}"),
            ProtoError::Zstd(e) => write!(f, "zstd: {e}"),
            ProtoError::VersionMismatch { ours, theirs } => {
                write!(
                    f,
                    "protocol version mismatch: server {ours}, client {theirs}"
                )
            }
        }
    }
}

impl std::error::Error for ProtoError {}

pub fn check_version(theirs: u32) -> Result<(), ProtoError> {
    if theirs == PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(ProtoError::VersionMismatch {
            ours: PROTOCOL_VERSION,
            theirs,
        })
    }
}

/// Serialises to JSON and compresses with zstd.
pub fn encode<T: Serialize>(msg: &T) -> Result<Vec<u8>, ProtoError> {
    let json = serde_json::to_vec(msg).map_err(ProtoError::Json)?;
    if json.len() > MAX_MESSAGE_BYTES {
        return Err(ProtoError::TooLarge(json.len()));
    }
    zstd::encode_all(json.as_slice(), ZSTD_LEVEL).map_err(ProtoError::Zstd)
}

/// Decompresses and parses; refuses to inflate beyond `MAX_MESSAGE_BYTES`.
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ProtoError> {
    let decoder = zstd::stream::Decoder::new(bytes).map_err(ProtoError::Zstd)?;
    let mut json = Vec::new();
    decoder
        .take(MAX_MESSAGE_BYTES as u64 + 1)
        .read_to_end(&mut json)
        .map_err(ProtoError::Zstd)?;
    if json.len() > MAX_MESSAGE_BYTES {
        return Err(ProtoError::TooLarge(json.len()));
    }
    serde_json::from_slice(&json).map_err(ProtoError::Json)
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: Vec2 = Vec2 {
        x: 1200.0,
        y: 850.5,
    };

    fn client_examples() -> Vec<ClientMsg> {
        vec![
            ClientMsg::Hello {
                protocol_version: PROTOCOL_VERSION,
                player_name: "baha".into(),
            },
            ClientMsg::SubmitOrders {
                turn: 3,
                orders: vec![
                    Order::Move {
                        units: vec![7, 8],
                        waypoints: vec![P],
                    },
                    Order::Stop { units: vec![9] },
                    Order::SetEmitter {
                        units: vec![10],
                        on: true,
                    },
                    Order::SetStance {
                        units: vec![7, 8],
                        stance: Stance::Return,
                    },
                    Order::FireMission {
                        units: vec![11],
                        target: FireTarget::Unit { unit: 12 },
                    },
                    Order::Embark {
                        units: vec![3],
                        carrier: 4,
                    },
                ],
            },
            ClientMsg::RequestPreview {
                units: vec![7, 8],
                waypoints: vec![P],
            },
            ClientMsg::Ready,
        ]
    }

    fn server_examples() -> Vec<ServerMsg> {
        let area = Position::Area {
            center: P,
            radius_m: 500.0,
        };
        let exact = Position::Exact { pos: P };
        vec![
            ServerMsg::Welcome {
                player_id: 0,
                map_id: "hasandag".into(),
            },
            ServerMsg::TurnStart {
                turn: 3,
                deadline_secs: 45,
                view: PlayerView {
                    own_units: vec![OwnUnit {
                        id: 7,
                        type_id: "mbt".into(),
                        pos: P,
                        heading_rad: 1.5,
                        hp: 100.0,
                        state: UnitState::Healthy,
                        emitting: false,
                        fuel: 1100.0,
                        ammo: 38,
                        supply_fuel: 0.0,
                        supply_ammo: 0,
                        stance: Stance::Free,
                        connected: true,
                        carried_by: None,
                        packed_s: 0.0,
                    }],
                    contacts: vec![
                        Contact {
                            unit: 12,
                            quality: 1,
                            type_id: None,
                            position: area.clone(),
                            last_seen_tick: 140,
                        },
                        Contact {
                            unit: 14,
                            quality: 1,
                            type_id: None,
                            position: Position::Bearing {
                                origin: P,
                                angle_rad: 0.8,
                            },
                            last_seen_tick: 150,
                        },
                    ],
                    kbg_budget: 8,
                    zones: vec![ZoneView {
                        id: 1,
                        name_tr: "Helvadere".into(),
                        center: P,
                        radius_m: 400.0,
                        depot: true,
                        owner: Some(0),
                        base: Some(0),
                    }],
                    scores: vec![8, 8],
                },
            },
            ServerMsg::Resolution {
                turn: 3,
                events: vec![
                    VisibleEvent {
                        tick: 1,
                        kind: EventKind::UnitMoved {
                            unit: 7,
                            position: exact.clone(),
                            heading_rad: Some(1.5),
                        },
                    },
                    VisibleEvent {
                        tick: 2,
                        kind: EventKind::TrackGained {
                            unit: 12,
                            quality: 3,
                            position: exact,
                        },
                    },
                    VisibleEvent {
                        tick: 3,
                        kind: EventKind::ShotFired {
                            shooter: Some(7),
                            target: Some(12),
                        },
                    },
                    VisibleEvent {
                        tick: 3,
                        kind: EventKind::UnitDamaged {
                            unit: 12,
                            hp: None,
                            state: UnitState::Damaged,
                        },
                    },
                    VisibleEvent {
                        tick: 83,
                        kind: EventKind::UnitDestroyed { unit: 12 },
                    },
                    VisibleEvent {
                        tick: 100,
                        kind: EventKind::ShellFired { shooter: 11 },
                    },
                    VisibleEvent {
                        tick: 150,
                        kind: EventKind::ShellImpact {
                            pos: P,
                            radius_m: 60.0,
                        },
                    },
                    VisibleEvent {
                        tick: 120,
                        kind: EventKind::ZoneCaptured { zone: 1, player: 1 },
                    },
                    VisibleEvent {
                        tick: 90,
                        kind: EventKind::TrackLost {
                            unit: 13,
                            last_known: area.clone(),
                        },
                    },
                ],
            },
            ServerMsg::PreviewResult {
                unit: 7,
                path: vec![P],
                reason: None,
            },
            ServerMsg::OrdersAccepted {
                turn: 3,
                kbg_spent: 2,
                kbg_budget: 8,
            },
            ServerMsg::OrderRejected {
                reason: "kbg budget exceeded".into(),
            },
            ServerMsg::Error {
                code: "version_mismatch".into(),
                message: "protocol version mismatch: server 2, client 1".into(),
            },
            ServerMsg::GameOver {
                winner: Some(1),
                scores: vec![9, 14],
            },
        ]
    }

    #[test]
    fn g6_client_messages_round_trip() {
        for msg in client_examples() {
            let back: ClientMsg = decode(&encode(&msg).unwrap()).unwrap();
            assert_eq!(back, msg);
        }
    }

    #[test]
    fn g6_server_messages_round_trip() {
        for msg in server_examples() {
            let back: ServerMsg = decode(&encode(&msg).unwrap()).unwrap();
            assert_eq!(back, msg);
        }
    }

    #[test]
    fn g6_version_mismatch_rejected() {
        assert!(check_version(PROTOCOL_VERSION).is_ok());
        assert!(matches!(
            check_version(PROTOCOL_VERSION + 1),
            Err(ProtoError::VersionMismatch { .. })
        ));
    }

    #[test]
    fn g6_oversized_message_rejected() {
        let msg = ClientMsg::Hello {
            protocol_version: 1,
            player_name: "x".repeat(MAX_MESSAGE_BYTES),
        };
        assert!(matches!(encode(&msg), Err(ProtoError::TooLarge(_))));
        // A small zstd frame that inflates past the limit is refused too.
        let json = serde_json::to_vec(&msg).unwrap();
        let bomb = zstd::encode_all(json.as_slice(), 3).unwrap();
        assert!(matches!(
            decode::<ClientMsg>(&bomb),
            Err(ProtoError::TooLarge(_))
        ));
    }

    /// docs/protocol.md examples are generated from the messages above.
    /// Regenerate with: UPDATE_DOCS=1 cargo test -p protocol
    #[test]
    fn g6_protocol_md_up_to_date() {
        let mut md = String::from(include_str!("../protocol_header.md"));
        for (title, msgs) in [
            (
                "İstemci → sunucu",
                client_examples().iter().map(to_json).collect::<Vec<_>>(),
            ),
            (
                "Sunucu → istemci",
                server_examples().iter().map(to_json).collect(),
            ),
        ] {
            md += &format!("\n## {title}\n");
            for json in msgs {
                md += &format!("\n```json\n{json}\n```\n");
            }
        }
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/protocol.md");
        if std::env::var("UPDATE_DOCS").is_ok() {
            std::fs::write(path, &md).unwrap();
        }
        assert_eq!(
            std::fs::read_to_string(path).unwrap_or_default(),
            md,
            "run UPDATE_DOCS=1 cargo test -p protocol"
        );
    }

    fn to_json<T: Serialize>(msg: &T) -> String {
        serde_json::to_string_pretty(msg).unwrap()
    }
}
