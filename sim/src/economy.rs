// SPDX-License-Identifier: AGPL-3.0-or-later
//! K-170…K-173: income from supplied regions, buying units, production at structures.

use crate::data::{Rules, UnitCatalog, UnitClass, UnitType};
use crate::supply::Networks;
use crate::turn::Game;
use crate::world::{PlayerId, Terrain, Unit, UnitId, UnitState, Vec2, World};
use serde::{Deserialize, Serialize};

/// A unit that was paid for and is being raised at a structure (K-172).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Purchase {
    pub player: PlayerId,
    pub type_id: String,
    /// The structure the unit will appear at.
    pub site: UnitId,
    /// Ends of turn still to go.
    pub turns_left: u32,
}

/// Why a purchase is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuyError {
    /// Unknown type or one that is not for sale.
    NotForSale,
    /// Not a standing structure of the player that raises this class of unit, or (for
    /// ships) no water in reach.
    BadSite,
    NoMoney,
}

/// Nearest map sample of the wanted kind within `reach` of `pos` (ties: north, then west).
pub fn nearest_cell(world: &World, pos: Vec2, reach: f64, water: bool) -> Option<Vec2> {
    let map = &world.map;
    let cell = map.cell_size_m();
    let r = (reach / cell).ceil() as i64;
    let (ci, cj) = ((pos.x / cell).round() as i64, (pos.y / cell).round() as i64);
    let mut best: Option<(f64, Vec2)> = None;
    for j in (cj - r)..=(cj + r) {
        for i in (ci - r)..=(ci + r) {
            if i < 0 || j < 0 || i as usize >= map.width() || j as usize >= map.height() {
                continue;
            }
            let p = Vec2::new(i as f64 * cell, j as f64 * cell);
            let d = p.distance(pos);
            let wet = map.terrain(i as usize, j as usize) == Terrain::Water;
            if wet == water && d <= reach && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, p));
            }
        }
    }
    best.map(|(_, p)| p)
}

/// Where a unit of type `t` bought at `site` appears: at the structure, ships on the
/// nearest water. None if `site` cannot raise it for `player`.
pub fn spawn_point(
    game: &Game,
    catalog: &UnitCatalog,
    rules: &Rules,
    player: PlayerId,
    t: &UnitType,
    site: UnitId,
) -> Option<Vec2> {
    let s = game
        .world
        .unit(site)
        .filter(|s| s.owner == player && s.state != UnitState::Destroyed)?;
    catalog
        .get(&s.type_id)
        .filter(|st| st.class == UnitClass::Structure && st.produces.contains(&t.class))?;
    if t.class.is_naval() {
        nearest_cell(&game.world, s.pos, rules.economy.launch_radius_m, true)
    } else {
        Some(s.pos)
    }
}

/// K-171: checks a purchase against the money the player has left; returns the price.
pub fn check_buy(
    game: &Game,
    catalog: &UnitCatalog,
    rules: &Rules,
    player: PlayerId,
    type_id: &str,
    site: UnitId,
    money_left: u32,
) -> Result<u32, BuyError> {
    let t = catalog
        .get(type_id)
        .filter(|t| t.cost > 0)
        .ok_or(BuyError::NotForSale)?;
    spawn_point(game, catalog, rules, player, t, site).ok_or(BuyError::BadSite)?;
    if t.cost > money_left {
        return Err(BuyError::NoMoney);
    }
    Ok(t.cost)
}

/// K-170: what the player's regions pay at the end of this turn if nothing changes.
pub fn income(game: &Game, catalog: &UnitCatalog, player: PlayerId) -> u32 {
    let nets = Networks::new(&game.world, &game.zones, catalog);
    game.zones
        .iter()
        .enumerate()
        .filter(|(k, z)| z.owner == Some(player) && nets.zone_supplied(&game.world, *k))
        .map(|(_, z)| z.income)
        .sum()
}

/// End of turn, after supply: regions pay (K-170), then units under way come one turn
/// closer and the finished ones appear (K-172). Orders whose structure is gone or in
/// enemy hands are lost with their money (K-173).
pub fn step_economy(game: &mut Game, catalog: &UnitCatalog, rules: &Rules) {
    let players: Vec<PlayerId> = game.money.keys().copied().collect();
    for p in players {
        let gain = income(game, catalog, p);
        if let Some(m) = game.money.get_mut(&p) {
            *m = m.saturating_add(gain);
        }
    }

    let mut next_id = game.world.units().last().map_or(1, |u| u.id.0 + 1);
    let mut still = Vec::new();
    for mut order in std::mem::take(&mut game.production) {
        let Some(t) = catalog.get(&order.type_id) else {
            continue;
        };
        let Some(pos) = spawn_point(game, catalog, rules, order.player, t, order.site) else {
            continue; // K-173
        };
        order.turns_left = order.turns_left.saturating_sub(1);
        if order.turns_left > 0 {
            still.push(order);
            continue;
        }
        let unit = Unit::new(UnitId(next_id), order.player, t, pos, 0.0);
        if game.world.add_unit(unit).is_ok() {
            next_id += 1;
        }
    }
    game.production = still;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logistics::{end_of_turn, Zone};
    use crate::turn::tests::{catalog, game, rules, unit};
    use crate::turn::{plan_phase, validate_orders, Order, RejectReason};
    use crate::world::HeightMap;
    use std::collections::BTreeMap;

    /// 20 x 10 km, sea east of x = 10 km.
    fn coast() -> HeightMap {
        let terrain = (0..80_000)
            .map(|k| if k % 400 < 200 { 0 } else { 3 })
            .collect();
        HeightMap::new(400, 200, 50.0, vec![0.0; 80_000])
            .unwrap()
            .with_terrain(terrain)
            .unwrap()
    }

    fn zone(id: u32, x: f64, owner: u8, links: &[u32]) -> Zone {
        Zone {
            id,
            name_tr: format!("Bölge {id}"),
            center: Vec2::new(x, 1000.0),
            radius_m: 500.0,
            links: links.to_vec(),
            owner: Some(PlayerId(owner)),
            base: None,
            income: 20,
            progress: None,
        }
    }

    /// A stocked structure.
    fn site(id: u32, owner: u8, kind: &str, x: f64) -> Unit {
        let mut u = unit(id, owner, kind, x, 1000.0);
        (u.supply_fuel, u.supply_ammo) = (1000.0, 100);
        u
    }

    fn buy(type_id: &str, site: u32) -> Order {
        Order::Buy {
            type_id: type_id.into(),
            site: UnitId(site),
        }
    }

    fn plan(g: &mut Game, player: u8, orders: Vec<Order>) -> Vec<RejectReason> {
        plan_phase(
            g,
            &catalog(),
            &rules(),
            &BTreeMap::from([(PlayerId(player), orders)]),
        )
        .remove(&PlayerId(player))
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.reason)
        .collect()
    }

    #[test]
    fn k170_only_supplied_own_regions_pay() {
        // Blue: zones 1-2 linked with a base in 1; zone 3 is cut off and has no stock.
        // Red holds zone 4 without any stock.
        let mut g = game(
            coast(),
            vec![
                site(1, 0, "logistics_base", 1000.0),
                unit(2, 1, "infantry", 9000.0, 9000.0),
            ],
        );
        g.zones = vec![
            zone(1, 1000.0, 0, &[2]),
            zone(2, 3000.0, 0, &[]),
            zone(3, 5000.0, 0, &[]),
            zone(4, 7000.0, 1, &[]),
        ];
        let start = rules().economy.start_money;
        assert_eq!(g.money[&PlayerId(0)], start);
        assert_eq!(income(&g, &catalog(), PlayerId(0)), 40);
        end_of_turn(&mut g, &catalog(), &rules());
        assert_eq!(g.money[&PlayerId(0)], start + 40);
        assert_eq!(g.money[&PlayerId(1)], start, "no stock, no income");
    }

    #[test]
    fn k171_buying_needs_money_and_the_right_structure() {
        let mut g = game(
            coast(),
            vec![
                site(1, 0, "logistics_base", 1000.0),
                site(2, 0, "port", 9800.0),
                site(3, 0, "port", 3000.0), // too far from the sea
                site(4, 1, "logistics_base", 5000.0),
                unit(5, 0, "mbt", 1000.0, 1200.0),
            ],
        );
        let price = |id: &str| catalog().get(id).unwrap().cost;
        g.money
            .insert(PlayerId(0), price("mbt") + 2 * price("infantry"));
        let orders = vec![
            buy("mbt", 1),
            buy("infantry", 1),
            buy("infantry", 1),       // nothing left after this one
            buy("decoy_tank", 1),     // no money
            buy("mbt", 2),            // a port raises no tanks
            buy("patrol_boat", 3),    // no water in reach
            buy("infantry", 4),       // not ours
            buy("infantry", 5),       // not a structure
            buy("logistics_base", 1), // not for sale
        ];
        let v = validate_orders(&g, &catalog(), &rules(), PlayerId(0), &orders);
        assert_eq!(v.purchases.len(), 3);
        assert_eq!(v.kbg_spent, 0, "buying costs no command points");
        let reasons: Vec<_> = v.rejected.iter().map(|r| r.reason.clone()).collect();
        use BuyError::*;
        assert_eq!(
            reasons,
            [NoMoney, BadSite, BadSite, BadSite, BadSite, NotForSale].map(RejectReason::Buy)
        );
        assert!(plan(&mut g, 0, orders).len() == 6);
        assert_eq!(g.money[&PlayerId(0)], 0, "paid up front");
        assert_eq!(g.production.len(), 3);
        // A port by the sea does build boats.
        g.money.insert(PlayerId(0), price("patrol_boat"));
        assert!(plan(&mut g, 0, vec![buy("patrol_boat", 2)]).is_empty());
    }

    #[test]
    fn k172_bought_units_appear_at_their_structure_after_the_build_time() {
        let mut g = game(
            coast(),
            vec![
                site(1, 0, "logistics_base", 1000.0),
                site(2, 0, "port", 9800.0),
                unit(7, 1, "infantry", 9000.0, 9000.0),
            ],
        );
        g.money.insert(PlayerId(0), 1000);
        assert!(plan(&mut g, 0, vec![buy("infantry", 1), buy("patrol_boat", 2)]).is_empty());
        let (cat, r) = (catalog(), rules());
        let turns = |id: &str| cat.get(id).unwrap().build_turns;
        assert!(turns("infantry") < turns("patrol_boat"));
        for _ in 1..turns("infantry") {
            end_of_turn(&mut g, &cat, &r);
            assert_eq!(g.world.units().len(), 3, "still under way");
        }
        end_of_turn(&mut g, &cat, &r);
        let inf = g.world.unit(UnitId(8)).unwrap();
        assert_eq!(
            (inf.type_id.as_str(), inf.owner, inf.pos),
            ("infantry", PlayerId(0), Vec2::new(1000.0, 1000.0))
        );
        assert_eq!(inf.hp, cat.get("infantry").unwrap().hp);
        for _ in turns("infantry")..turns("patrol_boat") {
            end_of_turn(&mut g, &cat, &r);
        }
        let boat = g.world.unit(UnitId(9)).unwrap();
        assert_eq!(boat.type_id, "patrol_boat");
        assert_eq!(g.world.map.terrain_at(boat.pos), Terrain::Water);
        assert_eq!(boat.pos, Vec2::new(10_000.0, 1000.0));
        assert!(g.production.is_empty());
    }

    #[test]
    fn k173_orders_are_lost_with_their_structure() {
        let mut g = game(
            coast(),
            vec![
                site(1, 0, "logistics_base", 1000.0),
                site(2, 0, "logistics_base", 3000.0),
                unit(7, 1, "infantry", 9000.0, 9000.0),
            ],
        );
        g.money
            .insert(PlayerId(0), 2 * catalog().get("infantry").unwrap().cost);
        assert!(plan(&mut g, 0, vec![buy("infantry", 1), buy("infantry", 2)]).is_empty());
        // One base falls into enemy hands, the other is destroyed.
        g.world.unit_mut(UnitId(1)).unwrap().owner = PlayerId(1);
        g.world.unit_mut(UnitId(2)).unwrap().state = UnitState::Destroyed;
        for _ in 0..3 {
            end_of_turn(&mut g, &catalog(), &rules());
        }
        assert!(g.production.is_empty());
        assert_eq!(g.world.units().len(), 3, "nothing was delivered");
        assert_eq!(g.money[&PlayerId(0)], 0, "and nothing refunded");
    }
}
