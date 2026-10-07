// SPDX-License-Identifier: AGPL-3.0-or-later
//! G4: direct fire and damage. No dice: every shot hits.

use crate::data::UnitCatalog;
use crate::data::{Rules, UnitClass, UnitType};
use crate::event::Event;
use crate::sensor::{line_of_sight, TrackTable};
use crate::world::{Unit, UnitId, UnitState, Vec2, World};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::f64::consts::{FRAC_PI_4, PI};

/// Ticks until each unit is loaded again. Absent = loaded (units start loaded).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Reloads(pub BTreeMap<UnitId, u32>);

/// One combat tick: all shooters pick targets from the same pre-fire state,
/// then all shots are applied together (simultaneous fire).
pub fn step_combat(
    world: &mut World,
    catalog: &UnitCatalog,
    rules: &Rules,
    tracks: &TrackTable,
    reloads: &mut Reloads,
) -> Vec<Event> {
    for left in reloads.0.values_mut() {
        *left = left.saturating_sub(1);
    }
    reloads.0.retain(|_, left| *left > 0);

    let shots: Vec<(UnitId, UnitId)> = world
        .units()
        .iter()
        .filter(|s| s.state != UnitState::Destroyed && !reloads.0.contains_key(&s.id))
        .filter_map(|s| {
            let st = catalog.get(&s.type_id)?;
            pick_target(world, catalog, rules, tracks, s, st).map(|t| (s.id, t))
        })
        .collect();

    let mut events = Vec::new();
    for (shooter_id, target_id) in shots {
        let (Some(shooter), Some(target)) = (world.unit(shooter_id), world.unit(target_id)) else {
            continue;
        };
        let (Some(st), Some(tt)) = (catalog.get(&shooter.type_id), catalog.get(&target.type_id))
        else {
            continue;
        };
        reloads.0.insert(shooter_id, st.reload_ticks);
        events.push(Event::ShotFired {
            shooter: shooter_id,
            target: target_id,
        });
        if target.state == UnitState::Destroyed {
            continue; // already killed by an earlier shot this tick
        }
        let from = shooter.pos;
        let (penetration, damage_hp) = (st.penetration_mm, st.damage_hp);
        let effective = effective_armor(tt, target, from);
        let class = tt.class;
        let Some(target) = world.unit_mut(target_id) else {
            continue;
        };
        let destroyed = match class {
            UnitClass::Infantry => {
                target.hp -= damage_hp;
                target.hp <= 0.0
            }
            UnitClass::Vehicle => {
                penetration >= effective
                    || (penetration >= effective * rules.combat.damaged_threshold
                        && target.state == UnitState::Damaged)
            }
        };
        if destroyed {
            target.hp = 0.0;
            target.state = UnitState::Destroyed;
            events.push(Event::UnitDestroyed { unit: target_id });
        } else if class == UnitClass::Infantry
            || penetration >= effective * rules.combat.damaged_threshold
        {
            if class == UnitClass::Vehicle {
                target.state = UnitState::Damaged;
            }
            events.push(Event::UnitDamaged {
                unit: target_id,
                hp: target.hp,
                state: target.state,
            });
        }
    }
    events
}

/// Nearest living enemy that the shooter's player tracks at fire quality, within
/// weapon range and in the shooter's own line of sight. Ties: lowest id.
fn pick_target(
    world: &World,
    catalog: &UnitCatalog,
    rules: &Rules,
    tracks: &TrackTable,
    shooter: &Unit,
    st: &UnitType,
) -> Option<UnitId> {
    world
        .units()
        .iter()
        .filter(|t| t.owner != shooter.owner && t.state != UnitState::Destroyed)
        .filter(|t| {
            tracks
                .get(shooter.owner, t.id)
                .is_some_and(|tr| tr.quality >= rules.sensor.optical.fire_quality)
        })
        .filter_map(|t| {
            let d = shooter.pos.distance(t.pos);
            let tt = catalog.get(&t.type_id)?;
            (d <= st.weapon_range_m
                && line_of_sight(&world.map, shooter.pos, t.pos, st.height_m, tt.height_m))
            .then_some((d, t.id))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        .map(|(_, id)| id)
}

/// armour / cos(impact angle). Facing is chosen from where the shot comes from relative
/// to the target's heading: front ±45°, rear ±45°, sides in between.
pub fn effective_armor(tt: &UnitType, target: &Unit, from: Vec2) -> f64 {
    let a = (from.y - target.pos.y).atan2(from.x - target.pos.x) - target.heading_rad;
    let rel = a.sin().atan2(a.cos()).abs(); // 0..=PI, 0 = dead ahead
    let (armor, impact) = if rel <= FRAC_PI_4 {
        (tt.armor_front_mm, rel)
    } else if rel < 3.0 * FRAC_PI_4 {
        (tt.armor_side_mm, (rel - PI / 2.0).abs())
    } else {
        (tt.armor_rear_mm, PI - rel)
    };
    armor / impact.cos() // impact <= 45°, so cos >= 0.707
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{load_rules, load_units};
    use crate::sensor::Track;
    use crate::world::{HeightMap, PlayerId};

    fn rules() -> Rules {
        load_rules(include_str!("../../data/rules.toml")).unwrap()
    }

    fn catalog() -> UnitCatalog {
        load_units(include_str!("../../data/units.toml")).unwrap()
    }

    fn unit(id: u32, owner: u8, type_id: &str, x: f64, heading: f64) -> Unit {
        Unit {
            id: UnitId(id),
            owner: PlayerId(owner),
            type_id: type_id.into(),
            pos: Vec2::new(x, 500.0),
            heading_rad: heading,
            hp: 100.0,
            state: UnitState::Healthy,
        }
    }

    fn world(units: Vec<Unit>) -> World {
        let mut w = World::new(HeightMap::new(100, 100, 50.0, vec![0.0; 10_000]).unwrap());
        for u in units {
            w.add_unit(u).unwrap();
        }
        w
    }

    /// Every player tracks every enemy at `quality`.
    fn tracks(w: &World, quality: u8) -> TrackTable {
        let mut t = TrackTable::default();
        for o in w.units() {
            for e in w.units().iter().filter(|e| e.owner != o.owner) {
                t.tracks.insert(
                    (o.owner, e.id),
                    Track {
                        observer_player: o.owner,
                        target_unit: e.id,
                        quality,
                        last_seen_tick: 0,
                        last_known_pos: e.pos,
                    },
                );
            }
        }
        t
    }

    fn fight(w: &mut World, cat: &UnitCatalog, q: u8) -> Vec<Event> {
        let t = tracks(w, q);
        step_combat(w, cat, &rules(), &t, &mut Reloads::default())
    }

    #[test]
    fn g4_tanks_kill_each_other_same_tick() {
        // Facing each other 1000 m apart: front hits, 650 mm >= 600 mm.
        let mut w = world(vec![
            unit(1, 0, "mbt", 0.0, 0.0),
            unit(2, 1, "mbt", 1000.0, PI),
        ]);
        let ev = fight(&mut w, &catalog(), 3);
        assert!(w.units().iter().all(|u| u.state == UnitState::Destroyed));
        assert_eq!(
            ev.iter()
                .filter(|e| matches!(e, Event::UnitDestroyed { .. }))
                .count(),
            2
        );
    }

    #[test]
    fn g4_front_vs_side_hit() {
        let mut cat = catalog();
        if let Some(t) = cat.types.get_mut("mbt") {
            t.penetration_mm = 500.0; // front 600: 500 >= 70% -> damaged; side 80 -> destroyed
        }
        // Target 2 faces the shooter (front hit).
        let mut w = world(vec![
            unit(1, 0, "mbt", 0.0, 0.0),
            unit(2, 1, "mbt", 1000.0, PI),
        ]);
        let t = tracks(&w, 3);
        let mut reloads = Reloads::default();
        reloads.0.insert(UnitId(2), 5); // only unit 1 fires
        step_combat(&mut w, &cat, &rules(), &t, &mut reloads);
        assert_eq!(w.unit(UnitId(2)).unwrap().state, UnitState::Damaged);
        // Target 2 turned 90°: side hit.
        let mut w = world(vec![
            unit(1, 0, "mbt", 0.0, 0.0),
            unit(2, 1, "mbt", 1000.0, PI / 2.0),
        ]);
        let mut reloads = Reloads::default();
        reloads.0.insert(UnitId(2), 5);
        step_combat(&mut w, &cat, &rules(), &t, &mut reloads);
        assert_eq!(w.unit(UnitId(2)).unwrap().state, UnitState::Destroyed);
    }

    #[test]
    fn g4_effective_armor_grows_with_angle() {
        let cat = catalog();
        let tt = cat.get("mbt").unwrap();
        let target = unit(1, 0, "mbt", 0.0, 0.0);
        let straight = effective_armor(tt, &target, Vec2::new(100.0, 500.0));
        let angled = effective_armor(tt, &target, Vec2::new(100.0, 550.0));
        assert_eq!(straight, tt.armor_front_mm);
        assert!(angled > straight);
        let rear = effective_armor(tt, &target, Vec2::new(-100.0, 500.0));
        assert_eq!(rear, tt.armor_rear_mm);
    }

    #[test]
    fn g4_no_fire_below_track_quality_3() {
        let mut w = world(vec![
            unit(1, 0, "mbt", 0.0, 0.0),
            unit(2, 1, "mbt", 1000.0, PI),
        ]);
        let ev = fight(&mut w, &catalog(), 2);
        assert!(ev.is_empty());
        assert!(w.units().iter().all(|u| u.state == UnitState::Healthy));
    }

    #[test]
    fn g4_out_of_range_no_fire() {
        let mut w = world(vec![
            unit(1, 0, "mbt", 0.0, 0.0),
            unit(2, 1, "mbt", 2100.0, PI),
        ]);
        assert!(fight(&mut w, &catalog(), 3).is_empty());
    }

    #[test]
    fn g4_nearest_target_then_lowest_id() {
        let mut w = world(vec![
            unit(1, 0, "mbt", 1000.0, 0.0),
            unit(5, 1, "infantry", 1300.0, 0.0),
            unit(3, 1, "infantry", 700.0, 0.0),
            unit(9, 1, "infantry", 1100.0, 0.0),
        ]);
        let t = tracks(&w, 3);
        let mut reloads = Reloads::default();
        for id in [3, 5, 9] {
            reloads.0.insert(UnitId(id), 5);
        }
        let ev = step_combat(&mut w, &catalog(), &rules(), &t, &mut reloads);
        // 9 at 100 m is nearest.
        assert_eq!(
            ev[0],
            Event::ShotFired {
                shooter: UnitId(1),
                target: UnitId(9)
            }
        );
        // Equal distance (3 at 700, 5 at 1300 -> both 300 m): lowest id.
        let mut w = world(vec![
            unit(1, 0, "mbt", 1000.0, 0.0),
            unit(5, 1, "infantry", 1300.0, 0.0),
            unit(3, 1, "infantry", 700.0, 0.0),
        ]);
        let mut reloads = Reloads::default();
        reloads.0.insert(UnitId(3), 5);
        reloads.0.insert(UnitId(5), 5);
        let ev = step_combat(&mut w, &catalog(), &rules(), &t, &mut reloads);
        assert_eq!(
            ev[0],
            Event::ShotFired {
                shooter: UnitId(1),
                target: UnitId(3)
            }
        );
    }

    #[test]
    fn g4_reload_and_infantry_hp() {
        let cat = catalog();
        let inf = cat.get("infantry").unwrap().clone();
        let mut w = world(vec![
            unit(1, 0, "infantry", 0.0, 0.0),
            unit(2, 1, "infantry", 300.0, PI),
        ]);
        let t = tracks(&w, 3);
        let mut reloads = Reloads::default();
        let mut shots_by_1 = Vec::new();
        for tick in 0..=inf.reload_ticks {
            for e in step_combat(&mut w, &cat, &rules(), &t, &mut reloads) {
                if matches!(
                    e,
                    Event::ShotFired {
                        shooter: UnitId(1),
                        ..
                    }
                ) {
                    shots_by_1.push(tick);
                }
            }
        }
        assert_eq!(shots_by_1, [0, inf.reload_ticks]);
        assert_eq!(w.unit(UnitId(2)).unwrap().hp, 100.0 - 2.0 * inf.damage_hp);
    }

    #[test]
    fn g4_damaged_twice_is_destroyed() {
        let mut cat = catalog();
        if let Some(t) = cat.types.get_mut("mbt") {
            t.penetration_mm = 500.0;
        }
        let mut w = world(vec![
            unit(1, 0, "mbt", 0.0, 0.0),
            unit(2, 1, "mbt", 1000.0, PI),
        ]);
        w.unit_mut(UnitId(2)).unwrap().state = UnitState::Damaged;
        let t = tracks(&w, 3);
        let mut reloads = Reloads::default();
        reloads.0.insert(UnitId(2), 5);
        step_combat(&mut w, &cat, &rules(), &t, &mut reloads);
        assert_eq!(w.unit(UnitId(2)).unwrap().state, UnitState::Destroyed);
    }

    #[test]
    fn g4_damaged_tank_moves_at_half_speed() {
        use crate::movement::{step_movement, Routes};
        let cat = catalog();
        let mut w = world(vec![unit(1, 0, "mbt", 0.0, 0.0)]);
        w.unit_mut(UnitId(1)).unwrap().state = UnitState::Damaged;
        let mut routes = Routes::new();
        routes.insert(UnitId(1), vec![Vec2::new(1000.0, 500.0)]);
        step_movement(&mut w, &cat, &rules(), &mut routes);
        let expected = cat.get("mbt").unwrap().speed_offroad_mps * crate::TICK_SECONDS * 0.5;
        assert!((w.unit(UnitId(1)).unwrap().pos.x - expected).abs() < 1e-9);
    }
}
