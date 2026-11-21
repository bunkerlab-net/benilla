// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored actual-shaped numeric rows, never extracted installation data.
use super::*;
use benilla_sim::starter_init as init;

pub fn captured_fireball_input() -> CapturedFireballInput {
    let case: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../scripts/native_starter/fireball-authored.json"
    )))
    .unwrap();
    let row = &case["numeric_projection"];
    let u = |name: &str| u32::try_from(row[name].as_u64().unwrap()).unwrap();
    let s = |name: &str| i32::try_from(row[name].as_i64().unwrap()).unwrap();
    let effects = std::array::from_fn(|i| CapturedSpellEffect {
        effect: u(["Effect1", "Effect2", "Effect3"][i]),
        effect_die_sides: s(["EffectDieSides1", "EffectDieSides2", "EffectDieSides3"][i]),
        effect_base_dice: u(["EffectBaseDice1", "EffectBaseDice2", "EffectBaseDice3"][i]),
        effect_dice_per_level: u([
            "EffectDicePerLevel1",
            "EffectDicePerLevel2",
            "EffectDicePerLevel3",
        ][i]),
        effect_real_points_per_level: u([
            "EffectRealPointsPerLevel1",
            "EffectRealPointsPerLevel2",
            "EffectRealPointsPerLevel3",
        ][i]),
        effect_base_points: s([
            "EffectBasePoints1",
            "EffectBasePoints2",
            "EffectBasePoints3",
        ][i]),
        effect_mechanic: u(["EffectMechanic1", "EffectMechanic2", "EffectMechanic3"][i]),
        effect_implicit_target_a: u([
            "EffectImplicitTargetA1",
            "EffectImplicitTargetA2",
            "EffectImplicitTargetA3",
        ][i]),
        effect_implicit_target_b: u([
            "EffectImplicitTargetB1",
            "EffectImplicitTargetB2",
            "EffectImplicitTargetB3",
        ][i]),
        effect_radius_index: u([
            "EffectRadiusIndex1",
            "EffectRadiusIndex2",
            "EffectRadiusIndex3",
        ][i]),
        effect_apply_aura_name: u([
            "EffectApplyAuraName1",
            "EffectApplyAuraName2",
            "EffectApplyAuraName3",
        ][i]),
        effect_amplitude: u(["EffectAmplitude1", "EffectAmplitude2", "EffectAmplitude3"][i]),
        effect_multiple_value: u([
            "EffectMultipleValue1",
            "EffectMultipleValue2",
            "EffectMultipleValue3",
        ][i]),
        effect_chain_target: u([
            "EffectChainTarget1",
            "EffectChainTarget2",
            "EffectChainTarget3",
        ][i]),
        effect_item_type: u(["EffectItemType1", "EffectItemType2", "EffectItemType3"][i]),
        effect_misc_value: s(["EffectMiscValue1", "EffectMiscValue2", "EffectMiscValue3"][i]),
        effect_trigger_spell: u([
            "EffectTriggerSpell1",
            "EffectTriggerSpell2",
            "EffectTriggerSpell3",
        ][i]),
        effect_points_per_combo_point: u([
            "EffectPointsPerComboPoint1",
            "EffectPointsPerComboPoint2",
            "EffectPointsPerComboPoint3",
        ][i]),
        effect_bonus_coefficient: u([
            "EffectBonusCoefficient1",
            "EffectBonusCoefficient2",
            "EffectBonusCoefficient3",
        ][i]),
        effect_bonus_coefficient_from_ap: u([
            "EffectBonusCoefficientFromAP1",
            "EffectBonusCoefficientFromAP2",
            "EffectBonusCoefficientFromAP3",
        ][i]),
        dmg_multiplier: u(["DmgMultiplier1", "DmgMultiplier2", "DmgMultiplier3"][i]),
    });
    let template = CapturedSpellTemplate {
        id: u("Id"),
        school: u("School"),
        category: u("Category"),
        dispel: u("Dispel"),
        mechanic: u("Mechanic"),
        attributes: u("Attributes"),
        attributes_ex: u("AttributesEx"),
        attributes_ex2: u("AttributesEx2"),
        attributes_ex3: u("AttributesEx3"),
        attributes_ex4: u("AttributesEx4"),
        stances: u("Stances"),
        stances_not: u("StancesNot"),
        targets: u("Targets"),
        target_creature_type: u("TargetCreatureType"),
        requires_spell_focus: u("RequiresSpellFocus"),
        caster_aura_state: u("CasterAuraState"),
        target_aura_state: u("TargetAuraState"),
        casting_time_index: u("CastingTimeIndex"),
        recovery_time: u("RecoveryTime"),
        category_recovery_time: u("CategoryRecoveryTime"),
        interrupt_flags: u("InterruptFlags"),
        aura_interrupt_flags: u("AuraInterruptFlags"),
        channel_interrupt_flags: u("ChannelInterruptFlags"),
        proc_flags: u("procFlags"),
        proc_chance: u("procChance"),
        proc_charges: u("procCharges"),
        max_level: u("maxLevel"),
        base_level: u("baseLevel"),
        spell_level: u("spellLevel"),
        duration_index: u("DurationIndex"),
        power_type: u("powerType"),
        mana_cost: u("manaCost"),
        mana_cost_perlevel: u("manaCostPerlevel"),
        mana_per_second: u("manaPerSecond"),
        mana_per_second_per_level: u("manaPerSecondPerLevel"),
        range_index: u("rangeIndex"),
        speed: u("speed"),
        stack_amount: u("StackAmount"),
        equipped_item_class: s("EquippedItemClass"),
        equipped_item_sub_class_mask: s("EquippedItemSubClassMask"),
        equipped_item_inventory_type_mask: s("EquippedItemInventoryTypeMask"),
        mana_cost_percentage: u("ManaCostPercentage"),
        start_recovery_category: u("StartRecoveryCategory"),
        start_recovery_time: u("StartRecoveryTime"),
        spell_family_name: u("SpellFamilyName"),
        dmg_class: u("DmgClass"),
        prevention_type: u("PreventionType"),
        cast_ui: u("CastUI"),
        modal_next_spell: u("ModalNextSpell"),
        totem1: u("Totem1"),
        totem2: u("Totem2"),
        reagent1: s("Reagent1"),
        reagent2: s("Reagent2"),
        reagent3: s("Reagent3"),
        reagent4: s("Reagent4"),
        reagent5: s("Reagent5"),
        reagent6: s("Reagent6"),
        reagent7: s("Reagent7"),
        reagent8: s("Reagent8"),
        reagent_count1: u("ReagentCount1"),
        reagent_count2: u("ReagentCount2"),
        reagent_count3: u("ReagentCount3"),
        reagent_count4: u("ReagentCount4"),
        reagent_count5: u("ReagentCount5"),
        reagent_count6: u("ReagentCount6"),
        reagent_count7: u("ReagentCount7"),
        reagent_count8: u("ReagentCount8"),
        spell_visual: u("SpellVisual"),
        spell_icon_id: u("SpellIconID"),
        active_icon_id: u("ActiveIconID"),
        spell_priority: u("SpellPriority"),
        max_target_level: u("MaxTargetLevel"),
        max_affected_targets: u("MaxAffectedTargets"),
        stance_bar_order: s("StanceBarOrder"),
        min_faction_id: u("MinFactionId"),
        min_reputation: u("MinReputation"),
        required_aura_vision: u("RequiredAuraVision"),
        spell_family_flags: row["SpellFamilyFlags"].as_u64().unwrap(),
        attributes_serverside: u("AttributesServerside"),
        is_server_side: u("IsServerSide"),
        effects,
    };
    assert_eq!(case["words"].as_array().unwrap().len(), 173);
    let words = std::array::from_fn(|i| u32::try_from(case["words"][i].as_u64().unwrap()).unwrap());
    CapturedFireballInput {
        templates: SpellSourceRows::Complete(vec![template]),
        spells: SpellSourceRows::Complete(vec![words]),
        cast_times: SpellSourceRows::Complete(vec![SpellCastTimeSource {
            id: 16,
            base_ms: 1500,
            per_level_ms: 0,
            minimum_ms: 1500,
        }]),
        durations: SpellSourceRows::Complete(vec![SpellDurationSource {
            id: 35,
            base_ms: 4000,
            per_level_ms: 0,
            maximum_ms: 4000,
        }]),
        ranges: SpellSourceRows::Complete(vec![SpellRangeSource {
            id: 35,
            minimum: spell::SpellFloat::ZERO,
            maximum: spell::SpellFloat::from_bits(0x420c0000).unwrap(),
            flags: 0,
        }]),
        facing: SpellSourceRows::Complete(vec![]),
        hooks: synthetic_spell_fixture::hooks(),
    }
}
pub fn captured_fireball_inputs() -> (Profile, init::StarterInput, SpellAuthorityInput) {
    let compiled = captured_fireball_input().compile().unwrap();
    let mut p = synthetic_combat_fixture::profile();
    let mut initial = synthetic_fixture::initialized_starter_input();
    synthetic_spell_fixture::rekey(&mut p, &mut initial);
    initial.items[0].max_durability = 0;
    initial.spells[0].words = compiled.source().words.clone().try_into().unwrap();
    initial.spells[0].duration_ms = Some(4000);
    let initialized = init::initialize_mage(&initial).unwrap();
    let recipe = compiled.ability_recipe(&initial, &initialized).unwrap();
    p.abilities[0] = recipe.ability;
    p.source_motion
        .as_mut()
        .unwrap()
        .spell_hits
        .insert(133, compiled.hit_policy());
    p.npcs[0].max_health = 10000;
    p.initialize_player(&initial, synthetic_fixture::float(100.0), 1)
        .unwrap();
    let source = synthetic_spell_fixture::context(&p, vec![compiled.into_source()]);
    (p, initial, source)
}
pub fn captured_fireball_profile() -> Profile {
    let (mut p, initial, sources) = captured_fireball_inputs();
    p.initialize_spell_authority(&initial, sources).unwrap();
    synthetic_spell_fixture::initialize_authored_location(&mut p);
    p
}
