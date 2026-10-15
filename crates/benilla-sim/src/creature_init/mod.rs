// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
// Source grant GPL-2.0-or-later; Rust translation GPL-3.0-or-later.
//! Bounded ordinary NPC creation arithmetic, not world admission or AI selection.
//! Inputs are raw, locally captured rows. No database values or rates are supplied.
//! See ../../CREATURE-INIT.md for loader preprocessing, draw order and exclusions.
mod model_receipt;
mod numeric;
use crate::classic_spell::SpellFloat;
use crate::npc_melee::{CreatureStatClass, DamageRange};
pub use model_receipt::{
    InitialCollisionHeight, InitialCollisionWidth, ModelSelectionDraw, ModelSelectionDrawPurpose,
    ModelSelectionGenderRow, ModelSelectionReceipt, CREATURE_COLLISION_HEIGHT_POLICY_ID,
    CREATURE_COLLISION_WIDTH_POLICY_ID, CREATURE_MODEL_SELECTION_POLICY_ID,
};
use std::collections::{BTreeMap, BTreeSet};
pub const CREATURE_INIT_POLICY_ID: &str = "cmangos-8ec338a1-ordinary-creature-init-softfloat-v1";
pub const REGEN_INTERVAL_MS: u32 = 5_000;
const MAX_AMOUNT: u32 = 1_000_000;
pub type Result<T> = std::result::Result<T, InitError>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitError {
    Missing { table: &'static str, id: u32 },
    Invalid(&'static str),
    Unsupported(&'static str),
    Numeric(&'static str),
    Roll(&'static str),
}
impl std::fmt::Display for InitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bounded creature initialization: {self:?}")
    }
}
impl std::error::Error for InitError {}
/// Missing is not evidence for an empty query. A Complete collection is a typed
/// input contract, not an authenticity/capture-completeness certificate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rows<T> {
    Missing,
    Complete(Vec<T>),
}
impl<T> Rows<T> {
    fn require(&self, table: &'static str, id: u32) -> Result<&[T]> {
        match self {
            Self::Missing => Err(InitError::Missing { table, id }),
            Self::Complete(rows) => Ok(rows),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct DisplayChoice {
    pub id: u32,
    pub probability: u32,
}
/// Relevant raw creature_template numeric fields, before loader preprocessing.
/// The caller must retain/dispose every other captured field in its AI/lifecycle
/// admission. This input intentionally cannot certify such admission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Template {
    pub entry: u32,
    pub minimum_level: u8,
    pub maximum_level: u8,
    pub unit_class: u32,
    pub rank: u32,
    pub displays: [DisplayChoice; 4],
    pub scale: SpellFloat,
    pub faction: u32,
    pub creature_type: u32,
    pub type_flags: u32,
    pub static_flags: [u32; 4],
    pub regenerate_stats: u32,
    pub health_multiplier: SpellFloat,
    pub power_multiplier: SpellFloat,
    pub armor_multiplier: SpellFloat,
    pub damage_multiplier: SpellFloat,
    pub damage_variance: SpellFloat,
    /// Strength, agility, stamina, intellect, spirit, in native order.
    pub attribute_multipliers: [SpellFloat; 5],
    pub legacy: LegacyStats,
    pub mainhand_interval_ms: u32,
    pub damage_school: u32,
    /// Holy, fire, nature, frost, shadow, arcane; native signed create resistances.
    pub resistances: [i32; 6],
    pub speed_walk: SpellFloat,
    pub speed_run: SpellFloat,
    pub corpse_delay_seconds: u32,
    pub equipment_template_id: u32,
    pub unit_flags: u32,
    pub npc_flags: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LegacyStats {
    pub minimum_health: u32,
    pub maximum_health: u32,
    pub minimum_mana: u32,
    pub maximum_mana: u32,
    pub armor: u32,
    pub minimum_melee_damage: SpellFloat,
    pub maximum_melee_damage: SpellFloat,
    pub melee_attack_power: u32,
    pub ranged_attack_power: u32,
}
/// Raw SQL classlevelstats. BaseMana/BaseArmor are NOT the loaded core values:
/// ObjectMgr removes the native intellect/agility bonuses before SelectLevel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassLevelRow {
    pub class: u32,
    pub level: u8,
    pub base_health: u32,
    pub base_mana: u32,
    pub base_armor: u32,
    pub base_melee_attack_power: SpellFloat,
    pub base_ranged_attack_power: SpellFloat,
    pub base_damage: SpellFloat,
    pub attributes: [u32; 5],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelRow {
    pub id: u32,
    pub bounding_radius: SpellFloat,
    pub combat_reach: SpellFloat,
    pub speed_walk: SpellFloat,
    pub speed_run: SpellFloat,
    pub gender: u8,
    pub other_gender_id: u32,
}
/// CreatureDisplayInfo identity/scale from the verified local DBC. Every named
/// template/model/other-gender display must exist; loader repairs are rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct DisplayRow {
    pub id: u32,
    pub scale: SpellFloat,
}
/// Actual effective creature_spawn_data_template row selected by LoadCreatures.
/// Entry zero is a real default row, never synthesized by this module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpawnTemplate {
    pub entry: u32,
    pub model_id: u32,
    pub equipment_id: i32,
    pub current_health: u32,
    pub current_mana: u32,
    pub faction: u32,
    pub npc_flags: i32,
    pub unit_flags: i64,
    pub spawn_flags: u32,
    pub relay_id: u32,
    pub string_id: u32,
}
/// Explicit effective world configuration; no Default implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rates {
    pub normal_health: SpellFloat,
    pub normal_damage: SpellFloat,
    pub normal_corpse_decay_seconds: u32,
}
/// Known applicable feature that changes this bounded fresh, living, uncontrolled,
/// outdoor, unmounted, no-equipment/no-aura creation path. All are unsupported.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum ExternalFeature {
    PetSummonOrControl,
    InstanceOrBattleground,
    FlightSwimmingTransportOrMount,
    ExistingStateOrForcedLevel,
    GameEventOrPoolSubstitution,
    ConditionalOrGroupedSpawn,
    LinkingOrAssistance,
    Addon,
    InitialAuraOrUnitModifier,
    ScriptOrAiSubstitution,
    SpellList,
    MapOrOutdoorPvpCreationHook,
    EquipmentOrLootWeapon,
    EncounterCorpseOverride,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Input {
    pub template: Template,
    pub class_levels: Rows<ClassLevelRow>,
    pub models: Rows<ModelRow>,
    pub displays: Rows<DisplayRow>,
    /// Exactly one effective resolved row, including the genuine Entry=0 default.
    pub spawn_template: Rows<SpawnTemplate>,
    /// Requires positive completeness evidence at the private consumer boundary.
    pub external_features: Rows<ExternalFeature>,
    pub rates: Rates,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerType {
    Mana,
    Rage,
    Energy,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resources {
    pub health: u32,
    pub max_health: u32,
    pub mana: u32,
    pub max_mana: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stats {
    pub create_attributes: [SpellFloat; 5],
    pub attributes: [u32; 5],
    pub create_health: u32,
    pub create_mana: u32,
    pub base_armor: SpellFloat,
    pub armor: i32,
    /// Armor followed by holy, fire, nature, frost, shadow, arcane.
    pub resistances: [i32; 7],
    pub base_melee_attack_power: SpellFloat,
    pub loaded_base_ranged_attack_power: SpellFloat,
    pub damage_multiplier: SpellFloat,
    pub base_weapon_minimum: SpellFloat,
    pub base_weapon_maximum: SpellFloat,
    pub damage: DamageRange,
    pub resources: Resources,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Model {
    pub display_id: u32,
    pub gender: u8,
    pub scale: SpellFloat,
    /// Native normalizedScale is one at initial creation, independent of scale.
    pub bounding_radius: SpellFloat,
    pub combat_reach: SpellFloat,
    /// Actual yards per second, not speed-rate multipliers.
    pub walk_speed: SpellFloat,
    pub run_speed: SpellFloat,
}
/// Arithmetic result only. Does not authorize spawning, replace an AI profile,
/// decide hostility, supply a position, or certify local source dependencies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitializedCreature {
    pub entry: u32,
    pub level: u8,
    pub power_type: PowerType,
    pub stats: Stats,
    pub model: Model,
    pub faction: u32,
    pub unit_flags: u32,
    pub npc_flags: u32,
    pub starts_running: bool,
    pub no_crushing_blows: bool,
    pub creature_is_critter: bool,
    pub mainhand_interval_ms: u32,
    /// InitEntry result BEFORE LoadFromDB legacy respawn-delay cap.
    pub entry_corpse_delay_seconds: u32,
    pub health_regenerates: bool,
    pub power_regenerates_out_of_combat: bool,
    pub power_regenerates_in_combat: bool,
}
/// Semantic draws in exact source order: model, conditional gender, conditional
/// level. A single eligible model STILL consumes irand(0, probability_sum-1).
/// Equal template levels consume no level draw. None and singleton are distinct.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectionRolls {
    pub model: Option<u32>,
    pub gender: Option<u32>,
    pub level: Option<u32>,
}
/// Validated before requesting randomness; every reachable level/model is checked.
#[derive(Debug)]
pub struct Plan<'a> {
    input: &'a Input,
    spawn: SpawnTemplate,
    levels: BTreeMap<u8, Stats>,
    models: BTreeMap<u32, Model>,
    other_gender: BTreeMap<u32, u32>,
    model_range: Option<(u32, u32)>,
    scale: SpellFloat,
}
impl Plan<'_> {
    pub fn model_roll_range(&self) -> Option<(u32, u32)> {
        self.model_range
    }
    pub fn level_roll_range(&self) -> Option<(u32, u32)> {
        let t = &self.input.template;
        (t.minimum_level != t.maximum_level)
            .then_some((u32::from(t.minimum_level), u32::from(t.maximum_level)))
    }
    fn selected_model(&self, roll: Option<u32>) -> Result<u32> {
        validate_roll(roll, self.model_range, "model draw")?;
        if self.spawn.model_id != 0 {
            return Ok(self.spawn.model_id);
        }
        let mut roll = roll.ok_or(InitError::Roll("missing model draw"))?;
        for choice in self.input.template.displays {
            if choice.id != 0 {
                if roll < choice.probability {
                    return Ok(choice.id);
                }
                roll -= choice.probability;
            }
        }
        Err(InitError::Invalid("template model fallback is unsupported"))
    }
    pub fn gender_roll_range(&self, model_roll: Option<u32>) -> Result<Option<(u32, u32)>> {
        let id = self.selected_model(model_roll)?;
        Ok((self.other_gender[&id] != 0).then_some((0, 1)))
    }
    pub fn resolve(self, rolls: SelectionRolls) -> Result<InitializedCreature> {
        self.resolve_with_model_receipt(rolls)
            .map(|(result, _)| result)
    }
    /// Resolve the same validated plan, retaining immutable model-selection evidence.
    /// The receipt binds typed inputs and supplied draws, not an authenticated capture,
    /// live RNG transcript, runnable creature or model after a later lifecycle reset.
    pub fn resolve_with_model_receipt(
        self,
        rolls: SelectionRolls,
    ) -> Result<(InitializedCreature, ModelSelectionReceipt)> {
        let original = self.selected_model(rolls.model)?;
        let gender_range = self.gender_roll_range(rolls.model)?;
        validate_roll(rolls.gender, gender_range, "gender draw")?;
        validate_roll(rolls.level, self.level_roll_range(), "level draw")?;
        let display = if rolls.gender == Some(0) {
            self.other_gender[&original]
        } else {
            original
        };
        let t = &self.input.template;
        let level = rolls.level.map_or(t.minimum_level, |n| n as u8);
        let mut stats = self.levels[&level];
        stats.resources.health = if self.spawn.current_health == 0 {
            stats.resources.max_health
        } else {
            self.spawn.current_health.min(stats.resources.max_health)
        };
        let power_type = power_type(t.unit_class)?;
        if power_type == PowerType::Mana {
            stats.resources.mana = if self.spawn.current_mana == 0 && t.regenerate_stats & 8 != 0 {
                stats.resources.max_mana
            } else {
                self.spawn.current_mana.min(stats.resources.max_mana)
            };
        }
        let mut model = self.models[&display];
        model.scale = self.scale;
        let result = InitializedCreature {
            entry: t.entry,
            level,
            power_type,
            stats,
            model,
            faction: if self.spawn.faction == 0 {
                t.faction
            } else {
                self.spawn.faction
            },
            unit_flags: if self.spawn.unit_flags == -1 {
                t.unit_flags
            } else {
                self.spawn.unit_flags as u32
            },
            npc_flags: if self.spawn.npc_flags == -1 {
                t.npc_flags
            } else {
                self.spawn.npc_flags as u32
            },
            starts_running: self.spawn.spawn_flags & 1 != 0,
            no_crushing_blows: t.static_flags[1] & 0x10 != 0,
            creature_is_critter: t.creature_type == 8,
            mainhand_interval_ms: if t.mainhand_interval_ms == 0 {
                2000
            } else {
                t.mainhand_interval_ms
            },
            entry_corpse_delay_seconds: if t.corpse_delay_seconds == 0 {
                self.input.rates.normal_corpse_decay_seconds
            } else {
                t.corpse_delay_seconds
            },
            health_regenerates: t.regenerate_stats & 2 != 0 && t.type_flags & 0x40000 == 0,
            power_regenerates_out_of_combat: t.regenerate_stats & 8 != 0,
            power_regenerates_in_combat: t.regenerate_stats & 4 != 0,
        };
        let receipt = ModelSelectionReceipt::from_resolved(&self, rolls, original, &result.model)?;
        Ok((result, receipt))
    }
}
fn validate_roll(value: Option<u32>, range: Option<(u32, u32)>, name: &'static str) -> Result<()> {
    match (value, range) {
        (None, None) => Ok(()),
        (Some(value), Some((min, max))) if (min..=max).contains(&value) => Ok(()),
        _ => Err(InitError::Roll(name)),
    }
}
fn power_type(class: u32) -> Result<PowerType> {
    match class {
        1 => Ok(PowerType::Rage),
        2 | 8 => Ok(PowerType::Mana),
        4 => Ok(PowerType::Energy),
        _ => Err(InitError::Invalid("creature class")),
    }
}
fn stat_class(class: u32) -> Result<CreatureStatClass> {
    match class {
        1 => Ok(CreatureStatClass::Warrior),
        2 => Ok(CreatureStatClass::Paladin),
        4 => Ok(CreatureStatClass::Rogue),
        8 => Ok(CreatureStatClass::Mage),
        _ => Err(InitError::Invalid("creature class")),
    }
}
/// Reject unknown dependencies and unimplemented source branches before any RNG.
pub fn prepare(input: &Input) -> Result<Plan<'_>> {
    let t = &input.template;
    if t.entry == 0
        || !(1..=20).contains(&t.minimum_level)
        || t.maximum_level < t.minimum_level
        || t.maximum_level > 20
    {
        return Err(InitError::Invalid("ordinary level 1..20 template"));
    }
    power_type(t.unit_class)?;
    if t.rank != 0 {
        return Err(InitError::Unsupported("non-normal rank"));
    }
    if t.damage_school != 0 {
        return Err(InitError::Unsupported("nonphysical mainhand"));
    }
    if t.mainhand_interval_ms > 60_000 {
        return Err(InitError::Invalid("mainhand attack time"));
    }
    if t.creature_type == 0 || t.creature_type > 10 || t.faction == 0 {
        return Err(InitError::Invalid("creature type/faction"));
    }
    if t.regenerate_stats & !15 != 0 {
        return Err(InitError::Invalid("regeneration flags"));
    }
    // Static settings outside the one arithmetic consumer are left to a future
    // branch, rather than silently executing their behavioral substitutions.
    if t.static_flags[0] != 0
        || t.static_flags[1] & !0x10 != 0
        || t.static_flags[2] != 0
        || t.static_flags[3] != 0
    {
        return Err(InitError::Unsupported(
            "static setting outside no-crushing branch",
        ));
    }
    if !input
        .external_features
        .require("applicable initialization dependencies", t.entry)?
        .is_empty()
    {
        return Err(InitError::Unsupported(
            "applicable external initialization feature",
        ));
    }
    let spawn = match input
        .spawn_template
        .require("creature_spawn_data_template resolved row", t.entry)?
    {
        [row] => *row,
        _ => return Err(InitError::Invalid("exactly one resolved spawn template")),
    };
    if spawn.relay_id != 0 || spawn.string_id != 0 || spawn.spawn_flags & !1 != 0 {
        return Err(InitError::Unsupported("spawn relay/string/hover/gravity"));
    }
    if spawn.equipment_id < -1
        || spawn.npc_flags < -1
        || !(-1..=i64::from(u32::MAX)).contains(&spawn.unit_flags)
    {
        return Err(InitError::Invalid("spawn override sentinel"));
    }
    let equipment = if spawn.equipment_id == -1 {
        t.equipment_template_id
    } else {
        spawn.equipment_id as u32
    };
    if equipment != 0 {
        return Err(InitError::Unsupported("equipment or loot weapon"));
    }
    let flags = if spawn.unit_flags == -1 {
        t.unit_flags
    } else {
        spawn.unit_flags as u32
    };
    if flags & (0x8 | 0x01000000) != 0 {
        return Err(InitError::Unsupported("player-controlled or possessed"));
    }
    let classes = input
        .class_levels
        .require("creature_template_classlevelstats", t.entry)?;
    let mut class_map = BTreeMap::new();
    for row in classes {
        if class_map.insert((row.class, row.level), row).is_some() {
            return Err(InitError::Invalid("duplicate classlevel row"));
        }
    }
    let mut levels = BTreeMap::new();
    for level in t.minimum_level..=t.maximum_level {
        let row = class_map
            .get(&(t.unit_class, level))
            .ok_or(InitError::Missing {
                table: "creature_template_classlevelstats",
                id: u32::from(level),
            })?;
        levels.insert(level, numeric::stats(t, row, input.rates)?);
    }
    let display_rows = input.displays.require("CreatureDisplayInfo.dbc", t.entry)?;
    let mut displays = BTreeMap::new();
    for row in display_rows {
        if row.id == 0 || displays.insert(row.id, row).is_some() {
            return Err(InitError::Invalid("display row identity"));
        }
        numeric::positive(row.scale, "display scale")?;
    }
    let model_rows = input.models.require("creature_model_info", t.entry)?;
    let mut raw_models = BTreeMap::new();
    for row in model_rows {
        if row.id == 0 || raw_models.insert(row.id, row).is_some() {
            return Err(InitError::Invalid("model row identity"));
        }
    }
    let mut roots = BTreeSet::new();
    let mut chance_total = 0u32;
    let mut first_display = None;
    for choice in t.displays {
        if choice.id != 0 {
            let display = displays.get(&choice.id).ok_or(InitError::Missing {
                table: "CreatureDisplayInfo.dbc",
                id: choice.id,
            })?;
            first_display.get_or_insert(display.scale);
            chance_total = chance_total
                .checked_add(choice.probability)
                .filter(|n| *n <= i32::MAX as u32)
                .ok_or(InitError::Invalid("model probability overflow"))?;
            roots.insert(choice.id);
        }
    }
    if spawn.model_id != 0 {
        roots.insert(spawn.model_id);
    }
    if roots.is_empty() || (spawn.model_id == 0 && chance_total == 0) {
        return Err(InitError::Unsupported("template entry-1 model fallback"));
    }
    let scale = if numeric::is_positive(t.scale) {
        t.scale
    } else {
        first_display.ok_or(InitError::Missing {
            table: "template default CreatureDisplayInfo.dbc scale",
            id: t.entry,
        })?
    };
    let mut models = BTreeMap::new();
    let mut other_gender = BTreeMap::new();
    for &id in &roots {
        let row = raw_models.get(&id).ok_or(InitError::Missing {
            table: "creature_model_info",
            id,
        })?;
        other_gender.insert(id, row.other_gender_id);
        if row.other_gender_id == id {
            return Err(InitError::Unsupported(
                "redundant other-gender loader repair",
            ));
        }
        for required in [id, row.other_gender_id].into_iter().filter(|id| *id != 0) {
            if !displays.contains_key(&required) {
                return Err(InitError::Missing {
                    table: "CreatureDisplayInfo.dbc",
                    id: required,
                });
            }
            let required_row = raw_models.get(&required).ok_or(InitError::Missing {
                table: "creature_model_info",
                id: required,
            })?;
            models.insert(required, numeric::model(t, required_row, scale)?);
        }
    }
    Ok(Plan {
        input,
        spawn,
        levels,
        models,
        other_gender,
        model_range: (spawn.model_id == 0).then_some((0, chance_total.saturating_sub(1))),
        scale,
    })
}
/// Live state for this module's OUT-OF-COMBAT regeneration subset. Persist the
/// accumulator; source performs at most one tick per Update, even for large diff.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegenState {
    pub elapsed_ms: u32,
    pub health: u32,
    pub mana: u32,
}
pub fn regenerate_out_of_combat(
    profile: &InitializedCreature,
    before: RegenState,
    diff_ms: u32,
) -> Result<RegenState> {
    let resources = profile.stats.resources;
    if resources.max_health == 0
        || resources.max_health > MAX_AMOUNT
        || resources.max_mana > MAX_AMOUNT
    {
        return Err(InitError::Invalid("bounded regeneration maxima"));
    }
    if before.health == 0
        || before.health > resources.max_health
        || before.mana > resources.max_mana
    {
        return Err(InitError::Invalid("living regeneration resources"));
    }
    let mut result = before;
    result.elapsed_ms = result
        .elapsed_ms
        .checked_add(diff_ms)
        .ok_or(InitError::Numeric("regeneration timer overflow"))?;
    if result.elapsed_ms < REGEN_INTERVAL_MS {
        return Ok(result);
    }
    if profile.health_regenerates {
        result.health = resources
            .max_health
            .min(result.health + resources.max_health / 3);
    }
    if profile.power_regenerates_out_of_combat && profile.power_type == PowerType::Mana {
        result.mana = resources
            .max_mana
            .min(result.mana + numeric::mana_regen(resources.max_mana)?);
    }
    result.elapsed_ms -= REGEN_INTERVAL_MS;
    Ok(result)
}

/// Captured creature respawn range plus actual allocated source identities. The
/// source uses dbGuid != runtime GUID low to select the new spawning system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpawnTimingInputs {
    pub database_guid: u32,
    pub runtime_guid_low: u32,
    pub minimum_respawn_seconds: u32,
    pub maximum_respawn_seconds: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpawnTiming {
    pub respawn_delay_seconds: u32,
    pub corpse_delay_seconds: u32,
}
impl SpawnTimingInputs {
    /// Source loader raises an inverted maximum to the minimum. The draw is
    /// ALWAYS consumed, including equal endpoints, after creature creation.
    pub fn respawn_roll_range(self) -> Result<(u32, u32)> {
        if self.database_guid == 0 || self.runtime_guid_low == 0 {
            return Err(InitError::Invalid("static spawn identities"));
        }
        let maximum = self
            .maximum_respawn_seconds
            .max(self.minimum_respawn_seconds);
        if maximum > u32::MAX / 9 {
            return Err(InitError::Numeric("respawn-delay multiplication overflow"));
        }
        Ok((self.minimum_respawn_seconds, maximum))
    }
    pub fn resolve(
        self,
        entry_corpse_delay_seconds: u32,
        respawn_draw: u32,
    ) -> Result<SpawnTiming> {
        let (minimum, maximum) = self.respawn_roll_range()?;
        if !(minimum..=maximum).contains(&respawn_draw) {
            return Err(InitError::Roll("respawn draw"));
        }
        let corpse_delay_seconds = if self.database_guid == self.runtime_guid_low {
            entry_corpse_delay_seconds.min(respawn_draw * 9 / 10)
        } else {
            entry_corpse_delay_seconds
        };
        Ok(SpawnTiming {
            respawn_delay_seconds: respawn_draw,
            corpse_delay_seconds,
        })
    }
}
