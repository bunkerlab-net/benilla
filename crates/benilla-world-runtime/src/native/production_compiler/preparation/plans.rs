// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
mod creature;
mod creature_combat;
pub(in super::super) mod item;
mod loot;
mod non_body_loot;
pub(in super::super) mod spawn;
pub(in super::super) mod spell_execution;
pub(super) use creature::creatures;
pub(super) use loot::{loot, loot_discharges, quest_loot};
const QUEST_ADAPTER: &str =
    "quest_lifecycle::CapturedOrdinaryQuests + captured_quests::CapturedQuestPresentation";
pub(super) fn outfit(grants: &[Value], items: &BTreeMap<i128, &Value>) -> (Value, Vec<Value>) {
    let mut steps = Vec::new();
    let mut reasons = Vec::new();
    let mut counts: BTreeMap<i128, i128> = BTreeMap::new();
    for grant in grants {
        let id = int(&grant["item_id"]);
        let Some(item) = items.get(&id) else {
            reasons.push(json!({"item_id":num(id),"reason":"item.placement_row_missing"}));
            continue;
        };
        let candidate_slots = item::equip_slots(item);
        let supported = candidate_slots.is_some();
        let slots = candidate_slots.unwrap_or_default();
        let reason = if !supported {
            Some("item.placement_type_unsupported")
        } else if integer(&grant["count"], 1, 10000, "domain").is_err()
            || !(1..=10000).contains(&int(&item["stackable"]))
        {
            Some("item.placement_stack_domain")
        } else if ["RequiredSkill", "RequiredSkillRank", "requiredspell"]
            .iter()
            .any(|n| nonzero(&item[*n]))
        {
            Some("item.placement_required_skill_spell_unsupported")
        } else if int(&item["Flags"]) & !0x40 != 0
            || [
                "BagFamily",
                "ContainerSlots",
                "itemset",
                "RandomProperty",
                "Duration",
                "ScriptName",
                "block",
                "requiredhonorrank",
                "RequiredReputationFaction",
            ]
            .iter()
            .any(|n| nonzero(&item[*n]))
        {
            Some("item.placement_property_unsupported")
        } else if !slots.is_empty()
            && (1..=5).any(|i| {
                nonzero(&item[format!("spellid_{i}")])
                    && nonzero(&item[format!("spelltrigger_{i}")])
            })
        {
            Some("item.placement_equip_spell_unsupported")
        } else {
            None
        };
        if let Some(reason) = reason {
            reasons.push(json!({"item_id":num(id),"reason":reason}));
            continue;
        }
        let count = counts.entry(id).or_default();
        *count += int(&grant["count"]);
        if int(&item["maxcount"]) > 0 && *count > int(&item["maxcount"]) {
            reasons.push(json!({"item_id":num(id),"reason":"item.placement_unique_count"}));
        }
        let mut step = grant.clone();
        step["operation"] = json!("equip_one_then_store_remainder");
        step["candidate_slots"] = json!(slots);
        step["stackable"] = item["stackable"].clone();
        steps.push(step);
    }
    if !reasons.is_empty() {
        return (Value::Null, reasons);
    }
    if counts
        .iter()
        .map(|(id, count)| {
            let stack = int(&items[id]["stackable"]);
            (count + stack - 1) / stack
        })
        .sum::<i128>()
        > 16
    {
        return (
            Value::Null,
            vec![json!({"reason":"item.placement_backpack_capacity"})],
        );
    }
    (
        json!({"policy":"cmangos-8ec338a1-starting-items-v1","grants":steps,"second_pass":"backpack_slot_order_equip_else_merge_then_first_empty","backpack_slots":16,"equipment_resolved":false,"runtime_consumer":"starter_init::CapturedStarterItems::compile"}),
        Vec::new(),
    )
}
pub(super) fn relationships(source: &Value, source_sha: &str) -> Value {
    let Some(value) = source.get("quest_relationship_capture") else {
        return json!({"status":"blocked","refusals":[{"quest":0,"field":"inbound","target":0,"reason":"unknown_coverage"}],"capture":null});
    };
    let mechanics = super::super::extensions::quest_mechanics::supported(source);
    let scripts: BTreeSet<_> = mechanics["scripts"]
        .as_array()
        .unwrap()
        .iter()
        .map(int)
        .collect();
    let items = ids(source, "item_template", "entry");
    let mut refusals = Vec::new();
    let mut refuse = |quest: &Value, field: &str, target: &Value, reason: &str| {
        refusals.push(json!({"quest":quest,"field":field,"target":target,"reason":reason}))
    };
    let own: BTreeMap<_, _> = value["own"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (int(&r["entry"]), r))
        .collect();
    let inbound = value["inbound"].as_array().unwrap();
    if own.len() > 1024 {
        refuse(&json!(0), "closure", &json!(own.len()), "closure_limit");
    }
    if inbound.len() > 1024 {
        refuse(&json!(0), "inbound", &json!(inbound.len()), "closure_limit");
    }
    for row in inbound {
        if !own.contains_key(&int(&row["entry"])) {
            for field in ["NextQuestId", "NextQuestInChain", "BreadcrumbForQuestId"] {
                if own.contains_key(&int(&row[field]).abs()) {
                    refuse(
                        &row["entry"],
                        field,
                        &row[field],
                        "uncaptured_incoming_quest",
                    );
                }
            }
        }
    }
    for row in own.values() {
        for field in [
            "PrevQuestId",
            "NextQuestId",
            "NextQuestInChain",
            "ExclusiveGroup",
            "BreadcrumbForQuestId",
        ] {
            let target = int(&row[field]);
            if target == 0 {
                continue;
            }
            if ["ExclusiveGroup", "BreadcrumbForQuestId"].contains(&field) {
                refuse(&row["entry"], field, &row[field], "unsupported_path");
            } else if !own.contains_key(&target.abs()) {
                refuse(&row["entry"], field, &row[field], "uncaptured_reference");
            }
        }
    }
    for row in rows(source, "quest_template") {
        for field in ["RequiredCondition", "StartScript", "SrcSpell"] {
            if nonzero(&row[field]) {
                refuse(&row["entry"], field, &row[field], "unsupported_path");
            }
        }
        if nonzero(&row["CompleteScript"]) && !scripts.contains(&int(&row["CompleteScript"])) {
            refuse(
                &row["entry"],
                "CompleteScript",
                &row["CompleteScript"],
                "unknown_or_unsupported_script",
            );
        }
        if nonzero(&row["SrcItemId"]) {
            if !items.contains(&int(&row["SrcItemId"])) {
                refuse(
                    &row["entry"],
                    "SrcItemId",
                    &row["SrcItemId"],
                    "unknown_lookup",
                );
            }
            if int(&row["SrcItemCount"]) > 10000 {
                refuse(
                    &row["entry"],
                    "SrcItemCount",
                    &row["SrcItemCount"],
                    "count_domain",
                );
            }
        }
        for slot in 1..=4 {
            for prefix in ["ReqSourceId", "ReqSourceCount", "ReqSpellCast"] {
                let field = format!("{prefix}{slot}");
                if nonzero(&row[&field]) {
                    refuse(&row["entry"], &field, &row[&field], "unsupported_path");
                }
            }
        }
    }
    if !refusals.is_empty() {
        return json!({"status":"blocked","refusals":refusals,"capture":null});
    }
    let mut previous: BTreeMap<i128, Vec<i128>> = own.keys().map(|n| (*n, Vec::new())).collect();
    let mut previous_chain = previous.clone();
    for (entry, row) in &own {
        let prev = int(&row["PrevQuestId"]);
        if prev != 0 {
            previous.get_mut(entry).unwrap().push(prev);
        }
        let next = int(&row["NextQuestId"]);
        if next != 0 {
            previous
                .get_mut(&next.abs())
                .unwrap()
                .push(if next > 0 { *entry } else { -entry });
        }
        let next = int(&row["NextQuestInChain"]);
        if next != 0 {
            previous_chain.get_mut(&next).unwrap().push(*entry);
        }
    }
    json!({"status":"resolved","refusals":[],"source_sha256":source_sha,"capture":{"roots":value["roots"],"own":value["own"],"inbound":value["inbound"]},"previous":previous.into_iter().map(|(k,v)|(k.to_string(),numbers(v))).collect::<BTreeMap<_,_>>(),"previous_chain":previous_chain.into_iter().map(|(k,v)|(k.to_string(),numbers(v))).collect::<BTreeMap<_,_>>()})
}
pub(super) fn quests(
    source: &Value,
    dbcs: &Tables,
    relationships: &Value,
    source_sha: &str,
) -> Value {
    let mut refusals = relationships["refusals"].as_array().unwrap().clone();
    let mut raw_resolved = true;
    let mut refuse = |quest: &Value, field: &str, target: &Value, reason: &str| {
        raw_resolved = false;
        refusals.push(json!({"quest":quest,"field":field,"target":target,"reason":reason}));
    };
    let Some(value) = source.get("quest_giver_presentation_capture") else {
        refuse(
            &json!(0),
            "quest_giver_presentation_capture",
            &json!(0),
            "unknown_coverage",
        );
        return json!({"status":"blocked","adapter":QUEST_ADAPTER,"refusals":refusals,"raw_presentation_resolved":false,"ordinary_catalog_resolved":false,"source_sha256":source_sha,"input_bindings":null});
    };
    let mechanics = super::super::extensions::quest_mechanics::supported(source);
    for row in mechanics["refusals"].as_array().unwrap() {
        refuse(
            &row["quest"],
            row["field"].as_str().unwrap(),
            &row["target"],
            row["reason"].as_str().unwrap(),
        );
    }
    if let Some(capture) = source.get("quest_mechanics_capture") {
        for row in capture["rows"]["dbscripts_on_quest_end"]
            .as_array()
            .unwrap()
        {
            if int(&row["command"]) == 1 {
                match dbcs["Emotes"].get(int(&row["datalong"]) as u32) {
                    None => refuse(&json!(0), "Emotes", &row["datalong"], "unknown_lookup"),
                    Some(emote) if emote.word(4) != 0 => refuse(
                        &json!(0),
                        "Emotes.EmoteType",
                        &row["datalong"],
                        "state_emote_not_supported",
                    ),
                    _ => {}
                }
            }
        }
    }
    let raw = &value["rows"];
    let emotes = &dbcs["Emotes"];
    for row in raw["quest_template"].as_array().unwrap() {
        for field in ["IncompleteEmoteDelay", "CompleteEmoteDelay"]
            .into_iter()
            .map(String::from)
            .chain(
                [
                    "DetailsEmote",
                    "DetailsEmoteDelay",
                    "OfferRewardEmote",
                    "OfferRewardEmoteDelay",
                ]
                .into_iter()
                .flat_map(|p| (1..=4).map(move |i| format!("{p}{i}"))),
            )
        {
            if nonzero(&row[&field]) {
                refuse(
                    &row["entry"],
                    &field,
                    &row[&field],
                    "unsupported_packet_shape",
                );
            }
        }
        for field in PLAN["quest_text"].as_array().unwrap() {
            let field = field.as_str().unwrap();
            if field == "RequestItemsText" && row[field].is_null() {
                continue;
            }
            if row[field].as_str().is_none_or(|text| {
                text.chars()
                    .any(|c| (c as u32) < 32 && c != '\n' && c != '\t')
            }) {
                refuse(&row["entry"], field, &json!(0), "invalid_text");
            }
        }
    }
    let entries: BTreeSet<_> = value["selectors"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(int)
        .collect();
    let spawns = ids(source, "creature", "id");
    for quest in value["selectors"]["quests"].as_array().unwrap() {
        for table in ["creature_questrelation", "creature_involvedrelation"] {
            if !rows(source, table)
                .iter()
                .any(|r| r["quest"] == *quest && spawns.contains(&int(&r["id"])))
                && !super::super::extensions::quest_endpoints::proved(source, table, int(quest))
            {
                refuse(quest, table, &json!(0), "missing_spawned_relation");
            }
        }
    }
    for row in raw["questgiver_greeting"].as_array().unwrap() {
        if int(&row["Type"]) != 0 || emotes.get(int(&row["EmoteId"]) as u32).is_none() {
            refuse(
                &json!(0),
                "questgiver_greeting",
                &row["Entry"],
                "invalid_type_or_emote",
            );
        }
    }
    for row in rows(source, "creature_template") {
        if entries.contains(&int(&row["Entry"])) {
            if int(&row["NpcFlags"]) & 2 == 0 {
                refuse(
                    &json!(0),
                    "NpcFlags",
                    &row["Entry"],
                    "missing_questgiver_flag",
                );
            }
            if nonzero(&row["ScriptName"]) {
                refuse(
                    &json!(0),
                    "ScriptName",
                    &row["Entry"],
                    "registered_callbacks_unresolved",
                );
            }
            if nonzero(&row["GossipMenuId"])
                && !mechanics["givers"]
                    .as_array()
                    .unwrap()
                    .contains(&row["Entry"])
            {
                refuse(
                    &json!(0),
                    "GossipMenuId",
                    &row["GossipMenuId"],
                    "gossip_applicability_unresolved",
                );
            }
        }
    }
    let presentation_resolved = raw_resolved;
    for field in [
        "loaded_relation_order",
        "effective_giver_selection",
        "registered_callbacks",
        "global_spell_area_and_item_effects",
        "session_locale_and_dialog_policy",
    ] {
        if field == "loaded_relation_order"
            && source
                .get("quest_mechanics_capture")
                .is_some_and(|v| !v["selectors"]["scripts"].as_array().unwrap().is_empty())
        {
            refusals.push(json!({"quest":0,"field":"loaded_script_order","target":0,"reason":"independent_context_required"}));
        }
        refusals.push(
            json!({"quest":0,"field":field,"target":0,"reason":"independent_context_required"}),
        );
    }
    let disposition = raw["quest_template"]
        .as_array()
        .unwrap()
        .first()
        .map_or(Vec::new(), |row| {
            PLAN["quest_text"]
                .as_array()
                .unwrap()
                .iter()
                .cloned()
                .chain([json!("IncompleteEmoteDelay"), json!("CompleteEmoteDelay")])
                .chain(
                    row.as_object()
                        .unwrap()
                        .keys()
                        .filter(|n| n.contains("Emote"))
                        .map(|n| json!(n)),
                )
                .collect()
        });
    let mut plan = json!({"status":"blocked","adapter":QUEST_ADAPTER,"source_sha256":source_sha,"raw_presentation_resolved":presentation_resolved,"ordinary_catalog_resolved":false,"refusals":refusals,"selected_quest_ids":value["selectors"]["quests"],"input_bindings":{"capture_sha256":digest(value),"Emotes_sha256":emotes.sha256},"field_dispositions":{"quest_template":{"consumed_by_ordinary_presentation":disposition},"remaining":"eligibility_credit_reward_rates_hooks_and_loaded_order"},"runtime_export":null});
    if source.get("quest_mechanics_capture").is_some() {
        plan["mechanics"] = mechanics;
    }
    if let Some(endpoints) = source.get("quest_endpoint_capture") {
        plan["endpoints"] = endpoints["endpoints"].clone();
    }
    plan
}
pub(super) fn talents(table: &dbc::Table, selected: &Value) -> Value {
    let ids: BTreeSet<_> = selected.as_array().unwrap().iter().map(int).collect();
    let mut matches = Vec::new();
    for index in table.by_id.values() {
        let row = table.row(*index);
        let slots: Vec<_> = (0..5)
            .filter(|s| row.word(4 + s) != 0 && ids.contains(&(row.word(4 + s) as i128)))
            .map(|s| json!({"slot":s,"spell":row.word(4+s)}))
            .collect();
        if !slots.is_empty() {
            matches.push(json!({"talent_id":row.word(0),"rank_slots":slots,"source":row.raw()}));
        }
    }
    json!({"coverage":"complete_raw_table","table_sha256":table.sha256,"row_count":table.count,"selected_spells":selected,"matches":matches})
}
pub(super) fn fireball(
    source: &Value,
    id: i128,
    dbc: Option<Row<'_>>,
    cast: &Value,
    range: &Value,
    duration: &Value,
) -> Result<Value> {
    if id != 133 {
        return Ok(Value::Null);
    }
    let scope = numeric_scope(source, id);
    let mut plan = json!({"adapter":FIREBALL_ADAPTER,"status":"blocked","effective_learning_proven":false,"runtime_export":null,"numeric_capture":scope,"refusals":[]});
    let mut refusals = Vec::new();
    if scope.is_null() {
        plan["refusals"] = json!(["spell_numeric.complete_effective_overlay_unknown"]);
        return Ok(plan);
    }
    let row = &scope["row"];
    let mut words: BTreeMap<usize, u32> = BTreeMap::new();
    let floats = names(&COMPILER_CONTRACT["numeric_floats"]);
    for name in PLAN["numeric_word_order"].as_array().unwrap() {
        let name = name.as_str().unwrap();
        let index = COMPILER_CONTRACT["numeric_words"][name].as_u64().unwrap() as usize;
        words.insert(
            index,
            if floats.contains(name) {
                dbc::binary32(row[name]["decimal"].as_str().unwrap())?
            } else {
                int(&row[name]) as u32
            },
        );
    }
    let family = int(&row["SpellFamilyFlags"]) as u64;
    words.insert(161, family as u32);
    words.insert(162, (family >> 32) as u32);
    let mut mismatches = Vec::new();
    if let Some(dbc) = dbc {
        for name in PLAN["numeric_word_order"].as_array().unwrap() {
            let name = name.as_str().unwrap();
            let i = COMPILER_CONTRACT["numeric_words"][name].as_u64().unwrap() as usize;
            if words[&i] != dbc.word(i)
                && !(floats.contains(name)
                    && words[&i] & 0x7fffffff == 0
                    && dbc.word(i) & 0x7fffffff == 0)
            {
                mismatches.push(json!(name));
            }
        }
        if [161, 162].iter().any(|i| words[i] != dbc.word(*i)) {
            mismatches.push(json!("SpellFamilyFlags"));
        }
    }
    plan["server_client_differences"] = json!(mismatches);
    if dbc.is_none() || !mismatches.is_empty() {
        refusals.push(if mismatches.is_empty() {
            "Spell.133.missing"
        } else {
            "spell.server_client_field_mismatch"
        });
    }
    let mut names_by_word: BTreeMap<usize, String> = COMPILER_CONTRACT["numeric_words"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, i)| (i.as_u64().unwrap() as usize, name.clone()))
        .collect();
    names_by_word.insert(161, "SpellFamilyFlags.low".into());
    names_by_word.insert(162, "SpellFamilyFlags.high".into());
    let exact: BTreeMap<usize, u32> = PLAN["fireball_exact"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(i, v)| (i.parse().unwrap(), v.as_u64().unwrap() as u32))
        .collect();
    let shape:Vec<_>=exact.iter().filter(|(i,expected)|words[i]!=**expected).map(|(i,expected)|json!({"table":"spell_template","key":[133],"field":names_by_word[i],"word":i,"observed":words[i],"required":expected})).collect();
    if !shape.is_empty() {
        refusals.push("spell_numeric.exact_shape");
    }
    plan["shape_differences"] = json!(shape);
    let coefficient_names: Vec<_> = ["EffectBonusCoefficient", "EffectBonusCoefficientFromAP"]
        .iter()
        .flat_map(|p| (1..=3).map(move |i| format!("{p}{i}")))
        .collect();
    let expected = [0x3dfbe76d, 0, 0, 0, 0, 0];
    let mut server = Vec::new();
    for (name, expected) in coefficient_names.iter().zip(expected) {
        let actual = dbc::binary32(row[name]["decimal"].as_str().unwrap())?;
        if actual != expected {
            server.push(
                json!({"field":name,"observed_binary32":actual,"required_binary32":expected}),
            );
        }
    }
    for name in ["IsServerSide", "AttributesServerside"] {
        if nonzero(&row[name]) {
            server.push(json!({"field":name,"observed":row[name],"required":0}));
        }
    }
    if !server.is_empty() {
        refusals.push("spell_numeric.server_shape");
    }
    plan["server_differences"] = json!(server);
    if *cast != json!({"id":16,"base_ms":1500,"per_level_ms":0,"minimum_ms":1500})
        || *range != json!({"id":35,"minimum_binary32":0,"maximum_binary32":0x420c0000,"flags":0})
        || *duration != json!({"id":35,"base_ms":4000,"per_level_ms":0,"maximum_ms":4000})
    {
        refusals.push("spell_numeric.auxiliary_shape");
    }
    if scope["facing"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| ![0, 1].contains(&int(&r["facingcasterflag"])))
    {
        refusals.push("spell_numeric.facing_bits");
    }
    if refusals.is_empty() {
        plan["status"] = json!("supported_shape");
        plan["input_bindings"] = json!({"numeric_capture_sha256":scope["coverage_sha256"],"Spell.133.sha256":digest(&json!((0..dbc.unwrap().len()).map(|i|dbc.unwrap().word(i)).collect::<Vec<_>>())) ,"cast_time":cast,"range":range,"duration":duration});
    }
    plan["refusals"] = json!(refusals);
    Ok(plan)
}
pub(super) fn fireball_discharges(issue: &Value, plan: &Value) -> bool {
    if plan["status"] != "supported_shape"
        || issue["table"] != "spell_template"
        || !(issue["key"] == json!([133]) || issue["key"] == "133")
        || !names(&PLAN["fireball_reasons"]).contains(issue["reason"].as_str().unwrap())
    {
        return false;
    }
    match issue["reason"].as_str().unwrap() {
        "spell.effect_execution_required" | "spell.target_selector_execution_required" => {
            issue.get("slot").is_some_and(|s| *s == 1 || *s == 2)
        }
        "spell.attribute_bits_execution_required" => {
            issue["field"] == "Attributes" && issue["bits"] == 65536
        }
        _ => true,
    }
}
fn closure(
    roots: &BTreeSet<i128>,
    spells: &BTreeMap<i128, &Value>,
    links: &BTreeMap<i128, BTreeSet<i128>>,
) -> BTreeSet<i128> {
    let mut result = roots.clone();
    let mut pending: Vec<_> = roots.iter().copied().collect();
    while let Some(parent) = pending.pop() {
        for id in links.get(&parent).into_iter().flatten() {
            if result.insert(*id) {
                pending.push(*id);
            }
        }
        if let Some(row) = spells.get(&parent) {
            for slot in 1..=3 {
                let id = int(&row[format!("EffectTriggerSpell{slot}")]);
                if id != 0 && result.insert(id) {
                    pending.push(id);
                }
            }
        }
    }
    result
}
pub(super) fn classify(
    source: &Value,
    spells: &Value,
    issues: &mut [Value],
    learning: &Value,
) -> Value {
    let sql: BTreeMap<_, _> = rows(source, "spell_template")
        .iter()
        .map(|r| (int(&r["Id"]), r))
        .collect();
    let bound: BTreeMap<_, _> = spells
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (int(&r["id"]), r))
        .collect();
    let initial = ids(source, "playercreateinfo_spell", "Spell");
    let passive: BTreeSet<_> = initial
        .iter()
        .filter(|id| int(&sql[id]["Attributes"]) & 0x40 != 0)
        .copied()
        .collect();
    let mut sensitive: BTreeSet<_> = initial
        .iter()
        .filter(|id| {
            let row = sql[id];
            int(&row["Attributes"]) & 0x80 != 0
                || int(&row["AttributesEx"]) & 0x80000000 != 0
                || (1..=3).any(|s| [36, 44, 118].contains(&int(&row[format!("Effect{s}")])))
        })
        .copied()
        .collect();
    sensitive.extend(
        rows(source, "spell_learn_spell")
            .iter()
            .map(|r| int(&r["entry"]))
            .filter(|id| initial.contains(id)),
    );
    let candidates: BTreeSet<_> = initial
        .iter()
        .filter(|id| {
            !passive.contains(id)
                && !sensitive.contains(id)
                && !bound[id]["dbc_only_inputs"].is_null()
                && !bound[id]["server_client_differences"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| {
                        [
                            "Attributes",
                            "AttributesEx",
                            "Effect1",
                            "Effect2",
                            "Effect3",
                        ]
                        .iter()
                        .any(|s| v == s)
                    })
        })
        .copied()
        .collect();
    let mut critical: BTreeSet<_> = passive.union(&sensitive).copied().collect();
    for edge in learning["edges"].as_array().unwrap() {
        if edge["to_kind"] == "spell"
            && [
                "ability_spell",
                "learn_spell",
                "prev_spell",
                "first_spell",
                "req_spell",
            ]
            .iter()
            .any(|r| edge["role"] == *r)
        {
            let id = int(&edge["to_id"]);
            if !candidates.contains(&id) {
                critical.insert(id);
            }
        }
    }
    for row in rows(source, "item_template") {
        for field in ["requiredspell".into()]
            .into_iter()
            .chain((1..=5).map(|i| format!("spellid_{i}")))
        {
            let id = int(&row[&field]);
            if id > 0 {
                critical.insert(id);
            }
        }
    }
    for row in rows(source, "quest_template") {
        for n in ["SrcSpell", "RewSpell", "RewSpellCast"] {
            let id = int(&row[n]);
            if id != 0 {
                critical.insert(id);
            }
        }
    }
    let mut links: BTreeMap<i128, BTreeSet<i128>> = BTreeMap::new();
    for r in rows(source, "spell_learn_spell") {
        links
            .entry(int(&r["entry"]))
            .or_default()
            .insert(int(&r["SpellID"]));
    }
    for r in rows(source, "spell_chain") {
        let set = links.entry(int(&r["spell_id"])).or_default();
        for field in ["prev_spell", "first_spell", "req_spell"] {
            let id = int(&r[field]);
            if id != 0 {
                set.insert(id);
            }
        }
    }
    critical = closure(&critical, &sql, &links);
    let active: BTreeSet<_> = candidates.difference(&critical).copied().collect();
    let closures: BTreeMap<_, _> = active
        .iter()
        .map(|id| (*id, closure(&BTreeSet::from([*id]), &sql, &links)))
        .collect();
    let active_only: BTreeSet<_> = closures
        .values()
        .flatten()
        .filter(|id| !critical.contains(id))
        .copied()
        .collect();
    let mut owners: BTreeMap<(String, i128), BTreeSet<i128>> = BTreeMap::new();
    for (entry, row) in &sql {
        for table in ["spell_template", "spell_scripts", "Spell"] {
            owners
                .entry((table.into(), *entry))
                .or_default()
                .insert(*entry);
        }
        for (table, field) in [
            ("SpellCastTimes", "CastingTimeIndex"),
            ("SpellRange", "rangeIndex"),
            ("SpellCategory", "Category"),
            ("SpellDuration", "DurationIndex"),
            ("SpellFocusObject", "RequiresSpellFocus"),
        ] {
            let key = int(&row[field]);
            if key != 0 || table == "SpellRange" {
                owners
                    .entry((table.into(), key))
                    .or_default()
                    .insert(*entry);
            }
        }
        for slot in 1..=3 {
            let key = int(&row[format!("EffectRadiusIndex{slot}")]);
            if key != 0 {
                owners
                    .entry(("SpellRadius".into(), key))
                    .or_default()
                    .insert(*entry);
            }
        }
        let dbc = &bound[entry]["dbc_only_inputs"];
        if !dbc.is_null() {
            for item in dbc["totems"]
                .as_array()
                .unwrap()
                .iter()
                .chain(dbc["reagents"].as_array().unwrap())
                .map(int)
                .filter(|n| *n > 0)
            {
                owners
                    .entry(("item_template".into(), item))
                    .or_default()
                    .insert(*entry);
            }
        }
    }
    let mut reasons: BTreeMap<i128, BTreeSet<String>> = active
        .iter()
        .map(|id| {
            (
                *id,
                BTreeSet::from(["spell.native_effect_target_scaling_rules_required".into()]),
            )
        })
        .collect();
    for issue in issues {
        let key = &issue["key"];
        let key = key
            .as_array()
            .filter(|a| a.len() == 1)
            .map_or(key, |a| &a[0]);
        let numeric = key
            .as_str()
            .and_then(|s| s.parse::<i128>().ok())
            .or_else(|| key.as_number().and_then(|n| n.as_str().parse().ok()));
        let table = issue["table"].as_str().unwrap();
        let reason = issue["reason"].as_str().unwrap();
        let mut related = numeric
            .and_then(|key| owners.get(&(table.into(), key)))
            .cloned()
            .unwrap_or_default();
        if *key == json!([]) && reason.starts_with("dbc.") {
            related = owners
                .iter()
                .filter(|((t, _), _)| t == table)
                .flat_map(|(_, ids)| ids)
                .copied()
                .collect();
        }
        let scopeable = if table == "item_template" {
            reason == "spell.dbc_reagent_or_totem_not_captured"
        } else {
            reason.starts_with("spell.") || reason.starts_with("dbc.")
        };
        if scopeable && !related.is_empty() && related.is_subset(&active_only) {
            let roots: Vec<_> = closures
                .iter()
                .filter(|(_, deps)| !deps.is_disjoint(&related))
                .map(|(id, _)| *id)
                .collect();
            for id in &roots {
                reasons.get_mut(id).unwrap().insert(reason.into());
            }
            issue["admission_scope"] =
                json!({"kind":"unavailable_active_ability","spell_ids":numbers(roots)});
        } else {
            issue["admission_scope"] = json!({"kind":"selection_entry"});
        }
    }
    json!({"available_abilities":[],"unavailable_active_abilities":active.iter().map(|id|json!({"id":num(*id),"status":"unavailable","presentation":"visible_unavailable","cast_admission":"reject","trigger_dependency_ids":numbers(closures[id].iter().filter(|n|*n!=id).copied()),"reason_ids":reasons[id]})).collect::<Vec<_>>(),"entry_critical_spell_ids":numbers(critical),"learning_sensitive_initial_spell_ids":numbers(sensitive.iter().copied()),"unresolved_initial_spell_roles":numbers(initial.iter().filter(|id|!passive.contains(id)&&!sensitive.contains(id)&&!candidates.contains(id)).copied()),"effective_learning_proven":false})
}
