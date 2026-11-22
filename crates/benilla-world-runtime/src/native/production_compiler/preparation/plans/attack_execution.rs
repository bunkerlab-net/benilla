// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS 8ec338a1 CombatHandler.cpp:26-61; SpellEffects.cpp:136.
//! An exact Attack marker binds the mainhand host, not EffectUnused execution.
use super::*;

pub(super) fn supported(bundle: &Value, spell: &Value) -> Result<Value> {
    let mut plan = json!({"adapter":"captured_attack_6603_existing_mainhand_operation_v1",
        "status":"blocked","classification":"existing_mainhand_operation_marker",
        "effective_learning_proven":false,"runtime_export":null,"refusals":[]});
    let scope = &spell["numeric_capture"];
    let Some(words) = spell["server_numeric_words"]
        .as_array()
        .filter(|_| !scope.is_null())
    else {
        plan["refusals"] =
            json!(["spell_execution.attack_complete_effective_numeric_inputs_required"]);
        return Ok(plan);
    };
    let mut refusals = Vec::new();
    let mut refuse =
        |reason: &str| refusals.push(json!(format!("spell_execution.attack_{reason}")));
    let expected = |i| match i {
        0 => 6603,
        6 => 0x10,
        18 | 36 | 117 => 1,
        25 => 101,
        58 | 166 => u32::MAX,
        61 => 78,
        167..=169 => 0x3f80_0000,
        _ => 0,
    };
    if int(&spell["id"]) != 6603
        || words.len() != 173
        || words
            .iter()
            .enumerate()
            .any(|(i, v)| !(120..=155).contains(&i) && int(v) != i128::from(expected(i)))
    {
        refuse("numeric_marker_shape");
    }
    if !spell["server_client_differences"]
        .as_array()
        .unwrap()
        .is_empty()
    {
        refuse("server_client_mismatch");
    }
    let row = &scope["row"];
    let mut coefficients = false;
    for field in ["EffectBonusCoefficient", "EffectBonusCoefficientFromAP"] {
        for slot in 1..=3 {
            coefficients |=
                dbc::binary32(row[format!("{field}{slot}")]["decimal"].as_str().unwrap())? != 0;
        }
    }
    if nonzero(&row["IsServerSide"]) || nonzero(&row["AttributesServerside"]) || coefficients {
        refuse("server_fields");
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
    if !scope["facing"].as_array().unwrap().is_empty() {
        refuse("selected_facing_absence_required");
    }
    if bundle.get("learning_capture").is_none()
        || rows(bundle, "spell_chain").iter().any(|r| {
            ["spell_id", "prev_spell", "first_spell", "req_spell"]
                .iter()
                .any(|f| int(&r[*f]) == 6603)
        })
        || rows(bundle, "spell_learn_spell")
            .iter()
            .any(|r| ["entry", "SpellID"].iter().any(|f| int(&r[*f]) == 6603))
    {
        refuse("neutral_learning_rank_scope_required");
    }
    if spell["cast_time"] != json!({"id":1,"base_ms":0,"per_level_ms":0,"minimum_ms":0})
        || spell["range"] != json!({"id":1,"minimum_binary32":0,"maximum_binary32":0,"flags":0})
        || !(spell["duration"].is_null()
            || spell["duration"] == json!({"id":0,"base_ms":0,"per_level_ms":0,"maximum_ms":0}))
    {
        refuse("auxiliary_resolution");
    }
    if refusals.is_empty() {
        plan["status"] = json!("supported_shape");
        plan["input_bindings"] = json!({"numeric_capture_sha256":scope["coverage_sha256"],
            "effective_words_sha256":digest(&spell["server_numeric_words"]),
            "registry_sha256":registry["coverage_sha256"],"initial_hooks_sha256":hooks["coverage_sha256"]});
    }
    plan["refusals"] = json!(refusals);
    Ok(plan)
}

pub(super) fn reason_supported(reason: &Value) -> bool {
    matches!(
        reason.as_str(),
        Some(
            "spell.attribute_bits_execution_required"
                | "spell.effect_execution_required"
                | "spell.native_effect_target_scaling_rules_required"
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_attack_marker_and_all_numeric_mutations_match_the_preparation_contract() {
        let corpus: Value = serde_json::from_slice(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../scripts/native_starter/combat_world_rules_cases.json"
        )))
        .unwrap();
        let fixture = corpus["fixtures"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == "food-drink-execution")
            .unwrap();
        let spells = fixture["preparation"]["spells"].as_array().unwrap();
        let spell = spells.iter().find(|s| s["id"] == 6603).unwrap();
        assert_eq!(
            supported(&fixture["source"], spell).unwrap(),
            spell["execution_plan"]
        );
        for i in (0..173).filter(|i| !(120..=155).contains(i)) {
            let mut changed = spell.clone();
            changed["server_numeric_words"][i] =
                json!(int(&changed["server_numeric_words"][i]) ^ 1);
            let plan = supported(&fixture["source"], &changed).unwrap();
            assert_eq!(plan["status"], "blocked", "word {i}");
            assert!(plan["refusals"]
                .as_array()
                .unwrap()
                .contains(&json!("spell_execution.attack_numeric_marker_shape")));
        }
        for (field, value) in [
            ("numeric_capture", Value::Null),
            ("server_numeric_words", Value::Null),
            ("script_registry", Value::Null),
            ("initial_hooks", Value::Null),
            ("cast_time", Value::Null),
            ("range", Value::Null),
            ("server_client_differences", json!([{"word":6}])),
            (
                "duration",
                json!({"id":0,"base_ms":1,"per_level_ms":0,"maximum_ms":1}),
            ),
        ] {
            let mut changed = spell.clone();
            changed[field] = value;
            assert_eq!(
                supported(&fixture["source"], &changed).unwrap()["status"],
                "blocked",
                "{field}"
            );
        }
        let other = spells.iter().find(|s| s["id"] == 966603).unwrap();
        assert_eq!(
            super::super::supported_plan(&fixture["source"], other, spells).unwrap()["status"],
            "blocked"
        );
    }
}
