// SPDX-License-Identifier: AGPL-3.0-or-later
//! G5: turn engine. Planning validates orders (KBG budget); resolution runs 200 ticks.

use crate::combat::{step_combat, Reloads};
use crate::data::{Rules, UnitCatalog};
use crate::event::{Event, TickEvent};
use crate::movement::{plan_route, step_movement, MoveError, MoveOrder, Routes};
use crate::sensor::{update_tracks, TrackTable};
use crate::world::{PlayerId, UnitId, World};
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
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RejectReason {
    KbgExceeded,
    NotOwner,
    DuplicateUnit,
    Move(MoveError),
}

impl fmt::Display for RejectReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RejectReason::KbgExceeded => write!(f, "kbg budget exceeded"),
            RejectReason::NotOwner => write!(f, "unit is not yours"),
            RejectReason::DuplicateUnit => write!(f, "more than one order for this unit"),
            RejectReason::Move(e) => write!(f, "{e}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rejection {
    pub order: MoveOrder,
    pub reason: RejectReason,
}

/// Validates one player's orders in submission order. Accepted orders come back as routes;
/// an order that would exceed the KBG budget is rejected (later cheaper orders still count).
pub fn validate_orders(
    game: &Game,
    catalog: &UnitCatalog,
    rules: &Rules,
    player: PlayerId,
    orders: &[MoveOrder],
) -> (Routes, Vec<Rejection>) {
    let mut routes = Routes::new();
    let mut rejected = Vec::new();
    let mut seen = BTreeSet::new();
    let mut spent = 0u32;
    for order in orders {
        let reason = match game.world.unit(order.unit) {
            Some(u) if u.owner != player => Some(RejectReason::NotOwner),
            _ if !seen.insert(order.unit) => Some(RejectReason::DuplicateUnit),
            _ if spent + rules.kbg.move_order_cost > rules.kbg.budget_per_turn => {
                Some(RejectReason::KbgExceeded)
            }
            _ => match plan_route(&game.world, catalog, order) {
                Ok(route) => {
                    routes.insert(order.unit, route);
                    spent += rules.kbg.move_order_cost;
                    None
                }
                Err(e) => Some(RejectReason::Move(e)),
            },
        };
        if let Some(reason) = reason {
            rejected.push(Rejection {
                order: order.clone(),
                reason,
            });
        }
    }
    (routes, rejected)
}

/// Validates every player's orders and installs the accepted routes (replacing old ones).
pub fn plan_phase(
    game: &mut Game,
    catalog: &UnitCatalog,
    rules: &Rules,
    orders_by_player: &BTreeMap<PlayerId, Vec<MoveOrder>>,
) -> BTreeMap<PlayerId, Vec<Rejection>> {
    let mut all_rejected = BTreeMap::new();
    for (&player, orders) in orders_by_player {
        let (routes, rejected) = validate_orders(game, catalog, rules, player, orders);
        game.routes.extend(routes);
        all_rejected.insert(player, rejected);
    }
    all_rejected
}

/// One tick: movement -> sensors/tracks -> fire + damage. Track events are listed before
/// movement events of the same tick so that filtering uses this tick's visibility.
pub fn step_tick(game: &mut Game, catalog: &UnitCatalog, rules: &Rules) -> Vec<Event> {
    game.world.tick += 1;
    let before: BTreeMap<UnitId, _> = game.world.units().iter().map(|u| (u.id, u.pos)).collect();
    step_movement(&mut game.world, catalog, rules, &mut game.routes);

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
    game.turn += 1;
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::data::{load_rules, load_units};
    use crate::world::{HeightMap, Unit, UnitState, Vec2};

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

    fn mv(unit: u32, x: f64, y: f64) -> MoveOrder {
        MoveOrder {
            unit: UnitId(unit),
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
        assert_eq!(r[0].order.unit, UnitId(9));
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
        let back: Game = serde_json::from_slice(&serde_json::to_vec(&g).unwrap()).unwrap();
        assert_eq!(back, g);
        assert!(!back.tracks.tracks.is_empty());
    }
}
