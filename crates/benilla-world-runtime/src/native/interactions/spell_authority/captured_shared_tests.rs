// SPDX-License-Identifier: GPL-3.0-or-later
use super::tests::{template, words};
use crate::native::synthetic_captured_spell_fixture as captured;

#[test]
fn shared_authored_refusal_vectors_agree_with_python_planning() {
    let cases: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../scripts/native_starter/fireball-authored.json"
    )))
    .unwrap();
    for refusal in cases["refusals"].as_array().unwrap() {
        let field = refusal["field"].as_str().unwrap();
        let value = if refusal["value"].is_object() {
            assert_eq!(refusal["value"]["decimal"], "0");
            0
        } else {
            refusal["value"].as_i64().unwrap() as u32
        };
        let mut input = captured::captured_fireball_input();
        let column = match field {
            "Id" => {
                template(&mut input).id = value;
                Some(0)
            }
            "School" => {
                template(&mut input).school = value;
                Some(1)
            }
            "AttributesEx2" => {
                template(&mut input).attributes_ex2 = value;
                Some(8)
            }
            "IsServerSide" => {
                template(&mut input).is_server_side = value;
                None
            }
            "Effect2" => {
                template(&mut input).effects[1].effect = value;
                Some(62)
            }
            "EffectApplyAuraName2" => {
                template(&mut input).effects[1].effect_apply_aura_name = value;
                Some(92)
            }
            "EffectTriggerSpell2" => {
                template(&mut input).effects[1].effect_trigger_spell = value;
                Some(110)
            }
            "EffectImplicitTargetA1" => {
                template(&mut input).effects[0].effect_implicit_target_a = value;
                Some(82)
            }
            "EffectChainTarget1" => {
                template(&mut input).effects[0].effect_chain_target = value;
                Some(100)
            }
            "EffectMiscValue3" => {
                template(&mut input).effects[2].effect_misc_value = value as i32;
                Some(108)
            }
            "Totem1" => {
                template(&mut input).totem1 = value;
                Some(40)
            }
            "Reagent8" => {
                template(&mut input).reagent8 = value as i32;
                Some(49)
            }
            "MaxTargetLevel" => {
                template(&mut input).max_target_level = value;
                Some(159)
            }
            "MaxAffectedTargets" => {
                template(&mut input).max_affected_targets = value;
                Some(163)
            }
            "SpellFamilyFlags" => {
                template(&mut input).spell_family_flags = value.into();
                words(&mut input)[162] = 0;
                Some(161)
            }
            "DurationIndex" => {
                template(&mut input).duration_index = value;
                Some(30)
            }
            "CastingTimeIndex" => {
                template(&mut input).casting_time_index = value;
                Some(18)
            }
            "RequiresSpellFocus" => {
                template(&mut input).requires_spell_focus = value;
                Some(15)
            }
            "InterruptFlags" => {
                template(&mut input).interrupt_flags = value;
                Some(21)
            }
            "manaCost" => {
                template(&mut input).mana_cost = value;
                Some(32)
            }
            "EffectBonusCoefficient1" => {
                template(&mut input).effects[0].effect_bonus_coefficient = value;
                None
            }
            "DmgMultiplier3" => {
                template(&mut input).effects[2].dmg_multiplier = value;
                Some(169)
            }
            _ => panic!("unmapped shared refusal {field}"),
        };
        if let Some(column) = column {
            words(&mut input)[column] = value;
        }
        assert!(input.compile().is_err(), "shared refusal {field}");
    }
}
