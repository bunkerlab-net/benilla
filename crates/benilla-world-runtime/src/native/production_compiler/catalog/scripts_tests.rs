// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn temp() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "benilla-script-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn trace(attempts: &[(&str, u32, &str)]) -> Vec<Vec<u8>> {
    let mut lines = Vec::new();
    let mut add = |kind: &str, payload: String| {
        lines.push(format!("1\t{}\t{kind}\t{payload}\n", lines.len() + 1).into_bytes());
    };
    let mut accepted = 0;
    for (index, &(name, id, old)) in attempts.iter().enumerate() {
        let operation = index + 1;
        add("script_registration", format!("{name} 1"));
        add(
            "script_registration_site",
            format!("{operation} /reference/source/src/game/test.cpp 7 void AddSC_test()"),
        );
        for slot in SLOTS {
            add("script_callback_binding", format!("{operation} {slot} 0"));
        }
        if id != 0 {
            add("script_binding", format!("{id} {name}"));
            add(
                "script_registry_store",
                format!("{id} {old} 0x{:x} {accepted}", index + 16),
            );
            accepted += 1;
        }
    }
    lines
}
#[test]
fn all_slots_absent_names_and_accepted_overrides_remain_inputs() {
    assert_eq!(SLOTS.len(), 22);
    let mut registry = Registry::default();
    for raw in trace(&[
        ("present", 7, "(nil)"),
        ("absent", 0, "(nil)"),
        ("present", 7, "0x10"),
    ]) {
        registry.feed(&raw).unwrap();
    }
    let report = registry.finish().unwrap();
    assert_eq!(report["attempts"], 3);
    assert_eq!(report["callback_slots"], 66);
    assert_eq!(report["unresolved_attempts"], 1);
    assert_eq!(report["accepted_overrides"], 1);
    assert_eq!(report["final_bindings"], 1);
    assert_eq!(
        report["records"][1]["outcome"],
        "unobserved_binding_outcome"
    );
    assert_eq!(report["records"][2]["outcome"], "accepted_override");
}
#[test]
fn duplicate_callbacks_missing_store_and_forged_previous_binding_refuse() {
    for (mutation, expected) in [
        (0, "scripts.registry_slots"),
        (1, "scripts.registry_store"),
        (2, "scripts.registry_store"),
    ] {
        let mut lines = trace(&[("present", 7, "(nil)")]);
        match mutation {
            0 => {
                lines[3] = String::from_utf8(lines[3].clone())
                    .unwrap()
                    .replace(SLOTS[1], SLOTS[0])
                    .into_bytes()
            }
            1 => {
                lines.pop();
            }
            _ => {
                let last = lines.last_mut().unwrap();
                *last = String::from_utf8(last.clone())
                    .unwrap()
                    .replace("(nil)", "0xff")
                    .into_bytes();
            }
        }
        let mut registry = Registry::default();
        let result = lines
            .iter()
            .try_for_each(|raw| registry.feed(raw))
            .and_then(|()| registry.finish().map(|_| ()));
        assert_eq!(result.unwrap_err().id, expected);
    }
    assert_eq!(
        Registry::default().finish().unwrap_err().id,
        "scripts.registry_slots"
    );
}
#[test]
fn every_factory_kind_refuses_unrequested_duplicate_replacement() {
    for kind in ["creature_ai", "motion", "spell", "aura"] {
        assert_eq!(registry_outcome(kind, false, false).unwrap(), "accepted");
        assert_eq!(
            registry_outcome(kind, true, false).unwrap(),
            "rejected_duplicate"
        );
        if ["spell", "aura"].contains(&kind) {
            assert_eq!(
                registry_outcome(kind, true, true).unwrap_err().id,
                "scripts.registry_replace_not_supported"
            );
        } else {
            assert_eq!(
                registry_outcome(kind, true, true).unwrap(),
                "accepted_override"
            );
        }
    }
    assert_eq!(
        registry_outcome("scriptdev", true, false).unwrap(),
        "accepted_override"
    );
    assert_eq!(
        registry_outcome("unknown", false, false).unwrap_err().id,
        "scripts.registry_kind"
    );
}
#[test]
fn signed_guid_selectors_do_not_become_template_entries() {
    assert_eq!(signed(Some(b"-7")).unwrap(), -7);
    assert_eq!(signed(Some(b"4294967295")).unwrap(), -1);
    assert_eq!(signed(Some(b"-2147483648")).unwrap(), i32::MIN);
}
fn frame(ordinal: u64, value: &[u8]) -> Vec<u8> {
    let mut frame = Vec::new();
    frame.extend_from_slice(&((21 + value.len()) as u32).to_le_bytes());
    frame.extend_from_slice(&ordinal.to_le_bytes());
    frame.extend_from_slice(&1u16.to_le_bytes());
    frame.extend_from_slice(&1u16.to_le_bytes());
    frame.push(0);
    frame.extend_from_slice(&(value.len() as u32).to_le_bytes());
    frame.extend_from_slice(value);
    frame
}
#[test]
fn ordered_inputs_preserve_commands_zero_one_and_arbitrary_tie_occurrences() {
    let path = temp();
    let raw: Vec<_> = frame(0, b"1").into_iter().chain(frame(1, b"0")).collect();
    std::fs::write(&path, &raw).unwrap();
    let descriptor = json!({"rows":2,"bytes":raw.len(),"sha256":hex(&Sha256::digest(&raw))});
    let expected = vec![
        (vec![2], hash_cells([Some(b"0".as_slice())])),
        (vec![2], hash_cells([Some(b"1".as_slice())])),
    ];
    assert_eq!(
        verify_stream(&path, 1, expected.clone(), &descriptor).unwrap()["rows"],
        2
    );
    let wrong_order = vec![
        (vec![1], hash_cells([Some(b"0".as_slice())])),
        (vec![2], hash_cells([Some(b"1".as_slice())])),
    ];
    assert_eq!(
        verify_stream(&path, 1, wrong_order, &descriptor)
            .unwrap_err()
            .id,
        "scripts.order"
    );
    std::fs::write(&path, &raw[..raw.len() - 1]).unwrap();
    assert_eq!(
        verify_stream(&path, 1, expected.clone(), &descriptor)
            .unwrap_err()
            .id,
        "catalog.truncated"
    );
    std::fs::write(&path, [raw.as_slice(), b"x"].concat()).unwrap();
    assert_eq!(
        verify_stream(&path, 1, expected, &descriptor)
            .unwrap_err()
            .id,
        "scripts.trailing"
    );
    std::fs::remove_file(path).unwrap();
}
#[test]
fn linked_source_and_manifest_substitution_refuse() {
    let root = temp();
    std::fs::create_dir(&root).unwrap();
    let binary = root.join("mangosd");
    let observer = root.join("transport-observer.so");
    let trace_path = root.join("trace.tsv");
    let definitions = root.join("definitions.json");
    let build_path = root.join("build.json");
    let manifest_path = root.join("manifest.json");
    let input_path = root.join("input.json");
    std::fs::write(&binary, b"authored-executable").unwrap();
    std::fs::write(&observer, b"authored-observer").unwrap();
    let raw: Vec<_> = trace(&[("present", 7, "(nil)")])
        .into_iter()
        .flatten()
        .collect();
    std::fs::write(&trace_path, &raw).unwrap();
    std::fs::write(&definitions, b"{}").unwrap();
    let desc = |path: &Path| {
        let mut d = hash_file(path, CHUNK as u64).unwrap();
        d["path"] = json!(path.to_str().unwrap());
        d
    };
    let mut recorded_binary = desc(&binary);
    recorded_binary["path"] = json!("/recorded/mangosd");
    let mut recorded_observer = desc(&observer);
    recorded_observer["path"] = json!("/recorded/transport-observer.so");
    std::fs::write(
        &build_path,
        json::canonical(&json!({"files":[recorded_binary,recorded_observer]})),
    )
    .unwrap();
    let mut files = Vec::new();
    let mut input = json!({"format":"benilla-registration-input-v1","linked":{"executable":desc(&binary),"observer":desc(&observer)}});
    for (key, path) in [
        ("trace", &trace_path),
        ("build", &build_path),
        ("definitions", &definitions),
    ] {
        let mut descriptor = desc(path);
        let mut file = identity(&descriptor);
        file["path"] = json!(key);
        files.push(file);
        descriptor["relative"] = json!(key);
        input[key] = descriptor;
    }
    let mut manifest = json!({"core_revision":catalog_schema::POLICY["core_revision"],"classic_db_revision":catalog_schema::POLICY["database_revision"],"reference_execution_sealed":false,"files":files});
    std::fs::write(&manifest_path, json::canonical(&manifest)).unwrap();
    input["manifest"] = desc(&manifest_path);
    std::fs::write(&input_path, json::canonical(&input)).unwrap();
    assert_eq!(
        registration_input(&input_path).unwrap()["registry"]["attempts"],
        1
    );
    std::fs::write(&binary, b"changed-executable").unwrap();
    assert_eq!(
        registration_input(&input_path).unwrap_err().id,
        "scripts.linked_source"
    );
    std::fs::write(&binary, b"authored-executable").unwrap();
    std::fs::write(
        &trace_path,
        [raw.as_slice(), b"1\\t999\\tunrelated\\ttampered\\n"].concat(),
    )
    .unwrap();
    assert_eq!(
        registration_input(&input_path).unwrap_err().id,
        "scripts.input_limit"
    );
    std::fs::write(&trace_path, &raw).unwrap();
    manifest["core_revision"] = json!("wrong");
    std::fs::write(&manifest_path, json::canonical(&manifest)).unwrap();
    input["manifest"] = desc(&manifest_path);
    std::fs::write(&input_path, json::canonical(&input)).unwrap();
    assert_eq!(
        registration_input(&input_path).unwrap_err().id,
        "scripts.linked_source"
    );
    std::fs::remove_dir_all(root).unwrap();
}
