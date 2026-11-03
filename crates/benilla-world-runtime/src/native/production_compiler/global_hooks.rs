// SPDX-License-Identifier: GPL-3.0-or-later
// SpellMgr raw cursors and ScriptDevAIMgr registrations at the bound pin.
use super::{check, dbc, json::*, source::*, Result};
use serde_json::{json, Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};
static CONTRACT: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../scripts/classic_export/global_hooks_contract.json"
    )))
    .expect("reviewed hook contract")
});
fn raw<'a>(r: &'a Value, table: &str) -> &'a [Value] {
    r[table].as_array().unwrap().as_slice()
}
fn nums(ids: impl IntoIterator<Item = i128>) -> Value {
    json!(ids.into_iter().map(|id| id as u32).collect::<Vec<_>>())
}
fn numeric_ids(b: &Value, r: &Value) -> BTreeSet<i128> {
    let mut result = ids(b, "spell_template", "Id");
    for item in rows(b, "item_template") {
        for i in 1..=5 {
            result.insert(int(&item[format!("spellid_{i}")]));
        }
    }
    for (table, keys) in [
        ("spell_area", vec!["spell", "aura_spell"]),
        (
            "spell_chain",
            vec!["spell_id", "prev_spell", "first_spell", "req_spell"],
        ),
        ("spell_proc_event", vec!["entry"]),
        ("spell_affect", vec!["entry"]),
        ("spell_threat", vec!["entry"]),
        ("spell_elixir", vec!["entry"]),
        ("spell_group_spell", vec!["SpellId"]),
    ] {
        for row in raw(r, table) {
            for key in &keys {
                result.insert(int(&row[*key]).abs());
            }
        }
    }
    result.remove(&0);
    result
}
pub(super) fn validate(b: &Value) -> Result<()> {
    let v = &b["global_hook_capture"];
    object(v, "global_hooks.extension")?;
    let config = &v["configuration"];
    let expected_config = CONTRACT["manifest"]["configuration"].as_object().unwrap();
    fields(
        config,
        &expected_config
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        "global_hooks.configuration",
    )?;
    check(
        config.as_object().unwrap().values().all(Value::is_boolean),
        "global_hooks.configuration",
    )?;
    let specs = CONTRACT["tables"].as_object().unwrap();
    let r = &v["rows"];
    fields(
        r,
        &specs.keys().map(String::as_str).collect::<Vec<_>>(),
        "global_hooks.tables",
    )?;
    let mut schema = Map::new();
    for (table, spec) in specs {
        let columns = spec["columns"].as_object().unwrap();
        let mut metadata = Map::new();
        let mut seen = BTreeSet::new();
        let records = array(&r[table], "global_hooks.row_limit")?;
        check(records.len() <= 4096, "global_hooks.row_limit")?;
        let field_names: Vec<_> = columns.keys().map(String::as_str).collect();
        let mut parsed_columns = Vec::with_capacity(columns.len());
        for (name, c) in columns {
            let kind = sql_type(c["type"].as_str().unwrap())?;
            metadata.insert(
                name.to_lowercase(),
                json!([kind.family, kind.unsigned, c["nullable"]]),
            );
            parsed_columns.push((
                name,
                Column {
                    kind,
                    nullable: c["nullable"].as_bool().unwrap(),
                },
            ));
        }
        schema.insert(table.clone(),json!({"columns":metadata,"primary_key":spec["keys"].as_array().unwrap().iter().map(|k|k.as_str().unwrap().to_lowercase()).collect::<Vec<_>>()}));
        for row in records {
            fields(row, &field_names, "global_hooks.row_fields")?;
            for (name, column) in &parsed_columns {
                check(
                    cell(&row[*name], column)? == row[*name],
                    "global_hooks.noncanonical_row",
                )?;
            }
            let key: Vec<_> = spec["keys"]
                .as_array()
                .unwrap()
                .iter()
                .map(|k| row[k.as_str().unwrap()].clone())
                .collect();
            check(
                seen.insert(canonical(&json!(key))),
                "global_hooks.duplicate_row",
            )?;
        }
    }
    check(v["schema"] == Value::Object(schema), "global_hooks.schema")?;
    let selected = numeric_ids(b, r);
    check(selected.len() <= 4096, "global_hooks.lookup_limit")?;
    check(
        selected.iter().all(|id| (1..=0xffffffff).contains(id)),
        "global_hooks.lookup_selection",
    )?;
    check(
        raw(r, "spell_template")
            .iter()
            .all(|row| selected.contains(&int(&row["Id"]))),
        "global_hooks.lookup_selection",
    )?;
    for table in [
        "spell_template",
        "spell_area",
        "spell_proc_event",
        "spell_chain",
        "spell_scripts",
    ] {
        let names = specs[table]["columns"].as_object().unwrap();
        let original = if table == "spell_template" && b.get("spell_numeric_capture").is_some() {
            b["spell_numeric_capture"]["rows"]
                .as_array()
                .unwrap()
                .as_slice()
        } else {
            rows(b, table)
        };
        for row in original {
            check(
                raw(r, table).iter().any(|record| {
                    names
                        .keys()
                        .filter(|n| row.get(*n).is_some())
                        .all(|n| record[n] == row[n])
                }),
                "global_hooks.overlapping_source_changed",
            )?;
        }
        if table != "spell_template" && b["tables"].get(table).is_some() {
            let key = match table {
                "spell_area" => "spell",
                "spell_scripts" => "Id",
                "spell_chain" => "spell_id",
                _ => "entry",
            };
            let selected = ids(b, "spell_template", "Id");
            let before: BTreeSet<_> = rows(b, table)
                .iter()
                .filter(|r| selected.contains(&int(&r[key])))
                .map(canonical)
                .collect();
            let after: BTreeSet<_> = raw(r, table)
                .iter()
                .filter(|r| selected.contains(&int(&r[key])))
                .map(canonical)
                .collect();
            check(before == after, "global_hooks.overlapping_source_changed")?;
        }
    }
    let expected = json!({"format":"benilla-classic-global-hooks-v1","contract_sha256":digest(&*CONTRACT),"configuration":config,"registration_manifest_sha256":digest(&CONTRACT["manifest"]),"schema":v["schema"],"rows":r,"spell_ids":nums(selected),"cursor_semantics":"captured_select_cursor_not_observed_reference_loader","binding":{"selection_sha256":digest(&b["selection"]),"schema_sha256":b["schema_sha256"],"table_sha256":digest(&b["table_sha256"]),"db_version_sha256":digest(&b["db_version"]),"consistency":b["consistency"],"rows_sha256":digest(r)}});
    check(*v == expected, "global_hooks.coverage_mismatch")
}
fn rank_edges(r: &Value, dbcs: &dbc::Tables) -> Vec<(i128, i128)> {
    let mut edges = Vec::new();
    for row in raw(r, "spell_chain") {
        for key in ["prev_spell", "first_spell"] {
            edges.push((int(&row["spell_id"]), int(&row[key]).abs()));
        }
    }
    if let Some(t) = dbcs.get("Talent") {
        for row in (0..t.count).map(|i| t.row(i)) {
            let mut previous = None;
            for id in (4..9).map(|i| i128::from(row.word(i))).filter(|v| *v != 0) {
                if let Some(prev) = previous {
                    edges.push((prev, id));
                }
                previous = Some(id);
            }
        }
    }
    if let Some(t) = dbcs.get("SkillLineAbility") {
        for row in (0..t.count).map(|i| t.row(i)) {
            edges.push((row.word(2).into(), row.word(8).into()));
        }
        edges.push((2366, 2368));
    }
    edges
}
fn domain(ids: impl IntoIterator<Item = i128>, edges: &[(i128, i128)]) -> BTreeSet<i128> {
    let mut result: BTreeSet<_> = ids.into_iter().collect();
    loop {
        let before = result.len();
        for (a, b) in edges {
            if *a != 0 && *b != 0 && (result.contains(a) || result.contains(b)) {
                result.insert(*a);
                result.insert(*b);
            }
        }
        if result.len() == before {
            return result;
        }
    }
}
fn callback(b: &Value, kind: &str, id: &Value, name: &Value) -> Value {
    let m = &CONTRACT["manifest"];
    let registration = &m["registrations"][name.as_str().unwrap()];
    let (status, reason) = if name == "" {
        ("absent", "empty_object_script_name")
    } else if b["global_hook_capture"]["configuration"] != m["configuration"] {
        ("unknown", "nondefault_build_configuration_not_reviewed")
    } else if registration.is_null() || registration["kind"] != kind {
        ("unknown", "registration_source_not_reviewed")
    } else {
        (
            "present_unsupported",
            "registered_callback_execution_required",
        )
    };
    json!({"kind":kind,"id":id,"name":name,"status":status,"reason":reason,"registration":if name==""{&Value::Null}else{registration}})
}
pub(super) fn reviewed_creature(b: &Value, row: &Value) -> bool {
    b.get("global_hook_capture").is_some()
        && b["global_hook_capture"]["configuration"] == CONTRACT["manifest"]["configuration"]
        && row["ScriptName"] == "npc_garments_of_quests"
}
pub(super) fn plan(b: &Value, dbcs: &dbc::Tables) -> Value {
    let capture = &b["global_hook_capture"];
    let r = &capture["rows"];
    let edges = rank_edges(r, dbcs);
    let ids = ids(b, "spell_template", "Id");
    let all = domain(ids.iter().copied(), &edges);
    let numeric: BTreeMap<_, _> = raw(r, "spell_template")
        .iter()
        .map(|r| (int(&r["Id"]), r))
        .collect();
    let rank_complete = ["Talent", "SkillLineAbility"]
        .iter()
        .all(|t| dbcs.contains_key(*t));
    let mut spells = Vec::new();
    for id in &ids {
        let inherited = domain([*id], &edges);
        let row = numeric[id];
        let mut families = Map::new();
        for table in [
            "spell_proc_event",
            "spell_affect",
            "spell_threat",
            "spell_elixir",
        ] {
            let applicable: Vec<_> = raw(r, table)
                .iter()
                .filter(|r| inherited.contains(&int(&r["entry"])))
                .collect();
            let intrinsic = match table {
                "spell_proc_event" => int(&row["procFlags"]) != 0,
                "spell_affect" => (1..=3).any(|i| {
                    int(&row[format!("EffectItemType{i}")]) != 0
                        && matches!(
                            int(&row[format!("EffectApplyAuraName{i}")]),
                            107 | 108 | 109 | 112
                        )
                }),
                _ => false,
            };
            families.insert(table.into(),json!({"status":if !applicable.is_empty()||intrinsic{"present_unsupported"}else if rank_complete{"absent"}else{"unknown"},"rows":applicable,"intrinsic":intrinsic}));
        }
        let groups: Vec<_> = raw(r, "spell_group_spell")
            .iter()
            .filter(|r| inherited.contains(&int(&r["SpellId"])))
            .collect();
        families.insert("spell_group".into(),json!({"status":if !groups.is_empty(){"present_unsupported"}else if rank_complete{"absent"}else{"unknown"},"rows":groups}));
        let coefficients: Map<_, _> = row
            .as_object()
            .unwrap()
            .iter()
            .filter(|(n, _)| n.starts_with("EffectBonus"))
            .map(|(n, v)| (n.clone(), v.clone()))
            .collect();
        let registry: Vec<_> = raw(r, "spell_scripts")
            .iter()
            .filter(|r| int(&r["Id"]) == *id)
            .map(|r| {
                callback(
                    b,
                    CONTRACT["manifest"]["registrations"][r["ScriptName"].as_str().unwrap()]
                        ["kind"]
                        .as_str()
                        .unwrap_or("spell"),
                    &r["Id"],
                    &r["ScriptName"],
                )
            })
            .collect();
        spells.push(json!({"id":*id as u32,"rank_closure":nums(inherited),"families":families,"bonus":{"status":"complete_inputs","owner":"spell_template","coefficients":coefficients},"registry":registry}));
    }
    let mut areas = Vec::new();
    for (ordinal, row) in raw(r, "spell_area").iter().enumerate() {
        let mut paths = Vec::new();
        if ids.contains(&int(&row["spell"])) {
            paths.push("spell_location_check");
        }
        if all.contains(&int(&row["aura_spell"]).abs()) {
            paths.push("aura_apply");
        }
        if int(&row["area"]) != 0 {
            paths.push("location_or_quest_transition_requires_location_owner");
        }
        let condition = int(&row["condition_id"]) & 0xffff;
        let status = if paths
            .iter()
            .any(|p| *p != "location_or_quest_transition_requires_location_owner")
        {
            "present_unsupported"
        } else if !paths.is_empty() {
            "unknown"
        } else {
            "unreachable_by_selected_spell_aura_and_area_indices"
        };
        areas.push(json!({"ordinal":ordinal,"row":row,"area_indexed":int(&row["area"])!=0,"condition_branch":if condition!=0{"condition"}else{"legacy_gender_race_quest"},"condition":if condition!=0{raw(r,"conditions").iter().find(|r|int(&r["condition_entry"])==condition)}else{None},"paths":paths,"status":status}));
    }
    let mut callbacks = Vec::new();
    for row in rows(b, "creature_template") {
        callbacks.push(callback(b, "creature", &row["Entry"], &row["ScriptName"]));
    }
    for row in rows(b, "item_template") {
        callbacks.push(callback(b, "item", &row["entry"], &row["ScriptName"]));
    }
    for table in ["world_template", "instance_template"] {
        for row in raw(r, table)
            .iter()
            .filter(|r| r["map"] == b["selection"]["map_id"])
        {
            callbacks.push(callback(
                b,
                "map",
                &b["selection"]["map_id"],
                &row["ScriptName"],
            ));
        }
    }
    let items:Vec<_>=rows(b,"item_template").iter().map(|r| {let spells:Vec<_>=(1..=5).map(|i|&r[format!("spellid_{i}")]).filter(|v|int(v)!=0).collect(); json!({"id":r["entry"],"spells":spells,"deletion_callback":callback(b,"item",&r["entry"],&r["ScriptName"]),"status":if r["ScriptName"]!=""||int(&r["area"])!=0||int(&r["Map"])!=0||!spells.is_empty(){"present_unsupported"}else{"absent"},"operands_sha256":digest(r)}) }).collect();
    let rank_inputs: Map<_, _> = ["Talent", "SkillLineAbility"]
        .into_iter()
        .map(|t| (t.into(), json!(dbcs.get(t).map(|t| &t.sha256))))
        .collect();
    json!({"format":"cmangos-8ec338a1-global-hook-applicability-v1","status":"diagnostic_only","capture_sha256":digest(capture),"configuration":capture["configuration"],"registration_manifest_sha256":capture["registration_manifest_sha256"],"rank_inputs":rank_inputs,"rank_semantics":"conservative_raw_connected_superset_not_effective_loader","spell_area_loader_order":"captured_cursor_reference_loader_receipt_not_observed","first_entry_location":"requires_source_location_owner_not_playercreateinfo_zone","registered_callbacks_resolved":callbacks.iter().all(|r|r["status"]=="absent"),"global_quest_effects_resolved":false,"quest_transition_domain":"unknown_requires_source_location_aura_and_inventory_owners","item_effect_scope":"selected_item_spell_and_ScriptDevAI_callbacks_only","spells":spells,"spell_area":areas,"callbacks":callbacks,"item_effects":items,"effective_learning_proven":false,"runtime_export":null})
}
