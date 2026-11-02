// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Creature.cpp:506-538,1265-1489,1613-1693; ObjectMgr.cpp:400-576,719-798.
// Upstream GPL-2.0-or-later, Copyright CMaNGOS Project contributors.
//! Captured SQL rows into the existing generator, not source authentication or world admission.
use super::*;
use benilla_sim::classic_spell::SpellFloat;
use serde_json::Value;

/// Exact captured SQL column names and decimal envelopes, before loader preprocessing.
pub type CreatureCapturedRow = BTreeMap<String, Value>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureCapturedFactionTemplate {
    pub id: u32,
    pub faction: u32,
    pub flags: u32,
    pub faction_group: u32,
    pub friend_group: u32,
    pub enemy_group: u32,
    pub enemies: [u32; 4],
    pub friends: [u32; 4],
}

#[derive(Clone, Debug, Serialize)]
pub struct CreatureCapturedInput {
    pub templates: init::Rows<CreatureCapturedRow>,
    pub class_levels: init::Rows<CreatureCapturedRow>,
    pub faction_templates: init::Rows<CreatureCapturedFactionTemplate>,
}
const TEMPLATE_FIELDS: &[&str] = &[
    "Entry",
    "Name",
    "SubName",
    "MinLevel",
    "MaxLevel",
    "DisplayId1",
    "DisplayId2",
    "DisplayId3",
    "DisplayId4",
    "DisplayIdProbability1",
    "DisplayIdProbability2",
    "DisplayIdProbability3",
    "DisplayIdProbability4",
    "Faction",
    "Scale",
    "Family",
    "CreatureType",
    "InhabitType",
    "RegenerateStats",
    "RacialLeader",
    "NpcFlags",
    "UnitFlags",
    "DynamicFlags",
    "ExtraFlags",
    "CreatureTypeFlags",
    "StaticFlags1",
    "StaticFlags2",
    "StaticFlags3",
    "StaticFlags4",
    "SpeedWalk",
    "SpeedRun",
    "Detection",
    "CallForHelp",
    "Pursuit",
    "Leash",
    "Timeout",
    "UnitClass",
    "Rank",
    "HealthMultiplier",
    "PowerMultiplier",
    "DamageMultiplier",
    "DamageVariance",
    "ArmorMultiplier",
    "ExperienceMultiplier",
    "StrengthMultiplier",
    "AgilityMultiplier",
    "StaminaMultiplier",
    "IntellectMultiplier",
    "SpiritMultiplier",
    "MinLevelHealth",
    "MaxLevelHealth",
    "MinLevelMana",
    "MaxLevelMana",
    "MinMeleeDmg",
    "MaxMeleeDmg",
    "MinRangedDmg",
    "MaxRangedDmg",
    "Armor",
    "MeleeAttackPower",
    "RangedAttackPower",
    "MeleeBaseAttackTime",
    "RangedBaseAttackTime",
    "DamageSchool",
    "MinLootGold",
    "MaxLootGold",
    "LootId",
    "PickpocketLootId",
    "SkinningLootId",
    "KillCredit1",
    "KillCredit2",
    "MechanicImmuneMask",
    "SchoolImmuneMask",
    "ResistanceHoly",
    "ResistanceFire",
    "ResistanceNature",
    "ResistanceFrost",
    "ResistanceShadow",
    "ResistanceArcane",
    "PetSpellDataId",
    "MovementType",
    "TrainerType",
    "TrainerSpell",
    "TrainerClass",
    "TrainerRace",
    "TrainerTemplateId",
    "VendorTemplateId",
    "GossipMenuId",
    "InteractionPauseTimer",
    "EquipmentTemplateId",
    "Civilian",
    "CorpseDecay",
    "SpellList",
    "CharmedSpellList",
    "StringId1",
    "StringId2",
    "AIName",
    "ScriptName",
    "DamageMultiplierOLD",
    "DamageVarianceOLD",
];
const CLASS_FIELDS: &[&str] = &[
    "Class",
    "Level",
    "BaseMana",
    "BaseMeleeAttackPower",
    "BaseRangedAttackPower",
    "BaseArmor",
    "Strength",
    "Agility",
    "Stamina",
    "Intellect",
    "Spirit",
    "BaseHealthExp0",
    "BaseDamageExp0",
    "BaseDamageExp0OLD",
];
const FLOAT_FIELDS: &[&str] = &[
    "AgilityMultiplier",
    "ArmorMultiplier",
    "CallForHelp",
    "DamageMultiplier",
    "DamageMultiplierOLD",
    "DamageVariance",
    "DamageVarianceOLD",
    "Detection",
    "ExperienceMultiplier",
    "HealthMultiplier",
    "IntellectMultiplier",
    "Leash",
    "MaxMeleeDmg",
    "MaxRangedDmg",
    "MinMeleeDmg",
    "MinRangedDmg",
    "PowerMultiplier",
    "Pursuit",
    "Scale",
    "SpeedRun",
    "SpeedWalk",
    "SpiritMultiplier",
    "StaminaMultiplier",
    "StrengthMultiplier",
    "Timeout",
];

fn invalid() -> NativeError {
    NativeError::UnsupportedMechanic("captured creature field domain")
}
pub(super) fn uint(row: &CreatureCapturedRow, field: &str) -> Result<u32, NativeError> {
    row.get(field)
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(invalid)
}
fn text<'a>(row: &'a CreatureCapturedRow, field: &str) -> Result<&'a str, NativeError> {
    row.get(field)
        .and_then(Value::as_str)
        .filter(|s| s.len() <= 4096)
        .ok_or_else(invalid)
}
pub(super) fn float(row: &CreatureCapturedRow, field: &str) -> Result<SpellFloat, NativeError> {
    let value = row.get(field).ok_or_else(invalid)?;
    let value = if let Some(n) = value.as_i64() {
        if n.unsigned_abs() > 1_000_000 {
            return Err(invalid());
        }
        n as f32
    } else {
        let envelope = value
            .as_object()
            .filter(|v| v.len() == 1)
            .ok_or_else(invalid)?;
        let decimal = envelope
            .get("decimal")
            .and_then(Value::as_str)
            .ok_or_else(invalid)?;
        if decimal.len() > 128
            || !decimal
                .bytes()
                .all(|c| c.is_ascii_digit() || b"+-.eE".contains(&c))
        {
            return Err(invalid());
        }
        decimal.parse::<f32>().map_err(|_| invalid())?
    };
    SpellFloat::from_bits(value.to_bits()).map_err(|_| invalid())
}
pub(super) fn shape(row: &CreatureCapturedRow, fields: &[&str]) -> Result<(), NativeError> {
    if row.len() != fields.len() || fields.iter().any(|field| !row.contains_key(*field)) {
        return Err(NativeError::UnsupportedMechanic(
            "captured creature column coverage",
        ));
    }
    Ok(())
}
impl CreatureCapturedInput {
    /// Replace template/class stats only. The caller supplies actual resolved spawn, model,
    /// display, equipment, addon, string and configuration inputs in the existing source.
    pub fn compile(
        self,
        mut source: CreatureGenerationSource,
    ) -> Result<CreatureGenerationSource, NativeError> {
        source.input.validate_retention_bounds()?;
        let templates = rows(&self.templates)?;
        let [raw] = templates else {
            return Err(NativeError::UnsupportedMechanic(
                "one captured creature template",
            ));
        };
        shape(raw, TEMPLATE_FIELDS)?;
        for &field in TEMPLATE_FIELDS {
            if ["Name", "SubName", "AIName", "ScriptName"].contains(&field) {
                text(raw, field)?;
            } else if FLOAT_FIELDS.contains(&field) {
                float(raw, field)?;
            } else if field.starts_with("Resistance") {
                let n = raw[field].as_i64().ok_or_else(invalid)?;
                if n.unsigned_abs() > 1_000_000 {
                    return Err(invalid());
                }
            } else {
                uint(raw, field)?;
            }
        }
        if uint(raw, "Entry")? != source.spawn_entry {
            return Err(NativeError::AuthorityBinding);
        }
        if !text(raw, "ScriptName")?.is_empty() {
            return Err(NativeError::UnsupportedMechanic("creature ScriptName"));
        }
        if !["", "EventAI"].contains(&text(raw, "AIName")?) {
            return Err(NativeError::UnsupportedMechanic("creature AIName"));
        }
        // Creature.h:50,65 NO_AGGRO_ON_SIGHT and CIVILIAN remain source data.
        for (field, mask) in [
            ("ExtraFlags", 0x10002),
            ("NpcFlags", 0x13),
            ("UnitFlags", 0x1300),
            ("CreatureTypeFlags", 0x40000),
            ("StaticFlags1", 2),
            ("StaticFlags2", 0x10),
            ("RegenerateStats", 15),
        ] {
            if uint(raw, field)? & !mask != 0 {
                return Err(NativeError::UnsupportedMechanic(
                    "captured creature flag bits",
                ));
            }
        }
        for field in [
            "DynamicFlags",
            "StaticFlags3",
            "StaticFlags4",
            "MechanicImmuneMask",
            "SchoolImmuneMask",
            "RacialLeader",
        ] {
            if uint(raw, field)? != 0 {
                return Err(NativeError::UnsupportedMechanic(
                    "captured creature unsupported flags",
                ));
            }
        }
        if uint(raw, "Rank")? != 0 {
            return Err(NativeError::UnsupportedMechanic("non-normal rank"));
        }
        if uint(raw, "InhabitType")? & !3 != 0
            || uint(raw, "InhabitType")? == 0
            || uint(raw, "Civilian")? > 1
        {
            return Err(invalid());
        }
        if uint(raw, "Civilian")? != 0 && uint(raw, "ExtraFlags")? & 0x10000 == 0 {
            return Err(NativeError::UnsupportedMechanic(
                "legacy civilian without retained civilian extra flag",
            ));
        }
        let faction_rows = rows(&self.faction_templates)?;
        if faction_rows.len() > init::MAX_CREATURE_INIT_SOURCE_ROWS {
            return Err(NativeError::TooLarge);
        }
        let mut factions = BTreeMap::new();
        for row in faction_rows {
            if row.id == 0
                || row.flags & !0x1800 != 0
                || (row.faction_group | row.friend_group | row.enemy_group) & !0xf != 0
                || factions.insert(row.id, row).is_some()
            {
                return Err(NativeError::UnsupportedMechanic(
                    "captured faction-template rows",
                ));
            }
        }
        for id in std::iter::once(uint(raw, "Faction")?).chain(
            rows(&source.input.spawn_template)?
                .iter()
                .filter_map(|s| (s.faction != 0).then_some(s.faction)),
        ) {
            if !factions.contains_key(&id) {
                return Err(NativeError::UnsupportedMechanic(
                    "missing creature faction template",
                ));
            }
        }
        let captured_levels = rows(&self.class_levels)?;
        if captured_levels.len() > init::MAX_CREATURE_INIT_SOURCE_ROWS {
            return Err(NativeError::TooLarge);
        }
        let mut levels = Vec::with_capacity(captured_levels.len());
        for row in captured_levels {
            shape(row, CLASS_FIELDS)?;
            let old_damage = float(row, "BaseDamageExp0OLD")?;
            let parsed = init::ClassLevelRow {
                class: uint(row, "Class")?,
                level: uint(row, "Level")?.try_into().map_err(|_| invalid())?,
                base_health: uint(row, "BaseHealthExp0")?,
                base_mana: uint(row, "BaseMana")?,
                base_armor: uint(row, "BaseArmor")?,
                base_melee_attack_power: float(row, "BaseMeleeAttackPower")?,
                base_ranged_attack_power: float(row, "BaseRangedAttackPower")?,
                base_damage: float(row, "BaseDamageExp0")?,
                attributes: [
                    uint(row, "Strength")?,
                    uint(row, "Agility")?,
                    uint(row, "Stamina")?,
                    uint(row, "Intellect")?,
                    uint(row, "Spirit")?,
                ],
            };
            if ![1, 2, 4, 8].contains(&parsed.class)
                || !(1..=20).contains(&parsed.level)
                || parsed
                    .attributes
                    .into_iter()
                    .chain([parsed.base_health, parsed.base_mana, parsed.base_armor])
                    .any(|n| n > 1_000_000)
                || [
                    parsed.base_melee_attack_power,
                    parsed.base_ranged_attack_power,
                    parsed.base_damage,
                    old_damage,
                ]
                .into_iter()
                .any(|n| n.bits() >> 31 != 0 && n.bits() & 0x7fff_ffff != 0)
            {
                return Err(invalid());
            }
            levels.push(parsed);
        }
        let u = |field| uint(raw, field);
        let f = |field| float(raw, field);
        source.input.template = init::Template {
            entry: u("Entry")?,
            minimum_level: u("MinLevel")?.try_into().map_err(|_| invalid())?,
            maximum_level: u("MaxLevel")?.try_into().map_err(|_| invalid())?,
            unit_class: u("UnitClass")?,
            rank: u("Rank")?,
            displays: [
                init::DisplayChoice {
                    id: u("DisplayId1")?,
                    probability: u("DisplayIdProbability1")?,
                },
                init::DisplayChoice {
                    id: u("DisplayId2")?,
                    probability: u("DisplayIdProbability2")?,
                },
                init::DisplayChoice {
                    id: u("DisplayId3")?,
                    probability: u("DisplayIdProbability3")?,
                },
                init::DisplayChoice {
                    id: u("DisplayId4")?,
                    probability: u("DisplayIdProbability4")?,
                },
            ],
            scale: f("Scale")?,
            faction: u("Faction")?,
            creature_type: u("CreatureType")?,
            type_flags: u("CreatureTypeFlags")?,
            static_flags: [
                u("StaticFlags1")?,
                u("StaticFlags2")?,
                u("StaticFlags3")?,
                u("StaticFlags4")?,
            ],
            extra_flags: u("ExtraFlags")?,
            string_id1: u("StringId1")?,
            string_id2: u("StringId2")?,
            regenerate_stats: u("RegenerateStats")?,
            health_multiplier: f("HealthMultiplier")?,
            power_multiplier: f("PowerMultiplier")?,
            armor_multiplier: f("ArmorMultiplier")?,
            damage_multiplier: f("DamageMultiplier")?,
            damage_variance: f("DamageVariance")?,
            attribute_multipliers: [
                f("StrengthMultiplier")?,
                f("AgilityMultiplier")?,
                f("StaminaMultiplier")?,
                f("IntellectMultiplier")?,
                f("SpiritMultiplier")?,
            ],
            legacy: init::LegacyStats {
                minimum_health: u("MinLevelHealth")?,
                maximum_health: u("MaxLevelHealth")?,
                minimum_mana: u("MinLevelMana")?,
                maximum_mana: u("MaxLevelMana")?,
                armor: u("Armor")?,
                minimum_melee_damage: f("MinMeleeDmg")?,
                maximum_melee_damage: f("MaxMeleeDmg")?,
                melee_attack_power: u("MeleeAttackPower")?,
                ranged_attack_power: u("RangedAttackPower")?,
            },
            mainhand_interval_ms: u("MeleeBaseAttackTime")?,
            damage_school: u("DamageSchool")?,
            resistances: [
                "ResistanceHoly",
                "ResistanceFire",
                "ResistanceNature",
                "ResistanceFrost",
                "ResistanceShadow",
                "ResistanceArcane",
            ]
            .map(|field| raw[field].as_i64().unwrap() as i32),
            speed_walk: f("SpeedWalk")?,
            speed_run: f("SpeedRun")?,
            corpse_delay_seconds: u("CorpseDecay")?,
            equipment_template_id: u("EquipmentTemplateId")?,
            unit_flags: u("UnitFlags")?,
            npc_flags: u("NpcFlags")?,
        };
        source.input.class_levels = init::Rows::Complete(levels);
        bounded_encoded_size(&self, MAX_SOURCE_BYTES)?;
        source.validate()?;
        Ok(source)
    }
}

#[cfg(test)]
mod captured_tests;
