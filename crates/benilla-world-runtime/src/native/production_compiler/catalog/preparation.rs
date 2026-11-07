// SPDX-License-Identifier: GPL-3.0-or-later
//! Complete checked catalog aggregation with explicit pending reference execution.
use super::*;
use std::collections::BTreeSet;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::LazyLock;

static POLICY: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../../../scripts/classic_export/catalog_preparation_policy.json"
    ))
    .expect("preparation policy")
});
const OCCURRENCES: &[&str] = &["dbc", "event", "group", "pool", "script", "spawn"];
const DOMAIN: &[u8] = b"benilla-native-catalog-preparation-v2\0";

fn input(path: &Path) -> Result<Vec<u8>> {
    let held = reader::Held::open(path, CHUNK as u64)?;
    held.read(0, held.len() as usize, &mut Metrics::default(), CHUNK)
}
fn write(path: &Path, value: &Value) -> Result<()> {
    let raw = json::canonical(value);
    write_raw(path, &raw)
}
fn write_raw(path: &Path, raw: &[u8]) -> Result<()> {
    check(raw.len() <= CHUNK, "preparation.metadata_limit")?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| fail("preparation.output"))?;
    file.write_all(raw)
        .and_then(|_| file.sync_all())
        .map_err(|_| fail("preparation.output"))
}
fn checked_bytes(value: &Value) -> Result<Vec<u8>> {
    catalog_schema::fields(
        value,
        &["path", "bytes", "sha256"],
        "preparation.input_descriptor",
    )?;
    let path = value["path"]
        .as_str()
        .ok_or_else(|| fail("preparation.input_descriptor"))?;
    let bytes = value["bytes"]
        .as_u64()
        .ok_or_else(|| fail("preparation.input_descriptor"))?;
    check(bytes <= CHUNK as u64, "preparation.input_descriptor")?;
    let raw = input(Path::new(path))?;
    let hash = hex(&Sha256::digest(&raw));
    check(
        bytes == raw.len() as u64 && value["sha256"] == hash,
        "preparation.input_hash",
    )?;
    Ok(raw)
}
fn bound(value: &Value) -> Result<Value> {
    checked_bytes(value)?;
    Ok(json!({"bytes":value["bytes"],"sha256":value["sha256"]}))
}
fn validate_input(value: &Value, hash: &Value, snapshot: &Value) -> Result<()> {
    catalog_schema::fields(
        value,
        &[
            "format",
            "catalog_sha256",
            "source_snapshot",
            "reference_execution",
            "occurrences",
            "registration",
            "initial_snapshot",
            "environment",
        ],
        "preparation.inputs",
    )?;
    check(
        value["format"] == "benilla-catalog-preparation-input-v1",
        "preparation.version",
    )?;
    check(
        &value["catalog_sha256"] == hash && &value["source_snapshot"] == snapshot,
        "preparation.snapshot",
    )?;
    check(
        value["reference_execution"] == POLICY["pending_reference"],
        "preparation.execution_pending",
    )?;
    catalog_schema::fields(
        &value["occurrences"],
        OCCURRENCES,
        "preparation.occurrences",
    )
}
fn check_occurrences(name: &str, report: &Value, metadata: &Value) -> Result<()> {
    let observed = match name {
        "spawn" => &report["joined"],
        "pool" => &report["streams"],
        "dbc" => &report["transports"]["occurrences"],
        _ => &report["loaders"],
    }
    .as_object()
    .ok_or_else(|| fail("preparation.occurrence_binding"))?;
    let streams = metadata["streams"]
        .as_object()
        .ok_or_else(|| fail("preparation.occurrence_binding"))?;
    check(
        observed.keys().eq(streams.keys()),
        "preparation.occurrence_binding",
    )?;
    for (key, stream) in streams {
        check(
            observed[key]["rows"] == stream["rows"]
                && observed[key]["occurrence_sha256"] == stream["sha256"],
            "preparation.occurrence_binding",
        )?;
    }
    Ok(())
}

struct PageIndex {
    pages: Vec<Value>,
    whole: Sha256,
    page: Sha256,
    offset: u64,
    size: usize,
    rows: u64,
    count: u64,
    total: u64,
}
impl PageIndex {
    fn new() -> Self {
        Self {
            pages: Vec::new(),
            whole: Sha256::new(),
            page: Sha256::new(),
            offset: 0,
            size: 0,
            rows: 0,
            count: 0,
            total: 0,
        }
    }
    fn flush(&mut self) {
        if self.size != 0 {
            self.pages.push(json!({"offset":self.offset,"bytes":self.size,"rows":self.rows,"sha256":hex(&std::mem::take(&mut self.page).finalize())}));
            self.offset += self.size as u64;
            self.size = 0;
            self.rows = 0;
        }
    }
    fn record(&mut self, raw: &[u8]) {
        if self.rows == 4096 || self.size + raw.len() > CHUNK {
            self.flush();
        }
        self.whole.update(raw);
        self.page.update(raw);
        self.size += raw.len();
        self.total += raw.len() as u64;
        self.rows += 1;
        self.count += 1;
    }
    fn finish(mut self, kind: &str) -> Value {
        self.flush();
        json!({"kind":kind,"bytes":self.total,"rows":self.count,"sha256":hex(&self.whole.finalize()),"pages":self.pages})
    }
}
fn index_stream(path: &Path, kind: &str, columns: usize) -> Result<Value> {
    let held = reader::Held::open(path, POLICY["limits"]["stream_bytes"].as_u64().unwrap())?;
    let mut index = PageIndex::new();
    let mut at = 0;
    let mut metrics = Metrics::default();
    while at < held.len() {
        let raw = held.read(
            at,
            (held.len() - at).min(CHUNK as u64) as usize,
            &mut metrics,
            CHUNK,
        )?;
        let mut used = 0;
        while used < raw.len() {
            let remaining = &raw[used..];
            let length = if kind == "frames" {
                if remaining.len() < 4 {
                    break;
                }
                let length = le(remaining, 0, 4)? as usize;
                check((16..=FRAME).contains(&length), "preparation.record_limit")?;
                if length > remaining.len() {
                    break;
                }
                RowView::new(&remaining[..length], index.count, columns)?;
                length
            } else if kind == "jsonl" {
                let Some(end) = remaining.iter().position(|b| *b == b'\n') else {
                    break;
                };
                check(end < FRAME, "preparation.record_limit")?;
                end + 1
            } else {
                check(
                    index.count == 0 && held.len() <= CHUNK as u64,
                    "preparation.record_limit",
                )?;
                bounded_json(remaining)?;
                remaining.len()
            };
            index.record(&remaining[..length]);
            used += length;
        }
        check(used != 0, "preparation.exhausted")?;
        at += used as u64;
    }
    Ok(index.finish(kind))
}

fn check_family(name: &str, report: &Value, source: &Value, hash: &Value) -> Result<()> {
    let spec = POLICY["families"]
        .get(name)
        .ok_or_else(|| fail("preparation.unknown_family"))?;
    check(report["complete"] == false, "preparation.complete")?;
    check(
        report["format"] == spec["format"],
        "preparation.family_format",
    )?;
    check(&report["catalog_sha256"] == hash, "preparation.snapshot")?;
    let tables = if name == "dbc" {
        &report["transports"]["raw"]
    } else if matches!(name, "spawn" | "pool") {
        &report["raw_sha256"]
    } else {
        &report["families"]
    };
    let wanted: BTreeSet<_> = spec["tables"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    check(
        tables
            .as_object()
            .is_some_and(|m| m.keys().map(String::as_str).collect::<BTreeSet<_>>() == wanted),
        "preparation.family_tables",
    )?;
    let refusals = report.get("refusals").unwrap_or(&report["blockers"]);
    let provided: BTreeSet<_> = refusals
        .as_array()
        .ok_or_else(|| fail("preparation.obligations"))?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    check(
        provided
            == spec["refusals"]
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect(),
        "preparation.obligations",
    )?;
    for table in wanted {
        let row = &tables[table];
        let (raw, count) = if matches!(name, "spawn" | "pool") {
            (row, &report["counts"][table])
        } else {
            (
                &row["raw_sha256"],
                row.get("rows").unwrap_or(&row["stream"]["rows"]),
            )
        };
        check(
            raw == &source[table]["raw_sha256"] && count == &source[table]["rows"],
            "preparation.cross_family",
        )?;
    }
    let mut own = report.clone();
    let hash = own.as_object_mut().unwrap().remove("view_sha256");
    check(
        hash == Some(json!(json::digest(&own))),
        "preparation.family_hash",
    )
}
fn obligations(reports: &serde_json::Map<String, Value>) -> Value {
    let coverage = &*catalog_schema::POLICY;
    let mut result = BTreeMap::<String, BTreeMap<String, Vec<String>>>::new();
    for kind in POLICY["obligation_kinds"].as_array().unwrap() {
        result.insert(kind.as_str().unwrap().to_owned(), BTreeMap::new());
    }
    for requirement in POLICY["pending_reference"]["required"].as_array().unwrap() {
        result.get_mut("F1_binding").unwrap().insert(
            format!("binding.{}", requirement.as_str().unwrap()),
            vec!["reference_execution".into()],
        );
    }
    for (family, report) in reports {
        for diagnostic in report
            .get("refusals")
            .unwrap_or(&report["blockers"])
            .as_array()
            .unwrap()
        {
            let id = diagnostic.as_str().unwrap();
            let kind = POLICY["families"][family]["refusals"][id].as_str().unwrap();
            result
                .get_mut(kind)
                .unwrap()
                .entry(id.to_owned())
                .or_default()
                .push(family.clone());
        }
    }
    for loader in coverage["loaders"].as_object().unwrap().keys() {
        result
            .get_mut("loader_execution")
            .unwrap()
            .insert(format!("loader.{loader}"), vec!["source_loader".into()]);
    }
    let covered: BTreeSet<_> = POLICY["families"]
        .as_object()
        .unwrap()
        .values()
        .flat_map(|s| {
            s["tables"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
        })
        .collect();
    for (table, spec) in coverage["tables"].as_object().unwrap() {
        if spec["status"] != "archive-only" && !covered.contains(table.as_str()) {
            result
                .get_mut("loader_execution")
                .unwrap()
                .insert(format!("view.{table}"), vec!["raw_catalog_only".into()]);
        }
    }
    result.get_mut("configuration").unwrap().insert(
        "configuration.effective_world_inputs_unbound".into(),
        vec!["reference_execution".into()],
    );
    Value::Object(
        result
            .into_iter()
            .map(|(kind, entries)| {
                (
                    kind,
                    Value::Array(
                        entries
                            .into_iter()
                            .map(|(id, mut owners)| {
                                owners.sort();
                                json!({"id":id,"status":"pending","owners":owners})
                            })
                            .collect(),
                    ),
                )
            })
            .collect(),
    )
}
fn compare_file(expected: &Path, actual: &Path, length: u64) -> Result<()> {
    let a = reader::Held::open(expected, 2_147_483_648)?;
    let b = reader::Held::open(actual, 2_147_483_648)?;
    check(
        a.len() == length && b.len() == length,
        "preparation.record_mismatch",
    )?;
    let mut at = 0;
    let mut metrics = Metrics::default();
    while at < length {
        let n = (length - at).min(FRAME as u64) as usize;
        check(
            a.read(at, n, &mut metrics, CHUNK)? == b.read(at, n, &mut metrics, CHUNK)?,
            "preparation.record_mismatch",
        )?;
        at += n as u64;
    }
    Ok(())
}
fn compare_output(expected: &Path, actual: &Path, manifest: &Value) -> Result<()> {
    let supplied = bounded_json(&input(&expected.join("preparation.json"))?)?;
    check(supplied["complete"] == false, "preparation.complete")?;
    check(&supplied == manifest, "preparation.manifest_mismatch")?;
    let mut files = BTreeSet::from([
        PathBuf::from("preparation.json"),
        PathBuf::from("identity.json"),
        PathBuf::from("resources.json"),
    ]);
    for (relative, stream) in manifest["streams"].as_object().unwrap() {
        files.insert(PathBuf::from(relative));
        compare_file(
            &expected.join(relative),
            &actual.join(relative),
            number(&stream["bytes"])?,
        )?;
    }
    for name in POLICY["families"].as_object().unwrap().keys() {
        let relative = format!("families/{name}.json");
        files.insert(PathBuf::from(&relative));
        check(
            input(&expected.join(&relative))? == input(&actual.join(relative))?,
            "preparation.family_mismatch",
        )?;
    }
    check(
        input(&expected.join("identity.json"))? == input(&actual.join("identity.json"))?,
        "preparation.identity_mismatch",
    )?;
    let mut folders = vec![expected.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for item in std::fs::read_dir(folder).map_err(|_| fail("preparation.output"))? {
            let item = item.map_err(|_| fail("preparation.output"))?;
            let path = item.path();
            let relative = path
                .strip_prefix(expected)
                .map_err(|_| fail("preparation.surplus"))?;
            let kind = item.file_type().map_err(|_| fail("preparation.output"))?;
            if kind.is_dir() {
                check(
                    files.iter().any(|f| f.starts_with(relative)),
                    "preparation.surplus",
                )?;
                folders.push(path);
            } else {
                check(
                    kind.is_file() && files.contains(relative),
                    "preparation.surplus",
                )?;
            }
        }
    }
    Ok(())
}

/// Recomputes all families and every record before comparing an optional published candidate.
pub fn diagnose_catalog_preparation(
    root: &Path,
    inputs: &Path,
    output: &Path,
    expected: Option<&Path>,
) -> Result<Value> {
    let started = Instant::now();
    std::fs::create_dir(output).map_err(|_| fail("preparation.output"))?;
    for directory in ["families", "source"] {
        std::fs::create_dir(output.join(directory)).map_err(|_| fail("preparation.output"))?;
    }
    let request = json::decode(&input(inputs)?, CHUNK)?;
    let mut catalog = VerifiedCatalog::open(root, false)?;
    validate_input(
        &request,
        &catalog.manifest["catalog_sha256"],
        &catalog.manifest["source_snapshot"],
    )?;
    let mut bindings = serde_json::Map::new();
    for name in ["registration", "initial_snapshot", "environment"] {
        bindings.insert(name.into(), bound(&request[name])?);
    }
    let mut occurrence_bindings = serde_json::Map::new();
    let mut occurrences = BTreeMap::new();
    let mut occurrence_meta = BTreeMap::new();
    for &name in OCCURRENCES {
        let raw = checked_bytes(&request["occurrences"][name])?;
        occurrence_bindings.insert(name.into(),json!({"bytes":request["occurrences"][name]["bytes"],"sha256":request["occurrences"][name]["sha256"]}));
        occurrence_meta.insert(name, json::decode(&raw, CHUNK)?);
        let path = Path::new(request["occurrences"][name]["path"].as_str().unwrap());
        occurrences.insert(
            name,
            path.parent()
                .ok_or_else(|| fail("preparation.inputs"))?
                .to_path_buf(),
        );
    }
    bindings.insert("occurrences".into(), Value::Object(occurrence_bindings));
    let coverage = &*catalog_schema::POLICY;
    check(
        catalog
            .relations
            .keys()
            .eq(coverage["tables"].as_object().unwrap().keys()),
        "preparation.source_tables",
    )?;
    let mut source = serde_json::Map::new();
    let mut streams = serde_json::Map::new();
    for table in coverage["tables"].as_object().unwrap().keys() {
        let relation = catalog.relations[table].clone();
        let relative = format!("source/{table}.frames");
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output.join(&relative))
            .map_err(|_| fail("preparation.output"))?;
        let mut writer = BufWriter::new(file);
        catalog.scan(table, 0, 256, |row| {
            writer
                .write_all(row.frame)
                .map_err(|_| fail("preparation.output"))
        })?;
        writer.flush().map_err(|_| fail("preparation.output"))?;
        drop(writer);
        let index = index_stream(&output.join(&relative), "frames", relation.columns)?;
        check(index["rows"] == relation.rows, "preparation.source_rows")?;
        let views: Vec<_> = POLICY["families"]
            .as_object()
            .unwrap()
            .iter()
            .filter(|(_, s)| {
                s["tables"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v.as_str() == Some(table.as_str()))
            })
            .map(|(n, _)| n)
            .collect();
        let root =
            |r: &Root| json!({"sha256":r.sha256,"bytes":r.bytes,"count":r.count,"height":r.height});
        source.insert(table.clone(),json!({"rows":relation.rows,"raw_sha256":index["sha256"],"ordinal_index":root(&relation.ordinal),"key_index":root(&relation.key),
            "classification":coverage["tables"][table]["status"],"field_disposition":if coverage["tables"][table]["status"] == "archive-only" {"retained_raw_archive"} else {"retained_raw_loader_input"},
            "columns":relation.columns,"key_columns":relation.key_columns,"views":views}));
        streams.insert(relative, index);
    }
    let source = Value::Object(source);
    let mut reports = serde_json::Map::new();
    reports.insert(
        "spawn".into(),
        spawns::prepare(&mut catalog, &occurrences["spawn"])?,
    );
    reports.insert(
        "group".into(),
        groups::prepare(&mut catalog, &occurrences["group"])?,
    );
    reports.insert(
        "pool".into(),
        pools::prepare(
            &mut catalog,
            &occurrences["pool"],
            Some(&output.join("pool")),
        )?,
    );
    let state = events::initial_state(
        Path::new(request["initial_snapshot"]["path"].as_str().unwrap()),
        Path::new(request["environment"]["path"].as_str().unwrap()),
    )?;
    check(
        state["snapshot"] == bindings["initial_snapshot"]
            && state["calendar"]["evidence"] == bindings["environment"],
        "preparation.snapshot",
    )?;
    reports.insert(
        "event".into(),
        events::prepare(
            &mut catalog,
            &occurrences["event"],
            state,
            &output.join("event"),
        )?,
    );
    reports.insert(
        "dbc".into(),
        dbc::prepare(&mut catalog, &occurrences["dbc"], Some(&output.join("dbc")))?,
    );
    let registration_path = Path::new(request["registration"]["path"].as_str().unwrap());
    let registration = scripts::registration_input(registration_path)?;
    let declared = json::decode(&checked_bytes(&request["registration"])?, CHUNK)?;
    let mut registered = serde_json::Map::new();
    for name in ["manifest", "trace", "build", "definitions"] {
        registered.insert(
            name.into(),
            json!({"bytes":declared[name]["bytes"],"sha256":declared[name]["sha256"]}),
        );
    }
    check(
        registration["input"] == Value::Object(registered),
        "preparation.build",
    )?;
    reports.insert(
        "script".into(),
        scripts::prepare(&mut catalog, &occurrences["script"], registration)?,
    );
    reports.insert(
        "spell".into(),
        spells::prepare(&mut catalog, Some(&output.join("spell")))?,
    );
    for name in ["quests", "items", "loot", "globals"] {
        reports.insert(
            name.into(),
            content::prepare(&mut catalog, name, Some(&output.join(name)))?,
        );
    }
    check(
        reports
            .keys()
            .eq(POLICY["families"].as_object().unwrap().keys()),
        "preparation.families",
    )?;
    let mut families = serde_json::Map::new();
    for (family, report) in &reports {
        check_family(family, report, &source, &catalog.manifest["catalog_sha256"])?;
        if OCCURRENCES.contains(&family.as_str()) {
            check_occurrences(family, report, &occurrence_meta[family.as_str()])?;
        }
        let report_binding = {
            let raw = json::canonical(report);
            write_raw(
                &output.join("families").join(format!("{family}.json")),
                &raw,
            )?;
            json!({"bytes":raw.len(),"sha256":hex(&Sha256::digest(&raw))})
        };
        let folder = output.join(family);
        if folder.is_dir() {
            for item in std::fs::read_dir(&folder).map_err(|_| fail("preparation.output"))? {
                let path = item.map_err(|_| fail("preparation.output"))?.path();
                let filename = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| fail("preparation.output"))?;
                let kind = if path.extension().is_some_and(|e| e == "jsonl") {
                    "jsonl"
                } else {
                    "json"
                };
                streams.insert(
                    format!("{family}/{filename}"),
                    index_stream(&path, kind, 0)?,
                );
            }
        }
        let rows: u64 = POLICY["families"][family]["tables"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| source[t.as_str().unwrap()]["rows"].as_u64().unwrap())
            .sum();
        families.insert(family.clone(),json!({"status":"input_views_verified_execution_pending","view_sha256":report["view_sha256"],"rows":rows,
            "report":report_binding,"refusals":report.get("refusals").unwrap_or(&report["blockers"])}));
    }
    let pending = obligations(&reports);
    let counts: serde_json::Map<_, _> = pending
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), json!(v.as_array().unwrap().len())))
        .collect();
    let output_bytes: u64 = streams.values().map(|s| s["bytes"].as_u64().unwrap()).sum();
    let output_pages: usize = streams
        .values()
        .map(|s| s["pages"].as_array().unwrap().len())
        .sum();
    let output_streams = streams.len();
    let mut manifest = json!({"format":"benilla-native-catalog-preparation-v2","catalog_sha256":catalog.manifest["catalog_sha256"],"core_revision":catalog.manifest["core_revision"],"database_revision":catalog.manifest["database_revision"],
        "source_snapshot":catalog.manifest["source_snapshot"],"reference_execution":request["reference_execution"],"input_bindings":bindings,"policy_sha256":json::digest(&*POLICY),"source":source,
        "families":families,"streams":streams,"obligations":pending,"obligation_counts":counts,"complete":false});
    let mut hash = Sha256::new();
    hash.update(DOMAIN);
    hash.update(json::canonical(&manifest));
    manifest["preparation_sha256"] = json!(hex(&hash.finalize()));
    let mut identity = json!({"format":"benilla-production-input-identity-v2","catalog_sha256":catalog.manifest["catalog_sha256"],"preparation_sha256":manifest["preparation_sha256"],
        "reference_execution":request["reference_execution"],"input_bindings":bindings,"runtime_admitted":false});
    identity["identity_sha256"] = json!(json::digest(&identity));
    write(&output.join("preparation.json"), &manifest)?;
    write(&output.join("identity.json"), &identity)?;
    if let Some(candidate) = expected {
        compare_output(candidate, output, &manifest)?;
    }
    let mut resources = catalog.report();
    resources["phase"] = json!("whole_aggregation");
    resources["output_bytes"] = json!(output_bytes);
    resources["output_pages"] = json!(output_pages);
    resources["output_streams"] = json!(output_streams);
    resources["output_buffer_limit"] = json!(CHUNK + FRAME);
    resources["elapsed_seconds"] = json!(started.elapsed().as_secs_f64());
    write(&output.join("resources.json"), &resources)?;
    Ok(json!({"preparation":manifest,"identity":identity}))
}

#[cfg(test)]
#[path = "preparation_tests.rs"]
mod tests;
