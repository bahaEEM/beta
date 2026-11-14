// SPDX-License-Identifier: AGPL-3.0-or-later
//! F2-3: indirect fire (artillery, rockets). Fire missions at a ground point or at a tracked
//! enemy; shells fly for distance / speed and hit everything within their radius.

use crate::combat::{penetration_factor, CombatState};
use crate::data::{Rules, UnitCatalog};
use crate::event::Event;
use crate::sensor::TrackTable;
use crate::world::{PlayerId, UnitId, UnitState, Vec2, World};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum FireTarget {
    Point(Vec2),
    /// Aim at the track's current position; needs track quality >= `indirect.min_track_quality`.
    Unit(UnitId),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shell {
    pub owner: PlayerId,
    pub impact_tick: u32,
    pub pos: Vec2,
    pub radius_m: f64,
    pub damage_hp: f64,
    pub penetration_mm: f64,
}

/// Launches shells for active fire missions (after direct fire in the tick order).
pub fn step_launch(
    world: &mut World,
    catalog: &UnitCatalog,
    rules: &Rules,
    tracks: &TrackTable,
    missions: &BTreeMap<UnitId, FireTarget>,
    combat: &mut CombatState,
    shells: &mut Vec<Shell>,
) -> Vec<Event> {
    let mut events = Vec::new();
    let tick = world.tick;
    for (&id, &target) in missions {
        let Some(u) = world.unit(id) else { continue };
        let Some(t) = catalog.get(&u.type_id) else {
            continue;
        };
        let Some(gun) = t.indirect else { continue };
        if u.state == UnitState::Destroyed || u.ammo == 0 || !u.deployed() {
            combat.lay.remove(&id);
            continue;
        }
        let aim_point = match target {
            FireTarget::Point(p) => Some(p),
            FireTarget::Unit(enemy) => tracks
                .get(u.owner, enemy)
                .filter(|tr| tr.quality >= rules.indirect.min_track_quality)
                .map(|tr| tr.last_known_pos),
        };
        let d = aim_point.map(|p| u.pos.distance(p));
        let Some(point) =
            aim_point.filter(|_| d.is_some_and(|d| d >= gun.min_range_m && d <= gun.range_m))
        else {
            combat.lay.remove(&id);
            continue;
        };
        // Laying the gun takes aim_s once per mission; reload between rounds.
        let held = combat.lay.get(&id).map_or(1, |&n| n + 1);
        combat.lay.insert(id, held);
        if held < rules.aim_ticks(t) || combat.reloads.contains_key(&id) {
            continue;
        }
        let flight = rules.ticks(d.unwrap_or(0.0) / gun.shell_speed_mps, 1);
        shells.push(Shell {
            owner: u.owner,
            impact_tick: tick + flight,
            pos: point,
            radius_m: gun.radius_m,
            damage_hp: gun.damage_hp,
            penetration_mm: gun.penetration_mm,
        });
        combat.reloads.insert(id, rules.reload_ticks(t));
        events.push(Event::ShellFired { shooter: id });
        if let Some(u) = world.unit_mut(id) {
            u.ammo = u.ammo.saturating_sub(1);
        }
    }
    events
}

/// Shells landing this tick: damage falls linearly to zero at the radius; armour (top armour
/// = rear plate) reduces it like direct fire. Hits friend and foe alike.
pub fn step_impacts(
    world: &mut World,
    catalog: &UnitCatalog,
    rules: &Rules,
    shells: &mut Vec<Shell>,
) -> Vec<Event> {
    let tick = world.tick;
    let (landing, flying): (Vec<Shell>, Vec<Shell>) =
        shells.drain(..).partition(|s| s.impact_tick <= tick);
    *shells = flying;
    let mut events = Vec::new();
    for s in landing {
        events.push(Event::ShellImpact {
            pos: s.pos,
            radius_m: s.radius_m,
        });
        let hits: Vec<(UnitId, f64, f64)> = world
            .units()
            .iter()
            .filter(|u| u.active())
            .filter_map(|u| {
                let t = catalog.get(&u.type_id).filter(|t| !t.class.is_air())?; // shells do not hit aircraft
                let d = u.pos.distance(s.pos);
                (d < s.radius_m).then(|| {
                    let dmg = s.damage_hp
                        * (1.0 - d / s.radius_m)
                        * penetration_factor(rules, s.penetration_mm, t.armor_rear_mm);
                    (u.id, dmg, t.hp * rules.combat.damaged_hp_fraction)
                })
            })
            .collect();
        for (id, dmg, damaged_at) in hits {
            let Some(u) = world.unit_mut(id) else {
                continue;
            };
            u.under_fire_tick = Some(tick);
            if dmg <= 0.0 || u.state == UnitState::Destroyed {
                continue;
            }
            u.hp = (u.hp - dmg).max(0.0);
            if u.hp <= 0.0 {
                u.state = UnitState::Destroyed;
                events.push(Event::UnitDestroyed { unit: id });
            } else {
                if u.hp <= damaged_at {
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
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::turn::tests::{catalog, game, rules, unit};
    use crate::turn::{plan_phase, step_tick, validate_orders, Order, RejectReason};
    use crate::world::HeightMap;

    fn flat() -> HeightMap {
        HeightMap::new(800, 200, 50.0, vec![0.0; 160_000]).unwrap()
    }

    fn fire(g: &mut crate::turn::Game, ids: &[u32], target: FireTarget) {
        let owner = g.world.unit(UnitId(ids[0])).unwrap().owner;
        let order = Order::FireMission {
            units: ids.iter().map(|&i| UnitId(i)).collect(),
            target,
        };
        let r = plan_phase(
            g,
            &catalog(),
            &rules(),
            &BTreeMap::from([(owner, vec![order])]),
        );
        assert!(r[&owner].is_empty(), "{:?}", r[&owner]);
    }

    fn run(g: &mut crate::turn::Game, ticks: u32) -> Vec<(u32, Event)> {
        let mut out = Vec::new();
        for _ in 0..ticks {
            let ev = step_tick(g, &catalog(), &rules());
            let t = g.world.tick;
            out.extend(ev.into_iter().map(|e| (t, e)));
        }
        out
    }

    #[test]
    fn k090_shell_lands_after_flight_time_and_hurts_everyone_in_radius() {
        let (cat, r) = (catalog(), rules());
        let spg = cat.get("spg").unwrap().clone();
        let gun = spg.indirect.unwrap();
        // Target 12 km away; an enemy and a friendly infantry right at the aim point.
        let mut g = game(
            flat(),
            vec![
                unit(1, 0, "spg", 1000.0, 5000.0),
                unit(2, 1, "infantry", 13000.0, 5000.0),
                unit(3, 0, "infantry", 13010.0, 5000.0),
            ],
        );
        g.world.unit_mut(UnitId(2)).unwrap().stance = crate::world::Stance::Hold;
        g.world.unit_mut(UnitId(3)).unwrap().stance = crate::world::Stance::Hold;
        fire(&mut g, &[1], FireTarget::Point(Vec2::new(13000.0, 5000.0)));
        // Only the first round has landed after 160 ticks (lay 100 + flight 50).
        let ev = run(&mut g, 160);
        let fired = ev
            .iter()
            .find(|(_, e)| matches!(e, Event::ShellFired { .. }))
            .unwrap()
            .0;
        let landed = ev
            .iter()
            .find(|(_, e)| matches!(e, Event::ShellImpact { .. }))
            .unwrap()
            .0;
        assert_eq!(fired, r.aim_ticks(&spg));
        assert_eq!(landed - fired, r.ticks(12000.0 / gun.shell_speed_mps, 1));
        let (enemy, friend) = (
            g.world.unit(UnitId(2)).unwrap().hp,
            g.world.unit(UnitId(3)).unwrap().hp,
        );
        assert!(enemy < 100.0 && friend < 100.0, "friendly fire too");
        assert!(enemy < friend, "damage falls with distance from the impact");
    }

    #[test]
    fn k091_no_fire_outside_min_or_max_range() {
        let cat = catalog();
        let gun = cat.get("spg").unwrap().indirect.unwrap();
        for d in [gun.min_range_m - 100.0, gun.range_m + 100.0] {
            let mut g = game(flat(), vec![unit(1, 0, "spg", 1000.0, 5000.0)]);
            fire(
                &mut g,
                &[1],
                FireTarget::Point(Vec2::new(1000.0 + d, 5000.0)),
            );
            assert!(!run(&mut g, 100)
                .iter()
                .any(|(_, e)| matches!(e, Event::ShellFired { .. })));
        }
    }

    #[test]
    fn k092_unit_mission_needs_track_quality_2() {
        // Enemy tank 10 km away, nobody sees it: no fire. With an exact track: fire.
        let mut g = game(
            flat(),
            vec![
                unit(1, 0, "spg", 1000.0, 5000.0),
                unit(2, 1, "mbt", 11000.0, 5000.0),
            ],
        );
        g.world.unit_mut(UnitId(2)).unwrap().stance = crate::world::Stance::Hold; // keep the spotter alive
        fire(&mut g, &[1], FireTarget::Unit(UnitId(2)));
        assert!(!run(&mut g, 60)
            .iter()
            .any(|(_, e)| matches!(e, Event::ShellFired { .. })));
        g.world
            .add_unit(unit(3, 0, "infantry", 10500.0, 5000.0))
            .unwrap(); // a spotter
        assert!(run(&mut g, 150)
            .iter()
            .any(|(_, e)| matches!(e, Event::ShellFired { .. })));
    }

    #[test]
    fn k093_armour_shrugs_off_high_explosive() {
        let mut g = game(
            flat(),
            vec![
                unit(1, 0, "spg", 1000.0, 5000.0),
                unit(2, 1, "mbt", 13000.0, 5000.0),
                unit(3, 1, "supply", 13000.0, 5020.0),
            ],
        );
        for id in [2, 3] {
            g.world.unit_mut(UnitId(id)).unwrap().stance = crate::world::Stance::Hold;
        }
        fire(&mut g, &[1], FireTarget::Point(Vec2::new(13000.0, 5010.0)));
        run(&mut g, 200);
        let tank_loss = 100.0 - g.world.unit(UnitId(2)).unwrap().hp;
        let truck_loss = 100.0 - g.world.unit(UnitId(3)).unwrap().hp;
        assert!(tank_loss < truck_loss);
    }

    #[test]
    fn k094_fire_mission_needs_indirect_weapon_and_ends_with_the_turn() {
        let g = game(flat(), vec![unit(1, 0, "mbt", 1000.0, 5000.0)]);
        let order = Order::FireMission {
            units: vec![UnitId(1)],
            target: FireTarget::Point(Vec2::new(9000.0, 5000.0)),
        };
        assert_eq!(
            validate_orders(&g, &catalog(), &rules(), PlayerId(0), &[order]).rejected[0].reason,
            RejectReason::NoIndirectWeapon
        );
        let mut g = game(flat(), vec![unit(1, 0, "spg", 1000.0, 5000.0)]);
        fire(&mut g, &[1], FireTarget::Point(Vec2::new(13000.0, 5000.0)));
        crate::turn::resolve_phase(&mut g, &catalog(), &rules());
        assert!(g.missions.is_empty());
    }
}
