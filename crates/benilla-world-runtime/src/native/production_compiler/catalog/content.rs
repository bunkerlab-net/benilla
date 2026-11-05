// SPDX-License-Identifier: GPL-3.0-or-later
//! Whole-world content input plans, not executable loaded managers.
use super::*;
use std::collections::BTreeSet;
use std::io::{BufWriter, Write};
use std::sync::LazyLock;
#[path = "content_globals.rs"]
mod globals;
#[path = "content_loot.rs"]
mod loot;
static POLICY: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../scripts/classic_export/catalog_content_policy.json"
    )))
    .expect("content policy")
});
const REFUSALS: &[&str] = &[
    "content.reference_execution_missing",
    "content.sql_occurrences_not_F1",
    "content.loaded_manager_validation_pending",
    "content.gameplay_not_admitted",
];
const RELATIONS: &[&str] = &[
    "creature_questrelation",
    "creature_involvedrelation",
    "gameobject_questrelation",
    "gameobject_involvedrelation",
];
type Ids = BTreeMap<String, BTreeSet<u32>>;
type Endpoints = BTreeMap<(String, u32), Vec<Value>>;
type Words = BTreeMap<String, BTreeMap<u32, Vec<u32>>>;
struct Dependencies {
    ids: Ids,
    reports: serde_json::Map<String, Value>,
    words: Words,
    outfit: BTreeMap<u32, Vec<[u32; 2]>>,
}
fn convert(raw: Option<&[u8]>, kind: &str) -> Result<Value> {
    Ok(match kind {
        "raw" => raw.map_or(Value::Null, |r| json!(hex(r))),
        "text" => json!(hex(raw
            .unwrap_or_default()
            .split(|b| *b == 0)
            .next()
            .unwrap_or_default())),
        "f32" => json!(spawns::floating(raw)?.to_bits()),
        "i32" => json!(spawns::uint(raw)? as i32),
        "u32" => json!(spawns::uint(raw)?),
        "u16" => json!(spawns::uint(raw)? as u16),
        "u8" => json!(spawns::uint(raw)? as u8),
        "bool" => json!(u32::from((spawns::uint(raw)? as i32) > 0)),
        "bool8" => json!(u32::from(spawns::uint(raw)? as u8 != 0)),
        _ => return Err(fail("content.conversion")),
    })
}
struct Stream {
    file: Option<BufWriter<std::fs::File>>,
    hash: Sha256,
    rows: u64,
    bytes: u64,
}
impl Stream {
    fn new(output: Option<&Path>, name: &str) -> Result<Self> {
        let file = output
            .map(|p| {
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(p.join(format!("{name}.jsonl")))
                    .map(BufWriter::new)
            })
            .transpose()
            .map_err(|_| fail("content.output"))?;
        Ok(Self {
            file,
            hash: Sha256::new(),
            rows: 0,
            bytes: 0,
        })
    }
    fn add(&mut self, record: &Value) -> Result<()> {
        let mut raw = json::canonical(record);
        raw.push(b'\n');
        check(raw.len() <= FRAME, "content.record_limit")?;
        self.hash.update(&raw);
        self.rows += 1;
        self.bytes += raw.len() as u64;
        if let Some(file) = &mut self.file {
            file.write_all(&raw).map_err(|_| fail("content.output"))?;
        }
        Ok(())
    }
    fn finish(mut self) -> Result<Value> {
        if let Some(file) = &mut self.file {
            file.flush().map_err(|_| fail("content.output"))?;
        }
        Ok(json!({"rows":self.rows,"bytes":self.bytes,"sha256":hex(&self.hash.finalize())}))
    }
}
fn number_field(row: &Value, field: &str) -> i64 {
    row[field].as_i64().expect("policy numeric field")
}
fn quest_edges(quest: i64, row: &Value) -> Vec<Value> {
    let mut result = Vec::new();
    for field in [
        "prevquestid",
        "nextquestid",
        "nextquestinchain",
        "breadcrumbforquestid",
    ] {
        let value = number_field(row, field);
        if value != 0 {
            result.push(json!(["quest_reverse", value.abs(), quest, field, value]));
        }
    }
    let group = number_field(row, "exclusivegroup");
    if group != 0 {
        result.push(json!(["quest_exclusive", group, quest]));
    }
    result
}
fn dependency_ids(catalog: &mut VerifiedCatalog, spec: &Value) -> Result<Dependencies> {
    let mut targets = BTreeSet::new();
    for table in spec["tables"].as_object().unwrap().values() {
        for r in table["references"].as_array().unwrap() {
            targets.insert(r[1].as_str().unwrap());
        }
    }
    for file in spec["dbc"].as_array().unwrap() {
        targets.insert(file.as_str().unwrap());
    }
    if let Some(external) = spec["external_references"].as_array() {
        for reference in external {
            targets.insert(reference[2].as_str().unwrap());
        }
    }
    let mut reports = serde_json::Map::new();
    let mut ids = Ids::new();
    let mut words = Words::new();
    let mut outfit = BTreeMap::new();
    let mut retained = 0usize;
    for target in targets {
        let mut set = BTreeSet::new();
        if target.ends_with(".dbc") {
            let mut view = catalog.dbc_view(target)?;
            reports.insert(target.to_owned(), view.report(None)?);
            for ordinal in 0..view.record_count() {
                let record = view.record(ordinal)?;
                set.insert(record["key"].as_u64().unwrap() as u32);
                let key = record["key"].as_u64().unwrap() as u32;
                if target == "SkillRaceClassInfo.dbc" || target == "SkillTiers.dbc" {
                    let mut values = vec![0u32; if target == "SkillTiers.dbc" { 33 } else { 8 }];
                    for field in record["values"].as_array().unwrap() {
                        values[field[0].as_u64().unwrap() as usize] =
                            field[2].as_u64().unwrap() as u32;
                    }
                    retained += 128 + values.len() * 4;
                    words
                        .entry(target.to_owned())
                        .or_default()
                        .insert(key, values);
                } else if target == "CharStartOutfit.dbc"
                    && key != 0
                    && spec.get("external_references").is_some()
                {
                    let mut items = Vec::new();
                    for field in record["values"].as_array().unwrap() {
                        let column = field[0].as_u64().unwrap() as u32;
                        let value = field[2].as_u64().unwrap() as u32;
                        if (5..=16).contains(&column) && (value as i32) > 0 {
                            items.push([column, value]);
                        }
                    }
                    retained += 128 + items.len() * 8;
                    outfit.insert(key, items);
                }
            }
            retained += set.len() * 64;
            check(retained < DECODED - 8_388_608, "content.view_limit")?;
        } else {
            let key = catalog.relations[target].key_columns[0];
            catalog.scan(target, 0, 256, |row| {
                if set.insert(spawns::uint(row.cells().nth(key).unwrap()?)?) {
                    retained += 64;
                    check(retained < DECODED - 8_388_608, "content.view_limit")?;
                }
                Ok(())
            })?;
        }
        ids.insert(target.to_owned(), set);
    }
    Ok(Dependencies {
        ids,
        reports,
        words,
        outfit,
    })
}
fn endpoint_plans(
    catalog: &mut VerifiedCatalog,
    schema: &Value,
    endpoints: &Endpoints,
    plans: &mut Stream,
) -> Result<()> {
    for namespace in ["creature", "gameobject"] {
        for table in [namespace.to_owned(), format!("{namespace}_spawn_entry")] {
            let cols = schema["tables"][&table]["columns"].as_array().unwrap();
            let gi = cols
                .iter()
                .position(|c| c["COLUMN_NAME"] == "guid")
                .unwrap();
            let ei = cols
                .iter()
                .position(|c| c["COLUMN_NAME"] == if table == namespace { "id" } else { "entry" })
                .unwrap();
            catalog.scan(&table, 0, 256, |view| {
                let guid = spawns::uint(view.cells().nth(gi).unwrap()?)?;
                let entry = spawns::uint(view.cells().nth(ei).unwrap()?)?;
                if let Some(rows) = endpoints.get(&(namespace.to_owned(), entry)) {
                    plans.add(&json!([
                        "quest_endpoint",
                        namespace,
                        guid,
                        entry,
                        table,
                        view.ordinal,
                        rows
                    ]))?;
                }
                Ok(())
            })?;
        }
    }
    Ok(())
}
impl VerifiedCatalog {
    /// Complete key-domain absence remains distinct from whole-empty and empty text.
    pub fn content_lookup_state(&mut self, family: &str, table: &str, key: &[u8]) -> Result<Value> {
        check(
            POLICY["families"][family]["tables"].get(table).is_some(),
            "content.unknown_relation",
        )?;
        let mut states = Vec::new();
        let mut retained = 0usize;
        let mut ordinals = Vec::new();
        self.lookup(table, key, |ordinal| {
            check(
                ordinals.len() * 8 < DECODED - 4_194_304,
                "content.view_limit",
            )?;
            ordinals.push(ordinal);
            Ok(())
        })?;
        let relation = self.relations[table].clone();
        for ordinal in ordinals {
            self.with_row(&relation, ordinal, &mut |view| {
                let row = view
                    .cells()
                    .map(|c| {
                        Ok(match c? {
                            None => "null",
                            Some([]) => "empty",
                            _ => "value",
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                retained += 128 + row.len() * 32;
                check(retained < DECODED - 4_194_304, "content.view_limit")?;
                states.push(row);
                Ok(())
            })?;
        }
        Ok(
            json!({"whole_empty":self.relations[table].rows == 0,"selected_empty":states.is_empty(),"rows":states.len(),"field_states":states}),
        )
    }
}
/// Independently derives complete content report and exact bounded record streams.
pub fn diagnose_catalog_content(root: &Path, family: &str, output: Option<&Path>) -> Result<Value> {
    let spec = POLICY["families"]
        .get(family)
        .ok_or_else(|| fail("content.unknown_family"))?;
    let mut catalog = VerifiedCatalog::open(root, false)?;
    check(
        catalog.manifest["core_revision"] == POLICY["core_revision"],
        "content.source_pin",
    )?;
    let schema = bounded_json(&catalog.object(&descriptor(&catalog.manifest["schema_root"])?, 0)?)?;
    for (table, entry) in spec["tables"].as_object().unwrap() {
        check(
            catalog.relations.contains_key(table),
            "content.missing_relation",
        )?;
        let columns = schema["tables"][table]["columns"]
            .as_array()
            .ok_or_else(|| fail("content.schema"))?;
        let fields = entry["fields"].as_array().unwrap();
        check(
            columns.len() == fields.len()
                && columns.iter().zip(fields).all(|(c, f)| {
                    c["COLUMN_NAME"].as_str().unwrap().to_lowercase() == f[0].as_str().unwrap()
                }),
            "content.schema",
        )?;
    }
    if let Some(output) = output {
        std::fs::create_dir(output).map_err(|_| fail("content.output"))?;
    }
    let mut plans = Stream::new(output, "plans")?;
    let mut lookups = Stream::new(output, "lookups")?;
    let Dependencies {
        ids,
        reports: dbc_reports,
        words,
        outfit,
    } = dependency_ids(&mut catalog, spec)?;
    let mut families = serde_json::Map::new();
    let mut endpoints = Endpoints::new();
    let mut reference_edges = BTreeMap::<u32, Vec<u32>>::new();
    let mut rows = 0u64;
    for (table, entry) in spec["tables"].as_object().unwrap() {
        let mut raw = Sha256::new();
        let mut stream = Stream::new(output, table)?;
        let fields = entry["fields"].as_array().unwrap();
        let mut slots = BTreeMap::<u32, u64>::new();
        let mut service_rows = Vec::<[u64; 3]>::new();
        let mut counts = BTreeMap::<String, u64>::new();
        catalog.scan(table, 0, 256, |view| {
            raw.update(view.frame);
            let cells = view.cells().collect::<Result<Vec<_>>>()?;
            let values = cells
                .iter()
                .zip(fields)
                .map(|(c, f)| convert(*c, f[1].as_str().unwrap()))
                .collect::<Result<Vec<_>>>()?;
            let row = Value::Object(
                fields
                    .iter()
                    .zip(&values)
                    .map(|(f, v)| (f[0].as_str().unwrap().to_owned(), v.clone()))
                    .collect(),
            );
            let mut status = if entry["status"] == "archive-only" {
                "archive-only"
            } else {
                "retained:loader_validation_pending"
            };
            if loot::STORES.contains(&table.as_str()) {
                status = loot::status(&row, &ids["item_template"], &ids["conditions"])?;
            }
            for reference in entry["references"].as_array().unwrap() {
                let field = reference[0].as_str().unwrap();
                let target = reference[1].as_str().unwrap();
                let value = number_field(&row, field);
                if reference[2] == "negative" && value >= 0
                    || reference[2] == "loot_item" && number_field(&row, "mincountorref") <= 0
                {
                    continue;
                }
                if value != 0 {
                    let key = if reference[2] == "abs" || reference[2] == "negative" {
                        value.abs()
                    } else {
                        value
                    };
                    let present = ids[target].contains(&(key as u32));
                    lookups.add(&json!([table, view.ordinal, field, target, key, present]))?;
                    if RELATIONS.contains(&table.as_str()) && field == "quest" && !present {
                        status = "skip:missing_quest";
                    }
                }
            }
            if table == "quest_template" {
                let quest = number_field(&row, "entry");
                for record in quest_edges(quest, &row) {
                    plans.add(&record)?;
                }
                plans.add(&json!([
                    "quest_masks",
                    quest,
                    row["requiredraces"],
                    row["requiredclasses"]
                ]))?;
                for i in 1..=4 {
                    let field = format!("reqcreatureorgoid{i}");
                    let value = number_field(&row, &field);
                    if value != 0 {
                        let target = if value > 0 {
                            "creature_template"
                        } else {
                            "gameobject_template"
                        };
                        lookups.add(&json!([
                            table,
                            view.ordinal,
                            field,
                            target,
                            value.abs(),
                            ids[target].contains(&(value.unsigned_abs() as u32))
                        ]))?;
                    }
                }
            } else if RELATIONS.contains(&table.as_str()) {
                let id = number_field(&row, "id") as u32;
                plans.add(&json!([
                    "quest_relation",
                    table,
                    view.ordinal,
                    id,
                    row["quest"],
                    status
                ]))?;
                let namespace = if table.starts_with("creature") {
                    "creature"
                } else {
                    "gameobject"
                };
                endpoints
                    .entry((namespace.to_owned(), id))
                    .or_default()
                    .push(json!([table, view.ordinal, row["quest"]]));
                check(
                    view.ordinal * 512 < (DECODED - 8_388_608) as u64,
                    "content.view_limit",
                )?;
            } else if table == "item_template" {
                plans.add(&json!([
                    "item_masks",
                    row["entry"],
                    row["allowablerace"],
                    row["allowableclass"]
                ]))?;
                for slot in 1..=5 {
                    let values = [
                        "spellid",
                        "spelltrigger",
                        "spellcharges",
                        "spellppmrate",
                        "spellcooldown",
                        "spellcategory",
                        "spellcategorycooldown",
                    ]
                    .iter()
                    .map(|n| &row[format!("{n}_{slot}")])
                    .collect::<Vec<_>>();
                    plans.add(&json!(["item_spell_slot", row["entry"], slot, values]))?;
                }
            } else if table == "page_text" {
                plans.add(&json!(["page_next_input", row["entry"], row["next_page"]]))?;
            } else if loot::STORES.contains(&table.as_str()) && !status.starts_with("skip:") {
                let entry = number_field(&row, "entry") as u32;
                let count = slots.entry(entry).or_default();
                *count += 1;
                let slot = *count;
                check(slots.len() * 128 < 4_194_304, "content.view_limit")?;
                let group = number_field(&row, "groupid");
                let lane = if group == 0 {
                    "entries"
                } else if number_field(&row, "chanceorquestchance") & 0x7fffffff != 0 {
                    "explicit"
                } else {
                    "equal"
                };
                plans.add(&json!([
                    "loot_slot_input",
                    table,
                    entry,
                    view.ordinal,
                    slot,
                    group,
                    lane,
                    row["mincountorref"],
                    row["maxcount"]
                ]))?;
                if table == "reference_loot_template" && number_field(&row, "mincountorref") < 0 {
                    reference_edges
                        .entry(entry)
                        .or_default()
                        .push((-number_field(&row, "mincountorref")) as u32);
                    check(view.ordinal * 64 < 4_194_304, "content.view_limit")?;
                }
            }
            if family == "globals" && entry["status"] == "input" {
                if table == "playercreateinfo_skills" {
                    let pairs = globals::skill_pairs(
                        &row,
                        &words["SkillRaceClassInfo.dbc"],
                        &words["SkillTiers.dbc"],
                    );
                    plans.add(&json!([
                        "player_skill_input",
                        view.ordinal,
                        row["racemask"],
                        row["classmask"],
                        row["skill"],
                        row["step"],
                        pairs
                    ]))?;
                } else if table.starts_with("player")
                    && row.get("race").is_some()
                    && row.get("class").is_some()
                {
                    plans.add(&json!([
                        "player_pair_input",
                        table,
                        view.ordinal,
                        row["race"],
                        row["class"]
                    ]))?;
                }
                if table == "playercreateinfo_action"
                    && matches!(number_field(&row, "type"), 0 | 128)
                {
                    let target = if number_field(&row, "type") == 0 {
                        "spell_template"
                    } else {
                        "item_template"
                    };
                    let action = number_field(&row, "action") as u32;
                    lookups.add(&json!([
                        table,
                        view.ordinal,
                        "action",
                        target,
                        action,
                        ids[target].contains(&action)
                    ]))?;
                }
                if table == "petcreateinfo_spell" {
                    for slot in 1..=4 {
                        plans.add(&json!([
                            "pet_spell_slot",
                            row["entry"],
                            slot,
                            row[format!("spell{slot}")]
                        ]))?;
                    }
                } else if table == "pet_name_generation" {
                    plans.add(&json!([
                        "pet_name_part",
                        row["entry"],
                        row["half"],
                        view.ordinal,
                        row["word"]
                    ]))?;
                }
                if table.starts_with("locales_") || table == "broadcast_text_locale" {
                    for (field, cell) in fields.iter().zip(&cells) {
                        if field[1] == "text" && field[0] != "locale" {
                            plans.add(&json!([
                                "locale_field",
                                table,
                                view.ordinal,
                                field[0],
                                globals::locale_state(*cell)
                            ]))?;
                        }
                    }
                }
                if table.starts_with("npc_vendor") {
                    service_rows.push([
                        row["entry"].as_u64().unwrap(),
                        row["slot"].as_u64().unwrap(),
                        view.ordinal,
                    ]);
                } else if table.starts_with("npc_trainer") {
                    service_rows.push([
                        row["entry"].as_u64().unwrap(),
                        row["spell"].as_u64().unwrap(),
                        view.ordinal,
                    ]);
                } else if table == "gossip_menu_option" {
                    service_rows.push([
                        row["menu_id"].as_u64().unwrap(),
                        row["id"].as_u64().unwrap(),
                        view.ordinal,
                    ]);
                }
                check(service_rows.len() * 128 <= 8_388_608, "content.view_limit")?;
            }
            *counts.entry(status.to_owned()).or_default() += 1;
            let nulls = cells
                .iter()
                .enumerate()
                .filter_map(|(i, c)| c.is_none().then_some(i))
                .collect::<Vec<_>>();
            stream.add(
                &json!({"ordinal":view.ordinal,"values":values,"nulls":nulls,"disposition":status}),
            )?;
            Ok(())
        })?;
        if table.starts_with("npc_vendor") {
            let (ordered, ties) = globals::service_order(service_rows)?;
            for [entry, slot, ordinal] in ordered {
                plans.add(&json!(["service_slot_input", table, entry, slot, ordinal]))?;
            }
            for tie in ties {
                plans.add(&json!([
                    "service_slot_tie_unproven",
                    table,
                    tie[0],
                    tie[1],
                    tie[2]
                ]))?;
            }
        } else if table.starts_with("npc_trainer") || table == "gossip_menu_option" {
            service_rows.sort_unstable();
            for [a, b, ordinal] in service_rows {
                plans.add(&json!(["service_key_order", table, a, b, ordinal]))?;
            }
        }
        rows += catalog.relations[table].rows;
        families.insert(table.clone(),json!({"raw_sha256":hex(&raw.finalize()),"fields":fields,"source":entry["source"],
            "stream":stream.finish()?,"dispositions":counts,"whole_empty":catalog.relations[table].rows == 0}));
    }
    if family == "quests" {
        endpoint_plans(&mut catalog, &schema, &endpoints, &mut plans)?;
    }
    if family == "loot" {
        loot::check_cycles(&reference_edges)?;
        for (entry, targets) in &reference_edges {
            plans.add(&json!(["loot_reference_edges", entry, targets]))?;
        }
    }
    if family == "globals" {
        for reference in spec["external_references"].as_array().unwrap() {
            let table = reference[0].as_str().unwrap();
            let field = reference[1].as_str().unwrap();
            let target = reference[2].as_str().unwrap();
            let columns = schema["tables"][table]["columns"].as_array().unwrap();
            let index = columns
                .iter()
                .position(|c| c["COLUMN_NAME"].as_str().unwrap().to_lowercase() == field)
                .ok_or_else(|| fail("content.schema"))?;
            catalog.scan(table, 0, 256, |view| {
                let key = spawns::uint(view.cells().nth(index).unwrap()?)?;
                if key != 0 {
                    lookups.add(&json!([
                        table,
                        view.ordinal,
                        field,
                        target,
                        key,
                        ids[target].contains(&key)
                    ]))?;
                }
                Ok(())
            })?;
        }
        for (key, items) in outfit {
            for [column, item] in items {
                lookups.add(&json!([
                    "CharStartOutfit.dbc",
                    key,
                    column,
                    "item_template",
                    item,
                    ids["item_template"].contains(&item)
                ]))?;
            }
        }
    }
    let mut result = json!({"format":"benilla-content-views-v1","family":family,"catalog_sha256":catalog.manifest["catalog_sha256"],
        "policy_sha256":hex(&Sha256::digest(json::canonical(spec))),"families":families,"plans":plans.finish()?,
        "lookups":lookups.finish()?,"rows":rows,"order":"catalog_ordinal_not_F1_cursor","complete":false,"refusals":REFUSALS});
    if let Some(extra) = spec["refusals"].as_array() {
        result["refusals"]
            .as_array_mut()
            .unwrap()
            .extend(extra.iter().cloned());
    }
    if !spec["dbc"].as_array().unwrap().is_empty() {
        result["dbc_dependencies"] = Value::Object(dbc_reports);
    }
    result["view_sha256"] = json!(hex(&Sha256::digest(json::canonical(&result))));
    check(
        json::canonical(&result).len() <= CHUNK,
        "content.report_limit",
    )?;
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_reverse_relations_and_breadcrumbs_keep_direction() {
        let row = json!({"prevquestid":-2,"nextquestid":3,"nextquestinchain":4,"breadcrumbforquestid":5,"exclusivegroup":-8});
        assert_eq!(
            quest_edges(1, &row),
            vec![
                json!(["quest_reverse", 2, 1, "prevquestid", -2]),
                json!(["quest_reverse", 3, 1, "nextquestid", 3]),
                json!(["quest_reverse", 4, 1, "nextquestinchain", 4]),
                json!(["quest_reverse", 5, 1, "breadcrumbforquestid", 5]),
                json!(["quest_exclusive", -8, 1])
            ]
        );
    }
    #[test]
    fn raw_null_and_empty_text_remain_distinct() {
        assert_eq!(convert(None, "raw").unwrap(), Value::Null);
        assert_eq!(convert(Some(b""), "raw").unwrap(), json!(""));
        assert_eq!(convert(Some(b"x\0tail"), "text").unwrap(), json!("78"));
        assert_eq!(convert(Some(b"-1"), "i32").unwrap(), json!(-1));
        assert_eq!(convert(Some(b"-0"), "f32").unwrap(), json!(0x80000000u32));
    }
}
