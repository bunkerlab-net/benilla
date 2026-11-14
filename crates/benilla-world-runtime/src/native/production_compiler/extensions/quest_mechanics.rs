// SPDX-License-Identifier: GPL-3.0-or-later
// classic_export/quest_mechanics.py: same-snapshot executable shapes and precise unresolved context.
use super::*;
use std::sync::LazyLock;
static CONTRACT: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("quest_mechanics_contract.json"))
        .expect("reviewed quest mechanics schema")
});
pub(in super::super) fn is_table(table: &str) -> bool {
    CONTRACT["schema"].get(table).is_some()
}
fn table_names() -> BTreeSet<&'static str> {
    CONTRACT["schema"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .chain([
            "npc_text",
            "npc_text_broadcast_text",
            "locales_npc_text",
            "broadcast_text",
            "broadcast_text_locale",
        ])
        .collect()
}
fn selector_scope(b: &Value) -> Value {
    let entries: BTreeSet<_> = ids(b, "creature_questrelation", "id")
        .union(&ids(b, "creature_involvedrelation", "id"))
        .copied()
        .collect();
    let scripts: BTreeSet<_> = rows(b, "quest_template")
        .iter()
        .map(|r| int(&r["CompleteScript"]))
        .filter(|n| *n != 0)
        .collect();
    let mut menus = BTreeSet::from([0]);
    menus.extend(
        rows(b, "creature_template")
            .iter()
            .filter(|r| entries.contains(&int(&r["Entry"])))
            .map(|r| int(&r["GossipMenuId"])),
    );
    json!({"scripts":numbers(scripts),"menus":numbers(menus),"entries":numbers(entries)})
}
pub(super) fn validate(bundle: &Value) -> Result<()> {
    let v = &bundle["quest_mechanics_capture"];
    object(v, "quest_mechanics.fields")?;
    let raw = object(&v["rows"], "quest_mechanics.fields")?;
    let required = table_names();
    let inventory = object(&bundle["schema"]["tables"], "schema.tables")?;
    check(
        required.iter().all(|n| inventory.contains_key(*n)),
        "quest_mechanics.missing_tables",
    )?;
    for (table, expected) in CONTRACT["schema"].as_object().unwrap() {
        let meta = &inventory[table];
        check(
            meta["primary_key"] == CONTRACT["keys"][table],
            &format!("quest_mechanics.primary_key:{table}"),
        )?;
        let actual = array(&meta["columns"], "schema.columns")?;
        let expected = expected.as_array().unwrap();
        check(
            actual.len() == expected.len(),
            &format!("quest_mechanics.columns:{table}"),
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
                &format!(
                    "quest_mechanics.column:{table}:{}",
                    spec[0].as_str().unwrap()
                ),
            )?;
        }
    }
    let profiles = schema(&bundle["schema"], &required)?;
    for (table, rs) in raw {
        check(required.contains(table.as_str()), "quest_mechanics.table")?;
        if CONTRACT["schema"].get(table).is_some() {
            let rs = array(rs, "quest_mechanics.row_limit")?;
            check(rs.len() <= 4096, "quest_mechanics.row_limit")?;
            let columns = raw_columns(bundle, table)?;
            let fields = names(&CONTRACT["columns"][table]);
            let keys = CONTRACT["keys"][table].as_array().unwrap();
            let mut seen = BTreeSet::new();
            let mut previous: Option<Vec<u8>> = None;
            for row in rs {
                let row_obj = object(row, "quest_mechanics.row_fields")?;
                check(
                    row_obj.len() == fields.len()
                        && fields.iter().all(|n| row_obj.contains_key(*n)),
                    "quest_mechanics.row_fields",
                )?;
                for (name, value) in row_obj {
                    check(
                        cell(value, &columns[name])? == *value,
                        "quest_mechanics.row_order_or_shape",
                    )?;
                }
                if !keys.is_empty() {
                    let key: Vec<_> = keys
                        .iter()
                        .map(|n| row[n.as_str().unwrap()].clone())
                        .collect();
                    check(
                        seen.insert(canonical(&json!(key))),
                        "quest_mechanics.duplicate_key",
                    )?;
                }
                let encoded = canonical(row);
                check(encoded.len() <= 65536, "quest_mechanics.row_bytes")?;
                check(
                    previous.as_ref().is_none_or(|p| p <= &encoded),
                    "quest_mechanics.row_order_or_shape",
                )?;
                previous = Some(encoded);
            }
            bounded_len(&rs, 16 * 1024 * 1024, "quest_mechanics.table_bytes")?;
        } else {
            let a = array(rs, "quest_mechanics.row_shape")?;
            check(a.iter().all(Value::is_object), "quest_mechanics.row_shape")?;
            validate_rows(table, rs, &profiles[table], 4096).map_err(|e| {
                if e.id == "bundle.table_hash_or_order" {
                    fail("quest_mechanics.row_order_or_shape")
                } else {
                    e
                }
            })?;
        }
    }
    check(
        raw.keys().map(String::as_str).collect::<BTreeSet<_>>() == required,
        "quest_mechanics.tables",
    )?;
    let scope = selector_scope(bundle);
    let conditions: BTreeSet<_> = [
        "dbscripts_on_quest_end",
        "gossip_menu",
        "gossip_menu_option",
    ]
    .iter()
    .flat_map(|t| raw[*t].as_array().unwrap())
    .map(|r| int(&r["condition_id"]))
    .filter(|n| *n != 0)
    .collect();
    let text_ids: BTreeSet<_> = raw["gossip_menu"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| int(&r["text_id"]))
        .filter(|n| *n != 0)
        .collect();
    let mut broadcasts: BTreeSet<_> = raw["dbscripts_on_quest_end"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| int(&r["command"]) == 0 && int(&r["dataint"]) > 0)
        .map(|r| int(&r["dataint"]))
        .collect();
    broadcasts.extend(
        raw["npc_text_broadcast_text"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|r| (0..8).map(move |i| int(&r[format!("BroadcastTextId{i}")])))
            .filter(|n| *n != 0),
    );
    broadcasts.extend(
        raw["gossip_menu_option"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|r| {
                [
                    int(&r["option_broadcast_text"]),
                    int(&r["box_broadcast_text"]),
                ]
            })
            .filter(|n| *n > 0),
    );
    let mut lookups = json!({"dbscripts_on_quest_end":scope["scripts"],"gossip_menu":scope["menus"],"gossip_menu_option":scope["menus"],"conditions":numbers(conditions)});
    for t in ["npc_text", "npc_text_broadcast_text", "locales_npc_text"] {
        lookups[t] = numbers(text_ids.iter().copied());
    }
    for t in ["broadcast_text", "broadcast_text_locale"] {
        lookups[t] = numbers(broadcasts.iter().copied());
    }
    for (table, selected) in lookups.as_object().unwrap() {
        let key = match table.as_str() {
            "dbscripts_on_quest_end" => "id",
            "gossip_menu" => "entry",
            "gossip_menu_option" => "menu_id",
            "conditions" => "condition_entry",
            "npc_text" => "ID",
            "npc_text_broadcast_text" | "broadcast_text" | "broadcast_text_locale" => "Id",
            "locales_npc_text" => "entry",
            _ => unreachable!(),
        };
        let selected: BTreeSet<_> = selected.as_array().unwrap().iter().map(int).collect();
        check(
            raw[table]
                .as_array()
                .unwrap()
                .iter()
                .all(|r| selected.contains(&int(&r[key]))),
            &format!("quest_mechanics.unselected_key:{table}"),
        )?;
    }
    bounded_len(&v["rows"], 16 * 1024 * 1024, "quest_mechanics.byte_limit")?;
    let mut snap = identity_snapshot(bundle);
    snap["tables_sha256"] = json!(digest(&bundle["table_sha256"]));
    coverage(
        bundle,
        "quest_mechanics_capture",
        json!({"format":"benilla-classic-quest-mechanics-v1","selectors":scope,"lookup_keys":lookups,"rows":v["rows"],"rows_sha256":digest(&v["rows"]),"loaded_script_order":null,"snapshot":snap}),
        "quest_mechanics.binding_mismatch",
    )
}
fn refuse(refusals: &mut Vec<Value>, field: &str, target: Value, reason: &str) {
    refusals.push(json!({"quest":0,"field":field,"target":target,"reason":reason}));
}
pub(in super::super) fn supported(bundle: &Value) -> Value {
    let Some(value) = bundle.get("quest_mechanics_capture") else {
        return json!({"status":"unknown","scripts":[],"givers":[],"refusals":[]});
    };
    let raw = &value["rows"];
    let mut refusals = Vec::new();
    let broadcasts: BTreeMap<_, _> = raw["broadcast_text"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (int(&r["Id"]), r))
        .collect();
    let mut scripts = BTreeSet::new();
    for script in value["selectors"]["scripts"].as_array().unwrap() {
        let rows: Vec<_> = raw["dbscripts_on_quest_end"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["id"] == *script)
            .collect();
        let start = refusals.len();
        if rows.is_empty() {
            refuse(
                &mut refusals,
                "quest_end_scripts.id",
                script.clone(),
                "missing_script",
            );
        }
        for row in rows {
            let command = int(&row["command"]);
            if ![0, 1].contains(&command) {
                refuse(
                    &mut refusals,
                    "quest_end_scripts.command",
                    row["command"].clone(),
                    "unsupported_command",
                );
                continue;
            }
            if int(&row["delay"]) == 0 {
                refuse(
                    &mut refusals,
                    "quest_end_scripts.delay",
                    json!(0),
                    "immediate_script_not_supported",
                );
            }
            for field in [
                "buddy_entry",
                "search_radius",
                "data_flags",
                "condition_id",
                "datalong2",
                "datalong3",
                "dataint2",
                "dataint3",
                "dataint4",
            ] {
                if int(&row[field]) != 0 {
                    refuse(
                        &mut refusals,
                        &format!("quest_end_scripts.{field}"),
                        row[field].clone(),
                        "unsupported_path",
                    );
                }
            }
            for field in ["datafloat", "x", "y", "z", "o", "speed"] {
                if row[field] != json!({"decimal":"0"}) {
                    refuse(
                        &mut refusals,
                        &format!("quest_end_scripts.{field}"),
                        json!(0),
                        "unsupported_path",
                    );
                }
            }
            if command == 0 {
                let text = broadcasts.get(&int(&row["dataint"]));
                if int(&row["datalong"]) != 0 || int(&row["dataint"]) <= 0 {
                    refuse(
                        &mut refusals,
                        "quest_end_scripts.dataint",
                        row["dataint"].clone(),
                        "unsupported_talk_shape",
                    );
                } else if let Some(text) = text {
                    if [
                        "ChatTypeID",
                        "LanguageID",
                        "SoundEntriesID1",
                        "SoundEntriesID2",
                    ]
                    .into_iter()
                    .map(String::from)
                    .chain((1..=3).map(|i| format!("EmoteID{i}")))
                    .chain((1..=3).map(|i| format!("EmoteDelay{i}")))
                    .any(|f| int(&text[f]) != 0)
                    {
                        refuse(
                            &mut refusals,
                            "broadcast_text",
                            row["dataint"].clone(),
                            "unsupported_speech_effects",
                        );
                    }
                } else {
                    refuse(
                        &mut refusals,
                        "broadcast_text",
                        row["dataint"].clone(),
                        "unknown_lookup",
                    );
                }
            } else if int(&row["dataint"]) != 0 {
                refuse(
                    &mut refusals,
                    "quest_end_scripts.dataint",
                    row["dataint"].clone(),
                    "unsupported_random_emote",
                );
            }
        }
        if refusals.len() == start {
            scripts.insert(int(script));
        }
    }
    let conditions: BTreeMap<_, _> = raw["conditions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (int(&r["condition_entry"]), r))
        .collect();
    let quest_ids = ids(bundle, "quest_template", "entry");
    for row in raw["conditions"].as_array().unwrap() {
        let kind = int(&row["type"]);
        if ![0, 8, 9, 22].contains(&kind)
            || int(&row["flags"]) & !1 != 0
            || int(&row["value3"]) != 0
            || int(&row["value4"]) != 0
            || int(&row["value2"]) > 2
            || kind != 9 && int(&row["value2"]) != 0
            || kind == 0 && int(&row["value1"]) != 0
        {
            refuse(
                &mut refusals,
                "conditions.type",
                row["type"].clone(),
                "unsupported_condition",
            );
        } else if kind != 0 && !quest_ids.contains(&int(&row["value1"])) {
            refuse(
                &mut refusals,
                "conditions.value1",
                row["value1"].clone(),
                "uncaptured_reference",
            );
        }
    }
    for t in ["gossip_menu", "gossip_menu_option"] {
        for row in raw[t].as_array().unwrap() {
            let id = int(&row["condition_id"]);
            if id != 0 && !conditions.contains_key(&id) {
                refuse(
                    &mut refusals,
                    "conditions.condition_entry",
                    row["condition_id"].clone(),
                    "unknown_lookup",
                );
            }
        }
    }
    let entries: BTreeSet<_> = value["selectors"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(int)
        .collect();
    let texts: BTreeSet<_> = raw["npc_text"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| int(&r["ID"]))
        .chain(
            raw["npc_text_broadcast_text"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| int(&r["Id"])),
        )
        .collect();
    let mut givers = Vec::new();
    for giver in rows(bundle, "creature_template") {
        if !entries.contains(&int(&giver["Entry"])) {
            continue;
        }
        let start = refusals.len();
        let menu = int(&giver["GossipMenuId"]);
        if menu == 0 && int(&giver["NpcFlags"]) & 1 != 0 {
            refuse(
                &mut refusals,
                "npc_gossip",
                giver["Entry"].clone(),
                "guid_gossip_text_not_supported",
            );
        }
        let menus: Vec<_> = raw["gossip_menu"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| int(&r["entry"]) == menu)
            .collect();
        if menu != 0 && menus.is_empty() {
            refuse(
                &mut refusals,
                "gossip_menu",
                giver["GossipMenuId"].clone(),
                "unknown_lookup",
            );
        }
        if menu != 0 && !menus.is_empty() && !menus.iter().any(|row| int(&row["condition_id"]) == 0)
        {
            refuse(
                &mut refusals,
                "gossip_menu",
                giver["GossipMenuId"].clone(),
                "unconditional_text_required",
            );
        }
        let mut seen_conditions = BTreeSet::new();
        for row in &menus {
            if !seen_conditions.insert(int(&row["condition_id"])) {
                refuse(
                    &mut refusals,
                    "gossip_menu.condition_id",
                    row["condition_id"].clone(),
                    "ambiguous_loaded_order",
                );
            }
        }
        for row in menus {
            if int(&row["script_id"]) != 0 {
                refuse(
                    &mut refusals,
                    "gossip_menu.script_id",
                    row["script_id"].clone(),
                    "unsupported_path",
                );
            }
            if !texts.contains(&int(&row["text_id"])) {
                refuse(
                    &mut refusals,
                    "npc_text",
                    row["text_id"].clone(),
                    "unknown_lookup",
                );
            }
        }
        let mut options: Vec<_> = raw["gossip_menu_option"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| int(&r["menu_id"]) == menu)
            .collect();
        if options.is_empty() {
            options = raw["gossip_menu_option"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| int(&r["menu_id"]) == 0)
                .collect();
        }
        for row in options {
            if int(&row["condition_id"]) != 0
                && int(&row["option_id"]) == 2
                && int(&giver["NpcFlags"]) & 1 == 0
            {
                refuse(
                    &mut refusals,
                    "gossip_menu_option.condition_id",
                    row["condition_id"].clone(),
                    "conditional_direct_quest_menu_not_supported",
                );
            }
            if int(&row["npc_option_npcflag"]) & int(&giver["NpcFlags"]) == 0 {
                continue;
            }
            if ![2, 99].contains(&int(&row["option_id"])) {
                refuse(
                    &mut refusals,
                    "gossip_menu_option.option_id",
                    row["option_id"].clone(),
                    "unsupported_option",
                );
            }
            for field in [
                "action_menu_id",
                "action_poi_id",
                "action_script_id",
                "box_coded",
                "box_money",
                "box_broadcast_text",
            ] {
                if int(&row[field]) != 0 {
                    refuse(
                        &mut refusals,
                        &format!("gossip_menu_option.{field}"),
                        row[field].clone(),
                        "unsupported_path",
                    );
                }
            }
            if row["box_text"].as_str().is_some_and(|s| !s.is_empty()) {
                refuse(
                    &mut refusals,
                    "gossip_menu_option.box_text",
                    row["id"].clone(),
                    "unsupported_path",
                );
            }
        }
        if refusals.len() == start {
            givers.push(giver["Entry"].clone());
        }
    }
    json!({"status":if refusals.is_empty(){"resolved"}else{"blocked"},"scripts":numbers(scripts),"givers":givers,"refusals":refusals,"loaded_script_order":null,"source_sha256":digest(value)})
}
