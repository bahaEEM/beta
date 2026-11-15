// SPDX-License-Identifier: AGPL-3.0-or-later
//! K-185…K-188: minefields and anti-tank ditches laid, dug and cleared by engineers.

use crate::data::{Rules, UnitCatalog, UnitClass};
use crate::event::Event;
use crate::turn::Game;
use crate::world::{HeightMap, PlayerId, Terrain, UnitId, UnitState, Vec2};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ObstacleKind {
    /// Anti-tank mines in a circle around `pos`; `charges` blasts are left.
    Mines { radius_m: f64, charges: u32 },
    /// A ditch from `pos` to `to` that no ground vehicle crosses.
    Ditch { to: Vec2 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Obstacle {
    pub id: u32,
    pub owner: PlayerId,
    pub pos: Vec2,
    pub kind: ObstacleKind,
    /// Engineer turns still needed; it only works at 0.
    pub work_left: u32,
    /// Players who know about it. Ditches are plain to see and ignore this.
    pub known_to: BTreeSet<PlayerId>,
}

impl Obstacle {
    pub fn finished(&self) -> bool {
        self.work_left == 0
    }

    /// True if `player` may be told about it (K-050).
    pub fn visible_to(&self, player: PlayerId) -> bool {
        matches!(self.kind, ObstacleKind::Ditch { .. }) || self.known_to.contains(&player)
    }

    /// Distance from `p` to the obstacle: to the edge of a minefield (0 inside), to the
    /// line of a ditch.
    pub fn distance(&self, p: Vec2) -> f64 {
        match self.kind {
            ObstacleKind::Mines { radius_m, .. } => (self.pos.distance(p) - radius_m).max(0.0),
            ObstacleKind::Ditch { to } => segment_distance(self.pos, to, p),
        }
    }
}

fn segment_distance(a: Vec2, b: Vec2, p: Vec2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    p.distance(Vec2::new(a.x + dx * t, a.y + dy * t))
}

/// What an engineer is told to make (K-185, K-187).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Works {
    Mines,
    /// A ditch from the engineer towards this point (the length is fixed by the rules).
    Ditch {
        towards: Vec2,
    },
}

/// Price of the works.
pub fn works_cost(rules: &Rules, works: Works) -> u32 {
    match works {
        Works::Mines => rules.obstacles.mine_cost,
        Works::Ditch { .. } => rules.obstacles.ditch_cost,
    }
}

/// True if `unit` is a sapper of `player` standing on land, and (for a ditch) the
/// direction is usable.
pub fn can_lay(
    game: &Game,
    catalog: &UnitCatalog,
    player: PlayerId,
    unit: UnitId,
    works: Works,
) -> bool {
    game.world
        .unit(unit)
        .filter(|u| u.owner == player && u.active())
        .is_some_and(|u| {
            catalog.get(&u.type_id).is_some_and(|t| t.sapper)
                && game.world.map.terrain_at(u.pos) != Terrain::Water
                && match works {
                    Works::Mines => true,
                    Works::Ditch { towards } => {
                        let d = towards.distance(u.pos);
                        d.is_finite() && d > 0.0
                    }
                }
        })
}

/// Marks out the obstacle where `unit` stands; it needs engineer turns before it works.
pub fn lay(game: &mut Game, rules: &Rules, unit: UnitId, works: Works) {
    let Some((owner, pos)) = game.world.unit(unit).map(|u| (u.owner, u.pos)) else {
        return;
    };
    let o = &rules.obstacles;
    let (kind, work_left) = match works {
        Works::Mines => (
            ObstacleKind::Mines {
                radius_m: o.mine_radius_m,
                charges: o.mine_charges,
            },
            o.mine_turns,
        ),
        Works::Ditch { towards } => {
            let d = towards.distance(pos);
            if !(d.is_finite() && d > 0.0) {
                return;
            }
            let f = o.ditch_length_m / d;
            let to = Vec2::new(
                pos.x + (towards.x - pos.x) * f,
                pos.y + (towards.y - pos.y) * f,
            );
            (ObstacleKind::Ditch { to }, o.ditch_turns)
        }
    };
    let id = game.obstacles.iter().map(|o| o.id).max().unwrap_or(0) + 1;
    game.obstacles.push(Obstacle {
        id,
        owner,
        pos,
        kind,
        work_left,
        known_to: BTreeSet::from([owner]),
    });
    game.routes.remove(&unit);
}

/// Map samples closed to ground vehicles by the finished ditches (K-187): every sample
/// within three quarters of a cell of a ditch line, so that no diagonal step slips through.
pub fn blocked_cells(map: &HeightMap, obstacles: &[Obstacle]) -> BTreeSet<(usize, usize)> {
    let cell = map.cell_size_m();
    let reach = cell * 0.75;
    let mut out = BTreeSet::new();
    for o in obstacles.iter().filter(|o| o.finished()) {
        let ObstacleKind::Ditch { to } = o.kind else {
            continue;
        };
        let lo = |a: f64, b: f64| (((a.min(b) - reach) / cell).floor().max(0.0)) as usize;
        let hi = |a: f64, b: f64, n: usize| {
            ((((a.max(b) + reach) / cell).ceil()).max(0.0) as usize).min(n.saturating_sub(1))
        };
        for j in lo(o.pos.y, to.y)..=hi(o.pos.y, to.y, map.height()) {
            for i in lo(o.pos.x, to.x)..=hi(o.pos.x, to.x, map.width()) {
                let p = Vec2::new(i as f64 * cell, j as f64 * cell);
                if segment_distance(o.pos, to, p) <= reach {
                    out.insert((i, j));
                }
            }
        }
    }
    out
}

/// Every tick, after movement: scouts spot minefields (K-186) and enemy vehicles driving
/// in one set a mine off (K-185).
pub fn step_mines(game: &mut Game, catalog: &UnitCatalog, rules: &Rules) -> Vec<Event> {
    let mut events = Vec::new();
    let o = &rules.obstacles;
    for field in &mut game.obstacles {
        let ObstacleKind::Mines { radius_m, charges } = &mut field.kind else {
            continue;
        };
        if field.work_left > 0 {
            continue;
        }
        let near: Vec<(UnitId, PlayerId, f64, bool, bool)> = game
            .world
            .units()
            .iter()
            .filter(|u| u.owner != field.owner && u.active())
            .filter_map(|u| {
                let t = catalog.get(&u.type_id)?;
                let d = u.pos.distance(field.pos);
                (d <= *radius_m + o.mine_spot_range_m).then_some((
                    u.id,
                    u.owner,
                    d,
                    t.class == UnitClass::Infantry || t.mine_spotter,
                    t.class == UnitClass::Vehicle,
                ))
            })
            .collect();
        for (id, owner, d, spotter, vehicle) in near {
            if spotter {
                field.known_to.insert(owner);
            }
            if !(vehicle && d <= *radius_m && *charges > 0 && game.routes.contains_key(&id)) {
                continue;
            }
            *charges -= 1;
            field.known_to.insert(owner);
            game.routes.remove(&id);
            let Some((u, t)) = game
                .world
                .unit(id)
                .and_then(|u| catalog.get(&u.type_id))
                .and_then(|t| game.world.unit_mut(id).map(|u| (u, t)))
            else {
                continue;
            };
            events.push(Event::ShellImpact {
                pos: u.pos,
                radius_m: o.mine_blast_m,
            });
            u.hp = (u.hp - o.mine_damage_hp).max(0.0);
            if u.hp <= 0.0 {
                u.state = UnitState::Destroyed;
                events.push(Event::UnitDestroyed { unit: id });
            } else {
                if u.hp <= t.hp * rules.combat.damaged_hp_fraction {
                    u.state = UnitState::Damaged;
                }
                events.push(Event::UnitDamaged {
                    unit: id,
                    hp: u.hp,
                    state: u.state,
                });
            }
        }
    }
    game.obstacles
        .retain(|f| !matches!(f.kind, ObstacleKind::Mines { charges: 0, .. }));
    events
}

/// End of turn: idle sappers next to unfinished works carry on with them (K-185, K-187),
/// sappers ordered to clear remove the nearest obstacle in reach (K-188). Adds the ones
/// that worked to `busy`.
pub fn step_works(
    game: &mut Game,
    catalog: &UnitCatalog,
    rules: &Rules,
    busy: &mut BTreeSet<UnitId>,
) {
    let reach = rules.engineer.build_radius_m;
    let sappers: Vec<(UnitId, PlayerId, Vec2, bool)> = game
        .world
        .units()
        .iter()
        .filter(|u| u.active() && catalog.get(&u.type_id).is_some_and(|t| t.sapper))
        .map(|u| (u.id, u.owner, u.pos, u.clearing))
        .collect();
    for ob in game.obstacles.iter_mut().filter(|o| !o.finished()) {
        let worker = sappers.iter().find(|&&(id, owner, pos, clearing)| {
            owner == ob.owner && !clearing && !busy.contains(&id) && pos.distance(ob.pos) <= reach
        });
        if let Some(&(id, ..)) = worker {
            busy.insert(id);
            ob.work_left -= 1;
        }
    }
    for &(id, owner, pos, clearing) in &sappers {
        if !clearing || busy.contains(&id) {
            continue;
        }
        // Nearest obstacle in reach that this side knows of (ties: lowest id).
        let target = game
            .obstacles
            .iter()
            .filter(|o| o.visible_to(owner) && o.distance(pos) <= reach)
            .min_by(|a, b| {
                a.distance(pos)
                    .total_cmp(&b.distance(pos))
                    .then(a.id.cmp(&b.id))
            })
            .map(|o| o.id);
        if let Some(target) = target {
            busy.insert(id);
            game.obstacles.retain(|o| o.id != target);
        }
    }
    for u in game.world.units_mut() {
        u.clearing = false;
    }
    game.world.blocked = blocked_cells(&game.world.map, &game.obstacles);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logistics::end_of_turn;
    use crate::movement::{plan_route, MoveOrder};
    use crate::turn::tests::{catalog, game, rules, unit};
    use crate::turn::{plan_phase, step_tick, Order, RejectReason};
    use crate::world::HeightMap;
    use std::collections::BTreeMap;

    fn flat() -> HeightMap {
        HeightMap::new(400, 200, 50.0, vec![0.0; 80_000]).unwrap()
    }

    fn plan(g: &mut Game, player: u8, orders: Vec<Order>) -> Vec<RejectReason> {
        plan_phase(
            g,
            &catalog(),
            &rules(),
            &BTreeMap::from([(PlayerId(player), orders)]),
        )
        .remove(&PlayerId(player))
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.reason)
        .collect()
    }

    fn go(unit: u32, x: f64, y: f64) -> Order {
        Order::Move {
            units: vec![UnitId(unit)],
            waypoints: vec![Vec2::new(x, y)],
        }
    }

    fn lay_order(unit: u32, works: Works) -> Order {
        Order::Lay {
            units: vec![UnitId(unit)],
            works,
        }
    }

    /// Nobody shoots: these tests are about the obstacles.
    fn hold(mut g: Game) -> Game {
        for u in g.world.units_mut() {
            u.stance = crate::world::Stance::Hold;
        }
        g
    }

    /// A finished blue minefield at (5000, 5000).
    fn mined(mut units: Vec<crate::world::Unit>) -> Game {
        units.push(unit(1, 0, "engineer", 5000.0, 5000.0));
        let mut g = hold(game(flat(), units));
        g.money.insert(PlayerId(0), 1000);
        assert!(plan(&mut g, 0, vec![lay_order(1, Works::Mines)]).is_empty());
        end_of_turn(&mut g, &catalog(), &rules());
        g
    }

    #[test]
    fn k185_mines_hurt_and_stop_enemy_vehicles_only_and_run_out() {
        let (cat, r) = (catalog(), rules());
        let o = &r.obstacles;
        // The order costs money and needs a sapper.
        let mut g = game(
            flat(),
            vec![
                unit(1, 0, "engineer", 5000.0, 5000.0),
                unit(2, 0, "mbt", 5000.0, 5100.0),
            ],
        );
        g.money.insert(PlayerId(0), o.mine_cost);
        let orders = vec![lay_order(2, Works::Mines), lay_order(1, Works::Mines)];
        assert_eq!(plan(&mut g, 0, orders), [RejectReason::CannotBuild]);
        assert_eq!(g.money[&PlayerId(0)], 0);
        assert!(!g.obstacles[0].finished());
        for _ in 0..o.mine_turns {
            end_of_turn(&mut g, &cat, &r);
        }
        assert!(g.obstacles[0].finished());

        // A red tank and red infantry drive in from the east; a blue tank crosses too.
        let mut g = mined(vec![
            unit(3, 1, "mbt", 5400.0, 5000.0),
            unit(4, 1, "infantry", 5200.0, 5050.0),
            unit(5, 0, "mbt", 4600.0, 5000.0),
        ]);
        plan(
            &mut g,
            1,
            vec![go(3, 4500.0, 5000.0), go(4, 4900.0, 5050.0)],
        );
        plan(&mut g, 0, vec![go(5, 5400.0, 5000.0)]);
        let mut blasts = 0;
        for _ in 0..200 {
            let ev = step_tick(&mut g, &cat, &r);
            blasts += ev
                .iter()
                .filter(|e| matches!(e, Event::ShellImpact { .. }))
                .count();
        }
        let u = |id: u32| g.world.unit(UnitId(id)).unwrap().clone();
        assert_eq!(blasts, 1);
        assert_eq!(u(3).hp, 100.0 - o.mine_damage_hp);
        let d = u(3).pos.distance(Vec2::new(5000.0, 5000.0));
        assert!(
            d <= o.mine_radius_m && d > o.mine_radius_m - 20.0,
            "stopped at the edge"
        );
        assert!(!g.routes.contains_key(&UnitId(3)));
        assert_eq!(u(4).hp, 100.0, "infantry walks through");
        assert_eq!(
            (u(5).hp, u(5).pos.x),
            (100.0, 5400.0),
            "own vehicles know the lanes"
        );
        // Driving on sets off the rest; then the field is gone.
        for _ in 1..o.mine_charges {
            plan(&mut g, 1, vec![go(3, 4500.0, 5000.0)]);
            step_tick(&mut g, &cat, &r);
        }
        assert!(g.obstacles.is_empty());
    }

    #[test]
    fn k186_minefields_are_hidden_until_scouted_or_struck() {
        let (cat, r) = (catalog(), rules());
        let edge = 5000.0 + r.obstacles.mine_radius_m + r.obstacles.mine_spot_range_m;
        let mut g = mined(vec![
            unit(3, 1, "mbt", edge - 50.0, 5000.0), // tanks see nothing
            unit(4, 1, "infantry", edge + 50.0, 5000.0), // too far
        ]);
        let known = |g: &Game| g.obstacles[0].visible_to(PlayerId(1));
        step_tick(&mut g, &cat, &r);
        assert!(g.obstacles[0].visible_to(PlayerId(0)) && !known(&g));
        g.world.unit_mut(UnitId(4)).unwrap().pos.x = edge - 10.0;
        step_tick(&mut g, &cat, &r);
        assert!(known(&g), "infantry close by finds it");
        // A scout car spots as well; a struck mine gives the field away too.
        for kind in ["recon", "engineer", "mbt"] {
            let mut g = mined(vec![unit(3, 1, kind, edge - 10.0, 5000.0)]);
            step_tick(&mut g, &cat, &r);
            assert_eq!(g.obstacles[0].visible_to(PlayerId(1)), kind != "mbt");
        }
        let mut g = mined(vec![unit(3, 1, "mbt", 5100.0, 5000.0)]);
        plan(&mut g, 1, vec![go(3, 5600.0, 5000.0)]);
        step_tick(&mut g, &cat, &r);
        assert!(g.obstacles[0].visible_to(PlayerId(1)));
    }

    #[test]
    fn k187_no_vehicle_crosses_a_finished_ditch() {
        let (cat, r) = (catalog(), rules());
        let o = &r.obstacles;
        // The engineer digs north to south across the tank's way west.
        let mut g = hold(game(
            flat(),
            vec![
                unit(1, 0, "engineer", 5000.0, 5000.0),
                unit(2, 0, "mbt", 5300.0, 5150.0),
                unit(3, 1, "infantry", 5300.0, 5100.0),
            ],
        ));
        g.money.insert(PlayerId(0), o.ditch_cost);
        let dig = lay_order(
            1,
            Works::Ditch {
                towards: Vec2::new(5000.0, 9000.0),
            },
        );
        assert!(plan(&mut g, 0, vec![dig]).is_empty());
        assert_eq!(
            g.obstacles[0].kind,
            ObstacleKind::Ditch {
                to: Vec2::new(5000.0, 5000.0 + o.ditch_length_m)
            }
        );
        assert!(g.obstacles[0].visible_to(PlayerId(1)), "plain to see");
        let route = |g: &Game, id: u32| {
            let order = MoveOrder {
                unit: UnitId(id),
                waypoints: vec![Vec2::new(4700.0, 5150.0)],
            };
            plan_route(&g.world, &cat, &order).unwrap()
        };
        let crosses = |route: &[Vec2]| {
            let ob = Obstacle {
                id: 0,
                owner: PlayerId(0),
                pos: Vec2::new(5000.0, 5000.0),
                kind: ObstacleKind::Ditch {
                    to: Vec2::new(5000.0, 5000.0 + o.ditch_length_m),
                },
                work_left: 0,
                known_to: BTreeSet::new(),
            };
            route.iter().any(|&p| ob.distance(p) < 30.0)
        };
        // Unfinished: no obstacle yet. A tank already under way is caught by it later.
        assert!(crosses(&route(&g, 2)));
        g.routes.insert(UnitId(2), route(&g, 2));
        for _ in 0..o.ditch_turns {
            assert!(g.world.blocked.is_empty());
            end_of_turn(&mut g, &cat, &r);
        }
        assert!(!g.world.blocked.is_empty());
        for _ in 0..200 {
            step_tick(&mut g, &cat, &r);
        }
        let tank = g.world.unit(UnitId(2)).unwrap();
        assert!(
            tank.pos.x > 5000.0 && !g.routes.contains_key(&UnitId(2)),
            "halted in front of the ditch"
        );
        // New routes go around it, also for the owner; infantry climbs through.
        let around = route(&g, 2);
        assert!(!crosses(&around));
        assert!(around
            .iter()
            .any(|p| p.y < 5000.0 || p.y > 5000.0 + o.ditch_length_m));
        assert!(crosses(&route(&g, 3)));
        // The engineer standing in its own ditch can still drive out.
        let out = MoveOrder {
            unit: UnitId(1),
            waypoints: vec![Vec2::new(5500.0, 5000.0)],
        };
        assert!(plan_route(&g.world, &cat, &out).is_ok());
    }

    #[test]
    fn k188_engineers_clear_known_minefields_and_fill_ditches() {
        let (cat, r) = (catalog(), rules());
        let clear = |id: u32| Order::Clear {
            units: vec![UnitId(id)],
        };
        let edge = 5000.0 + r.obstacles.mine_radius_m;
        // A red engineer at the edge of the blue field; a second field it has not found.
        let mut g = mined(vec![
            unit(3, 1, "engineer", edge + 50.0, 5000.0),
            unit(4, 1, "mbt", 9000.0, 9000.0),
        ]);
        assert_eq!(plan(&mut g, 1, vec![clear(4)]), [RejectReason::CannotBuild]);
        // Not found yet: nothing to clear.
        plan(&mut g, 1, vec![clear(3)]);
        end_of_turn(&mut g, &cat, &r);
        assert_eq!(g.obstacles.len(), 1);
        step_tick(&mut g, &cat, &r); // now it spots the field
        end_of_turn(&mut g, &cat, &r);
        assert_eq!(g.obstacles.len(), 1, "no order, no clearing");
        plan(&mut g, 1, vec![clear(3)]);
        end_of_turn(&mut g, &cat, &r);
        assert!(g.obstacles.is_empty());

        // Ditches: anyone's engineer fills one, and the way is open again.
        let mut g = game(
            flat(),
            vec![
                unit(1, 0, "engineer", 5000.0, 5000.0),
                unit(3, 1, "engineer", 5050.0, 5100.0),
            ],
        );
        g.money.insert(PlayerId(0), 1000);
        let towards = Vec2::new(5000.0, 9000.0);
        plan(&mut g, 0, vec![lay_order(1, Works::Ditch { towards })]);
        for _ in 0..r.obstacles.ditch_turns {
            end_of_turn(&mut g, &cat, &r);
        }
        assert!(!g.world.blocked.is_empty());
        plan(&mut g, 1, vec![clear(3)]);
        end_of_turn(&mut g, &cat, &r);
        assert!(g.obstacles.is_empty() && g.world.blocked.is_empty());
    }
}
