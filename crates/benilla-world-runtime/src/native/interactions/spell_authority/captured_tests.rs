// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored captured adapter and unchanged final authority boundary regressions.
use super::*;
use crate::native::{
    synthetic_captured_spell_fixture as captured, synthetic_fixture as f,
    synthetic_spell_fixture as spatial,
};

pub(super) fn template(input: &mut CapturedFireballInput) -> &mut CapturedSpellTemplate {
    let SpellSourceRows::Complete(rows) = &mut input.templates else {
        unreachable!()
    };
    &mut rows[0]
}
pub(super) fn words(input: &mut CapturedFireballInput) -> &mut [u32; 173] {
    let SpellSourceRows::Complete(rows) = &mut input.spells else {
        unreachable!()
    };
    &mut rows[0]
}
#[test]
fn authored_exact_capture_generates_existing_recipe_source_and_hit_policy() {
    let compiled = captured::captured_fireball_input().compile().unwrap();
    assert_eq!(compiled.source().bonus_coefficients, [0x3dfbe76d, 0, 0]);
    assert_eq!(compiled.source().words[169], 0x3f800000);
    assert_eq!(compiled.hit_policy().attributes, [65536, 0, 0, 0, 0]);
    assert_eq!(compiled.hit_policy().implicit_target_a, [6, 6, 0]);
    let (_, initial, _) = captured::captured_fireball_inputs();
    let mut mage = init::initialize_mage(&initial).unwrap();
    let init::Rows::Complete(links) = &initial.skill_abilities else {
        unreachable!()
    };
    let skill = links.iter().min_by_key(|r| r.row_id).unwrap().skill;
    for rank in [5, 10, 25, 30] {
        mage.skills.get_mut(&skill).unwrap().value = rank;
        let recipe = compiled.ability_recipe(&initial, &mage).unwrap();
        let AbilityAdmission::Available(spec) = recipe.ability.admission else {
            unreachable!()
        };
        assert_eq!(spec.spell_rank, rank.min(25));
        assert_eq!(
            (spec.cast_ms, spec.mana_cost, spec.global_recovery_ms),
            (1500, 30, 1500)
        );
        assert_eq!(spec.projectile_speed.bits(), 0x41c00000);
        let spell::MageEffect::SchoolDamage(direct) = spec.effects[0] else {
            unreachable!()
        };
        assert_eq!(
            (direct.base_points, direct.base_dice, direct.die_sides),
            (13, 1, 9)
        );
        assert_eq!(direct.points_per_level.bits(), 0x3f19999a);
        let spell::MageEffect::PeriodicDamage {
            value,
            amplitude_ms,
            duration_ms,
        } = spec.effects[1]
        else {
            unreachable!()
        };
        assert_eq!(
            (
                value.base_points,
                value.base_dice,
                value.die_sides,
                amplitude_ms,
                duration_ms
            ),
            (0, 1, 1, 2000, 4000)
        );
        assert_eq!(spec.effects[2], spell::MageEffect::Empty);
    }
    let mut competing = initial.clone();
    let init::Rows::Complete(links) = &mut competing.skill_abilities else {
        unreachable!()
    };
    let mut earlier = links[0];
    earlier.row_id = 0;
    earlier.skill = 95;
    links.push(earlier);
    let mut competing_mage = init::initialize_mage(&initial).unwrap();
    competing_mage.skills.get_mut(&95).unwrap().value = 14;
    let AbilityAdmission::Available(spec) = compiled
        .ability_recipe(&competing, &competing_mage)
        .unwrap()
        .ability
        .admission
    else {
        unreachable!()
    };
    assert_eq!(spec.spell_rank, 14);
    mage.create_mana += 1;
    assert!(compiled.ability_recipe(&initial, &mage).is_err());
    mage.create_mana -= 1;
    mage.skills.clear();
    assert!(compiled.ability_recipe(&initial, &mage).is_err());
    let mut wrong = initial.clone();
    wrong.spells[0].words[115] += 1;
    assert!(compiled
        .ability_recipe(&wrong, &init::initialize_mage(&initial).unwrap())
        .is_err());
    wrong = initial.clone();
    wrong.skill_abilities = init::Rows::Missing;
    assert!(compiled
        .ability_recipe(&wrong, &init::initialize_mage(&initial).unwrap())
        .is_err());
}
#[test]
fn every_shared_numeric_column_mismatch_refuses_before_overlay() {
    let original = captured::captured_fireball_input().compile().unwrap();
    for index in [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
        25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47,
        48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70,
        71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93,
        94, 95, 96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112,
        113, 114, 115, 117, 118, 119, 156, 157, 158, 159, 160, 161, 162, 163, 164, 165, 166, 167,
        168, 169, 170, 171, 172,
    ] {
        let mut input = captured::captured_fireball_input();
        words(&mut input)[index] ^= 1;
        assert!(input.compile().is_err(), "shared SQL/DBC word {index}");
    }
    assert_eq!(original.source().bonus_coefficients[0], 0x3dfbe76d);
    let mut input = captured::captured_fireball_input();
    for index in [116, 128, 137, 146, 155] {
        words(&mut input)[index] = 73;
    }
    let preserved = input.compile().unwrap();
    for index in [116, 128, 137, 146, 155] {
        assert_eq!(preserved.source().words[index], 73);
    }
}
#[test]
fn every_audited_semantic_operand_is_exact_not_just_the_spell_id() {
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).id ^= 1;
        words(&mut input)[0] ^= 1;
        assert!(input.compile().is_err(), "Id");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).school ^= 1;
        words(&mut input)[1] ^= 1;
        assert!(input.compile().is_err(), "School");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).category ^= 1;
        words(&mut input)[2] ^= 1;
        assert!(input.compile().is_err(), "Category");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).dispel ^= 1;
        words(&mut input)[4] ^= 1;
        assert!(input.compile().is_err(), "Dispel");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).mechanic ^= 1;
        words(&mut input)[5] ^= 1;
        assert!(input.compile().is_err(), "Mechanic");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).attributes ^= 1;
        words(&mut input)[6] ^= 1;
        assert!(input.compile().is_err(), "Attributes");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).attributes_ex ^= 1;
        words(&mut input)[7] ^= 1;
        assert!(input.compile().is_err(), "AttributesEx");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).attributes_ex2 ^= 1;
        words(&mut input)[8] ^= 1;
        assert!(input.compile().is_err(), "AttributesEx2");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).attributes_ex3 ^= 1;
        words(&mut input)[9] ^= 1;
        assert!(input.compile().is_err(), "AttributesEx3");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).attributes_ex4 ^= 1;
        words(&mut input)[10] ^= 1;
        assert!(input.compile().is_err(), "AttributesEx4");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).stances ^= 1;
        words(&mut input)[11] ^= 1;
        assert!(input.compile().is_err(), "Stances");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).stances_not ^= 1;
        words(&mut input)[12] ^= 1;
        assert!(input.compile().is_err(), "StancesNot");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).targets ^= 1;
        words(&mut input)[13] ^= 1;
        assert!(input.compile().is_err(), "Targets");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).target_creature_type ^= 1;
        words(&mut input)[14] ^= 1;
        assert!(input.compile().is_err(), "TargetCreatureType");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).requires_spell_focus ^= 1;
        words(&mut input)[15] ^= 1;
        assert!(input.compile().is_err(), "RequiresSpellFocus");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).caster_aura_state ^= 1;
        words(&mut input)[16] ^= 1;
        assert!(input.compile().is_err(), "CasterAuraState");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).target_aura_state ^= 1;
        words(&mut input)[17] ^= 1;
        assert!(input.compile().is_err(), "TargetAuraState");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).casting_time_index ^= 1;
        words(&mut input)[18] ^= 1;
        assert!(input.compile().is_err(), "CastingTimeIndex");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).recovery_time ^= 1;
        words(&mut input)[19] ^= 1;
        assert!(input.compile().is_err(), "RecoveryTime");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).category_recovery_time ^= 1;
        words(&mut input)[20] ^= 1;
        assert!(input.compile().is_err(), "CategoryRecoveryTime");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).interrupt_flags ^= 1;
        words(&mut input)[21] ^= 1;
        assert!(input.compile().is_err(), "InterruptFlags");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).aura_interrupt_flags ^= 1;
        words(&mut input)[22] ^= 1;
        assert!(input.compile().is_err(), "AuraInterruptFlags");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).channel_interrupt_flags ^= 1;
        words(&mut input)[23] ^= 1;
        assert!(input.compile().is_err(), "ChannelInterruptFlags");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).proc_flags ^= 1;
        words(&mut input)[24] ^= 1;
        assert!(input.compile().is_err(), "procFlags");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).proc_chance ^= 1;
        words(&mut input)[25] ^= 1;
        assert!(input.compile().is_err(), "procChance");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).proc_charges ^= 1;
        words(&mut input)[26] ^= 1;
        assert!(input.compile().is_err(), "procCharges");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).max_level ^= 1;
        words(&mut input)[27] ^= 1;
        assert!(input.compile().is_err(), "maxLevel");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).base_level ^= 1;
        words(&mut input)[28] ^= 1;
        assert!(input.compile().is_err(), "baseLevel");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).spell_level ^= 1;
        words(&mut input)[29] ^= 1;
        assert!(input.compile().is_err(), "spellLevel");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).duration_index ^= 1;
        words(&mut input)[30] ^= 1;
        assert!(input.compile().is_err(), "DurationIndex");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).power_type ^= 1;
        words(&mut input)[31] ^= 1;
        assert!(input.compile().is_err(), "powerType");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).mana_cost ^= 1;
        words(&mut input)[32] ^= 1;
        assert!(input.compile().is_err(), "manaCost");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).mana_cost_perlevel ^= 1;
        words(&mut input)[33] ^= 1;
        assert!(input.compile().is_err(), "manaCostPerlevel");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).mana_per_second ^= 1;
        words(&mut input)[34] ^= 1;
        assert!(input.compile().is_err(), "manaPerSecond");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).mana_per_second_per_level ^= 1;
        words(&mut input)[35] ^= 1;
        assert!(input.compile().is_err(), "manaPerSecondPerLevel");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).range_index ^= 1;
        words(&mut input)[36] ^= 1;
        assert!(input.compile().is_err(), "rangeIndex");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).speed ^= 1;
        words(&mut input)[37] ^= 1;
        assert!(input.compile().is_err(), "speed");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).stack_amount ^= 1;
        words(&mut input)[39] ^= 1;
        assert!(input.compile().is_err(), "StackAmount");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).equipped_item_class ^= 1;
        words(&mut input)[58] ^= 1;
        assert!(input.compile().is_err(), "EquippedItemClass");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).equipped_item_sub_class_mask ^= 1;
        words(&mut input)[59] ^= 1;
        assert!(input.compile().is_err(), "EquippedItemSubClassMask");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).equipped_item_inventory_type_mask ^= 1;
        words(&mut input)[60] ^= 1;
        assert!(input.compile().is_err(), "EquippedItemInventoryTypeMask");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).mana_cost_percentage ^= 1;
        words(&mut input)[156] ^= 1;
        assert!(input.compile().is_err(), "ManaCostPercentage");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).start_recovery_category ^= 1;
        words(&mut input)[157] ^= 1;
        assert!(input.compile().is_err(), "StartRecoveryCategory");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).start_recovery_time ^= 1;
        words(&mut input)[158] ^= 1;
        assert!(input.compile().is_err(), "StartRecoveryTime");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).spell_family_name ^= 1;
        words(&mut input)[160] ^= 1;
        assert!(input.compile().is_err(), "SpellFamilyName");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).dmg_class ^= 1;
        words(&mut input)[164] ^= 1;
        assert!(input.compile().is_err(), "DmgClass");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).prevention_type ^= 1;
        words(&mut input)[165] ^= 1;
        assert!(input.compile().is_err(), "PreventionType");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[0].effect ^= 1;
        words(&mut input)[61] ^= 1;
        assert!(input.compile().is_err(), "Effect1");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[1].effect ^= 1;
        words(&mut input)[62] ^= 1;
        assert!(input.compile().is_err(), "Effect2");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[2].effect ^= 1;
        words(&mut input)[63] ^= 1;
        assert!(input.compile().is_err(), "Effect3");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[0].effect_die_sides ^= 1;
        words(&mut input)[64] ^= 1;
        assert!(input.compile().is_err(), "EffectDieSides1");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[1].effect_die_sides ^= 1;
        words(&mut input)[65] ^= 1;
        assert!(input.compile().is_err(), "EffectDieSides2");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[2].effect_die_sides ^= 1;
        words(&mut input)[66] ^= 1;
        assert!(input.compile().is_err(), "EffectDieSides3");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[0].effect_base_dice ^= 1;
        words(&mut input)[67] ^= 1;
        assert!(input.compile().is_err(), "EffectBaseDice1");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[1].effect_base_dice ^= 1;
        words(&mut input)[68] ^= 1;
        assert!(input.compile().is_err(), "EffectBaseDice2");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[2].effect_base_dice ^= 1;
        words(&mut input)[69] ^= 1;
        assert!(input.compile().is_err(), "EffectBaseDice3");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[0].effect_dice_per_level ^= 1;
        words(&mut input)[70] ^= 1;
        assert!(input.compile().is_err(), "EffectDicePerLevel1");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[1].effect_dice_per_level ^= 1;
        words(&mut input)[71] ^= 1;
        assert!(input.compile().is_err(), "EffectDicePerLevel2");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[2].effect_dice_per_level ^= 1;
        words(&mut input)[72] ^= 1;
        assert!(input.compile().is_err(), "EffectDicePerLevel3");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[0].effect_real_points_per_level ^= 1;
        words(&mut input)[73] ^= 1;
        assert!(input.compile().is_err(), "EffectRealPointsPerLevel1");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[1].effect_real_points_per_level ^= 1;
        words(&mut input)[74] ^= 1;
        assert!(input.compile().is_err(), "EffectRealPointsPerLevel2");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[2].effect_real_points_per_level ^= 1;
        words(&mut input)[75] ^= 1;
        assert!(input.compile().is_err(), "EffectRealPointsPerLevel3");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[0].effect_base_points ^= 1;
        words(&mut input)[76] ^= 1;
        assert!(input.compile().is_err(), "EffectBasePoints1");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[1].effect_base_points ^= 1;
        words(&mut input)[77] ^= 1;
        assert!(input.compile().is_err(), "EffectBasePoints2");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[2].effect_base_points ^= 1;
        words(&mut input)[78] ^= 1;
        assert!(input.compile().is_err(), "EffectBasePoints3");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[0].effect_mechanic ^= 1;
        words(&mut input)[79] ^= 1;
        assert!(input.compile().is_err(), "EffectMechanic1");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[1].effect_mechanic ^= 1;
        words(&mut input)[80] ^= 1;
        assert!(input.compile().is_err(), "EffectMechanic2");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[2].effect_mechanic ^= 1;
        words(&mut input)[81] ^= 1;
        assert!(input.compile().is_err(), "EffectMechanic3");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[0].effect_implicit_target_a ^= 1;
        words(&mut input)[82] ^= 1;
        assert!(input.compile().is_err(), "EffectImplicitTargetA1");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[1].effect_implicit_target_a ^= 1;
        words(&mut input)[83] ^= 1;
        assert!(input.compile().is_err(), "EffectImplicitTargetA2");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[2].effect_implicit_target_a ^= 1;
        words(&mut input)[84] ^= 1;
        assert!(input.compile().is_err(), "EffectImplicitTargetA3");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[0].effect_implicit_target_b ^= 1;
        words(&mut input)[85] ^= 1;
        assert!(input.compile().is_err(), "EffectImplicitTargetB1");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[1].effect_implicit_target_b ^= 1;
        words(&mut input)[86] ^= 1;
        assert!(input.compile().is_err(), "EffectImplicitTargetB2");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[2].effect_implicit_target_b ^= 1;
        words(&mut input)[87] ^= 1;
        assert!(input.compile().is_err(), "EffectImplicitTargetB3");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[0].effect_radius_index ^= 1;
        words(&mut input)[88] ^= 1;
        assert!(input.compile().is_err(), "EffectRadiusIndex1");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[1].effect_radius_index ^= 1;
        words(&mut input)[89] ^= 1;
        assert!(input.compile().is_err(), "EffectRadiusIndex2");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[2].effect_radius_index ^= 1;
        words(&mut input)[90] ^= 1;
        assert!(input.compile().is_err(), "EffectRadiusIndex3");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[0].effect_apply_aura_name ^= 1;
        words(&mut input)[91] ^= 1;
        assert!(input.compile().is_err(), "EffectApplyAuraName1");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[1].effect_apply_aura_name ^= 1;
        words(&mut input)[92] ^= 1;
        assert!(input.compile().is_err(), "EffectApplyAuraName2");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[2].effect_apply_aura_name ^= 1;
        words(&mut input)[93] ^= 1;
        assert!(input.compile().is_err(), "EffectApplyAuraName3");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[0].effect_amplitude ^= 1;
        words(&mut input)[94] ^= 1;
        assert!(input.compile().is_err(), "EffectAmplitude1");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[1].effect_amplitude ^= 1;
        words(&mut input)[95] ^= 1;
        assert!(input.compile().is_err(), "EffectAmplitude2");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[2].effect_amplitude ^= 1;
        words(&mut input)[96] ^= 1;
        assert!(input.compile().is_err(), "EffectAmplitude3");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[0].effect_multiple_value ^= 1;
        words(&mut input)[97] ^= 1;
        assert!(input.compile().is_err(), "EffectMultipleValue1");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[1].effect_multiple_value ^= 1;
        words(&mut input)[98] ^= 1;
        assert!(input.compile().is_err(), "EffectMultipleValue2");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[2].effect_multiple_value ^= 1;
        words(&mut input)[99] ^= 1;
        assert!(input.compile().is_err(), "EffectMultipleValue3");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[0].effect_chain_target ^= 1;
        words(&mut input)[100] ^= 1;
        assert!(input.compile().is_err(), "EffectChainTarget1");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[1].effect_chain_target ^= 1;
        words(&mut input)[101] ^= 1;
        assert!(input.compile().is_err(), "EffectChainTarget2");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[2].effect_chain_target ^= 1;
        words(&mut input)[102] ^= 1;
        assert!(input.compile().is_err(), "EffectChainTarget3");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[0].effect_item_type ^= 1;
        words(&mut input)[103] ^= 1;
        assert!(input.compile().is_err(), "EffectItemType1");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[1].effect_item_type ^= 1;
        words(&mut input)[104] ^= 1;
        assert!(input.compile().is_err(), "EffectItemType2");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[2].effect_item_type ^= 1;
        words(&mut input)[105] ^= 1;
        assert!(input.compile().is_err(), "EffectItemType3");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[0].effect_misc_value ^= 1;
        words(&mut input)[106] ^= 1;
        assert!(input.compile().is_err(), "EffectMiscValue1");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[1].effect_misc_value ^= 1;
        words(&mut input)[107] ^= 1;
        assert!(input.compile().is_err(), "EffectMiscValue2");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[2].effect_misc_value ^= 1;
        words(&mut input)[108] ^= 1;
        assert!(input.compile().is_err(), "EffectMiscValue3");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[0].effect_trigger_spell ^= 1;
        words(&mut input)[109] ^= 1;
        assert!(input.compile().is_err(), "EffectTriggerSpell1");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[1].effect_trigger_spell ^= 1;
        words(&mut input)[110] ^= 1;
        assert!(input.compile().is_err(), "EffectTriggerSpell2");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[2].effect_trigger_spell ^= 1;
        words(&mut input)[111] ^= 1;
        assert!(input.compile().is_err(), "EffectTriggerSpell3");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[0].effect_points_per_combo_point ^= 1;
        words(&mut input)[112] ^= 1;
        assert!(input.compile().is_err(), "EffectPointsPerComboPoint1");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[1].effect_points_per_combo_point ^= 1;
        words(&mut input)[113] ^= 1;
        assert!(input.compile().is_err(), "EffectPointsPerComboPoint2");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[2].effect_points_per_combo_point ^= 1;
        words(&mut input)[114] ^= 1;
        assert!(input.compile().is_err(), "EffectPointsPerComboPoint3");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).totem1 ^= 1;
        words(&mut input)[40] ^= 1;
        assert!(input.compile().is_err(), "Totem1");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).totem2 ^= 1;
        words(&mut input)[41] ^= 1;
        assert!(input.compile().is_err(), "Totem2");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).reagent1 ^= 1;
        words(&mut input)[42] ^= 1;
        assert!(input.compile().is_err(), "Reagent1");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).reagent2 ^= 1;
        words(&mut input)[43] ^= 1;
        assert!(input.compile().is_err(), "Reagent2");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).reagent3 ^= 1;
        words(&mut input)[44] ^= 1;
        assert!(input.compile().is_err(), "Reagent3");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).reagent4 ^= 1;
        words(&mut input)[45] ^= 1;
        assert!(input.compile().is_err(), "Reagent4");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).reagent5 ^= 1;
        words(&mut input)[46] ^= 1;
        assert!(input.compile().is_err(), "Reagent5");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).reagent6 ^= 1;
        words(&mut input)[47] ^= 1;
        assert!(input.compile().is_err(), "Reagent6");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).reagent7 ^= 1;
        words(&mut input)[48] ^= 1;
        assert!(input.compile().is_err(), "Reagent7");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).reagent8 ^= 1;
        words(&mut input)[49] ^= 1;
        assert!(input.compile().is_err(), "Reagent8");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).reagent_count1 ^= 1;
        words(&mut input)[50] ^= 1;
        assert!(input.compile().is_err(), "ReagentCount1");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).reagent_count2 ^= 1;
        words(&mut input)[51] ^= 1;
        assert!(input.compile().is_err(), "ReagentCount2");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).reagent_count3 ^= 1;
        words(&mut input)[52] ^= 1;
        assert!(input.compile().is_err(), "ReagentCount3");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).reagent_count4 ^= 1;
        words(&mut input)[53] ^= 1;
        assert!(input.compile().is_err(), "ReagentCount4");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).reagent_count5 ^= 1;
        words(&mut input)[54] ^= 1;
        assert!(input.compile().is_err(), "ReagentCount5");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).reagent_count6 ^= 1;
        words(&mut input)[55] ^= 1;
        assert!(input.compile().is_err(), "ReagentCount6");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).reagent_count7 ^= 1;
        words(&mut input)[56] ^= 1;
        assert!(input.compile().is_err(), "ReagentCount7");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).reagent_count8 ^= 1;
        words(&mut input)[57] ^= 1;
        assert!(input.compile().is_err(), "ReagentCount8");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).max_target_level ^= 1;
        words(&mut input)[159] ^= 1;
        assert!(input.compile().is_err(), "MaxTargetLevel");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).max_affected_targets ^= 1;
        words(&mut input)[163] ^= 1;
        assert!(input.compile().is_err(), "MaxAffectedTargets");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[0].dmg_multiplier ^= 1;
        words(&mut input)[167] ^= 1;
        assert!(input.compile().is_err(), "DmgMultiplier1");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[1].dmg_multiplier ^= 1;
        words(&mut input)[168] ^= 1;
        assert!(input.compile().is_err(), "DmgMultiplier2");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).effects[2].dmg_multiplier ^= 1;
        words(&mut input)[169] ^= 1;
        assert!(input.compile().is_err(), "DmgMultiplier3");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).min_faction_id ^= 1;
        words(&mut input)[170] ^= 1;
        assert!(input.compile().is_err(), "MinFactionId");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).min_reputation ^= 1;
        words(&mut input)[171] ^= 1;
        assert!(input.compile().is_err(), "MinReputation");
    }
    {
        let mut input = captured::captured_fireball_input();
        template(&mut input).required_aura_vision ^= 1;
        words(&mut input)[172] ^= 1;
        assert!(input.compile().is_err(), "RequiredAuraVision");
    }
    for field in 0..4 {
        let mut input = captured::captured_fireball_input();
        match field {
            0 => template(&mut input).is_server_side = 1,
            1 => template(&mut input).attributes_serverside = 1,
            2 => template(&mut input).effects[0].effect_bonus_coefficient = 0,
            _ => template(&mut input).effects[2].effect_bonus_coefficient_from_ap = 1,
        }
        assert!(input.compile().is_err());
    }
}
#[test]
fn unknown_empty_duplicate_wrong_and_excessive_selected_collections_refuse() {
    for collection in 0..6 {
        for fault in 0..4 {
            let mut input = captured::captured_fireball_input();
            macro_rules! damage {
                ($rows:expr) => {{
                    let rows = &mut $rows;
                    match fault {
                        0 => *rows = SpellSourceRows::Unknown,
                        1 => *rows = SpellSourceRows::Complete(vec![]),
                        2 | 3 => {
                            if let SpellSourceRows::Complete(v) = rows {
                                let row = v[0].clone();
                                v.resize(if fault == 2 { 2 } else { 4097 }, row);
                            }
                        }
                        _ => unreachable!(),
                    }
                }};
            }
            match collection {
                0 => damage!(input.templates),
                1 => damage!(input.spells),
                2 => damage!(input.cast_times),
                3 => damage!(input.ranges),
                4 => damage!(input.durations),
                _ => {
                    input.facing = SpellSourceRows::Complete(vec![CapturedSpellFacing {
                        entry: 133,
                        facingcasterflag: 0,
                    }]);
                    damage!(input.facing);
                    if fault == 1 {
                        assert!(input.compile().is_ok());
                        continue;
                    }
                }
            }
            let result = input.compile();
            assert!(result.is_err(), "collection {collection}, fault {fault}");
            if fault == 3 {
                assert!(matches!(result, Err(NativeError::TooLarge)));
            }
        }
    }
    for fault in 0..9 {
        let mut input = captured::captured_fireball_input();
        match fault {
            0 => {
                if let SpellSourceRows::Complete(v) = &mut input.cast_times {
                    v[0].id = 17;
                }
            }
            1 => {
                if let SpellSourceRows::Complete(v) = &mut input.ranges {
                    v[0].id = 36;
                }
            }
            2 => {
                if let SpellSourceRows::Complete(v) = &mut input.durations {
                    v[0].id = 36;
                }
            }
            3 => {
                if let SpellSourceRows::Complete(v) = &mut input.cast_times {
                    v[0].base_ms = -1;
                }
            }
            4 => {
                if let SpellSourceRows::Complete(v) = &mut input.durations {
                    v[0].per_level_ms = 1;
                }
            }
            5 => {
                input.facing = SpellSourceRows::Complete(vec![CapturedSpellFacing {
                    entry: 134,
                    facingcasterflag: 0,
                }])
            }
            6 => {
                input.facing = SpellSourceRows::Complete(vec![CapturedSpellFacing {
                    entry: 133,
                    facingcasterflag: 2,
                }])
            }
            7 => template(&mut input).speed = 0x7fc00000,
            _ => template(&mut input).spell_family_flags |= 1 << 32,
        }
        assert!(input.compile().is_err());
    }
}
#[test]
fn independent_hooks_and_final_profile_binding_remain_closed_and_atomic() {
    for dependency in 0..11 {
        let mut input = captured::captured_fireball_input();
        match dependency {
            0 => input.hooks.spell_area = SpellAreaSourceRows::Unknown,
            1 => input.hooks.spell_script_names = SpellSourceRows::Unknown,
            2 => input.hooks.conditions = SpellSourceRows::Unknown,
            3 => input.hooks.spell_proc_event = SpellSourceRows::Unknown,
            4 => input.hooks.spell_affect = SpellSourceRows::Unknown,
            5 => input.hooks.spell_bonus_data = SpellSourceRows::Unknown,
            6 => input.hooks.spell_threat = SpellSourceRows::Unknown,
            7 => input.hooks.spell_elixir = SpellSourceRows::Unknown,
            8 => input.hooks.spell_group_membership = SpellSourceRows::Unknown,
            9 => input.hooks.spell_rank_chain = SpellSourceRows::Unknown,
            _ => input.hooks.spell_script_names = SpellSourceRows::Complete(vec![String::new()]),
        }
        assert!(input.compile().is_err());
    }
    for dependency in 0..7 {
        let mut input = captured::captured_fireball_input();
        let rows = SpellSourceRows::Complete(vec![133]);
        match dependency {
            0 => input.hooks.conditions = rows,
            1 => input.hooks.spell_proc_event = rows,
            2 => input.hooks.spell_affect = rows,
            3 => input.hooks.spell_bonus_data = rows,
            4 => input.hooks.spell_threat = rows,
            5 => input.hooks.spell_elixir = rows,
            _ => {
                input.hooks.spell_script_names = SpellSourceRows::Complete(vec!["authored".into()])
            }
        }
        assert!(input.compile().is_err());
    }
    for budget in 0..3 {
        let mut input = captured::captured_fireball_input();
        match budget {
            0 => input.hooks.conditions = SpellSourceRows::Complete(vec![133; 4097]),
            1 => input.hooks.spell_script_names = SpellSourceRows::Complete(vec!["x".repeat(257)]),
            _ => {
                input.hooks.spell_group_membership =
                    SpellSourceRows::Complete(vec![SpellGroupSource {
                        id: 1,
                        rule: 1,
                        name: "x".repeat(257),
                        spell_ids: vec![133],
                    }])
            }
        }
        assert!(matches!(input.compile(), Err(NativeError::TooLarge)));
    }
    let mut grouped = captured::captured_fireball_input();
    grouped.hooks.spell_group_membership = SpellSourceRows::Complete(vec![SpellGroupSource {
        id: 1,
        rule: 1,
        name: "authored singleton group".into(),
        spell_ids: vec![133],
    }]);
    grouped.hooks.spell_rank_chain = SpellSourceRows::Complete(vec![SpellRankSource {
        spell_id: 133,
        previous_spell: 0,
        first_spell: 133,
        rank: 1,
        required_spell: 0,
    }]);
    assert!(grouped.compile().is_ok());
    for fault in 0..8 {
        let (mut profile, mut initial, mut source) = captured::captured_fireball_inputs();
        match fault {
            0 => initial.spells[0].words[115] += 1,
            1 => {
                profile
                    .source_motion
                    .as_mut()
                    .unwrap()
                    .spell_hits
                    .get_mut(&133)
                    .unwrap()
                    .attributes[0] = 0
            }
            2 => source.modifier_spell_ids = SpellSourceRows::Unknown,
            3 => source.initial_cast_dependencies = SpellSourceRows::Unknown,
            4 => source.session.latency_ms = 1,
            5 => source.npcs[0].resistances[1] = 1,
            6 => initial.items[0].scripted = true,
            _ => initial.spells[1].external_initial_hooks = true,
        }
        let before = profile.clone();
        assert!(profile
            .initialize_spell_authority(&initial, source)
            .is_err());
        assert_eq!(profile, before);
    }
    captured::captured_fireball_profile().validate().unwrap();
}
#[test]
fn generated_spell_replays_and_restores_through_cast_travel_aura_and_expiry() {
    let profile = captured::captured_fireball_profile();
    let mut world =
        NativeWorld::synthetic_component_test(profile.clone(), [8; 16], f::player(), 7).unwrap();
    let mut authority = spatial::spatial_only();
    let cast = f::cast(&world, 133);
    world.step(1, Some(cast), &mut authority).unwrap();
    for _ in 0..80 {
        let checkpoint = world.checkpoint().unwrap();
        let mut reopened =
            NativeWorld::restore_synthetic_component_test(profile.clone(), checkpoint).unwrap();
        let tick = world.state.tick + 1;
        world.step(tick, None, &mut authority).unwrap();
        reopened
            .step(tick, None, &mut spatial::spatial_only())
            .unwrap();
        assert_eq!(world.checkpoint().unwrap(), reopened.checkpoint().unwrap());
    }
    assert!(world.state.casts.is_empty());
    assert!(authority
        .queries
        .iter()
        .all(|r| !matches!(r.query, Query::Spell { .. })));
}
