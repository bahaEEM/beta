// SPDX-License-Identifier: AGPL-3.0-or-later
//! Regions (capture by infantry, bases, victory), supply and the end-of-turn step.

use crate::data::{Rules, UnitCatalog, UnitClass};
use crate::event::Event;
use crate::turn::Game;
use crate::world::{PlayerId, UnitId, UnitState, Vec2};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A named region (K-063), captured by holding the settlement at its centre (K-060).
/// Owner and captures are public (map flags); progress is not sent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Zone {
    pub id: u32,
    pub name_tr: String,
    pub center: Vec2,
    pub radius_m: f64,
    /// Supply units of the owner refill here.
    pub depot: bool,
    pub owner: Option<PlayerId>,
    /// Home base of this player: whoever else takes it wins the match (K-061).
    #[serde(default)]
    pub base: Option<PlayerId>,
    /// Player capturing and ticks spent so far.
    #[serde(default)]
    pub progress: Option<(PlayerId, u32)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchResult {
    /// None = draw.
    pub winner: Option<PlayerId>,
}

/// Capture step (last in the tick order). A zone changes hands when living infantry of
/// exactly one player, other than the owner, stays inside for `capture_s`.
pub fn step_capture(game: &mut Game, catalog: &UnitCatalog, rules: &Rules) -> Vec<Event> {
    let needed = (rules.zones.capture_s / rules.turn.game_seconds_per_tick)
        .ceil()
        .max(1.0) as u32;
    let mut events = Vec::new();
    for zone in &mut game.zones {
        let present: BTreeSet<PlayerId> = game
            .world
            .units()
            .iter()
            .filter(|u| u.active())
            .filter(|u| {
                catalog
                    .get(&u.type_id)
                    .is_some_and(|t| t.class == UnitClass::Infantry)
            })
            .filter(|u| u.pos.distance(zone.center) <= zone.radius_m)
            .map(|u| u.owner)
            .collect();
        let mut only = present.iter().copied();
        match (only.next(), only.next()) {
            (Some(p), None) if zone.owner != Some(p) => {
                let ticks = match zone.progress {
                    Some((q, t)) if q == p => t + 1,
                    _ => 1,
                };
                if ticks >= needed {
                    zone.owner = Some(p);
                    zone.progress = None;
                    events.push(Event::ZoneCaptured {
                        zone: zone.id,
                        player: p,
                    });
                } else {
                    zone.progress = Some((p, ticks));
                }
            }
            (None, _) => zone.progress = None,
            _ => {} // contested or held by the owner: progress frozen
        }
    }
    events
}

/// Regions held per player (players without a region are absent).
pub fn region_counts(zones: &[Zone]) -> BTreeMap<PlayerId, u32> {
    let mut counts = BTreeMap::new();
    for owner in zones.iter().filter_map(|z| z.owner) {
        *counts.entry(owner).or_insert(0) += 1;
    }
    counts
}

/// End of turn: refill supply units at depots, resupply nearby friendly units, then
/// decide the match.
pub fn end_of_turn(game: &mut Game, catalog: &UnitCatalog, rules: &Rules) {
    let suppliers: Vec<UnitId> = game
        .world
        .units()
        .iter()
        .filter(|u| u.state != UnitState::Destroyed)
        .filter(|u| {
            catalog
                .get(&u.type_id)
                .is_some_and(|t| t.supply_fuel_l > 0.0 || t.supply_ammo > 0)
        })
        .map(|u| u.id)
        .collect();
    for sid in suppliers {
        let Some(s) = game.world.unit(sid) else {
            continue;
        };
        let (owner, pos) = (s.owner, s.pos);
        let at_depot = game
            .zones
            .iter()
            .any(|z| z.depot && z.owner == Some(owner) && pos.distance(z.center) <= z.radius_m);
        if at_depot {
            if let (Some(t), Some(s)) = (catalog.get(&s.type_id).cloned(), game.world.unit_mut(sid))
            {
                s.supply_fuel = t.supply_fuel_l;
                s.supply_ammo = t.supply_ammo;
            }
        }
        // Units in reach, in id order (the supplier itself included).
        let needs: Vec<(UnitId, f64, u32)> = game
            .world
            .units()
            .iter()
            .filter(|u| u.owner == owner && u.state != UnitState::Destroyed)
            .filter(|u| u.pos.distance(pos) <= rules.supply.radius_m)
            .filter_map(|u| {
                let t = catalog.get(&u.type_id)?;
                Some((
                    u.id,
                    (t.fuel_l - u.fuel).max(0.0),
                    t.ammo.saturating_sub(u.ammo),
                ))
            })
            .collect();
        for (uid, fuel_need, ammo_need) in needs {
            let Some(s) = game.world.unit_mut(sid) else {
                break;
            };
            let fuel = fuel_need.min(s.supply_fuel);
            let ammo = ammo_need.min(s.supply_ammo);
            s.supply_fuel -= fuel;
            s.supply_ammo -= ammo;
            if let Some(u) = game.world.unit_mut(uid) {
                u.fuel += fuel;
                u.ammo += ammo;
            }
        }
    }

    // K-061: holding an enemy base wins; after the last turn the side with more regions.
    let takers: BTreeSet<PlayerId> = game
        .zones
        .iter()
        .filter_map(|z| z.owner.filter(|&o| z.base.is_some_and(|b| b != o)))
        .collect();
    let leaders: Vec<PlayerId> = if !takers.is_empty() {
        takers.into_iter().collect()
    } else if game.turn >= rules.zones.max_turns {
        let counts = region_counts(&game.zones);
        let best = counts.values().copied().max().unwrap_or(0);
        counts
            .into_iter()
            .filter(|&(_, n)| n == best)
            .map(|(p, _)| p)
            .collect()
    } else {
        return;
    };
    game.result = Some(MatchResult {
        winner: (leaders.len() == 1).then(|| leaders[0]),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::turn::tests::{catalog, game, rules, unit};
    use crate::turn::{plan_phase, resolve_phase, step_tick, Order};
    use crate::world::HeightMap;
    use std::collections::BTreeMap;

    fn flat() -> HeightMap {
        HeightMap::new(200, 200, 50.0, vec![0.0; 40_000]).unwrap()
    }

    fn zone(id: u32, x: f64, owner: Option<u8>, depot: bool) -> Zone {
        Zone {
            id,
            name_tr: format!("Bölge {id}"),
            center: Vec2::new(x, 1000.0),
            radius_m: 300.0,
            depot,
            owner: owner.map(PlayerId),
            base: None,
            progress: None,
        }
    }

    #[test]
    fn f1_3_infantry_captures_after_capture_time() {
        let mut g = game(
            flat(),
            vec![
                unit(1, 0, "infantry", 3000.0, 1000.0),
                unit(2, 1, "mbt", 8000.0, 8000.0),
            ],
        );
        g.zones = vec![zone(1, 3000.0, Some(1), false)];
        let needed = (rules().zones.capture_s / rules().turn.game_seconds_per_tick).ceil() as u32;
        let mut captured_at = None;
        for tick in 1..=needed + 1 {
            let ev = step_tick(&mut g, &catalog(), &rules());
            if ev.iter().any(|e| {
                matches!(
                    e,
                    Event::ZoneCaptured {
                        zone: 1,
                        player: PlayerId(0)
                    }
                )
            }) {
                captured_at = Some(tick);
            }
        }
        assert_eq!(captured_at, Some(needed));
        assert_eq!(g.zones[0].owner, Some(PlayerId(0)));
    }

    #[test]
    fn f1_3_only_infantry_captures_and_contest_freezes() {
        let mut g = game(
            flat(),
            vec![
                unit(1, 0, "mbt", 3000.0, 1000.0),
                unit(2, 0, "infantry", 6000.0, 1000.0),
                unit(3, 1, "infantry", 6050.0, 1000.0),
            ],
        );
        g.zones = vec![zone(1, 3000.0, None, false), zone(2, 6000.0, None, false)];
        for _ in 0..300 {
            step_tick(&mut g, &catalog(), &rules());
        }
        assert_eq!(g.zones[0].owner, None, "tanks cannot capture");
        assert_eq!(
            g.zones[1].owner, None,
            "contested zone does not change hands"
        );
    }

    #[test]
    fn k061_taking_the_enemy_base_wins_at_the_end_of_the_turn() {
        // Blue infantry stands in the red base; red infantry is far from the blue base.
        let mut g = game(
            flat(),
            vec![
                unit(1, 0, "infantry", 6000.0, 1000.0),
                unit(2, 1, "infantry", 9000.0, 9000.0),
            ],
        );
        g.zones = vec![
            zone(1, 3000.0, Some(0), true),
            zone(2, 6000.0, Some(1), true),
            zone(3, 8000.0, Some(1), false),
        ];
        g.zones[0].base = Some(PlayerId(0));
        g.zones[1].base = Some(PlayerId(1));
        resolve_phase(&mut g, &catalog(), &rules());
        assert_eq!(g.zones[1].owner, Some(PlayerId(0)));
        // The base decides, at the end of the turn it fell in.
        assert_eq!(
            g.result,
            Some(MatchResult {
                winner: Some(PlayerId(0))
            })
        );
        // Both bases changing hands in the same turn is a draw.
        g.zones[0].owner = Some(PlayerId(1));
        g.result = None;
        end_of_turn(&mut g, &catalog(), &rules());
        assert_eq!(g.result, Some(MatchResult { winner: None }));
    }

    #[test]
    fn k061_most_regions_wins_after_the_last_turn() {
        let mut g = game(flat(), vec![unit(1, 0, "mbt", 9000.0, 9000.0)]);
        g.zones = vec![
            zone(1, 3000.0, Some(0), false),
            zone(2, 6000.0, Some(1), false),
            zone(3, 8000.0, Some(1), false),
            zone(4, 9000.0, None, false),
        ];
        g.zones[0].base = Some(PlayerId(0));
        assert_eq!(
            region_counts(&g.zones),
            BTreeMap::from([(PlayerId(0), 1), (PlayerId(1), 2)])
        );
        end_of_turn(&mut g, &catalog(), &rules());
        assert_eq!(g.result, None, "nobody lost a base and turns remain");
        g.turn = rules().zones.max_turns;
        end_of_turn(&mut g, &catalog(), &rules());
        assert_eq!(
            g.result,
            Some(MatchResult {
                winner: Some(PlayerId(1))
            })
        );
        // Equal counts: draw.
        g.zones[3].owner = Some(PlayerId(0));
        end_of_turn(&mut g, &catalog(), &rules());
        assert_eq!(g.result, Some(MatchResult { winner: None }));
    }

    #[test]
    fn f1_4_moving_uses_fuel_and_empty_tank_stops() {
        let mut g = game(flat(), vec![unit(1, 0, "mbt", 1000.0, 1000.0)]);
        g.world.unit_mut(UnitId(1)).unwrap().fuel = 1.0; // 1 l = 250 m at 4 l/km
        let go = Order::Move {
            units: vec![UnitId(1)],
            waypoints: vec![Vec2::new(5000.0, 1000.0)],
        };
        plan_phase(
            &mut g,
            &catalog(),
            &rules(),
            &BTreeMap::from([(PlayerId(0), vec![go])]),
        );
        for _ in 0..200 {
            step_tick(&mut g, &catalog(), &rules());
        }
        let u = g.world.unit(UnitId(1)).unwrap();
        assert!(
            (u.pos.x - 1250.0).abs() < 1e-6,
            "stopped after 250 m, at {}",
            u.pos.x
        );
        assert_eq!(u.fuel, 0.0);
    }

    #[test]
    fn f1_4_no_ammo_no_fire() {
        let mut g = game(
            flat(),
            vec![
                unit(1, 0, "mbt", 1000.0, 1000.0),
                unit(2, 1, "mbt", 2000.0, 1000.0),
            ],
        );
        g.world.unit_mut(UnitId(1)).unwrap().ammo = 0;
        g.world.unit_mut(UnitId(2)).unwrap().ammo = 0;
        let ev = step_tick(&mut g, &catalog(), &rules());
        assert!(!ev.iter().any(|e| matches!(e, Event::ShotFired { .. })));
    }

    #[test]
    fn f1_4_supply_refills_nearby_units_and_restocks_at_depot() {
        let cat = catalog();
        let mbt = cat.get("mbt").unwrap().clone();
        let truck = cat.get("supply").unwrap().clone();
        let mut g = game(
            flat(),
            vec![
                unit(1, 0, "mbt", 3000.0, 1000.0),
                unit(2, 0, "supply", 3100.0, 1000.0),
                unit(3, 0, "mbt", 6000.0, 1000.0),
            ],
        );
        for id in [1, 3] {
            let u = g.world.unit_mut(UnitId(id)).unwrap();
            u.fuel = 100.0;
            u.ammo = 1;
        }
        {
            let s = g.world.unit_mut(UnitId(2)).unwrap();
            s.fuel = truck.fuel_l;
            s.ammo = 0;
            s.supply_fuel = 500.0;
            s.supply_ammo = 10;
        }
        end_of_turn(&mut g, &cat, &rules());
        let (a, far) = (
            g.world.unit(UnitId(1)).unwrap(),
            g.world.unit(UnitId(3)).unwrap(),
        );
        assert_eq!(
            (a.fuel, a.ammo),
            (600.0, 11),
            "limited by the truck's stock"
        );
        assert_eq!((far.fuel, far.ammo), (100.0, 1), "out of reach");
        // At an owned depot the truck restocks to full and serves the tank completely.
        g.zones = vec![zone(1, 3050.0, Some(0), true)];
        end_of_turn(&mut g, &cat, &rules());
        let a = g.world.unit(UnitId(1)).unwrap();
        assert_eq!((a.fuel, a.ammo), (mbt.fuel_l, mbt.ammo));
    }
}

#[cfg(test)]
mod road_tests {
    use crate::movement::{plan_route, step_movement, MoveOrder, Routes};
    use crate::turn::tests::{catalog, game, rules, unit};
    use crate::world::{HeightMap, UnitId, Vec2};

    /// Flat 100x100 map (50 m cells) with a U-shaped road: down column 0, along row 0, up column 80.
    fn road_map() -> HeightMap {
        let n = 100;
        let mut roads = vec![0u8; n * n];
        roads[..=80].fill(3);
        for j in 0..=10 {
            roads[j * n] = 3;
            roads[j * n + 80] = 3;
        }
        HeightMap::new(n, n, 50.0, vec![0.0; n * n])
            .unwrap()
            .with_roads(roads)
            .unwrap()
    }

    #[test]
    fn f1_2_path_prefers_faster_road_detour() {
        let g = game(road_map(), vec![unit(1, 0, "mbt", 0.0, 500.0)]);
        let order = MoveOrder {
            unit: UnitId(1),
            waypoints: vec![Vec2::new(4000.0, 500.0)],
        };
        let route = plan_route(&g.world, &catalog(), &order).unwrap();
        // 5000 m of road at 16 m/s beats 4000 m off road at 10 m/s.
        assert!(
            route.iter().any(|p| p.y == 0.0),
            "route should use the road along row 0"
        );
    }

    #[test]
    fn f1_2_road_speed_on_road() {
        let mut g = game(road_map(), vec![unit(1, 0, "mbt", 0.0, 0.0)]);
        let mut routes = Routes::from([(UnitId(1), vec![Vec2::new(4000.0, 0.0)])]);
        step_movement(&mut g.world, &catalog(), &rules(), &mut routes);
        let t = catalog().get("mbt").unwrap().clone();
        let x = g.world.unit(UnitId(1)).unwrap().pos.x;
        assert!((x - t.speed_road_mps * rules().turn.game_seconds_per_tick).abs() < 1e-9);
    }
}
