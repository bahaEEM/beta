// SPDX-License-Identifier: AGPL-3.0-or-later
//! G5: per-player visibility. Nothing leaves here about a unit the player cannot see.

use crate::data::Rules;
use crate::event::{Event, TickEvent};
use crate::sensor::{Bearing, TrackTable};
use crate::world::{PlayerId, UnitId, UnitState, Vec2, World};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Exact for own units and quality >= 2; a coarse area for quality 1;
/// only a direction for bearing-only (single ESM line) tracks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Position {
    Exact(Vec2),
    Area { center: Vec2, radius_m: f64 },
    Bearing { origin: Vec2, angle_rad: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SeenEvent {
    UnitMoved {
        unit: UnitId,
        position: Position,
        heading_rad: Option<f64>,
    },
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeenTickEvent {
    pub tick: u32,
    pub event: SeenEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contact {
    pub unit: UnitId,
    pub quality: u8,
    /// Only known at quality >= 2.
    pub type_id: Option<String>,
    pub position: Position,
    pub last_seen_tick: u32,
}

const OWN: u8 = u8::MAX;
const EXACT_QUALITY: u8 = 2;

/// Snaps a position to the centre of its contact cell.
pub fn area(rules: &Rules, pos: Vec2) -> Position {
    let cell = rules.visibility.contact_cell_m;
    let snap = |v: f64| ((v / cell).floor() + 0.5) * cell;
    Position::Area {
        center: Vec2::new(snap(pos.x), snap(pos.y)),
        radius_m: cell * std::f64::consts::SQRT_2 / 2.0,
    }
}

fn position(rules: &Rules, level: u8, pos: Vec2) -> Position {
    if level >= EXACT_QUALITY {
        Position::Exact(pos)
    } else {
        area(rules, pos)
    }
}

/// What a player knows about one enemy unit while filtering.
#[derive(Clone, Copy, Default)]
struct Known {
    quality: u8,
    bearing: Option<Bearing>,
    position_known: bool,
}

fn bearing_position(b: Bearing) -> Position {
    Position::Bearing {
        origin: b.origin,
        angle_rad: b.angle_rad,
    }
}

/// Filters one turn's events for `player`. `start_tracks` is the track table at the start
/// of the turn; visibility then follows the TrackGained/TrackLost events in the stream,
/// which `step_tick` emits before the other events of the same tick.
/// Bearing-only tracks reveal the unit id and a direction, never a position.
pub fn filter_for_player(
    events: &[TickEvent],
    start_tracks: &TrackTable,
    world: &World,
    rules: &Rules,
    player: PlayerId,
) -> Vec<SeenTickEvent> {
    let mut known: BTreeMap<UnitId, Known> = start_tracks
        .tracks
        .values()
        .filter(|t| t.observer_player == player)
        .map(|t| {
            let k = Known {
                quality: t.quality,
                bearing: t.bearing,
                position_known: t.position_known,
            };
            (t.target_unit, k)
        })
        .collect();
    let own = |id: UnitId| world.unit(id).is_some_and(|u| u.owner == player);
    let level = |k: &BTreeMap<UnitId, Known>, id: UnitId| {
        if own(id) {
            OWN
        } else {
            k.get(&id).map_or(0, |k| k.quality)
        }
    };

    let mut out = Vec::new();
    for TickEvent { tick, event } in events {
        let seen = match *event {
            Event::TrackGained {
                player: p,
                unit,
                quality: q,
                pos,
                bearing,
            } if p == player => {
                let k = known.entry(unit).or_default();
                k.quality = q;
                k.bearing = bearing;
                k.position_known |= bearing.is_none();
                Some(SeenEvent::TrackGained {
                    unit,
                    quality: q,
                    position: bearing.map_or_else(|| position(rules, q, pos), bearing_position),
                })
            }
            Event::TrackLost {
                player: p,
                unit,
                last_known,
            } if p == player => {
                let k = known.entry(unit).or_default();
                k.quality = 0;
                let last_known = match (k.position_known, k.bearing) {
                    (false, Some(b)) => bearing_position(b),
                    _ => area(rules, last_known),
                };
                Some(SeenEvent::TrackLost { unit, last_known })
            }
            Event::TrackGained { .. } | Event::TrackLost { .. } => None,
            Event::UnitMoved {
                unit,
                pos,
                heading_rad,
            } => {
                let l = level(&known, unit);
                let bearing_only =
                    l != OWN && known.get(&unit).is_some_and(|k| k.bearing.is_some());
                (l > 0 && !bearing_only).then(|| SeenEvent::UnitMoved {
                    unit,
                    position: position(rules, l, pos),
                    heading_rad: (l >= EXACT_QUALITY).then_some(heading_rad),
                })
            }
            Event::ShotFired { shooter, target } => {
                let s = (level(&known, shooter) > 0).then_some(shooter);
                let t = (level(&known, target) > 0).then_some(target);
                (s.is_some() || t.is_some()).then_some(SeenEvent::ShotFired {
                    shooter: s,
                    target: t,
                })
            }
            Event::UnitDamaged { unit, hp, state } => {
                let l = level(&known, unit);
                (l > 0).then_some(SeenEvent::UnitDamaged {
                    unit,
                    hp: (l == OWN).then_some(hp),
                    state,
                })
            }
            Event::UnitDestroyed { unit } => {
                (level(&known, unit) > 0).then_some(SeenEvent::UnitDestroyed { unit })
            }
        };
        if let Some(event) = seen {
            out.push(SeenTickEvent { tick: *tick, event });
        }
    }
    out
}

/// Enemy units this player has ever tracked. Lost tracks (quality 0) show the last known
/// area, or the last bearing if the unit was never located.
pub fn contacts(
    tracks: &TrackTable,
    world: &World,
    rules: &Rules,
    player: PlayerId,
) -> Vec<Contact> {
    tracks
        .tracks
        .values()
        .filter(|t| t.observer_player == player)
        .filter_map(|t| {
            let position = match (t.quality, t.bearing, t.position_known) {
                (q, Some(b), _) if q > 0 => bearing_position(b),
                (0, Some(b), false) => bearing_position(b),
                (0, None, false) => return None,
                (0, _, true) => area(rules, t.last_known_pos),
                (q, _, _) => position(rules, q, t.last_known_pos),
            };
            Some(Contact {
                unit: t.target_unit,
                quality: t.quality,
                type_id: (t.quality >= EXACT_QUALITY)
                    .then(|| world.unit(t.target_unit).map(|u| u.type_id.clone()))
                    .flatten(),
                position,
                last_seen_tick: t.last_seen_tick,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::turn::tests::{catalog, game, rules, unit};
    use crate::turn::{plan_phase, step_tick, Order};
    use crate::world::HeightMap;
    use crate::TICKS_PER_TURN;

    /// Tiny deterministic generator for tests (no `rand`).
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    fn hilly(rng: &mut Lcg) -> HeightMap {
        let n = 120;
        let hills: Vec<(f64, f64, f64)> = (0..6)
            .map(|_| (rng.next() * 6000.0, rng.next() * 6000.0, rng.next() * 80.0))
            .collect();
        let h = (0..n * n)
            .map(|k| {
                let (x, y) = ((k % n) as f64 * 50.0, (k / n) as f64 * 50.0);
                hills
                    .iter()
                    .map(|&(hx, hy, a)| a * (-((x - hx).powi(2) + (y - hy).powi(2)) / 6e5).exp())
                    .sum::<f64>() as f32
            })
            .collect();
        HeightMap::new(n, n, 50.0, h).unwrap()
    }

    fn ids(e: &SeenEvent) -> Vec<UnitId> {
        match *e {
            SeenEvent::UnitMoved { unit, .. }
            | SeenEvent::UnitDamaged { unit, .. }
            | SeenEvent::UnitDestroyed { unit }
            | SeenEvent::TrackGained { unit, .. }
            | SeenEvent::TrackLost { unit, .. } => vec![unit],
            SeenEvent::ShotFired { shooter, target } => shooter.into_iter().chain(target).collect(),
        }
    }

    #[test]
    fn g5_cheat_no_unseen_unit_in_filtered_stream_20_scenarios() {
        let (cat, rules) = (catalog(), rules());
        let mut rng = Lcg(42);
        let mut checked = 0;
        let mut bearings = 0;
        for _ in 0..20 {
            let map = hilly(&mut rng);
            let mut units = Vec::new();
            for id in 1..=16u32 {
                let owner = u8::from(id > 8);
                let kind = [
                    "infantry", "infantry", "mbt", "gsr", "esm", "jammer", "infantry", "mbt",
                ][(id % 8) as usize];
                units.push(unit(
                    id,
                    owner,
                    kind,
                    rng.next() * 5900.0,
                    rng.next() * 5900.0,
                ));
            }
            let mut g = game(map, units);
            let mut orders: BTreeMap<PlayerId, Vec<Order>> = BTreeMap::new();
            for u in g.world.units() {
                if cat.get(&u.type_id).is_some_and(|t| t.is_emitter()) && rng.next() < 0.7 {
                    orders.entry(u.owner).or_default().push(Order::SetEmitter {
                        units: vec![u.id],
                        on: true,
                    });
                }
            }
            for u in g.world.units() {
                orders.entry(u.owner).or_default().push(Order::Move {
                    units: vec![u.id],
                    waypoints: vec![Vec2::new(rng.next() * 5900.0, rng.next() * 5900.0)],
                });
            }
            plan_phase(&mut g, &cat, &rules, &orders);

            let start_tracks = g.tracks.clone();
            let mut events = Vec::new();
            // Oracle: quality per (player, unit) after each tick, taken from the live table.
            let mut snapshots = vec![start_tracks.clone()];
            for tick in 1..=TICKS_PER_TURN {
                for event in step_tick(&mut g, &cat, &rules) {
                    events.push(TickEvent { tick, event });
                }
                snapshots.push(g.tracks.clone());
            }
            for player in [PlayerId(0), PlayerId(1)] {
                let q = |tick: u32, id: UnitId| {
                    snapshots[tick as usize]
                        .get(player, id)
                        .map_or(0, |t| t.quality)
                };
                let bearing_only = |tick: u32, id: UnitId| {
                    snapshots[tick as usize]
                        .get(player, id)
                        .is_some_and(|t| t.quality > 0 && t.bearing.is_some())
                };
                for SeenTickEvent { tick, event } in
                    filter_for_player(&events, &start_tracks, &g.world, &rules, player)
                {
                    for id in ids(&event) {
                        let own = g.world.unit(id).is_some_and(|u| u.owner == player);
                        // A TrackLost is about a unit seen until the previous tick.
                        let visible = match event {
                            SeenEvent::TrackLost { .. } => q(tick - 1, id) > 0,
                            _ => q(tick, id) > 0,
                        };
                        assert!(
                            own || visible,
                            "player {player:?} saw unit {id:?} at tick {tick}"
                        );
                        if !own && q(tick, id) < 2 {
                            if let SeenEvent::UnitMoved { position, .. }
                            | SeenEvent::TrackGained { position, .. } = &event
                            {
                                assert!(
                                    !matches!(position, Position::Exact(_)),
                                    "exact pos leaked"
                                );
                            }
                        }
                        if !own && bearing_only(tick, id) {
                            match &event {
                                SeenEvent::UnitMoved { .. } => {
                                    panic!("bearing-only unit {id:?} moved event leaked")
                                }
                                SeenEvent::TrackGained { position, .. } => {
                                    assert!(
                                        matches!(position, Position::Bearing { .. }),
                                        "bearing-only position leaked"
                                    )
                                }
                                _ => {}
                            }
                            bearings += 1;
                        }
                        checked += 1;
                    }
                }
            }
        }
        assert!(checked > 1000, "scenarios too quiet: {checked}");
        assert!(bearings > 0, "no bearing-only tracks exercised");
    }

    #[test]
    fn g5_filter_hides_unseen_and_blurs_contacts() {
        let rules = rules();
        // Enemy 2 at 3000 m: beyond mbt fire range (2500) but inside 1.5x -> quality 1.
        // Enemy 3 far away: never seen.
        let g = game(
            HeightMap::new(400, 400, 50.0, vec![0.0; 160_000]).unwrap(),
            vec![
                unit(1, 0, "mbt", 1000.0, 1000.0),
                unit(2, 1, "mbt", 4000.0, 1000.0),
                unit(3, 1, "mbt", 19000.0, 19000.0),
            ],
        );
        let events = vec![
            TickEvent {
                tick: 1,
                event: Event::UnitMoved {
                    unit: UnitId(2),
                    pos: Vec2::new(4010.0, 1000.0),
                    heading_rad: 0.0,
                },
            },
            TickEvent {
                tick: 1,
                event: Event::UnitMoved {
                    unit: UnitId(3),
                    pos: Vec2::new(19010.0, 19000.0),
                    heading_rad: 0.0,
                },
            },
            TickEvent {
                tick: 1,
                event: Event::UnitMoved {
                    unit: UnitId(1),
                    pos: Vec2::new(1010.0, 1000.0),
                    heading_rad: 0.0,
                },
            },
        ];
        let seen = filter_for_player(&events, &g.tracks, &g.world, &rules, PlayerId(0));
        assert_eq!(seen.len(), 2);
        assert!(matches!(
            &seen[0].event,
            SeenEvent::UnitMoved {
                unit: UnitId(2),
                position: Position::Area { .. },
                heading_rad: None
            }
        ));
        assert!(matches!(
            &seen[1].event,
            SeenEvent::UnitMoved {
                unit: UnitId(1),
                position: Position::Exact(_),
                ..
            }
        ));
        let c = contacts(&g.tracks, &g.world, &rules, PlayerId(0));
        assert_eq!(c.len(), 1);
        assert_eq!(
            (c[0].unit, c[0].quality, c[0].type_id.clone()),
            (UnitId(2), 1, None)
        );
    }
}
