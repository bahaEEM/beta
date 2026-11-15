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
    /// Building (K-121): never moves and changes hands with its region.
    Structure,
    /// Surface vessel (K-160): moves on water only.
    Ship,
    /// Submarine (K-161): moves on water only, always submerged; only sonar finds it.
    Sub,
}

impl UnitClass {
    pub fn is_air(self) -> bool {
        self == UnitClass::Air
    }

    /// Ships and submarines.
    pub fn is_naval(self) -> bool {
        matches!(self, UnitClass::Ship | UnitClass::Sub)
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
    /// The direct-fire weapon engages ships (K-163).
    #[serde(default)]
    pub weapon_vs_sea: bool,
    /// The direct-fire weapon engages submarines (torpedoes, depth charges).
    #[serde(default)]
    pub weapon_vs_sub: bool,
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
    /// Units this type can carry: infantry teams (IFV, ambulance)...
    #[serde(default)]
    pub carry_capacity: u32,
    /// ...and vehicles too (landing ship, K-164).
    #[serde(default)]
    pub carry_vehicles: bool,
    /// Naval units moving no faster than this make no noise for passive sonar (K-162).
    #[serde(default)]
    pub quiet_speed_mps: f64,
    /// Command post: the root of its side's command network.
    #[serde(default)]
    pub hq: bool,
    /// Stock capacity: what a carrier (truck, cargo plane) hauls for others or a storage
    /// structure (logistics base, depot, airfield, port) holds. Units start full.
    #[serde(default)]
    pub supply_fuel_l: f64,
    #[serde(default)]
    pub supply_ammo: u32,
    /// Stock a logistics base makes every turn (K-131).
    #[serde(default)]
    pub produce_fuel_l: f64,
    #[serde(default)]
    pub produce_ammo: u32,
    /// Game seconds to set up after stopping and to pack up before moving (K-120);
    /// 0 = works on the move. Until set up the unit cannot fire and its radar, ESM
    /// and jammer are off.
    #[serde(default)]
    pub deploy_s: f64,
    /// Hit points given to each friendly infantry unit in reach at the end of a turn (K-123).
    #[serde(default)]
    pub heal_hp_per_turn: f64,
    /// Counter-battery radar (K-153): while it radiates, enemy guns that fire from, or
    /// at a point, within this range are located. 0 = none.
    #[serde(default)]
    pub counter_battery_range_m: f64,
    /// Flown over a data link (K-155): without a link to the HQ it flies home.
    #[serde(default)]
    pub remote_piloted: bool,
    /// Home for remote-piloted aircraft that lose their link (K-155).
    #[serde(default)]
    pub airbase: bool,
    /// Decoy (K-144): the enemy sees this type id until he identifies it from close up.
    #[serde(default)]
    pub decoy_of: Option<String>,
    /// Price (K-171); 0 = cannot be bought.
    #[serde(default)]
    pub cost: u32,
    /// Ends of turn from the purchase until the unit appears (K-172).
    #[serde(default)]
    pub build_turns: u32,
    /// Structures: the unit classes bought here appear at this structure (K-171).
    #[serde(default)]
    pub produces: Vec<UnitClass>,
    /// Engineers and repair tents (K-180): share of its maximum hp given back per turn
    /// to each damaged friendly vehicle and structure in reach. 0 = none.
    #[serde(default)]
    pub repair_share: f64,
    /// Engineers (K-181): ids of the structure types this unit can build.
    #[serde(default)]
    pub builds: Vec<String>,
    /// Dug-in position (K-182): this many infantry units next to it fight from cover.
    #[serde(default)]
    pub shelter: u32,
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
    /// Loitering anti-radiation munition (K-152): fired at an enemy unit it homes on it
    /// for as long as that unit's radar or jammer is on the air.
    #[serde(default)]
    pub anti_radiation: bool,
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
    /// Active sonar (K-162): while the unit emits, locates ships and submarines in
    /// range; enemy passive sonar hears the ping.
    SonarActive {
        range_m: f64,
    },
    /// Passive sonar: bearings to noisy ships and submarines.
    SonarPassive {
        range_m: f64,
    },
}

impl SensorSpec {
    pub fn range_m(&self) -> f64 {
        match *self {
            SensorSpec::Optical { range_m }
            | SensorSpec::Radar { range_m, .. }
            | SensorSpec::Esm { range_m }
            | SensorSpec::SonarActive { range_m }
            | SensorSpec::SonarPassive { range_m }
            | SensorSpec::Jammer { range_m, .. } => range_m,
        }
    }

    pub fn is_emitter(&self) -> bool {
        matches!(
            self,
            SensorSpec::Radar { .. } | SensorSpec::Jammer { .. } | SensorSpec::SonarActive { .. }
        )
    }
}

impl UnitType {
    /// True if the type has a radar or jammer that can be switched on.
    pub fn is_emitter(&self) -> bool {
        self.sensors.iter().any(SensorSpec::is_emitter)
    }

    /// Has a radar or jammer: on the air it can be heard by ESM.
    pub fn emits_rf(&self) -> bool {
        self.sensors
            .iter()
            .any(|s| matches!(s, SensorSpec::Radar { .. } | SensorSpec::Jammer { .. }))
    }

    /// Has an active sonar: pinging it can be heard by passive sonar.
    pub fn pings(&self) -> bool {
        self.sensors
            .iter()
            .any(|s| matches!(s, SensorSpec::SonarActive { .. }))
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
    // K-181: engineers build structures that have a price.
    let buildable = |id: &String| {
        types
            .get(id)
            .is_some_and(|s: &UnitType| s.class == UnitClass::Structure && s.cost > 0)
    };
    if let Some(t) = types.values().find(|t| !t.builds.iter().all(buildable)) {
        return Err(DataError::Invalid {
            unit: t.id.clone(),
            field: "builds",
        });
    }
    if let Some(t) = types
        .values()
        .find(|t| t.decoy_of.as_ref().is_some_and(|d| !types.contains_key(d)))
    {
        return Err(DataError::Invalid {
            unit: t.id.clone(),
            field: "decoy_of",
        });
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
        ("deploy_s", t.deploy_s),
        ("produce_fuel_l", t.produce_fuel_l),
        ("counter_battery_range_m", t.counter_battery_range_m),
        ("quiet_speed_mps", t.quiet_speed_mps),
        ("heal_hp_per_turn", t.heal_hp_per_turn),
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
    if !(0.0..=1.0).contains(&t.repair_share) {
        return Err(DataError::Invalid {
            unit: t.id.clone(),
            field: "repair_share",
        });
    }
    if t.cost > 0 && t.build_turns == 0 {
        return Err(DataError::Invalid {
            unit: t.id.clone(),
            field: "build_turns",
        });
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
    pub medical: MedicalRules,
    pub conceal: ConcealRules,
    pub cover: CoverRules,
    pub ambush: AmbushRules,
    pub suppression: SuppressionRules,
    pub decoy: DecoyRules,
    pub indirect: IndirectRules,
    pub economy: EconomyRules,
    pub engineer: EngineerRules,
}

/// K-180…K-182.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineerRules {
    /// Repair reach of engineers and repair tents at the end of a turn.
    pub repair_radius_m: f64,
    /// A full repair costs this share of the unit's price (pro rata for less).
    pub repair_cost_share: f64,
    /// An engineer this close to a building site works on it.
    pub build_radius_m: f64,
    /// Unit type of a structure under construction.
    pub site_type: String,
    /// Infantry this close to a dug-in position can use it.
    pub shelter_radius_m: f64,
    /// Damage multipliers for the infantry in a position: direct fire, shells.
    pub shelter_direct_factor: f64,
    pub shelter_shell_factor: f64,
    /// Multiplier on the suppression it takes.
    pub shelter_suppression_factor: f64,
}

/// K-170…K-173.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EconomyRules {
    /// Money every player starts with.
    pub start_money: u32,
    /// Income per turn of a supplied own region that sets no value of its own.
    pub region_income: u32,
    /// A ship bought at a port appears on the nearest water within this distance.
    pub launch_radius_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndirectRules {
    /// Track quality needed to fire a mission at an enemy unit.
    pub min_track_quality: u8,
    /// A gun located by a counter-battery radar stays a counter-fire target this long (K-154).
    pub counter_fire_window_s: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransportRules {
    /// Infantry must be this close to the carrier to embark.
    pub embark_radius_m: f64,
    /// Share of their hp passengers lose when the carrier is destroyed.
    pub bail_out_damage: f64,
    /// Boarding a ship from the shore and landing from it works within this distance (K-164).
    pub ship_embark_radius_m: f64,
    /// Suppression of a unit that has just been put ashore (it sorts itself out on the beach).
    pub landing_suppression: f64,
    /// Track quality weapons need on a ship or submarine (a radar or sonar fix is enough).
    pub naval_fire_quality: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommsRules {
    /// A unit this close to a road cell is on the wire network of that road.
    pub wire_reach_m: f64,
    /// Game seconds a unit transmits by radio after receiving an order (K-150).
    pub radio_emit_s: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZoneRules {
    /// Game seconds of uncontested infantry presence needed to capture.
    pub capture_s: f64,
    pub max_turns: u32,
}

/// K-140: factors on the optical range of whoever looks at a ground unit (multiplied).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConcealRules {
    pub forest_factor: f64,
    pub settlement_factor: f64,
    /// Infantry class, anywhere.
    pub infantry_factor: f64,
    /// Not moving.
    pub halted_factor: f64,
    /// After firing a unit has no concealment for this long (game seconds).
    pub fire_reveal_s: f64,
}

/// K-141: damage multipliers for infantry in cover.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoverRules {
    pub forest_damage_factor: f64,
    pub settlement_damage_factor: f64,
}

/// K-142: the 'ambush' stance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AmbushRules {
    /// Fire is held until a target is within this share of the weapon range.
    pub range_share: f64,
    /// Damage multiplier while the enemy does not know where the shooter is.
    pub damage_factor: f64,
}

/// K-143: suppression, a value from 0 to 1 per ground unit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuppressionRules {
    /// Added per direct-fire shot aimed at the unit (hit or miss).
    pub per_shot: f64,
    /// Added by a shell landing on the unit; falls to zero at the blast radius.
    pub per_shell: f64,
    pub decay_per_s: f64,
    /// At full suppression the unit's hit share drops by this share...
    pub hit_penalty: f64,
    /// ...and its speed by this share.
    pub speed_penalty: f64,
    /// Infantry at or above this value is pinned: it lies down and does not move.
    pub pin_threshold: f64,
    /// Damage multiplier for pinned infantry (it has gone to ground).
    pub pinned_damage_factor: f64,
}

/// K-144: decoys.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecoyRules {
    /// An enemy with eyes on a decoy from this close recognises it for good.
    pub reveal_range_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MedicalRules {
    /// Medical units treat friendly infantry this close at the end of each turn.
    pub radius_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupplyRules {
    /// Carriers and storages resupply friendly units this close at the end of each turn.
    pub radius_m: f64,
    /// Share of its fuel and ammunition capacity a unit gets per turn from the network
    /// of the own region it stands in (K-133).
    pub region_share: f64,
    /// A ground unit starts losing hit points at the end of this many unsupplied turns
    /// in a row (K-134)...
    pub attrition_after_turns: u32,
    /// ...by this share of its maximum per turn.
    pub attrition_hp_share: f64,
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
    pub sonar: SonarRules,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SonarRules {
    /// Track quality an active sonar gives (position + class).
    pub active_quality: u8,
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
    if !(rules.comms.radio_emit_s >= 0.0 && rules.indirect.counter_fire_window_s >= 0.0) {
        return Err(DataError::InvalidRule(
            "comms.radio_emit_s / indirect.counter_fire_window_s",
        ));
    }
    if !(rules.comms.wire_reach_m.is_finite() && rules.comms.wire_reach_m >= 0.0) {
        return Err(DataError::InvalidRule("comms.wire_reach_m"));
    }
    let tr = &rules.transport;
    let q = [rules.sensor.sonar.active_quality, tr.naval_fire_quality];
    if q.iter().any(|&q| q == 0 || q > MAX_TRACK_QUALITY)
        || !(tr.ship_embark_radius_m > 0.0 && (0.0..=1.0).contains(&tr.landing_suppression))
    {
        return Err(DataError::InvalidRule("sensor.sonar / transport"));
    }
    let (cn, cv, sp) = (&rules.conceal, &rules.cover, &rules.suppression);
    let shares = [
        cn.forest_factor,
        cn.settlement_factor,
        cn.infantry_factor,
        cn.halted_factor,
        cv.forest_damage_factor,
        cv.settlement_damage_factor,
        rules.ambush.range_share,
        sp.per_shot,
        sp.per_shell,
        sp.hit_penalty,
        sp.speed_penalty,
        sp.pin_threshold,
        sp.pinned_damage_factor,
    ];
    if shares.iter().any(|v| !(0.0..=1.0).contains(v))
        || !(cn.fire_reveal_s >= 0.0 && sp.decay_per_s >= 0.0)
        || !(rules.ambush.damage_factor >= 1.0 && rules.decoy.reveal_range_m >= 0.0)
    {
        return Err(DataError::InvalidRule(
            "conceal/cover/ambush/suppression/decoy",
        ));
    }
    let sup = &rules.supply;
    if !((0.0..=1.0).contains(&sup.region_share) && (0.0..=1.0).contains(&sup.attrition_hp_share))
        || sup.attrition_after_turns == 0
    {
        return Err(DataError::InvalidRule("supply"));
    }
    if !(rules.zones.capture_s > 0.0 && rules.supply.radius_m > 0.0 && rules.medical.radius_m > 0.0)
        || rules.zones.max_turns == 0
    {
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
