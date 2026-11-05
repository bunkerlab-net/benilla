// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
pub(super) struct Temp(pub PathBuf);
impl Temp {
    pub fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "benilla-source-execution-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
pub(super) fn canonical(value: &impl Serialize) -> Vec<u8> {
    serde_json::to_vec(&serde_json::to_value(value).unwrap()).unwrap()
}
pub(super) fn hash(n: u8) -> Sha256Id {
    Sha256Id::of(&[n])
}
pub(super) fn callsite() -> Callsite {
    Callsite {
        core_revision: "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f".into(),
        source_file: "src/shared/Util/Util.cpp".into(),
        line: 68,
        function: "irand".into(),
        inlined_caller_chain: Vec::new(),
        site_ordinal: 1,
    }
}
fn record(id: u64, thread: u64, sequence: u64) -> TraceRecord {
    TraceRecord {
        id,
        kind: EventKind::State,
        thread,
        thread_sequence: sequence,
        operation: None,
        parent_operation: None,
        cause_events: Vec::new(),
        world_update: None,
        map_update: None,
        map_instance: None,
        object: None,
        callsite: callsite(),
        payload: BTreeMap::from([(
            "bits".into(),
            TypedBytes {
                ty: ScalarType::Binary32,
                bytes: 0x80000000_u32.to_le_bytes().into(),
            },
        )]),
    }
}
fn prefix(events: &[TraceRecord]) -> PrefixDescriptor {
    let mut per_thread = BTreeMap::new();
    for e in events {
        *per_thread.entry(e.thread).or_default() += 1;
    }
    PrefixDescriptor {
        binding_sha256: hash(1),
        reference_manifest_sha256: hash(2),
        schema_sha256: hash(3),
        instrumentation_sha256: hash(4),
        projection: Projection::F0StructuralV1,
        start: Boundary {
            name: "P0.before_startup".into(),
            per_thread: BTreeMap::new(),
        },
        end: Boundary {
            name: "authored.component".into(),
            per_thread,
        },
    }
}
struct Fixture {
    dir: Temp,
    document: TraceDocument,
    path: PathBuf,
    digest: Sha256Id,
    prefix: PrefixDescriptor,
}
impl Fixture {
    fn new(events: &[TraceRecord]) -> Self {
        let dir = Temp::new();
        let prefix = prefix(events);
        let mut chunks = Vec::new();
        let mut whole = Sha256::new();
        let mut prior = None;
        for (i, batch) in events.chunks(2).enumerate() {
            let mut raw = Vec::new();
            for event in batch {
                raw.extend(canonical(event));
                raw.push(b'\n');
            }
            whole.update(&raw);
            let digest = Sha256Id::of(&raw);
            std::fs::write(dir.0.join(digest.as_str()), &raw).unwrap();
            chunks.push(TraceChunk {
                index: i as u64,
                first_event: batch.first().unwrap().id,
                last_event: batch.last().unwrap().id,
                bytes: raw.len() as u64,
                sha256: digest.clone(),
                previous_chunk_sha256: prior,
            });
            prior = Some(digest);
        }
        let document = TraceDocument {
            header: TraceHeader {
                format: "benilla-reference-trace-v1".into(),
                reference_manifest_sha256: prefix.reference_manifest_sha256.clone(),
                schema_sha256: prefix.schema_sha256.clone(),
                instrumentation_sha256: prefix.instrumentation_sha256.clone(),
                start_boundary: prefix.start.clone(),
                coverage: BTreeSet::from(["authored.state".into()]),
            },
            chunks,
            footer: TraceFooter {
                end_boundary: prefix.end.clone(),
                event_count: events.len() as u64,
                per_thread_counts: prefix.end.per_thread.clone(),
                per_stream_counts: BTreeMap::new(),
                final_state_sha256: hash(5),
                completeness: false,
                gaps: vec!["semantic_owners_pending".into()],
                trace_sha256: Sha256Id(format!("{:x}", whole.finalize())),
            },
        };
        let path = dir.0.join("trace.json");
        let mut fixture = Self {
            dir,
            document,
            path,
            digest: hash(0),
            prefix,
        };
        fixture.save();
        fixture
    }
    fn save(&mut self) {
        let raw = canonical(&self.document);
        std::fs::write(&self.path, &raw).unwrap();
        self.digest = Sha256Id::of(&raw);
    }
    fn open(&self) -> Result<TraceReader> {
        self.open_limits(TraceLimits::default())
    }
    fn open_limits(&self, limits: TraceLimits) -> Result<TraceReader> {
        TraceReader::open(&self.path, &self.dir.0, &self.digest, &self.prefix, limits)
    }
}
fn compare(
    left: &[TraceRecord],
    right: &[TraceRecord],
) -> std::result::Result<ComparisonReport, Box<FirstDivergence>> {
    let l = Fixture::new(left);
    let r = Fixture::new(right);
    DifferentialOracle::compare(
        &l.prefix,
        l.open().unwrap(),
        r.open().unwrap(),
        ExecutionCursors::default(),
    )
}
#[test]
fn independent_interleavings_match_without_recorder_order() {
    let left = [
        record(1, 1, 1),
        record(2, 2, 1),
        record(3, 1, 2),
        record(4, 2, 2),
    ];
    let right = [
        record(1, 2, 1),
        record(2, 2, 2),
        record(3, 1, 1),
        record(4, 1, 2),
    ];
    let result = compare(&left, &right).unwrap();
    assert_eq!(result.matched_records, 4);
    assert!(!result.whole_execution_accepted);
    assert!(result
        .pending
        .contains(&PendingF1Obligation::SealedReferenceExecution));
}
#[test]
fn causal_edges_match_by_logical_event_not_recording_id() {
    let mut left = vec![record(1, 1, 1), record(2, 2, 1), record(3, 3, 1)];
    left[2].cause_events = vec![1];
    let mut right = vec![record(1, 2, 1), record(2, 1, 1), record(3, 3, 1)];
    right[2].cause_events = vec![2];
    assert_eq!(compare(&left, &right).unwrap().matched_records, 3);
    right[2].cause_events = vec![1];
    let refusal = compare(&left, &right).unwrap_err();
    assert_eq!(refusal.reason.id, "execution.causal_mismatch");
    assert_eq!(refusal.expected.unwrap().id, 3);
    assert_eq!(refusal.actual.unwrap().id, 3);
}
#[test]
fn first_raw_bit_difference_stops_without_resynchronization() {
    let left = [record(1, 1, 1), record(2, 1, 2), record(3, 1, 3)];
    let mut right = left.clone();
    right[1].payload.get_mut("bits").unwrap().bytes = 0_u32.to_le_bytes().into();
    right[2].payload.get_mut("bits").unwrap().bytes = 0_u32.to_le_bytes().into();
    let report = compare(&left, &right).unwrap_err();
    assert_eq!(report.matched_records, 1);
    assert_eq!(report.expected.unwrap().thread_sequence, 2);
    assert_eq!(report.actual.unwrap().thread_sequence, 2);
}
#[test]
fn reversed_fifo_edge_refuses() {
    let mut left = [record(1, 1, 1), record(2, 2, 1)];
    left[1].cause_events = vec![1];
    let mut right = [record(1, 2, 1), record(2, 1, 1)];
    right[1].cause_events = vec![1];
    assert_eq!(
        compare(&left, &right).unwrap_err().reason.id,
        "execution.causal_mismatch"
    );
}
#[test]
fn duplicate_or_out_of_order_thread_records_refuse() {
    for seq in [1, 3] {
        let fixture = Fixture::new(&[record(1, 1, 1), record(2, 1, seq)]);
        let mut reader = fixture.open().unwrap();
        assert!(reader.next_record().unwrap().is_some());
        assert_eq!(
            reader.next_record().unwrap_err().id,
            "execution.thread_order"
        );
        assert_eq!(reader.next_record().unwrap_err().id, "execution.stopped");
    }
}
#[test]
fn duplicate_global_event_refuses_even_with_rehashed_metadata() {
    let mut fixture = Fixture::new(&[record(1, 1, 1), record(1, 1, 2)]);
    fixture.document.chunks[0].last_event = 2;
    fixture.save();
    let mut reader = fixture.open().unwrap();
    reader.next_record().unwrap();
    assert_eq!(
        reader.next_record().unwrap_err().id,
        "execution.event_order"
    );
}
#[test]
fn broken_cause_parent_and_payload_width_refuse() {
    let mut e = record(1, 1, 1);
    e.cause_events = vec![9];
    let fixture = Fixture::new(&[e.clone()]);
    assert_eq!(
        fixture.open().unwrap().next_record().unwrap_err().id,
        "execution.broken_cause"
    );
    e.cause_events.clear();
    e.operation = Some(OperationId {
        thread: 1,
        ordinal: 1,
    });
    e.parent_operation = Some(OperationId {
        thread: 1,
        ordinal: 9,
    });
    let fixture = Fixture::new(&[e.clone()]);
    assert_eq!(
        fixture.open().unwrap().next_record().unwrap_err().id,
        "execution.broken_parent"
    );
    e.operation = None;
    e.parent_operation = None;
    e.payload.get_mut("bits").unwrap().bytes.pop();
    let fixture = Fixture::new(&[e]);
    assert_eq!(
        fixture.open().unwrap().next_record().unwrap_err().id,
        "execution.payload_width"
    );
}
#[test]
fn unfinished_operation_is_a_valid_nonquiescent_prefix() {
    let mut event = record(1, 1, 1);
    event.kind = EventKind::OperationBegin;
    event.operation = Some(OperationId {
        thread: 1,
        ordinal: 1,
    });
    assert_eq!(
        compare(&[event.clone()], &[event]).unwrap().matched_records,
        1
    );
}
#[test]
fn stale_object_and_map_generations_refuse() {
    let mut e = record(1, 1, 1);
    e.map_instance = Some(MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    });
    e.object = Some(ObjectIdentity {
        object_type: "creature".into(),
        source_guid: Some(1),
        runtime_guid: Some(1),
        construction_generation: 1,
    });
    let mut a = e.clone();
    a.object.as_mut().unwrap().construction_generation = 2;
    assert_eq!(
        compare(&[e.clone()], &[a]).unwrap_err().reason.id,
        "execution.record_mismatch"
    );
    let mut a = e.clone();
    a.map_instance.as_mut().unwrap().creation_generation = 2;
    assert_eq!(
        compare(&[e], &[a]).unwrap_err().reason.id,
        "execution.record_mismatch"
    );
}
#[test]
fn missing_surplus_and_wrong_prefix_refuse() {
    let mut fixture = Fixture::new(&[record(1, 1, 1), record(2, 1, 2)]);
    fixture.prefix.end.per_thread.insert(1, 1);
    assert_eq!(fixture.open().err().unwrap().id, "execution.wrong_prefix");
    fixture.document.footer.end_boundary = fixture.prefix.end.clone();
    fixture.save();
    assert_eq!(fixture.open().err().unwrap().id, "execution.footer_counts");
    fixture.document.footer.per_thread_counts.insert(1, 1);
    fixture.document.footer.event_count = 1;
    fixture.save();
    assert_eq!(fixture.open().err().unwrap().id, "execution.chunk_chain");
}
#[test]
fn missing_and_surplus_envelopes_refuse_after_rehashing_chunk() {
    for actual in [
        vec![record(1, 1, 1)],
        vec![record(1, 1, 1), record(2, 1, 2), record(3, 1, 3)],
    ] {
        let mut fixture = Fixture::new(&[record(1, 1, 1), record(2, 1, 2)]);
        let mut raw = Vec::new();
        for e in actual {
            raw.extend(canonical(&e));
            raw.push(b'\n');
        }
        let hash = Sha256Id::of(&raw);
        std::fs::write(fixture.dir.0.join(hash.as_str()), &raw).unwrap();
        fixture.document.chunks[0].bytes = raw.len() as u64;
        fixture.document.chunks[0].sha256 = hash.clone();
        fixture.document.footer.trace_sha256 = hash;
        fixture.save();
        let mut reader = fixture.open().unwrap();
        let mut refusal = None;
        for _ in 0..4 {
            match reader.next_record() {
                Err(e) => {
                    refusal = Some(e);
                    break;
                }
                Ok(None) => break,
                _ => (),
            }
        }
        assert_eq!(refusal.unwrap().id, "execution.chunk_events");
    }
}
#[test]
fn budget_refuses_before_execution_and_pending_records_are_bounded() {
    let fixture = Fixture::new(&[record(1, 1, 1), record(2, 1, 2)]);
    assert_eq!(
        fixture
            .open_limits(TraceLimits {
                history_events: 1,
                ..TraceLimits::default()
            })
            .err()
            .unwrap()
            .id,
        "execution.memory_budget"
    );
    let left = Fixture::new(&[
        record(1, 1, 1),
        record(2, 1, 2),
        record(3, 2, 1),
        record(4, 2, 2),
    ]);
    let right = Fixture::new(&[
        record(1, 2, 1),
        record(2, 2, 2),
        record(3, 1, 1),
        record(4, 1, 2),
    ]);
    let limits = TraceLimits {
        pending_records: 1,
        ..TraceLimits::default()
    };
    assert_eq!(
        DifferentialOracle::compare(
            &left.prefix,
            left.open_limits(limits).unwrap(),
            right.open_limits(limits).unwrap(),
            ExecutionCursors::default()
        )
        .unwrap_err()
        .reason
        .id,
        "execution.pending_budget"
    );
}
#[test]
fn truncated_chunk_footer_and_missing_explicit_none_refuse() {
    let fixture = Fixture::new(&[record(1, 1, 1)]);
    let chunk = fixture
        .dir
        .0
        .join(fixture.document.chunks[0].sha256.as_str());
    let mut bytes = std::fs::read(&chunk).unwrap();
    bytes.pop();
    std::fs::write(&chunk, &bytes).unwrap();
    assert!(fixture.open().is_err());
    let mut document = serde_json::to_value(&fixture.document).unwrap();
    document.as_object_mut().unwrap().remove("footer");
    assert!(decode::<TraceDocument>(&canonical(&document), 1_048_576).is_err());
    let mut event = serde_json::to_value(record(1, 1, 1)).unwrap();
    event.as_object_mut().unwrap().remove("object");
    assert!(decode::<TraceRecord>(&canonical(&event), 65_536).is_err());
}
#[test]
fn claimed_complete_does_not_prove_execution() {
    let mut left = Fixture::new(&[record(1, 1, 1)]);
    left.document.footer.completeness = true;
    left.document.footer.gaps.clear();
    left.save();
    let report = DifferentialOracle::compare(
        &left.prefix,
        left.open().unwrap(),
        left.open().unwrap(),
        ExecutionCursors::default(),
    )
    .unwrap();
    assert!(!report.whole_execution_accepted);
    assert!(!report.pending.is_empty());
}
#[test]
fn retained_original_f1_rng_projection_or_clean_absence() {
    let explicit = std::env::var_os("BENILLA_F1_RNG_TRACE");
    let path = explicit.clone().map(PathBuf::from).unwrap_or_else(|| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("workspace root")
            .join("benilla-config/full-server/f1/run/startup-through-third-map0.tsv")
    });
    if explicit.is_none() && !path.try_exists().unwrap() {
        eprintln!("SKIP private F1 RNG projection: set BENILLA_F1_RNG_TRACE to retained original startup-through-third-map0.tsv");
        return;
    }
    let report = compare_original_unsealed_rng(&path).unwrap();
    assert_eq!(report.initial_states, 8);
    assert_eq!(report.raw_words, 69_586);
    assert_eq!(report.semantic_draws, 27_865);
    assert!(!report.whole_execution_accepted);
    println!("{}", serde_json::to_string(&report).unwrap());
}

include!("clock_projection_tests.rs");
