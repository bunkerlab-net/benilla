// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
const ADAPTER: &str = "benilla_sim::loot::captured::CapturedLoot::compile";
const TABLES: [&str; 2] = ["creature_loot_template", "reference_loot_template"];
const DISCHARGED: [&str; 3] = [
    "loot.source_loader_order_unproven",
    "loot.reference_generation_not_implemented",
    "loot.generation_and_item_admission_not_established",
];
fn chance(row: &Value) -> &str {
    row["ChanceOrQuestChance"]["decimal"].as_str().unwrap()
}
fn quest_leaf(row: &Value) -> bool {
    chance(row).starts_with('-')
        && compare_decimal(chance(row).trim_start_matches('-'), "0.000001")
            != std::cmp::Ordering::Less
        && int(&row["condition_id"]) == 0
        && int(&row["groupid"]) == 0
        && int(&row["item"]) > 0
        && int(&row["mincountOrRef"]) > 0
        && int(&row["mincountOrRef"]) <= int(&row["maxcount"])
        && int(&row["maxcount"]) <= 255
}
pub(in super::super) fn quest_loot(source: &Value) -> Value {
    json!(rows(source,TABLES[0]).iter().filter(|r|quest_leaf(r)).map(|r|json!({"entry":r["entry"],"item":r["item"],"quest_only":true,"chance":r["ChanceOrQuestChance"]["decimal"].as_str().unwrap().trim_start_matches('-'),"minimum_count":r["mincountOrRef"],"maximum_count":r["maxcount"],"rate_policy":"explicit_CONFIG_FLOAT_RATE_DROP_ITEM_QUEST","owner_policy":"active_incomplete_ReqItem_below_required_count","runtime_admitted":false})).collect::<Vec<_>>())
}
fn budget(rs: &[&Value], references: &BTreeMap<i128, (u64, u64, u64)>) -> (u64, u64, u64) {
    let (mut passes, mut work, mut draws) = (1, 1, 0);
    let mut groups: BTreeMap<i128, [u64; 2]> = BTreeMap::new();
    for row in rs {
        let chance = chance(row).trim_start_matches('-');
        let group = int(&row["groupid"]);
        if group != 0 {
            groups.entry(group).or_default()[usize::from(chance == "0")] += 1;
        } else {
            work += 1;
            draws += u64::from(compare_decimal(chance, "100") == std::cmp::Ordering::Less);
            let min = int(&row["mincountOrRef"]);
            if min < 0 {
                let child = references.get(&-min).copied().unwrap_or((0, 0, 0));
                let repeat = int(&row["maxcount"]) as u64;
                passes += child.0 * repeat;
                work += child.1 * repeat;
                draws += child.2 * repeat;
            } else {
                draws += 1;
            }
        }
    }
    for [explicit, equal] in groups.values() {
        work += 2 * (explicit + equal);
        draws +=
            explicit + u64::from(*explicit != 0) + 2 * equal + u64::from(explicit + equal != 0);
    }
    work += groups.keys().next_back().copied().unwrap_or(0) as u64;
    (passes, work, draws)
}
fn budgets(source: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    if TABLES.iter().map(|t| rows(source, t).len()).sum::<usize>() > 4096 {
        out.push(json!({"table":"loot","key":"scope","reason":"loot.retained_closure_limit"}));
    }
    let mut indexed: BTreeMap<&str, BTreeMap<i128, Vec<&Value>>> = BTreeMap::new();
    for t in TABLES {
        let mut templates: BTreeMap<i128, Vec<&Value>> = BTreeMap::new();
        for row in rows(source, t) {
            templates.entry(int(&row["entry"])).or_default().push(row);
        }
        for (id, rs) in &templates {
            if rs.len() > 1024 {
                out.push(
                    json!({"table":t,"key":id.to_string(),"reason":"loot.template_row_limit"}),
                );
            }
        }
        indexed.insert(t, templates);
    }
    let references: BTreeMap<_, _> = indexed[TABLES[1]]
        .iter()
        .map(|(id, rs)| (*id, budget(rs, &BTreeMap::new())))
        .collect();
    for (id, rs) in &indexed[TABLES[0]] {
        let (p, w, d) = budget(rs, &references);
        if p.max(w).max(d + 2) > 4096 {
            out.push(
                json!({"table":TABLES[0],"key":id.to_string(),"reason":"loot.processing_budget"}),
            );
        }
    }
    out
}
pub(in super::super) fn loot(source: &Value, source_sha: &str) -> Result<Value> {
    let Some(capture) = source.get("loot_capture") else {
        return Ok(json!({"status":"not_captured","adapter":ADAPTER,"refusals":[]}));
    };
    let mut refusals: Vec<Value> = capture["unsupported"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| !DISCHARGED.iter().any(|s| r["reason"] == *s))
        .cloned()
        .collect();
    refusals.extend(budgets(source));
    let mut ordered = Map::new();
    for t in TABLES {
        let loader = source
            .get("loot_loader_capture")
            .and_then(|v| v["tables"].get(t));
        if let Some(loader) = loader {
            let indexed: BTreeMap<_, _> = rows(source, t)
                .iter()
                .map(|r| ((int(&r["entry"]), int(&r["item"])), r))
                .collect();
            let mut rs = Vec::new();
            for selected in loader["selected_rows"].as_array().unwrap() {
                let key = &selected["key"];
                let row = indexed[&(int(&key[0]), int(&key[1]))];
                let bits =
                    dbc::binary32(row["ChanceOrQuestChance"]["decimal"].as_str().unwrap()).ok();
                if bits.is_none() {
                    refusals.push(json!({"table":t,"key":format!("{}:{}",row["entry"],row["item"]),"reason":"loot.binary32_chance_domain"}));
                }
                if bits == Some(0) && int(&row["groupid"]) == 0 {
                    refusals.push(json!({"table":t,"key":format!("{}:{}",row["entry"],row["item"]),"reason":"loot.ungrouped_binary32_zero_chance"}));
                }
                let mut value = row.clone();
                value["source_position"] = selected["position"].clone();
                value["chance_or_quest_chance_bits"] = json!(bits);
                rs.push(value);
            }
            ordered.insert(t.into(), json!(rs));
        } else {
            refusals
                .push(json!({"table":t,"key":"scope","reason":"loot.whole_loader_order_unknown"}));
        }
    }
    let items: BTreeMap<_, _> = rows(source, "item_template")
        .iter()
        .map(|r| (int(&r["entry"]), r))
        .collect();
    let mut admitted = Vec::new();
    for id in capture["leaf_item_ids"].as_array().unwrap() {
        let entry = int(id);
        let mut reasons = Vec::new();
        if let Some(item) = items.get(&entry) {
            let plan = item::item_plan(item);
            reasons.extend(
                plan["refusals"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|r| r["reason"].as_str().unwrap().to_owned()),
            );
            if reasons.is_empty() {
                admitted.push(json!({"entry":id,"item_template_sha256":digest(item),
                    "acquisition":plan["carried_operation"],"equip_and_use_admitted":false}));
            }
        } else {
            reasons.push("item_template_missing".into());
        }
        for reason in reasons {
            refusals.push(json!({"table":"item_template","key":entry.to_string(),"reason":format!("loot.{reason}")}));
        }
    }
    Ok(
        json!({"status":if refusals.is_empty(){"supported_shape"}else{"blocked"},"adapter":ADAPTER,"runtime_admitted":false,"ordered_rows":ordered,"leaf_items":admitted,"input_bindings":{"source_sha256":source_sha,"loader":source.get("loot_loader_capture")},"remaining_dependencies":["explicit_loot_Rules","independent_inventory_audit","ordinary_corpse_host_audit_and_creature_rows","client_item_query_metadata"],"refusals":refusals}),
    )
}
pub(in super::super) fn loot_discharges(issue: &Value, plan: &Value) -> bool {
    plan["status"] == "supported_shape" && DISCHARGED.iter().any(|s| issue["reason"] == *s)
}
