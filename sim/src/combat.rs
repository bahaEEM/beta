// SPDX-License-Identifier: AGPL-3.0-or-later
//! Direct fire with deterministic expected damage (no dice), aim time and rules of engagement.

use crate::data::UnitCatalog;
use crate::data::{Rules, UnitType};
use crate::event::Event;
use crate::sensor::{line_of_sight, TrackTable};
use crate::world::{Stance, Unit, UnitId, UnitState, Vec2, World};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::f64::consts::{FRAC_PI_4, PI};

/// Per-unit fire control state.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CombatState {
    /// Ticks until loaded again. Absent = loaded (units start loaded).
    pub reloads: BTreeMap<UnitId, u32>,
    /// Current target and ticks it has been held in the sights.
    pub aims: BTreeMap<UnitId, (UnitId, u32)>,
    /// Indirect fire: ticks spent laying the gun on the current fire mission.
    #[serde(default)]
    pub lay: BTreeMap<UnitId, u32>,
}

/// Expected hit share at distance `d` (linear from `hit_close` to `hit_far` over the range).
pub fn hit_share(t: &UnitType, d: f64) -> f64 {
    if t.weapon_range_m <= 0.0 {
        return 0.0;
    }
    let f = (d / t.weapon_range_m).clamp(0.0, 1.0);
    t.hit_close + (t.hit_far - t.hit_close) * f
}

/// Penetration multiplier: full, partial or none.
pub fn penetration_factor(rules: &Rules, penetration: f64, effective_armor: f64) -> f64 {
    let c = &rules.combat;
    if penetration >= effective_armor {
        1.0
    } else if penetration >= effective_armor * c.partial_pen_threshold {
        c.partial_damage_factor
    } else {
        0.0
    }
}

fn may_fire(u: &Unit, rules: &Rules, tick: u32) -> bool {
    match u.stance {
        Stance::Free => true,
        Stance::Hold => false,
        Stance::Return => u
            .under_fire_tick
            .is_some_and(|t| tick.saturating_sub(t) <= rules.ticks(rules.combat.return_fire_s, 0)),
    }
}

/// One combat tick: all shooters aim and pick targets from the same pre-fire state,
/// then all shots are applied together (simultaneous fire).
pub fn step_combat(
    world: &mut World,
    catalog: &UnitCatalog,
    rules: &Rules,
    tracks: &TrackTable,
    state: &mut CombatState,
) -> Vec<Event> {
    for left in state.reloads.values_mut() {
        *left = left.saturating_sub(1);
    }
    state.reloads.retain(|_, left| *left > 0);

    let mut shots: Vec<(UnitId, UnitId)> = Vec::new();
    for s in world.units() {
        let target = (s.active() && s.deployed() && s.ammo > 0 && may_fire(s, rules, world.tick))
            .then(|| catalog.get(&s.type_id))
            .flatten()
            .and_then(|st| pick_target(world, catalog, rules, tracks, s, st).map(|t| (st, t)));
        let Some((st, target)) = target else {
            state.aims.remove(&s.id);
            continue;
        };
        let held = match state.aims.get(&s.id) {
            Some(&(t, n)) if t == target => n + 1,
            _ => 1,
        };
        state.aims.insert(s.id, (target, held));
        if held >= rules.aim_ticks(st) && !state.reloads.contains_key(&s.id) {
            shots.push((s.id, target));
        }
    }

    let mut events = Vec::new();
    let tick = world.tick;
    for (shooter_id, target_id) in shots {
        let (Some(shooter), Some(target)) = (world.unit(shooter_id), world.unit(target_id)) else {
            continue;
        };
        let (Some(st), Some(tt)) = (catalog.get(&shooter.type_id), catalog.get(&target.type_id))
        else {
            continue;
        };
        state.reloads.insert(shooter_id, rules.reload_ticks(st));
        events.push(Event::ShotFired {
            shooter: shooter_id,
            target: target_id,
        });
        let already_dead = target.state == UnitState::Destroyed;
        let d = shooter.pos.distance(target.pos);
        let damage = st.damage_hp
            * hit_share(st, d)
            * penetration_factor(
                rules,
                st.penetration_mm,
                effective_armor(tt, target, shooter.pos),
            );
        let damaged_at = tt.hp * rules.combat.damaged_hp_fraction;
        if let Some(s) = world.unit_mut(shooter_id) {
            s.ammo = s.ammo.saturating_sub(1);
        }
        if already_dead {
            continue; // killed by an earlier shot this tick; the round is still spent
        }
        let Some(target) = world.unit_mut(target_id) else {
            continue;
        };
        target.under_fire_tick = Some(tick);
        if damage <= 0.0 {
            continue;
        }
        target.hp = (target.hp - damage).max(0.0);
        if target.hp <= 0.0 {
            target.state = UnitState::Destroyed;
            events.push(Event::UnitDestroyed { unit: target_id });
        } else {
            if target.hp <= damaged_at {
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
    if st.weapon_range_m <= 0.0 {
        return None;
    }
    world
        .units()
        .iter()
        .filter(|t| t.owner != shooter.owner && t.active())
        .filter(|t| {
            tracks.get(shooter.owner, t.id).is_some_and(|tr| {
                let air = catalog.get(&t.type_id).is_some_and(|tt| tt.class.is_air());
                let needed = if air {
                    rules.combat.air_fire_quality
                } else {
                    rules.sensor.optical.fire_quality
                };
                tr.quality >= needed
            })
        })
        .filter_map(|t| {
            let d = shooter.pos.distance(t.pos);
            let tt = catalog.get(&t.type_id)?;
            let engageable = if tt.class.is_air() {
                st.weapon_vs_air
            } else {
                st.weapon_vs_ground
            };
            (engageable
                && d <= st.weapon_range_m
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
    use crate::sensor::Track;
    use crate::turn::tests::{catalog, rules};
    use crate::world::{HeightMap, PlayerId};

    fn unit(id: u32, owner: u8, type_id: &str, x: f64, heading: f64) -> Unit {
        let cat = catalog();
        let mut u = Unit::new(
            UnitId(id),
            PlayerId(owner),
            cat.get(type_id).unwrap(),
            Vec2::new(x, 500.0),
            heading,
        );
        u.ammo = 1000;
        u
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
                let track = Track {
                    observer_player: o.owner,
                    target_unit: e.id,
                    quality,
                    last_seen_tick: 0,
                    last_known_pos: e.pos,
                    bearing: None,
                    position_known: true,
                };
                t.tracks.insert((o.owner, e.id), track);
            }
        }
        t
    }

    /// Runs `ticks` combat ticks with fixed full tracks; returns all events.
    fn fight(w: &mut World, ticks: u32, quality: u8) -> Vec<(u32, Event)> {
        let (cat, r) = (catalog(), rules());
        let t = tracks(w, quality);
        let mut state = CombatState::default();
        let mut out = Vec::new();
        for tick in 1..=ticks {
            w.tick = tick;
            out.extend(
                step_combat(w, &cat, &r, &t, &mut state)
                    .into_iter()
                    .map(|e| (tick, e)),
            );
        }
        out
    }

    fn shots_by(ev: &[(u32, Event)], id: u32) -> Vec<u32> {
        ev.iter()
            .filter(
                |(_, e)| matches!(e, Event::ShotFired { shooter, .. } if *shooter == UnitId(id)),
            )
            .map(|&(t, _)| t)
            .collect()
    }

    #[test]
    fn k041_first_shot_only_after_aim_time_then_reload() {
        let (cat, r) = (catalog(), rules());
        let mbt = cat.get("mbt").unwrap();
        let mut w = world(vec![
            unit(1, 0, "mbt", 0.0, 0.0),
            unit(2, 1, "mbt", 1500.0, PI),
        ]);
        w.unit_mut(UnitId(2)).unwrap().stance = Stance::Hold;
        let ev = fight(&mut w, r.aim_ticks(mbt) + r.reload_ticks(mbt), 3);
        assert_eq!(
            shots_by(&ev, 1),
            [r.aim_ticks(mbt), r.aim_ticks(mbt) + r.reload_ticks(mbt)]
        );
    }

    #[test]
    fn k043_expected_damage_falls_with_range_and_armour() {
        let (cat, r) = (catalog(), rules());
        let mbt = cat.get("mbt").unwrap();
        assert!(hit_share(mbt, 500.0) > hit_share(mbt, 1900.0));
        assert_eq!(hit_share(mbt, 0.0), mbt.hit_close);
        assert_eq!(hit_share(mbt, mbt.weapon_range_m), mbt.hit_far);
        assert_eq!(penetration_factor(&r, 650.0, 600.0), 1.0);
        assert_eq!(
            penetration_factor(&r, 450.0, 600.0),
            r.combat.partial_damage_factor
        );
        assert_eq!(penetration_factor(&r, 5.0, 600.0), 0.0);
        // One hit on a tank front at 1500 m.
        let mut w = world(vec![
            unit(1, 0, "mbt", 0.0, 0.0),
            unit(2, 1, "mbt", 1500.0, PI),
        ]);
        w.unit_mut(UnitId(2)).unwrap().stance = Stance::Hold;
        fight(&mut w, r.aim_ticks(mbt), 3);
        let expected = mbt.hp - mbt.damage_hp * hit_share(mbt, 1500.0);
        assert!((w.unit(UnitId(2)).unwrap().hp - expected).abs() < 1e-9);
    }

    #[test]
    fn k043_side_hit_hurts_more_than_front_with_partial_penetration() {
        let mut cat = catalog();
        if let Some(t) = cat.types.get_mut("mbt") {
            t.penetration_mm = 500.0; // front 600: partial; side 80: full
        }
        let r = rules();
        let hp_after = |heading: f64| {
            let mut w = world(vec![
                unit(1, 0, "mbt", 0.0, 0.0),
                unit(2, 1, "mbt", 1000.0, heading),
            ]);
            w.unit_mut(UnitId(2)).unwrap().stance = Stance::Hold;
            let t = tracks(&w, 3);
            let mut s = CombatState::default();
            for tick in 1..=r.aim_ticks(cat.get("mbt").unwrap()) {
                w.tick = tick;
                step_combat(&mut w, &cat, &r, &t, &mut s);
            }
            w.unit(UnitId(2)).unwrap().hp
        };
        assert!(hp_after(PI / 2.0) < hp_after(PI), "side < front");
    }

    #[test]
    fn k044_damage_states_by_hp() {
        let (cat, r) = (catalog(), rules());
        let mut w = world(vec![
            unit(1, 0, "mbt", 0.0, 0.0),
            unit(2, 1, "mbt", 200.0, PI),
        ]);
        w.unit_mut(UnitId(2)).unwrap().stance = Stance::Hold;
        let ev = fight(&mut w, 400, 3);
        let mbt = cat.get("mbt").unwrap();
        let damaged_tick = ev.iter().find_map(|(t, e)| {
            matches!(
                e,
                Event::UnitDamaged {
                    state: UnitState::Damaged,
                    ..
                }
            )
            .then_some(*t)
        });
        let destroyed_tick = ev
            .iter()
            .find_map(|(t, e)| matches!(e, Event::UnitDestroyed { .. }).then_some(*t));
        assert!(
            damaged_tick.is_some() && destroyed_tick.is_some() && damaged_tick < destroyed_tick
        );
        assert_eq!(w.unit(UnitId(2)).unwrap().hp, 0.0);
        // No more shots at a destroyed target.
        assert_eq!(shots_by(&ev, 1).last(), destroyed_tick.as_ref());
        assert!(r.combat.damaged_hp_fraction * mbt.hp > 0.0);
    }

    #[test]
    fn k045_simultaneous_fire_both_take_damage_same_tick() {
        let mut w = world(vec![
            unit(1, 0, "mbt", 0.0, 0.0),
            unit(2, 1, "mbt", 1000.0, PI),
        ]);
        let ev = fight(&mut w, rules().aim_ticks(catalog().get("mbt").unwrap()), 3);
        assert_eq!(shots_by(&ev, 1), shots_by(&ev, 2));
        assert!(w.units().iter().all(|u| u.hp < 100.0));
    }

    #[test]
    fn k040_no_fire_below_track_quality_3_or_out_of_range() {
        let mut w = world(vec![
            unit(1, 0, "mbt", 0.0, 0.0),
            unit(2, 1, "mbt", 1000.0, PI),
        ]);
        assert!(fight(&mut w, 50, 2).is_empty());
        let mut w = world(vec![
            unit(1, 0, "mbt", 0.0, 0.0),
            unit(2, 1, "mbt", 2100.0, PI),
        ]);
        assert!(fight(&mut w, 50, 3).is_empty());
    }

    #[test]
    fn k040_nearest_target_then_lowest_id() {
        let mut w = world(vec![
            unit(1, 0, "mbt", 1000.0, 0.0),
            unit(5, 1, "infantry", 1300.0, 0.0),
            unit(3, 1, "infantry", 700.0, 0.0),
        ]);
        for id in [3, 5] {
            w.unit_mut(UnitId(id)).unwrap().stance = Stance::Hold;
        }
        let ev = fight(&mut w, 20, 3);
        assert!(ev.iter().any(|(_, e)| *e
            == Event::ShotFired {
                shooter: UnitId(1),
                target: UnitId(3)
            }));
        assert!(!ev.iter().any(|(_, e)| *e
            == Event::ShotFired {
                shooter: UnitId(1),
                target: UnitId(5)
            }));
    }

    #[test]
    fn k046_stances_hold_and_return_fire() {
        let r = rules();
        let mbt = catalog().get("mbt").unwrap().clone();
        // Both on 'return': nobody starts shooting.
        let mut w = world(vec![
            unit(1, 0, "mbt", 0.0, 0.0),
            unit(2, 1, "mbt", 1000.0, PI),
        ]);
        for id in [1, 2] {
            w.unit_mut(UnitId(id)).unwrap().stance = Stance::Return;
        }
        assert!(fight(&mut w, 50, 3).is_empty());
        // One 'free', one 'return': the second answers after being hit, then aims.
        let mut w = world(vec![
            unit(1, 0, "mbt", 0.0, 0.0),
            unit(2, 1, "mbt", 1000.0, PI),
        ]);
        w.unit_mut(UnitId(2)).unwrap().stance = Stance::Return;
        let ev = fight(&mut w, 60, 3);
        let first = shots_by(&ev, 1)[0];
        assert_eq!(shots_by(&ev, 2)[0], first + r.aim_ticks(&mbt));
    }

    #[test]
    fn k042_no_ammo_no_fire_and_each_shot_uses_a_round() {
        let mut w = world(vec![
            unit(1, 0, "mbt", 0.0, 0.0),
            unit(2, 1, "mbt", 1500.0, PI),
        ]);
        w.unit_mut(UnitId(1)).unwrap().ammo = 2;
        w.unit_mut(UnitId(2)).unwrap().stance = Stance::Hold;
        let ev = fight(&mut w, 200, 3);
        assert_eq!(shots_by(&ev, 1).len(), 2);
        assert_eq!(w.unit(UnitId(1)).unwrap().ammo, 0);
    }

    #[test]
    fn k043_effective_armour_by_facing() {
        let cat = catalog();
        let tt = cat.get("mbt").unwrap();
        let target = unit(1, 0, "mbt", 0.0, 0.0);
        assert_eq!(
            effective_armor(tt, &target, Vec2::new(100.0, 500.0)),
            tt.armor_front_mm
        );
        assert!(effective_armor(tt, &target, Vec2::new(100.0, 550.0)) > tt.armor_front_mm);
        assert_eq!(
            effective_armor(tt, &target, Vec2::new(-100.0, 500.0)),
            tt.armor_rear_mm
        );
    }
}
