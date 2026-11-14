// SPDX-License-Identifier: GPL-3.0-or-later
// Independently reconstructs the captured Food/Drink item-spell execution plan.
use super::*;
pub(super) fn supported(bundle: &Value, spell: &Value) -> Result<Value> {
    let id = int(&spell["id"]);
    let food = id == 433;
    let mut plan = json!({"adapter":"captured_food_drink_existing_item_spell_authority_v1","status":"blocked",
        "classification":"existing_spell_authority","effective_learning_proven":false,"runtime_export":null,"refusals":[]});
    let scope = &spell["numeric_capture"];
    let Some(words) = spell["server_numeric_words"]
        .as_array()
        .filter(|_| !scope.is_null())
    else {
        plan["refusals"] = json!(["spell_execution.complete_effective_numeric_inputs_required"]);
        return Ok(plan);
    };
    let w: [u32; 173] = std::array::from_fn(|i| int(&words[i]) as u32);
    let mut reasons = Vec::<String>::new();
    let mut refuse = |reason: &str| reasons.push(format!("spell_execution.{reason}"));
    if !spell["server_client_differences"]
        .as_array()
        .unwrap()
        .is_empty()
    {
        refuse("server_client_mismatch");
    }
    let registry = &spell["script_registry"];
    let hooks = &spell["initial_hooks"];
    let scripts = if food {
        json!([{"Id":433,"ScriptName":"spell_food_animation"}])
    } else {
        json!([])
    };
    if registry.is_null() || registry["rows"] != scripts {
        refuse("consumable_script_source_closure");
    }
    if hooks.is_null()
        || ["spell_area", "spell_proc_event"]
            .iter()
            .any(|k| !hooks[*k].as_array().unwrap().is_empty())
    {
        refuse("selected_hook_absence_required");
    }
    let expected = [
        (0, id as u32),
        (1, 0),
        (2, if food { 11 } else { 59 }),
        (6, 0x1800_0100),
        (18, 1),
        (19, 0),
        (20, 60_000),
        (21, 1),
        (22, 0x40080),
        (25, 101),
        (27, 0),
        (28, 5),
        (29, if food { 0 } else { 5 }),
        (31, 0),
        (32, 0),
        (36, 1),
        (58, u32::MAX),
        (59, if food { 0 } else { u32::MAX }),
        (61, 6),
        (64, 1),
        (67, 1),
        (76, if food { 16 } else { 41 }),
        (82, 1),
        (91, if food { 84 } else { 85 }),
        (97, if food { 0x3f80_0000 } else { 0 }),
        (166, u32::MAX),
        (167, 0x3f80_0000),
        (168, 0x3f80_0000),
        (169, 0x3f80_0000),
    ];
    let r = &scope["row"];
    let modifier = (1..=3).try_fold(false, |bad, slot| -> Result<bool> {
        Ok(bad
            || dbc::binary32(
                r[format!("EffectBonusCoefficient{slot}")]["decimal"]
                    .as_str()
                    .unwrap(),
            )? != 0
            || dbc::binary32(
                r[format!("EffectBonusCoefficientFromAP{slot}")]["decimal"]
                    .as_str()
                    .unwrap(),
            )? != 0)
    })?;
    let cast = &spell["cast_time"];
    let duration = &spell["duration"];
    let range = &spell["range"];
    if expected.into_iter().any(|(i,v)|w[i]!=v)
        || w.iter().enumerate().any(|(i,v)|*v!=0&&!matches!(i,0..=2|6|18..=22|25|28..=30|36|58..=59|61|64|67|76|82|91|97|115..=155|166..=169))
        || nonzero(&r["IsServerSide"]) || nonzero(&r["AttributesServerside"]) || modifier
        || scope["facing"].as_array().unwrap().iter().any(|r|nonzero(&r["facingcasterflag"]))
        || cast.is_null() || int(&cast["id"])!=1 || ["base_ms","per_level_ms","minimum_ms"].iter().any(|k|nonzero(&cast[*k]))
        || range.is_null() || int(&range["id"])!=1 || ["minimum_binary32","maximum_binary32","flags"].iter().any(|k|nonzero(&range[*k])) {
        refuse("consumable_effective_source_morphology");
    }
    if duration.is_null()
        || int(&duration["id"]) != i128::from(w[30])
        || !(1..=3_600_000).contains(&int(&duration["base_ms"]))
        || nonzero(&duration["per_level_ms"])
        || duration["maximum_ms"] != duration["base_ms"]
    {
        refuse("consumable_finite_duration");
    }
    let item = bundle["tables"]["item_template"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|r| int(&r["entry"]) == if food { 2070 } else { 159 })
        });
    if item.is_none_or(|item| {
        int(&item["class"]) != 0
            || int(&item["subclass"]) != 0
            || int(&item["stackable"]) != 20
            || int(&item["spellid_1"]) != id
            || nonzero(&item["spelltrigger_1"])
            || int(&item["spellcharges_1"]) != -1
            || int(&item["spellcategory_1"]) != i128::from(w[2])
            || int(&item["RequiredLevel"]) > 1
            || int(&item["AllowableClass"]) & 128 == 0
            || int(&item["AllowableRace"]) & 1 == 0
            || [
                "RequiredSkill",
                "RequiredSkillRank",
                "requiredspell",
                "requiredhonorrank",
                "RequiredReputationFaction",
            ]
            .iter()
            .any(|k| nonzero(&item[*k]))
            || (2..=5).any(|i| nonzero(&item[format!("spellid_{i}")]))
    }) {
        refuse("starting_consumable_item_source");
    }
    if reasons.is_empty() {
        plan["status"] = json!("supported_shape");
        plan["input_bindings"] = json!({"numeric_capture_sha256":scope["coverage_sha256"],"effective_words_sha256":digest(&spell["server_numeric_words"]),
            "registry_sha256":registry["coverage_sha256"],"initial_hooks_sha256":hooks["coverage_sha256"],"item_template_sha256":digest(item.unwrap())});
    }
    plan["refusals"] = json!(reasons);
    Ok(plan)
}
