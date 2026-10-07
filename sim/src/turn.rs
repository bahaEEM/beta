// SPDX-License-Identifier: AGPL-3.0-or-later
//! G5: turn engine. Planning validates orders (KBG budget); resolution runs 200 ticks.

use crate::combat::{step_combat, Reloads};
use crate::data::{Rules, UnitCatalog};
use crate::event::{Event, TickEvent};
use crate::logistics::{end_of_turn, step_capture, MatchResult, Zone};
use crate::movement::{plan_route, step_movement, MoveError, MoveOrder, Routes};
use crate::sensor::{update_tracks, TrackTable};
use crate::world::{PlayerId, UnitId, Vec2, World};
use crate::TICKS_PER_TURN;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Complete simulation state between turns (this is what the server saves).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Game {
    pub turn: u32,
    pub world: World,
    pub routes: Routes,
    pub tracks: TrackTable,
    pub reloads: Reloads,
    #[serde(default)]
    pub zones: Vec<Zone>,
    #[serde(default)]
    pub scores: BTreeMap<PlayerId, u32>,
    /// Set when the match is over.
    #[serde(default)]
    pub result: Option<MatchResult>,
}

impl Game {
    /// Starts at turn 1 with tracks already computed for the initial positions.
    pub fn new(world: World, catalog: &UnitCatalog, rules: &Rules) -> Self {
        let mut tracks = TrackTable::default();
        update_tracks(&world, catalog, rules, &mut tracks);
        Self {
            turn: 1,
            world,
            routes: Routes::new(),
            tracks,
            reloads: Reloads::default(),
            zones: Vec::new(),
            scores: BTreeMap::new(),
            result: None,
        }
    }
}

/// A player's order. Group orders keep the units' relative layout around the waypoints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Order {
    Move {
        units: Vec<UnitId>,
        waypoints: Vec<Vec2>,
    },
    /// Cancels a route that continues from an earlier turn.
    Stop { units: Vec<UnitId> },
    /// Switches radars/jammers on or off (takes effect from the first tick).
    SetEmitter { units: Vec<UnitId>, on: bool },
}

impl Order {
    pub fn units(&self) -> &[UnitId] {
        match self {
            Order::Move { units, .. } | Order::Stop { units } | Order::SetEmitter { units, .. } => {
                units
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RejectReason {
    KbgExceeded,
    NotOwner,
    DuplicateUnit,
    NoUnits,
    NotEmitter,
    Move(MoveError),
}

impl fmt::Display for RejectReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RejectReason::KbgExceeded => write!(f, "kbg budget exceeded"),
            RejectReason::NotOwner => write!(f, "unit is not yours"),
            RejectReason::DuplicateUnit => write!(f, "more than one order for this unit"),
            RejectReason::NoUnits => write!(f, "order has no units"),
            RejectReason::NotEmitter => write!(f, "unit has no radar or jammer"),
            RejectReason::Move(e) => write!(f, "{e}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rejection {
    pub order: Order,
    pub reason: RejectReason,
}

/// Per unit: `Some(route)` = new route, `None` = stop.
pub type RouteChanges = BTreeMap<UnitId, Option<Vec<Vec2>>>;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Validated {
    pub changes: RouteChanges,
    /// Emitter switches: unit -> on.
    pub emitters: BTreeMap<UnitId, bool>,
    pub rejected: Vec<Rejection>,
    pub kbg_spent: u32,
}

/// KBG cost of an order for `n` units: `order_cost` per started `units_per_point` units.
pub fn order_cost(rules: &Rules, n: usize) -> u32 {
    let per = rules.kbg.units_per_point.max(1) as usize;
    n.div_ceil(per) as u32 * rules.kbg.order_cost
}

/// Routes for a group: each unit keeps its offset from the group's centre; if the offset
/// point is unreachable the unit goes to the exact waypoints instead.
pub fn group_routes(
    world: &World,
    catalog: &UnitCatalog,
    units: &[UnitId],
    waypoints: &[Vec2],
) -> Result<Vec<(UnitId, Vec<Vec2>)>, MoveError> {
    let mut positions = Vec::with_capacity(units.len());
    for &id in units {
        positions.push((id, world.unit(id).ok_or(MoveError::UnknownUnit(id))?.pos));
    }
    let n = positions.len().max(1) as f64;
    let (sx, sy) = positions
        .iter()
        .fold((0.0, 0.0), |(x, y), (_, p)| (x + p.x, y + p.y));
    let centre = Vec2::new(sx / n, sy / n);
    let mut out = Vec::with_capacity(positions.len());
    for (unit, pos) in positions {
        let (ox, oy) = (pos.x - centre.x, pos.y - centre.y);
        let shifted = MoveOrder {
            unit,
            waypoints: waypoints
                .iter()
                .map(|w| Vec2::new(w.x + ox, w.y + oy))
                .collect(),
        };
        let exact = MoveOrder {
            unit,
            waypoints: waypoints.to_vec(),
        };
        let route =
            plan_route(world, catalog, &shifted).or_else(|_| plan_route(world, catalog, &exact))?;
        out.push((unit, route));
    }
    Ok(out)
}

/// Validates one player's orders in submission order. An order that would exceed the KBG
/// budget is rejected; later cheaper orders still count.
pub fn validate_orders(
    game: &Game,
    catalog: &UnitCatalog,
    rules: &Rules,
    player: PlayerId,
    orders: &[Order],
) -> Validated {
    let mut v = Validated::default();
    // Movement (move/stop) and emitter switches are independent: one of each per unit.
    let mut seen = BTreeSet::new();
    for order in orders {
        let units = order.units();
        let kind = matches!(order, Order::SetEmitter { .. });
        let cost = order_cost(rules, units.len());
        let unique: BTreeSet<_> = units.iter().collect();
        let reason = if units.is_empty() {
            Some(RejectReason::NoUnits)
        } else if let Some(&id) = units.iter().find(|&&id| game.world.unit(id).is_none()) {
            Some(RejectReason::Move(MoveError::UnknownUnit(id)))
        } else if units
            .iter()
            .any(|&id| game.world.unit(id).is_some_and(|u| u.owner != player))
        {
            Some(RejectReason::NotOwner)
        } else if unique.len() != units.len() || units.iter().any(|&id| seen.contains(&(id, kind)))
        {
            Some(RejectReason::DuplicateUnit)
        } else if v.kbg_spent + cost > rules.kbg.budget_per_turn {
            Some(RejectReason::KbgExceeded)
        } else {
            match order {
                Order::Move { units, waypoints } => {
                    match group_routes(&game.world, catalog, units, waypoints) {
                        Ok(routes) => {
                            v.changes
                                .extend(routes.into_iter().map(|(id, r)| (id, Some(r))));
                            None
                        }
                        Err(e) => Some(RejectReason::Move(e)),
                    }
                }
                Order::Stop { units } => {
                    v.changes.extend(units.iter().map(|&id| (id, None)));
                    None
                }
                Order::SetEmitter { units, on } => {
                    let emitter = |id: &UnitId| {
                        game.world
                            .unit(*id)
                            .and_then(|u| catalog.get(&u.type_id))
                            .is_some_and(|t| t.is_emitter())
                    };
                    if units.iter().all(emitter) {
                        v.emitters.extend(units.iter().map(|&id| (id, *on)));
                        None
                    } else {
                        Some(RejectReason::NotEmitter)
                    }
                }
            }
        };
        match reason {
            Some(reason) => v.rejected.push(Rejection {
                order: order.clone(),
                reason,
            }),
            None => {
                seen.extend(units.iter().map(|&id| (id, kind)));
                v.kbg_spent += cost;
            }
        }
    }
    v
}

/// Validates every player's orders and applies the accepted route changes.
pub fn plan_phase(
    game: &mut Game,
    catalog: &UnitCatalog,
    rules: &Rules,
    orders_by_player: &BTreeMap<PlayerId, Vec<Order>>,
) -> BTreeMap<PlayerId, Vec<Rejection>> {
    let mut all_rejected = BTreeMap::new();
    for (&player, orders) in orders_by_player {
        let v = validate_orders(game, catalog, rules, player, orders);
        for (id, change) in v.changes {
            match change {
                Some(route) => game.routes.insert(id, route),
                None => game.routes.remove(&id),
            };
        }
        for (id, on) in v.emitters {
            if let Some(u) = game.world.unit_mut(id) {
                u.emitting = on;
            }
        }
        all_rejected.insert(player, v.rejected);
    }
    all_rejected
}

/// One tick: movement -> sensors/tracks -> fire + damage. Track events are listed before
/// movement events of the same tick so that filtering uses this tick's visibility.
pub fn step_tick(game: &mut Game, catalog: &UnitCatalog, rules: &Rules) -> Vec<Event> {
    game.world.tick += 1;
    let before: BTreeMap<UnitId, _> = game.world.units().iter().map(|u| (u.id, u.pos)).collect();
    step_movement(&mut game.world, catalog, rules, &mut game.routes);
    let dt = rules.turn.game_seconds_per_tick;
    for u in game.world.units_mut() {
        u.speed_mps = before.get(&u.id).map_or(0.0, |p| p.distance(u.pos) / dt);
    }

    let mut events = Vec::new();
    for c in update_tracks(&game.world, catalog, rules, &mut game.tracks) {
        let Some(track) = game.tracks.get(c.player, c.unit) else {
            continue;
        };
        events.push(if c.new_quality > 0 {
            Event::TrackGained {
                player: c.player,
                unit: c.unit,
                quality: c.new_quality,
                pos: track.last_known_pos,
                bearing: track.bearing,
            }
        } else {
            Event::TrackLost {
                player: c.player,
                unit: c.unit,
                last_known: track.last_known_pos,
            }
        });
    }
    for u in game.world.units() {
        if before.get(&u.id) != Some(&u.pos) {
            events.push(Event::UnitMoved {
                unit: u.id,
                pos: u.pos,
                heading_rad: u.heading_rad,
            });
        }
    }
    events.extend(step_combat(
        &mut game.world,
        catalog,
        rules,
        &game.tracks,
        &mut game.reloads,
    ));
    events.extend(step_capture(game, catalog, rules));
    events
}

/// Runs the 200-tick resolution phase. Event ticks are 1..=200 within this turn.
pub fn resolve_phase(game: &mut Game, catalog: &UnitCatalog, rules: &Rules) -> Vec<TickEvent> {
    let mut out = Vec::new();
    for tick in 1..=TICKS_PER_TURN {
        out.extend(
            step_tick(game, catalog, rules)
                .into_iter()
                .map(|event| TickEvent { tick, event }),
        );
    }
    end_of_turn(game, catalog, rules);
    game.turn += 1;
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::data::{load_rules, load_units};
    use crate::world::{HeightMap, Unit, UnitState};

    pub fn rules() -> Rules {
        load_rules(include_str!("../../data/rules.toml")).unwrap()
    }

    pub fn catalog() -> UnitCatalog {
        load_units(include_str!("../../data/units.toml")).unwrap()
    }

    pub fn unit(id: u32, owner: u8, type_id: &str, x: f64, y: f64) -> Unit {
        Unit {
            id: UnitId(id),
            owner: PlayerId(owner),
            type_id: type_id.into(),
            pos: Vec2::new(x, y),
            heading_rad: 0.0,
            hp: 100.0,
            state: UnitState::Healthy,
            fuel: 1e6,
            ammo: 1000,
            ..Default::default()
        }
    }

    pub fn game(map: HeightMap, units: Vec<Unit>) -> Game {
        let mut w = World::new(map);
        for u in units {
            w.add_unit(u).unwrap();
        }
        Game::new(w, &catalog(), &rules())
    }

    fn flat() -> HeightMap {
        HeightMap::new(200, 200, 50.0, vec![0.0; 40_000]).unwrap()
    }

    fn mv(unit: u32, x: f64, y: f64) -> Order {
        Order::Move {
            units: vec![UnitId(unit)],
            waypoints: vec![Vec2::new(x, y)],
        }
    }

    #[test]
    fn g5_kbg_budget_exceeded_rejected() {
        let units = (1..=9)
            .map(|i| unit(i, 0, "infantry", 100.0 * i as f64, 100.0))
            .collect();
        let mut g = game(flat(), units);
        let orders = (1..=9).map(|i| mv(i, 100.0 * i as f64, 2000.0)).collect();
        let rejected = plan_phase(
            &mut g,
            &catalog(),
            &rules(),
            &BTreeMap::from([(PlayerId(0), orders)]),
        );
        let r = &rejected[&PlayerId(0)];
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].order.units(), [UnitId(9)]);
        assert_eq!(r[0].reason, RejectReason::KbgExceeded);
        assert_eq!(g.routes.len(), rules().kbg.budget_per_turn as usize);
    }

    #[test]
    fn g5_rejects_foreign_and_duplicate_orders() {
        let mut g = game(
            flat(),
            vec![
                unit(1, 0, "mbt", 100.0, 100.0),
                unit(2, 1, "mbt", 9000.0, 9000.0),
            ],
        );
        let orders = vec![mv(2, 0.0, 0.0), mv(1, 500.0, 500.0), mv(1, 600.0, 600.0)];
        let rejected = plan_phase(
            &mut g,
            &catalog(),
            &rules(),
            &BTreeMap::from([(PlayerId(0), orders)]),
        );
        let reasons: Vec<_> = rejected[&PlayerId(0)]
            .iter()
            .map(|r| r.reason.clone())
            .collect();
        assert_eq!(
            reasons,
            [RejectReason::NotOwner, RejectReason::DuplicateUnit]
        );
    }

    #[test]
    fn g5_full_turn_is_deterministic() {
        let run = || {
            let mut g = game(
                flat(),
                vec![
                    unit(1, 0, "mbt", 500.0, 500.0),
                    unit(2, 0, "infantry", 600.0, 500.0),
                    unit(3, 1, "mbt", 1800.0, 1800.0),
                    unit(4, 1, "infantry", 1800.0, 1900.0),
                ],
            );
            let orders = BTreeMap::from([
                (
                    PlayerId(0),
                    vec![mv(1, 3000.0, 3000.0), mv(2, 1500.0, 1500.0)],
                ),
                (
                    PlayerId(1),
                    vec![mv(3, 2500.0, 2500.0), mv(4, 4000.0, 4000.0)],
                ),
            ]);
            plan_phase(&mut g, &catalog(), &rules(), &orders);
            let events = resolve_phase(&mut g, &catalog(), &rules());
            (
                serde_json::to_vec(&events).unwrap(),
                serde_json::to_vec(&g).unwrap(),
            )
        };
        let (e1, g1) = run();
        let (e2, g2) = run();
        assert_eq!(e1, e2);
        assert_eq!(g1, g2);
        // The scenario actually contains a fight.
        let text = String::from_utf8(e1).unwrap();
        assert!(text.contains("ShotFired") && text.contains("UnitMoved"));
    }

    #[test]
    fn g5_game_state_round_trips_through_json() {
        let g = game(
            flat(),
            vec![
                unit(1, 0, "mbt", 500.0, 500.0),
                unit(2, 1, "mbt", 1500.0, 500.0),
            ],
        );
        let mut back: Game = serde_json::from_slice(&serde_json::to_vec(&g).unwrap()).unwrap();
        back.world.map = g.world.map.clone(); // the map is not part of saves
        assert_eq!(back, g);
        assert!(!back.tracks.tracks.is_empty());
    }
}

#[cfg(test)]
mod group_tests {
    use super::tests::{catalog, game, rules, unit};
    use super::*;
    use crate::world::HeightMap;

    fn flat() -> HeightMap {
        HeightMap::new(200, 200, 50.0, vec![0.0; 40_000]).unwrap()
    }

    #[test]
    fn f1_1_group_order_costs_one_point_per_12_units() {
        let r = rules();
        assert_eq!(order_cost(&r, 1), 1);
        assert_eq!(order_cost(&r, 12), 1);
        assert_eq!(order_cost(&r, 13), 2);
        let units = (1..=13)
            .map(|i| unit(i, 0, "infantry", 1000.0 + 50.0 * i as f64, 1000.0))
            .collect();
        let g = game(flat(), units);
        let order = Order::Move {
            units: (1..=13).map(UnitId).collect(),
            waypoints: vec![Vec2::new(1350.0, 3000.0)],
        };
        let v = validate_orders(&g, &catalog(), &r, PlayerId(0), &[order]);
        assert!(v.rejected.is_empty());
        assert_eq!(v.kbg_spent, 2);
        assert_eq!(v.changes.len(), 13);
    }

    #[test]
    fn f1_1_group_keeps_formation() {
        let g = game(
            flat(),
            vec![
                unit(1, 0, "mbt", 1000.0, 1000.0),
                unit(2, 0, "mbt", 1200.0, 1000.0),
            ],
        );
        let routes = group_routes(
            &g.world,
            &catalog(),
            &[UnitId(1), UnitId(2)],
            &[Vec2::new(5100.0, 5000.0)],
        )
        .unwrap();
        assert_eq!(routes[0].1.last(), Some(&Vec2::new(5000.0, 5000.0)));
        assert_eq!(routes[1].1.last(), Some(&Vec2::new(5200.0, 5000.0)));
    }

    #[test]
    fn f1_1_move_and_emitter_orders_are_independent() {
        let g = game(flat(), vec![unit(1, 0, "gsr", 1000.0, 1000.0)]);
        let orders = [
            Order::Move {
                units: vec![UnitId(1)],
                waypoints: vec![Vec2::new(3000.0, 1000.0)],
            },
            Order::SetEmitter {
                units: vec![UnitId(1)],
                on: true,
            },
        ];
        let v = validate_orders(&g, &catalog(), &rules(), PlayerId(0), &orders);
        assert!(v.rejected.is_empty(), "{:?}", v.rejected);
    }

    #[test]
    fn f1_1_stop_cancels_route_and_duplicates_rejected() {
        let mut g = game(
            flat(),
            vec![
                unit(1, 0, "mbt", 1000.0, 1000.0),
                unit(2, 0, "mbt", 1200.0, 1000.0),
            ],
        );
        let go = Order::Move {
            units: vec![UnitId(1), UnitId(2)],
            waypoints: vec![Vec2::new(5000.0, 5000.0)],
        };
        plan_phase(
            &mut g,
            &catalog(),
            &rules(),
            &BTreeMap::from([(PlayerId(0), vec![go])]),
        );
        assert_eq!(g.routes.len(), 2);
        let stop = Order::Stop {
            units: vec![UnitId(1)],
        };
        let dup = Order::Stop {
            units: vec![UnitId(1), UnitId(2)],
        };
        let rejected = plan_phase(
            &mut g,
            &catalog(),
            &rules(),
            &BTreeMap::from([(PlayerId(0), vec![stop, dup])]),
        );
        assert_eq!(
            rejected[&PlayerId(0)][0].reason,
            RejectReason::DuplicateUnit
        );
        assert!(!g.routes.contains_key(&UnitId(1)));
        assert!(g.routes.contains_key(&UnitId(2)));
    }
}
