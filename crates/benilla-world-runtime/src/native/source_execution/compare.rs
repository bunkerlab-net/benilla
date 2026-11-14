// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamCursor {
    pub cursor: u64,
    #[serde(deserialize_with = "required_option")]
    pub state_sha256: Option<Sha256Id>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionCursors {
    pub inputs: BTreeMap<String, u64>,
    pub streams: BTreeMap<String, StreamCursor>,
}
#[derive(Clone, Debug, Serialize)]
pub struct FirstDivergence {
    pub reason: Refusal,
    pub prefix: PrefixDescriptor,
    pub matched_records: u64,
    pub reference_cursor: u64,
    pub native_cursor: u64,
    pub expected: Option<TraceRecord>,
    pub actual: Option<TraceRecord>,
    pub expected_predecessors: BTreeSet<EventKey>,
    pub actual_predecessors: BTreeSet<EventKey>,
    pub cursors: ExecutionCursors,
}
#[derive(Clone, Debug, Serialize)]
pub struct ComparisonReport {
    pub prefix: PrefixDescriptor,
    pub matched_records: u64,
    pub reference_gaps: Vec<String>,
    pub native_gaps: Vec<String>,
    pub pending: BTreeSet<PendingF1Obligation>,
    pub whole_execution_accepted: bool,
}

pub struct DifferentialOracle;
impl DifferentialOracle {
    /// Matches logical-thread fronts and causal predecessors, never recorder mutex order.
    pub fn compare(
        prefix: &PrefixDescriptor,
        mut expected: TraceReader,
        mut actual: TraceReader,
        cursors: ExecutionCursors,
    ) -> std::result::Result<ComparisonReport, Box<FirstDivergence>> {
        if expected.prefix != *prefix || actual.prefix != *prefix {
            return Err(divergence(
                refuse("execution.wrong_prefix"),
                prefix,
                0,
                &expected,
                &actual,
                None,
                None,
                &cursors,
            ));
        }
        let mut left = BTreeMap::<EventKey, TraceRecord>::new();
        let mut right = BTreeMap::<EventKey, TraceRecord>::new();
        let mut matched = BTreeSet::<EventKey>::new();
        let mut ends = [false; 2];
        let mut count = 0;
        let pending_limit = expected
            .limits
            .pending_records
            .min(actual.limits.pending_records);
        loop {
            for (side, ended) in ends.iter_mut().enumerate() {
                if *ended {
                    continue;
                }
                let result = if side == 0 {
                    expected.next_record()
                } else {
                    actual.next_record()
                };
                match result {
                    Ok(Some(event)) => {
                        let pending = if side == 0 { &mut left } else { &mut right };
                        pending.insert(event.key(), event);
                    }
                    Ok(None) => *ended = true,
                    Err(reason) => {
                        return Err(divergence(
                            reason,
                            prefix,
                            count,
                            &expected,
                            &actual,
                            left.first_key_value().map(|(_, e)| e.clone()),
                            right.first_key_value().map(|(_, e)| e.clone()),
                            &cursors,
                        ))
                    }
                }
            }
            loop {
                let ready = left
                    .iter()
                    .filter(|(key, e)| {
                        right.contains_key(key)
                            && predecessors(e, &expected.events).is_subset(&matched)
                    })
                    .min_by_key(|(_, e)| e.id)
                    .map(|(key, _)| *key);
                let Some(key) = ready else {
                    break;
                };
                let e = left.remove(&key).expect("ready reference front");
                let a = right.remove(&key).expect("ready native front");
                let ep = predecessors(&e, &expected.events);
                let ap = predecessors(&a, &actual.events);
                let reason = if ep != ap {
                    Some("execution.causal_mismatch")
                } else if !same_operation(&e, &a) {
                    Some("execution.record_mismatch")
                } else {
                    None
                };
                if let Some(reason) = reason {
                    return Err(divergence(
                        refuse(reason),
                        prefix,
                        count,
                        &expected,
                        &actual,
                        Some(e),
                        Some(a),
                        &cursors,
                    ));
                }
                matched.insert(key);
                count += 1;
            }
            if left.len() + right.len() > pending_limit {
                return Err(divergence(
                    refuse("execution.pending_budget"),
                    prefix,
                    count,
                    &expected,
                    &actual,
                    left.first_key_value().map(|(_, e)| e.clone()),
                    right.first_key_value().map(|(_, e)| e.clone()),
                    &cursors,
                ));
            }
            if ends == [true, true] {
                if !left.is_empty() || !right.is_empty() {
                    let reason = if right.is_empty() {
                        "execution.missing_native_record"
                    } else if left.is_empty() {
                        "execution.surplus_native_record"
                    } else {
                        "execution.causal_deadlock"
                    };
                    return Err(divergence(
                        refuse(reason),
                        prefix,
                        count,
                        &expected,
                        &actual,
                        left.into_values().next(),
                        right.into_values().next(),
                        &cursors,
                    ));
                }
                if expected.document().footer.final_state_sha256
                    != actual.document().footer.final_state_sha256
                    || expected.document().header.coverage != actual.document().header.coverage
                {
                    return Err(divergence(
                        refuse("execution.final_state_or_coverage"),
                        prefix,
                        count,
                        &expected,
                        &actual,
                        None,
                        None,
                        &cursors,
                    ));
                }
                return Ok(ComparisonReport {
                    prefix: prefix.clone(),
                    matched_records: count,
                    reference_gaps: expected.document().footer.gaps.clone(),
                    native_gaps: actual.document().footer.gaps.clone(),
                    pending: [
                        PendingF1Obligation::SealedReferenceExecution,
                        PendingF1Obligation::InitialSnapshotJoin,
                        PendingF1Obligation::SqlOccurrenceCausality,
                        PendingF1Obligation::RegistryOccurrences,
                        PendingF1Obligation::CompleteCausalCoverage,
                        PendingF1Obligation::ObserverNonperturbation,
                        PendingF1Obligation::UnsupportedSemanticOwners,
                    ]
                    .into(),
                    whole_execution_accepted: false,
                });
            }
        }
    }
}
fn same_operation(e: &TraceRecord, a: &TraceRecord) -> bool {
    e.key() == a.key()
        && e.kind == a.kind
        && e.operation == a.operation
        && e.parent_operation == a.parent_operation
        && e.world_update == a.world_update
        && e.map_update == a.map_update
        && e.map_instance == a.map_instance
        && e.object == a.object
        && e.callsite == a.callsite
        && e.payload == a.payload
}
fn predecessors(event: &TraceRecord, events: &BTreeMap<u64, EventKey>) -> BTreeSet<EventKey> {
    let mut result: BTreeSet<_> = event.cause_events.iter().map(|id| events[id]).collect();
    if event.thread_sequence > 1 {
        result.insert(EventKey {
            thread: event.thread,
            sequence: event.thread_sequence - 1,
        });
    }
    result
}
#[allow(clippy::too_many_arguments)]
fn divergence(
    reason: Refusal,
    prefix: &PrefixDescriptor,
    matched_records: u64,
    reference: &TraceReader,
    native: &TraceReader,
    expected: Option<TraceRecord>,
    actual: Option<TraceRecord>,
    cursors: &ExecutionCursors,
) -> Box<FirstDivergence> {
    Box::new(FirstDivergence {
        reason,
        prefix: prefix.clone(),
        matched_records,
        reference_cursor: reference.count + 1,
        native_cursor: native.count + 1,
        expected_predecessors: expected
            .as_ref()
            .map(|e| predecessors(e, &reference.events))
            .unwrap_or_default(),
        actual_predecessors: actual
            .as_ref()
            .map(|e| predecessors(e, &native.events))
            .unwrap_or_default(),
        expected,
        actual,
        cursors: cursors.clone(),
    })
}

/// Diagnostic TSV observations cannot become F0 envelopes or admissible inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum SchedulerOperationKind {
    Submit,
    Dispatch,
    Complete,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct SchedulerOperation {
    pub kind: SchedulerOperationKind,
    pub thread: u64,
    pub observed_occurrence: u64,
    pub job: u64,
}
#[derive(Debug, Serialize)]
pub struct SchedulerDivergence {
    pub reason: Refusal,
    pub matched_records: u64,
    pub expected: SchedulerOperation,
    pub actual: SchedulerOperation,
}
impl DifferentialOracle {
    /// Compare independently computed queue operations without inventing F0 causes.
    pub fn compare_scheduler_operation(
        expected: SchedulerOperation,
        actual: SchedulerOperation,
        matched_records: u64,
    ) -> std::result::Result<(), SchedulerDivergence> {
        if expected == actual {
            Ok(())
        } else {
            Err(SchedulerDivergence {
                reason: refuse("execution.scheduler_diagnostic_mismatch"),
                matched_records,
                expected,
                actual,
            })
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ClockPhaseOperation {
    WorldTick {
        previous: u32,
        current: u32,
        diff: u32,
    },
    MapBegin {
        world: u64,
        map: MapIdentity,
        diff: u32,
    },
    MapEndOwner {
        world: u64,
        map: MapIdentity,
    },
}
#[derive(Debug, Serialize)]
pub struct ClockPhaseDivergence {
    pub reason: Refusal,
    pub expected: ClockPhaseOperation,
    pub actual: ClockPhaseOperation,
}
impl DifferentialOracle {
    pub fn compare_clock_phase(
        expected: ClockPhaseOperation,
        actual: ClockPhaseOperation,
    ) -> std::result::Result<(), ClockPhaseDivergence> {
        if expected == actual {
            Ok(())
        } else {
            Err(ClockPhaseDivergence {
                reason: refuse("execution.clock_phase_diagnostic_mismatch"),
                expected,
                actual,
            })
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum GuidProjectionKind {
    AddCounter,
    UpdateFullGuid,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct GuidProjectionOperation {
    pub kind: GuidProjectionKind,
    pub captured_generation: u64,
    pub value: u64,
}
#[derive(Debug, Serialize)]
pub struct GuidProjectionDivergence {
    pub reason: Refusal,
    pub expected: GuidProjectionOperation,
    pub actual: GuidProjectionOperation,
}
impl DifferentialOracle {
    /// Compare GUID composition, never admit observed _Create arguments as allocator inputs.
    pub fn compare_guid_projection(
        expected: GuidProjectionOperation,
        actual: GuidProjectionOperation,
    ) -> std::result::Result<(), GuidProjectionDivergence> {
        if expected == actual {
            Ok(())
        } else {
            Err(GuidProjectionDivergence {
                reason: refuse("execution.guid_projection_mismatch"),
                expected,
                actual,
            })
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum GridProjectionOperation {
    InitialState {
        map: MapIdentity,
        x: u16,
        y: u16,
        phase: u8,
    },
    ObjectData {
        map: MapIdentity,
        x: u16,
        y: u16,
        loaded_before: bool,
    },
    ProductRequest {
        map: MapIdentity,
        x: u16,
        y: u16,
    },
    ForcedUnloadVisit {
        map: MapIdentity,
        x: u16,
        y: u16,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GridProjectionDivergence {
    pub reason: Refusal,
    pub expected: GridProjectionOperation,
    pub actual: GridProjectionOperation,
}
impl DifferentialOracle {
    pub fn compare_grid_projection(
        expected: GridProjectionOperation,
        actual: GridProjectionOperation,
    ) -> std::result::Result<(), GridProjectionDivergence> {
        if expected == actual {
            Ok(())
        } else {
            Err(GridProjectionDivergence {
                reason: refuse("execution.grid_projection_mismatch"),
                expected,
                actual,
            })
        }
    }
}
