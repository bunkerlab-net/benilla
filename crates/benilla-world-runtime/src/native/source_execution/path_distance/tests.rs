// SPDX-License-Identifier: GPL-3.0-or-later
use super::super::tests::Temp;
use super::*;

fn owner(caller: DistanceCaller) -> DistanceReadOwner {
    DistanceReadOwner {
        caller,
        thread: 7,
        object_generation: 30064771139,
        occurrence: 1,
    }
}
fn observation(caller: DistanceCaller) -> PathNearestObservation {
    PathNearestObservation {
        owner: owner(caller),
        point_bits: [0., -2., 0.].map(f32::to_bits),
        extent_bits: [5.0_f32.to_bits(); 3],
        include_flags: 1,
        exclude_flags: 0,
        area_cost_bits: vec![1.0_f32.to_bits(); 64],
        status: 0x40000000,
        polygon: 4503599627370496,
        closest_bits: [0; 3],
        distance_bits: 2.0_f32.to_bits(),
    }
}
fn fixture(raw: &[u8], records: u64) -> (Temp, InputStream) {
    let temp = Temp::new();
    std::fs::write(temp.0.join("storage.jsonl"), raw).unwrap();
    let stream = InputStream {
        receipt: FileReceipt {
            name: "storage.jsonl".into(),
            bytes: raw.len() as u64,
            sha256: Sha256Id::of(raw),
        },
        records,
    };
    (temp, stream)
}
fn row(caller: u8, occurrence: u64, event: u64, bits: u32) -> Vec<u8> {
    let bytes_le: String = bits
        .to_le_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let mut raw = serde_json::to_vec(&serde_json::json!({"caller":caller,"thread":7,
        "object_generation":30064771139_u64,"occurrence":occurrence,"width_bytes":4,
        "bytes_le":bytes_le,"source_event":event}))
    .unwrap();
    raw.push(b'\n');
    raw
}
#[test]
fn all_five_declarations_bind_exactly_without_float_canonicalization() {
    let bits = [0, 0x80000000, 0x7fc00001, 0x7f800000, 0xff800000];
    let raw: Vec<u8> = bits
        .iter()
        .enumerate()
        .flat_map(|(i, b)| row(i as u8 + 1, i as u64 + 1, i as u64 + 10, *b))
        .collect();
    let (temp, stream) = fixture(&raw, 5);
    let mut tape = ReferenceDistanceStorage::open(&temp.0, &stream).unwrap();
    assert!(tape.finish().is_err());
    for (i, bits) in bits.into_iter().enumerate() {
        let caller = DistanceCaller::try_from(i as u8 + 1).unwrap();
        assert_eq!(caller.declaration_line(), [410, 410, 717, 830, 1400][i]);
        let mut owner = owner(caller);
        owner.occurrence = i as u64 + 1;
        assert_eq!(tape.read(&owner, 4, i as u64 + 10).unwrap(), bits);
    }
    tape.finish().unwrap();
    assert!(tape
        .read(&owner(DistanceCaller::RandomPoint), 4, 15)
        .is_err());
}
#[test]
fn wrong_caller_thread_generation_width_occurrence_and_event_do_not_consume() {
    let (temp, stream) = fixture(&row(1, 1, 10, 0x80000000), 1);
    let mut tape = ReferenceDistanceStorage::open(&temp.0, &stream).unwrap();
    let expected = owner(DistanceCaller::Start);
    for change in 0..6 {
        let mut wrong = expected.clone();
        let mut width = 4;
        let mut event = 10;
        match change {
            0 => wrong.caller = DistanceCaller::End,
            1 => wrong.thread = 8,
            2 => wrong.object_generation += 1,
            3 => wrong.occurrence += 1,
            4 => width = 8,
            5 => event += 1,
            _ => unreachable!(),
        }
        assert!(tape.read(&wrong, width, event).is_err());
    }
    assert_eq!(tape.read(&expected, 4, 10).unwrap(), 0x80000000);
    tape.finish().unwrap();
}
#[test]
fn malformed_storage_and_wrong_hash_refuse() {
    for raw in [
        row(6, 1, 10, 0),
        row(1, 2, 10, 0),
        b"{}\n".to_vec(),
        b"{}".to_vec(),
    ] {
        let (temp, stream) = fixture(&raw, 1);
        assert!(ReferenceDistanceStorage::open(&temp.0, &stream).is_err());
    }
    let (temp, mut stream) = fixture(&row(1, 1, 10, 0), 1);
    stream.receipt.sha256 = Sha256Id::of(b"different");
    assert!(ReferenceDistanceStorage::open(&temp.0, &stream).is_err());
    for (width, text) in [
        (8, "00000000"),
        (4, "000000"),
        (4, "zz000000"),
        (4, "é000000"),
    ] {
        assert!(storage_bits(width, text).is_err());
    }
}
#[test]
fn absent_intent_differs_only_when_reference_bytes_reject_the_nearest() {
    for caller in 1..=5 {
        let query = observation(DistanceCaller::try_from(caller).unwrap());
        for (bits, reference_appended) in [
            (0, true),
            (0x80000000, true),
            (1.0_f32.to_bits(), false),
            (2.0_f32.to_bits(), false),
            (3.0_f32.to_bits(), true),
            (f32::NAN.to_bits(), false),
            (f32::INFINITY.to_bits(), true),
            (f32::NEG_INFINITY.to_bits(), false),
        ] {
            let report = DifferentialOracle::compare_path_distance_lookup(
                10,
                &query,
                ReferencePriorDistance::Unwritten {
                    recorded_bits: bits,
                },
                &query,
                None,
                true,
            )
            .unwrap();
            assert_eq!(report.reference_appended, reference_appended);
            assert_eq!(
                report.approved_absent_distance_difference,
                !reference_appended
            );
        }
    }
}
#[test]
fn defined_warm_distance_and_unrelated_geometry_mismatches_are_never_waived() {
    let query = observation(DistanceCaller::Stepped);
    for bits in [
        0,
        0x80000000,
        1.0_f32.to_bits(),
        2.0_f32.to_bits(),
        3.0_f32.to_bits(),
    ] {
        let appended = reference_append(bits, query.distance_bits);
        let report = DifferentialOracle::compare_path_distance_lookup(
            10,
            &query,
            ReferencePriorDistance::Defined { bits },
            &query,
            Some(bits),
            appended,
        )
        .unwrap();
        assert!(!report.approved_absent_distance_difference);
        assert!(DifferentialOracle::compare_path_distance_lookup(
            10,
            &query,
            ReferencePriorDistance::Defined { bits },
            &query,
            None,
            true
        )
        .is_err());
    }
    for change in 0..5 {
        let mut native = query.clone();
        match change {
            0 => native.polygon += 1,
            1 => native.include_flags = 3,
            2 => native.owner.object_generation += 1,
            3 => native.area_cost_bits[63] = 2.0_f32.to_bits(),
            4 => native.distance_bits = 3.0_f32.to_bits(),
            _ => unreachable!(),
        }
        assert!(DifferentialOracle::compare_path_distance_lookup(
            10,
            &query,
            ReferencePriorDistance::Unwritten { recorded_bits: 1 },
            &native,
            None,
            true
        )
        .is_err());
    }
    assert!(DifferentialOracle::compare_path_distance_lookup(
        10,
        &query,
        ReferencePriorDistance::Unwritten { recorded_bits: 1 },
        &query,
        None,
        false
    )
    .is_err());
}
#[test]
fn actual_native_near_and_far_mesh_results_match_the_authored_predicate_report() {
    use benilla_native_nav::{
        Mesh, SourceWarmCacheBudget, SourceWarmPrimitive, TileInput,
        SOURCE_PATH_DISTANCE_INTENT_VERSION,
    };
    use std::time::{Duration, Instant};
    let params = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../benilla-native-nav/tools/fixtures/direct-corridor/params.test-fixture"
    ));
    let tile = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../benilla-native-nav/tools/fixtures/warm-cache/precision.test-fixture"
    ));
    for (y, extent, primitive) in [
        (-2.0_f32, 5.0_f32, SourceWarmPrimitive::Nearest5),
        (-6., 10., SourceWarmPrimitive::Nearest10),
    ] {
        let mut mesh = Mesh::load(
            params,
            &[TileInput {
                grid: [31, 31],
                bytes: tile,
            }],
        )
        .unwrap();
        let point = [5., y, 0.].map(f32::to_bits);
        let result = mesh
            .loaded_source_path_distance_intent(
                SOURCE_PATH_DISTANCE_INTENT_VERSION,
                point,
                point,
                SourceWarmCacheBudget::default(),
                Instant::now() + Duration::from_secs(5),
            )
            .unwrap();
        let start = result.start();
        let trace = result
            .trace()
            .iter()
            .find(|trace| trace.endpoint == 0 && trace.primitive == primitive)
            .unwrap();
        // Compare the actual nearest result with the authored rectangle edge; PathFinder.cpp:358-383.
        let mut expected = observation(DistanceCaller::Start);
        expected.point_bits = [y, 0., 5.].map(f32::to_bits);
        expected.closest_bits = [0., 0., 5.].map(f32::to_bits);
        expected.extent_bits = [extent.to_bits(); 3];
        expected.distance_bits = (-y).to_bits();
        let mut actual = expected.clone();
        actual.distance_bits = start.distance_bits;
        actual.polygon = start.polygon;
        actual.status = trace.status;
        let report = DifferentialOracle::compare_path_distance_lookup(
            10,
            &expected,
            ReferencePriorDistance::Unwritten {
                recorded_bits: 1.0_f32.to_bits(),
            },
            &actual,
            None,
            start.appended,
        )
        .unwrap();
        assert!(report.approved_absent_distance_difference);
        assert!(!report.reference_appended && report.native_appended);
        assert!(report.native_nearest.is_some());
    }
}
#[test]
fn retained_round5_storage_stream_or_clean_absence() {
    let explicit = std::env::var_os("BENILLA_F1_DISTANCE_STORAGE");
    let path = explicit.clone().map(std::path::PathBuf::from).unwrap_or_else(|| {
        Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap()
            .join("benilla-config/full-server/f1/sealing/source-choice/stop-full-05/initial-storage-input.jsonl")
    });
    if explicit.is_none() && !path.try_exists().unwrap() {
        eprintln!("SKIP round-5 storage inputs: set BENILLA_F1_DISTANCE_STORAGE");
        return;
    }
    let stream = InputStream {
        records: 2121,
        receipt: FileReceipt {
            name: path.file_name().unwrap().to_str().unwrap().into(),
            bytes: 286168,
            sha256: Sha256Id::parse(
                "1cbf1f7be701ec7a5139cab5b55d2cab391bb78514baa467c9da459d0e00e981",
            )
            .unwrap(),
        },
    };
    let mut tape = ReferenceDistanceStorage::open(path.parent().unwrap(), &stream).unwrap();
    let reads: Vec<_> = tape
        .threads
        .values()
        .flat_map(|(reads, _)| {
            reads
                .iter()
                .map(|read| (read.owner.clone(), read.bits, read.event))
        })
        .collect();
    let mut counts = BTreeMap::new();
    for (owner, bits, event) in reads {
        assert_eq!(tape.read(&owner, 4, event).unwrap(), bits);
        *counts.entry(u8::from(owner.caller)).or_insert(0) += 1;
    }
    tape.finish().unwrap();
    assert_eq!(counts, BTreeMap::from([(1, 1181), (2, 581), (5, 359)]));
    eprintln!(
        "Bound 2121 round-5 storage reads. Callers 3 and 4 have no observed first unwritten read."
    );
}
#[test]
fn retained_seal8_distance_report_or_clean_absence() {
    let explicit = std::env::var_os("BENILLA_F1_SEAL8_FRAMING");
    let path = explicit
        .clone()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(3)
                .unwrap()
                .join("benilla-config/full-server/f1/sealing/source-choice/seal8-full-framing-08")
        });
    if explicit.is_none() && !path.try_exists().unwrap() {
        eprintln!("SKIP seal8 distance projection: set BENILLA_F1_SEAL8_FRAMING");
        return;
    }
    let report = compare_seal8_unsealed_distances(&path).unwrap();
    assert_eq!(report.storage_reads_bound, 1457);
    assert_eq!(
        report.by_caller,
        BTreeMap::from([(1, 879), (2, 248), (5, 330)])
    );
    assert!(report.strict.first_recorded_predicate_difference.is_some());
    assert_eq!(
        report.strict.matching_predicates_before_first_difference,
        186
    );
    let first = report
        .strict
        .first_recorded_predicate_difference
        .as_ref()
        .unwrap();
    assert_eq!(first.source_event, 31_027_447);
    assert_eq!(first.owner.caller, DistanceCaller::RandomPoint);
    assert_eq!((first.owner.thread, first.owner.occurrence), (9, 7));
    assert!(!first.reference_appended && first.native_appended);
    assert!(first.native_nearest.is_none());
    assert_eq!(report.intent.equal_predicates, 957);
    assert_eq!(report.intent.absent_distance_differences, 500);
    assert!(report.strict.matched_world_records.is_none());
    assert!(!report.intent.native_geometry_executed);
    assert!(!report.intent.post_deviation_gameplay_accepted);
    assert!(!report.reference_execution_sealed && !report.whole_execution_accepted);
    eprintln!("{}", serde_json::to_string_pretty(&report).unwrap());
}
