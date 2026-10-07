// SPDX-License-Identifier: AGPL-3.0-or-later
//! Data-driven game definitions loaded from `data/*.toml`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitClass {
    Infantry,
    Vehicle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitType {
    pub id: String,
    pub name_tr: String,
    pub class: UnitClass,
    pub speed_offroad_mps: f64,
    pub max_slope_deg: f64,
    pub armor_front_mm: f64,
    pub armor_side_mm: f64,
    pub armor_rear_mm: f64,
    pub weapon_range_m: f64,
    pub penetration_mm: f64,
    pub damage_hp: f64,
    pub reload_ticks: u32,
    pub sensor_optical_range_m: f64,
    pub hp: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DataError {
    Parse(String),
    Invalid { unit: String, field: &'static str },
    DuplicateId(String),
}

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataError::Parse(msg) => write!(f, "parse error: {msg}"),
            DataError::Invalid { unit, field } => {
                write!(f, "unit '{unit}': field '{field}' must be finite and >= 0")
            }
            DataError::DuplicateId(id) => write!(f, "duplicate unit id '{id}'"),
        }
    }
}

impl std::error::Error for DataError {}

/// All unit types, keyed by `id` (sorted, deterministic iteration).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct UnitCatalog {
    pub types: BTreeMap<String, UnitType>,
}

impl UnitCatalog {
    pub fn get(&self, id: &str) -> Option<&UnitType> {
        self.types.get(id)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UnitsFile {
    unit: Vec<UnitType>,
}

/// Parses and validates the contents of `units.toml`.
pub fn load_units(toml_text: &str) -> Result<UnitCatalog, DataError> {
    let file: UnitsFile = toml::from_str(toml_text).map_err(|e| DataError::Parse(e.to_string()))?;
    let mut types = BTreeMap::new();
    for t in file.unit {
        validate(&t)?;
        if types.contains_key(&t.id) {
            return Err(DataError::DuplicateId(t.id));
        }
        types.insert(t.id.clone(), t);
    }
    Ok(UnitCatalog { types })
}

fn validate(t: &UnitType) -> Result<(), DataError> {
    let fields = [
        ("speed_offroad_mps", t.speed_offroad_mps),
        ("max_slope_deg", t.max_slope_deg),
        ("armor_front_mm", t.armor_front_mm),
        ("armor_side_mm", t.armor_side_mm),
        ("armor_rear_mm", t.armor_rear_mm),
        ("weapon_range_m", t.weapon_range_m),
        ("penetration_mm", t.penetration_mm),
        ("damage_hp", t.damage_hp),
        ("sensor_optical_range_m", t.sensor_optical_range_m),
        ("hp", t.hp),
    ];
    for (field, v) in fields {
        if !v.is_finite() || v < 0.0 {
            return Err(DataError::Invalid {
                unit: t.id.clone(),
                field,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNITS: &str = include_str!("../../data/units.toml");

    #[test]
    fn g1_loads_shipped_units_toml() {
        let cat = load_units(UNITS).unwrap();
        assert_eq!(cat.types.keys().collect::<Vec<_>>(), ["infantry", "mbt"]);
        assert_eq!(cat.get("infantry").unwrap().weapon_range_m, 400.0);
        assert_eq!(cat.get("mbt").unwrap().weapon_range_m, 2000.0);
        assert_eq!(cat.get("mbt").unwrap().class, UnitClass::Vehicle);
    }

    #[test]
    fn g1_rejects_negative_value() {
        let bad = UNITS.replacen("hp = 100.0", "hp = -1.0", 1);
        assert_eq!(
            load_units(&bad),
            Err(DataError::Invalid {
                unit: "infantry".into(),
                field: "hp"
            })
        );
    }

    #[test]
    fn g1_rejects_missing_field() {
        let bad = UNITS.replacen("weapon_range_m = 400.0", "", 1);
        assert!(
            matches!(load_units(&bad), Err(DataError::Parse(m)) if m.contains("weapon_range_m"))
        );
    }

    #[test]
    fn g1_rejects_duplicate_id() {
        let bad = UNITS.replacen("id = \"mbt\"", "id = \"infantry\"", 1);
        assert_eq!(
            load_units(&bad),
            Err(DataError::DuplicateId("infantry".into()))
        );
    }
}
