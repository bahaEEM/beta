// SPDX-License-Identifier: AGPL-3.0-or-later
//! F2-2: command network. A unit can receive orders only if it is linked to its side's HQ,
//! by radio (relayed through other units) or by wire along the road network.

use crate::data::{Rules, UnitCatalog, UnitType};
use crate::world::{HeightMap, PlayerId, Unit, UnitId, UnitState, World};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Road network component per sample (0 = no road). 8-neighbour flood fill in index order.
pub fn road_components(map: &HeightMap) -> Vec<u32> {
    let (w, h) = (map.width(), map.height());
    let mut comp = vec![0u32; w * h];
    let mut next = 0u32;
    for start in 0..w * h {
        if comp[start] != 0 || map.road(start % w, start / w) == 0 {
            continue;
        }
        next += 1;
        comp[start] = next;
        let mut queue = VecDeque::from([start]);
        while let Some(k) = queue.pop_front() {
            let (i, j) = (k % w, k / w);
            for (dx, dy) in [
                (-1, -1),
                (0, -1),
                (1, -1),
                (-1, 0),
                (1, 0),
                (-1, 1),
                (0, 1),
                (1, 1),
            ] {
                let (Some(ni), Some(nj)) = (i.checked_add_signed(dx), j.checked_add_signed(dy))
                else {
                    continue;
                };
                if ni >= w || nj >= h {
                    continue;
                }
                let n = nj * w + ni;
                if comp[n] == 0 && map.road(ni, nj) > 0 {
                    comp[n] = next;
                    queue.push_back(n);
                }
            }
        }
    }
    comp
}

/// Wire network a unit is plugged into: component of the nearest road sample within reach.
/// Aircraft are never on the wire.
fn wire_component(map: &HeightMap, comps: &[u32], u: &Unit, t: &UnitType, reach_m: f64) -> u32 {
    if t.class.is_air() || t.class.is_naval() {
        return 0;
    }
    let cell = map.cell_size_m();
    let r = (reach_m / cell).ceil() as i64;
    let (ci, cj) = (
        (u.pos.x / cell).round() as i64,
        (u.pos.y / cell).round() as i64,
    );
    let mut best: Option<(f64, u32)> = None;
    for j in (cj - r)..=(cj + r) {
        for i in (ci - r)..=(ci + r) {
            if i < 0 || j < 0 || i as usize >= map.width() || j as usize >= map.height() {
                continue;
            }
            let c = comps
                .get(j as usize * map.width() + i as usize)
                .copied()
                .unwrap_or(0);
            let d = (i as f64 * cell - u.pos.x).hypot(j as f64 * cell - u.pos.y);
            if c != 0 && d <= reach_m && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, c));
            }
        }
    }
    best.map_or(0, |(_, c)| c)
}

/// Living units of `player` plugged into the same road wire as one of its living HQs:
/// they get their orders by wire and do not transmit (K-150).
pub fn wired_to_hq(
    world: &World,
    catalog: &UnitCatalog,
    rules: &Rules,
    player: PlayerId,
) -> BTreeSet<UnitId> {
    let comps = road_components(&world.map);
    let wires: Vec<(UnitId, bool, u32)> = world
        .units()
        .iter()
        .filter(|u| u.owner == player && u.state != UnitState::Destroyed)
        .filter_map(|u| {
            let t = catalog.get(&u.type_id)?;
            let wire = wire_component(&world.map, &comps, u, t, rules.comms.wire_reach_m);
            (wire != 0).then_some((u.id, t.hq, wire))
        })
        .collect();
    let hq_wires: BTreeSet<u32> = wires.iter().filter(|w| w.1).map(|w| w.2).collect();
    wires
        .iter()
        .filter(|w| hq_wires.contains(&w.2))
        .map(|w| w.0)
        .collect()
}

/// Players that started with an HQ; the command-network rule applies only to them.
pub fn players_with_hq(world: &World, catalog: &UnitCatalog) -> BTreeSet<PlayerId> {
    world
        .units()
        .iter()
        .filter(|u| catalog.get(&u.type_id).is_some_and(|t| t.hq))
        .map(|u| u.owner)
        .collect()
}

/// For each player in `players`: its living units linked to a living HQ right now.
pub fn connected_units(
    world: &World,
    catalog: &UnitCatalog,
    rules: &Rules,
    players: &BTreeSet<PlayerId>,
) -> BTreeMap<PlayerId, BTreeSet<UnitId>> {
    let comps = road_components(&world.map);
    let alive: Vec<(&Unit, &UnitType)> = world
        .units()
        .iter()
        .filter(|u| u.state != UnitState::Destroyed)
        .filter_map(|u| catalog.get(&u.type_id).map(|t| (u, t)))
        .collect();
    let radio_jammers: Vec<(PlayerId, crate::world::Vec2, f64)> = alive
        .iter()
        .filter(|(u, _)| u.radiating())
        .filter_map(|(u, t)| t.radio_jam_range_m().map(|r| (u.owner, u.pos, r)))
        .collect();
    let mut out = BTreeMap::new();
    for &player in players {
        let nodes: Vec<(&Unit, &UnitType, bool, u32)> = alive
            .iter()
            .filter(|(u, _)| u.owner == player)
            .map(|&(u, t)| {
                // K-151: a silent unit is off the radio net.
                let radio = !u.silent
                    && !radio_jammers
                        .iter()
                        .any(|&(o, pos, r)| o != player && pos.distance(u.pos) <= r);
                (
                    u,
                    t,
                    radio,
                    wire_component(&world.map, &comps, u, t, rules.comms.wire_reach_m),
                )
            })
            .collect();
        let linked = |a: usize, b: usize| {
            let ((ua, ta, ra, wa), (ub, tb, rb, wb)) = (nodes[a], nodes[b]);
            (ra && rb && ua.pos.distance(ub.pos) <= ta.radio_range_m.min(tb.radio_range_m))
                || (wa != 0 && wa == wb)
        };
        let mut seen = vec![false; nodes.len()];
        let mut queue: VecDeque<usize> = VecDeque::new();
        for (k, n) in nodes.iter().enumerate() {
            if n.1.hq {
                seen[k] = true;
                queue.push_back(k);
            }
        }
        while let Some(a) = queue.pop_front() {
            for (b, s) in seen.iter_mut().enumerate() {
                if !*s && linked(a, b) {
                    *s = true;
                    queue.push_back(b);
                }
            }
        }
        let set = nodes
            .iter()
            .zip(&seen)
            .filter(|(_, &s)| s)
            .map(|(n, _)| n.0.id)
            .collect();
        out.insert(player, set);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::turn::tests::{catalog, game, rules, unit};
    use crate::turn::{plan_phase, validate_orders, Order, RejectReason};
    use crate::world::Vec2;

    fn flat_with_road() -> HeightMap {
        // 1000 x 100 samples at 50 m = 50 x 5 km; a road along row 2 from x = 0 to 40 km.
        let (w, h) = (1000, 100);
        let mut roads = vec![0u8; w * h];
        roads[2 * w..2 * w + 800].fill(2);
        HeightMap::new(w, h, 50.0, vec![0.0; w * h])
            .unwrap()
            .with_roads(roads)
            .unwrap()
    }

    fn conn(g: &crate::turn::Game) -> BTreeSet<UnitId> {
        connected_units(
            &g.world,
            &catalog(),
            &rules(),
            &BTreeSet::from([PlayerId(0)]),
        )[&PlayerId(0)]
            .clone()
    }

    #[test]
    fn k080_radio_links_relay_through_units() {
        // HQ at 0, tank at 9 km (in HQ and tank reach), infantry at 13 km (only via the tank: 4 km),
        // infantry at 30 km: nobody reaches it (y = 4000 keeps them off the road).
        let g = game(
            flat_with_road(),
            vec![
                unit(1, 0, "hq", 0.0, 4000.0),
                unit(2, 0, "mbt", 9000.0, 4000.0),
                unit(3, 0, "infantry", 13000.0, 4000.0),
                unit(4, 0, "infantry", 30000.0, 4000.0),
            ],
        );
        assert_eq!(conn(&g), BTreeSet::from([UnitId(1), UnitId(2), UnitId(3)]));
    }

    #[test]
    fn k081_enemy_jammer_cuts_radio_but_not_wire() {
        let mut g = game(
            flat_with_road(),
            vec![
                unit(1, 0, "hq", 0.0, 100.0),           // on the road
                unit(2, 0, "mbt", 8000.0, 4000.0),      // radio only
                unit(3, 0, "infantry", 30000.0, 100.0), // on the road, far away
                unit(9, 1, "jammer", 8000.0, 4500.0),
            ],
        );
        assert_eq!(
            conn(&g),
            BTreeSet::from([UnitId(1), UnitId(2), UnitId(3)]),
            "wire reaches the far unit"
        );
        g.world.unit_mut(UnitId(9)).unwrap().emitting = true;
        assert_eq!(
            conn(&g),
            BTreeSet::from([UnitId(1), UnitId(3)]),
            "jammed radio, wire still works"
        );
    }

    #[test]
    fn k082_no_hq_no_command() {
        let mut g = game(
            flat_with_road(),
            vec![
                unit(1, 0, "hq", 0.0, 4000.0),
                unit(2, 0, "mbt", 1000.0, 4000.0),
            ],
        );
        g.world.unit_mut(UnitId(1)).unwrap().state = UnitState::Destroyed;
        assert!(conn(&g).is_empty());
    }

    #[test]
    fn k083_cut_off_units_reject_new_orders_but_keep_moving() {
        let mut g = game(
            flat_with_road(),
            vec![
                unit(1, 0, "hq", 0.0, 4000.0),
                unit(2, 0, "mbt", 5000.0, 4000.0),
                unit(9, 1, "jammer", 5000.0, 4500.0),
            ],
        );
        let go = Order::Move {
            units: vec![UnitId(2)],
            waypoints: vec![Vec2::new(20000.0, 4000.0)],
        };
        plan_phase(
            &mut g,
            &catalog(),
            &rules(),
            &BTreeMap::from([(PlayerId(0), vec![go])]),
        );
        assert!(g.routes.contains_key(&UnitId(2)));
        g.world.unit_mut(UnitId(9)).unwrap().emitting = true;
        g.connected = connected_units(&g.world, &catalog(), &rules(), &g.hq_players);
        let stop = Order::Stop {
            units: vec![UnitId(2)],
        };
        let v = validate_orders(&g, &catalog(), &rules(), PlayerId(0), &[stop]);
        assert_eq!(v.rejected[0].reason, RejectReason::NoComms);
        assert!(g.routes.contains_key(&UnitId(2)), "keeps its old route");
    }

    #[test]
    fn k084_sides_without_hq_are_not_restricted() {
        let g = game(flat_with_road(), vec![unit(2, 0, "mbt", 5000.0, 4000.0)]);
        let go = Order::Move {
            units: vec![UnitId(2)],
            waypoints: vec![Vec2::new(9000.0, 4000.0)],
        };
        assert!(
            validate_orders(&g, &catalog(), &rules(), PlayerId(0), &[go])
                .rejected
                .is_empty()
        );
    }
}
