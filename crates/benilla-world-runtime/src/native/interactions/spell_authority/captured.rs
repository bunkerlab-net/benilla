// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f:
// src/game/Server/SQLStorages.cpp:40,55, SpellEntryfmt/sSpellTemplate;
// src/game/Server/DBCStructure.h:716-814, SpellEntry;
// src/game/Spells/Spell.cpp:3023-3291, Spell::Prepare/Spell::cast;
// src/game/Entities/Unit.cpp:6988-7105, Unit::SpellBonusWithCoeffs.
// Upstream GPL-2.0-or-later; Copyright CMaNGOS contributors. No warranty.
//! Bounded effective captured numeric inputs, never an execution authorization.
use super::*;
use benilla_sim::mage_resource as mana;

#[cfg(test)]
#[path = "captured_lifecycle_tests.rs"]
mod lifecycle_tests;
#[cfg(test)]
#[path = "captured_shared_tests.rs"]
mod shared_tests;
#[cfg(test)]
#[path = "captured_tests.rs"]
mod tests;

/// Binary32 members are exact projections recomputed by the local decimal verifier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedSpellEffect {
    pub effect: u32,
    pub effect_die_sides: i32,
    pub effect_base_dice: u32,
    pub effect_dice_per_level: u32,
    pub effect_real_points_per_level: u32,
    pub effect_base_points: i32,
    pub effect_mechanic: u32,
    pub effect_implicit_target_a: u32,
    pub effect_implicit_target_b: u32,
    pub effect_radius_index: u32,
    pub effect_apply_aura_name: u32,
    pub effect_amplitude: u32,
    pub effect_multiple_value: u32,
    pub effect_chain_target: u32,
    pub effect_item_type: u32,
    pub effect_misc_value: i32,
    pub effect_trigger_spell: u32,
    pub effect_points_per_combo_point: u32,
    pub effect_bonus_coefficient: u32,
    pub effect_bonus_coefficient_from_ap: u32,
    pub dmg_multiplier: u32,
}
/// Complete pinned numeric SQL projection. Legacy partial overlays cannot construct this row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedSpellTemplate {
    pub id: u32,
    pub school: u32,
    pub category: u32,
    pub dispel: u32,
    pub mechanic: u32,
    pub attributes: u32,
    pub attributes_ex: u32,
    pub attributes_ex2: u32,
    pub attributes_ex3: u32,
    pub attributes_ex4: u32,
    pub stances: u32,
    pub stances_not: u32,
    pub targets: u32,
    pub target_creature_type: u32,
    pub requires_spell_focus: u32,
    pub caster_aura_state: u32,
    pub target_aura_state: u32,
    pub casting_time_index: u32,
    pub recovery_time: u32,
    pub category_recovery_time: u32,
    pub interrupt_flags: u32,
    pub aura_interrupt_flags: u32,
    pub channel_interrupt_flags: u32,
    pub proc_flags: u32,
    pub proc_chance: u32,
    pub proc_charges: u32,
    pub max_level: u32,
    pub base_level: u32,
    pub spell_level: u32,
    pub duration_index: u32,
    pub power_type: u32,
    pub mana_cost: u32,
    pub mana_cost_perlevel: u32,
    pub mana_per_second: u32,
    pub mana_per_second_per_level: u32,
    pub range_index: u32,
    pub speed: u32,
    pub stack_amount: u32,
    pub equipped_item_class: i32,
    pub equipped_item_sub_class_mask: i32,
    pub equipped_item_inventory_type_mask: i32,
    pub mana_cost_percentage: u32,
    pub start_recovery_category: u32,
    pub start_recovery_time: u32,
    pub spell_family_name: u32,
    pub dmg_class: u32,
    pub prevention_type: u32,
    pub cast_ui: u32,
    pub modal_next_spell: u32,
    pub totem1: u32,
    pub totem2: u32,
    pub reagent1: i32,
    pub reagent2: i32,
    pub reagent3: i32,
    pub reagent4: i32,
    pub reagent5: i32,
    pub reagent6: i32,
    pub reagent7: i32,
    pub reagent8: i32,
    pub reagent_count1: u32,
    pub reagent_count2: u32,
    pub reagent_count3: u32,
    pub reagent_count4: u32,
    pub reagent_count5: u32,
    pub reagent_count6: u32,
    pub reagent_count7: u32,
    pub reagent_count8: u32,
    pub spell_visual: u32,
    pub spell_icon_id: u32,
    pub active_icon_id: u32,
    pub spell_priority: u32,
    pub max_target_level: u32,
    pub max_affected_targets: u32,
    pub stance_bar_order: i32,
    pub min_faction_id: u32,
    pub min_reputation: u32,
    pub required_aura_vision: u32,
    pub spell_family_flags: u64,
    pub attributes_serverside: u32,
    pub is_server_side: u32,
    pub effects: [CapturedSpellEffect; 3],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedSpellFacing {
    pub entry: u32,
    pub facingcasterflag: u32,
}
pub struct CapturedFireballInput {
    pub templates: SpellSourceRows<CapturedSpellTemplate>,
    pub spells: SpellSourceRows<[u32; 173]>,
    pub cast_times: SpellSourceRows<SpellCastTimeSource>,
    pub ranges: SpellSourceRows<SpellRangeSource>,
    pub durations: SpellSourceRows<SpellDurationSource>,
    pub facing: SpellSourceRows<CapturedSpellFacing>,
    pub hooks: SpellHookSources,
}
pub struct CompiledFireball(SpellAuthoritySource);
fn one<T>(rows: SpellSourceRows<T>, label: &'static str) -> Result<T, NativeError> {
    rows.complete(label)?;
    let SpellSourceRows::Complete(mut rows) = rows else {
        unreachable!()
    };
    if rows.is_empty() {
        return Err(NativeError::AuthorityUnavailable(label));
    }
    if rows.len() != 1 {
        return Err(NativeError::InvalidProfile);
    }
    Ok(rows.pop().unwrap())
}
impl CapturedSpellTemplate {
    fn overlay(&self, words: &mut [u32; 173]) -> Result<(), NativeError> {
        // Every shared SQL numeric field must agree before any effective overwrite.
        let mut compare = |index: usize, value: u32, label: &'static str, float: bool| {
            if float {
                spell::SpellFloat::from_bits(value)?;
                spell::SpellFloat::from_bits(words[index])?;
            }
            if words[index] != value
                && !(float && words[index] & 0x7fff_ffff == 0 && value & 0x7fff_ffff == 0)
            {
                return Err(NativeError::Initialization(init::InitError::Invalid(label)));
            }
            words[index] = value;
            Ok(())
        };
        compare(0, self.id, "spell_template.133.Id SQL/DBC mismatch", false)?;
        compare(
            1,
            self.school,
            "spell_template.133.School SQL/DBC mismatch",
            false,
        )?;
        compare(
            2,
            self.category,
            "spell_template.133.Category SQL/DBC mismatch",
            false,
        )?;
        compare(
            4,
            self.dispel,
            "spell_template.133.Dispel SQL/DBC mismatch",
            false,
        )?;
        compare(
            5,
            self.mechanic,
            "spell_template.133.Mechanic SQL/DBC mismatch",
            false,
        )?;
        compare(
            6,
            self.attributes,
            "spell_template.133.Attributes SQL/DBC mismatch",
            false,
        )?;
        compare(
            7,
            self.attributes_ex,
            "spell_template.133.AttributesEx SQL/DBC mismatch",
            false,
        )?;
        compare(
            8,
            self.attributes_ex2,
            "spell_template.133.AttributesEx2 SQL/DBC mismatch",
            false,
        )?;
        compare(
            9,
            self.attributes_ex3,
            "spell_template.133.AttributesEx3 SQL/DBC mismatch",
            false,
        )?;
        compare(
            10,
            self.attributes_ex4,
            "spell_template.133.AttributesEx4 SQL/DBC mismatch",
            false,
        )?;
        compare(
            11,
            self.stances,
            "spell_template.133.Stances SQL/DBC mismatch",
            false,
        )?;
        compare(
            12,
            self.stances_not,
            "spell_template.133.StancesNot SQL/DBC mismatch",
            false,
        )?;
        compare(
            13,
            self.targets,
            "spell_template.133.Targets SQL/DBC mismatch",
            false,
        )?;
        compare(
            14,
            self.target_creature_type,
            "spell_template.133.TargetCreatureType SQL/DBC mismatch",
            false,
        )?;
        compare(
            15,
            self.requires_spell_focus,
            "spell_template.133.RequiresSpellFocus SQL/DBC mismatch",
            false,
        )?;
        compare(
            16,
            self.caster_aura_state,
            "spell_template.133.CasterAuraState SQL/DBC mismatch",
            false,
        )?;
        compare(
            17,
            self.target_aura_state,
            "spell_template.133.TargetAuraState SQL/DBC mismatch",
            false,
        )?;
        compare(
            18,
            self.casting_time_index,
            "spell_template.133.CastingTimeIndex SQL/DBC mismatch",
            false,
        )?;
        compare(
            19,
            self.recovery_time,
            "spell_template.133.RecoveryTime SQL/DBC mismatch",
            false,
        )?;
        compare(
            20,
            self.category_recovery_time,
            "spell_template.133.CategoryRecoveryTime SQL/DBC mismatch",
            false,
        )?;
        compare(
            21,
            self.interrupt_flags,
            "spell_template.133.InterruptFlags SQL/DBC mismatch",
            false,
        )?;
        compare(
            22,
            self.aura_interrupt_flags,
            "spell_template.133.AuraInterruptFlags SQL/DBC mismatch",
            false,
        )?;
        compare(
            23,
            self.channel_interrupt_flags,
            "spell_template.133.ChannelInterruptFlags SQL/DBC mismatch",
            false,
        )?;
        compare(
            24,
            self.proc_flags,
            "spell_template.133.procFlags SQL/DBC mismatch",
            false,
        )?;
        compare(
            25,
            self.proc_chance,
            "spell_template.133.procChance SQL/DBC mismatch",
            false,
        )?;
        compare(
            26,
            self.proc_charges,
            "spell_template.133.procCharges SQL/DBC mismatch",
            false,
        )?;
        compare(
            27,
            self.max_level,
            "spell_template.133.maxLevel SQL/DBC mismatch",
            false,
        )?;
        compare(
            28,
            self.base_level,
            "spell_template.133.baseLevel SQL/DBC mismatch",
            false,
        )?;
        compare(
            29,
            self.spell_level,
            "spell_template.133.spellLevel SQL/DBC mismatch",
            false,
        )?;
        compare(
            30,
            self.duration_index,
            "spell_template.133.DurationIndex SQL/DBC mismatch",
            false,
        )?;
        compare(
            31,
            self.power_type,
            "spell_template.133.powerType SQL/DBC mismatch",
            false,
        )?;
        compare(
            32,
            self.mana_cost,
            "spell_template.133.manaCost SQL/DBC mismatch",
            false,
        )?;
        compare(
            33,
            self.mana_cost_perlevel,
            "spell_template.133.manaCostPerlevel SQL/DBC mismatch",
            false,
        )?;
        compare(
            34,
            self.mana_per_second,
            "spell_template.133.manaPerSecond SQL/DBC mismatch",
            false,
        )?;
        compare(
            35,
            self.mana_per_second_per_level,
            "spell_template.133.manaPerSecondPerLevel SQL/DBC mismatch",
            false,
        )?;
        compare(
            36,
            self.range_index,
            "spell_template.133.rangeIndex SQL/DBC mismatch",
            false,
        )?;
        compare(
            37,
            self.speed,
            "spell_template.133.speed SQL/DBC mismatch",
            true,
        )?;
        compare(
            39,
            self.stack_amount,
            "spell_template.133.StackAmount SQL/DBC mismatch",
            false,
        )?;
        compare(
            58,
            self.equipped_item_class as u32,
            "spell_template.133.EquippedItemClass SQL/DBC mismatch",
            false,
        )?;
        compare(
            59,
            self.equipped_item_sub_class_mask as u32,
            "spell_template.133.EquippedItemSubClassMask SQL/DBC mismatch",
            false,
        )?;
        compare(
            60,
            self.equipped_item_inventory_type_mask as u32,
            "spell_template.133.EquippedItemInventoryTypeMask SQL/DBC mismatch",
            false,
        )?;
        compare(
            156,
            self.mana_cost_percentage,
            "spell_template.133.ManaCostPercentage SQL/DBC mismatch",
            false,
        )?;
        compare(
            157,
            self.start_recovery_category,
            "spell_template.133.StartRecoveryCategory SQL/DBC mismatch",
            false,
        )?;
        compare(
            158,
            self.start_recovery_time,
            "spell_template.133.StartRecoveryTime SQL/DBC mismatch",
            false,
        )?;
        compare(
            160,
            self.spell_family_name,
            "spell_template.133.SpellFamilyName SQL/DBC mismatch",
            false,
        )?;
        compare(
            164,
            self.dmg_class,
            "spell_template.133.DmgClass SQL/DBC mismatch",
            false,
        )?;
        compare(
            165,
            self.prevention_type,
            "spell_template.133.PreventionType SQL/DBC mismatch",
            false,
        )?;
        compare(
            3,
            self.cast_ui,
            "spell_template.133.CastUI SQL/DBC mismatch",
            false,
        )?;
        compare(
            38,
            self.modal_next_spell,
            "spell_template.133.ModalNextSpell SQL/DBC mismatch",
            false,
        )?;
        compare(
            40,
            self.totem1,
            "spell_template.133.Totem1 SQL/DBC mismatch",
            false,
        )?;
        compare(
            41,
            self.totem2,
            "spell_template.133.Totem2 SQL/DBC mismatch",
            false,
        )?;
        compare(
            42,
            self.reagent1 as u32,
            "spell_template.133.Reagent1 SQL/DBC mismatch",
            false,
        )?;
        compare(
            43,
            self.reagent2 as u32,
            "spell_template.133.Reagent2 SQL/DBC mismatch",
            false,
        )?;
        compare(
            44,
            self.reagent3 as u32,
            "spell_template.133.Reagent3 SQL/DBC mismatch",
            false,
        )?;
        compare(
            45,
            self.reagent4 as u32,
            "spell_template.133.Reagent4 SQL/DBC mismatch",
            false,
        )?;
        compare(
            46,
            self.reagent5 as u32,
            "spell_template.133.Reagent5 SQL/DBC mismatch",
            false,
        )?;
        compare(
            47,
            self.reagent6 as u32,
            "spell_template.133.Reagent6 SQL/DBC mismatch",
            false,
        )?;
        compare(
            48,
            self.reagent7 as u32,
            "spell_template.133.Reagent7 SQL/DBC mismatch",
            false,
        )?;
        compare(
            49,
            self.reagent8 as u32,
            "spell_template.133.Reagent8 SQL/DBC mismatch",
            false,
        )?;
        compare(
            50,
            self.reagent_count1,
            "spell_template.133.ReagentCount1 SQL/DBC mismatch",
            false,
        )?;
        compare(
            51,
            self.reagent_count2,
            "spell_template.133.ReagentCount2 SQL/DBC mismatch",
            false,
        )?;
        compare(
            52,
            self.reagent_count3,
            "spell_template.133.ReagentCount3 SQL/DBC mismatch",
            false,
        )?;
        compare(
            53,
            self.reagent_count4,
            "spell_template.133.ReagentCount4 SQL/DBC mismatch",
            false,
        )?;
        compare(
            54,
            self.reagent_count5,
            "spell_template.133.ReagentCount5 SQL/DBC mismatch",
            false,
        )?;
        compare(
            55,
            self.reagent_count6,
            "spell_template.133.ReagentCount6 SQL/DBC mismatch",
            false,
        )?;
        compare(
            56,
            self.reagent_count7,
            "spell_template.133.ReagentCount7 SQL/DBC mismatch",
            false,
        )?;
        compare(
            57,
            self.reagent_count8,
            "spell_template.133.ReagentCount8 SQL/DBC mismatch",
            false,
        )?;
        compare(
            115,
            self.spell_visual,
            "spell_template.133.SpellVisual SQL/DBC mismatch",
            false,
        )?;
        compare(
            117,
            self.spell_icon_id,
            "spell_template.133.SpellIconID SQL/DBC mismatch",
            false,
        )?;
        compare(
            118,
            self.active_icon_id,
            "spell_template.133.ActiveIconID SQL/DBC mismatch",
            false,
        )?;
        compare(
            119,
            self.spell_priority,
            "spell_template.133.SpellPriority SQL/DBC mismatch",
            false,
        )?;
        compare(
            159,
            self.max_target_level,
            "spell_template.133.MaxTargetLevel SQL/DBC mismatch",
            false,
        )?;
        compare(
            163,
            self.max_affected_targets,
            "spell_template.133.MaxAffectedTargets SQL/DBC mismatch",
            false,
        )?;
        compare(
            166,
            self.stance_bar_order as u32,
            "spell_template.133.StanceBarOrder SQL/DBC mismatch",
            false,
        )?;
        compare(
            170,
            self.min_faction_id,
            "spell_template.133.MinFactionId SQL/DBC mismatch",
            false,
        )?;
        compare(
            171,
            self.min_reputation,
            "spell_template.133.MinReputation SQL/DBC mismatch",
            false,
        )?;
        compare(
            172,
            self.required_aura_vision,
            "spell_template.133.RequiredAuraVision SQL/DBC mismatch",
            false,
        )?;
        compare(
            161,
            self.spell_family_flags as u32,
            "spell_template.133.SpellFamilyFlags low SQL/DBC mismatch",
            false,
        )?;
        compare(
            162,
            (self.spell_family_flags >> 32) as u32,
            "spell_template.133.SpellFamilyFlags high SQL/DBC mismatch",
            false,
        )?;
        for (i, effect) in self.effects.iter().enumerate() {
            compare(
                61 + i,
                effect.effect,
                "spell_template.133.Effect SQL/DBC mismatch",
                false,
            )?;
            compare(
                64 + i,
                effect.effect_die_sides as u32,
                "spell_template.133.EffectDieSides SQL/DBC mismatch",
                false,
            )?;
            compare(
                67 + i,
                effect.effect_base_dice,
                "spell_template.133.EffectBaseDice SQL/DBC mismatch",
                false,
            )?;
            compare(
                70 + i,
                effect.effect_dice_per_level,
                "spell_template.133.EffectDicePerLevel SQL/DBC mismatch",
                true,
            )?;
            compare(
                73 + i,
                effect.effect_real_points_per_level,
                "spell_template.133.EffectRealPointsPerLevel SQL/DBC mismatch",
                true,
            )?;
            compare(
                76 + i,
                effect.effect_base_points as u32,
                "spell_template.133.EffectBasePoints SQL/DBC mismatch",
                false,
            )?;
            compare(
                79 + i,
                effect.effect_mechanic,
                "spell_template.133.EffectMechanic SQL/DBC mismatch",
                false,
            )?;
            compare(
                82 + i,
                effect.effect_implicit_target_a,
                "spell_template.133.EffectImplicitTargetA SQL/DBC mismatch",
                false,
            )?;
            compare(
                85 + i,
                effect.effect_implicit_target_b,
                "spell_template.133.EffectImplicitTargetB SQL/DBC mismatch",
                false,
            )?;
            compare(
                88 + i,
                effect.effect_radius_index,
                "spell_template.133.EffectRadiusIndex SQL/DBC mismatch",
                false,
            )?;
            compare(
                91 + i,
                effect.effect_apply_aura_name,
                "spell_template.133.EffectApplyAuraName SQL/DBC mismatch",
                false,
            )?;
            compare(
                94 + i,
                effect.effect_amplitude,
                "spell_template.133.EffectAmplitude SQL/DBC mismatch",
                false,
            )?;
            compare(
                97 + i,
                effect.effect_multiple_value,
                "spell_template.133.EffectMultipleValue SQL/DBC mismatch",
                true,
            )?;
            compare(
                100 + i,
                effect.effect_chain_target,
                "spell_template.133.EffectChainTarget SQL/DBC mismatch",
                false,
            )?;
            compare(
                103 + i,
                effect.effect_item_type,
                "spell_template.133.EffectItemType SQL/DBC mismatch",
                false,
            )?;
            compare(
                106 + i,
                effect.effect_misc_value as u32,
                "spell_template.133.EffectMiscValue SQL/DBC mismatch",
                false,
            )?;
            compare(
                109 + i,
                effect.effect_trigger_spell,
                "spell_template.133.EffectTriggerSpell SQL/DBC mismatch",
                false,
            )?;
            compare(
                112 + i,
                effect.effect_points_per_combo_point,
                "spell_template.133.EffectPointsPerComboPoint SQL/DBC mismatch",
                true,
            )?;
            compare(
                167 + i,
                effect.dmg_multiplier,
                "spell_template.133.DmgMultiplier SQL/DBC mismatch",
                true,
            )?;
        }
        Ok(())
    }
}
impl CapturedFireballInput {
    pub fn compile(self) -> Result<CompiledFireball, NativeError> {
        // Bound every collection before constructing or cloning the effective row.
        self.templates
            .complete("spell_template complete numeric overlay")?;
        self.spells.complete("Spell.133")?;
        self.cast_times.complete("SpellCastTimes.16")?;
        self.ranges.complete("SpellRange.35")?;
        self.durations.complete("SpellDuration.35")?;
        self.facing.complete("spell_facing.133")?;
        self.hooks
            .spell_script_names
            .complete("spell script source closure")?;
        for rows in [
            &self.hooks.conditions,
            &self.hooks.spell_proc_event,
            &self.hooks.spell_affect,
            &self.hooks.spell_bonus_data,
            &self.hooks.spell_threat,
            &self.hooks.spell_elixir,
        ] {
            if matches!(rows, SpellSourceRows::Complete(rows) if rows.len() > 4096) {
                return Err(NativeError::TooLarge);
            }
        }
        if matches!(&self.hooks.spell_script_names, SpellSourceRows::Complete(rows)
            if rows.iter().any(|name| name.len() > 256))
        {
            return Err(NativeError::TooLarge);
        }
        if matches!(&self.hooks.spell_group_membership, SpellSourceRows::Complete(rows)
            if rows.len() > 4096 || rows.iter().any(|row| row.name.len() > 256 || row.spell_ids.len() > 4096))
        {
            return Err(NativeError::TooLarge);
        }
        self.hooks.validate()?;
        let template = one(self.templates, "spell_template complete numeric overlay")?;
        let mut words = one(self.spells, "Spell.133")?;
        if template.id != 133 || words[0] != 133 {
            return Err(NativeError::InvalidProfile);
        }
        template.overlay(&mut words)?;
        for (index, expected) in [
            (0, 0x85),
            (1, 0x2),
            (2, 0x0),
            (4, 0x0),
            (5, 0x0),
            (6, 0x10000),
            (7, 0x0),
            (8, 0x0),
            (9, 0x0),
            (10, 0x0),
            (11, 0x0),
            (12, 0x0),
            (13, 0x0),
            (14, 0x0),
            (15, 0x0),
            (16, 0x0),
            (17, 0x0),
            (18, 0x10),
            (19, 0x0),
            (20, 0x0),
            (21, 0xf),
            (22, 0x0),
            (23, 0x0),
            (24, 0x0),
            (25, 0x65),
            (26, 0x0),
            (27, 0x5),
            (28, 0x1),
            (29, 0x1),
            (30, 0x23),
            (31, 0x0),
            (32, 0x1e),
            (33, 0x0),
            (34, 0x0),
            (35, 0x0),
            (36, 0x23),
            (37, 0x41c00000),
            (39, 0x0),
            (40, 0x0),
            (41, 0x0),
            (42, 0x0),
            (43, 0x0),
            (44, 0x0),
            (45, 0x0),
            (46, 0x0),
            (47, 0x0),
            (48, 0x0),
            (49, 0x0),
            (50, 0x0),
            (51, 0x0),
            (52, 0x0),
            (53, 0x0),
            (54, 0x0),
            (55, 0x0),
            (56, 0x0),
            (57, 0x0),
            (58, 0xffffffff),
            (59, 0xffffffff),
            (60, 0x0),
            (61, 0x2),
            (62, 0x6),
            (63, 0x0),
            (64, 0x9),
            (65, 0x1),
            (66, 0x0),
            (67, 0x1),
            (68, 0x1),
            (69, 0x0),
            (70, 0x0),
            (71, 0x0),
            (72, 0x0),
            (73, 0x3f19999a),
            (74, 0x0),
            (75, 0x0),
            (76, 0xd),
            (77, 0x0),
            (78, 0x0),
            (79, 0x0),
            (80, 0x0),
            (81, 0x0),
            (82, 0x6),
            (83, 0x6),
            (84, 0x0),
            (85, 0x0),
            (86, 0x0),
            (87, 0x0),
            (88, 0x0),
            (89, 0x0),
            (90, 0x0),
            (91, 0x0),
            (92, 0x3),
            (93, 0x0),
            (94, 0x0),
            (95, 0x7d0),
            (96, 0x0),
            (97, 0x0),
            (98, 0x0),
            (99, 0x0),
            (100, 0x0),
            (101, 0x0),
            (102, 0x0),
            (103, 0x0),
            (104, 0x0),
            (105, 0x0),
            (106, 0x0),
            (107, 0x0),
            (108, 0x0),
            (109, 0x0),
            (110, 0x0),
            (111, 0x0),
            (112, 0x0),
            (113, 0x0),
            (114, 0x0),
            (156, 0x0),
            (157, 0x85),
            (158, 0x5dc),
            (159, 0x0),
            (160, 0x3),
            (161, 0x40000001),
            (162, 0x0),
            (163, 0x0),
            (164, 0x1),
            (165, 0x1),
            (167, 0x3f800000),
            (168, 0x3f800000),
            (169, 0x3f800000),
            (170, 0x0),
            (171, 0x0),
            (172, 0x0),
        ] {
            if words[index] != expected {
                return Err(NativeError::UnsupportedMechanic(
                    "captured Fireball exact numeric shape",
                ));
            }
        }
        let cast_time = one(self.cast_times, "SpellCastTimes.16")?;
        let duration = one(self.durations, "SpellDuration.35")?;
        let range = one(self.ranges, "SpellRange.35")?;
        if cast_time.id != 16 || duration.id != 35 || range.id != 35 {
            return Err(NativeError::InvalidProfile);
        }
        if (
            cast_time.base_ms,
            cast_time.per_level_ms,
            cast_time.minimum_ms,
        ) != (1500, 0, 1500)
            || (duration.base_ms, duration.per_level_ms, duration.maximum_ms) != (4000, 0, 4000)
            || (range.minimum.bits(), range.maximum.bits(), range.flags) != (0, 0x420c_0000, 0)
            || template.is_server_side != 0
            || template.attributes_serverside != 0
            || template
                .effects
                .each_ref()
                .map(|e| e.effect_bonus_coefficient)
                != [FIREBALL_BONUS_COEFFICIENT, 0, 0]
            || template
                .effects
                .iter()
                .any(|e| e.effect_bonus_coefficient_from_ap != 0)
        {
            return Err(NativeError::UnsupportedMechanic(
                "captured Fireball auxiliary or server shape",
            ));
        }
        let SpellSourceRows::Complete(facing) = self.facing else {
            unreachable!()
        };
        if facing.len() > 1 || facing.iter().any(|r| r.entry != 133) {
            return Err(NativeError::InvalidProfile);
        }
        let facing_flags = facing.first().map_or(0, |r| r.facingcasterflag);
        let source = SpellAuthoritySource {
            words: words.to_vec(),
            server_side: template.is_server_side,
            server_attributes: template.attributes_serverside,
            bonus_coefficients: template
                .effects
                .each_ref()
                .map(|e| e.effect_bonus_coefficient),
            attack_power_coefficients: template
                .effects
                .each_ref()
                .map(|e| e.effect_bonus_coefficient_from_ap),
            cast_time,
            duration: Some(duration),
            range,
            category_flags: 0,
            facing_flags,
            hooks: self.hooks,
        };
        source.validate()?;
        Ok(CompiledFireball(source))
    }
}
impl CompiledFireball {
    pub fn source(&self) -> &SpellAuthoritySource {
        &self.0
    }
    pub fn into_source(self) -> SpellAuthoritySource {
        self.0
    }
    pub fn hit_policy(&self) -> SpellHitPolicy {
        let w = &self.0.words;
        SpellHitPolicy {
            attributes: w[6..11].try_into().unwrap(),
            implicit_target_a: w[82..85].try_into().unwrap(),
            implicit_target_b: w[85..88].try_into().unwrap(),
            aura_interrupt_flags: w[22],
        }
    }
    pub fn ability_recipe(
        &self,
        starter: &init::StarterInput,
        initialized: &init::InitializedMage,
    ) -> Result<AbilityRecipe, NativeError> {
        if starter.spells.len() > 4096 || initialized.skills.len() > 4096 {
            return Err(NativeError::TooLarge);
        }
        let mut candidates = starter.spells.iter().filter(|r| r.id == 133);
        let raw = candidates
            .next()
            .ok_or(NativeError::AuthorityUnavailable("initializer Spell.133"))?;
        if candidates.next().is_some()
            || raw.words.as_slice() != self.0.words
            || initialized.create_mana != starter.create_mana
        {
            return Err(NativeError::AuthorityBinding);
        }
        let known = initialized
            .abilities
            .get(&133)
            .ok_or(NativeError::AuthorityUnavailable("initialized Fireball"))?;
        if known.spell_id != 133
            || !known.active
            || known.availability != init::AbilityAvailability::Available
        {
            return Err(NativeError::AuthorityBinding);
        }
        let init::Rows::Complete(rows) = &starter.skill_abilities else {
            return Err(NativeError::AuthorityUnavailable(
                "SkillLineAbility complete mappings",
            ));
        };
        if rows.len() > 4096 {
            return Err(NativeError::TooLarge);
        }
        let links = mana::SkillLineAbilities::new(
            rows.iter()
                .map(|r| mana::SkillLineAbilityRow {
                    row_id: r.row_id,
                    spell_id: r.spell,
                    skill_id: r.skill.into(),
                })
                .collect(),
        )?;
        let skills = mana::CurrentSkills::new(
            initialized
                .skills
                .values()
                .map(|s| mana::CurrentSkill {
                    id: s.id,
                    value: s.value,
                    permanent_bonus: s.permanent_bonus,
                    temporary_bonus: s.temporary_bonus,
                    deleted: false,
                })
                .collect(),
        )?;
        let w = &self.0.words;
        let rank = u16::try_from(mana::spell_rank(133, w[27], &links, &skills)?)
            .map_err(|_| NativeError::Limit)?;
        if rank == 0 {
            return Err(NativeError::AuthorityUnavailable(
                "initialized Fireball skill rank",
            ));
        }
        let time = self.0.cast_time;
        let spec = spell::MageProjectileSpec {
            spell_id: 133,
            effects: [self.0.effect(0)?, self.0.effect(1)?, self.0.effect(2)?],
            spell_rank: rank,
            cast_ms: spell::cast_time(
                time.base_ms,
                time.per_level_ms,
                time.minimum_ms,
                rank,
                w[28] as u8,
            )?,
            mana_cost: spell::mana_cost(
                w[32],
                w[33],
                w[156],
                initialized.create_mana,
                rank,
                w[28] as u8,
            )?,
            global_category: w[157],
            global_recovery_ms: w[158],
            recovery_ms: w[19],
            category: w[2],
            category_recovery_ms: w[20],
            projectile_speed: spell::SpellFloat::from_bits(w[37])?,
            movement_interrupts: w[21] & INTERRUPT_MOVEMENT != 0,
            damage_pushback: w[21] & INTERRUPT_DAMAGE_PUSHBACK != 0,
        };
        self.0.matches_spec(2, &spec)?;
        self.0.matches_cost(&spec, initialized.create_mana)?;
        Ok(AbilityRecipe {
            ability: Ability {
                id: 133,
                school: 2,
                admission: AbilityAdmission::Available(spec),
            },
            cast_time: Some(CastTimeRow {
                id: time.id,
                base_ms: time.base_ms,
                per_level_ms: time.per_level_ms,
                minimum_ms: time.minimum_ms,
            }),
        })
    }
}
