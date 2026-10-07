// SPDX-License-Identifier: AGPL-3.0-or-later
//! G2: path finding and movement.

use crate::data::UnitCatalog;
use crate::world::{HeightMap, UnitId, UnitState, Vec2, World};
use crate::TICK_SECONDS;
use pathfinding::prelude::astar;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MoveOrder {
    pub unit: UnitId,
    pub waypoints: Vec<Vec2>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MoveError {
    UnknownUnit(UnitId),
    UnknownType(String),
    UnitDestroyed(UnitId),
    /// Waypoint `index` has no passable path, lies outside the map, or the unit cannot move.
    Unreachable {
        index: usize,
    },
}

impl fmt::Display for MoveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MoveError::UnknownUnit(id) => write!(f, "unknown unit {}", id.0),
            MoveError::UnknownType(t) => write!(f, "unknown unit type '{t}'"),
            MoveError::UnitDestroyed(id) => write!(f, "unit {} is destroyed", id.0),
            MoveError::Unreachable { index } => write!(f, "waypoint {index} is unreachable"),
        }
    }
}

impl std::error::Error for MoveError {}

/// Remaining route points per unit, consumed front to back by `step_movement`.
pub type Routes = BTreeMap<UnitId, Vec<Vec2>>;

type Node = (usize, usize);

/// Fixed neighbour order; ties in A* resolve the same way every run.
const NEIGHBOURS: [(isize, isize); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

/// Plans a route through all waypoints in order. The result excludes the unit's start
/// position and ends exactly at the last waypoint.
pub fn plan_route(
    world: &World,
    catalog: &UnitCatalog,
    order: &MoveOrder,
) -> Result<Vec<Vec2>, MoveError> {
    let unit = world
        .unit(order.unit)
        .ok_or(MoveError::UnknownUnit(order.unit))?;
    if unit.state == UnitState::Destroyed {
        return Err(MoveError::UnitDestroyed(unit.id));
    }
    let ut = catalog
        .get(&unit.type_id)
        .ok_or_else(|| MoveError::UnknownType(unit.type_id.clone()))?;
    let map = &world.map;
    let cell = map.cell_size_m();
    let speed = ut.speed_offroad_mps;
    // Integer edge costs (ms) keep A* tie-breaking exact.
    let straight_ms = (cell / speed * 1000.0).round() as u64;
    let diag_ms = (cell * std::f64::consts::SQRT_2 / speed * 1000.0).round() as u64;

    let mut route = Vec::new();
    let mut start = snap(map, unit.pos).ok_or(MoveError::Unreachable { index: 0 })?;
    for (index, &wp) in order.waypoints.iter().enumerate() {
        let goal = snap(map, wp).ok_or(MoveError::Unreachable { index })?;
        if !(speed.is_finite() && speed > 0.0) {
            return Err(MoveError::Unreachable { index });
        }
        let (path, _) = astar(
            &start,
            |&n| successors(map, n, ut.max_slope_deg, straight_ms, diag_ms),
            |&(i, j)| {
                // Octile distance with the same integer costs: admissible and consistent.
                let (dx, dy) = (i.abs_diff(goal.0) as u64, j.abs_diff(goal.1) as u64);
                dx.min(dy) * diag_ms + dx.abs_diff(dy) * straight_ms
            },
            |&n| n == goal,
        )
        .ok_or(MoveError::Unreachable { index })?;
        // Drop the start sample; replace the goal sample with the exact waypoint.
        let inner = path.len().saturating_sub(1);
        route.extend(
            path.iter()
                .skip(1)
                .take(inner.saturating_sub(1))
                .map(|&(i, j)| Vec2::new(i as f64 * cell, j as f64 * cell)),
        );
        route.push(wp);
        start = goal;
    }
    Ok(route)
}

/// Nearest sample, or None if the point is not finite or outside the map.
fn snap(map: &HeightMap, p: Vec2) -> Option<Node> {
    let cell = map.cell_size_m();
    let (gx, gy) = ((p.x / cell).round(), (p.y / cell).round());
    let inside = gx >= 0.0 && gy >= 0.0 && gx < map.width() as f64 && gy < map.height() as f64;
    inside.then_some((gx as usize, gy as usize))
}

fn successors(
    map: &HeightMap,
    (i, j): Node,
    max_slope_deg: f64,
    straight_ms: u64,
    diag_ms: u64,
) -> Vec<(Node, u64)> {
    let cell = map.cell_size_m();
    let h0 = map.sample(i, j);
    NEIGHBOURS
        .iter()
        .filter_map(|&(dx, dy)| {
            let ni = i.checked_add_signed(dx).filter(|&v| v < map.width())?;
            let nj = j.checked_add_signed(dy).filter(|&v| v < map.height())?;
            let diag = dx != 0 && dy != 0;
            let horiz = if diag {
                cell * std::f64::consts::SQRT_2
            } else {
                cell
            };
            let slope = ((map.sample(ni, nj) - h0).abs() / horiz)
                .atan()
                .to_degrees();
            (slope <= max_slope_deg).then_some(((ni, nj), if diag { diag_ms } else { straight_ms }))
        })
        .collect()
}

/// Advances every routed unit one tick along its route (id order). Finished routes and
/// routes of destroyed, missing or untyped units are removed.
pub fn step_movement(world: &mut World, catalog: &UnitCatalog, routes: &mut Routes) {
    for unit in world.units_mut() {
        let Some(route) = routes.get_mut(&unit.id) else {
            continue;
        };
        let speed = match catalog.get(&unit.type_id) {
            Some(t) if unit.state != UnitState::Destroyed => t.speed_offroad_mps,
            _ => {
                route.clear();
                continue;
            }
        };
        let mut budget = speed * TICK_SECONDS;
        while budget > 0.0 {
            let Some(&target) = route.first() else { break };
            let (dx, dy) = (target.x - unit.pos.x, target.y - unit.pos.y);
            let d = dx.hypot(dy);
            if d > 0.0 {
                unit.heading_rad = dy.atan2(dx);
            }
            if d <= budget {
                unit.pos = target;
                budget -= d;
                route.remove(0);
            } else {
                let f = budget / d;
                unit.pos = Vec2::new(unit.pos.x + dx * f, unit.pos.y + dy * f);
                budget = 0.0;
            }
        }
    }
    routes.retain(|id, r| !r.is_empty() && world.unit(*id).is_some());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::load_units;
    use crate::world::{PlayerId, Unit};

    fn catalog() -> UnitCatalog {
        load_units(include_str!("../../data/units.toml")).unwrap()
    }

    fn world(map: HeightMap, units: &[(u32, Vec2)]) -> World {
        let mut w = World::new(map);
        for &(id, pos) in units {
            w.add_unit(Unit {
                id: UnitId(id),
                owner: PlayerId(0),
                type_id: "mbt".into(),
                pos,
                heading_rad: 0.0,
                hp: 100.0,
                state: UnitState::Healthy,
            })
            .unwrap();
        }
        w
    }

    fn flat(n: usize, cell: f64) -> HeightMap {
        HeightMap::new(n, n, cell, vec![0.0; n * n]).unwrap()
    }

    #[test]
    fn g2_flat_map_straight_line_and_timing() {
        let cat = catalog();
        let speed = cat.get("mbt").unwrap().speed_offroad_mps;
        let mut w = world(flat(11, 10.0), &[(1, Vec2::new(0.0, 0.0))]);
        let goal = Vec2::new(100.0, 0.0);
        let order = MoveOrder {
            unit: UnitId(1),
            waypoints: vec![goal],
        };
        let route = plan_route(&w, &cat, &order).unwrap();
        let expected: Vec<Vec2> = (1..=10).map(|i| Vec2::new(i as f64 * 10.0, 0.0)).collect();
        assert_eq!(route, expected);

        let ticks = (100.0 / (speed * TICK_SECONDS)).round() as u32;
        let mut routes = Routes::from([(UnitId(1), route)]);
        for _ in 0..ticks - 1 {
            step_movement(&mut w, &cat, &mut routes);
        }
        assert!(routes.contains_key(&UnitId(1)));
        step_movement(&mut w, &cat, &mut routes);
        assert!(routes.is_empty());
        let u = w.unit(UnitId(1)).unwrap();
        assert!(u.pos.distance(goal) < 1e-9);
        assert_eq!(u.heading_rad, 0.0);
    }

    #[test]
    fn g2_flat_map_diagonal_is_octile_shortest() {
        let cat = catalog();
        let w = world(flat(11, 10.0), &[(1, Vec2::new(0.0, 0.0))]);
        let order = MoveOrder {
            unit: UnitId(1),
            waypoints: vec![Vec2::new(100.0, 50.0)],
        };
        let route = plan_route(&w, &cat, &order).unwrap();
        let mut len = 0.0;
        let mut prev = Vec2::new(0.0, 0.0);
        for p in &route {
            len += prev.distance(*p);
            prev = *p;
        }
        let expected = 5.0 * 10.0 * std::f64::consts::SQRT_2 + 5.0 * 10.0;
        assert!((len - expected).abs() < 1e-9, "len {len}");
    }

    /// Ridge of height 100 m at column 10 (cell 10 m), optional gap at row 15.
    fn ridge(gap: bool) -> HeightMap {
        let n = 21;
        let mut h = vec![0.0f32; n * n];
        for j in 0..n {
            if !(gap && j == 15) {
                h[j * n + 10] = 100.0;
            }
        }
        HeightMap::new(n, n, 10.0, h).unwrap()
    }

    #[test]
    fn g2_steep_ridge_routes_through_gap() {
        let cat = catalog();
        let max_slope = cat.get("mbt").unwrap().max_slope_deg;
        let w = world(ridge(true), &[(1, Vec2::new(0.0, 50.0))]);
        let order = MoveOrder {
            unit: UnitId(1),
            waypoints: vec![Vec2::new(200.0, 50.0)],
        };
        let route = plan_route(&w, &cat, &order).unwrap();
        assert!(route.contains(&Vec2::new(100.0, 150.0)));
        let mut prev = Vec2::new(0.0, 50.0);
        for &p in &route {
            assert!(prev.distance(p) <= 10.0 * std::f64::consts::SQRT_2 + 1e-9);
            let dh = (w.map.height_at(p) - w.map.height_at(prev)).abs();
            assert!((dh / prev.distance(p)).atan().to_degrees() <= max_slope);
            prev = p;
        }

        let blocked = world(ridge(false), &[(1, Vec2::new(0.0, 50.0))]);
        assert_eq!(
            plan_route(&blocked, &cat, &order),
            Err(MoveError::Unreachable { index: 0 })
        );
    }

    #[test]
    fn g2_errors() {
        let cat = catalog();
        let mut w = world(flat(5, 10.0), &[(1, Vec2::default())]);
        let order = |id| MoveOrder {
            unit: UnitId(id),
            waypoints: vec![Vec2::new(20.0, 20.0)],
        };
        assert_eq!(
            plan_route(&w, &cat, &order(9)),
            Err(MoveError::UnknownUnit(UnitId(9)))
        );
        w.unit_mut(UnitId(1)).unwrap().type_id = "nope".into();
        assert_eq!(
            plan_route(&w, &cat, &order(1)),
            Err(MoveError::UnknownType("nope".into()))
        );
        w.unit_mut(UnitId(1)).unwrap().state = UnitState::Destroyed;
        assert_eq!(
            plan_route(&w, &cat, &order(1)),
            Err(MoveError::UnitDestroyed(UnitId(1)))
        );
    }

    #[test]
    fn g2_200_ticks_bit_identical() {
        let cat = catalog();
        let run = || {
            let mut w = world(
                ridge(true),
                &[(1, Vec2::new(3.0, 47.0)), (2, Vec2::new(190.0, 10.0))],
            );
            let mut routes = Routes::new();
            for (id, wps) in [
                (1, vec![Vec2::new(200.0, 50.0), Vec2::new(150.0, 200.0)]),
                (2, vec![Vec2::new(5.0, 195.0)]),
            ] {
                let order = MoveOrder {
                    unit: UnitId(id),
                    waypoints: wps,
                };
                routes.insert(UnitId(id), plan_route(&w, &cat, &order).unwrap());
            }
            for _ in 0..crate::TICKS_PER_TURN {
                step_movement(&mut w, &cat, &mut routes);
            }
            w.units()
                .iter()
                .map(|u| {
                    (
                        u.pos.x.to_bits(),
                        u.pos.y.to_bits(),
                        u.heading_rad.to_bits(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(run(), run());
    }
}
