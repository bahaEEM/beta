// SPDX-License-Identifier: AGPL-3.0-or-later
//! G3: line of sight, sensors and tracks.

use crate::data::{Rules, SensorSpec, UnitCatalog, UnitType};
use crate::world::{HeightMap, PlayerId, Unit, UnitId, Vec2, World};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// True if nothing on the terrain blocks the straight ray between eye and target.
/// The ray is sampled every half cell; only interior samples are tested.
pub fn line_of_sight(
    map: &HeightMap,
    from: Vec2,
    to: Vec2,
    eye_height_m: f64,
    target_height_m: f64,
) -> bool {
    let eye = map.height_at(from) + eye_height_m;
    let target = map.height_at(to) + target_height_m;
    let steps = (from.distance(to) / (map.cell_size_m() / 2.0)).ceil() as u32;
    (1..steps).all(|k| {
        let t = f64::from(k) / f64::from(steps);
        let p = Vec2::new(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t);
        map.height_at(p) <= eye + (target - eye) * t
    })
}

/// Direct (non-ESM) track quality one sensor of `observer` achieves on `target`.
/// `jammed`: an enemy jammer currently covers the observer.
pub fn sensor_quality(
    sensor: &SensorSpec,
    map: &HeightMap,
    rules: &Rules,
    observer: (&Unit, &UnitType),
    target: (&Unit, &UnitType),
    jammed: bool,
) -> u8 {
    let d = observer.0.pos.distance(target.0.pos);
    let los = || {
        line_of_sight(
            map,
            observer.0.pos,
            target.0.pos,
            observer.1.height_m,
            target.1.height_m,
        )
    };
    match *sensor {
        SensorSpec::Optical { range_m } => {
            let o = &rules.sensor.optical;
            if d > range_m * o.track_range_factor || !los() {
                0
            } else if d <= range_m {
                o.fire_quality
            } else {
                o.contact_quality
            }
        }
        SensorSpec::Radar {
            range_m,
            min_target_speed_mps,
            air,
        } => {
            let r = &rules.sensor.radar;
            if !observer.0.radiating()
                || air != target.1.class.is_air()
                || d > range_m
                || target.0.speed_mps < min_target_speed_mps
                || !los()
            {
                0
            } else if jammed {
                r.quality.saturating_sub(r.jam_quality_drop)
            } else {
                r.quality
            }
        }
        // ESM is combined across observers in `update_tracks`; jammers do not detect.
        SensorSpec::Esm { .. } | SensorSpec::Jammer { .. } => 0,
    }
}

/// Direction from a passive sensor to an emitter.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bearing {
    pub origin: Vec2,
    pub angle_rad: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub observer_player: PlayerId,
    pub target_unit: UnitId,
    pub quality: u8,
    pub last_seen_tick: u32,
    /// Last position from a non-bearing sensor (meaningful only if `position_known`).
    pub last_known_pos: Vec2,
    /// Set while the track is bearing-only (single ESM line); kept after loss.
    #[serde(default)]
    pub bearing: Option<Bearing>,
    /// True once any sensor other than a single bearing has located the target.
    #[serde(default)]
    pub position_known: bool,
}

/// Every track ever gained, keyed by (observer player, target unit).
/// Serialised as a list because JSON maps cannot have tuple keys.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(from = "Vec<Track>", into = "Vec<Track>")]
pub struct TrackTable {
    pub tracks: BTreeMap<(PlayerId, UnitId), Track>,
}

impl From<Vec<Track>> for TrackTable {
    fn from(v: Vec<Track>) -> Self {
        let tracks = v
            .into_iter()
            .map(|t| ((t.observer_player, t.target_unit), t))
            .collect();
        Self { tracks }
    }
}

impl From<TrackTable> for Vec<Track> {
    fn from(t: TrackTable) -> Self {
        t.tracks.into_values().collect()
    }
}

impl TrackTable {
    pub fn get(&self, player: PlayerId, unit: UnitId) -> Option<&Track> {
        self.tracks.get(&(player, unit))
    }
}

/// A track whose quality or bearing changed this tick (for TrackGained/TrackLost events).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackChange {
    pub player: PlayerId,
    pub unit: UnitId,
    pub old_quality: u8,
    pub new_quality: u8,
}

fn angle_between(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(std::f64::consts::PI);
    d.min(std::f64::consts::PI - d)
}

/// Recomputes all tracks for the current tick. Returns changes in (player, unit) order.
/// Destroyed units neither observe nor are observed; units of unknown type are skipped.
pub fn update_tracks(
    world: &World,
    catalog: &UnitCatalog,
    rules: &Rules,
    table: &mut TrackTable,
) -> Vec<TrackChange> {
    let alive: Vec<(&Unit, &UnitType)> = world
        .units()
        .iter()
        .filter_map(|u| live_type(catalog, u).map(|t| (u, t)))
        .collect();
    let jammers: Vec<(PlayerId, Vec2, f64)> = alive
        .iter()
        .filter(|(u, _)| u.radiating())
        .filter_map(|(u, t)| t.jammer_range_m().map(|r| (u.owner, u.pos, r)))
        .collect();
    let players: BTreeSet<PlayerId> = world.units().iter().map(|u| u.owner).collect();
    let esm = &rules.sensor.esm;
    let mut changes = Vec::new();
    for &player in &players {
        let observers: Vec<(&Unit, &UnitType, bool)> = alive
            .iter()
            .filter(|(o, _)| o.owner == player)
            .map(|&(o, t)| {
                let jammed = jammers
                    .iter()
                    .any(|&(owner, pos, r)| owner != player && pos.distance(o.pos) <= r);
                (o, t, jammed)
            })
            .collect();
        for target in world.units().iter().filter(|u| u.owner != player) {
            let (direct, bearings) = match live_type(catalog, target) {
                None => (0, Vec::new()),
                Some(tt) => {
                    let direct = observers
                        .iter()
                        .flat_map(|&(o, ot, jammed)| {
                            ot.sensors.iter().map(move |s| {
                                sensor_quality(s, &world.map, rules, (o, ot), (target, tt), jammed)
                            })
                        })
                        .max()
                        .unwrap_or(0);
                    let emitting = target.radiating() && tt.is_emitter();
                    let bearings: Vec<Bearing> = observers
                        .iter()
                        .filter(|_| emitting)
                        .filter(|&&(o, ot, _)| {
                            o.deployed() && ot.sensors.iter().any(|s| {
                                matches!(s, SensorSpec::Esm { range_m } if o.pos.distance(target.pos) <= *range_m)
                            }) && line_of_sight(&world.map, o.pos, target.pos, ot.height_m, tt.height_m)
                        })
                        .map(|&(o, _, _)| Bearing {
                            origin: o.pos,
                            angle_rad: (target.pos.y - o.pos.y).atan2(target.pos.x - o.pos.x),
                        })
                        .collect();
                    (direct, bearings)
                }
            };
            let crossed = bearings.iter().enumerate().any(|(i, a)| {
                bearings.iter().skip(i + 1).any(|b| {
                    angle_between(a.angle_rad, b.angle_rad) >= esm.min_cross_deg.to_radians()
                })
            });
            let esm_quality = if crossed {
                esm.triangulated_quality
            } else if bearings.is_empty() {
                0
            } else {
                esm.bearing_quality
            };
            let quality = direct.max(esm_quality);
            // Bearing-only: the single ESM line is the best we have.
            let bearing = (quality > 0 && direct == 0 && !crossed)
                .then(|| bearings.first().copied())
                .flatten();

            let key = (player, target.id);
            let old = table.tracks.get(&key).cloned();
            let old_quality = old.as_ref().map_or(0, |t| t.quality);
            let old_bearing = old
                .as_ref()
                .and_then(|t| (t.quality > 0).then_some(t.bearing).flatten());
            if quality > 0 {
                let located = bearing.is_none();
                let prev_known = old.as_ref().is_some_and(|t| t.position_known);
                let entry = table.tracks.entry(key).or_insert(Track {
                    observer_player: player,
                    target_unit: target.id,
                    quality,
                    last_seen_tick: world.tick,
                    last_known_pos: target.pos,
                    bearing: None,
                    position_known: false,
                });
                entry.quality = quality;
                entry.last_seen_tick = world.tick;
                entry.bearing = bearing;
                entry.position_known = prev_known || located;
                if located {
                    entry.last_known_pos = target.pos;
                }
            } else if let Some(t) = table.tracks.get_mut(&key) {
                t.quality = 0;
            }
            let bearing_moved = match (old_bearing, bearing) {
                (None, None) => false,
                (Some(a), Some(b)) => {
                    angle_between(a.angle_rad, b.angle_rad) >= esm.bearing_update_deg.to_radians()
                }
                _ => true,
            };
            if quality != old_quality || (quality > 0 && bearing_moved) {
                changes.push(TrackChange {
                    player,
                    unit: target.id,
                    old_quality,
                    new_quality: quality,
                });
            }
        }
    }
    changes
}

/// Type of a non-destroyed unit; None if destroyed or of unknown type.
fn live_type<'a>(catalog: &'a UnitCatalog, u: &Unit) -> Option<&'a UnitType> {
    if u.active() {
        catalog.get(&u.type_id)
    } else {
        None // destroyed, or riding inside a carrier
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{load_rules, load_units};
    use crate::world::UnitState;

    const CELL: f64 = 10.0;

    fn flat_map(n: usize) -> HeightMap {
        HeightMap::new(n, n, CELL, vec![0.0; n * n]).unwrap()
    }

    fn unit(id: u32, owner: u8, type_id: &str, x: f64) -> Unit {
        Unit {
            id: UnitId(id),
            owner: PlayerId(owner),
            type_id: type_id.into(),
            pos: Vec2::new(x, 50.0),
            heading_rad: 0.0,
            hp: 100.0,
            state: UnitState::Healthy,
            fuel: 1e6,
            ammo: 1000,
            ..Default::default()
        }
    }

    fn setup(map: HeightMap, units: Vec<Unit>) -> (World, UnitCatalog, Rules) {
        let mut w = World::new(map);
        for u in units {
            w.add_unit(u).unwrap();
        }
        let cat = load_units(include_str!("../../data/units.toml")).unwrap();
        let rules = load_rules(include_str!("../../data/rules.toml")).unwrap();
        (w, cat, rules)
    }

    fn quality_after_update(w: &World, cat: &UnitCatalog, rules: &Rules) -> u8 {
        let mut table = TrackTable::default();
        update_tracks(w, cat, rules, &mut table);
        table.get(PlayerId(0), UnitId(2)).map_or(0, |t| t.quality)
    }

    #[test]
    fn g3_hill_blocks_line_of_sight() {
        // 30x30 cells of 10 m; a 20 m ridge along column 15 (x = 150 m).
        let n = 30;
        let mut h = vec![0.0f32; n * n];
        for j in 0..n {
            h[j * n + 15] = 20.0;
        }
        let hill = HeightMap::new(n, n, CELL, h).unwrap();
        let units = vec![unit(1, 0, "infantry", 50.0), unit(2, 1, "infantry", 250.0)];
        let (w, cat, rules) = setup(hill, units.clone());
        assert!(!line_of_sight(
            &w.map,
            Vec2::new(50.0, 50.0),
            Vec2::new(250.0, 50.0),
            1.8,
            1.8
        ));
        assert_eq!(quality_after_update(&w, &cat, &rules), 0);

        let (w, cat, rules) = setup(flat_map(n), units);
        assert!(line_of_sight(
            &w.map,
            Vec2::new(50.0, 50.0),
            Vec2::new(250.0, 50.0),
            1.8,
            1.8
        ));
        assert_eq!(quality_after_update(&w, &cat, &rules), 3);
    }

    #[test]
    fn g3_optical_range_boundaries() {
        // Infantry optical range 1000 m, contact range 1500 m.
        for (x, expected) in [(1049.0, 3), (1051.0, 1), (1549.0, 1), (1551.0, 0)] {
            let units = vec![unit(1, 0, "infantry", 50.0), unit(2, 1, "infantry", x)];
            let (w, cat, rules) = setup(flat_map(200), units);
            assert_eq!(quality_after_update(&w, &cat, &rules), expected, "x = {x}");
        }
    }

    #[test]
    fn g3_lost_track_keeps_last_known_pos() {
        let units = vec![unit(1, 0, "infantry", 50.0), unit(2, 1, "infantry", 500.0)];
        let (mut w, cat, rules) = setup(flat_map(300), units);
        let mut table = TrackTable::default();
        w.tick = 7;
        let ch = update_tracks(&w, &cat, &rules, &mut table);
        let gained = |p, u| TrackChange {
            player: PlayerId(p),
            unit: UnitId(u),
            old_quality: 0,
            new_quality: 3,
        };
        assert_eq!(ch, [gained(0, 2), gained(1, 1)]);

        w.tick = 8;
        if let Some(u) = w.unit_mut(UnitId(2)) {
            u.pos = Vec2::new(2500.0, 50.0);
        }
        let ch = update_tracks(&w, &cat, &rules, &mut table);
        assert_eq!(ch.len(), 2);
        assert_eq!((ch[0].old_quality, ch[0].new_quality), (3, 0));
        assert_eq!((ch[1].player, ch[1].unit), (PlayerId(1), UnitId(1)));
        let t = table.get(PlayerId(0), UnitId(2)).unwrap();
        assert_eq!(t.quality, 0);
        assert_eq!(t.last_seen_tick, 7);
        assert_eq!(t.last_known_pos, Vec2::new(500.0, 50.0));
    }

    #[test]
    fn g3_destroyed_units_do_not_observe() {
        let mut obs = unit(1, 0, "infantry", 50.0);
        obs.state = UnitState::Destroyed;
        let (w, cat, rules) = setup(flat_map(100), vec![obs, unit(2, 1, "infantry", 300.0)]);
        assert_eq!(quality_after_update(&w, &cat, &rules), 0);
    }
}
