// SPDX-License-Identifier: AGPL-3.0-or-later
//! K-130…K-134: supply networks, stocks and production, carriers, region supply and the
//! attrition of units that are cut off. Everything here runs at the end of a turn.

use crate::data::{Rules, UnitCatalog, UnitClass, UnitType};
use crate::logistics::Zone;
use crate::turn::Game;
use crate::world::{HeightMap, UnitId, UnitState, Vec2, World};
use std::collections::{BTreeMap, BTreeSet};

/// Index of the zone a position lies in: by the region grid, or by the capture circles
/// when the map has none there.
pub fn zone_at(map: &HeightMap, zones: &[Zone], pos: Vec2) -> Option<usize> {
    match map.region_at(pos) {
        0 => zones
            .iter()
            .position(|z| pos.distance(z.center) <= z.radius_m),
        region => zones.iter().position(|z| z.id == region),
    }
}

fn has_stock_room(t: &UnitType) -> bool {
    t.supply_fuel_l > 0.0 || t.supply_ammo > 0
}

/// Storage: a structure that holds stock (logistics base, depot, airfield, port).
fn is_storage(t: &UnitType) -> bool {
    t.class == UnitClass::Structure && has_stock_room(t)
}

/// The storage structures that share their stock.
#[derive(Debug, Default)]
struct Pool {
    /// Ascending id order.
    storages: Vec<UnitId>,
    /// A logistics base feeds this pool.
    has_base: bool,
}

/// K-130: the supply networks at the end of a turn. A network is a set of regions of one
/// player joined by road links; its pool is every standing storage structure of that
/// player inside it. A storage outside its owner's regions is a pool of its own.
#[derive(Debug)]
pub struct Networks {
    /// Pool per zone (same order as `Game::zones`); None for neutral zones.
    zone_pool: Vec<Option<usize>>,
    storage_pool: BTreeMap<UnitId, usize>,
    pools: Vec<Pool>,
}

impl Networks {
    pub fn new(world: &World, zones: &[Zone], catalog: &UnitCatalog) -> Self {
        let mut zone_pool: Vec<Option<usize>> = vec![None; zones.len()];
        let mut pools: Vec<Pool> = Vec::new();
        for (start, first) in zones.iter().enumerate() {
            if first.owner.is_none() || zone_pool.get(start).copied().flatten().is_some() {
                continue;
            }
            let pool = pools.len();
            pools.push(Pool::default());
            let mut todo = vec![first];
            if let Some(slot) = zone_pool.get_mut(start) {
                *slot = Some(pool);
            }
            while let Some(from) = todo.pop() {
                for (n, z) in zones.iter().enumerate() {
                    let linked = from.links.contains(&z.id) || z.links.contains(&from.id);
                    if let Some(slot) = zone_pool.get_mut(n) {
                        if linked && z.owner == first.owner && slot.is_none() {
                            *slot = Some(pool);
                            todo.push(z);
                        }
                    }
                }
            }
        }
        let mut storage_pool = BTreeMap::new();
        for u in world.units() {
            let Some(t) = catalog
                .get(&u.type_id)
                .filter(|t| is_storage(t) && u.state != UnitState::Destroyed)
            else {
                continue;
            };
            let home = zone_at(&world.map, zones, u.pos)
                .filter(|&z| zones.get(z).is_some_and(|z| z.owner == Some(u.owner)))
                .and_then(|z| zone_pool.get(z).copied().flatten());
            let pool = home.unwrap_or_else(|| {
                pools.push(Pool::default());
                pools.len() - 1
            });
            if let Some(p) = pools.get_mut(pool) {
                p.storages.push(u.id);
                p.has_base |= t.produce_fuel_l > 0.0 || t.produce_ammo > 0;
            }
            storage_pool.insert(u.id, pool);
        }
        Self {
            zone_pool,
            storage_pool,
            pools,
        }
    }

    fn storages(&self, pool: usize) -> &[UnitId] {
        self.pools.get(pool).map_or(&[], |p| &p.storages)
    }

    /// Fuel and ammunition left in a pool.
    fn stock(&self, world: &World, pool: usize) -> (f64, u32) {
        self.storages(pool)
            .iter()
            .filter_map(|&id| world.unit(id))
            .fold((0.0, 0), |(f, a), s| (f + s.supply_fuel, a + s.supply_ammo))
    }

    /// True if units of the owner standing in this zone are in supply (K-133).
    pub fn zone_supplied(&self, world: &World, zone: usize) -> bool {
        self.zone_pool
            .get(zone)
            .copied()
            .flatten()
            .is_some_and(|pool| {
                let (fuel, ammo) = self.stock(world, pool);
                fuel > 0.0 || ammo > 0
            })
    }

    /// Takes up to the wanted amounts out of a pool, storages in id order.
    fn draw(&self, world: &mut World, pool: usize, fuel: f64, ammo: u32) -> (f64, u32) {
        let (mut fuel_left, mut ammo_left) = (fuel.max(0.0), ammo);
        for &id in self.storages(pool) {
            let Some(s) = world.unit_mut(id) else {
                continue;
            };
            let (f, a) = (fuel_left.min(s.supply_fuel), ammo_left.min(s.supply_ammo));
            s.supply_fuel -= f;
            s.supply_ammo -= a;
            fuel_left -= f;
            ammo_left -= a;
        }
        (fuel.max(0.0) - fuel_left, ammo - ammo_left)
    }

    /// Puts stock into a pool: `first` up to its capacity, then the other storages in id
    /// order. What does not fit is lost.
    fn store(
        &self,
        world: &mut World,
        catalog: &UnitCatalog,
        pool: usize,
        first: UnitId,
        (mut fuel, mut ammo): (f64, u32),
    ) {
        let rest = self
            .storages(pool)
            .iter()
            .copied()
            .filter(|&id| id != first);
        for id in std::iter::once(first).chain(rest) {
            let Some(s) = world.unit_mut(id) else {
                continue;
            };
            let Some(t) = catalog.get(&s.type_id) else {
                continue;
            };
            let f = fuel.min((t.supply_fuel_l - s.supply_fuel).max(0.0));
            let a = ammo.min(t.supply_ammo.saturating_sub(s.supply_ammo));
            s.supply_fuel += f;
            s.supply_ammo += a;
            fuel -= f;
            ammo -= a;
        }
    }
}

fn type_of<'c>(world: &World, catalog: &'c UnitCatalog, id: UnitId) -> Option<&'c UnitType> {
    world.unit(id).and_then(|u| catalog.get(&u.type_id))
}

/// The supply step of the end of a turn: production, carriers, resupply at suppliers,
/// region supply, attrition.
pub fn step_supply(game: &mut Game, catalog: &UnitCatalog, rules: &Rules) {
    let nets = Networks::new(&game.world, &game.zones, catalog);
    let world = &mut game.world;
    let radius = rules.supply.radius_m;
    let standing: Vec<UnitId> = world
        .units()
        .iter()
        .filter(|u| u.state != UnitState::Destroyed)
        .map(|u| u.id)
        .collect();

    // K-131: logistics bases produce into their pool.
    for &id in &standing {
        let Some(t) = type_of(world, catalog, id) else {
            continue;
        };
        if let Some(&pool) = nets.storage_pool.get(&id) {
            if t.produce_fuel_l > 0.0 || t.produce_ammo > 0 {
                let made = (t.produce_fuel_l, t.produce_ammo);
                nets.store(world, catalog, pool, id, made);
            }
        }
    }

    // K-132: a carrier next to a storage loads from a pool that has a logistics base and
    // unloads into one that is cut off from every base.
    for &id in &standing {
        let Some(t) = type_of(world, catalog, id).filter(|t| has_stock_room(t) && !is_storage(t))
        else {
            continue;
        };
        let Some(c) = world.unit(id) else { continue };
        let (owner, pos, fuel, ammo) = (c.owner, c.pos, c.supply_fuel, c.supply_ammo);
        let near = world.units().iter().find_map(|s| {
            let pool = nets.storage_pool.get(&s.id)?;
            (s.owner == owner && s.pos.distance(pos) <= radius).then_some((s.id, *pool))
        });
        let Some((storage, pool)) = near else {
            continue;
        };
        if nets.pools.get(pool).is_some_and(|p| p.has_base) {
            let want = (t.supply_fuel_l - fuel, t.supply_ammo.saturating_sub(ammo));
            let (f, a) = nets.draw(world, pool, want.0, want.1);
            if let Some(c) = world.unit_mut(id) {
                c.supply_fuel += f;
                c.supply_ammo += a;
            }
        } else {
            let before = nets.stock(world, pool);
            nets.store(world, catalog, pool, storage, (fuel, ammo));
            let after = nets.stock(world, pool);
            if let Some(c) = world.unit_mut(id) {
                c.supply_fuel -= after.0 - before.0;
                c.supply_ammo -= after.1 - before.1;
            }
        }
    }

    // K-132: carriers and storages fill up the friendly units in reach (id order; the
    // supplier's own tank included).
    let mut served: BTreeSet<UnitId> = BTreeSet::new();
    for &sid in &standing {
        let Some(s) = world.unit(sid) else { continue };
        if !type_of(world, catalog, sid).is_some_and(has_stock_room)
            || (s.supply_fuel <= 0.0 && s.supply_ammo == 0)
        {
            continue;
        }
        let (owner, pos) = (s.owner, s.pos);
        let needs: Vec<(UnitId, f64, u32)> = world
            .units()
            .iter()
            .filter(|u| u.owner == owner && u.state != UnitState::Destroyed)
            .filter(|u| u.pos.distance(pos) <= radius)
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
            served.insert(uid);
            let Some(s) = world.unit_mut(sid) else {
                break;
            };
            let fuel = fuel_need.min(s.supply_fuel);
            let ammo = ammo_need.min(s.supply_ammo);
            s.supply_fuel -= fuel;
            s.supply_ammo -= ammo;
            if let Some(u) = world.unit_mut(uid) {
                u.fuel += fuel;
                u.ammo += ammo;
            }
        }
    }

    // K-133: units in an own region whose network still has stock get a share of their
    // capacity each turn.
    let share = rules.supply.region_share;
    for &id in &standing {
        let Some((u, t)) = world.unit(id).zip(type_of(world, catalog, id)) else {
            continue;
        };
        let pool = zone_at(&world.map, &game.zones, u.pos)
            .filter(|&z| game.zones.get(z).is_some_and(|z| z.owner == Some(u.owner)))
            .and_then(|z| nets.zone_pool.get(z).copied().flatten());
        let Some(pool) = pool else { continue };
        let (in_fuel, in_ammo) = nets.stock(world, pool);
        if in_fuel <= 0.0 && in_ammo == 0 {
            continue;
        }
        served.insert(id);
        let fuel = (t.fuel_l - u.fuel).min(t.fuel_l * share);
        let ammo = t
            .ammo
            .saturating_sub(u.ammo)
            .min((f64::from(t.ammo) * share).ceil() as u32);
        let (fuel, ammo) = nets.draw(world, pool, fuel, ammo);
        if let Some(u) = world.unit_mut(id) {
            u.fuel += fuel;
            u.ammo += ammo;
        }
    }

    // K-134: ground units nobody could supply wither away.
    for u in world.units_mut() {
        let Some(t) = catalog.get(&u.type_id).filter(|t| {
            matches!(t.class, UnitClass::Infantry | UnitClass::Vehicle)
                && u.state != UnitState::Destroyed
                && u.carried_by.is_none() // passengers share their carrier's fate
        }) else {
            continue;
        };
        if served.contains(&u.id) {
            u.unsupplied_turns = 0;
            continue;
        }
        u.unsupplied_turns += 1;
        if u.unsupplied_turns >= rules.supply.attrition_after_turns {
            u.hp = (u.hp - t.hp * rules.supply.attrition_hp_share).max(0.0);
            if u.hp <= 0.0 {
                u.state = UnitState::Destroyed;
            } else if u.hp <= t.hp * rules.combat.damaged_hp_fraction {
                u.state = UnitState::Damaged;
            }
        }
    }
}
