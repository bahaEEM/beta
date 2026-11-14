// SPDX-License-Identifier: AGPL-3.0-or-later
//! K-140…K-144: concealment, cover, ambush, suppression, decoys.

use crate::combat::{effective_armor, hit_share, penetration_factor, pinned, protection};
use crate::event::Event;
use crate::sensor::concealment;
use crate::turn::tests::{catalog, game, rules, unit};
use crate::turn::{plan_phase, step_tick, Game, Order};
use crate::visibility::contacts;
use crate::world::{HeightMap, PlayerId, Stance, Unit, UnitId, Vec2};
use std::collections::BTreeMap;

/// Flat 20 x 10 km: open west of x = 10 km, forest from 10 to 15 km, settlement east of it.
fn map() -> HeightMap {
    let terrain = (0..80_000)
        .map(|k| match k % 400 {
            0..200 => 0,
            200..300 => 1,
            _ => 2,
        })
        .collect();
    HeightMap::new(400, 200, 50.0, vec![0.0; 80_000])
        .unwrap()
        .with_terrain(terrain)
        .unwrap()
}

fn quality(g: &Game, player: u8, target: u32) -> u8 {
    g.tracks
        .get(PlayerId(player), UnitId(target))
        .map_or(0, |t| t.quality)
}

fn run(g: &mut Game, ticks: u32) -> Vec<Event> {
    (0..ticks)
        .flat_map(|_| step_tick(g, &catalog(), &rules()))
        .collect()
}

fn held(u: Unit) -> Unit {
    Unit {
        stance: Stance::Hold,
        ..u
    }
}

/// First shot of `shooter` in the next `ticks` ticks: (distance at the shot, hp the
/// target lost, shooter suppression at the shot).
fn first_shot(g: &mut Game, shooter: u32, ticks: u32) -> Option<(f64, f64, f64)> {
    for _ in 0..ticks {
        let hp_before: BTreeMap<UnitId, f64> =
            g.world.units().iter().map(|u| (u.id, u.hp)).collect();
        let events = step_tick(g, &catalog(), &rules());
        let target = events.iter().find_map(|e| match e {
            Event::ShotFired { shooter: s, target } if *s == UnitId(shooter) => Some(*target),
            _ => None,
        });
        if let Some(target) = target {
            let s = g.world.unit(UnitId(shooter)).unwrap();
            let t = g.world.unit(target).unwrap();
            return Some((
                s.pos.distance(t.pos),
                hp_before[&target] - t.hp,
                s.suppression,
            ));
        }
    }
    None
}

/// Expected damage of one mbt round on the front of an mbt at distance `d`.
fn mbt_on_mbt(d: f64) -> f64 {
    let cat = catalog();
    let mbt = cat.get("mbt").unwrap();
    // Shooter west of a target that faces east (heading 0): a rear hit; use the real geometry.
    let target = unit(9, 1, "mbt", d, 0.0);
    mbt.damage_hp
        * hit_share(mbt, d)
        * penetration_factor(
            &rules(),
            mbt.penetration_mm,
            effective_armor(mbt, &target, Vec2::new(0.0, 0.0)),
        )
}

#[test]
fn k140_cover_and_standing_still_hide_ground_units() {
    let (cat, r) = (catalog(), rules());
    let c = &r.conceal;
    let m = map();
    let factor = |kind: &str, x: f64, moving: bool, fired: bool| {
        let u = Unit {
            speed_mps: if moving { 5.0 } else { 0.0 },
            last_fired_tick: fired.then_some(10),
            ..unit(1, 0, kind, x, 5000.0)
        };
        concealment(&m, &r, &u, cat.get(kind).unwrap(), 20)
    };
    assert_eq!(factor("mbt", 5000.0, true, false), 1.0);
    assert_eq!(factor("mbt", 5000.0, false, false), c.halted_factor);
    assert_eq!(factor("mbt", 12000.0, true, false), c.forest_factor);
    assert_eq!(
        factor("infantry", 17000.0, false, false),
        c.settlement_factor * c.infantry_factor * c.halted_factor
    );
    assert_eq!(
        factor("infantry", 12000.0, false, true),
        1.0,
        "muzzle flash"
    );
    assert_eq!(factor("uav", 12000.0, false, false), 1.0);
    assert_eq!(factor("depot", 12000.0, false, false), 1.0);
    // The reveal wears off.
    let old = Unit {
        last_fired_tick: Some(10),
        ..unit(1, 0, "mbt", 12000.0, 5000.0)
    };
    let later = 10 + r.ticks(c.fire_reveal_s, 0) + 1;
    assert!(concealment(&m, &r, &old, cat.get("mbt").unwrap(), later) < 1.0);

    // A tank (optical 2500 m) looking east at a halted tank in the forest: fire quality
    // only inside 2500 * 0.5 * 0.7 = 875 m, a vague contact up to 1.5 times that.
    let seen = |d: f64, fired: bool| {
        let target = Unit {
            last_fired_tick: fired.then_some(0),
            ..unit(2, 1, "mbt", 10_000.0 + d, 5000.0)
        };
        quality(
            &game(map(), vec![unit(1, 0, "mbt", 10_000.0, 5000.0), target]),
            0,
            2,
        )
    };
    let range = cat.get("mbt").unwrap().sensors[0].range_m() * c.forest_factor * c.halted_factor;
    let o = &r.sensor.optical;
    assert_eq!(seen(range - 50.0, false), o.fire_quality);
    assert_eq!(seen(range + 50.0, false), o.contact_quality);
    assert_eq!(seen(range * o.track_range_factor + 50.0, false), 0);
    assert_eq!(seen(2400.0, true), o.fire_quality, "it fired: plain to see");
}

#[test]
fn k141_cover_protects_infantry_only() {
    let (cat, r) = (catalog(), rules());
    let m = map();
    let p = |kind: &str, x: f64| {
        protection(&m, &r, &unit(1, 0, kind, x, 5000.0), cat.get(kind).unwrap())
    };
    assert_eq!(p("infantry", 5000.0), 1.0);
    assert_eq!(p("infantry", 12000.0), r.cover.forest_damage_factor);
    assert_eq!(p("atgm", 17000.0), r.cover.settlement_damage_factor);
    assert_eq!(p("mbt", 12000.0), 1.0, "vehicles get no cover");

    // The same tank round on infantry 400 m away, in the open and in the forest.
    let loss = |x: f64| {
        let mut g = game(
            map(),
            vec![
                unit(1, 0, "mbt", x - 400.0, 5000.0),
                held(unit(2, 1, "infantry", x, 5000.0)),
            ],
        );
        first_shot(&mut g, 1, 60).unwrap().1
    };
    let (open, forest) = (loss(5000.0), loss(12000.0));
    assert!(open > 0.0);
    assert!((forest - open * r.cover.forest_damage_factor).abs() < 1e-9);
}

#[test]
fn k142_ambush_waits_for_a_close_target_and_strikes_from_hiding() {
    let (cat, r) = (catalog(), rules());
    let range = cat.get("mbt").unwrap().weapon_range_m;
    // Blue tank lying in ambush just inside the forest edge, facing west; a red tank
    // halted in the open `d` metres west of it, holding fire.
    let setup = |d: f64| {
        let ambusher = Unit {
            stance: Stance::Ambush,
            ..unit(1, 0, "mbt", 10_000.0, 5000.0)
        };
        game(
            map(),
            vec![ambusher, held(unit(2, 1, "mbt", 10_000.0 - d, 5000.0))],
        )
    };
    // Seen at fire quality but farther than half the weapon range: no shot.
    let far = range * r.ambush.range_share + 300.0;
    let mut g = setup(far);
    assert_eq!(quality(&g, 0, 2), r.sensor.optical.fire_quality);
    assert!(first_shot(&mut g, 1, 60).is_none());
    // ...unless the ambusher is shot at: then it fights back at any range.
    g.world.unit_mut(UnitId(1)).unwrap().under_fire_tick = Some(g.world.tick);
    assert!(first_shot(&mut g, 1, 60).is_some());

    // Close, and red only has a vague contact of the ambusher: the bonus applies.
    let mut g = setup(950.0);
    assert!(quality(&g, 1, 1) < crate::visibility::EXACT_QUALITY);
    let (d, loss, _) = first_shot(&mut g, 1, 60).unwrap();
    assert!((loss - mbt_on_mbt(d) * r.ambush.damage_factor).abs() < 1e-9);

    // Closer still red has located it: an ordinary shot.
    let mut g = setup(600.0);
    assert!(quality(&g, 1, 1) >= crate::visibility::EXACT_QUALITY);
    let (d, loss, _) = first_shot(&mut g, 1, 60).unwrap();
    assert!((loss - mbt_on_mbt(d)).abs() < 1e-9);
}

#[test]
fn k143_fire_suppresses_slows_and_pins() {
    let (cat, r) = (catalog(), rules());
    let s = &r.suppression;
    let dt = r.turn.game_seconds_per_tick;
    let go = |g: &mut Game, id: u32, x: f64| {
        let o = Order::Move {
            units: vec![UnitId(id)],
            waypoints: vec![Vec2::new(x, 5000.0)],
        };
        plan_phase(g, &cat, &r, &BTreeMap::from([(PlayerId(0), vec![o])]));
    };
    // Speed: a fully suppressed tank crawls; suppression wears off tick by tick.
    let mut g = game(map(), vec![unit(1, 0, "mbt", 1000.0, 5000.0)]);
    g.world.unit_mut(UnitId(1)).unwrap().suppression = 1.0;
    go(&mut g, 1, 9000.0);
    run(&mut g, 1);
    let u = g.world.unit(UnitId(1)).unwrap();
    let left = 1.0 - s.decay_per_s * dt;
    assert!((u.suppression - left).abs() < 1e-12);
    let speed = cat.get("mbt").unwrap().speed_offroad_mps * (1.0 - left * s.speed_penalty);
    assert!((u.pos.x - (1000.0 + speed * dt)).abs() < 1e-9);

    // Pinned infantry stays down until the suppression has worn off below the threshold.
    let mut g = game(map(), vec![unit(1, 0, "infantry", 1000.0, 5000.0)]);
    g.world.unit_mut(UnitId(1)).unwrap().suppression = s.pin_threshold + 0.1;
    go(&mut g, 1, 9000.0);
    let inf = cat.get("infantry").unwrap();
    assert!(pinned(&r, g.world.unit(UnitId(1)).unwrap(), inf));
    let pinned_ticks = (0.1 / (s.decay_per_s * dt)).floor() as u32;
    run(&mut g, pinned_ticks - 1);
    assert_eq!(g.world.unit(UnitId(1)).unwrap().pos.x, 1000.0, "lying down");
    run(&mut g, 3);
    assert!(g.world.unit(UnitId(1)).unwrap().pos.x > 1000.0, "up again");
    // Gone to ground it is harder to hurt; tanks are never pinned.
    let low = Unit {
        suppression: 1.0,
        ..unit(1, 0, "infantry", 1000.0, 5000.0)
    };
    assert_eq!(protection(&map(), &r, &low, inf), s.pinned_damage_factor);
    let tank = Unit {
        suppression: 1.0,
        ..unit(1, 0, "mbt", 1000.0, 5000.0)
    };
    assert!(!pinned(&r, &tank, cat.get("mbt").unwrap()));

    // Every shot aimed at a unit adds suppression, and a suppressed shooter hits less.
    let mut g = game(
        map(),
        vec![
            unit(1, 0, "mbt", 1000.0, 5000.0),
            held(unit(2, 1, "mbt", 2000.0, 5000.0)),
        ],
    );
    g.world.unit_mut(UnitId(1)).unwrap().suppression = 1.0;
    let (d, loss, at_shot) = first_shot(&mut g, 1, 60).unwrap();
    assert!(at_shot > 0.5 && at_shot < 1.0);
    assert!((loss - mbt_on_mbt(d) * (1.0 - at_shot * s.hit_penalty)).abs() < 1e-9);
    assert_eq!(g.world.unit(UnitId(2)).unwrap().suppression, s.per_shot);
    // Aircraft and structures are not suppressed.
    let mut plane = unit(3, 0, "uav", 0.0, 0.0);
    crate::combat::suppress(&mut plane, cat.get("uav").unwrap(), 0.5);
    assert_eq!(plane.suppression, 0.0);
}

#[test]
fn k144_decoys_look_real_until_seen_from_close_up() {
    let (cat, r) = (catalog(), rules());
    let decoy = |id: u32, kind: &str, x: f64| {
        Unit::new(
            UnitId(id),
            PlayerId(0),
            cat.get(kind).unwrap(),
            Vec2::new(x, 5000.0),
            0.0,
        )
    };
    let shown = |g: &Game| {
        contacts(&g.tracks, &g.world, &r, PlayerId(1))
            .first()
            .and_then(|c| c.type_id.clone())
    };
    // A red tank 1500 m from a dummy tank: it sees a tank and wastes rounds on it.
    let far = r.decoy.reveal_range_m + 700.0;
    let mut g = game(
        map(),
        vec![
            decoy(1, "decoy_tank", 5000.0),
            unit(2, 1, "mbt", 5000.0 + far, 5000.0),
        ],
    );
    assert_eq!(quality(&g, 1, 1), r.sensor.optical.fire_quality);
    assert_eq!(shown(&g).as_deref(), Some("mbt"));
    assert!(first_shot(&mut g, 2, 60).is_some(), "the dummy draws fire");

    // From close up it is recognised, for good, and no longer shot at.
    let near = r.decoy.reveal_range_m - 100.0;
    let mut g = game(
        map(),
        vec![
            decoy(1, "decoy_tank", 5000.0),
            unit(2, 1, "mbt", 5000.0 + near, 5000.0),
        ],
    );
    assert_eq!(shown(&g).as_deref(), Some("decoy_tank"));
    assert!(first_shot(&mut g, 2, 60).is_none());
    g.world.unit_mut(UnitId(2)).unwrap().pos = Vec2::new(5000.0 + far, 5000.0);
    run(&mut g, 1);
    assert_eq!(
        shown(&g).as_deref(),
        Some("decoy_tank"),
        "once known, always known"
    );

    // A dummy radar imitates the emission: enemy ESM gets a bearing, and two of them a
    // position with the type of a real radar.
    let mut radar = decoy(1, "decoy_radar", 5000.0);
    radar.emitting = true;
    let mut g = game(
        map(),
        vec![
            radar,
            unit(2, 1, "esm", 12000.0, 5000.0),
            unit(3, 1, "esm", 5000.0, 9000.0),
        ],
    );
    run(&mut g, 1);
    assert_eq!(quality(&g, 1, 1), r.sensor.esm.triangulated_quality);
    assert_eq!(shown(&g).as_deref(), Some("gsr"));
    // Real units are never renamed.
    assert_eq!(unit(9, 0, "mbt", 0.0, 0.0).decoy_of, None);
}
