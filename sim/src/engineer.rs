// SPDX-License-Identifier: AGPL-3.0-or-later
//! K-180…K-182: engineers repair vehicles and structures, build structures, and the
//! dug-in positions they build shelter infantry.

use crate::data::{Rules, UnitCatalog, UnitClass, UnitType};
use crate::supply::zone_at;
use crate::turn::Game;
use crate::world::{Building, PlayerId, Terrain, Unit, UnitId, UnitState, Vec2, World};
use std::collections::BTreeSet;

fn state_for(hp: f64, t: &UnitType, rules: &Rules) -> UnitState {
    if hp <= t.hp * rules.combat.damaged_hp_fraction {
        UnitState::Damaged
    } else {
        UnitState::Healthy
    }
}

/// K-181: true if `engineer` may start building a `t` where it stands (money aside):
/// it is the player's, on the field, can build that type, and stands on land in a
/// region the player holds.
pub fn can_build(
    game: &Game,
    catalog: &UnitCatalog,
    player: PlayerId,
    engineer: UnitId,
    t: &UnitType,
) -> bool {
    game.world
        .unit(engineer)
        .filter(|e| e.owner == player && e.active())
        .is_some_and(|e| {
            catalog
                .get(&e.type_id)
                .is_some_and(|et| et.builds.contains(&t.id))
                && game.world.map.terrain_at(e.pos) != Terrain::Water
                && zone_at(&game.world.map, &game.zones, e.pos)
                    .and_then(|z| game.zones.get(z))
                    .is_some_and(|z| z.owner == Some(player))
        })
}

/// K-181: puts a building site for `type_id` where `engineer` stands.
pub fn start_building(
    game: &mut Game,
    catalog: &UnitCatalog,
    rules: &Rules,
    engineer: UnitId,
    type_id: &str,
) {
    let Some((site_t, t)) = catalog
        .get(&rules.engineer.site_type)
        .zip(catalog.get(type_id))
    else {
        return;
    };
    let Some((owner, pos)) = game.world.unit(engineer).map(|e| (e.owner, e.pos)) else {
        return;
    };
    let id = UnitId(game.world.units().last().map_or(1, |u| u.id.0 + 1));
    let mut site = Unit::new(id, owner, site_t, pos, 0.0);
    site.building = Some(Building {
        type_id: t.id.clone(),
        turns_left: t.build_turns,
    });
    if game.world.add_unit(site).is_ok() {
        game.routes.remove(&engineer);
    }
}

/// K-182: the infantry fighting from dug-in positions right now. Each standing position
/// takes its owner's nearest-by-id `shelter` infantry units within the shelter radius.
pub fn dug_in(world: &World, catalog: &UnitCatalog, rules: &Rules) -> BTreeSet<UnitId> {
    let mut out = BTreeSet::new();
    for b in world.units() {
        let room = catalog.get(&b.type_id).map_or(0, |t| t.shelter) as usize;
        if room == 0 || b.state == UnitState::Destroyed {
            continue;
        }
        let inside: Vec<UnitId> = world
            .units()
            .iter()
            .filter(|u| u.owner == b.owner && u.active() && !out.contains(&u.id))
            .filter(|u| u.pos.distance(b.pos) <= rules.engineer.shelter_radius_m)
            .filter(|u| {
                catalog
                    .get(&u.type_id)
                    .is_some_and(|t| t.class == UnitClass::Infantry)
            })
            .map(|u| u.id)
            .take(room)
            .collect();
        out.extend(inside);
    }
    out
}

/// End of turn, after the economy: engineers next to a building site work on it (K-181);
/// the other engineers and the repair tents mend what is in reach (K-180).
pub fn step_engineers(game: &mut Game, catalog: &UnitCatalog, rules: &Rules) {
    // K-181: one engineer per site, sites and engineers in id order.
    let mut busy: BTreeSet<UnitId> = BTreeSet::new();
    let sites: Vec<(UnitId, PlayerId, Vec2)> = game
        .world
        .units()
        .iter()
        .filter(|u| u.building.is_some() && u.state != UnitState::Destroyed)
        .map(|u| (u.id, u.owner, u.pos))
        .collect();
    for (site, owner, pos) in sites {
        let worker = game
            .world
            .units()
            .iter()
            .filter(|e| e.owner == owner && e.active() && !busy.contains(&e.id))
            .filter(|e| e.pos.distance(pos) <= rules.engineer.build_radius_m)
            .find(|e| {
                catalog
                    .get(&e.type_id)
                    .is_some_and(|t| !t.builds.is_empty())
            })
            .map(|e| e.id);
        let Some(worker) = worker else {
            continue; // work waits
        };
        busy.insert(worker);
        let Some(u) = game.world.unit_mut(site) else {
            continue;
        };
        let Some(b) = u.building.as_mut() else {
            continue;
        };
        b.turns_left = b.turns_left.saturating_sub(1);
        if b.turns_left == 0 {
            if let Some(t) = catalog.get(&b.type_id) {
                *u = Unit::new(u.id, u.owner, t, u.pos, u.heading_rad);
            }
        }
    }

    crate::obstacles::step_works(game, catalog, rules, &mut busy);

    // K-180: (id, owner, position, share) of everything that repairs this turn.
    let menders: Vec<(UnitId, PlayerId, Vec2, f64)> = game
        .world
        .units()
        .iter()
        .filter(|u| u.active() && !busy.contains(&u.id))
        .filter_map(|u| {
            let share = catalog.get(&u.type_id)?.repair_share;
            (share > 0.0).then_some((u.id, u.owner, u.pos, share))
        })
        .collect();
    let reach = rules.engineer.repair_radius_m;
    let money = &mut game.money;
    for u in game.world.units_mut() {
        let Some(t) = catalog.get(&u.type_id).filter(|t| {
            matches!(t.class, UnitClass::Vehicle | UnitClass::Structure)
                && u.active()
                && u.building.is_none()
                && u.hp < t.hp
        }) else {
            continue;
        };
        let share = menders
            .iter()
            .filter(|&&(id, owner, pos, _)| {
                id != u.id && owner == u.owner && pos.distance(u.pos) <= reach
            })
            .map(|&(_, _, _, share)| share)
            .fold(0.0, f64::max);
        if share <= 0.0 {
            continue;
        }
        let gain = (t.hp * share).min(t.hp - u.hp);
        let cost = (gain / t.hp * f64::from(t.cost) * rules.engineer.repair_cost_share).ceil();
        let cost = cost as u32;
        let Some(purse) = money.get_mut(&u.owner).filter(|m| **m >= cost) else {
            continue; // no money, no repair
        };
        *purse -= cost;
        u.hp += gain;
        u.state = state_for(u.hp, t, rules);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Event;
    use crate::indirect::{step_impacts, Shell};
    use crate::logistics::{end_of_turn, Zone};
    use crate::turn::tests::{catalog, game, rules, unit};
    use crate::turn::{plan_phase, step_tick, Order, RejectReason};
    use crate::world::HeightMap;
    use std::collections::BTreeMap;

    fn flat() -> HeightMap {
        HeightMap::new(400, 200, 50.0, vec![0.0; 80_000]).unwrap()
    }

    /// A unit of the catalog type with `hp_share` of its hit points.
    fn at(id: u32, owner: u8, kind: &str, x: f64, hp_share: f64) -> Unit {
        let mut u = unit(id, owner, kind, x, 1000.0);
        u.hp = catalog().get(kind).unwrap().hp * hp_share;
        u
    }

    fn zone(id: u32, x: f64, owner: u8) -> Zone {
        Zone {
            id,
            name_tr: format!("Bölge {id}"),
            center: Vec2::new(x, 1000.0),
            radius_m: 1000.0,
            links: Vec::new(),
            owner: Some(PlayerId(owner)),
            base: None,
            income: 0,
            progress: None,
        }
    }

    fn build(engineer: u32, type_id: &str) -> Order {
        Order::Build {
            units: vec![UnitId(engineer)],
            type_id: type_id.into(),
        }
    }

    fn plan(g: &mut Game, orders: Vec<Order>) -> Vec<RejectReason> {
        plan_phase(
            g,
            &catalog(),
            &rules(),
            &BTreeMap::from([(PlayerId(0), orders)]),
        )
        .remove(&PlayerId(0))
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.reason)
        .collect()
    }

    fn hp_share(g: &Game, id: u32) -> f64 {
        let u = g.world.unit(UnitId(id)).unwrap();
        u.hp / catalog().get(&u.type_id).unwrap().hp
    }

    #[test]
    fn k180_engineers_and_repair_tents_mend_vehicles_and_structures_for_money() {
        let (cat, r) = (catalog(), rules());
        let mut g = game(
            flat(),
            vec![
                at(1, 0, "engineer", 1000.0, 0.5),
                at(2, 0, "mbt", 1100.0, 0.3),
                at(3, 0, "depot", 1200.0, 0.95),
                at(4, 0, "infantry", 1050.0, 0.3),
                at(5, 0, "mbt", 2000.0, 0.3), // out of reach
                at(6, 1, "mbt", 1150.0, 0.3), // not ours
                at(7, 0, "repair_tent", 5000.0, 1.0),
                at(8, 0, "ifv", 5100.0, 0.5),
            ],
        );
        g.money.insert(PlayerId(0), 1000);
        end_of_turn(&mut g, &cat, &r);
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        assert!(near(hp_share(&g, 2), 0.5), "a fifth of its hit points");
        assert_eq!(
            g.world.unit(UnitId(2)).unwrap().state,
            UnitState::Damaged,
            "half is still damaged"
        );
        assert!(near(hp_share(&g, 3), 1.0), "never above the maximum");
        assert!(near(hp_share(&g, 4), 0.3), "infantry needs a medic");
        assert!(near(hp_share(&g, 5), 0.3) && near(hp_share(&g, 6), 0.3));
        assert!(near(hp_share(&g, 1), 0.5), "an engineer cannot mend itself");
        assert!(near(hp_share(&g, 8), 0.7), "the tent mends as well");
        // A fifth of a tank and of an IFV at half price; the depot has no price.
        let price = |id: &str| f64::from(cat.get(id).unwrap().cost);
        let paid = (0.2 * price("mbt") * r.engineer.repair_cost_share).ceil()
            + (0.2 * price("ifv") * r.engineer.repair_cost_share).ceil();
        assert_eq!(g.money[&PlayerId(0)], 1000 - paid as u32);
        end_of_turn(&mut g, &cat, &r);
        assert_eq!(g.world.unit(UnitId(2)).unwrap().state, UnitState::Healthy);
        // Without money nothing is repaired.
        g.money.insert(PlayerId(0), 0);
        for u in g.world.units_mut() {
            u.unsupplied_turns = 0; // keep K-134 attrition out of this
        }
        end_of_turn(&mut g, &cat, &r);
        assert!(near(hp_share(&g, 2), 0.7));
    }

    #[test]
    fn k181_engineers_build_structures_in_own_regions_and_must_stay() {
        let (cat, r) = (catalog(), rules());
        let mut g = game(
            flat(),
            vec![
                at(1, 0, "engineer", 1000.0, 1.0),
                at(2, 0, "engineer", 9000.0, 1.0), // in the enemy's region
                at(3, 0, "mbt", 1100.0, 0.5),
                at(4, 1, "infantry", 9500.0, 1.0),
            ],
        );
        g.zones = vec![zone(1, 1000.0, 0), zone(2, 9000.0, 1)];
        let bunker = cat.get("bunker").unwrap();
        g.money.insert(PlayerId(0), bunker.cost + 10);
        let reasons = plan(
            &mut g,
            vec![
                build(2, "bunker"),   // not our region
                build(3, "bunker"),   // not an engineer
                build(1, "airfield"), // engineers do not build those
                build(1, "bunker"),
            ],
        );
        assert_eq!(reasons, vec![RejectReason::CannotBuild; 3]);
        assert_eq!(g.money[&PlayerId(0)], 10, "paid when work starts");
        let site = g.world.unit(UnitId(5)).unwrap().clone();
        assert_eq!(site.type_id, r.engineer.site_type);
        assert_eq!(
            (site.owner, site.pos),
            (PlayerId(0), Vec2::new(1000.0, 1000.0))
        );
        assert_eq!(
            plan(&mut g, vec![build(1, "bunker")]),
            [RejectReason::Buy(crate::economy::BuyError::NoMoney)]
        );
        let left = |g: &Game| {
            g.world
                .unit(UnitId(5))
                .unwrap()
                .building
                .clone()
                .unwrap()
                .turns_left
        };
        assert_eq!(left(&g), bunker.build_turns);
        end_of_turn(&mut g, &cat, &r);
        assert_eq!(left(&g), bunker.build_turns - 1);
        assert_eq!(hp_share(&g, 3), 0.5, "a building engineer does not repair");
        // The engineer leaves: work waits. It is killed: another one finishes the job.
        g.world.unit_mut(UnitId(1)).unwrap().pos.x = 3000.0;
        end_of_turn(&mut g, &cat, &r);
        assert_eq!(left(&g), bunker.build_turns - 1);
        g.world.unit_mut(UnitId(1)).unwrap().state = UnitState::Destroyed;
        g.world.unit_mut(UnitId(2)).unwrap().pos.x = 1050.0;
        for _ in 1..bunker.build_turns {
            assert_eq!(
                g.world.unit(UnitId(5)).unwrap().type_id,
                r.engineer.site_type
            );
            end_of_turn(&mut g, &cat, &r);
        }
        let done = g.world.unit(UnitId(5)).unwrap();
        assert_eq!((done.type_id.as_str(), done.hp), ("bunker", bunker.hp));
        assert!(done.building.is_none());
    }

    /// Blue infantry 1 (and 2, 3 with `crowd`) at a bunker if `bunker`; a red tank 800 m off.
    fn position(bunker: bool, crowd: bool) -> Game {
        let mut units = vec![
            unit(1, 0, "infantry", 1000.0, 1000.0),
            unit(9, 1, "mbt", 1800.0, 1000.0),
        ];
        if bunker {
            units.push(at(8, 0, "bunker", 990.0, 1.0));
        }
        if crowd {
            units.push(unit(2, 0, "infantry", 1000.0, 1010.0));
            units.push(unit(3, 0, "infantry", 1000.0, 990.0));
        }
        game(flat(), units)
    }

    #[test]
    fn k182_dug_in_infantry_is_hard_to_hurt_and_to_suppress() {
        let (cat, r) = (catalog(), rules());
        // Two units fit; the third (highest id) stays outside.
        let ids = |g: &Game| dug_in(&g.world, &cat, &r).into_iter().collect::<Vec<_>>();
        assert_eq!(ids(&position(true, true)), [UnitId(1), UnitId(2)]);
        assert!(ids(&position(false, true)).is_empty());

        // Direct fire: the first hit on infantry 1 with and without the position.
        let first_hit = |mut g: Game| {
            for _ in 0..200 {
                for e in step_tick(&mut g, &cat, &r) {
                    if let Event::UnitDamaged {
                        unit: UnitId(1),
                        hp,
                        ..
                    } = e
                    {
                        return (100.0 - hp, g.world.unit(UnitId(1)).unwrap().suppression);
                    }
                }
            }
            panic!("the tank never hit infantry 1");
        };
        let (open, open_supp) = first_hit(position(false, false));
        let (dug, dug_supp) = first_hit(position(true, false));
        assert!(open > 0.0);
        assert!((dug - open * r.engineer.shelter_direct_factor).abs() < 1e-9);
        assert!((dug_supp - open_supp * r.engineer.shelter_suppression_factor).abs() < 1e-9);

        // Shells: the same round on the same spot.
        let shelled = |mut g: Game| {
            let mut shells = vec![Shell {
                owner: PlayerId(1),
                impact_tick: 0,
                pos: Vec2::new(1020.0, 1000.0),
                radius_m: 60.0,
                damage_hp: 50.0,
                penetration_mm: 0.0,
                homing: None,
            }];
            step_impacts(&mut g.world, &cat, &r, &mut shells);
            100.0 - g.world.unit(UnitId(1)).unwrap().hp
        };
        let (open, dug) = (
            shelled(position(false, false)),
            shelled(position(true, false)),
        );
        assert!(open > 0.0);
        assert!((dug - open * r.engineer.shelter_shell_factor).abs() < 1e-9);
    }
}
