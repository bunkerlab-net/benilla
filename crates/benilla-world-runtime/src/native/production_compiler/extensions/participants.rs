// SPDX-License-Identifier: GPL-3.0-or-later
// Same conservative selected-root/whole-loader contract as classic_export::participants.
use super::*;
use std::sync::LazyLock;
static CONTRACT: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("participants_contract.json"))
        .expect("reviewed participant schema")
});
pub(in super::super) fn is_table(table: &str) -> bool {
    CONTRACT["schema"].get(table).is_some()
}
fn names_order() -> impl Iterator<Item = &'static str> {
    [
        "gameobject",
        "gameobject_spawn_entry",
        "transports",
        "gameobject_template",
        "gameobject_addon",
        "game_event_gameobject",
        "pool_gameobject",
        "pool_gameobject_template",
        "creature_ai_summons",
    ]
    .into_iter()
    .chain(
        CONTRACT["schema"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .filter(|n| n.starts_with("dbscripts_")),
    )
}
fn query(table: &str) -> Value {
    let columns = CONTRACT["columns"][table]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| format!("`{}`", n.as_str().unwrap()))
        .collect::<Vec<_>>()
        .join(",");
    let order = if table.starts_with("dbscripts_") {
        " ORDER BY `priority`"
    } else {
        ""
    };
    json!([format!("SELECT {columns} FROM `{table}`{order}"), []])
}
fn keep(table: &str, row: &Value, raw: &Value, selection: &Selection) -> bool {
    match table {
        "gameobject" => {
            int(&row["map"]) == selection.map
                && ["position_x", "position_y", "position_z"]
                    .iter()
                    .enumerate()
                    .all(|(i, axis)| inside(&row[*axis], selection.min[i], selection.max[i]))
        }
        "gameobject_template" | "pool_gameobject_template" => {
            let entry = &row[if table == "gameobject_template" {
                "entry"
            } else {
                "id"
            }];
            rows_value(raw, "gameobject")
                .iter()
                .any(|r| r["id"] == *entry)
                || rows_value(raw, "gameobject_spawn_entry")
                    .iter()
                    .any(|r| r["entry"] == *entry)
                || rows_value(raw, "transports")
                    .iter()
                    .any(|r| r["entry"] == *entry)
        }
        "gameobject_addon"
        | "gameobject_spawn_entry"
        | "game_event_gameobject"
        | "pool_gameobject" => rows_value(raw, "gameobject")
            .iter()
            .any(|r| r["guid"] == row["guid"]),
        t if t.starts_with("dbscripts_") => ![0, 1].contains(&int(&row["command"])),
        _ => true,
    }
}
fn rows_value<'a>(raw: &'a Value, table: &str) -> &'a [Value] {
    raw[table].as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn roots(b: &Value) -> Value {
    json!({"selection":b["selection"], "creature_guids":rows(b,"creature").iter().map(|r| &r["guid"]).collect::<Vec<_>>(),
        "creature_entries":numbers(ids(b,"creature_template","Entry")),"quest_ids":numbers(ids(b,"quest_template","entry")),
        "spell_ids":numbers(ids(b,"spell_template","Id"))})
}
pub(super) fn validate(b: &Value) -> Result<()> {
    let value = &b["participants_capture"];
    object(value, "participants.fields")?;
    let raw = object(&value["rows"], "participants.fields")?;
    let observations = object(&value["observations"], "participants.fields")?;
    let inventory = &b["schema"]["tables"];
    let tables: BTreeSet<_> = names_order().collect();
    let available: BTreeSet<_> = tables
        .iter()
        .copied()
        .filter(|t| inventory.get(*t).is_some())
        .collect();
    check(
        observations
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            == tables,
        "participants.tables_required",
    )?;
    check(
        raw.keys().map(String::as_str).collect::<BTreeSet<_>>() == available,
        "participants.tables_required",
    )?;
    let selection = Selection::parse(&b["selection"])?;
    for table in names_order() {
        let obs = &observations[table];
        if !available.contains(table) {
            check(
                *obs == json!({"status":"unknown"}),
                "participants.absent_table_unknown",
            )?;
            continue;
        }
        let meta = &inventory[table];
        check(
            meta["primary_key"] == CONTRACT["keys"][table],
            &format!("participants.primary_key:{table}"),
        )?;
        let actual = array(&meta["columns"], "schema.columns")?;
        let expected = CONTRACT["schema"][table].as_array().unwrap();
        check(
            actual.len() == expected.len(),
            &format!("participants.columns:{table}"),
        )?;
        for (ordinal, (col, spec)) in actual.iter().zip(expected).enumerate() {
            let observed = sql_type(col["type"].as_str().unwrap())?;
            let wanted = sql_type(spec[1].as_str().unwrap())?;
            let equal =
                if ["int", "mediumint", "smallint", "tinyint"].contains(&wanted.family.as_str()) {
                    observed.family == wanted.family && observed.unsigned == wanted.unsigned
                } else {
                    observed == wanted
                };
            check(
                col["name"] == spec[0]
                    && int(&col["ordinal"]) == ordinal as i128 + 1
                    && col["nullable"] == spec[2]
                    && equal,
                &format!("participants.column:{table}:{}", spec[0].as_str().unwrap()),
            )?;
        }
        let rs = array(&raw[table], "participants.selected_row_limit")?;
        check(rs.len() <= 4096, "participants.selected_row_limit")?;
        let columns = raw_columns(b, table)?;
        let fields_set = names(&CONTRACT["columns"][table]);
        let keys = CONTRACT["keys"][table].as_array().unwrap();
        let mut seen = BTreeSet::new();
        let mut previous = None;
        for row in rs {
            let row_obj = object(row, "participants.row_fields")?;
            check(
                row_obj.len() == fields_set.len()
                    && fields_set.iter().all(|n| row_obj.contains_key(*n)),
                "participants.row_fields",
            )?;
            for (name, v) in row_obj {
                check(
                    cell(v, &columns[name])? == *v,
                    "participants.row_order_or_shape",
                )?;
            }
            let encoded = canonical(row);
            check(
                previous.as_ref().is_none_or(|p| p <= &encoded),
                "participants.row_order_or_shape",
            )?;
            previous = Some(encoded);
            if !keys.is_empty() {
                check(
                    seen.insert(canonical(&json!(keys
                        .iter()
                        .map(|k| &row[k.as_str().unwrap()])
                        .collect::<Vec<_>>()))),
                    "participants.duplicate_key",
                )?;
            }
            check(
                keep(table, row, &value["rows"], &selection),
                "participants.unselected_row",
            )?;
        }
        fields(
            obs,
            &[
                "status",
                "query_sha256",
                "result_row_count",
                "ordered_result_sha256",
                "selected_positions",
                "selected_sha256",
                "selected_map_rows",
            ],
            "participants.observation_fields",
        )?;
        check(
            obs["status"] == "observed" && obs["query_sha256"] == digest(&query(table)),
            "participants.loader_query",
        )?;
        let count = integer(
            &obs["result_row_count"],
            0,
            2_000_000,
            "participants.loader_count",
        )?;
        let fingerprint = string(
            &obs["ordered_result_sha256"],
            "participants.loader_fingerprint",
        )?;
        check(
            fingerprint.len() == 64
                && fingerprint != "0".repeat(64)
                && fingerprint
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
            "participants.loader_fingerprint",
        )?;
        let positions = array(&obs["selected_positions"], "participants.loader_positions")?;
        check(positions.len() == rs.len(), "participants.loader_positions")?;
        let mut previous = -1;
        for p in positions {
            let position = integer(p, 0, (count - 1).max(0), "participants.loader_position")?;
            check(
                position > previous && position < count,
                "participants.loader_positions",
            )?;
            previous = position;
        }
        check(
            obs["selected_sha256"] == digest(&raw[table]),
            "participants.projection_binding",
        )?;
        if table == "gameobject" {
            integer(
                &obs["selected_map_rows"],
                rs.len() as i128,
                count,
                "participants.map_count",
            )?;
        } else {
            check(obs["selected_map_rows"].is_null(), "participants.map_count")?;
        }
    }
    bounded_len(&value["rows"], 16 * 1024 * 1024, "participants.byte_limit")?;
    let mut snapshot = json!({
        "core_revision": b["core_revision"],
        "database_revision": b["database_revision"],
        "schema_sha256": b["schema_sha256"],
        "consistency": b["consistency"],
        "db_version_sha256": digest(&b["db_version"]),
    });
    snapshot["tables_sha256"] = json!(digest(&b["table_sha256"]));
    snapshot["rows_sha256"] = json!(digest(&value["rows"]));
    snapshot["observations_sha256"] = json!(digest(&value["observations"]));
    coverage(
        b,
        "participants_capture",
        json!({"format":"benilla-classic-participants-v1", "scope":"conservative_candidates_no_runtime_or_exterior_exclusion",
        "roots":roots(b), "rows":value["rows"], "observations":value["observations"], "snapshot":snapshot}),
        "participants.coverage_mismatch",
    )
}

pub(in super::super) fn supported(source: &Value) -> Value {
    let value = source.get("participants_capture");
    let mut refusals = Vec::new();
    let mut inventory = Vec::new();
    let mut refuse = |table: &str, key: Value, reason: &str, extra: Value| {
        let mut v = json!({"table":table,"key":key,"reason":format!("participants.{reason}")});
        v.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        refusals.push(v);
    };
    if let Some(value) = value {
        let raw = &value["rows"];
        for table in CONTRACT["schema"].as_object().unwrap().keys() {
            if value["observations"][table]["status"] == "unknown" {
                refuse(table, json!([]), "table_unknown", json!({}));
            }
        }
        let templates: BTreeMap<_, _> = rows_value(raw, "gameobject_template")
            .iter()
            .map(|r| (int(&r["entry"]), r))
            .collect();
        const TYPES: [&str; 31] = [
            "door",
            "button",
            "questgiver",
            "chest",
            "binder",
            "generic",
            "trap",
            "chair",
            "spell_focus",
            "text",
            "goober",
            "transport",
            "area_damage",
            "camera",
            "mapobject",
            "mo_transport",
            "duel_flag",
            "fishing_node",
            "summoning_ritual",
            "mailbox",
            "auctionhouse",
            "guardpost",
            "spellcaster",
            "meetingstone",
            "flagstand",
            "fishing_hole",
            "flagdrop",
            "mini_game",
            "unknown_28",
            "capture_point",
            "aura_generator",
        ];
        for spawn in rows_value(raw, "gameobject") {
            let alternatives: BTreeSet<_> = std::iter::once(int(&spawn["id"]))
                .chain(
                    rows_value(raw, "gameobject_spawn_entry")
                        .iter()
                        .filter(|r| r["guid"] == spawn["guid"])
                        .map(|r| int(&r["entry"])),
                )
                .filter(|n| *n != 0)
                .collect();
            for entry in alternatives {
                let Some(template) = templates.get(&entry) else {
                    refuse(
                        "gameobject_template",
                        json!([entry as i64]),
                        "template_unknown",
                        json!({}),
                    );
                    continue;
                };
                let kind = int(&template["type"]);
                let name = usize::try_from(kind)
                    .ok()
                    .and_then(|k| TYPES.get(k))
                    .copied()
                    .unwrap_or("unknown");
                let field = match kind {
                    5 => Some("data2"),
                    6 => Some("data8"),
                    8 => Some("data3"),
                    30 => Some("data6"),
                    _ => None,
                };
                let server_only = field.is_some_and(|f| int(&template[f]) != 0);
                let collision = if server_only || kind == 6 {
                    "disabled"
                } else if kind == 0 {
                    "door_state_dependent"
                } else {
                    "enabled_when_spawned"
                };
                inventory.push(json!({"guid":spawn["guid"],"entry":entry as i64,"type":template["type"],"type_name":name,"display_id":template["displayId"],"collision_policy":collision,"collision_model":"unproved","required_port":format!("5.6c.{name}")}));
                refuse(
                    "gameobject",
                    json!([spawn["guid"], entry as i64]),
                    &format!("unsupported_{name}"),
                    json!({}),
                );
                if template["ScriptName"] != "" {
                    refuse(
                        "gameobject_template",
                        json!([entry as i64]),
                        "runtime_script_required",
                        json!({}),
                    );
                }
            }
            if int(&spawn["id"]) == 0
                || rows_value(raw, "gameobject_spawn_entry")
                    .iter()
                    .any(|r| r["guid"] == spawn["guid"])
            {
                refuse(
                    "gameobject",
                    json!([spawn["guid"]]),
                    "spawn_alternatives_required",
                    json!({}),
                );
            }
        }
        let obs = &value["observations"]["gameobject"];
        if obs["status"] == "observed"
            && int(&obs["selected_map_rows"]) > rows_value(raw, "gameobject").len() as i128
        {
            refuse(
                "gameobject",
                json!([]),
                "exterior_bounds_coverage_required",
                json!({"count":(int(&obs["selected_map_rows"]) - rows_value(raw,"gameobject").len() as i128) as i64}),
            );
        }
        for table in [
            "game_event_gameobject",
            "pool_gameobject",
            "pool_gameobject_template",
        ] {
            for row in rows_value(raw, table) {
                let key = if table == "pool_gameobject_template" {
                    "id"
                } else {
                    "guid"
                };
                refuse(
                    table,
                    json!([row[key]]),
                    if table == "game_event_gameobject" {
                        "event_membership_required"
                    } else {
                        "pool_membership_required"
                    },
                    json!({}),
                );
            }
        }
        for row in rows_value(raw, "transports") {
            let template = templates.get(&int(&row["entry"]));
            if template.is_none() {
                refuse(
                    "gameobject_template",
                    json!([row["entry"]]),
                    "template_unknown",
                    json!({}),
                );
            }
            refuse(
                "transports",
                json!([row["entry"]]),
                "transport_path_coverage_required",
                json!({"taxi_path_id":template.map(|t| &t["data0"])}),
            );
        }
        for table in CONTRACT["schema"]
            .as_object()
            .unwrap()
            .keys()
            .filter(|t| t.starts_with("dbscripts_"))
        {
            let mut counts = BTreeMap::new();
            for r in rows_value(raw, table) {
                *counts.entry(int(&r["command"])).or_insert(0usize) += 1;
            }
            for (command, count) in counts {
                refuse(
                    table,
                    json!([command as i64]),
                    "runtime_db_script_command_required",
                    json!({"count":count}),
                );
            }
        }
    } else {
        refuse("participants", json!([]), "capture_required", json!({}));
    }
    for ext in [
        "event_ai_capture",
        "spell_script_capture",
        "initial_hook_capture",
    ] {
        if source.get(ext).is_none() {
            refuse(ext, json!([]), "creation_hook_coverage_required", json!({}));
        }
    }
    for (table, key) in [
        ("creature_template", "Entry"),
        ("spell_scripts", "Id"),
        ("item_template", "entry"),
    ] {
        for row in rows(source, table) {
            if row["ScriptName"] != "" {
                refuse(
                    table,
                    json!([row[key]]),
                    "runtime_script_required",
                    json!({}),
                );
            }
        }
    }
    for table in ["spell_area", "spell_proc_event"] {
        if !rows(source, table).is_empty() {
            refuse(table, json!([]), "runtime_initial_hook_required", json!({}));
        }
    }
    for row in rows(source, "creature_ai_scripts") {
        for slot in 1..=3 {
            if ![0, 1, 4, 5, 9, 10, 44, 54].contains(&int(&row[format!("action{slot}_type")])) {
                refuse(
                    "creature_ai_scripts",
                    json!([row["id"], slot]),
                    "runtime_event_ai_creation_path_required",
                    json!({}),
                );
            }
        }
    }
    for row in rows(source, "spell_template") {
        for slot in 1..=3 {
            if [
                3, 28, 41, 42, 50, 56, 61, 64, 72, 73, 74, 76, 77, 85, 87, 88, 89, 90, 97, 104,
                105, 106, 107, 109, 112,
            ]
            .contains(&int(&row[format!("Effect{slot}")]))
            {
                refuse(
                    "spell_template",
                    json!([row["Id"], slot]),
                    "runtime_spell_creation_path_required",
                    json!({}),
                );
            }
        }
    }
    let empty = value.is_some() && refusals.is_empty();
    json!({"format":"cmangos-8ec338a1-empty-participants-plan-v1","status":if value.is_none() {"unknown"} else if empty {"proved_empty"} else {"blocked"},
        "capture_sha256":value.map(digest),"controller_binding":"closed_no_native_capability","empty_participant_obligation_discharged":empty,"production_admission":false,
        "inventory":inventory,"refusals":refusals})
}
