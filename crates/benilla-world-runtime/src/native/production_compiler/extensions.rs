// SPDX-License-Identifier: GPL-3.0-or-later
use super::{check, fail, json::*, source::*, Result, COMPILER_CONTRACT};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
mod loot;
pub(super) mod participants;
pub(super) mod quest_mechanics;
pub(super) mod skill_training;
const FILTER: &str = "all_matching_rows_no_loader_filters";
const REL_FIELDS: [&str; 6] = [
    "entry",
    "PrevQuestId",
    "NextQuestId",
    "NextQuestInChain",
    "ExclusiveGroup",
    "BreadcrumbForQuestId",
];
fn present(b: &Value, key: &str) -> bool {
    b.get(key).is_some()
}
fn has(b: &Value, table: &str) -> bool {
    b["tables"].get(table).is_some()
}
fn numbers(ids: impl IntoIterator<Item = i128>) -> Value {
    json!(ids.into_iter().map(|v| v as i64).collect::<Vec<_>>())
}
pub(super) fn snapshot(b: &Value, tables: &[&str]) -> Value {
    let hashes: Map<_, _> = tables
        .iter()
        .filter(|t| has(b, t))
        .map(|t| ((*t).into(), b["table_sha256"][*t].clone()))
        .collect();
    json!({"consistency":b["consistency"],"selection_sha256":digest(&b["selection"]),"schema_sha256":b["schema_sha256"],"db_version_sha256":digest(&b["db_version"]),"table_sha256":hashes})
}
fn identity_snapshot(b: &Value) -> Value {
    json!({"core_revision":b["core_revision"],"database_revision":b["database_revision"],"schema_sha256":b["schema_sha256"],"consistency":b["consistency"],"selection_sha256":digest(&b["selection"]),"db_version_sha256":digest(&b["db_version"])})
}
fn require_tables(b: &Value, tables: &[&str], code: &str) -> Result<()> {
    check(tables.iter().all(|t| has(b, t)), code)
}
fn selected_domain(
    b: &Value,
    table: &str,
    key: &str,
    low: i128,
    high: i128,
    code: &str,
) -> Result<BTreeSet<i128>> {
    rows(b, table)
        .iter()
        .map(|r| integer(&r[key], low, high, code))
        .collect()
}
fn within(b: &Value, table: &str, key: &str, ids: &BTreeSet<i128>, code: &str) -> Result<()> {
    check(
        rows(b, table).iter().all(|r| ids.contains(&int(&r[key]))),
        code,
    )
}
fn coverage(b: &Value, extension: &str, expected: Value, code: &str) -> Result<()> {
    check(b[extension] == expected, code)
}

pub(super) fn validate(b: &Value, s: &Selection, p: &Profiles) -> Result<()> {
    let mut guards = vec![
        (
            has(b, "creature_ai_scripts"),
            present(b, "event_ai_capture"),
            "event_ai.coverage_required_with_table",
        ),
        (
            has(b, "creature_ai_scripts"),
            present(b, "event_text_capture"),
            "event_text.coverage_required_with_events",
        ),
    ];
    for (group, extension, code) in [
        (
            "learning",
            "learning_capture",
            "learning.coverage_required_with_tables",
        ),
        (
            "spawn",
            "native_spawn_capture",
            "native_spawn.coverage_required_with_tables",
        ),
        (
            "hooks",
            "initial_hook_capture",
            "initial_hooks.coverage_required_with_tables",
        ),
    ] {
        guards.push((
            contract_names(group).iter().any(|t| has(b, t)),
            present(b, extension),
            code,
        ));
    }
    guards.push((
        has(b, "spell_scripts"),
        present(b, "spell_script_capture"),
        "spell_registry.coverage_required_with_table",
    ));
    for (a, c, code) in guards {
        check(a == c, code)?;
    }
    check(
        !["broadcast_text", "broadcast_text_locale"]
            .iter()
            .any(|t| has(b, t))
            || present(b, "event_text_capture"),
        "event_text.orphan_tables",
    )?;
    check(
        !contract_names("loot").iter().any(|t| has(b, t)) || present(b, "loot_capture"),
        "loot.coverage_required_with_tables",
    )?;
    for (extension, code) in [
        ("event_ai_capture", "event_ai.starter_selection_required"),
        ("learning_capture", "learning.starter_selection_required"),
        (
            "native_spawn_capture",
            "native_spawn.starter_selection_required",
        ),
        (
            "initial_hook_capture",
            "initial_hooks.starter_selection_required",
        ),
        ("loot_capture", "loot.starter_selection_required"),
        (
            "spell_script_capture",
            "spell_registry.starter_selection_required",
        ),
    ] {
        if present(b, extension) {
            check(s.starter.is_some(), code)?;
        }
    }
    if present(b, "event_ai_capture") {
        let mut selected = selected_domain(
            b,
            "creature_template",
            "Entry",
            1,
            0x7fffffff,
            "event_ai.entry_domain",
        )?;
        selected.extend(
            selected_domain(b, "creature", "guid", 1, 0x7fffffff, "event_ai.guid_domain")?
                .iter()
                .map(|n| -n),
        );
        check(selected.len() <= 8192, "event_ai.selector_limit")?;
        coverage(
            b,
            "event_ai_capture",
            json!({"format":"benilla-classic-event-ai-selection-v1","selector_column":"creature_id","selectors":numbers(selected.iter().copied()),"row_limit":4096,"filter":"all_matching_rows_no_event_filters"}),
            "event_ai.coverage_mismatch",
        )?;
        for row in rows(b, "creature_ai_scripts") {
            integer(&row["id"], 1, 0xffffffff, "event_ai.id_domain")?;
            let id = integer(
                &row["creature_id"],
                -0x7fffffff,
                0x7fffffff,
                "event_ai.selector_domain",
            )?;
            check(selected.contains(&id), "event_ai.unselected_row")?;
        }
    }
    if present(b, "event_text_capture") {
        event_text(b, p)?;
    }
    if present(b, "learning_capture") {
        learning(b, s, p)?;
    }
    if present(b, "native_spawn_capture") {
        spawn(b)?;
    }
    if present(b, "initial_hook_capture") {
        require_tables(
            b,
            &["spell_area", "spell_proc_event"],
            "initial_hooks.tables_required",
        )?;
        require_tables(
            b,
            &["spell_chain", "spell_learn_spell"],
            "initial_hooks.learning_required",
        )?;
        let selected = selected_domain(
            b,
            "spell_template",
            "Id",
            1,
            0xffffff,
            "initial_hooks.selector_domain",
        )?;
        check(selected.len() <= 512, "initial_hooks.selector_limit")?;
        let expected = json!({"format":"benilla-classic-initial-hook-selection-v1","selectors":{"spell_area":{"column":"spell","ids":numbers(selected.iter().copied())},"spell_proc_event":{"column":"entry","ids":numbers(selected.iter().copied())}},"filter":FILTER,"row_limits":{"spell_area":4096,"spell_proc_event":512},"scope":"selected_raw_area_and_proc_inputs_rank_semantics_not_resolved","snapshot":snapshot(b,&["spell_template","spell_chain","spell_learn_spell","spell_area","spell_proc_event"])});
        coverage(
            b,
            "initial_hook_capture",
            expected,
            "initial_hooks.coverage_mismatch",
        )?;
        for (table, key, limit) in [
            ("spell_area", "spell", 4096),
            ("spell_proc_event", "entry", 512),
        ] {
            check(rows(b, table).len() <= limit, "initial_hooks.row_limit")?;
            within(b, table, key, &selected, "initial_hooks.unselected_row")?;
        }
    }
    if present(b, "loot_capture") {
        loot::validate(b, p)?;
    }
    check(
        !present(b, "loot_loader_capture") || present(b, "loot_capture"),
        "loot.loader_requires_capture",
    )?;
    loot::loader(b)?;
    if present(b, "spell_script_capture") {
        check(
            rows(b, "spell_scripts").len() <= 512,
            "spell_registry.row_limit",
        )?;
        let selected = selected_domain(
            b,
            "spell_template",
            "Id",
            1,
            0x7fffffff,
            "spell_registry.selector_domain",
        )?;
        check(selected.len() <= 512, "spell_registry.selector_limit")?;
        coverage(
            b,
            "spell_script_capture",
            json!({"format":"benilla-classic-spell-script-selection-v1","selector":{"column":"Id","ids":numbers(selected.iter().copied())},"filter":FILTER,"row_limit":512,"scope":"spell_and_aura_numeric_registry_for_selected_spells","snapshot":snapshot(b,&["spell_template","spell_scripts"])}),
            "spell_registry.coverage_mismatch",
        )?;
        within(
            b,
            "spell_scripts",
            "Id",
            &selected,
            "spell_registry.unselected_row",
        )?;
    }
    Ok(())
}
fn learning(b: &Value, s: &Selection, p: &Profiles) -> Result<()> {
    require_tables(
        b,
        &["spell_chain", "spell_learn_spell"],
        "learning.tables_required",
    )?;
    let v = &b["learning_capture"];
    object(v, "learning.coverage_fields")?;
    let request = v.get("dependency_request").unwrap_or(&Value::Null);
    if !request.is_null() {
        fields(
            request,
            &[
                "format",
                "core_revision",
                "selection",
                "source_sha256",
                "dbc_sha256",
                "item_ids",
                "spell_ids",
            ],
            "learning.request_fields",
        )?;
        check(
            request["format"] == "benilla-native-dependency-request-v1"
                && request["core_revision"] == CORE,
            "learning.request_profile",
        )?;
        check(
            s.starter.is_some() && request["selection"] == s.json(),
            "learning.request_selection",
        )?;
        valid_hash(&request["source_sha256"], "learning.request_hash")?;
        let hashes = object(&request["dbc_sha256"], "learning.request_dbc_hashes")?;
        check(hashes.len() <= 32, "learning.request_dbc_hashes")?;
        for (name, hash) in hashes {
            check(
                !name.is_empty()
                    && name.len() <= 64
                    && name.bytes().all(|c| c.is_ascii_alphanumeric()),
                "learning.request_dbc_name",
            )?;
            valid_hash(hash, "learning.request_hash")?;
        }
        for (field, limit) in [("item_ids", 2048), ("spell_ids", 512)] {
            let a = array(&request[field], "learning.request_id_limit")?;
            check(a.len() <= limit, "learning.request_id_limit")?;
            let mut prev = 0;
            for v in a {
                let n = integer(v, 1, 0xffffffff, "learning.request_id")?;
                check(n > prev, "learning.request_id_order")?;
                prev = n;
            }
        }
    }
    let probe = &v["global_learn_spell_probe"];
    fields(probe, &["predicate", "rows"], "learning.probe_fields")?;
    validate_rows(
        "spell_learn_spell",
        &probe["rows"],
        &p["spell_learn_spell"],
        1,
    )?;
    let mut expected = json!({"format":"benilla-classic-learning-selection-v1","spell_ids":b["tables"]["spell_template"].as_array().unwrap().iter().map(|r|r["Id"].clone()).collect::<Vec<_>>(),"tables":["spell_chain","spell_learn_spell"],"filter":FILTER,"row_limit":4096,"dependency_request":request,"global_learn_spell_probe":{"predicate":"all_rows_limit_one","rows":probe["rows"]}});
    let spells = ids(b, "spell_template", "Id");
    if v["format"] == "benilla-classic-learning-selection-v2" {
        let chains = &v["global_chain_probe"];
        fields(
            chains,
            &["predicate", "rows"],
            "learning.chain_probe_fields",
        )?;
        validate_rows("spell_chain", &chains["rows"], &p["spell_chain"], 1)?;
        let rs = chains["rows"].as_array().unwrap();
        check(
            !rs.is_empty() || rows(b, "spell_chain").is_empty(),
            "learning.empty_chain_probe_with_selected_rows",
        )?;
        if let Some(row) = rs.first() {
            if spells.contains(&int(&row["spell_id"])) {
                check(
                    rows(b, "spell_chain").contains(row),
                    "learning.chain_probe_missing_selected_row",
                )?;
            }
        }
        expected["format"] = json!("benilla-classic-learning-selection-v2");
        expected["global_chain_probe"] =
            json!({"predicate":"all_rows_limit_one","rows":chains["rows"]});
    }
    coverage(
        b,
        "learning_capture",
        expected,
        "learning.coverage_mismatch",
    )?;
    let rs = probe["rows"].as_array().unwrap();
    check(
        !rs.is_empty() || rows(b, "spell_learn_spell").is_empty(),
        "learning.empty_probe_with_selected_rows",
    )?;
    if let Some(row) = rs.first() {
        if spells.contains(&int(&row["entry"])) {
            check(
                rows(b, "spell_learn_spell").contains(row),
                "learning.probe_missing_selected_row",
            )?;
        }
    }
    for (table, key) in [("spell_chain", "spell_id"), ("spell_learn_spell", "entry")] {
        within(b, table, key, &spells, "learning.unselected_row")?;
    }
    let mut referenced = BTreeSet::new();
    for row in rows(b, "spell_chain") {
        integer(&row["rank"], 0, 127, "learning.rank_domain")?;
        for key in ["prev_spell", "first_spell", "req_spell"] {
            let n = integer(&row[key], 0, 0x7fffff, "learning.chain_reference")?;
            if n != 0 {
                referenced.insert(n);
            }
        }
    }
    for row in rows(b, "spell_learn_spell") {
        integer(&row["Active"], 0, 1, "learning.active_domain")?;
        referenced.insert(integer(
            &row["SpellID"],
            1,
            65535,
            "learning.learned_spell",
        )?);
    }
    check(referenced.is_subset(&spells), "learning.reference_missing")?;
    if !request.is_null() {
        check(
            request["spell_ids"]
                .as_array()
                .unwrap()
                .iter()
                .all(|v| spells.contains(&int(v))),
            "learning.request_spell_missing",
        )?;
        let items = ids(b, "item_template", "entry");
        check(
            request["item_ids"]
                .as_array()
                .unwrap()
                .iter()
                .all(|v| items.contains(&int(v))),
            "learning.request_item_missing",
        )?;
    }
    Ok(())
}
pub(super) fn valid_hash(v: &Value, code: &str) -> Result<()> {
    let s = string(v, code)?;
    check(
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        code,
    )
}
fn spawn(b: &Value) -> Result<()> {
    let tables: Vec<_> = contract_names("spawn").into_iter().collect();
    require_tables(b, &tables, "native_spawn.tables_required")?;
    let guids = selected_domain(
        b,
        "creature",
        "guid",
        1,
        0xffffffff,
        "native_spawn.guid_domain",
    )?;
    let entries = selected_domain(
        b,
        "creature_template",
        "Entry",
        1,
        0xffffffff,
        "native_spawn.entry_domain",
    )?;
    let mut templates = selected_domain(
        b,
        "creature_spawn_data",
        "Id",
        0,
        0xffffffff,
        "native_spawn.template_domain",
    )?;
    templates.insert(0);
    let mut selectors = Map::new();
    for table in &tables {
        let set = if *table == "creature_spawn_data_template" {
            &templates
        } else if ["pool_creature_template", "creature_template_addon"].contains(table) {
            &entries
        } else {
            &guids
        };
        check(set.len() <= 4096, "native_spawn.selector_limit")?;
        let column = columns(table)[0];
        selectors.insert(
            (*table).into(),
            json!({"column":column,"ids":numbers(set.iter().copied())}),
        );
        within(b, table, column, set, "native_spawn.unselected_row")?;
    }
    let mut snapshot_tables = vec!["creature", "creature_template"];
    snapshot_tables.extend(tables.iter());
    coverage(
        b,
        "native_spawn_capture",
        json!({"format":"benilla-classic-native-spawn-selection-v1","filter":FILTER,"row_limit":4096,"scope":"named_tables_current_entries_and_selected_spawn_guids","other_native_hooks_proven":false,"selectors":selectors,"snapshot":snapshot(b,&snapshot_tables)}),
        "native_spawn.coverage_mismatch",
    )?;
    let present = ids(b, "creature_spawn_data_template", "Entry");
    check(
        templates.iter().all(|id| *id == 0 || present.contains(id)),
        "native_spawn.template_reference_missing",
    )?;
    for row in rows(b, "creature_spawn_data_template") {
        integer(
            &row["NpcFlags"],
            -1,
            0x7fffffff,
            "native_spawn.npc_flags_domain",
        )?;
        integer(
            &row["UnitFlags"],
            -1,
            0xffffffff,
            "native_spawn.unit_flags_domain",
        )?;
        integer(
            &row["EquipmentId"],
            -1,
            0x7fffff,
            "native_spawn.equipment_domain",
        )?;
    }
    let mut models = BTreeSet::new();
    for row in rows(b, "creature_template") {
        for i in 1..=4 {
            models.insert(integer(
                &row[format!("DisplayId{i}")],
                0,
                0xffffffff,
                "native_spawn.model_domain",
            )?);
        }
    }
    let binding: BTreeMap<_, _> = rows(b, "creature_spawn_data")
        .iter()
        .map(|r| (int(&r["Guid"]), int(&r["Id"])))
        .collect();
    for spawn in rows(b, "creature") {
        let template_id = binding.get(&int(&spawn["guid"])).copied().unwrap_or(0);
        if let Some(t) = rows(b, "creature_spawn_data_template")
            .iter()
            .find(|r| int(&r["Entry"]) == template_id)
        {
            models.insert(integer(
                &t["ModelId"],
                0,
                0xffffffff,
                "native_spawn.model_domain",
            )?);
        }
    }
    models.remove(&0);
    check(
        models.is_subset(&ids(b, "creature_model_info", "modelid")),
        "native_spawn.model_reference_missing",
    )
}

fn text_references(b: &Value) -> Result<(Vec<Value>, Vec<Value>, Vec<Value>)> {
    let (mut positive, mut negative, mut unsupported) = (Vec::new(), Vec::new(), Vec::new());
    for row in rows(b, "creature_ai_scripts") {
        for action in 1..=3 {
            let kind = int(&row[format!("action{action}_type")]);
            let origin = json!({"event_id":row["id"],"creature_id":row["creature_id"],"action_index":action});
            if kind != 0 && kind != 1 {
                let mut v = origin.clone();
                v["action_type"] = json!(kind as i64);
                unsupported.push(v);
            }
            if kind != 1 {
                continue;
            }
            for parameter in 1..=3 {
                let id = integer(
                    &row[format!("action{action}_param{parameter}")],
                    i32::MIN as i128,
                    i32::MAX as i128,
                    "event_text.parameter_domain",
                )?;
                if id != 0 {
                    let mut v = origin.clone();
                    v["parameter_index"] = json!(parameter);
                    v["broadcast_id"] = json!(id as i64);
                    if id > 0 {
                        positive.push(v);
                    } else {
                        negative.push(v);
                    }
                }
            }
        }
    }
    Ok((positive, negative, unsupported))
}
fn query_hash(
    table: &str,
    selected: &BTreeSet<i128>,
    source_names: Option<&BTreeMap<String, String>>,
) -> Result<Value> {
    if selected.is_empty() {
        return Ok(Value::Null);
    }
    let mut params = Vec::new();
    for n in selected {
        let high = if table == "creature_loot_template" || table == "reference_loot_template" {
            0xffffff
        } else {
            0x7fffffff
        };
        check(
            *n >= 1 && *n <= high,
            if high == 0xffffff {
                "query.loot_selector"
            } else {
                "query.event_text_selector"
            },
        )?;
        params.push(json!(*n as i64));
    }
    params.push(json!(4097));
    let mapped = |n: &str| {
        source_names
            .and_then(|m| m.get(n))
            .cloned()
            .unwrap_or_else(|| n.into())
    };
    let cols = columns(table)
        .iter()
        .map(|n| format!("`{}` AS `{n}`", mapped(n)))
        .collect::<Vec<_>>()
        .join(",");
    let order = keys(table)
        .iter()
        .map(|n| format!("`{}`", mapped(n)))
        .collect::<Vec<_>>()
        .join(",");
    let key = if ["broadcast_text", "broadcast_text_locale"].contains(&table) {
        "Id"
    } else {
        "entry"
    };
    let query = format!(
        "SELECT {cols} FROM `{table}` WHERE `{}` IN ({}) ORDER BY {order} LIMIT %s",
        mapped(key),
        vec!["%s"; selected.len()].join(",")
    );
    Ok(json!(digest(&json!([query, params]))))
}
fn event_text(b: &Value, p: &Profiles) -> Result<()> {
    check(
        has(b, "creature_ai_scripts"),
        "event_text.event_rows_required",
    )?;
    let (positive, negative, unsupported) = text_references(b)?;
    let selected: BTreeSet<_> = positive.iter().map(|r| int(&r["broadcast_id"])).collect();
    check(selected.len() <= 4096, "event_text.selector_limit")?;
    let mut captured = Map::new();
    let mut states = Vec::new();
    for table in ["broadcast_text", "broadcast_text_locale"] {
        check(
            p.contains_key(table) == has(b, table),
            "event_text.table_coverage_required",
        )?;
        let mut record = json!({"requested_ids":numbers(selected.iter().copied()),"selector_column":"Id","mode":"event_text_selectors","row_limit":4096,"query_performed":false,"source_columns":null,"query_sha256":null,"returned_keys":null,"rows_sha256":null,"missing_ids":null,"status":"unknown"});
        if has(b, table) {
            within(b, table, "Id", &selected, "event_text.unselected_row")?;
            let meta = b["schema"]["tables"][table]["columns"].as_array().unwrap();
            let mapping: BTreeMap<_, _> = columns(table)
                .into_iter()
                .map(|n| {
                    (
                        n.to_string(),
                        meta.iter()
                            .find(|c| c["name"].as_str().unwrap().eq_ignore_ascii_case(n))
                            .unwrap()["name"]
                            .as_str()
                            .unwrap()
                            .into(),
                    )
                })
                .collect();
            let missing = if table == "broadcast_text" {
                selected.difference(&ids(b, table, "Id")).copied().collect()
            } else {
                BTreeSet::new()
            };
            record["source_columns"] = json!(mapping);
            record["query_sha256"] = query_hash(table, &selected, Some(&mapping))?;
            record["query_performed"] = json!(!selected.is_empty());
            record["returned_keys"] = json!(rows(b, table)
                .iter()
                .map(|r| keys(table)
                    .iter()
                    .map(|n| r[*n].clone())
                    .collect::<Vec<_>>())
                .collect::<Vec<_>>());
            record["rows_sha256"] = json!(digest(&b["tables"][table]));
            record["missing_ids"] = numbers(missing.iter().copied());
            record["status"] = json!(if missing.is_empty() {
                "complete"
            } else {
                "missing"
            });
        }
        states.push(record["status"].as_str().unwrap().to_string());
        captured.insert(table.into(), record);
    }
    let status = if states.iter().any(|s| s == "missing") {
        "missing"
    } else if !negative.is_empty() || states.iter().any(|s| s == "unknown") {
        "unknown"
    } else {
        "complete"
    };
    coverage(
        b,
        "event_text_capture",
        json!({"format":"benilla-classic-event-text-selection-v1","scope":"positive_action_t_text_broadcast_dependencies_only","status":status,"references":positive,"unresolved_negative_text_parameters":negative,"other_actions_not_resolved":unsupported,"tables":captured,"snapshot":snapshot(b,&["creature","creature_template","creature_ai_scripts","broadcast_text","broadcast_text_locale"])}),
        "event_text.coverage_mismatch",
    )
}

pub(super) fn validate_late(b: &Value, p: &Profiles) -> Result<()> {
    check(
        has(b, "creature_spawn_entry") == present(b, "spawn_entry_capture"),
        "spawn_entries.coverage_required_with_table",
    )?;
    if present(b, "spawn_entry_capture") {
        let roots = selected_domain(
            b,
            "creature",
            "guid",
            1,
            0xffffffff,
            "spawn_entries.guid_domain",
        )?;
        check(roots.len() <= 4096, "spawn_entries.root_limit")?;
        for row in rows(b, "creature_spawn_entry") {
            fields(row, &["guid", "entry"], "spawn_entries.row_fields")?;
            let guid = integer(&row["guid"], 1, 0xffffffff, "spawn_entries.guid_domain")?;
            integer(&row["entry"], 0, 0xffffff, "spawn_entries.entry_domain")?;
            check(roots.contains(&guid), "spawn_entries.unselected_row")?;
        }
        let mut snap = identity_snapshot(b);
        snap["creature_sha256"] = json!(digest(&b["tables"]["creature"]));
        coverage(
            b,
            "spawn_entry_capture",
            json!({"format":"benilla-classic-spawn-entry-exclusion-v1","status":"complete","scope":"raw_selected_guid_exclusion_only","roots":numbers(roots.iter().copied()),"selector":{"column":"guid","ids":numbers(roots.iter().copied())},"filter":FILTER,"row_limit":4096,"source_loader_order_proven":false,"other_native_hooks_proven":false,"rows_sha256":digest(&b["tables"]["creature_spawn_entry"]),"snapshot":snap}),
            "spawn_entries.coverage_mismatch",
        )?;
    }
    let context_tables = contract_names("spawn_context");
    check(
        context_tables.iter().any(|table| has(b, table)) == present(b, "spawn_context_capture"),
        "spawn_context.coverage_required_with_tables",
    )?;
    if present(b, "spawn_context_capture") {
        check(
            present(b, "spawn_entry_capture") && present(b, "native_spawn_capture"),
            "spawn_context.related_coverage_required",
        )?;
        require_tables(
            b,
            &context_tables.iter().copied().collect::<Vec<_>>(),
            "spawn_context.tables_required",
        )?;
        let guids = selected_domain(
            b,
            "creature",
            "guid",
            1,
            0xffffffff,
            "spawn_entries.guid_domain",
        )?;
        let entries = selected_domain(b, "creature", "id", 0, 0xffffff, "spawn_context.entry")?
            .into_iter()
            .filter(|id| *id != 0)
            .collect::<BTreeSet<_>>();
        let mut selectors = Map::new();
        for (table, columns) in [
            ("creature_conditional_spawn", vec!["Guid"]),
            ("spawn_group_spawn", vec!["Guid"]),
            ("creature_linking", vec!["guid", "master_guid"]),
            ("creature_linking_template", vec!["entry", "master_entry"]),
            ("game_event_creature_data", vec!["guid"]),
        ] {
            let selected = if table == "creature_linking_template" {
                &entries
            } else {
                &guids
            };
            check(rows(b, table).len() <= 4096, "spawn_context.row_limit")?;
            check(
                rows(b, table).iter().all(|row| {
                    columns
                        .iter()
                        .any(|field| selected.contains(&int(&row[*field])))
                }),
                "spawn_context.unselected_row",
            )?;
            selectors.insert(
                table.to_string(),
                json!({"columns_or":columns,"ids":numbers(selected.iter().copied())}),
            );
        }
        let mut snapshot = identity_snapshot(b);
        snapshot["table_sha256"] = json!(["creature", "creature_spawn_entry"]
            .into_iter()
            .chain(context_tables.iter().copied())
            .map(|table| (table, digest(&b["tables"][table])))
            .collect::<BTreeMap<_, _>>());
        coverage(
            b,
            "spawn_context_capture",
            json!({"format":"benilla-classic-spawn-context-exclusion-v1","status":"complete","filter":FILTER,"selectors":selectors,"source_loader_order_proven":false,"assistance_traversal_proven":false,"snapshot":snapshot}),
            "spawn_context.coverage_mismatch",
        )?;
    }
    if present(b, "quest_relationship_capture") {
        relationships(b, p)?;
    }
    if present(b, "spell_numeric_capture") {
        numeric(b)?;
    }
    if present(b, "quest_giver_presentation_capture") {
        presentation(b)?;
    }
    if present(b, "quest_mechanics_capture") {
        quest_mechanics::validate(b)?;
    }
    if present(b, "participants_capture") {
        participants::validate(b)?;
    }
    if present(b, "skill_training_capture") {
        check(
            b["selection"].get("starter").is_some(),
            "skill_training.starter_required",
        )?;
        skill_training::validate(b)?;
    }
    Ok(())
}
fn projection(value: &Value) -> Result<Value> {
    let rs = array(value, "quest_relationships.row_limit")?;
    check(rs.len() <= 4096, "quest_relationships.row_limit")?;
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for row in rs {
        let obj = object(row, "quest_relationships.row_fields")?;
        check(
            REL_FIELDS.iter().all(|n| obj.contains_key(*n)),
            "quest_relationships.row_fields",
        )?;
        let mut out = Map::new();
        for name in REL_FIELDS {
            let signed = ["PrevQuestId", "NextQuestId", "ExclusiveGroup"].contains(&name);
            integer(
                &row[name],
                if signed {
                    -0x7fffffff
                } else if name == "entry" {
                    1
                } else {
                    0
                },
                0x7fffffff,
                &format!(
                    "quest_relationships.numeric_domain:{}:{name}:{}",
                    row["entry"], row[name]
                ),
            )?;
            out.insert(name.into(), row[name].clone());
        }
        check(
            seen.insert(int(&row["entry"])),
            "quest_relationships.duplicate_entry",
        )?;
        result.push(Value::Object(out));
    }
    result.sort_by_key(|r| int(&r["entry"]));
    Ok(json!(result))
}
fn relationships(b: &Value, p: &Profiles) -> Result<()> {
    let v = &b["quest_relationship_capture"];
    object(v, "quest_relationships.evidence_fields")?;
    let inbound = array(&v["inbound"], "quest_relationships.evidence_fields")?;
    for row in inbound {
        fields(row, &REL_FIELDS, "quest_relationships.row_fields")?;
    }
    for field in REL_FIELDS {
        let c = &p["quest_template"].columns[field];
        check(
            [
                "tinyint",
                "smallint",
                "mediumint",
                "int",
                "integer",
                "bigint",
                "bit",
            ]
            .contains(&c.kind.family.as_str())
                && !c.nullable
                && (!["PrevQuestId", "NextQuestId", "ExclusiveGroup"].contains(&field)
                    || !c.kind.unsigned),
            "quest_relationships.schema_domain",
        )?;
    }
    let own = projection(&b["tables"]["quest_template"])?;
    check(
        own.as_array().unwrap().len() <= 512,
        "quest_relationships.root_limit",
    )?;
    let roots: BTreeSet<_> = own
        .as_array()
        .unwrap()
        .iter()
        .map(|r| int(&r["entry"]))
        .collect();
    let inbound = projection(&v["inbound"])?;
    let matches = |r: &Value| {
        ["NextQuestId", "NextQuestInChain", "BreadcrumbForQuestId"]
            .iter()
            .any(|n| roots.contains(&int(&r[*n])))
            || roots.contains(&-int(&r["NextQuestId"]))
    };
    check(
        inbound.as_array().unwrap().iter().all(matches),
        "quest_relationships.unselected_row",
    )?;
    for row in inbound.as_array().unwrap() {
        if let Some(root) = own
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["entry"] == row["entry"])
        {
            check(root == row, "quest_relationships.conflicting_root")?;
        }
    }
    check(
        own.as_array()
            .unwrap()
            .iter()
            .filter(|r| matches(r))
            .all(|r| inbound.as_array().unwrap().contains(r)),
        "quest_relationships.missing_root_edge",
    )?;
    let mut signed = roots.clone();
    signed.extend(roots.iter().map(|n| -n));
    let mut snap = identity_snapshot(b);
    snap["quest_table_sha256"] = json!(digest(&b["tables"]["quest_template"]));
    coverage(
        b,
        "quest_relationship_capture",
        json!({"format":"benilla-classic-quest-relationship-exclusion-v1","status":"complete","scope":"raw_direct_inbound_exclusion_only","roots":numbers(roots.iter().copied()),"selectors":{"NextQuestId":numbers(signed),"NextQuestInChain":numbers(roots.iter().copied()),"BreadcrumbForQuestId":numbers(roots.iter().copied())},"row_limit":4096,"own":own,"inbound":inbound,"own_sha256":digest(&own),"inbound_sha256":digest(&inbound),"snapshot":snap}),
        "quest_relationships.coverage_mismatch",
    )
}
fn raw_columns(b: &Value, table: &str) -> Result<BTreeMap<String, Column>> {
    let meta = &b["schema"]["tables"][table];
    object(meta, &format!("spell_numeric.schema_missing:{table}"))?;
    let mut result = BTreeMap::new();
    for c in array(&meta["columns"], "schema.columns")? {
        let name = string(&c["name"], "schema.column_name")?;
        result.insert(
            name.to_string(),
            Column {
                kind: sql_type(string(&c["type"], "schema.column_type")?)?,
                nullable: c["nullable"]
                    .as_bool()
                    .ok_or_else(|| fail("schema.nullability"))?,
            },
        );
    }
    Ok(result)
}
fn normalized_projection(
    value: &Value,
    columns: &BTreeMap<String, Column>,
    limit: usize,
    key: &str,
    code: &str,
) -> Result<()> {
    let rs = array(value, code)?;
    check(rs.len() <= limit, code)?;
    let mut previous = None;
    let mut total = 0;
    for row in rs {
        let obj = object(row, code)?;
        check(obj.keys().eq(columns.keys()), code)?;
        for (n, c) in columns {
            check(cell(&row[n], c)? == row[n], code)?;
        }
        let n = integer(&row[key], i64::MIN as i128, u64::MAX as i128, code)?;
        check(previous.is_none_or(|p| p < n), code)?;
        previous = Some(n);
        let size = bounded_len(row, 65536, code)?;
        total += size;
        check(total <= 16 * 1024 * 1024, code)?;
    }
    Ok(())
}
fn numeric(b: &Value) -> Result<()> {
    let extension = &b["spell_numeric_capture"];
    object(extension, "spell_numeric.extension")?;
    for (table, key) in [("spell_template", "rows"), ("spell_facing", "facing")] {
        let available = raw_columns(b, table)?;
        let required = if table == "spell_template" {
            COMPILER_CONTRACT["numeric_columns"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>()
        } else {
            vec!["entry", "facingcasterflag"]
        };
        let mut cols = BTreeMap::new();
        for name in &required {
            let actual = available
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, c)| c)
                .ok_or_else(|| fail(&format!("spell_numeric.missing_column:{name}")))?;
            let f = &actual.kind.family;
            let valid = if names(&COMPILER_CONTRACT["numeric_floats"]).contains(name) {
                f == "float" && !actual.kind.unsigned
            } else if *name == "SpellFamilyFlags" {
                f == "bigint" && actual.kind.unsigned
            } else if *name == "facingcasterflag" {
                f == "tinyint" && !actual.kind.unsigned
            } else {
                f == "int"
                    && actual.kind.unsigned
                        != names(&COMPILER_CONTRACT["numeric_signed"]).contains(name)
            };
            check(
                valid && !actual.nullable,
                &format!("spell_numeric.column_type:{name}"),
            )?;
            cols.insert(name.to_string(), actual.clone());
        }
        let meta = &b["schema"]["tables"][table];
        let first = required[0];
        let actual_name = available
            .keys()
            .find(|n| n.eq_ignore_ascii_case(first))
            .unwrap();
        check(
            meta["primary_key"] == json!([actual_name]),
            "spell_numeric.primary_key",
        )?;
        if table == "spell_template" {
            for name in names(&COMPILER_CONTRACT["numeric_text"]) {
                let c = available
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case(name))
                    .map(|(_, c)| c)
                    .ok_or_else(|| fail(&format!("spell_numeric.missing_text_column:{name}")))?;
                check(c.kind.family == "text", "spell_numeric.text_type")?;
            }
        }
        check(
            array(&extension[key], "spell_numeric.row_limit")?.len() <= 512,
            "spell_numeric.row_limit",
        )?;
        normalized_projection(
            &extension[key],
            &cols,
            512,
            first,
            "spell_numeric.row_normalization",
        )?;
    }
    let expected = json!({"format":"benilla-classic-spell-numeric-selection-v1","selected_ids":numbers(ids(b,"spell_template","Id")),"fields":COMPILER_CONTRACT["numeric_columns"],"rows":extension["rows"],"facing":extension["facing"],"row_limit":512,"text_disposition":names(&COMPILER_CONTRACT["numeric_text"]).into_iter().map(|n|(n,"presentation_only_positive_mage_domain")).collect::<BTreeMap<_,_>>(),"snapshot":{"consistency":b["consistency"],"schema_sha256":b["schema_sha256"],"selection_sha256":digest(&b["selection"]),"db_version_sha256":digest(&b["db_version"]),"legacy_rows_sha256":digest(&b["tables"]["spell_template"]),"numeric_rows_sha256":digest(&extension["rows"]),"facing_rows_sha256":digest(&extension["facing"])}});
    coverage(
        b,
        "spell_numeric_capture",
        expected,
        "spell_numeric.coverage_mismatch",
    )?;
    let legacy: BTreeMap<_, _> = rows(b, "spell_template")
        .iter()
        .map(|r| (int(&r["Id"]), r))
        .collect();
    let rs = extension["rows"].as_array().unwrap();
    check(
        rs.iter().map(|r| int(&r["Id"])).eq(legacy.keys().copied()),
        "spell_numeric.exact_selected_rows",
    )?;
    for row in rs {
        check(
            legacy[&int(&row["Id"])]
                .as_object()
                .unwrap()
                .iter()
                .all(|(n, v)| row[n] == *v),
            "spell_numeric.legacy_row_mismatch",
        )?;
    }
    check(
        extension["facing"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| legacy.contains_key(&int(&r["entry"]))),
        "spell_numeric.unselected_facing",
    )
}
fn presentation(b: &Value) -> Result<()> {
    let value = &b["quest_giver_presentation_capture"];
    object(value, "quest_presentation.fields")?;
    let raw = object(&value["rows"], "quest_presentation.fields")?;
    let mut required = contract_names("presentation");
    required.extend(["broadcast_text", "broadcast_text_locale"]);
    let inventory = object(&b["schema"]["tables"], "schema.tables")?;
    check(
        required.iter().all(|t| inventory.contains_key(*t)),
        "quest_presentation.missing_tables",
    )?;
    for table in contract_names("presentation") {
        let meta = &inventory[table];
        check(
            meta["primary_key"] == CONTRACT["keys"][table],
            &format!("quest_presentation.primary_key:{table}"),
        )?;
        let observed: BTreeSet<_> = meta["columns"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["name"].as_str().unwrap().to_lowercase())
            .collect();
        check(
            observed == columns(table).iter().map(|n| n.to_lowercase()).collect(),
            &format!("quest_presentation.schema_columns:{table}"),
        )?;
    }
    let mut quest_cols = raw_columns(b, "quest_template")?;
    for field in ["IncompleteEmoteDelay", "CompleteEmoteDelay"] {
        let c = quest_cols
            .get(field)
            .ok_or_else(|| fail(&format!("quest_presentation.delay_schema:{field}")))?;
        check(
            !c.nullable
                && [
                    "tinyint",
                    "smallint",
                    "mediumint",
                    "int",
                    "integer",
                    "bigint",
                    "bit",
                ]
                .contains(&c.kind.family.as_str()),
            &format!("quest_presentation.delay_schema:{field}"),
        )?;
    }
    required.insert("quest_template");
    let profiles = schema(&b["schema"], &required)?;
    for (table, rs) in raw {
        check(
            required.contains(table.as_str()),
            "quest_presentation.table",
        )?;
        let rs_a = array(rs, "quest_presentation.row_shape_or_limit")?;
        check(
            rs_a.len() <= 4096 && rs_a.iter().all(Value::is_object),
            "quest_presentation.row_shape_or_limit",
        )?;
        if table == "quest_template" {
            quest_cols.retain(|n, _| {
                columns("quest_template").contains(&n.as_str())
                    || ["IncompleteEmoteDelay", "CompleteEmoteDelay"].contains(&n.as_str())
            });
            normalized_projection(
                rs,
                &quest_cols,
                512,
                "entry",
                "quest_presentation.row_shape_or_order",
            )?;
        } else {
            validate_rows(table, rs, &profiles[table], 4096)?;
        }
    }
    check(
        raw.keys().map(String::as_str).collect::<BTreeSet<_>>() == required,
        "quest_presentation.tables",
    )?;
    let entries: BTreeSet<_> = ids(b, "creature_questrelation", "id")
        .union(&ids(b, "creature_involvedrelation", "id"))
        .copied()
        .collect();
    let quests = ids(b, "quest_template", "entry");
    let guids: BTreeSet<_> = rows(b, "creature")
        .iter()
        .filter(|r| entries.contains(&int(&r["id"])))
        .map(|r| int(&r["guid"]))
        .collect();
    let scope = json!({"quests":numbers(quests.iter().copied()),"entries":numbers(entries.iter().copied()),"guids":numbers(guids.iter().copied())});
    let base: Vec<Value> = raw["quest_template"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            Value::Object(
                columns("quest_template")
                    .iter()
                    .map(|n| ((*n).into(), r[*n].clone()))
                    .collect(),
            )
        })
        .collect();
    check(
        json!(base) == b["tables"]["quest_template"],
        "quest_presentation.base_disagreement",
    )?;
    let mut expected: BTreeMap<&str, BTreeSet<i128>> = [
        ("questgiver_greeting", entries.clone()),
        ("locales_questgiver_greeting", entries),
        ("locales_quest", quests),
        ("npc_gossip", guids),
    ]
    .into_iter()
    .collect();
    let mut text_ids = BTreeSet::from([0xffffff]);
    text_ids.extend(
        raw["npc_gossip"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| int(&r["textid"]))
            .filter(|n| *n != 0),
    );
    for t in ["npc_text", "locales_npc_text", "npc_text_broadcast_text"] {
        expected.insert(t, text_ids.clone());
    }
    let broadcasts: BTreeSet<_> = raw["npc_text_broadcast_text"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| (0..8).map(move |i| int(&r[format!("BroadcastTextId{i}")])))
        .filter(|n| *n != 0)
        .collect();
    for t in ["broadcast_text", "broadcast_text_locale"] {
        expected.insert(t, broadcasts.clone());
    }
    for (table, keys) in &expected {
        check(
            raw[*table]
                .as_array()
                .unwrap()
                .iter()
                .all(|r| keys.contains(&int(&r[columns(table)[0]]))),
            &format!("quest_presentation.unselected_key:{table}"),
        )?;
    }
    bounded_len(
        &value["rows"],
        16 * 1024 * 1024,
        "quest_presentation.byte_limit",
    )?;
    let mut snap = identity_snapshot(b);
    snap["tables_sha256"] = json!(digest(&b["table_sha256"]));
    coverage(
        b,
        "quest_giver_presentation_capture",
        json!({"format":"benilla-classic-quest-giver-presentation-v1","status":"complete_raw_projection","selectors":scope,"lookup_keys":expected.into_iter().map(|(t,ids)|(t,numbers(ids))).collect::<BTreeMap<_,_>>(),"rows":value["rows"],"rows_sha256":digest(&value["rows"]),"loaded_relation_order":null,"effective_giver_selection":null,"registered_callbacks":null,"global_spell_area_and_item_effects":null,"session_locale_and_dialog_policy":null,"snapshot":snap}),
        "quest_presentation.binding_mismatch",
    )
}
pub(super) fn blockers(b: &Value) -> Result<Vec<Value>> {
    let mut out = Vec::new();
    if has(b, "creature_ai_scripts") {
        let (pos, negative, unsupported) = text_references(b)?;
        let selected: BTreeSet<_> = pos.iter().map(|r| int(&r["broadcast_id"])).collect();
        for table in ["broadcast_text", "broadcast_text_locale"] {
            if !has(b, table) {
                out.push(issue(
                    table,
                    "schema",
                    "event_text.table_missing_dependency_unknown",
                ));
            }
        }
        if has(b, "broadcast_text") {
            for id in selected.difference(&ids(b, "broadcast_text", "Id")) {
                out.push(issue("broadcast_text", id, "event_text.base_row_missing"));
            }
        }
        for (rs, reason) in [
            (negative, "event_text.negative_parameter_dependency_unknown"),
            (
                unsupported,
                "event_text.other_action_dependencies_not_captured",
            ),
        ] {
            for id in rs
                .iter()
                .map(|r| int(&r["event_id"]))
                .collect::<BTreeSet<_>>()
            {
                out.push(issue("creature_ai_scripts", id, reason));
            }
        }
    }
    if present(b, "loot_capture") {
        out.extend(loot::blockers(b)?);
    }
    if let Some(v) = b.get("quest_relationship_capture") {
        for root in v["roots"].as_array().unwrap() {
            for row in v["inbound"].as_array().unwrap() {
                for field in ["NextQuestId", "NextQuestInChain", "BreadcrumbForQuestId"] {
                    if row[field] == *root
                        || (field == "NextQuestId" && int(&row[field]) == -int(root))
                    {
                        out.push(issue(
                            "quest_template",
                            format!("{}:{}:{field}", root, row["entry"]),
                            "quest.inbound_relationship",
                        ));
                    }
                }
            }
        }
    }
    if present(b, "spawn_entry_capture") {
        for row in rows(b, "creature_spawn_entry") {
            out.push(issue(
                "creature_spawn_entry",
                format!("{}:{}", row["guid"], row["entry"]),
                "spawn.raw_entry_alternative_unimplemented",
            ));
        }
    }
    Ok(out)
}
