// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static CASES: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../../../scripts/classic_export/catalog_preparation_cases.json"
    ))
    .unwrap()
});
fn corpus_records(values: &Value) -> Vec<u8> {
    values
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|record| json::canonical(record).into_iter().chain(*b"\n"))
        .collect()
}

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "benilla-preparation-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn file(&self, name: &str, raw: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, raw).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn request() -> Value {
    json!({"format":"benilla-catalog-preparation-input-v1","catalog_sha256":"catalog","source_snapshot":"snapshot",
        "reference_execution":POLICY["pending_reference"],"occurrences":{"dbc":{},"event":{},"group":{},"pool":{},"script":{},"spawn":{}},
        "registration":{},"initial_snapshot":{},"environment":{}})
}
fn family() -> (Value, Value) {
    let spec = &POLICY["families"]["quests"];
    let tables: serde_json::Map<_, _> = spec["tables"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            (
                t.as_str().unwrap().to_owned(),
                json!({"rows":1,"raw_sha256":"raw"}),
            )
        })
        .collect();
    let refusals: Vec<_> = spec["refusals"].as_object().unwrap().keys().collect();
    let mut report = json!({"format":spec["format"],"catalog_sha256":"catalog","complete":false,"families":tables,"refusals":refusals});
    report["view_sha256"] = json!(json::digest(&report));
    (report, Value::Object(tables))
}
fn seal_candidate(root: &Path, manifest: &mut Value) {
    manifest
        .as_object_mut()
        .unwrap()
        .remove("preparation_sha256");
    let mut hash = Sha256::new();
    hash.update(DOMAIN);
    hash.update(json::canonical(manifest));
    manifest["preparation_sha256"] = json!(hex(&hash.finalize()));
    let mut identity = json!({"format":"benilla-production-input-identity-v2","preparation_sha256":manifest["preparation_sha256"],"runtime_admitted":false});
    identity["identity_sha256"] = json!(json::digest(&identity));
    std::fs::write(root.join("preparation.json"), json::canonical(manifest)).unwrap();
    std::fs::write(root.join("identity.json"), json::canonical(&identity)).unwrap();
}
fn candidates(f: &Fixture) -> (PathBuf, PathBuf, Value) {
    let actual = f.0.join("actual");
    let expected = f.0.join("expected");
    for root in [&actual, &expected] {
        std::fs::create_dir(root).unwrap();
        std::fs::create_dir(root.join("families")).unwrap();
        for name in POLICY["families"].as_object().unwrap().keys() {
            std::fs::write(root.join("families").join(format!("{name}.json")), b"{}").unwrap();
        }
        std::fs::write(root.join("rows.jsonl"), corpus_records(&CASES["original"])).unwrap();
    }
    let index = index_stream(&actual.join("rows.jsonl"), "jsonl", 0).unwrap();
    let mut manifest = json!({"format":"benilla-native-catalog-preparation-v2","complete":false,"catalog_sha256":"a".repeat(64),
        "reference_execution":POLICY["pending_reference"],"input_bindings":{"registration":{"bytes":1,"sha256":"a".repeat(64)}},"streams":{"rows.jsonl":index}});
    for root in [&actual, &expected] {
        seal_candidate(root, &mut manifest);
    }
    (actual, expected, manifest)
}

#[test]
fn typed_pending_binding_refuses_wrong_execution_snapshot_and_occurrence_sets() {
    let original = request();
    validate_input(&original, &json!("catalog"), &json!("snapshot")).unwrap();
    assert_eq!(
        validate_input(&original, &json!("wrong"), &json!("snapshot"))
            .unwrap_err()
            .id,
        "preparation.snapshot"
    );
    for pending in [
        Value::Null,
        json!({"sha256":"0000000000000000000000000000000000000000000000000000000000000000"}),
        json!({"status":"sealed"}),
    ] {
        let mut value = original.clone();
        value["reference_execution"] = pending;
        assert_eq!(
            validate_input(&value, &json!("catalog"), &json!("snapshot"))
                .unwrap_err()
                .id,
            "preparation.execution_pending"
        );
    }
    let mut missing = original.clone();
    missing["occurrences"]
        .as_object_mut()
        .unwrap()
        .remove("spawn");
    assert_eq!(
        validate_input(&missing, &json!("catalog"), &json!("snapshot"))
            .unwrap_err()
            .id,
        "preparation.occurrences"
    );
    let mut surplus = original;
    surplus["occurrences"]["extra"] = json!({});
    assert_eq!(
        validate_input(&surplus, &json!("catalog"), &json!("snapshot"))
            .unwrap_err()
            .id,
        "preparation.occurrences"
    );
}
#[test]
fn family_omissions_rehashed_wrong_rows_and_unknown_family_refuse() {
    let (report, source) = family();
    check_family("quests", &report, &source, &json!("catalog")).unwrap();
    assert_eq!(
        check_family("unknown", &report, &source, &json!("catalog"))
            .unwrap_err()
            .id,
        "preparation.unknown_family"
    );
    let table = POLICY["families"]["quests"]["tables"][0].as_str().unwrap();
    let mut missing = report.clone();
    missing["families"].as_object_mut().unwrap().remove(table);
    assert_eq!(
        check_family("quests", &missing, &source, &json!("catalog"))
            .unwrap_err()
            .id,
        "preparation.family_tables"
    );
    let mut forged = report;
    forged["families"][table]["rows"] = json!(0);
    forged.as_object_mut().unwrap().remove("view_sha256");
    forged["view_sha256"] = json!(json::digest(&forged));
    assert_eq!(
        check_family("quests", &forged, &source, &json!("catalog"))
            .unwrap_err()
            .id,
        "preparation.cross_family"
    );
}
#[test]
fn complete_flag_and_erased_obligations_cannot_grant_admission() {
    let (mut report, source) = family();
    report["complete"] = json!(true);
    assert_eq!(
        check_family("quests", &report, &source, &json!("catalog"))
            .unwrap_err()
            .id,
        "preparation.complete"
    );
    report["complete"] = json!(false);
    report["refusals"] = json!([]);
    assert_eq!(
        check_family("quests", &report, &source, &json!("catalog"))
            .unwrap_err()
            .id,
        "preparation.obligations"
    );
}
#[test]
fn changed_build_binding_refuses() {
    let f = Fixture::new();
    let path = f.file("build", b"actual");
    assert_eq!(
        bound(&json!({"path":path,"bytes":6,"sha256":hex(&Sha256::digest(b"forged"))}))
            .unwrap_err()
            .id,
        "preparation.input_hash"
    );
}
#[test]
fn every_record_survives_bounded_page_boundaries() {
    let f = Fixture::new();
    let raw = b"[0]\n".repeat(4097);
    let index = index_stream(&f.file("rows", &raw), "jsonl", 0).unwrap();
    assert_eq!(index["rows"], 4097);
    assert_eq!(index["pages"][0]["rows"], 4096);
    assert_eq!(index["pages"][1]["rows"], 1);
    assert_eq!(index["bytes"], raw.len());
    assert_eq!(index["sha256"], hex(&Sha256::digest(&raw)));
}
#[test]
fn source_order_and_stream_exhaustion_are_not_negative_lookups() {
    let f = Fixture::new();
    let mut raw = Vec::new();
    raw.extend(22u32.to_le_bytes());
    raw.extend(1u64.to_le_bytes());
    raw.extend(1u16.to_le_bytes());
    raw.extend(1u16.to_le_bytes());
    raw.push(0);
    raw.extend(1u32.to_le_bytes());
    raw.push(b'x');
    let path = f.file("frames", &raw);
    assert_eq!(
        index_stream(&path, "frames", 1).unwrap_err().id,
        "catalog.duplicate_row"
    );
    raw[4..12].copy_from_slice(&0u64.to_le_bytes());
    std::fs::write(&path, &raw).unwrap();
    assert_eq!(index_stream(&path, "frames", 1).unwrap()["rows"], 1);
    raw.pop();
    std::fs::write(&path, &raw).unwrap();
    assert_eq!(
        index_stream(&path, "frames", 1).unwrap_err().id,
        "preparation.exhausted"
    );
}
#[test]
fn false_negatives_missing_surplus_and_reordered_records_refuse() {
    for case in CASES["cases"].as_array().unwrap() {
        let f = Fixture::new();
        let (actual, expected, manifest) = candidates(&f);
        std::fs::write(
            expected.join("rows.jsonl"),
            corpus_records(&case["records"]),
        )
        .unwrap();
        assert_eq!(
            compare_output(&expected, &actual, &manifest)
                .unwrap_err()
                .id,
            case["diagnostic"].as_str().unwrap()
        );
    }
}
#[test]
fn fully_rehashed_candidate_still_compares_against_current_inputs() {
    let f = Fixture::new();
    let (actual, expected, manifest) = candidates(&f);
    std::fs::write(expected.join("rows.jsonl"), b"[false]\n[true]\n").unwrap();
    let mut forged = manifest.clone();
    forged["streams"]["rows.jsonl"] =
        index_stream(&expected.join("rows.jsonl"), "jsonl", 0).unwrap();
    seal_candidate(&expected, &mut forged);
    assert_eq!(
        compare_output(&expected, &actual, &manifest)
            .unwrap_err()
            .id,
        "preparation.manifest_mismatch"
    );
}
#[test]
fn identity_family_surplus_and_complete_forgery_refuse() {
    for (kind, diagnostic) in [
        ("identity", "identity_mismatch"),
        ("family", "family_mismatch"),
        ("surplus", "surplus"),
        ("complete", "complete"),
    ] {
        let f = Fixture::new();
        let (actual, expected, manifest) = candidates(&f);
        match kind {
            "identity" => {
                std::fs::write(expected.join("identity.json"), b"{\"forged\":true}").unwrap()
            }
            "family" => std::fs::write(
                expected.join("families/quests.json"),
                b"{\"complete\":true}",
            )
            .unwrap(),
            "surplus" => std::fs::write(expected.join("extra"), b"x").unwrap(),
            _ => {
                let mut forged = manifest.clone();
                forged["complete"] = json!(true);
                std::fs::write(expected.join("preparation.json"), json::canonical(&forged))
                    .unwrap();
            }
        }
        assert_eq!(
            compare_output(&expected, &actual, &manifest)
                .unwrap_err()
                .id,
            format!("preparation.{diagnostic}")
        );
    }
}
#[test]
fn all_obligations_remain_pending_in_one_named_kind() {
    let reports: serde_json::Map<_, _> = POLICY["families"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, spec)| {
            let refusals: Vec<_> = spec["refusals"].as_object().unwrap().keys().collect();
            (name.clone(), json!({"refusals":refusals}))
        })
        .collect();
    let ledger = obligations(&reports);
    let mut ids = BTreeSet::new();
    for entries in ledger.as_object().unwrap().values() {
        for entry in entries.as_array().unwrap() {
            assert_eq!(entry["status"], "pending");
            assert!(ids.insert(entry["id"].as_str().unwrap()));
        }
    }
    assert!(ids.contains("binding.initial_snapshot"));
    assert_eq!(ledger.as_object().unwrap().len(), 4);
}

#[test]
fn observed_cursor_order_must_match_the_bound_metadata() {
    for &name in OCCURRENCES {
        let observed = json!({"0":{"rows":2,"occurrence_sha256":"recorded"}});
        let report = match name {
            "spawn" => json!({"joined":observed}),
            "pool" => json!({"streams":observed}),
            "dbc" => json!({"transports":{"occurrences":observed}}),
            _ => json!({"loaders":observed}),
        };
        let mut metadata = json!({"streams":{"0":{"rows":2,"sha256":"recorded"}}});
        check_occurrences(name, &report, &metadata).unwrap();
        metadata["streams"]["0"]["sha256"] = json!("same_rows_other_order");
        assert_eq!(
            check_occurrences(name, &report, &metadata).unwrap_err().id,
            "preparation.occurrence_binding"
        );
    }
}

#[test]
fn rehashed_wrong_snapshot_execution_and_build_refuse() {
    for kind in ["snapshot", "execution", "build"] {
        let f = Fixture::new();
        let (actual, expected, manifest) = candidates(&f);
        let mut forged = manifest.clone();
        match kind {
            "snapshot" => forged["catalog_sha256"] = json!("b".repeat(64)),
            "execution" => {
                forged["reference_execution"] = json!({"status":"sealed","sha256":"b".repeat(64)})
            }
            _ => forged["input_bindings"]["registration"]["sha256"] = json!("b".repeat(64)),
        }
        seal_candidate(&expected, &mut forged);
        assert_eq!(
            compare_output(&expected, &actual, &manifest)
                .unwrap_err()
                .id,
            "preparation.manifest_mismatch"
        );
    }
}
