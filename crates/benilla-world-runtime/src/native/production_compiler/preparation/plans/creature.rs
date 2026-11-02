// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
fn scalar(v: &Value) -> Result<f32> {
    let raw = v
        .get("decimal")
        .and_then(Value::as_str)
        .map(String::from)
        .unwrap_or_else(|| v.to_string());
    Ok(f32::from_bits(dbc::binary32(&raw)?))
}
fn plan(
    creature: &Value,
    levels: &[Value],
    factions: &dbc::Table,
    models: &[&Value],
    displays: &[Value],
) -> Result<Value> {
    let mut refused = BTreeSet::new();
    let mut refuse = |s: &str| {
        refused.insert(s.to_string());
    };
    let low = int(&creature["MinLevel"]);
    let high = int(&creature["MaxLevel"]);
    let class = int(&creature["UnitClass"]);
    if !(1..=20).contains(&low) || low > high || high > 20 || ![1, 2, 4, 8].contains(&class) {
        refuse("level_or_class_domain");
    }
    if nonzero(&creature["Rank"]) {
        refuse("non_normal_rank");
    }
    if nonzero(&creature["ScriptName"]) {
        refuse("script_name");
    }
    if creature["AIName"] != "" && creature["AIName"] != "EventAI" {
        refuse("ai_name");
    }
    for (field, mask) in [
        ("ExtraFlags", 0x10002),
        ("NpcFlags", 0x13),
        ("UnitFlags", 0x1300),
        ("CreatureTypeFlags", 0x40000),
        ("StaticFlags1", 2),
        ("StaticFlags2", 0x10),
        ("RegenerateStats", 15),
    ] {
        if int(&creature[field]) & !mask != 0 {
            refuse("unsupported_flag_bits");
        }
    }
    if int(&creature["ExtraFlags"]) & 0x10000 != 0
        && (int(&creature["StaticFlags1"]) & 2 == 0 || int(&creature["UnitFlags"]) & 0x100 == 0)
    {
        refuse("civilian_requires_no_xp_and_player_immunity");
    }
    if [
        "DynamicFlags",
        "StaticFlags3",
        "StaticFlags4",
        "MechanicImmuneMask",
        "SchoolImmuneMask",
        "RacialLeader",
    ]
    .iter()
    .any(|n| nonzero(&creature[*n]))
    {
        refuse("unsupported_flags");
    }
    if int(&creature["StaticFlags1"]) & 2 != 0 && int(&creature["UnitFlags"]) & 0x100 == 0 {
        refuse("no_xp_requires_player_immunity");
    }
    if ![1, 2, 3].contains(&int(&creature["InhabitType"]))
        || ![0, 1].contains(&int(&creature["Civilian"]))
    {
        refuse("inhabit_or_civilian_domain");
    }
    if nonzero(&creature["Civilian"]) && int(&creature["ExtraFlags"]) & 0x10000 == 0 {
        refuse("legacy_civilian_without_retained_extra_flag");
    }
    if nonzero(&creature["DamageSchool"])
        || int(&creature["MeleeBaseAttackTime"]) > 60000
        || !(1..=10).contains(&int(&creature["CreatureType"]))
    {
        refuse("physical_mainhand_domain");
    }
    let floats: BTreeMap<_, _> = names(&PLAN["creature_floats"])
        .into_iter()
        .map(|f| Ok((f, scalar(&creature[f])?)))
        .collect::<Result<_>>()?;
    if floats.values().any(|f| f.abs() > 1000000.0) {
        refuse("numeric_domain");
    }
    if [
        "HealthMultiplier",
        "PowerMultiplier",
        "ArmorMultiplier",
        "DamageMultiplier",
    ]
    .iter()
    .any(|f| floats[f] < 0.0)
    {
        refuse("legacy_or_negative_multiplier");
    }
    if !(0.0..=2.0).contains(&floats["DamageVariance"]) {
        refuse("damage_variance");
    }
    for f in [
        "MinLevelHealth",
        "MaxLevelHealth",
        "MinLevelMana",
        "MaxLevelMana",
        "Armor",
        "MeleeAttackPower",
        "RangedAttackPower",
    ] {
        if !(0..=1000000).contains(&int(&creature[f])) {
            refuse("numeric_domain");
        }
    }
    if ["Holy", "Fire", "Nature", "Frost", "Shadow", "Arcane"]
        .iter()
        .any(|n| int(&creature[format!("Resistance{n}")]).abs() > 1000000)
    {
        refuse("numeric_domain");
    }
    let selected: Vec<_> = levels
        .iter()
        .filter(|r| int(&r["Class"]) == class && (low..=high).contains(&int(&r["Level"])))
        .collect();
    if levels
        .iter()
        .map(|r| (int(&r["Class"]), int(&r["Level"])))
        .collect::<BTreeSet<_>>()
        .len()
        != levels.len()
    {
        refuse("conflicting_classlevel_rows");
    }
    if selected
        .iter()
        .map(|r| int(&r["Level"]))
        .collect::<BTreeSet<_>>()
        != (low..=high).collect()
    {
        refuse("missing_classlevel_rows");
    }
    for row in &selected {
        if !nonzero(&row["BaseHealthExp0"]) && !nonzero(&row["Stamina"])
            || scalar(&row["BaseDamageExp0"])? as f64 <= 0.1
        {
            refuse("classlevel_getter_domain");
        }
        for (field, value) in row.as_object().unwrap() {
            if !["Class", "Level"].contains(&field.as_str())
                && !(0.0..=1000000.0).contains(&scalar(value)?)
            {
                refuse("classlevel_numeric_domain");
            }
        }
    }
    if [2, 8].contains(&class)
        && int(&creature["RegenerateStats"]) & 12 != 0
        && selected.iter().any(|r| {
            let i = int(&r["Intellect"]);
            int(&r["BaseMana"]) > i.min(20) + (i - 20).max(0) * 15
        })
    {
        refuse("active_power_regeneration");
    }
    let faction = factions.get(int(&creature["Faction"]) as u32);
    if let Some(f) = faction {
        if f.word(2) & !0x1800 != 0 || (3..6).any(|i| f.word(i) & !0xf != 0) {
            refuse("faction_template_domain");
        }
    } else {
        refuse("missing_faction_template");
    }
    let model_by_id: BTreeMap<_, _> = models.iter().map(|r| (int(&r["modelid"]), *r)).collect();
    let display_by_id: BTreeMap<_, _> = displays.iter().map(|r| (int(&r["id"]), r)).collect();
    let choices:Vec<_>=(1..=4).map(|i|json!({"id":creature[format!("DisplayId{i}")],"probability":creature[format!("DisplayIdProbability{i}")]})).collect();
    if !choices
        .iter()
        .any(|r| nonzero(&r["id"]) && nonzero(&r["probability"]))
        || choices
            .iter()
            .filter(|r| nonzero(&r["id"]))
            .map(|r| int(&r["probability"]))
            .sum::<i128>()
            > 0x7fffffff
    {
        refuse("display_probability_domain");
    }
    for r in &choices {
        if nonzero(&r["id"]) && !model_by_id.contains_key(&int(&r["id"])) {
            refuse("missing_model");
        }
    }
    for model in models {
        if nonzero(&model["modelid_alternative"]) || ![0, 1, 2].contains(&int(&model["gender"])) {
            refuse("model_substitution_domain");
        }
        if model["modelid_other_gender"] == model["modelid"] {
            refuse("redundant_gender_model");
        }
        if nonzero(&model["modelid_other_gender"])
            && !model_by_id.contains_key(&int(&model["modelid_other_gender"]))
        {
            refuse("missing_gender_model");
        }
        let numeric: BTreeMap<_, _> = ["bounding_radius", "combat_reach", "SpeedWalk", "SpeedRun"]
            .into_iter()
            .map(|n| Ok((n, scalar(&model[n])?)))
            .collect::<Result<_>>()?;
        if numeric.values().any(|v| !(0.0..=1000000.0).contains(v))
            || numeric["bounding_radius"] <= 0.0
        {
            refuse("model_numeric_domain");
        }
        let walk = if floats["SpeedWalk"] != 0.0 {
            floats["SpeedWalk"]
        } else {
            numeric["SpeedWalk"]
        };
        let run = if floats["SpeedRun"] != 0.0 {
            floats["SpeedRun"]
        } else {
            numeric["SpeedRun"]
        };
        if !(walk > 0.0 && walk <= 400000.0 && run > 0.0 && (run as f64) <= 1000000.0 / 7.0) {
            refuse("model_speed_domain");
        }
        if display_by_id.get(&int(&model["modelid"])).is_none_or(|d| {
            let scale = f32::from_bits(int(&d["scale_binary32"]) as u32);
            !(scale > 0.0 && scale <= 1000000.0)
        }) {
            refuse("missing_or_invalid_display");
        }
    }
    Ok(
        json!({"policy":"cmangos-8ec338a1-captured-creature-generation-v1","status":if refused.is_empty(){"complete_inputs"}else{"refused"},"refusals":refused,"entry":creature["Entry"],"level_roll":{"minimum":num(low),"maximum":num(high),"inclusive":true,"draw_required":low!=high,"rng_owner":"existing_world_counter"},"template_numeric_binary32":floats.iter().map(|(n,f)|(*n,f.to_bits())).collect::<BTreeMap<_,_>>(),"display_choices":choices,"class_level_rows":selected,"faction_template_words":faction.map(|f|(0..f.len()).map(|i|f.word(i)).collect::<Vec<_>>()),"runtime_consumer":"CreatureCapturedInput::compile -> creature_init::prepare","resolved_stats":null,"world_admission":false}),
    )
}
fn spawn_scope(b: &Value, entry: i128) -> Value {
    let Some(coverage) = b.get("native_spawn_capture") else {
        return Value::Null;
    };
    let mut result = Vec::new();
    let guid_tables = [
        "creature_spawn_data",
        "game_event_creature",
        "pool_creature",
        "creature_addon",
    ];
    let entry_tables = ["pool_creature_template", "creature_template_addon"];
    for spawn in rows(b, "creature")
        .iter()
        .filter(|s| int(&s["id"]) == entry)
    {
        let guid = int(&spawn["guid"]);
        let rs: BTreeMap<_, _> = guid_tables
            .iter()
            .chain(&entry_tables)
            .map(|t| {
                (
                    *t,
                    filtered(
                        b,
                        t,
                        columns(t)[0],
                        if guid_tables.contains(t) { guid } else { entry },
                    ),
                )
            })
            .collect();
        let binding = rs["creature_spawn_data"].as_array().unwrap();
        let template_id = binding.first().map_or(0, |r| int(&r["Id"]));
        let template = rows(b, "creature_spawn_data_template")
            .iter()
            .find(|r| int(&r["Entry"]) == template_id);
        let addon = if !rs["creature_addon"].as_array().unwrap().is_empty() {
            Some("creature_addon")
        } else if !rs["creature_template_addon"].as_array().unwrap().is_empty() {
            Some("creature_template_addon")
        } else {
            None
        };
        let mut hooks: Vec<_> = [
            "game_event_creature",
            "pool_creature",
            "pool_creature_template",
        ]
        .into_iter()
        .filter(|t| !rs[*t].as_array().unwrap().is_empty())
        .collect();
        if let Some(addon) = addon {
            hooks.push(addon);
        }
        result.push(json!({"guid":num(guid),"entry":num(entry),"coverage_sha256":digest(coverage),"tables":rs.iter().map(|(t,rs)|(*t,json!({"status":"complete","rows":rs}))).collect::<BTreeMap<_,_>>(),"spawn_template":{"id":num(template_id),"selection":if template_id!=0{"guid_binding"}else{"default_zero"},"status":if template.is_some(){"captured_row_present"}else{"captured_row_absent"},"row":template},"addon":{"status":if addon.is_some(){"applicable_row_present"}else{"captured_absent"},"table":addon,"row":addon.map(|a|&rs[a][0])},"unimplemented_hook_tables":hooks,"scope":"captured_current_static_entry_and_spawn_guid"}));
    }
    json!(result)
}
fn event_plan(source: &Value, entry: i128, scope: &Value) -> Value {
    if source["event_text_capture"]["status"] != "complete" {
        return Value::Null;
    }
    let rs = scope["matching_rows"].as_array().unwrap();
    if rs.is_empty() {
        return Value::Null;
    }
    let broadcasts: BTreeMap<_, _> = rows(source, "broadcast_text")
        .iter()
        .map(|r| (int(&r["Id"]), r))
        .collect();
    for r in rs {
        if int(&r["event_type"]) != 4
            || nonzero(&r["event_inverse_phase_mask"])
            || nonzero(&r["event_flags"])
            || !(1..=100).contains(&int(&r["event_chance"]))
            || (1..=6).any(|i| nonzero(&r[format!("event_param{i}")]))
            || int(&r["action1_type"]) != 1
            || (2..=3).any(|i| {
                nonzero(&r[format!("action{i}_type")])
                    || (1..=3).any(|j| nonzero(&r[format!("action{i}_param{j}")]))
            })
        {
            return Value::Null;
        }
        let ids = [
            int(&r["action1_param1"]),
            int(&r["action1_param2"]),
            int(&r["action1_param3"]),
        ];
        if ids[0] <= 0 || ids.iter().any(|n| *n < 0) || ids[1] == 0 && ids[2] != 0 {
            return Value::Null;
        }
        for id in ids.into_iter().filter(|n| *n != 0) {
            let Some(text) = broadcasts.get(&id) else {
                return Value::Null;
            };
            if nonzero(&text["ChatTypeID"])
                || ![0, 7].contains(&int(&text["LanguageID"]))
                || !nonzero(&text["Text"])
                || [
                    "SoundEntriesID1",
                    "SoundEntriesID2",
                    "EmoteID1",
                    "EmoteID2",
                    "EmoteID3",
                    "EmoteDelay1",
                    "EmoteDelay2",
                    "EmoteDelay3",
                ]
                .iter()
                .any(|f| nonzero(&text[*f]))
            {
                return Value::Null;
            }
        }
    }
    let owners: Vec<_> = rows(source, "creature")
        .iter()
        .filter(|r| int(&r["id"]) == entry)
        .map(|spawn| {
            let event_ids = [entry, -int(&spawn["guid"])]
                .iter()
                .flat_map(|selector| {
                    rs.iter()
                        .filter(move |r| int(&r["creature_id"]) == *selector)
                        .map(|r| r["id"].clone())
                })
                .collect::<Vec<_>>();
            json!({"database_guid":spawn["guid"],"captured_entry":num(entry),"event_ids":event_ids})
        })
        .collect();
    json!({"policy":"cmangos-8ec338a1-aggro-text-v2","status":"supported_catalog_plan","scope":"captured_current_entry_and_selected_spawn_guids","owners":owners,"runtime_configuration":"unknown","loaded_audience":"unknown","runtime_admission":false})
}
pub(in super::super) fn creatures(c: &mut Compiler<'_>) -> Result<Value> {
    let b = c.source;
    let mut result = Vec::new();
    let models: BTreeMap<_, _> = rows(b, "creature_model_info")
        .iter()
        .map(|r| (int(&r["modelid"]), r))
        .collect();
    let bindings: BTreeMap<_, _> = rows(b, "creature_spawn_data")
        .iter()
        .map(|r| (int(&r["Guid"]), int(&r["Id"])))
        .collect();
    let templates: BTreeMap<_, _> = rows(b, "creature_spawn_data_template")
        .iter()
        .map(|r| (int(&r["Entry"]), r))
        .collect();
    for creature in rows(b, "creature_template") {
        let entry = int(&creature["Entry"]);
        let spawn = spawn_scope(b, entry);
        let selected_spawn = super::spawn::plan(b, entry, &spawn)?;
        for refusal in selected_spawn["refusals"].as_array().unwrap() {
            c.issue(
                "creature",
                json!([refusal["guid"]]),
                refusal["reason"].as_str().unwrap(),
            );
        }
        if selected_spawn["status"] == "supported_shape" {
            c.issue(
                "creature_template",
                json!([num(entry)]),
                "creature.cold_loader_order_required",
            );
            c.issue(
                "creature_template",
                json!([num(entry)]),
                "creature.assistance_runtime_traversal_required",
            );
        }
        let mut selectors = BTreeSet::from([entry]);
        selectors.extend(
            rows(b, "creature")
                .iter()
                .filter(|r| int(&r["id"]) == entry)
                .map(|r| -int(&r["guid"])),
        );
        let event = if b["tables"].get("creature_ai_scripts").is_some() {
            json!({"selectors":numbers(selectors.iter().copied()),"matching_rows":rows(b,"creature_ai_scripts").iter().filter(|r|selectors.contains(&int(&r["creature_id"]))).collect::<Vec<_>>(),"status":if rows(b,"creature_ai_scripts").iter().any(|r|selectors.contains(&int(&r["creature_id"]))){"matching_rows_present"}else{"captured_selectors_empty"},"scope":"captured_current_entry_and_selected_spawn_guids"})
        } else {
            Value::Null
        };
        let event_plan = if event.is_null() {
            Value::Null
        } else {
            event_plan(b, entry, &event)
        };
        let mut roots: BTreeSet<_> = (1..=4)
            .map(|i| int(&creature[format!("DisplayId{i}")]))
            .collect();
        for s in rows(b, "creature")
            .iter()
            .filter(|s| int(&s["id"]) == entry)
        {
            let template = bindings.get(&int(&s["guid"])).copied().unwrap_or(0);
            if let Some(t) = templates.get(&template) {
                roots.insert(int(&t["ModelId"]));
            }
        }
        roots.remove(&0);
        let mut model_ids = BTreeSet::new();
        let mut pending = roots;
        while let Some(current) = pending.pop_first() {
            integer(&num(current), 1, 0xffffffff, "native_spawn.model_domain")?;
            model_ids.insert(current);
            check(model_ids.len() <= 4096, "native_spawn.model_limit")?;
            if let Some(row) = models.get(&current) {
                let other = integer(
                    &row["modelid_other_gender"],
                    0,
                    0xffffffff,
                    "native_spawn.model_domain",
                )?;
                if other != 0 && !model_ids.contains(&other) {
                    pending.insert(other);
                }
            }
        }
        let mut displays = Vec::new();
        for model_id in &model_ids {
            if let Some(model) = models.get(model_id) {
                if nonzero(&model["modelid_alternative"]) {
                    c.context_issue(
                        "creature_model_info",
                        json!([num(*model_id)]),
                        "creature.alternative_model_context_required",
                        json!({"alternative":model["modelid_alternative"]}),
                    );
                }
            } else {
                c.need(
                    "creature_model_info",
                    *model_id,
                    "creature.gender_model_not_captured",
                );
            }
            if let Some(display) = c.lookup("CreatureDisplayInfo", *model_id, [0, 1, 3, 4, 9])? {
                let scale = dbc::finite(display.word(4), "creature.display_scale_nonfinite")?;
                if scale & 0x80000000 != 0 || scale == 0 {
                    c.issue(
                        "CreatureDisplayInfo",
                        json!([num(*model_id)]),
                        "creature.display_scale_nonpositive",
                    );
                }
                displays.push(json!({"id":num(*model_id),"model_id":display.word(1),"extended_display_info_id":display.word(3),"scale_binary32":scale,"size_class":display.word(9) as i32,"source":display.raw()}));
            }
        }
        c.lookup("FactionTemplate", int(&creature["Faction"]), 0..14)?;
        let selected_models: Vec<_> = rows(b, "creature_model_info")
            .iter()
            .filter(|r| model_ids.contains(&int(&r["modelid"])))
            .collect();
        let generation = plan(
            creature,
            rows(b, "creature_template_classlevelstats"),
            &c.dbcs["FactionTemplate"],
            &selected_models,
            &displays,
        )?;
        result.push(json!({"entry":num(entry),"source_row":creature,"spawns":filtered(b,"creature","id",entry),"class_level_inputs":rows(b,"creature_template_classlevelstats").iter().filter(|r|r["Class"]==creature["UnitClass"]&&(int(&creature["MinLevel"])..=int(&creature["MaxLevel"])).contains(&int(&r["Level"]))).collect::<Vec<_>>(),"model_inputs":selected_models,"display_inputs":displays,"selected_display_ids":numbers(model_ids),"event_ai_capture":event,"event_ai_plan":event_plan,"generation_plan":generation,"native_spawn_dependencies":spawn,"spawn_context_plan":selected_spawn,"resolved_combat_stats":null,"ai_profile":null}));
        c.context_issue("creature_template",json!([num(entry)]),"creature.ai_selection_and_native_melee_required",json!({"ai_name":creature["AIName"],"scripted":nonzero(&creature["ScriptName"]),"movement_type":creature["MovementType"],"unit_class":creature["UnitClass"]}));
        if spawn.is_null() {
            c.issue(
                "creature_template",
                json!([num(entry)]),
                "creature.spawn_dependency_coverage_not_proven",
            );
        } else {
            for s in spawn.as_array().unwrap() {
                if s["spawn_template"]["row"].is_null() {
                    c.issue(
                        "creature",
                        json!([s["guid"]]),
                        "creature.default_spawn_template_absent",
                    );
                }
                if !s["unimplemented_hook_tables"]
                    .as_array()
                    .unwrap()
                    .is_empty()
                {
                    c.context_issue(
                        "creature",
                        json!([s["guid"]]),
                        "creature.applicable_spawn_hooks_require_execution",
                        json!({"tables":s["unimplemented_hook_tables"]}),
                    );
                }
            }
        }
        c.issue(
            "creature_template",
            json!([num(entry)]),
            if spawn.is_null() {
                "creature.unexported_groups_addons_assistance_context"
            } else {
                "creature.unexported_groups_assistance_context"
            },
        );
        if event.is_null() {
            c.issue(
                "creature_template",
                json!([num(entry)]),
                "creature.event_ai_applicability_absence_not_proven",
            );
        } else if !event["matching_rows"].as_array().unwrap().is_empty() && event_plan.is_null() {
            c.context_issue("creature_template",json!([num(entry)]),"creature.event_ai_matching_rows_require_execution",json!({"event_ids":event["matching_rows"].as_array().unwrap().iter().map(|r|r["id"].clone()).collect::<Vec<_>>()}));
        }
        c.issue(
            "creature_template",
            json!([num(entry)]),
            "creature.explicit_world_rules_and_aggro_context_required",
        );
        c.issue(
            "creature_template",
            json!([num(entry)]),
            "creature.deterministic_respawn_selection_required",
        );
    }
    Ok(json!(result))
}
