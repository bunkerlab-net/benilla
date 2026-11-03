// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

pub(super) fn plan(source: &Value, closure: &Value, source_sha: &str) -> Value {
    let mut refusals = Vec::new();
    let has_ids = rows(source, "creature_template")
        .iter()
        .any(|r| int(&r["PickpocketLootId"]) != 0 || int(&r["SkinningLootId"]) != 0)
        || rows(source, "item_template")
            .iter()
            .any(|r| int(&r["DisenchantID"]) != 0);
    let mut refuse = |table: &str, key: String, reason: &str| {
        refusals.push(json!({"table":table,"key":key,"reason":format!("loot.{reason}")}));
    };
    if has_ids && closure["captured_inputs_complete"] != true {
        refuse(
            "spell_template",
            "scope".into(),
            "non_body_learning_closure_incomplete",
        );
    }
    for extension in [
        "learning_capture",
        "initial_hook_capture",
        "spell_script_capture",
        "event_ai_capture",
    ] {
        if has_ids && source.get(extension).is_none() {
            refuse(extension, "scope".into(), "non_body_hook_coverage_unknown");
        }
    }
    for row in rows(source, "spell_template") {
        for (effect_key, misc_key) in [
            ("Effect1", "EffectMiscValue1"),
            ("Effect2", "EffectMiscValue2"),
            ("Effect3", "EffectMiscValue3"),
        ] {
            let effect = int(&row[effect_key]);
            let misc = int(&row[misc_key]);
            if matches!(effect, 71 | 95 | 99) {
                refuse(
                    "spell_template",
                    int(&row["Id"]).to_string(),
                    "non_body_spell_reachable",
                );
            }
            if matches!(effect, 44 | 118) && matches!(misc, 182 | 186 | 333 | 393) {
                refuse(
                    "spell_template",
                    int(&row["Id"]).to_string(),
                    "non_body_skill_reachable",
                );
            }
        }
    }
    for skill in closure["skill_ids"].as_array().into_iter().flatten() {
        if matches!(int(skill), 182 | 186 | 333 | 393) {
            refuse(
                "SkillLine",
                int(skill).to_string(),
                "non_body_skill_reachable",
            );
        }
    }
    for table in ["spell_area", "spell_proc_event"] {
        if !rows(source, table).is_empty() {
            refuse(table, "scope".into(), "non_body_hook_execution_unproven");
        }
    }
    let mut hooks = Vec::new();
    for row in rows(source, "spell_scripts") {
        let gate = match row["ScriptName"].as_str().unwrap() {
            "spell_food_animation" => Some("food_animation_heartbeat_only"),
            "spell_gameobject_call_for_help_on_usage" | "spell_opening_capping" => {
                Some("unavailable_gameobject_executor")
            }
            _ => None,
        };
        if gate.is_none() || row["ScriptName"] == "spell_food_animation" && row["Id"] != 433 {
            refuse(
                "spell_scripts",
                int(&row["Id"]).to_string(),
                "non_body_script_hook_unreviewed",
            );
        }
        let mut hook = row.clone();
        hook["gate"] = json!(gate);
        hooks.push(hook);
    }
    for row in rows(source, "item_template") {
        if row["ScriptName"] != "" {
            refuse(
                "item_template",
                int(&row["entry"]).to_string(),
                "non_body_item_hook_unreviewed",
            );
        }
    }
    for row in rows(source, "creature_template") {
        if row["ScriptName"] != "" {
            refuse(
                "creature_template",
                int(&row["Entry"]).to_string(),
                "non_body_creature_hook_unreviewed",
            );
        }
    }
    let hook_bindings: Map<String, Value> = [
        "creature_ai_scripts",
        "creature_template_addon",
        "creature_addon",
        "spell_scripts",
        "spell_area",
        "spell_proc_event",
        "quest_template",
    ]
    .into_iter()
    .map(|name| (name.into(), json!(digest(&source["tables"][name]))))
    .collect();
    let mut unavailable_calls = Vec::new();
    for row in rows(source, "creature_ai_scripts") {
        for (slot, key) in [
            (1, "action1_type"),
            (2, "action2_type"),
            (3, "action3_type"),
        ] {
            let action = &row[key];
            if !matches!(int(action), 0 | 1) {
                unavailable_calls.push(json!({"event":row["id"],"slot":slot,"action":action,
                    "refusal":"creature.event_ai_matching_row_requires_execution"}));
            }
        }
    }
    let creatures: Vec<_> = rows(source, "creature_template").iter().map(|r| {
        json!({"Entry":r["Entry"],"LootId":r["LootId"],"PickpocketLootId":r["PickpocketLootId"],"SkinningLootId":r["SkinningLootId"]})
    }).collect();
    let items: Vec<_> = rows(source, "item_template")
        .iter()
        .map(|r| json!({"entry":r["entry"],"DisenchantID":r["DisenchantID"]}))
        .collect();
    json!({"status":if !refusals.is_empty(){"blocked"}else if has_ids{"unavailable_proven"}else{"not_applicable_no_non_body_loot_ids"},
        "policy":crate::native::loot::BODY_ONLY_COMMAND_POLICY,
        "command_domain":"Command_casts_use_only_bound_executors_body_loot_has_no_type_operand",
        "trainer_domain":"no_train_learn_skill_or_learn_spell_command",
        "hook_domain":"only_bound_native_executors_unimplemented_hooks_refuse_before_entry",
        "global_source_hooks_absent":false,"learning_closure_sha256":digest(closure),"source_sha256":source_sha,
        "hook_table_sha256":hook_bindings,"unavailable_event_ai_calls":unavailable_calls,
        "native_hook_executors":["EventAI_TEXT","quest_script_21_presentation","bound_spell_callbacks"],
        "spell_ids":closure["spell_ids"].as_array().cloned().unwrap_or_default(),
        "skill_ids":closure["skill_ids"].as_array().cloned().unwrap_or_default(),"script_gates":hooks,
        "creature_loot_ids":creatures,"item_loot_ids":items,"refusals":refusals})
}
