// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
const TABLES: [&str; 2] = ["creature_loot_template", "reference_loot_template"];
const FIELDS: [&str; 7] = [
    "entry",
    "item",
    "ChanceOrQuestChance",
    "groupid",
    "mincountOrRef",
    "maxcount",
    "condition_id",
];
fn creatures(b: &Value) -> Result<Value> {
    let fields = [
        "Entry",
        "LootId",
        "PickpocketLootId",
        "SkinningLootId",
        "MinLootGold",
        "MaxLootGold",
    ];
    let mut result = Vec::new();
    for r in rows(b, "creature_template") {
        let mut out = Map::new();
        for n in fields {
            integer(
                &r[n],
                if n == "Entry" { 1 } else { 0 },
                0xffffff,
                "loot.creature_field_domain",
            )?;
            out.insert(n.into(), r[n].clone());
        }
        result.push(Value::Object(out));
    }
    Ok(json!(result))
}
fn references(rs: &[Value]) -> BTreeSet<i128> {
    rs.iter()
        .map(|r| int(&r["mincountOrRef"]))
        .filter(|n| *n < 0)
        .map(|n| -n)
        .collect()
}
fn leaves(b: &Value) -> Result<BTreeSet<i128>> {
    let ids: BTreeSet<_> = TABLES
        .iter()
        .flat_map(|t| rows(b, t))
        .filter(|r| int(&r["mincountOrRef"]) > 0 && int(&r["item"]) > 0)
        .map(|r| int(&r["item"]))
        .collect();
    check(ids.len() <= 2048, "loot.item_limit")?;
    Ok(ids)
}
fn cycles(ids: &BTreeSet<i128>, rs: &[Value]) -> Vec<Vec<i64>> {
    let mut graph: BTreeMap<i128, BTreeSet<i128>> =
        ids.iter().map(|n| (*n, BTreeSet::new())).collect();
    let mut reverse = graph.clone();
    for r in rs {
        let target = -int(&r["mincountOrRef"]);
        if target > 0 {
            let source = int(&r["entry"]);
            graph.get_mut(&source).unwrap().insert(target);
            reverse.get_mut(&target).unwrap().insert(source);
        }
    }
    let mut seen = BTreeSet::new();
    let mut order = Vec::new();
    for start in ids {
        if !seen.insert(*start) {
            continue;
        }
        let mut stack = vec![(*start, graph[start].iter())];
        while let Some((node, children)) = stack.last_mut() {
            if let Some(child) = children.next() {
                if seen.insert(*child) {
                    stack.push((*child, graph[child].iter()));
                }
            } else {
                order.push(*node);
                stack.pop();
            }
        }
    }
    seen.clear();
    let mut result = Vec::new();
    for start in order.into_iter().rev() {
        if !seen.insert(start) {
            continue;
        }
        let mut component = Vec::new();
        let mut pending = vec![start];
        while let Some(node) = pending.pop() {
            component.push(node as i64);
            for child in &reverse[&node] {
                if seen.insert(*child) {
                    pending.push(*child);
                }
            }
        }
        if component.len() > 1 || graph[&start].contains(&start) {
            component.sort();
            result.push(component);
        }
    }
    result.sort();
    result
}
fn record(b: &Value, table: &str, selected: &BTreeSet<i128>, rs: &[Value]) -> Result<Value> {
    let present = has(b, table);
    let missing: BTreeSet<_> = selected
        .difference(&rs.iter().map(|r| int(&r["entry"])).collect())
        .copied()
        .collect();
    Ok(
        json!({"requested_ids":numbers(selected.iter().copied()),"mode":"body_loot_selectors","selector_column":"entry","row_limit":4096,"query_performed":present&&!selected.is_empty(),"query_sha256":if present{query_hash(table,selected,None)?}else{Value::Null},"returned_keys":if present{json!(rs.iter().map(|r|vec![r["entry"].clone(),r["item"].clone()]).collect::<Vec<_>>())}else{Value::Null},"rows_sha256":if present{json!(digest(&rs))}else{Value::Null},"missing_ids":if present{numbers(missing.iter().copied())}else{Value::Null},"status":if !present{"unknown"}else if selected.is_empty(){"not_needed"}else if !missing.is_empty(){"missing"}else{"complete"}}),
    )
}
fn derived(b: &Value) -> Result<Value> {
    let cs = creatures(b)?;
    let roots: BTreeSet<_> = cs
        .as_array()
        .unwrap()
        .iter()
        .map(|r| int(&r["LootId"]))
        .filter(|n| *n != 0)
        .collect();
    check(roots.len() <= 4096, "loot.root_id_limit")?;
    within(b, TABLES[0], "entry", &roots, "loot.unselected_row")?;
    let mut records: BTreeMap<&str, Vec<Value>> = BTreeMap::new();
    records.insert(
        TABLES[0],
        vec![record(b, TABLES[0], &roots, rows(b, TABLES[0]))?],
    );
    records.insert(TABLES[1], Vec::new());
    let mut requested = references(rows(b, TABLES[0]));
    let mut visited = BTreeSet::new();
    let indexed: BTreeMap<i128, Vec<&Value>> =
        rows(b, TABLES[1]).iter().fold(BTreeMap::new(), |mut m, r| {
            m.entry(int(&r["entry"])).or_default().push(r);
            m
        });
    while !requested.is_subset(&visited) {
        check(requested.len() <= 4096, "loot.reference_id_limit")?;
        let batch: BTreeSet<_> = requested.difference(&visited).copied().collect();
        let mut rs: Vec<_> = batch
            .iter()
            .flat_map(|id| indexed.get(id).into_iter().flatten())
            .map(|r| (*r).clone())
            .collect();
        rs.sort_by_key(|r| (int(&r["entry"]), int(&r["item"])));
        records
            .get_mut(TABLES[1])
            .unwrap()
            .push(record(b, TABLES[1], &batch, &rs)?);
        visited.extend(batch);
        requested.extend(references(&rs));
    }
    if records[TABLES[1]].is_empty() {
        records
            .get_mut(TABLES[1])
            .unwrap()
            .push(record(b, TABLES[1], &BTreeSet::new(), &[])?);
    }
    check(
        indexed.keys().all(|id| requested.contains(id)),
        "loot.unselected_row",
    )?;
    let mut captured = Map::new();
    for (table, queries) in records {
        let missing: BTreeSet<_> = queries
            .iter()
            .flat_map(|q| q["missing_ids"].as_array().into_iter().flatten())
            .map(int)
            .collect();
        let ids: BTreeSet<_> = queries
            .iter()
            .flat_map(|q| q["requested_ids"].as_array().unwrap())
            .map(int)
            .collect();
        captured.insert(table.into(),json!({"requested_ids":numbers(ids.iter().copied()),"queries":queries,"missing_ids":if has(b,table){numbers(missing.iter().copied())}else{Value::Null},"status":if !has(b,table){"unknown"}else if ids.is_empty(){"not_needed"}else if !missing.is_empty(){"missing"}else{"complete"}}));
    }
    let mut edges = Vec::new();
    for t in TABLES {
        for r in rows(b, t) {
            if int(&r["mincountOrRef"]) < 0 {
                let mut edge = json!({"table":t,"entry":r["entry"],"item":r["item"],"raw_mincountOrRef":r["mincountOrRef"],"target_id":-int(&r["mincountOrRef"]) as i64});
                for n in ["ChanceOrQuestChance", "groupid", "maxcount", "condition_id"] {
                    edge[n] = r[n].clone();
                }
                edges.push(edge);
            }
        }
    }
    Ok(
        json!({"root_ids":numbers(roots),"reference_ids":numbers(requested.iter().copied()),"reference_edges":edges,"leaf_item_ids":numbers(leaves(b)?),"cyclic_reference_components":cycles(&requested,rows(b,TABLES[1])),"tables":captured}),
    )
}
fn chance(r: &Value) -> &str {
    r["ChanceOrQuestChance"]["decimal"].as_str().unwrap()
}
fn quest_leaf(r: &Value, t: &str) -> bool {
    let c = chance(r);
    t == TABLES[0]
        && c.starts_with('-')
        && compare_decimal(c.trim_start_matches('-'), "0.000001") != std::cmp::Ordering::Less
        && int(&r["condition_id"]) == 0
        && int(&r["groupid"]) == 0
        && int(&r["item"]) > 0
        && int(&r["mincountOrRef"]) > 0
        && int(&r["mincountOrRef"]) <= int(&r["maxcount"])
        && int(&r["maxcount"]) <= 255
}
fn unsupported(b: &Value, graph: &Value) -> Result<Vec<Value>> {
    let mut findings = vec![
        issue(TABLES[0], "scope", "loot.source_loader_order_unproven"),
        issue(
            TABLES[0],
            "scope",
            "loot.generation_and_item_admission_not_established",
        ),
    ];
    for (table, rec) in graph["tables"].as_object().unwrap() {
        if rec["status"] == "unknown" {
            findings.push(issue(
                table,
                "schema",
                "loot.table_missing_dependency_unknown",
            ));
        }
        for key in rec["missing_ids"].as_array().into_iter().flatten() {
            findings.push(issue(table, key, "loot.selected_template_missing"));
        }
    }
    for c in creatures(b)?.as_array().unwrap() {
        for field in ["PickpocketLootId", "SkinningLootId"] {
            if int(&c[field]) != 0 {
                findings.push(issue(
                    "creature_template",
                    &c["Entry"],
                    &format!("loot.{field}_outside_body_scope"),
                ));
            }
        }
    }
    for component in graph["cyclic_reference_components"].as_array().unwrap() {
        for key in component.as_array().unwrap() {
            findings.push(issue(TABLES[1], key, "loot.reference_cycle_not_executable"));
        }
    }
    for t in TABLES {
        for r in rows(b, t) {
            let key = format!("{}:{}", r["entry"], r["item"]);
            let c = chance(r);
            let mut reasons = Vec::new();
            let condition = int(&r["condition_id"]);
            let min = int(&r["mincountOrRef"]);
            let max = int(&r["maxcount"]);
            let group = int(&r["groupid"]);
            if condition != 0 {
                reasons.push("condition_not_resolved");
            }
            if condition > 0xffff {
                reasons.push("condition_uint16_conversion_unproven");
            }
            if c.starts_with('-') && !quest_leaf(r, t) {
                reasons.push("quest_leaf_shape_unsupported");
            }
            if min == 0 {
                reasons.push("zero_mincount_unsupported");
            }
            if max == 0 || min > max {
                reasons.push("count_shape_unsupported");
            }
            if group >= 128 {
                reasons.push("group_domain_unsupported");
            }
            if c == "0" && group == 0 {
                reasons.push("ungrouped_zero_chance_unsupported");
            }
            if c != "0"
                && compare_decimal(c.trim_start_matches('-'), "0.000001")
                    == std::cmp::Ordering::Less
            {
                reasons.push("small_chance_loader_conversion_unproven");
            }
            if min > 0 && int(&r["item"]) == 0 {
                reasons.push("zero_leaf_item_unsupported");
            }
            if min < 0 {
                reasons.push("reference_generation_not_implemented");
                if t == TABLES[1] {
                    reasons.push("nested_reference_not_executable");
                }
                if group != 0 {
                    reasons.push("grouped_reference_not_executable");
                }
            }
            for reason in reasons {
                findings.push(issue(t, &key, &format!("loot.{reason}")));
            }
        }
    }
    findings.sort_by(|a, b| {
        a["table"]
            .as_str()
            .cmp(&b["table"].as_str())
            .then_with(|| a["key"].as_str().cmp(&b["key"].as_str()))
            .then_with(|| a["reason"].as_str().cmp(&b["reason"].as_str()))
    });
    Ok(findings)
}
pub(super) fn validate(b: &Value, p: &Profiles) -> Result<()> {
    for t in TABLES {
        check(
            p.contains_key(t) == has(b, t),
            "loot.table_coverage_required",
        )?;
    }
    check(
        leaves(b)?.is_subset(&ids(b, "item_template", "entry")),
        "loot.leaf_item_missing",
    )?;
    let graph = derived(b)?;
    let cs = creatures(b)?;
    let mut nonbody = Vec::new();
    for c in cs.as_array().unwrap() {
        for field in ["PickpocketLootId", "SkinningLootId"] {
            if int(&c[field]) != 0 {
                nonbody.push(json!({"creature_entry":c["Entry"],"field":field,"loot_id":c[field],"status":"unknown","reason":"outside_body_scope"}));
            }
        }
    }
    let mut expected = json!({"format":"benilla-classic-body-loot-selection-v1","scope":"selected_body_loot_dependencies_only","requested_mode":"body_loot_selectors","selected_rows_complete":graph["tables"].as_object().unwrap().values().all(|r|r["status"]=="complete"||r["status"]=="not_needed"),"query_order":"entry_item_key_order","loader_order":"unproven","creatures":cs,"non_body_dependencies":nonbody,"unsupported":unsupported(b,&graph)?});
    for (n, v) in graph.as_object().unwrap() {
        expected[n] = v.clone();
    }
    let mut snap = snapshot(
        b,
        &[
            "creature",
            "creature_template",
            TABLES[0],
            TABLES[1],
            "item_template",
            "spell_template",
        ],
    );
    snap["core_revision"] = json!(CORE);
    snap["database_revision"] = json!(DATABASE);
    snap["revision_claims_verified_against_installation"] = json!(false);
    expected["snapshot"] = snap;
    coverage(b, "loot_capture", expected, "loot.coverage_mismatch")
}
pub(super) fn blockers(b: &Value) -> Result<Vec<Value>> {
    unsupported(b, &derived(b)?)
}
pub(super) fn loader(b: &Value) -> Result<()> {
    let Some(v) = b.get("loot_loader_capture") else {
        return Ok(());
    };
    fields(v, &["format", "tables"], "loot.loader_extension")?;
    check(
        v["format"] == "benilla-classic-loot-loader-order-v1",
        "loot.loader_extension",
    )?;
    let ts = object(&v["tables"], "loot.loader_extension")?;
    check(
        ts.keys().map(String::as_str).collect::<BTreeSet<_>>()
            == TABLES.into_iter().filter(|t| has(b, t)).collect(),
        "loot.loader_tables",
    )?;
    for (t, r) in ts {
        fields(
            r,
            &[
                "query_sha256",
                "result_row_count",
                "ordered_result_sha256",
                "selected_rows",
                "selected_projection_sha256",
            ],
            "loot.loader_record",
        )?;
        let query = json!([format!("SELECT {} FROM {t}", FIELDS.join(", ")), []]);
        check(r["query_sha256"] == digest(&query), "loot.loader_query")?;
        let count = integer(&r["result_row_count"], 0, 2_000_000, "loot.loader_count")?;
        valid_hash(&r["ordered_result_sha256"], "loot.loader_fingerprint")?;
        check(
            r["ordered_result_sha256"] != "0".repeat(64),
            "loot.loader_fingerprint",
        )?;
        check(
            r["selected_projection_sha256"] == digest(&b["tables"][t]),
            "loot.loader_projection_binding",
        )?;
        let positions = array(&r["selected_rows"], "loot.loader_positions")?;
        check(positions.len() == rows(b, t).len(), "loot.loader_positions")?;
        let mut seen = BTreeSet::new();
        let mut previous = -1;
        for selected in positions {
            fields(selected, &["position", "key"], "loot.loader_position")?;
            let pos = integer(
                &selected["position"],
                0,
                (count - 1).max(0),
                "loot.loader_position",
            )?;
            check(previous < pos && pos < count, "loot.loader_position_order")?;
            previous = pos;
            let key = array(&selected["key"], "loot.loader_key")?;
            check(key.len() == 2, "loot.loader_key")?;
            let key = (
                integer(&key[0], i128::MIN, i128::MAX, "loot.loader_key")?,
                integer(&key[1], i128::MIN, i128::MAX, "loot.loader_key")?,
            );
            check(seen.insert(key), "loot.loader_duplicate_key")?;
        }
        check(
            seen == rows(b, t)
                .iter()
                .map(|r| (int(&r["entry"]), int(&r["item"])))
                .collect(),
            "loot.loader_key_coverage",
        )?;
    }
    Ok(())
}
