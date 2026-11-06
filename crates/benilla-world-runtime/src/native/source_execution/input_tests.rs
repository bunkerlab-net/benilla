// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::native::source_execution::tests::{callsite, canonical, hash, Temp};
fn owner() -> InputOwner {
    InputOwner {
        thread: 1,
        occurrence: 1,
        operation: OperationId {
            thread: 1,
            ordinal: 1,
        },
        map_instance: None,
        object: Some(ObjectIdentity {
            object_type: "creature".into(),
            source_guid: Some(7),
            runtime_guid: Some(7),
            construction_generation: 1,
        }),
        callsite: callsite(),
    }
}
fn clock() -> AdmissibleInput {
    AdmissibleInput::ClockObservation {
        clock: "steady_ms".into(),
        value: TypedBytes {
            ty: ScalarType::U32,
            bytes: 42_u32.to_le_bytes().into(),
        },
    }
}
fn binding() -> ExecutionBinding {
    ExecutionBinding {
        format: "benilla-source-execution-binding-v1".into(),
        catalog_sha256: Sha256Id::parse(F2_CATALOG_SHA256).unwrap(),
        preparation_sha256: Sha256Id::parse(F2_PREPARATION_SHA256).unwrap(),
        source_snapshot: "authored-not-F1".into(),
        schema_root_sha256: hash(1),
        preparation_input_bindings: serde_json::json!({"initial_snapshot":{"bytes":0,"sha256":hash(2)}}),
        reference_manifest_sha256: None,
        build_sha256: hash(4),
        schema_sha256: hash(5),
        instrumentation_sha256: hash(6),
        initial_state: FileReceipt {
            name: "initial".into(),
            bytes: 0,
            sha256: hash(2),
        },
        streams: Vec::new(),
        f1_binding: F1Binding::Pending {
            obligations: REQUIRED_PENDING.into(),
        },
    }
}
fn tape(dir: &Temp, records: &[InputRecord]) -> Tape {
    let mut raw = Vec::new();
    for record in records {
        let record = canonical(record);
        raw.extend((record.len() as u32).to_le_bytes());
        raw.extend(record);
    }
    std::fs::write(dir.0.join("input"), &raw).unwrap();
    let descriptor = InputStream {
        receipt: FileReceipt {
            name: "input".into(),
            bytes: raw.len() as u64,
            sha256: Sha256Id::of(&raw),
        },
        records: records.len() as u64,
    };
    let held = descriptor.receipt.open(&dir.0, 1_048_576).unwrap();
    Tape {
        descriptor,
        held,
        offset: 0,
        cursor: 0,
    }
}
#[test]
fn independent_anchor_rejects_forged_and_rehashed_binding_fields() {
    let dir = Temp::new();
    let path = dir.0.join("binding");
    let original = canonical(&binding());
    let approved = Sha256Id::of(&original);
    std::fs::write(&path, &original).unwrap();
    anchored_binding(&path, &approved).unwrap();
    for field in [
        "catalog_sha256",
        "preparation_sha256",
        "schema_root_sha256",
        "build_sha256",
        "reference_manifest_sha256",
        "instrumentation_sha256",
        "preparation_input_bindings",
        "source_snapshot",
    ] {
        let mut forged = serde_json::to_value(binding()).unwrap();
        forged[field] = serde_json::json!(hash(9));
        let rehashed = canonical(&forged);
        assert_ne!(Sha256Id::of(&rehashed), approved);
        std::fs::write(&path, rehashed).unwrap();
        assert_eq!(
            anchored_binding(&path, &approved).err().unwrap().id,
            "execution.binding_anchor"
        );
    }
}
#[test]
fn a_rehashed_preparation_never_replaces_the_frozen_f2_identity() {
    let mut prepared = serde_json::json!({"catalog_sha256":F2_CATALOG_SHA256,"complete":false,"format":"benilla-native-catalog-preparation-v2","source_snapshot":"authored-not-F1","input_bindings":{}});
    let mut hash = Sha256::new();
    hash.update(b"benilla-native-catalog-preparation-v2\0");
    hash.update(canonical(&prepared));
    prepared["preparation_sha256"] = serde_json::json!(format!("{:x}", hash.finalize()));
    let catalog = serde_json::json!({"catalog_sha256":F2_CATALOG_SHA256});
    assert_eq!(
        validate_binding(&binding(), &prepared, &catalog)
            .unwrap_err()
            .id,
        "execution.preparation_identity"
    );
    prepared["preparation_sha256"] = serde_json::json!(F2_PREPARATION_SHA256);
    assert_eq!(
        validate_binding(&binding(), &prepared, &catalog)
            .unwrap_err()
            .id,
        "execution.preparation_identity"
    );
}
#[test]
fn wrong_input_owner_callsite_generation_and_occurrence_refuse_without_advancing() {
    let dir = Temp::new();
    let mut tape = tape(
        &dir,
        &[InputRecord {
            owner: owner(),
            value: clock(),
        }],
    );
    for change in 0..4 {
        let mut wrong = owner();
        match change {
            0 => wrong.thread = 2,
            1 => wrong.callsite.site_ordinal = 2,
            2 => wrong.object.as_mut().unwrap().construction_generation = 2,
            _ => wrong.occurrence = 2,
        }
        assert_eq!(
            tape.next(&wrong).unwrap_err().id,
            "execution.input_identity"
        );
        assert_eq!(tape.cursor, 0);
    }
    assert_eq!(tape.next(&owner()).unwrap(), clock());
    tape.finish().unwrap();
    assert_eq!(
        tape.next(&owner()).unwrap_err().id,
        "execution.input_exhausted"
    );
}
#[test]
fn wrong_width_and_truncated_frames_refuse() {
    let dir = Temp::new();
    let invalid = AdmissibleInput::ClockObservation {
        clock: "steady_ms".into(),
        value: TypedBytes {
            ty: ScalarType::U32,
            bytes: vec![1],
        },
    };
    let mut tape = tape(
        &dir,
        &[InputRecord {
            owner: owner(),
            value: invalid,
        }],
    );
    assert_eq!(
        tape.next(&owner()).unwrap_err().id,
        "execution.payload_width"
    );
    std::fs::write(dir.0.join("short"), [8, 0, 0, 0, b'{']).unwrap();
    let mut tape = Tape {
        descriptor: InputStream {
            receipt: FileReceipt {
                name: "short".into(),
                bytes: 5,
                sha256: Sha256Id::of(&[8, 0, 0, 0, b'{']),
            },
            records: 1,
        },
        held: ExecutionFile::open(&dir.0.join("short"), 65_536).unwrap(),
        cursor: 0,
        offset: 0,
    };
    assert_eq!(tape.next(&owner()).unwrap_err().id, "catalog.truncated");
}
#[test]
fn missing_or_surplus_inputs_refuse() {
    let dir = Temp::new();
    let mut tape = tape(
        &dir,
        &[InputRecord {
            owner: owner(),
            value: clock(),
        }],
    );
    assert_eq!(tape.finish().unwrap_err().id, "execution.input_surplus");
    tape.descriptor.records = 0;
    assert_eq!(
        tape.next(&owner()).unwrap_err().id,
        "execution.input_exhausted"
    );
    assert_eq!(tape.finish().unwrap_err().id, "execution.input_surplus");
}
#[test]
fn expected_semantic_results_and_reference_storage_are_not_inputs() {
    for family in [
        "rng_raw",
        "rng_semantic",
        "geometry_answer",
        "update_list",
        "callback_result",
        "unwritten_distance",
    ] {
        let value = serde_json::json!({"family":family,"bytes":[1,2,3,4]});
        assert!(decode::<AdmissibleInput>(&canonical(&value), 65_536).is_err());
    }
    let mut forged = serde_json::to_value(binding()).unwrap();
    forged["complete"] = serde_json::json!(true);
    assert!(decode::<ExecutionBinding>(&canonical(&forged), 1_048_576).is_err());
    forged.as_object_mut().unwrap().remove("complete");
    forged["f1_binding"] = serde_json::json!({"status":"sealed","complete":true});
    assert!(decode::<ExecutionBinding>(&canonical(&forged), 1_048_576).is_err());
}
#[test]
fn duplicate_json_fields_unknown_fields_and_noncanonical_data_refuse() {
    assert!(decode::<serde_json::Value>(br#"{"a":1,"a":2}"#, 65_536).is_err());
    assert!(decode::<serde_json::Value>(br#"{ "a":1}"#, 65_536).is_err());
    let mut value = serde_json::to_value(owner()).unwrap();
    value["unexpected"] = serde_json::json!(1);
    assert!(decode::<InputOwner>(&canonical(&value), 65_536).is_err());
}
#[test]
fn retained_handles_reject_mutation_and_never_reopen_replaced_paths() {
    let dir = Temp::new();
    let mut tape = tape(
        &dir,
        &[InputRecord {
            owner: owner(),
            value: clock(),
        }],
    );
    #[cfg(unix)]
    {
        std::fs::rename(dir.0.join("input"), dir.0.join("original")).unwrap();
        std::fs::write(dir.0.join("input"), b"forged replacement").unwrap();
        match tape.next(&owner()) {
            Ok(value) => assert_eq!(value, clock()),
            Err(e) => assert_eq!(e.id, "catalog.changed"),
        }
        std::fs::write(dir.0.join("original"), b"mutated held inode").unwrap();
        assert_eq!(tape.finish().unwrap_err().id, "catalog.changed");
    }
    #[cfg(windows)]
    {
        assert_eq!(
            std::fs::rename(dir.0.join("input"), dir.0.join("original"))
                .unwrap_err()
                .raw_os_error(),
            Some(32)
        );
        assert_eq!(
            std::fs::write(dir.0.join("input"), b"forged replacement")
                .unwrap_err()
                .raw_os_error(),
            Some(32)
        );
        assert_eq!(tape.next(&owner()).unwrap(), clock());
        tape.finish().unwrap();
    }
}
#[test]
fn file_receipts_reject_byte_changes_even_at_the_same_length() {
    let dir = Temp::new();
    std::fs::write(dir.0.join("initial"), b"abcd").unwrap();
    let receipt = FileReceipt {
        name: "initial".into(),
        bytes: 4,
        sha256: Sha256Id::of(b"abce"),
    };
    assert_eq!(
        receipt.open(&dir.0, 10).err().unwrap().id,
        "execution.file_hash"
    );
    let mut wrong_path = receipt;
    wrong_path.name = "../initial".into();
    assert_eq!(
        wrong_path.open(&dir.0, 10).err().unwrap().id,
        "execution.relative_path"
    );
}
