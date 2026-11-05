// SPDX-License-Identifier: GPL-3.0-or-later
//! Whole-world content input plans, not executable loaded managers.
use super::*;
use std::collections::BTreeSet;
use std::io::{BufWriter, Write};
use std::sync::LazyLock;
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
fn dependency_ids(
    catalog: &mut VerifiedCatalog,
    spec: &Value,
) -> Result<(Ids, serde_json::Map<String, Value>)> {
    let mut targets = BTreeSet::new();
    for table in spec["tables"].as_object().unwrap().values() {
        for r in table["references"].as_array().unwrap() {
            targets.insert(r[1].as_str().unwrap());
        }
    }
    for file in spec["dbc"].as_array().unwrap() {
        targets.insert(file.as_str().unwrap());
    }
    let mut reports = serde_json::Map::new();
    let mut ids = Ids::new();
    let mut retained = 0usize;
    for target in targets {
        let mut set = BTreeSet::new();
        if target.ends_with(".dbc") {
            let mut view = catalog.dbc_view(target)?;
            reports.insert(target.to_owned(), view.report(None)?);
            for ordinal in 0..view.record_count() {
                let record = view.record(ordinal)?;
                set.insert(record["key"].as_u64().unwrap() as u32);
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
    Ok((ids, reports))
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
    let (ids, dbc_reports) = dependency_ids(&mut catalog, spec)?;
    let mut families = serde_json::Map::new();
    let mut endpoints = Endpoints::new();
    let mut rows = 0u64;
    for (table, entry) in spec["tables"].as_object().unwrap() {
        let mut raw = Sha256::new();
        let mut stream = Stream::new(output, table)?;
        let fields = entry["fields"].as_array().unwrap();
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
            for reference in entry["references"].as_array().unwrap() {
                let field = reference[0].as_str().unwrap();
                let target = reference[1].as_str().unwrap();
                let value = number_field(&row, field);
                if value != 0 {
                    let key = if reference[2] == "abs" {
                        value.abs()
                    } else {
                        value
                    };
                    let present = u32::try_from(key).is_ok_and(|k| ids[target].contains(&k));
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
        rows += catalog.relations[table].rows;
        families.insert(table.clone(),json!({"raw_sha256":hex(&raw.finalize()),"fields":fields,"source":entry["source"],
            "stream":stream.finish()?,"dispositions":counts,"whole_empty":catalog.relations[table].rows == 0}));
    }
    if family == "quests" {
        endpoint_plans(&mut catalog, &schema, &endpoints, &mut plans)?;
    }
    let mut result = json!({"format":"benilla-content-views-v1","family":family,"catalog_sha256":catalog.manifest["catalog_sha256"],
        "policy_sha256":hex(&Sha256::digest(json::canonical(spec))),"families":families,"plans":plans.finish()?,
        "lookups":lookups.finish()?,"rows":rows,"order":"catalog_ordinal_not_F1_cursor","complete":false,"refusals":REFUSALS});
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
