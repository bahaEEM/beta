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
    /// Flies at `height_m` above ground in straight lines; only anti-air weapons can hit it.
    Air,
}

impl UnitClass {
    pub fn is_air(self) -> bool {
        self == UnitClass::Air
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitType {
    pub id: String,
    pub name_tr: String,
    pub class: UnitClass,
    pub speed_offroad_mps: f64,
    pub speed_road_mps: f64,
    pub max_slope_deg: f64,
    pub armor_front_mm: f64,
    pub armor_side_mm: f64,
    pub armor_rear_mm: f64,
    pub weapon_range_m: f64,
    pub penetration_mm: f64,
    pub damage_hp: f64,
    /// Game seconds between shots.
    pub reload_s: f64,
    /// What the direct-fire weapon can engage.
    #[serde(default = "yes")]
    pub weapon_vs_ground: bool,
    #[serde(default)]
    pub weapon_vs_air: bool,
    /// Expected hit share at point blank and at maximum range (linear in between).
    pub hit_close: f64,
    pub hit_far: f64,
    /// Game seconds a target must be held in the sights before the first shot.
    pub aim_s: f64,
    /// Sensor/target height above ground, used for line of sight.
    pub height_m: f64,
    pub hp: f64,
    pub fuel_l: f64,
    pub fuel_l_per_km: f64,
    pub ammo: u32,
    #[serde(default)]
    pub sensors: Vec<SensorSpec>,
    /// Radio reach; two units link if both are within each other's reach.
    pub radio_range_m: f64,
    /// Indirect-fire weapon (artillery, rockets), fired by fire missions.
    #[serde(default)]
    pub indirect: Option<IndirectSpec>,
    /// Infantry teams this unit can carry (IFV).
    #[serde(default)]
    pub carry_capacity: u32,
    /// Command post: the root of its side's command network.
    #[serde(default)]
    pub hq: bool,
    /// Stock a supply unit carries for others.
    #[serde(default)]
    pub supply_fuel_l: f64,
    #[serde(default)]
    pub supply_ammo: u32,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndirectSpec {
    pub range_m: f64,
    pub min_range_m: f64,
    /// Average speed over the trajectory; flight time = distance / speed.
    pub shell_speed_mps: f64,
    /// Damage falls linearly from the impact point to zero at this radius.
    pub radius_m: f64,
    pub damage_hp: f64,
    pub penetration_mm: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SensorSpec {
    Optical {
        range_m: f64,
    },
    /// Active; only works while the unit emits; sees targets moving at least this fast.
    Radar {
        range_m: f64,
        min_target_speed_mps: f64,
        /// Air-search radar (sees aircraft) instead of ground surveillance.
        #[serde(default)]
        air: bool,
    },
    /// Passive; bearings to enemy emitters.
    Esm {
        range_m: f64,
    },
    /// Active; while emitting, degrades enemy radars within `range_m` and blocks enemy
    /// radios within `radio_range_m`.
    Jammer {
        range_m: f64,
        radio_range_m: f64,
    },
}

impl SensorSpec {
    pub fn range_m(&self) -> f64 {
        match *self {
            SensorSpec::Optical { range_m }
            | SensorSpec::Radar { range_m, .. }
            | SensorSpec::Esm { range_m }
            | SensorSpec::Jammer { range_m, .. } => range_m,
        }
    }

    pub fn is_emitter(&self) -> bool {
        matches!(self, SensorSpec::Radar { .. } | SensorSpec::Jammer { .. })
    }
}

impl UnitType {
    /// True if the type has a radar or jammer that can be switched on.
    pub fn is_emitter(&self) -> bool {
        self.sensors.iter().any(SensorSpec::is_emitter)
    }

    pub fn jammer_range_m(&self) -> Option<f64> {
        self.sensors.iter().find_map(|s| match *s {
            SensorSpec::Jammer { range_m, .. } => Some(range_m),
            _ => None,
        })
    }

    pub fn radio_jam_range_m(&self) -> Option<f64> {
        self.sensors.iter().find_map(|s| match *s {
            SensorSpec::Jammer { radio_range_m, .. } => Some(radio_range_m),
            _ => None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DataError {
    Parse(String),
    Invalid { unit: String, field: &'static str },
    DuplicateId(String),
    InvalidRule(&'static str),
}

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataError::Parse(msg) => write!(f, "parse error: {msg}"),
            DataError::Invalid { unit, field } => {
                write!(f, "unit '{unit}': field '{field}' must be finite and >= 0")
            }
            DataError::DuplicateId(id) => write!(f, "duplicate unit id '{id}'"),
            DataError::InvalidRule(field) => write!(f, "rules: invalid value for '{field}'"),
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
        ("speed_road_mps", t.speed_road_mps),
        ("max_slope_deg", t.max_slope_deg),
        ("armor_front_mm", t.armor_front_mm),
        ("armor_side_mm", t.armor_side_mm),
        ("armor_rear_mm", t.armor_rear_mm),
        ("weapon_range_m", t.weapon_range_m),
        ("penetration_mm", t.penetration_mm),
        ("damage_hp", t.damage_hp),
        ("reload_s", t.reload_s),
        ("aim_s", t.aim_s),
        ("height_m", t.height_m),
        ("hp", t.hp),
        ("fuel_l", t.fuel_l),
        ("fuel_l_per_km", t.fuel_l_per_km),
        ("supply_fuel_l", t.supply_fuel_l),
        ("radio_range_m", t.radio_range_m),
    ];
    for (field, v) in fields {
        if !v.is_finite() || v < 0.0 {
            return Err(DataError::Invalid {
                unit: t.id.clone(),
                field,
            });
        }
    }
    for (field, v) in [("hit_close", t.hit_close), ("hit_far", t.hit_far)] {
        if !(0.0..=1.0).contains(&v) {
            return Err(DataError::Invalid {
                unit: t.id.clone(),
                field,
            });
        }
    }
    if let Some(g) = t.indirect {
        let ok = [g.range_m, g.shell_speed_mps, g.radius_m]
            .iter()
            .all(|v| v.is_finite() && *v > 0.0)
            && [g.min_range_m, g.damage_hp, g.penetration_mm]
                .iter()
                .all(|v| v.is_finite() && *v >= 0.0)
            && g.min_range_m < g.range_m;
        if !ok {
            return Err(DataError::Invalid {
                unit: t.id.clone(),
                field: "indirect",
            });
        }
    }
    for s in &t.sensors {
        let extra = match *s {
            SensorSpec::Radar {
                min_target_speed_mps,
                ..
            } => min_target_speed_mps,
            _ => 0.0,
        };
        if !(s.range_m().is_finite() && s.range_m() > 0.0 && extra.is_finite() && extra >= 0.0) {
            return Err(DataError::Invalid {
                unit: t.id.clone(),
                field: "sensors",
            });
        }
    }
    Ok(())
}

/// Game rules from `rules.toml`. One field per top-level section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rules {
    pub sensor: SensorRules,
    pub combat: CombatRules,
    pub kbg: KbgRules,
    pub visibility: VisibilityRules,
    pub turn: TurnRules,
    pub zones: ZoneRules,
    pub supply: SupplyRules,
    pub comms: CommsRules,
    pub transport: TransportRules,
    pub indirect: IndirectRules,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndirectRules {
    /// Track quality needed to fire a mission at an enemy unit.
    pub min_track_quality: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransportRules {
    /// Infantry must be this close to the carrier to embark.
    pub embark_radius_m: f64,
    /// Share of their hp passengers lose when the carrier is destroyed.
    pub bail_out_damage: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommsRules {
    /// A unit this close to a road cell is on the wire network of that road.
    pub wire_reach_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZoneRules {
    /// Game seconds of uncontested infantry presence needed to capture.
    pub capture_s: f64,
    pub max_turns: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupplyRules {
    /// Supply units resupply friendly units this close at the end of each turn.
    pub radius_m: f64,
}

/// Command budget (KBG) per player per turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KbgRules {
    pub budget_per_turn: u32,
    /// Cost of one order per started `units_per_point` units.
    pub order_cost: u32,
    pub units_per_point: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisibilityRules {
    /// Quality-1 contacts are reported as the centre of this grid cell, not the exact position.
    pub contact_cell_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnRules {
    pub planning_secs: u32,
    /// Game time simulated by one tick (200 ticks per turn).
    pub game_seconds_per_tick: f64,
}

impl Rules {
    /// Game seconds -> whole ticks (at least `min`).
    pub fn ticks(&self, seconds: f64, min: u32) -> u32 {
        ((seconds / self.turn.game_seconds_per_tick).ceil() as u32).max(min)
    }

    /// Reload time of a unit type in ticks (at least 1).
    pub fn reload_ticks(&self, t: &UnitType) -> u32 {
        self.ticks(t.reload_s, 1)
    }

    /// Aim time of a unit type in ticks (at least 1: the tick the target is acquired).
    pub fn aim_ticks(&self, t: &UnitType) -> u32 {
        self.ticks(t.aim_s, 1)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatRules {
    /// penetration >= effective armour * this (but below full armour) -> damaged.
    pub partial_pen_threshold: f64,
    /// Share of damage dealt by a partial penetration.
    pub partial_damage_factor: f64,
    /// At or below this share of max hp a unit counts as damaged.
    pub damaged_hp_fraction: f64,
    /// 'return' stance may fire this long (game seconds) after being shot at.
    pub return_fire_s: f64,
    /// Track quality anti-air weapons need on an aircraft (radar lock is enough).
    pub air_fire_quality: u8,
    /// Speed multiplier for damaged units.
    pub damaged_speed_factor: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SensorRules {
    pub optical: OpticalRules,
    pub radar: RadarRules,
    pub esm: EsmRules,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RadarRules {
    /// Track quality a radar gives (position + class).
    pub quality: u8,
    /// Quality lost while an enemy jammer covers the radar.
    pub jam_quality_drop: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EsmRules {
    /// Quality of a single bearing (direction only).
    pub bearing_quality: u8,
    /// Quality when two bearings cross at least `min_cross_deg` (triangulation).
    pub triangulated_quality: u8,
    pub min_cross_deg: f64,
    /// A bearing-only track is re-reported when the bearing moves this much.
    pub bearing_update_deg: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpticalRules {
    /// Contact range = optical range_m * this factor.
    pub track_range_factor: f64,
    /// Track quality inside the optical range.
    pub fire_quality: u8,
    /// Track quality between the optical range and the contact range.
    pub contact_quality: u8,
}

/// Highest track quality (0 none .. 3 fire quality).
pub const MAX_TRACK_QUALITY: u8 = 3;

/// Parses and validates the contents of `rules.toml`.
pub fn load_rules(toml_text: &str) -> Result<Rules, DataError> {
    let rules: Rules = toml::from_str(toml_text).map_err(|e| DataError::Parse(e.to_string()))?;
    let o = &rules.sensor.optical;
    if !(o.track_range_factor.is_finite() && o.track_range_factor >= 1.0) {
        return Err(DataError::InvalidRule("sensor.optical.track_range_factor"));
    }
    if o.fire_quality == 0 || o.fire_quality > MAX_TRACK_QUALITY {
        return Err(DataError::InvalidRule("sensor.optical.fire_quality"));
    }
    if o.contact_quality == 0 || o.contact_quality > o.fire_quality {
        return Err(DataError::InvalidRule("sensor.optical.contact_quality"));
    }
    let s = &rules.sensor;
    if s.radar.quality == 0 || s.radar.quality > MAX_TRACK_QUALITY {
        return Err(DataError::InvalidRule("sensor.radar.quality"));
    }
    if s.esm.bearing_quality == 0 || s.esm.triangulated_quality > MAX_TRACK_QUALITY {
        return Err(DataError::InvalidRule("sensor.esm"));
    }
    if !(s.esm.min_cross_deg > 0.0 && s.esm.min_cross_deg < 90.0 && s.esm.bearing_update_deg > 0.0)
    {
        return Err(DataError::InvalidRule("sensor.esm.min_cross_deg"));
    }
    if !(rules.comms.wire_reach_m.is_finite() && rules.comms.wire_reach_m >= 0.0) {
        return Err(DataError::InvalidRule("comms.wire_reach_m"));
    }
    if !(rules.zones.capture_s > 0.0 && rules.supply.radius_m > 0.0) || rules.zones.max_turns == 0 {
        return Err(DataError::InvalidRule("zones/supply"));
    }
    let c = &rules.combat;
    for (name, v) in [
        ("combat.partial_pen_threshold", c.partial_pen_threshold),
        ("combat.partial_damage_factor", c.partial_damage_factor),
        ("combat.damaged_hp_fraction", c.damaged_hp_fraction),
    ] {
        if !(v > 0.0 && v <= 1.0) {
            return Err(DataError::InvalidRule(name));
        }
    }
    if c.air_fire_quality == 0 || c.air_fire_quality > MAX_TRACK_QUALITY {
        return Err(DataError::InvalidRule("combat.air_fire_quality"));
    }
    if !(c.return_fire_s.is_finite() && c.return_fire_s >= 0.0) {
        return Err(DataError::InvalidRule("combat.return_fire_s"));
    }
    if !(c.damaged_speed_factor > 0.0 && c.damaged_speed_factor <= 1.0) {
        return Err(DataError::InvalidRule("combat.damaged_speed_factor"));
    }
    if rules.kbg.order_cost == 0 {
        return Err(DataError::InvalidRule("kbg.order_cost"));
    }
    if rules.kbg.units_per_point == 0 {
        return Err(DataError::InvalidRule("kbg.units_per_point"));
    }
    let cell = rules.visibility.contact_cell_m;
    if !(cell.is_finite() && cell > 0.0) {
        return Err(DataError::InvalidRule("visibility.contact_cell_m"));
    }
    let dt = rules.turn.game_seconds_per_tick;
    if !(dt.is_finite() && dt > 0.0) {
        return Err(DataError::InvalidRule("turn.game_seconds_per_tick"));
    }
    if rules.turn.planning_secs == 0 {
        return Err(DataError::InvalidRule("turn.planning_secs"));
    }
    Ok(rules)
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNITS: &str = include_str!("../../data/units.toml");

    #[test]
    fn g1_loads_shipped_units_toml() {
        let cat = load_units(UNITS).unwrap();
        for id in [
            "infantry", "mbt", "gsr", "esm", "hq", "spg", "uav", "shorad",
        ] {
            assert!(cat.get(id).is_some(), "missing unit type {id}");
        }
        assert!(cat.get("gsr").unwrap().is_emitter());
        assert!(!cat.get("esm").unwrap().is_emitter());
        assert_eq!(cat.get("infantry").unwrap().weapon_range_m, 400.0);
        assert_eq!(cat.get("mbt").unwrap().weapon_range_m, 2000.0);
        assert_eq!(cat.get("mbt").unwrap().class, UnitClass::Vehicle);
    }

    const RULES: &str = include_str!("../../data/rules.toml");

    #[test]
    fn g3_loads_shipped_rules_toml() {
        let r = load_rules(RULES).unwrap();
        assert_eq!(r.sensor.optical.track_range_factor, 1.5);
        assert_eq!(r.sensor.optical.fire_quality, 3);
        assert_eq!(r.sensor.optical.contact_quality, 1);
    }

    #[test]
    fn g3_rejects_invalid_rules() {
        let bad = RULES.replacen("track_range_factor = 1.5", "track_range_factor = 0.5", 1);
        assert_eq!(
            load_rules(&bad),
            Err(DataError::InvalidRule("sensor.optical.track_range_factor"))
        );
        let bad = RULES.replacen("fire_quality = 3", "fire_quality = 4", 1);
        assert_eq!(
            load_rules(&bad),
            Err(DataError::InvalidRule("sensor.optical.fire_quality"))
        );
        let bad = RULES.replacen("contact_quality = 1", "contact_quality = 1\nfoo = 2", 1);
        assert!(matches!(load_rules(&bad), Err(DataError::Parse(_))));
    }

    #[test]
    fn g3_rejects_negative_height() {
        let bad = UNITS.replacen("height_m = 1.8", "height_m = -1.0", 1);
        assert_eq!(
            load_units(&bad),
            Err(DataError::Invalid {
                unit: "infantry".into(),
                field: "height_m"
            })
        );
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
