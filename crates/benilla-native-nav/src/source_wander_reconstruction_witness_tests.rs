// SPDX-License-Identifier: GPL-3.0-or-later
//! Test-only reconstruction argument for the selected immutable dry Wander lineage.
//!
//! CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f: PathFinder.h:141-150,
//! PathFinder.cpp:306-383,506-519,1351-1425. Every operation clears active refs;
//! lookup then reads only the ordered tail at 74, and only appends to it. A normal
//! same-polygon terminal writes slot zero; slots 1..74 stay constructor zeroes.
//! Consequently replaying every append-causing operation in order reproduces the
//! entire tail by induction. Elided operations cannot influence those lookups.
//! The last normal operation restores slot zero. The last non-MinimumDistance
//! operation restores the endpoint, including failed endpoint-height and missing
//! endpoint branches; MinimumDistance alone retains it. The final operation
//! restores active length, path type and points. Keep the ordered union of these
//! writers, not three independent suffixes and not a collection of polygon refs.
//!
//! At most 4096 - 74 operations append, since each appends at least one ref. Thus
//! the witness has at most 4025 operations, regardless of original history length.
//! This does not reproduce omitted before-state observations or owner RNG, time,
//! revision, dispatch/ACK history, or unsupported PathFinder fields/branches. It
//! establishes no production persistence format, restore API or owner authority.
use super::*;

const FILTER: SourceOwnerFilter = SourceOwnerFilter {
    include_flags: 13,
    exclude_flags: 0,
};

const MAX_APPEND_OPERATIONS: usize =
    crate::SOURCE_WARM_CACHE_MAX_STORAGE - crate::SOURCE_WARM_CACHE_POINT_LIMIT;
const MAX_WITNESS_OPERATIONS: usize = MAX_APPEND_OPERATIONS + 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Operation {
    from: [u32; 3],
    raw_end: [u32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OrderedOperation {
    ordinal: usize,
    input: Operation,
}

/// No cache, polygon reference, loaded handle, observation transcript or RNG state.
/// The source slot/config/inventory are fixed; the dry filter is immutable per witness.
#[derive(Debug)]
struct Witness {
    filter: SourceOwnerFilter,
    append_writers: Vec<OrderedOperation>,
    last_normal: Option<OrderedOperation>,
    last_endpoint: Option<OrderedOperation>,
    last: Option<OrderedOperation>,
    operation_count: usize,
}

impl Default for Witness {
    fn default() -> Self {
        Self {
            filter: FILTER,
            append_writers: Vec::new(),
            last_normal: None,
            last_endpoint: None,
            last: None,
            operation_count: 0,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum WitnessError {
    Empty,
    UnsupportedLineage,
    Native(SourceWanderError),
}

impl Witness {
    /// Observe only a genuine typed operation, after all native work succeeded.
    /// Reads validate the induction's domain; only bounded operation inputs persist.
    /// The sequential assignment in `append` is trusted: these coordinate checks
    /// do not authenticate accepted-chain ownership, revisions or ACK history.
    fn record(
        &mut self,
        input: Operation,
        prior: Option<&LoadedSourceWanderTerminal>,
        terminal: &LoadedSourceWanderTerminal,
    ) -> Result<(), WitnessError> {
        let limit = crate::SOURCE_WARM_CACHE_POINT_LIMIT;
        let before = terminal.before_storage();
        let after = terminal.storage();
        if terminal.slot() != SLOT
            || terminal.query_capacity() != 1024
            || terminal.config() != CONFIG
            || terminal.filter() != self.filter
            || terminal.from != input.from
            || terminal.raw_end() != input.raw_end
            || before.len() < limit
            || after.len() < before.len()
            || after.len() > crate::SOURCE_WARM_CACHE_MAX_STORAGE
            || before[1..limit].iter().any(|&reference| reference != 0)
            || after[1..limit].iter().any(|&reference| reference != 0)
            || after[limit..before.len()] != before[limit..]
        {
            return Err(WitnessError::UnsupportedLineage);
        }
        match (self.last, prior) {
            (None, None)
                if before == [0; crate::SOURCE_WARM_CACHE_POINT_LIMIT]
                    && terminal.before_active_len() == 0 => {}
            (Some(last), Some(prior))
                if prior.from == last.input.from
                    && prior.raw_end() == last.input.raw_end
                    && before == prior.storage()
                    && terminal.before_active_len() == prior.active_len()
                    && terminal
                        .query
                        .retain_default()
                        .check_query(&prior.query)
                        .is_ok() => {}
            _ => return Err(WitnessError::UnsupportedLineage),
        }
        let normal = terminal.no_path_reason().is_none();
        if (normal
            && (terminal.path_type() != 1
                || terminal.active_len() != 1
                || terminal.points().len() != 2))
            || (!normal
                && (terminal.path_type() != 8
                    || terminal.active_len() != 0
                    || !terminal.points().is_empty()))
        {
            return Err(WitnessError::UnsupportedLineage);
        }
        let operation = OrderedOperation {
            ordinal: self.operation_count,
            input,
        };
        if after.len() > before.len() {
            if self.append_writers.len() >= MAX_APPEND_OPERATIONS {
                return Err(WitnessError::UnsupportedLineage);
            }
            self.append_writers.push(operation);
        }
        if normal {
            self.last_normal = Some(operation);
        }
        if terminal.no_path_reason() != Some(SourceWanderNoPathReason::MinimumDistance) {
            self.last_endpoint = Some(operation);
        }
        self.last = Some(operation);
        self.operation_count += 1;
        Ok(())
    }

    fn ordered_inputs(&self) -> Result<Vec<OrderedOperation>, WitnessError> {
        if self.last.is_none() {
            return Err(WitnessError::Empty);
        }
        let mut operations = self.append_writers.clone();
        operations.extend(self.last_normal);
        operations.extend(self.last_endpoint);
        operations.extend(self.last);
        operations.sort_by_key(|operation| operation.ordinal);
        operations.dedup_by_key(|operation| operation.ordinal);
        assert!(operations.len() <= MAX_WITNESS_OPERATIONS);
        Ok(operations)
    }
}

fn normal_a() -> Operation {
    Operation {
        from: point(5., 5., 0.),
        raw_end: point(6., 5., 0.25),
    }
}

fn normal_b() -> Operation {
    Operation {
        from: point(5., 15., 0.),
        raw_end: point(6., 15., 0.75),
    }
}

fn height_failure() -> Operation {
    Operation {
        from: point(25., 5., 0.),
        raw_end: point(31., 5., 0.5),
    }
}

fn missing_endpoint() -> Operation {
    Operation {
        from: point(5., 5., 0.),
        raw_end: point(200., 200., 2.),
    }
}

fn minimum_distance() -> Operation {
    Operation {
        from: point(7., 5., 0.),
        raw_end: point(7., 5., 0.),
    }
}

fn execute(
    retained: &mut RetainedSourceQuery,
    prior: Option<&LoadedSourceWanderTerminal>,
    input: Operation,
    filter: SourceOwnerFilter,
) -> Result<LoadedSourceWanderTerminal, SourceWanderError> {
    let mut prefix = retained.prepare_wander_same_polygon(
        prior,
        SOURCE_WANDER_ENDPOINT_COMPATIBILITY_VERSION,
        filter,
        CONFIG,
        // Use the existing per-operation ceilings without changing their policy.
        // Three tail refs plus an endpoint miss already exceed the default three.
        SourceWanderBudget {
            endpoint_lookup_primitives: SOURCE_WANDER_MAX_ENDPOINT_LOOKUP_PRIMITIVES,
            build_lookup_primitives: SOURCE_WANDER_MAX_BUILD_LOOKUP_PRIMITIVES,
            terminal_primitives: 1,
        },
        until(),
    )?;
    assert!(!prefix.current_selected());
    assert_eq!(prefix.cleared_active_len(), 0);
    assert_eq!(prefix.path_type(), 8);
    assert!(prefix.straight_line());
    assert!(!prefix.force_destination());
    assert!(prefix.points().is_empty());
    retained.select_wander_prefix(&mut prefix, until())?;
    let selected = retained.continue_wander_prefix(&prefix, input.from, input.raw_end)?;
    match selected.loaded_prepared_wander_no_path(&prefix, until()) {
        Err(SourceWanderError::NotTooClose) => {
            assert!(selected.tile_presence(until())?.both_present());
            selected.loaded_prepared_wander_same_polygon(&prefix, until())
        }
        result => result,
    }
}

fn append(
    retained: &mut RetainedSourceQuery,
    terminal: &mut Option<LoadedSourceWanderTerminal>,
    witness: &mut Witness,
    input: Operation,
) -> Result<(), WitnessError> {
    let next = execute(retained, terminal.as_ref(), input, witness.filter)
        .map_err(WitnessError::Native)?;
    witness.record(input, terminal.as_ref(), &next)?;
    *terminal = Some(next);
    Ok(())
}

/// Re-executes operation inputs on a fresh native allocation; never installs refs.
fn replay(
    source: &SourceMapQuery,
    inputs: &[OrderedOperation],
    filter: SourceOwnerFilter,
) -> Result<LoadedSourceWanderTerminal, WitnessError> {
    let mut retained = source.retain_default();
    assert!(!retained.current_selected());
    let mut terminal = None;
    for operation in inputs {
        terminal = Some(
            execute(&mut retained, terminal.as_ref(), operation.input, filter)
                .map_err(WitnessError::Native)?,
        );
    }
    terminal.ok_or(WitnessError::Empty)
}

/// Semantic selected-lineage state and final input identity, not replay diagnostics.
#[derive(Debug, PartialEq, Eq)]
struct SemanticState<'a> {
    slot: SourceMapQuerySlot,
    capacity: u32,
    config: SourceSamePolygonConfig,
    filter: SourceOwnerFilter,
    from: [u32; 3],
    raw_end: [u32; 3],
    actual_end: [u32; 3],
    storage: &'a [u64],
    active_len: usize,
    path_type: u32,
    points: &'a [[u32; 3]],
    no_path_reason: Option<SourceWanderNoPathReason>,
}

fn semantic(terminal: &LoadedSourceWanderTerminal) -> SemanticState<'_> {
    SemanticState {
        slot: terminal.slot(),
        capacity: terminal.query_capacity(),
        config: terminal.config(),
        filter: terminal.filter(),
        from: terminal.from,
        raw_end: terminal.raw_end(),
        actual_end: terminal.actual_end(),
        storage: terminal.storage(),
        active_len: terminal.active_len(),
        path_type: terminal.path_type(),
        points: terminal.points(),
        no_path_reason: terminal.no_path_reason(),
    }
}

fn assert_fresh_equivalent(
    original: &LoadedSourceWanderTerminal,
    witness: &Witness,
) -> (SourceMapQuery, LoadedSourceWanderTerminal) {
    let fresh = query();
    assert_eq!(
        fresh.retain_default().check_query(&original.query),
        Err(Error::Identity),
        "fixture bytes and refs cannot substitute for native allocation identity"
    );
    let restored = replay(&fresh, &witness.ordered_inputs().unwrap(), witness.filter).unwrap();
    assert_eq!(semantic(&restored), semantic(original));
    (fresh, restored)
}

#[test]
fn bounded_witness_reconstructs_long_mixed_history_and_every_continuation() {
    assert_eq!(MAX_APPEND_OPERATIONS, 4022);
    assert_eq!(MAX_WITNESS_OPERATIONS, 4025);
    let source = query();
    let mut retained = source.retain_default();
    let mut terminal = None;
    let mut witness = Witness::default();
    for input in [normal_a(), normal_b(), height_failure()] {
        append(&mut retained, &mut terminal, &mut witness, input).unwrap();
    }
    assert_eq!(witness.append_writers.len(), 3);
    assert_eq!(
        terminal.as_ref().unwrap().no_path_reason(),
        Some(SourceWanderNoPathReason::EndpointHeight)
    );
    let mut branch_counts = [0; 4];
    for index in 0..4099 {
        let input = [
            normal_b(),
            minimum_distance(),
            missing_endpoint(),
            height_failure(),
        ][index % 4];
        append(&mut retained, &mut terminal, &mut witness, input).unwrap();
        let branch = match terminal.as_ref().unwrap().no_path_reason() {
            None => 0,
            Some(SourceWanderNoPathReason::MinimumDistance) => 1,
            Some(SourceWanderNoPathReason::EndpointMissing) => 2,
            Some(SourceWanderNoPathReason::EndpointHeight) => 3,
        };
        branch_counts[branch] += 1;
        assert!(witness.ordered_inputs().unwrap().len() <= 6);
    }
    assert!(branch_counts.into_iter().all(|count| count >= 1024));
    for input in [normal_a(), missing_endpoint(), minimum_distance()] {
        append(&mut retained, &mut terminal, &mut witness, input).unwrap();
    }
    assert!(witness.operation_count > 2048);
    assert_eq!(witness.ordered_inputs().unwrap().len(), 6);
    assert_eq!(terminal.as_ref().unwrap().storage().len(), 77);
    let append_count = witness.append_writers.len();
    for _ in 0..2051 {
        append(
            &mut retained,
            &mut terminal,
            &mut witness,
            minimum_distance(),
        )
        .unwrap();
        assert_eq!(witness.append_writers.len(), append_count);
        assert_eq!(witness.ordered_inputs().unwrap().len(), 6);
    }
    let (fresh, mut restored) = assert_fresh_equivalent(terminal.as_ref().unwrap(), &witness);
    let mut original = terminal.unwrap();
    let mut fresh_retained = fresh.retain_default();
    // Retained opaque handles stay tied to their own real allocation throughout.
    for input in [
        minimum_distance(),
        normal_b(),
        missing_endpoint(),
        minimum_distance(),
        height_failure(),
        normal_a(),
    ] {
        original = execute(&mut retained, Some(&original), input, FILTER).unwrap();
        restored = execute(&mut fresh_retained, Some(&restored), input, FILTER).unwrap();
        assert_eq!(semantic(&restored), semantic(&original));
        assert_eq!(restored.before_storage(), original.before_storage());
        assert_eq!(restored.before_active_len(), original.before_active_len());
        assert_eq!(restored.endpoint_trace(), original.endpoint_trace());
        assert_eq!(restored.build_trace(), original.build_trace());
        assert_eq!(restored.height_status(), original.height_status());
    }
}

#[test]
fn bounded_witness_preserves_each_stable_dry_filter() {
    for filter in [SourceOwnerFilter::GROUND, FILTER] {
        let source = query();
        let mut retained = source.retain_default();
        let mut terminal = None;
        let mut witness = Witness {
            filter,
            ..Witness::default()
        };
        for input in [normal_a(), normal_b(), height_failure()] {
            append(&mut retained, &mut terminal, &mut witness, input).unwrap();
        }
        for index in 0..2051 {
            let input = [
                normal_a(),
                minimum_distance(),
                missing_endpoint(),
                height_failure(),
            ][index % 4];
            append(&mut retained, &mut terminal, &mut witness, input).unwrap();
        }
        let original = terminal.unwrap();
        assert_eq!(original.filter(), filter);
        let (fresh, restored) = assert_fresh_equivalent(&original, &witness);
        let original = execute(&mut retained, Some(&original), normal_b(), filter).unwrap();
        let restored = execute(
            &mut fresh.retain_default(),
            Some(&restored),
            normal_b(),
            filter,
        )
        .unwrap();
        assert_eq!(semantic(&restored), semantic(&original));
        assert_eq!(restored.filter(), filter);
    }
}

#[test]
fn bounded_witness_last_non_minimum_restores_each_endpoint_writer() {
    for writer in [normal_a(), missing_endpoint(), height_failure()] {
        let source = query();
        let mut retained = source.retain_default();
        let mut terminal = None;
        let mut witness = Witness::default();
        for _ in 0..2051 {
            append(
                &mut retained,
                &mut terminal,
                &mut witness,
                minimum_distance(),
            )
            .unwrap();
        }
        assert!(witness.last_endpoint.is_none());
        assert!(witness.last_normal.is_none());
        assert!(witness.append_writers.is_empty());
        assert_eq!(witness.ordered_inputs().unwrap().len(), 1);
        assert_eq!(terminal.as_ref().unwrap().actual_end(), [0; 3]);
        assert_fresh_equivalent(terminal.as_ref().unwrap(), &witness);
        append(&mut retained, &mut terminal, &mut witness, writer).unwrap();
        let written_end = terminal.as_ref().unwrap().actual_end();
        assert_ne!(written_end, [0; 3]);
        for _ in 0..2051 {
            append(
                &mut retained,
                &mut terminal,
                &mut witness,
                minimum_distance(),
            )
            .unwrap();
        }
        assert_eq!(terminal.as_ref().unwrap().actual_end(), written_end);
        assert_eq!(witness.ordered_inputs().unwrap().len(), 2);
        assert_fresh_equivalent(terminal.as_ref().unwrap(), &witness);
    }
}

#[test]
fn bounded_witness_omission_and_reordering_are_detected_by_full_state() {
    let source = query();
    let mut retained = source.retain_default();
    let mut terminal = None;
    let mut witness = Witness::default();
    for input in [
        normal_a(),
        normal_b(),
        height_failure(),
        normal_a(),
        missing_endpoint(),
        minimum_distance(),
    ] {
        append(&mut retained, &mut terminal, &mut witness, input).unwrap();
    }
    let original = terminal.unwrap();
    let inputs = witness.ordered_inputs().unwrap();
    assert_eq!(inputs.len(), 6);
    for omitted in 0..inputs.len() {
        let mut mutation = inputs.clone();
        mutation.remove(omitted);
        let fresh = query();
        let changed = replay(&fresh, &mutation, FILTER).unwrap();
        assert_ne!(semantic(&changed), semantic(&original), "omitted {omitted}");
        if omitted < 3 {
            assert_ne!(changed.storage(), original.storage());
        } else if omitted == 3 {
            assert_ne!(changed.storage()[0], original.storage()[0]);
        } else if omitted == 4 {
            assert_ne!(changed.actual_end(), original.actual_end());
        }
    }
    let mut reordered = inputs;
    reordered.swap(0, 1);
    let changed = replay(&query(), &reordered, FILTER).unwrap();
    assert_ne!(&changed.storage()[74..], &original.storage()[74..]);
    assert_ne!(semantic(&changed), semantic(&original));

    // With a final MinimumDistance after NORMAL, dropping that final operation
    // also changes actual path/points/active state, not just terminal evidence.
    let mut normal_then_clear = Witness::default();
    let mut terminal = None;
    for input in [normal_a(), minimum_distance()] {
        append(&mut retained, &mut terminal, &mut normal_then_clear, input).unwrap();
    }
    let clear = terminal.unwrap();
    let normal = replay(
        &query(),
        &normal_then_clear.ordered_inputs().unwrap()[..1],
        FILTER,
    )
    .unwrap();
    assert_eq!(clear.active_len(), 0);
    assert_eq!(normal.active_len(), 1);
    assert_ne!(clear.path_type(), normal.path_type());
    assert_ne!(clear.points(), normal.points());
}

#[test]
fn bounded_witness_semantic_equivalence_does_not_claim_original_before_observations() {
    let source = query();
    let mut retained = source.retain_default();
    let mut terminal = None;
    let mut witness = Witness::default();
    for input in [
        normal_a(),
        normal_b(),
        normal_a(),
        minimum_distance(),
        normal_b(),
    ] {
        append(&mut retained, &mut terminal, &mut witness, input).unwrap();
    }
    assert_eq!(witness.operation_count, 5);
    assert_eq!(witness.ordered_inputs().unwrap().len(), 3);
    let original = terminal.unwrap();
    let (_, restored) = assert_fresh_equivalent(&original, &witness);
    assert_ne!(restored.before_storage(), original.before_storage());
    assert_eq!(restored.before_active_len(), 1);
    assert_eq!(original.before_active_len(), 0);
    assert_eq!(restored.endpoint_trace(), original.endpoint_trace());
    assert_eq!(restored.build_trace(), original.build_trace());
}

#[test]
fn bounded_witness_rejects_unsupported_or_nonconstructor_lineage_without_importing_refs() {
    assert_eq!(
        Witness::default().ordered_inputs(),
        Err(WitnessError::Empty)
    );
    assert!(matches!(
        replay(&query(), &[], FILTER),
        Err(WitnessError::Empty)
    ));
    let source = query();
    let mut retained = source.retain_default();
    let mut terminal = None;
    let mut witness = Witness::default();
    append(&mut retained, &mut terminal, &mut witness, normal_a()).unwrap();
    let saved = witness.ordered_inputs().unwrap();
    let cross_polygon = Operation {
        from: normal_a().from,
        raw_end: normal_b().raw_end,
    };
    assert_eq!(
        append(&mut retained, &mut terminal, &mut witness, cross_polygon),
        Err(WitnessError::Native(
            SourceWanderError::NonSamePolygonUnsupported
        ))
    );
    assert_eq!(witness.ordered_inputs().unwrap(), saved);
    let original = terminal.unwrap();
    let mut fresh = query().retain_default();
    assert!(matches!(
        execute(&mut fresh, Some(&original), normal_a(), FILTER),
        Err(SourceWanderError::Boundary(Error::Identity))
    ));
    let later = execute(&mut retained, Some(&original), normal_b(), FILTER).unwrap();
    assert_eq!(
        Witness::default().record(normal_b(), None, &later),
        Err(WitnessError::UnsupportedLineage),
        "a suffix cannot pretend to originate in constructor zero storage"
    );
    assert_eq!(
        witness.record(missing_endpoint(), Some(&original), &later),
        Err(WitnessError::UnsupportedLineage)
    );
    for filter in [
        SourceOwnerFilter {
            include_flags: 4,
            exclude_flags: 0,
        },
        SourceOwnerFilter::GROUND,
    ] {
        assert!(retained
            .prepare_wander_same_polygon(
                Some(&original),
                1,
                filter,
                CONFIG,
                SourceWanderBudget::default(),
                until()
            )
            .is_err());
    }
    assert!(matches!(
        retained.prepare_wander_same_polygon(
            Some(&original),
            1,
            FILTER,
            SourceSamePolygonConfig {
                normalize_z: true,
                ignore_normalization: false
            },
            SourceWanderBudget::default(),
            until()
        ),
        Err(SourceWanderError::NormalizationConfigurationUnsupported)
    ));
    assert_eq!(witness.ordered_inputs().unwrap(), saved);
    assert_eq!(original.storage().len(), 75);
}
