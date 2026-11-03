// SPDX-License-Identifier: GPL-3.0-or-later
// Mirrors starter_init::initial_cast_phases::classify, not its source-plan constructor.
use super::*;
#[path = "consumable_execution.rs"]
mod consumable_execution;
#[path = "frost_execution.rs"]
mod frost_execution;
pub(in super::super) const ADAPTER: &str = "captured_initial_spell_existing_phase_handlers_v1";
const ONE: u32 = 0x3f80_0000;
type InitialShape = (&'static [(u32, u32, i32)], i32, u32);
fn shape(id: i128) -> Option<InitialShape> {
    Some(match id {
        81 => (&[(20, 0, 90)], -1, u32::MAX),
        203 => (&[(25, 0, 0)], 2, 8192),
        204 => (&[(26, 0, 0)], -1, 0),
        227 => (&[(25, 0, 0), (60, 0, 0)], 2, 1024),
        522 => (&[(37, 0, 0)], -1, u32::MAX),
        668 => (&[(39, 0, 7)], -1, u32::MAX),
        2382 => (&[(25, 0, 0), (60, 0, 0)], 2, 16384),
        3050 => (&[(49, 0, 0)], -1, u32::MAX),
        5009 => (&[(25, 0, 0), (60, 0, 0)], 2, 524288),
        9078 => (&[(60, 0, 0)], 4, 2),
        9125 => (&[(60, 0, 0)], 4, 1),
        20597 => (&[(6, 98, 43), (6, 98, 55)], -1, 0),
        20598 => (&[(6, 137, 4)], -1, 0),
        20599 => (&[(6, 156, 0)], -1, 0),
        20864 => (&[(6, 98, 54), (6, 98, 160)], -1, 0),
        _ => return None,
    })
}
pub(in super::super) fn supported_plan(
    bundle: &Value,
    spell: &Value,
    spells: &[Value],
) -> Result<Value> {
    let id = int(&spell["id"]);
    if matches!(id, 430 | 433) {
        return consumable_execution::supported(bundle, spell);
    }
    if matches!(id, 168 | 6136) {
        return frost_execution::frost_plan(spell, spells);
    }
    let mut plan = json!({"adapter":ADAPTER,"status":"blocked","classification":"out_of_scope","effective_learning_proven":false,"runtime_export":null,"refusals":[]});
    if id == 133 {
        plan["classification"] = json!("existing_spell_authority");
        plan["refusals"] = json!(["spell_execution.use_captured_fireball_plan"]);
        return Ok(plan);
    }
    let Some((effects, item_class, item_mask)) = shape(id) else {
        let extension = match id {
            8690 => Some("item_use_teleport_and_home_bind_lifecycle"),
            _ => None,
        };
        plan["classification"] = json!(if extension.is_some() {
            "bounded_extension"
        } else {
            "out_of_scope"
        });
        plan["refusals"] = json!([format!(
            "spell_execution.{}",
            extension.unwrap_or("handler_not_implemented")
        )]);
        return Ok(plan);
    };
    plan["classification"] = json!("existing_initial_phase_handlers");
    let scope = &spell["numeric_capture"];
    let Some(words) = spell["server_numeric_words"].as_array() else {
        plan["refusals"] = json!(["spell_execution.complete_effective_numeric_inputs_required"]);
        return Ok(plan);
    };
    if scope.is_null() {
        plan["refusals"] = json!(["spell_execution.complete_effective_numeric_inputs_required"]);
        return Ok(plan);
    }
    let w: [u32; 173] = std::array::from_fn(|i| int(&words[i]) as u32);
    let r = &scope["row"];
    let mut refusals = Vec::new();
    let mut refuse = |reason: &str| refusals.push(json!(format!("spell_execution.{reason}")));
    if !spell["server_client_differences"]
        .as_array()
        .unwrap()
        .is_empty()
    {
        refuse("server_client_mismatch");
    }
    let registry = &spell["script_registry"];
    let hooks = &spell["initial_hooks"];
    if registry.is_null() || !registry["rows"].as_array().unwrap().is_empty() {
        refuse("selected_registry_absence_required");
    }
    if hooks.is_null()
        || ["spell_area", "spell_proc_event"]
            .iter()
            .any(|t| !hooks[*t].as_array().unwrap().is_empty())
    {
        refuse("selected_hook_absence_required");
    }
    if bundle.get("learning_capture").is_none()
        || rows(bundle, "spell_chain").iter().any(|row| {
            ["spell_id", "prev_spell", "first_spell", "req_spell"]
                .iter()
                .any(|f| int(&row[*f]) == id)
        })
        || rows(bundle, "spell_learn_spell")
            .iter()
            .any(|row| ["entry", "SpellID"].iter().any(|f| int(&row[*f]) == id))
    {
        refuse("neutral_learning_rank_scope_required");
    }
    if scope["facing"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| nonzero(&row["facingcasterflag"]))
    {
        refuse("facing_predicate_not_initial");
    }
    let holder = effects[0].0 == 6;
    let weapon = matches!(id, 81 | 203 | 204 | 227 | 522 | 668 | 2382 | 5009);
    let expected = [
        (0, id as u32),
        (1, 0),
        (6, if holder || id == 81 { 0x50 } else { 0xc0 }),
        (7, if id == 3050 { 0x0200_0000 } else { 0 }),
        (25, 101),
        (36, 1),
        (58, item_class as u32),
        (59, item_mask),
        (164, u32::from(weapon)),
        (165, u32::from(weapon)),
        (166, if holder { 0 } else { u32::MAX }),
    ];
    if expected.iter().any(|&(i, v)| w[i] != v) {
        refuse("initial_morphology");
    }
    if w.iter().enumerate().any(|(i,&v)| v != 0 && !matches!(i,0 | 3 | 6 | 7 | 18 | 25 | 27..=30 | 36 | 38 | 58 | 59 | 61..=69 | 73..=78 | 82..=84 | 91..=93 | 97..=99 | 106..=108 | 115..=155 | 164..=169)) {
        refuse("unexecuted_numeric_field");
    }
    let cast = &spell["cast_time"];
    // SpellMgr.cpp:56-73 resolves a missing indexed duration to zero.
    let zero_duration = json!({"id":0,"base_ms":0,"per_level_ms":0,"maximum_ms":0});
    let duration = if spell["duration"].is_null() && w[30] == 0 {
        &zero_duration
    } else {
        &spell["duration"]
    };
    if cast.is_null()
        || int(&cast["id"]) != i128::from(w[18])
        || ["base_ms", "per_level_ms", "minimum_ms"]
            .iter()
            .any(|f| nonzero(&cast[*f]))
        || duration.is_null()
        || int(&duration["id"]) != i128::from(w[30])
        || !matches!(int(&duration["base_ms"]), -1 | 0)
        || nonzero(&duration["per_level_ms"])
        || duration["maximum_ms"] != duration["base_ms"]
        || holder && !(duration["base_ms"] == -1 || w[30] == 0)
    {
        refuse("initial_auxiliary_resolution");
    }
    if spell["range"] != json!({"id":1,"minimum_binary32":0,"maximum_binary32":0,"flags":0}) {
        refuse("initial_self_range");
    }
    let mut ap = false;
    for slot in 1..=3 {
        ap |= dbc::binary32(
            r[format!("EffectBonusCoefficientFromAP{slot}")]["decimal"]
                .as_str()
                .unwrap(),
        )? != 0;
    }
    if nonzero(&r["IsServerSide"]) || nonzero(&r["AttributesServerside"]) || ap {
        refuse("server_fields");
    }
    for slot in 0..3 {
        let (effect, aura, misc) = effects.get(slot).copied().unwrap_or((0, 0, 0));
        let coefficient = if (holder || id == 81) && effect != 0 {
            ONE
        } else {
            0
        };
        if (w[61 + slot], w[91 + slot], w[106 + slot] as i32) != (effect, aura, misc)
            || w[82 + slot] != u32::from(holder && effect != 0)
            || w[97 + slot] != if id == 81 && slot == 0 { ONE } else { 0 }
            || w[167 + slot] != if !holder || effect != 0 { ONE } else { 0 }
            || dbc::binary32(
                r[format!("EffectBonusCoefficient{}", slot + 1)]["decimal"]
                    .as_str()
                    .unwrap(),
            )? != coefficient
        {
            refuse(&format!("effect_{}_morphology", slot + 1));
        }
        let dice = w[67 + slot];
        let amount = i64::from(w[76 + slot] as i32) + i64::from(dice);
        if dice > i32::MAX as u32 || i32::try_from(amount).is_err() {
            refuse(&format!("effect_{}_cached_amount", slot + 1));
        }
        let values = (w[64 + slot], w[67 + slot], w[73 + slot], w[76 + slot]);
        let valid = if effect == 0 {
            values == (0, 0, 0, 0)
        } else if id == 3050 {
            values == (1, 1, 0x40a0_0000, u32::MAX)
        } else if id == 81 {
            values == (6, 0, 0, 0)
        } else {
            matches!(values.0, 0 | 1)
                && values.2 == 0
                && (aura != 98 || i16::try_from(amount).is_ok())
                && (!holder || amount.unsigned_abs() <= 1_000_000)
        };
        if !valid {
            refuse(&format!("effect_{}_amount", slot + 1));
        }
    }
    if refusals.is_empty() {
        plan["status"] = json!("supported_shape");
        plan["input_bindings"] = json!({"numeric_capture_sha256":scope["coverage_sha256"],"effective_words_sha256":digest(&spell["server_numeric_words"]),"registry_sha256":registry["coverage_sha256"],"initial_hooks_sha256":hooks["coverage_sha256"]});
    }
    plan["refusals"] = json!(refusals);
    Ok(plan)
}
pub(in super::super) fn discharges(issue: &Value, spells: &Value) -> bool {
    if issue["table"] == "spell_scripts" {
        return matches!(
            issue["reason"].as_str(),
            Some("spell.registry_row_requires_named_handler_semantics" | "runtime_script_required")
        ) && (issue["key"] == json!([433]) || issue["key"] == "433")
            && spells.as_array().unwrap().iter().any(|s| {
                s["id"] == 433
                    && s["execution_plan"]["status"] == "supported_shape"
                    && s["execution_plan"]["adapter"]
                        == "captured_food_drink_existing_item_spell_authority_v1"
            });
    }
    if issue["table"] != "spell_template"
        || !matches!(
            issue["reason"].as_str().unwrap(),
            "spell.native_effect_target_scaling_rules_required"
                | "spell.multiple_or_missing_effects"
                | "spell.aura_or_trigger_dependency"
                | "spell.effect_execution_required"
                | "spell.target_selector_execution_required"
                | "spell.attribute_bits_execution_required"
        )
    {
        return false;
    }
    let id = match &issue["key"] {
        Value::Array(ids) if ids.len() == 1 => ids[0].as_u64(),
        Value::String(id)
            if !id.starts_with('0') && id.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            id.parse::<u64>().ok()
        }
        _ => None,
    };
    id.is_some()
        && spells
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"].as_u64() == id && s["execution_plan"]["status"] == "supported_shape")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_spell_keys_require_exact_positive_decimal_or_single_id() {
        let spells = json!([{"id":81,"execution_plan":{"status":"supported_shape"}}]);
        for (key, expected) in [
            (json!([81]), true),
            (json!("81"), true),
            (json!("081"), false),
            (json!("+81"), false),
            (json!(" 81"), false),
            (json!([81, 203]), false),
            (json!(81), false),
        ] {
            let issue = json!({"table":"spell_template","key":key,
                "reason":"spell.effect_execution_required"});
            assert_eq!(discharges(&issue, &spells), expected);
        }
    }
}
