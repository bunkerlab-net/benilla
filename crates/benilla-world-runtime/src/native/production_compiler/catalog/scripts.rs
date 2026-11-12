// SPDX-License-Identifier: GPL-3.0-or-later
//! Complete script inputs, never executable manager state or admission.
use super::*;
use std::collections::{BTreeSet, VecDeque};
const SLOTS: &[&str] = &[
    "pGossipHello",
    "pGossipHelloGO",
    "pGossipSelect",
    "pGossipSelectGO",
    "pGossipSelectWithCode",
    "pGossipSelectGOWithCode",
    "pDialogStatusNPC",
    "pDialogStatusGO",
    "pQuestAcceptNPC",
    "pQuestAcceptGO",
    "pQuestAcceptItem",
    "pQuestRewardedNPC",
    "pQuestRewardedGO",
    "pGOUse",
    "pItemUse",
    "pItemLoot",
    "pAreaTrigger",
    "pProcessEventId",
    "pTrapSearching",
    "GetGameObjectAI",
    "GetAI",
    "GetInstanceData",
];
const EXTRA_TABLES: &[&str] = &[
    "dbscript_random_templates",
    "dbscript_string",
    "string_id",
    "creature_ai_scripts",
    "creature_ai_summons",
    "creature_ai_texts",
    "script_texts",
    "custom_texts",
    "gossip_texts",
    "script_waypoint",
    "scripted_areatrigger",
    "scripted_event_id",
    "spell_scripts",
    "broadcast_text",
    "broadcast_text_locale",
    "world_template",
    "instance_template",
];
const EXTRA_LOADERS: &[&str] = &[
    "DBScripts/ScriptMgr:1164",
    "DBScripts/ScriptMgr:1230",
    "AI/EventAI/CreatureEventAIMgr:211",
    "AI/EventAI/CreatureEventAIMgr:41",
    "AI/ScriptDevAI/ScriptDevAIMgr:480",
    "AI/ScriptDevAI/ScriptDevAIMgr:550",
    "AI/ScriptDevAI/ScriptDevAIMgr:593",
    "AI/ScriptDevAI/system/system:68",
    "AI/ScriptDevAI/system/system:75",
    "spell-scripts",
    "Globals/ObjectMgr:7786",
    "Globals/ObjectMgr:7834",
];
const KINDS: &[&str] = &[
    "script_registration",
    "script_registration_site",
    "script_callback_binding",
    "script_binding",
    "script_registry_store",
];
fn columns<'a>(schema: &'a Value, table: &str) -> Result<&'a [Value]> {
    schema["tables"][table]["columns"]
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| fail("catalog.schema"))
}
fn position(cols: &[Value], name: &str) -> Result<usize> {
    cols.iter()
        .position(|c| {
            c["COLUMN_NAME"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case(name))
        })
        .ok_or_else(|| fail("catalog.schema"))
}
fn cells<'a>(row: &RowView<'a>) -> Result<[Option<&'a [u8]>; 256]> {
    let mut result = [None; 256];
    for (i, c) in row.cells().enumerate() {
        result[i] = c?;
    }
    Ok(result)
}
fn hash_cells<'a>(values: impl IntoIterator<Item = Option<&'a [u8]>>) -> [u8; 32] {
    let mut hash = Sha256::new();
    for value in values {
        hash.update(value.map_or(u32::MAX, |v| v.len() as u32).to_le_bytes());
        if let Some(value) = value {
            hash.update(value);
        }
    }
    hash.finalize().into()
}
fn signed(raw: Option<&[u8]>) -> Result<i32> {
    Ok(spawns::uint(raw)? as i32)
}
fn db_tables() -> Vec<String> {
    catalog_schema::POLICY["tables"]
        .as_object()
        .expect("policy")
        .keys()
        .filter(|k| k.starts_with("dbscripts_on_"))
        .cloned()
        .collect()
}
fn loaders(tables: &[String]) -> Vec<String> {
    tables
        .iter()
        .map(|t| format!("dbscript:{t}"))
        .chain(EXTRA_LOADERS.iter().map(|s| (*s).to_owned()))
        .collect()
}
type OrderedHash = (Vec<u32>, [u8; 32]);
fn expected(
    catalog: &mut VerifiedCatalog,
    schema: &Value,
    spec: &Value,
) -> Result<(usize, Vec<OrderedHash>)> {
    let query = spec["query"]
        .as_str()
        .ok_or_else(|| fail("scripts.query"))?;
    let table = spec["relations"][0]
        .as_str()
        .ok_or_else(|| fail("scripts.query"))?;
    let mut output = Vec::new();
    if query.contains("UNION") {
        let mut names = BTreeSet::new();
        for relation in spec["relations"]
            .as_array()
            .ok_or_else(|| fail("scripts.query"))?
        {
            let table = relation.as_str().ok_or_else(|| fail("scripts.query"))?;
            let index = position(columns(schema, table)?, "scriptname")?;
            catalog.scan(table, 0, 256, |row| {
                if let Some(value) = cells(&row)?[index].filter(|v| !v.is_empty()) {
                    check(value.is_ascii(), "scripts.name_collation")?;
                    names.insert(value.to_vec());
                    check(names.len() * 256 <= DECODED, "scripts.view_limit")?;
                }
                Ok(())
            })?;
        }
        let normalized: BTreeSet<_> = names
            .iter()
            .map(|v| v.trim_ascii_end().to_ascii_lowercase())
            .collect();
        check(normalized.len() == names.len(), "scripts.name_collation")?;
        for name in names {
            output.push((Vec::new(), hash_cells([Some(name.as_slice())])));
        }
        return Ok((1, output));
    }
    let cols = columns(schema, table)?;
    if query.contains("COUNT(Entry)") {
        let index = position(cols, "entry")?;
        let mut counts = BTreeMap::<Vec<u8>, u64>::new();
        catalog.scan(table, 0, 256, |row| {
            if let Some(value) = cells(&row)?[index] {
                *counts.entry(value.to_vec()).or_default() += 1;
            }
            check(counts.len() * 256 <= DECODED, "scripts.view_limit")?;
            Ok(())
        })?;
        for count in counts.values() {
            output.push((Vec::new(), hash_cells([Some(count.to_string().as_bytes())])));
        }
        return Ok((1, output));
    }
    let projection = query
        .strip_prefix("SELECT ")
        .and_then(|v| v.split_once(" FROM "))
        .ok_or_else(|| fail("scripts.query"))?
        .0;
    let indexes = projection
        .split(',')
        .map(|n| position(cols, n.trim()))
        .collect::<Result<Vec<_>>>()?;
    let ordering = query
        .split_once(" ORDER BY ")
        .map_or(Ok(Vec::new()), |(_, order)| {
            order
                .split(',')
                .map(|n| position(cols, n.trim()))
                .collect::<Result<Vec<_>>>()
        })?;
    catalog.scan(table, 0, 256, |row| {
        let values = cells(&row)?;
        check(
            (output.len() + 1) * (if ordering.is_empty() { 320 } else { 768 }) + 4_194_304
                <= DECODED,
            "scripts.view_limit",
        )?;
        output.push((
            ordering
                .iter()
                .map(|&i| spawns::uint(values[i]))
                .collect::<Result<_>>()?,
            hash_cells(indexes.iter().map(|&i| values[i])),
        ));
        Ok(())
    })?;
    Ok((indexes.len(), output))
}
fn verify_stream(
    path: &Path,
    columns: usize,
    mut expected: Vec<OrderedHash>,
    d: &Value,
) -> Result<Value> {
    let mut keys = BTreeMap::<[u8; 32], VecDeque<Vec<u32>>>::new();
    expected.sort_unstable();
    let ordered = expected.first().is_some_and(|(order, _)| !order.is_empty());
    if ordered {
        for (order, hash) in &expected {
            keys.entry(*hash).or_default().push_back(order.clone());
        }
    }
    let held = reader::Held::open(path, MAX_RAW)?;
    let mut buffer = vec![0; FRAME];
    let mut offset = 0;
    let mut hash = Sha256::new();
    let mut seen = Vec::new();
    let mut previous = Vec::new();
    for ordinal in 0..expected.len() {
        check(offset + 4 <= held.len(), "scripts.exhausted")?;
        held.read_into(offset, &mut buffer[..4])?;
        let size = le(&buffer, 0, 4)? as usize;
        check((16..=FRAME).contains(&size), "catalog.frame_limit")?;
        held.read_into(offset + 4, &mut buffer[4..size])?;
        let row = RowView::new(&buffer[..size], ordinal as u64, columns)?;
        row.validate()?;
        let h = hash_cells(cells(&row)?[..columns].iter().copied());
        if ordered {
            let order = keys
                .get_mut(&h)
                .and_then(VecDeque::pop_front)
                .ok_or_else(|| fail("scripts.view_mismatch"))?;
            check(previous <= order, "scripts.order")?;
            previous = order;
        }
        seen.push(h);
        hash.update(&buffer[..size]);
        offset += size as u64;
    }
    check(offset == held.len(), "scripts.trailing")?;
    let occurrence = hex(&hash.finalize());
    check(
        d["rows"] == seen.len() as u64 && d["bytes"] == held.len() && d["sha256"] == occurrence,
        "scripts.hash",
    )?;
    seen.sort_unstable();
    if !ordered {
        check(
            seen.iter().copied().eq(expected.iter().map(|(_, h)| *h)),
            "scripts.order",
        )?;
    }
    let mut multiset = Sha256::new();
    for h in seen {
        multiset.update(h);
    }
    Ok(
        json!({"rows":expected.len(),"occurrence_sha256":occurrence,"multiset_sha256":hex(&multiset.finalize())}),
    )
}
fn summaries(
    catalog: &mut VerifiedCatalog,
    schema: &Value,
    tables: &[String],
) -> Result<(Value, Value)> {
    let mut db = serde_json::Map::new();
    for table in tables {
        let cols = columns(schema, table)?;
        let command = position(cols, "command")?;
        let priority = position(cols, "priority")?;
        let mut commands = BTreeMap::<String, u64>::new();
        let mut priorities = BTreeMap::<u32, u64>::new();
        catalog.scan(table, 0, 256, |row| {
            let values = cells(&row)?;
            *commands
                .entry(spawns::uint(values[command])?.to_string())
                .or_default() += 1;
            *priorities
                .entry(spawns::uint(values[priority])?)
                .or_default() += 1;
            check(priorities.len() * 128 <= DECODED, "scripts.view_limit")
        })?;
        db.insert(table.clone(),json!({"commands":commands,"equal_priority_groups":priorities.values().filter(|&&v| v>1).count(),"equal_priority_rows":priorities.values().filter(|&&v| v>1).sum::<u64>()}));
    }
    let cols = columns(schema, "creature_ai_scripts")?;
    let owner = position(cols, "creature_id")?;
    let mut result = BTreeMap::<&str, u64>::from_iter(
        [
            "entry_selectors",
            "guid_selectors",
            "zero_selectors",
            "extreme_selectors",
            "positive_text_parameters",
            "negative_text_parameters",
            "other_action_slots",
        ]
        .map(|k| (k, 0)),
    );
    let mut actions = Vec::new();
    for a in 1..=3 {
        actions.push((
            position(cols, &format!("action{a}_type"))?,
            (1..=3)
                .map(|p| position(cols, &format!("action{a}_param{p}")))
                .collect::<Result<Vec<_>>>()?,
        ));
    }
    catalog.scan("creature_ai_scripts", 0, 256, |row| {
        let values = cells(&row)?;
        let selector = signed(values[owner])?;
        *result
            .get_mut(if selector > 0 {
                "entry_selectors"
            } else if selector < 0 {
                "guid_selectors"
            } else {
                "zero_selectors"
            })
            .expect("key") += 1;
        *result.get_mut("extreme_selectors").expect("key") += u64::from(selector == i32::MIN);
        for (kind, params) in &actions {
            match spawns::uint(values[*kind])? {
                0 => (),
                1 => {
                    for &index in params {
                        let n = signed(values[index])?;
                        if n != 0 {
                            *result
                                .get_mut(if n > 0 {
                                    "positive_text_parameters"
                                } else {
                                    "negative_text_parameters"
                                })
                                .expect("key") += 1;
                        }
                    }
                }
                _ => *result.get_mut("other_action_slots").expect("key") += 1,
            }
        }
        Ok(())
    })?;
    Ok((Value::Object(db), json!(result)))
}
fn input_bytes(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let held = reader::Held::open(path, limit as u64)?;
    let mut bytes = vec![0; held.len() as usize];
    for (i, chunk) in bytes.chunks_mut(FRAME).enumerate() {
        held.read_into((i * FRAME) as u64, chunk)?;
    }
    Ok(bytes)
}
fn input_json(path: &Path, limit: usize) -> Result<Value> {
    json::decode_with_limits(&input_bytes(path, limit)?, limit, 32, 150_000)
}
fn path_value(v: &Value) -> Result<&Path> {
    v.as_str()
        .map(Path::new)
        .ok_or_else(|| fail("scripts.registration_input"))
}
fn hash_file(path: &Path, limit: u64) -> Result<Value> {
    let held = reader::Held::open(path, limit)?;
    let mut bytes = vec![0; FRAME];
    let mut hash = Sha256::new();
    let mut offset = 0;
    while offset < held.len() {
        let size = (held.len() - offset).min(FRAME as u64) as usize;
        held.read_into(offset, &mut bytes[..size])?;
        hash.update(&bytes[..size]);
        offset += size as u64;
    }
    Ok(json!({"bytes":held.len(),"sha256":hex(&hash.finalize())}))
}
fn identity(v: &Value) -> Value {
    json!({"bytes":v["bytes"],"sha256":v["sha256"]})
}
#[derive(Default)]
struct Attempt {
    name: String,
    thread: String,
    operation: Option<String>,
    slots: BTreeMap<String, String>,
    source: Value,
    report_error: bool,
    outcome: &'static str,
    id: Option<u32>,
    stored: bool,
}
#[derive(Default)]
struct Registry {
    current: Option<Attempt>,
    bindings: BTreeMap<u32, u64>,
    slots: BTreeMap<String, u64>,
    counts: BTreeMap<&'static str, u64>,
    raw: Sha256,
    records: Vec<Value>,
    charge: usize,
}
fn decimal(v: &str) -> Result<u32> {
    v.parse().map_err(|_| fail("scripts.registry_frame"))
}
fn registry_outcome(kind: &str, duplicate: bool, replace: bool) -> Result<&'static str> {
    check(
        ["scriptdev", "creature_ai", "motion", "spell", "aura"].contains(&kind),
        "scripts.registry_kind",
    )?;
    check(
        !replace || !["spell", "aura"].contains(&kind),
        "scripts.registry_replace_not_supported",
    )?;
    Ok(if duplicate && kind != "scriptdev" && !replace {
        "rejected_duplicate"
    } else if duplicate {
        "accepted_override"
    } else {
        "accepted"
    })
}
fn pointer(v: &str) -> Result<u64> {
    if v == "(nil)" {
        Ok(0)
    } else {
        u64::from_str_radix(v.strip_prefix("0x").unwrap_or(v), 16)
            .map_err(|_| fail("scripts.registry_frame"))
    }
}
impl Registry {
    fn close(&mut self) -> Result<()> {
        if let Some(c) = self.current.take() {
            check(c.slots.len() == SLOTS.len(), "scripts.registry_slots")?;
            check(c.stored == c.id.is_some(), "scripts.registry_store")?;
            let record = json!({"name":c.name,"id":c.id,"operation":c.operation,"source":c.source,
                "slots":c.slots,"report_error":c.report_error,
                "outcome":if c.stored {c.outcome} else {"unobserved_binding_outcome"}});
            *self.counts.entry("unresolved_attempts").or_default() += u64::from(c.id.is_none());
            self.charge += 4096 + 4 * json::canonical(&record).len();
            check(self.charge <= DECODED, "scripts.view_limit")?;
            self.records.push(record);
        }
        Ok(())
    }
    fn feed(&mut self, raw: &[u8]) -> Result<()> {
        let text = std::str::from_utf8(raw).map_err(|_| fail("scripts.registry_frame"))?;
        let parts: Vec<_> = text
            .strip_suffix('\n')
            .ok_or_else(|| fail("scripts.registry_frame"))?
            .splitn(4, '\t')
            .collect();
        check(parts.len() == 4, "scripts.registry_frame")?;
        let kind = parts[2];
        if !KINDS.contains(&kind) {
            return Ok(());
        }
        self.raw.update(raw);
        let words: Vec<_> = parts[3].split(' ').collect();
        if kind == "script_registration" {
            self.close()?;
            check(
                words.len() == 2 && matches!(words[1], "0" | "1"),
                "scripts.registry_frame",
            )?;
            self.current = Some(Attempt {
                name: words[0].to_owned(),
                thread: parts[0].to_owned(),
                report_error: words[1] == "1",
                ..Attempt::default()
            });
            *self.counts.entry("attempts").or_default() += 1;
            return Ok(());
        }
        let c = self
            .current
            .as_mut()
            .ok_or_else(|| fail("scripts.registry_frame"))?;
        check(c.thread == parts[0], "scripts.registry_frame")?;
        match kind {
            "script_registration_site" => {
                check(
                    c.operation.is_none()
                        && words.len() >= 4
                        && words[1].starts_with("/reference/source/src/"),
                    "scripts.registry_site",
                )?;
                c.operation = Some(words[0].to_owned());
                c.source = json!({"file":words[1],"line":decimal(words[2])?,"function":words[3..].join(" ")});
            }
            "script_callback_binding" => {
                check(
                    words.len() == 3
                        && c.operation.as_deref() == Some(words[0])
                        && SLOTS.contains(&words[1])
                        && !c.slots.contains_key(words[1]),
                    "scripts.registry_slots",
                )?;
                c.slots
                    .insert(words[1].to_owned(), format!("{:x}", pointer(words[2])?));
                *self.slots.entry(words[1].to_owned()).or_default() += 1;
                *self.counts.entry("callback_slots").or_default() += 1;
                *self.counts.entry("nonnull_callbacks").or_default() +=
                    u64::from(pointer(words[2])? != 0);
            }
            "script_binding" => {
                check(
                    words.len() == 2 && c.id.is_none() && words[1] == c.name,
                    "scripts.registry_store",
                )?;
                let id = decimal(words[0])?;
                c.id = Some(id);
                check(id != 0, "scripts.registry_store")?;
            }
            "script_registry_store" => {
                check(
                    words.len() == 4 && c.id.is_some() && !c.stored,
                    "scripts.registry_store",
                )?;
                let id = decimal(words[0])?;
                check(c.id == Some(id) && id != 0, "scripts.registry_store")?;
                let old = pointer(words[1])?;
                check(
                    old == self.bindings.get(&id).copied().unwrap_or(0),
                    "scripts.registry_store",
                )?;
                check(
                    u64::from(decimal(words[3])?)
                        == self.counts.get("accepted").copied().unwrap_or(0),
                    "scripts.registry_store",
                )?;
                let new = pointer(words[2])?;
                check(new != 0, "scripts.registry_store")?;
                self.bindings.insert(id, new);
                c.stored = true;
                c.outcome = registry_outcome("scriptdev", old != 0, true)?;
                *self.counts.entry("accepted_overrides").or_default() += u64::from(old != 0);
                *self.counts.entry("accepted").or_default() += 1;
            }
            _ => unreachable!(),
        }
        Ok(())
    }
    fn finish(mut self) -> Result<Value> {
        self.close()?;
        check(
            self.counts.get("attempts").copied().unwrap_or(0) > 0,
            "scripts.registry_slots",
        )?;
        let mut result = serde_json::Map::new();
        for k in [
            "attempts",
            "accepted",
            "accepted_overrides",
            "unresolved_attempts",
            "callback_slots",
            "nonnull_callbacks",
        ] {
            result.insert(k.into(), json!(self.counts.get(k).copied().unwrap_or(0)));
        }
        result.insert("final_bindings".into(), json!(self.bindings.len()));
        result.insert("slots".into(), json!(self.slots));
        result.insert(
            "raw_registry_sha256".into(),
            json!(hex(&self.raw.finalize())),
        );
        result.insert("observed_registries".into(), json!(["scriptdev"]));
        result.insert(
            "missing_registries".into(),
            json!(["creature_ai", "motion", "spell", "aura"]),
        );
        result.insert("records".into(), json!(self.records));
        Ok(Value::Object(result))
    }
}
pub(super) fn registration_input(path: &Path) -> Result<Value> {
    let value = input_json(path, CHUNK)?;
    catalog_schema::fields(
        &value,
        &[
            "format",
            "manifest",
            "trace",
            "build",
            "definitions",
            "linked",
        ],
        "scripts.registration_input",
    )?;
    check(
        value["format"] == "benilla-registration-input-v1",
        "scripts.registration_input",
    )?;
    catalog_schema::fields(
        &value["manifest"],
        &["path", "bytes", "sha256"],
        "scripts.registration_input",
    )?;
    let manifest_raw = input_bytes(path_value(&value["manifest"]["path"])?, 2 * CHUNK)?;
    let manifest = json::decode_with_limits(&manifest_raw, 2 * CHUNK, 32, 150_000)?;
    check(
        manifest["core_revision"] == catalog_schema::POLICY["core_revision"]
            && manifest["classic_db_revision"] == catalog_schema::POLICY["database_revision"],
        "scripts.linked_source",
    )?;
    check(
        manifest["reference_execution_sealed"] == false,
        "scripts.reference_manifest",
    )?;
    check(
        value["manifest"]["bytes"] == manifest_raw.len()
            && value["manifest"]["sha256"] == hex(&Sha256::digest(&manifest_raw)),
        "scripts.input_hash",
    )?;
    let files = manifest["files"]
        .as_array()
        .ok_or_else(|| fail("scripts.reference_manifest"))?;
    for name in ["trace", "build", "definitions"] {
        let d = &value[name];
        catalog_schema::fields(
            d,
            &["path", "relative", "bytes", "sha256"],
            "scripts.registration_input",
        )?;
        let matches: Vec<_> = files
            .iter()
            .filter(|f| f["path"] == d["relative"])
            .collect();
        check(
            matches.len() == 1 && identity(matches[0]) == identity(d),
            "scripts.input_hash",
        )?;
    }
    check(
        hash_file(
            path_value(&value["definitions"]["path"])?,
            (2 * CHUNK) as u64,
        )? == identity(&value["definitions"]),
        "scripts.linked_source",
    )?;
    let build_raw = input_bytes(path_value(&value["build"]["path"])?, CHUNK)?;
    check(
        value["build"]["bytes"] == build_raw.len()
            && value["build"]["sha256"] == hex(&Sha256::digest(&build_raw)),
        "scripts.linked_source",
    )?;
    let build = json::decode_with_limits(&build_raw, CHUNK, 32, 100_000)?;
    catalog_schema::fields(
        &value["linked"],
        &["executable", "observer"],
        "scripts.registration_input",
    )?;
    let mut linked = Vec::new();
    for d in build["files"]
        .as_array()
        .ok_or_else(|| fail("scripts.linked_source"))?
    {
        let path = d["path"]
            .as_str()
            .ok_or_else(|| fail("scripts.linked_source"))?;
        if path.ends_with("/mangosd") || path.ends_with("/transport-observer.so") {
            let expected = identity(d);
            let local = &value["linked"][if path.ends_with("/mangosd") {
                "executable"
            } else {
                "observer"
            }];
            catalog_schema::fields(
                local,
                &["path", "bytes", "sha256"],
                "scripts.registration_input",
            )?;
            check(
                identity(local) == expected
                    && hash_file(path_value(&local["path"])?, 67_108_864)? == expected,
                "scripts.linked_source",
            )?;
            linked.push(expected);
        }
    }
    check(linked.len() == 2, "scripts.linked_source")?;
    let held = reader::Held::open(path_value(&value["trace"]["path"])?, 8_589_934_592)?;
    check(value["trace"]["bytes"] == held.len(), "scripts.input_limit")?;
    let mut registry = Registry::default();
    let mut buffer = vec![0; FRAME];
    let mut line = Vec::with_capacity(FRAME);
    let mut offset = 0;
    let mut hash = Sha256::new();
    let mut records = 0u64;
    while offset < held.len() {
        let size = (held.len() - offset).min(FRAME as u64) as usize;
        held.read_into(offset, &mut buffer[..size])?;
        hash.update(&buffer[..size]);
        let mut start = 0;
        for (i, &byte) in buffer[..size].iter().enumerate() {
            if byte == b'\n' {
                check(
                    line.len() + i + 1 - start <= FRAME,
                    "scripts.registry_frame",
                )?;
                let raw = if line.is_empty() {
                    &buffer[start..=i]
                } else {
                    line.extend_from_slice(&buffer[start..=i]);
                    &line
                };
                if raw.windows(8).any(|w| w == b"\tscript_") {
                    registry.feed(raw)?;
                }
                line.clear();
                records += 1;
                start = i + 1;
            }
        }
        check(line.len() + size - start <= FRAME, "scripts.registry_frame")?;
        line.extend_from_slice(&buffer[start..size]);
        offset += size as u64;
    }
    check(line.is_empty(), "scripts.registry_frame")?;
    check(
        value["trace"]["sha256"] == hex(&hash.finalize()),
        "scripts.input_hash",
    )?;
    let input: serde_json::Map<String, Value> = ["manifest", "trace", "build", "definitions"]
        .into_iter()
        .map(|k| (k.to_owned(), identity(&value[k])))
        .collect();
    Ok(
        json!({"input":input,"linked_binaries":linked,"trace_records":records,"registry":registry.finish()?,"definitions_status":"bound_F1_receipt_not_native_callback_execution"}),
    )
}
/// Independently scans every script relation and binds the observed F1 registry as input.
pub fn diagnose_catalog_scripts(
    root: &Path,
    occurrences: &Path,
    registration: &Path,
) -> Result<Value> {
    let registration = registration_input(registration)?;
    let mut catalog = VerifiedCatalog::open(root, false)?;
    prepare(&mut catalog, occurrences, registration)
}
pub(super) fn prepare(
    catalog: &mut VerifiedCatalog,
    occurrences: &Path,
    mut registration: Value,
) -> Result<Value> {
    let schema = bounded_json(&catalog.object(&descriptor(&catalog.manifest["schema_root"])?, 0)?)?;
    let meta = bounded_json(&input_bytes(&occurrences.join("occurrences.json"), CHUNK)?)?;
    catalog_schema::fields(
        &meta,
        &["format", "streams", "complete", "source_snapshot"],
        "scripts.occurrences",
    )?;
    check(
        meta["format"] == "benilla-script-occurrences-v1"
            && meta["complete"] == false
            && meta["source_snapshot"] == "phase0-sequential-diagnostic-not-F1",
        "scripts.occurrences",
    )?;
    let db_tables = db_tables();
    let ids = loaders(&db_tables);
    let keys: Vec<_> = (0..ids.len()).map(|i| i.to_string()).collect();
    catalog_schema::fields(
        &meta["streams"],
        &keys.iter().map(String::as_str).collect::<Vec<_>>(),
        "scripts.occurrences",
    )?;
    let mut streams = serde_json::Map::new();
    for (i, id) in ids.iter().enumerate() {
        let spec = &catalog_schema::POLICY["loaders"][id];
        let key = i.to_string();
        let d = &meta["streams"][&key];
        catalog_schema::fields(d, &["query", "rows", "bytes", "sha256"], "scripts.query")?;
        check(d["query"] == spec["query"], "scripts.query")?;
        check(
            d["rows"].as_u64().is_some_and(|n| n <= 2_000_000)
                && d["bytes"].as_u64().is_some_and(|n| n <= MAX_RAW),
            "scripts.hash",
        )?;
        let (cols, expected) = expected(catalog, &schema, spec)?;
        let mut result =
            verify_stream(&occurrences.join(format!("{key}.rows")), cols, expected, d)?;
        result["query"] = spec["query"].clone();
        result["source"] = spec["source"].clone();
        streams.insert(key, result);
    }
    let tables: BTreeSet<_> = db_tables
        .iter()
        .map(String::as_str)
        .chain(EXTRA_TABLES.iter().copied())
        .collect();
    let mut families = serde_json::Map::new();
    for table in tables {
        let mut hash = Sha256::new();
        let mut bits = Sha256::new();
        let mut count = 0u64;
        let floats: Vec<_> = columns(&schema, table)?
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                matches!(
                    c["DATA_TYPE"].as_str(),
                    Some("float" | "double" | "decimal")
                )
            })
            .map(|(i, _)| i)
            .collect();
        catalog.scan(table, 0, 256, |row| {
            hash.update(row.frame);
            let values = cells(&row)?;
            for &i in &floats {
                bits.update(row.ordinal.to_le_bytes());
                bits.update((i as u32).to_le_bytes());
                bits.update(spawns::floating(values[i])?.to_bits().to_le_bytes());
            }
            count += 1;
            Ok(())
        })?;
        families.insert(table.into(),json!({"rows":count,"raw_sha256":hex(&hash.finalize()),"float_bits_sha256":hex(&bits.finalize())}));
    }
    let (db, event) = summaries(catalog, &schema, &db_tables)?;
    let spec = &catalog_schema::POLICY["loaders"]["AI/ScriptDevAI/ScriptDevAIMgr:480"];
    let mut names = BTreeSet::new();
    for relation in spec["relations"]
        .as_array()
        .ok_or_else(|| fail("scripts.query"))?
    {
        let table = relation.as_str().ok_or_else(|| fail("scripts.query"))?;
        let index = position(columns(&schema, table)?, "scriptname")?;
        catalog.scan(table, 0, 256, |row| {
            if let Some(name) = cells(&row)?[index].filter(|v| !v.is_empty()) {
                names.insert(
                    std::str::from_utf8(name)
                        .map_err(|_| fail("scripts.name_collation"))?
                        .to_owned(),
                );
            }
            Ok(())
        })?;
    }
    let name_ids: BTreeMap<_, _> = names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.as_str(), i + 1))
        .collect();
    for record in registration["registry"]["records"]
        .as_array()
        .ok_or_else(|| fail("scripts.registry_frame"))?
    {
        let name = record["name"]
            .as_str()
            .ok_or_else(|| fail("scripts.registry_frame"))?;
        check(
            record["id"].is_null() || record["id"] == name_ids.get(name).copied().unwrap_or(0),
            "scripts.registration_names_drift",
        )?;
    }
    registration["sql_name_count"] = json!(names.len());
    registration["unbound_absent_catalog_names"] = json!(registration["registry"]["records"]
        .as_array()
        .expect("checked records")
        .iter()
        .filter(|r| r["id"].is_null()
            && !name_ids.contains_key(r["name"].as_str().expect("checked name")))
        .count());
    let contract: &Value = &super::super::global_hooks::CONTRACT;
    let mut result = json!({"format":"benilla-script-views-v1","catalog_sha256":catalog.manifest["catalog_sha256"],"families":families,"loaders":streams,"db_scripts":db,"event_ai":event,"registrations":registration,"legacy_global_hooks_contract_sha256":json::digest(contract),"complete":false,"refusals":["scripts.reference_execution_unsealed","scripts.sql_occurrences_not_F1","scripts.equal_priority_F1_order_missing","scripts.creature_ai_registry_missing","scripts.motion_registry_missing","scripts.spell_registry_missing","scripts.aura_registry_missing","scripts.callback_dispatch_coverage_incomplete","scripts.rejected_attempt_outcome_unobserved","scripts.linked_source_proof_incomplete","scripts.loader_validation_execution_pending","scripts.gameplay_not_admitted"]});
    result["view_sha256"] = json!(json::digest(&result));
    check(
        json::canonical(&result).len() <= CHUNK,
        "scripts.report_limit",
    )?;
    Ok(result)
}

/// Raw narrow template operands. This is not effective ObjectMgr loader acceptance.
pub struct CatalogCreatureAiTemplate {
    pub ordinal: u64,
    pub entry: u32,
    pub faction: u32,
    pub extra_flags: u32,
    pub unit_flags: u32,
    pub type_flags: u32,
    pub static_flags: [u32; 4],
    pub rank: u32,
    pub inhabit_type: u32,
    pub stats_multipliers: [u32; 3],
    pub civilian: bool,
    pub pursuit_ms: u32,
    pub call_for_help: u32,
    pub creature_type: u32,
    pub ai_name: String,
    pub script_name: String,
}
pub struct CatalogScriptAiBinding {
    pub script_id: u32,
    pub name: String,
    pub registration: u64,
    pub operation: String,
    pub source_file: String,
    pub source_line: u32,
    pub source_function: String,
    pub registry_sha256: String,
    pub get_ai_nonnull: bool,
}
impl VerifiedCatalog {
    pub(crate) fn execution_ai_templates(
        &mut self,
        entry: Option<u32>,
        mut visit: impl FnMut(CatalogCreatureAiTemplate) -> Result<()>,
    ) -> Result<()> {
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let cols = spawns::columns(&schema, "creature_template")?;
        let indices = [
            "Entry",
            "Faction",
            "ExtraFlags",
            "AIName",
            "ScriptName",
            "UnitFlags",
            "CreatureTypeFlags",
            "StaticFlags1",
            "StaticFlags2",
            "StaticFlags3",
            "StaticFlags4",
            "Rank",
            "InhabitType",
            "HealthMultiplier",
            "PowerMultiplier",
            "ArmorMultiplier",
            "Civilian",
            "Pursuit",
            "CallForHelp",
            "CreatureType",
        ]
        .map(|name| spawns::position(&cols, name))
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
        let mut decode = |row: RowView<'_>| {
            let values = cells(&row)?;
            let text = |index: usize| {
                let raw = values[index].unwrap_or(b"");
                let raw = raw.split(|b| *b == 0).next().unwrap_or(b"");
                std::str::from_utf8(raw)
                    .map(str::to_owned)
                    .map_err(|_| fail("scripts.ai_name_encoding"))
            };
            visit(CatalogCreatureAiTemplate {
                ordinal: row.ordinal,
                entry: spawns::uint(values[indices[0]])?,
                faction: u32::from(spawns::uint(values[indices[1]])? as u16),
                extra_flags: spawns::uint(values[indices[2]])?,
                unit_flags: spawns::uint(values[indices[5]])?,
                type_flags: spawns::uint(values[indices[6]])?,
                static_flags: [
                    spawns::uint(values[indices[7]])?,
                    spawns::uint(values[indices[8]])?,
                    spawns::uint(values[indices[9]])?,
                    spawns::uint(values[indices[10]])?,
                ],
                rank: spawns::uint(values[indices[11]])?,
                inhabit_type: spawns::uint(values[indices[12]])?,
                stats_multipliers: [
                    spawns::floating(values[indices[13]])?.to_bits(),
                    spawns::floating(values[indices[14]])?.to_bits(),
                    spawns::floating(values[indices[15]])?.to_bits(),
                ],
                civilian: spawns::uint(values[indices[16]])? as u8 != 0,
                pursuit_ms: spawns::uint(values[indices[17]])?,
                // SQLStorages.cpp:21-22,45; Creature.h:100,116 use uint32 fields.
                call_for_help: spawns::uint(values[indices[18]])?,
                creature_type: spawns::uint(values[indices[19]])?,
                ai_name: text(indices[3])?,
                script_name: text(indices[4])?,
            })
        };
        if let Some(entry) = entry {
            spawns::visit_key(self, "creature_template", entry, decode)
        } else {
            self.scan("creature_template", 0, 256, &mut decode)
        }
    }
    /// Retained F2 input, not sealed executable binding or a constructor result.
    pub(crate) fn execution_script_ai_bindings(
        &self,
        report: &Path,
    ) -> Result<Vec<CatalogScriptAiBinding>> {
        let mut value = input_json(report, CHUNK)?;
        let digest = value
            .as_object_mut()
            .ok_or_else(|| fail("scripts.ai_report"))?
            .remove("view_sha256")
            .ok_or_else(|| fail("scripts.ai_report"))?;
        check(digest == json::digest(&value), "scripts.ai_report_hash")?;
        check(
            value["format"] == "benilla-script-views-v1"
                && value["catalog_sha256"] == self.manifest["catalog_sha256"],
            "scripts.ai_report_catalog",
        )?;
        script_ai_bindings(&value["registrations"]["registry"])
    }
}
fn script_ai_bindings(registry: &Value) -> Result<Vec<CatalogScriptAiBinding>> {
    let records = registry["records"]
        .as_array()
        .ok_or_else(|| fail("scripts.ai_registry"))?;
    check(registry["attempts"] == records.len(), "scripts.ai_registry")?;
    let digest = registry["raw_registry_sha256"]
        .as_str()
        .ok_or_else(|| fail("scripts.ai_registry"))?;
    check(
        digest.len() == 64
            && digest
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "scripts.ai_registry",
    )?;
    let mut ids = BTreeSet::new();
    let mut bindings = Vec::new();
    for (ordinal, record) in records.iter().enumerate() {
        let Some(id) = record["id"].as_u64() else {
            check(
                record["id"].is_null() && record["outcome"] == "unobserved_binding_outcome",
                "scripts.ai_registry",
            )?;
            continue;
        };
        let id = u32::try_from(id).map_err(|_| fail("scripts.ai_registry"))?;
        check(
            id != 0
                && record["outcome"]
                    == if ids.insert(id) {
                        "accepted"
                    } else {
                        "accepted_override"
                    },
            "scripts.ai_registry",
        )?;
        let text = |value: &Value| {
            value
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| fail("scripts.ai_registry"))
        };
        let callback = text(&record["slots"]["GetAI"])?;
        bindings.push(CatalogScriptAiBinding {
            script_id: id,
            name: text(&record["name"])?,
            registration: ordinal as u64,
            operation: text(&record["operation"])?,
            source_file: text(&record["source"]["file"])?,
            source_line: u32::try_from(number(&record["source"]["line"])?)
                .map_err(|_| fail("scripts.ai_registry"))?,
            source_function: text(&record["source"]["function"])?,
            registry_sha256: digest.into(),
            get_ai_nonnull: pointer(&callback)? != 0,
        });
    }
    check(
        registry["accepted"] == bindings.len() && registry["final_bindings"] == ids.len(),
        "scripts.ai_registry",
    )?;
    Ok(bindings)
}
#[cfg(test)]
#[path = "scripts_tests.rs"]
mod tests;

#[cfg(test)]
pub(crate) struct MotionSpawnOperands {
    pub guid: u32,
    pub entry: u32,
    pub movement_type: u8,
    pub radius: f32,
    pub pose: [f32; 4],
}

impl VerifiedCatalog {
    /// WaypointManager.cpp:198,329,441 and ScriptDevAI/system/system.cpp:75.
    /// Raw retained rows only. ScriptMgr validation is a separate required owner.
    pub(crate) fn execution_waypoint_rows(
        &mut self,
        table: &str,
    ) -> Result<Vec<crate::native::source_execution::SourceWaypointRow>> {
        use crate::native::source_execution::{SourceWaypointNode, SourceWaypointRow};
        check(
            matches!(
                table,
                "creature_movement"
                    | "creature_movement_template"
                    | "waypoint_path"
                    | "script_waypoint"
            ),
            "scripts.waypoint_relation",
        )?;
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let cols = columns(&schema, table)?;
        let template = matches!(table, "creature_movement_template" | "script_waypoint");
        let owner = position(
            cols,
            if template {
                "Entry"
            } else if table == "waypoint_path" {
                "PathId"
            } else {
                "Id"
            },
        )?;
        let path = if template {
            Some(position(cols, "PathId")?)
        } else {
            None
        };
        let point = position(cols, "Point")?;
        let x = position(cols, "PositionX")?;
        let y = position(cols, "PositionY")?;
        let z = position(cols, "PositionZ")?;
        let orientation = position(cols, "Orientation")?;
        let delay = position(cols, "WaitTime")?;
        let script = position(cols, "ScriptId")?;
        let mut rows = Vec::new();
        self.scan(table, 0, 256, |row| {
            let v = cells(&row)?;
            let o = spawns::floating(v[orientation])?;
            rows.push(SourceWaypointRow {
                ordinal: row.ordinal,
                owner: spawns::uint(v[owner])?,
                path: path.map(|i| spawns::uint(v[i])).transpose()?.unwrap_or(0),
                point: spawns::uint(v[point])?,
                node: SourceWaypointNode {
                    position: [
                        spawns::floating(v[x])?,
                        spawns::floating(v[y])?,
                        spawns::floating(v[z])?,
                    ],
                    orientation: if table != "script_waypoint" && o == 100. {
                        None
                    } else {
                        Some(o)
                    },
                    delay: spawns::uint(v[delay])?,
                    script: spawns::uint(v[script])?,
                },
            });
            Ok(())
        })?;
        Ok(rows)
    }
    /// Narrow numeric EventAI loader operands using the retained F2 schema.
    pub(crate) fn execution_event_ai_operands<const N: usize>(
        &mut self,
        table: &str,
        names: [&str; N],
    ) -> Result<Vec<(u64, [u32; N])>> {
        check(
            matches!(
                table,
                "creature_ai_scripts"
                    | "creature_template"
                    | "creature"
                    | "conditions"
                    | "quest_template"
                    | "broadcast_text"
                    | "spell_template"
                    | "creature_ai_summons"
                    | "dbscripts_on_relay"
                    | "dbscript_random_templates"
                    | "creature_spell_list"
                    | "creature_immunities"
            ),
            "scripts.event_ai_relation",
        )?;
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let cols = columns(&schema, table)?;
        let indices = names
            .map(|name| position(cols, name))
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
        let mut rows = Vec::new();
        self.scan(table, 0, 256, |row| {
            let values = cells(&row)?;
            let mut words = [0; N];
            for (word, &index) in words.iter_mut().zip(&indices) {
                *word = spawns::uint(values[index])?;
            }
            rows.push((row.ordinal, words));
            Ok(())
        })?;
        Ok(rows)
    }
    /// Test-only raw census operands, never EventAI execution relations.
    #[cfg(test)]
    pub(crate) fn motion_census_operands<const N: usize>(
        &mut self,
        table: &str,
        names: [&str; N],
    ) -> Result<Vec<(u64, [u32; N])>> {
        check(
            matches!(
                table,
                "creature_spawn_entry"
                    | "spawn_group"
                    | "spawn_group_spawn"
                    | "spawn_group_entry"
                    | "spawn_group_squad"
                    | "creature_conditional_spawn"
                    | "game_event_creature_data"
            ),
            "scripts.motion_census_relation",
        )?;
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let cols = columns(&schema, table)?;
        let indices = names
            .map(|name| position(cols, name))
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
        let mut rows = Vec::new();
        self.scan(table, 0, 256, |row| {
            let values = cells(&row)?;
            let mut words = [0; N];
            for (word, &index) in words.iter_mut().zip(&indices) {
                *word = spawns::uint(values[index])?;
            }
            rows.push((row.ordinal, words));
            Ok(())
        })?;
        Ok(rows)
    }
    /// ObjectMgr.cpp:1965-1966 reads Field::GetFloat/GetUInt8, Field.h:53,56.
    #[cfg(test)]
    pub(crate) fn execution_motion_spawn_operands(&mut self) -> Result<Vec<MotionSpawnOperands>> {
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let cols = columns(&schema, "creature")?;
        let guid = position(cols, "guid")?;
        let entry = position(cols, "id")?;
        let motion = position(cols, "MovementType")?;
        let radius = position(cols, "spawndist")?;
        let pose = ["position_x", "position_y", "position_z", "orientation"]
            .map(|name| position(cols, name))
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
        let mut rows = Vec::new();
        self.scan("creature", 0, 256, |row| {
            let values = cells(&row)?;
            rows.push(MotionSpawnOperands {
                guid: spawns::uint(values[guid])?,
                entry: spawns::uint(values[entry])?,
                movement_type: spawns::uint(values[motion])? as u8,
                radius: spawns::floating(values[radius])?,
                pose: [
                    spawns::floating(values[pose[0]])?,
                    spawns::floating(values[pose[1]])?,
                    spawns::floating(values[pose[2]])?,
                    spawns::floating(values[pose[3]])?,
                ],
            });
            Ok(())
        })?;
        Ok(rows)
    }
}

#[cfg(test)]
pub(crate) const UNIT_EVENT_SCRIPT_RELATIONS: [&str; 10] = [
    "dbscripts_on_creature_death",
    "dbscripts_on_creature_movement",
    "dbscripts_on_event",
    "dbscripts_on_go_template_use",
    "dbscripts_on_go_use",
    "dbscripts_on_gossip",
    "dbscripts_on_quest_end",
    "dbscripts_on_quest_start",
    "dbscripts_on_relay",
    "dbscripts_on_spell",
];
#[cfg(test)]
fn unit_event_census_relation(table: &str) -> Result<()> {
    check(
        UNIT_EVENT_SCRIPT_RELATIONS.contains(&table) || table == "spawn_group",
        "scripts.unit_event_census_relation",
    )
}
#[cfg(test)]
impl VerifiedCatalog {
    /// Inventory only, never an executable content gate.
    pub(crate) fn unit_event_census_operands<const N: usize>(
        &mut self,
        table: &str,
        names: [&str; N],
    ) -> Result<Vec<(u64, [u32; N])>> {
        unit_event_census_relation(table)?;
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let cols = columns(&schema, table)?;
        let indices = names
            .map(|name| position(cols, name))
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
        let mut rows = Vec::new();
        self.scan(table, 0, 256, |row| {
            let values = cells(&row)?;
            let mut words = [0; N];
            for (word, &index) in words.iter_mut().zip(&indices) {
                *word = spawns::uint(values[index])?;
            }
            rows.push((row.ordinal, words));
            Ok(())
        })?;
        Ok(rows)
    }
}
#[cfg(test)]
#[test]
fn unit_event_census_relation_gate_rejects_unlisted_tables() {
    for relation in UNIT_EVENT_SCRIPT_RELATIONS
        .into_iter()
        .chain(["spawn_group"])
    {
        unit_event_census_relation(relation).unwrap();
    }
    for relation in [
        "creature_template",
        "spell_template",
        "dbscripts_on_unknown",
        "",
    ] {
        assert_eq!(
            unit_event_census_relation(relation).unwrap_err().id,
            "scripts.unit_event_census_relation"
        );
    }
}
#[path = "db_scripts.rs"]
mod db_scripts;
