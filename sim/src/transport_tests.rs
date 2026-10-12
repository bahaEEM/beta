// SPDX-License-Identifier: AGPL-3.0-or-later
//! K-110…K-113: infantry riding in IFVs.

use crate::event::Event;
use crate::indirect::FireTarget;
use crate::turn::tests::{catalog, game, rules, unit};
use crate::turn::{plan_phase, step_tick, validate_orders, Order, RejectReason};
use crate::world::{HeightMap, PlayerId, UnitId, UnitState, Vec2};
use std::collections::BTreeMap;

fn flat() -> HeightMap {
    HeightMap::new(400, 200, 50.0, vec![0.0; 80_000]).unwrap()
}

fn order(g: &mut crate::turn::Game, o: Order) -> Vec<crate::turn::Rejection> {
    let owner = g.world.unit(o.units()[0]).unwrap().owner;
    plan_phase(g, &catalog(), &rules(), &BTreeMap::from([(owner, vec![o])]))
        .remove(&owner)
        .unwrap()
}

#[test]
fn k110_infantry_boards_nearby_carrier_and_rides_along() {
    let mut g = game(
        flat(),
        vec![
            unit(1, 0, "ifv", 1000.0, 1000.0),
            unit(2, 0, "infantry", 1050.0, 1000.0),
        ],
    );
    assert!(order(
        &mut g,
        Order::Embark {
            units: vec![UnitId(2)],
            carrier: UnitId(1)
        }
    )
    .is_empty());
    assert_eq!(g.world.unit(UnitId(2)).unwrap().carried_by, Some(UnitId(1)));
    order(
        &mut g,
        Order::Move {
            units: vec![UnitId(1)],
            waypoints: vec![Vec2::new(5000.0, 1000.0)],
        },
    );
    for _ in 0..100 {
        step_tick(&mut g, &catalog(), &rules());
    }
    let (c, p) = (
        g.world.unit(UnitId(1)).unwrap().pos,
        g.world.unit(UnitId(2)).unwrap().pos,
    );
    assert!(c.x > 1500.0 && c == p, "passenger moves with the carrier");
    assert!(order(
        &mut g,
        Order::Disembark {
            units: vec![UnitId(2)]
        }
    )
    .is_empty());
    assert_eq!(g.world.unit(UnitId(2)).unwrap().carried_by, None);
}

#[test]
fn k110_boarding_limits() {
    let g = game(
        flat(),
        vec![
            unit(1, 0, "ifv", 1000.0, 1000.0),
            unit(2, 0, "infantry", 1050.0, 1000.0),
            unit(3, 0, "infantry", 1060.0, 1000.0),
            unit(4, 0, "infantry", 3000.0, 1000.0),
            unit(5, 0, "mbt", 1040.0, 1000.0),
        ],
    );
    let bad = |units: Vec<u32>| {
        let o = Order::Embark {
            units: units.into_iter().map(UnitId).collect(),
            carrier: UnitId(1),
        };
        validate_orders(&g, &catalog(), &rules(), PlayerId(0), &[o])
            .rejected
            .first()
            .map(|r| r.reason.clone())
    };
    assert_eq!(
        bad(vec![2, 3]),
        Some(RejectReason::CannotEmbark),
        "capacity 1"
    );
    assert_eq!(bad(vec![4]), Some(RejectReason::CannotEmbark), "too far");
    assert_eq!(
        bad(vec![5]),
        Some(RejectReason::CannotEmbark),
        "only infantry"
    );
    assert_eq!(bad(vec![2]), None);
}

#[test]
fn k111_passengers_are_hidden_protected_and_take_no_orders() {
    let mut g = game(
        flat(),
        vec![
            unit(1, 0, "ifv", 10000.0, 5000.0),
            unit(2, 0, "infantry", 10000.0, 5000.0),
            unit(9, 1, "spg", 1000.0, 5000.0),
            unit(8, 1, "infantry", 10500.0, 5000.0),
        ],
    );
    order(
        &mut g,
        Order::Embark {
            units: vec![UnitId(2)],
            carrier: UnitId(1),
        },
    );
    let r = order(
        &mut g,
        Order::Move {
            units: vec![UnitId(2)],
            waypoints: vec![Vec2::new(0.0, 0.0)],
        },
    );
    assert_eq!(r[0].reason, RejectReason::Embarked);
    order(
        &mut g,
        Order::FireMission {
            units: vec![UnitId(9)],
            target: FireTarget::Point(Vec2::new(10000.0, 5000.0)),
        },
    );
    for _ in 0..150 {
        // lay 100 ticks + ~38 ticks flight: exactly one round has landed
        step_tick(&mut g, &catalog(), &rules());
    }
    assert!(
        g.tracks
            .get(PlayerId(1), UnitId(2))
            .is_none_or(|t| t.quality == 0),
        "enemy never sees the passenger"
    );
    let ifv_hp = g.world.unit(UnitId(1)).unwrap().hp;
    assert!(ifv_hp < 100.0, "the carrier is hit");
    assert_eq!(
        g.world.unit(UnitId(2)).unwrap().hp,
        100.0,
        "the passenger is not"
    );
}

#[test]
fn k112_carrier_destroyed_passengers_bail_out_hurt() {
    let mut g = game(
        flat(),
        vec![
            unit(1, 0, "ifv", 1000.0, 1000.0),
            unit(2, 0, "infantry", 1000.0, 1000.0),
        ],
    );
    order(
        &mut g,
        Order::Embark {
            units: vec![UnitId(2)],
            carrier: UnitId(1),
        },
    );
    g.world.unit_mut(UnitId(1)).unwrap().state = UnitState::Destroyed;
    let ev = step_tick(&mut g, &catalog(), &rules());
    let p = g.world.unit(UnitId(2)).unwrap();
    assert_eq!(p.carried_by, None);
    assert_eq!(p.hp, 100.0 * (1.0 - rules().transport.bail_out_damage));
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::UnitDamaged {
            unit: UnitId(2),
            ..
        }
    )));
}
