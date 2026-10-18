// SPDX-License-Identifier: GPL-3.0-or-later
//! Native differential proof for the narrow fixed-cache NORMAL Chase domain.
//! The precision tile is authored geometry, not a cache fixture. Each history below
//! executes real source1024 lookups from the approved cold entry; replay starts on
//! a different live allocation. No app checkpoint or owner revision is synthesized.
use crate::*;
use std::time::{Duration, Instant};

const SLOT: SourceMapQuerySlot = SourceMapQuerySlot {
    map_id: 0,
    instance_id: 0,
};
const CONFIG: SourceSamePolygonConfig = SourceSamePolygonConfig {
    normalize_z: false,
    ignore_normalization: false,
};
const PARAMS: &[u8] = include_bytes!("../tools/fixtures/direct-corridor/params.test-fixture");
const PRECISION: &[u8] = include_bytes!("../tools/fixtures/warm-cache/precision.test-fixture");
const LONG_COUNT: u64 = 4097;

type Input = ([u32; 3], [u32; 3]);

fn point(x: f32, y: f32, z: f32) -> [u32; 3] {
    [x, y, z].map(f32::to_bits)
}
fn a() -> Input {
    (point(5., 5., 0.), point(5., 6., 0.))
}
fn b() -> Input {
    (point(5., 15., 0.), point(5., 16., 0.75))
}
fn b_zero() -> Input {
    (point(5., 15., 0.), point(5., 16., 0.))
}
fn shared_edge() -> Input {
    (point(5., 10., 0.), point(6., 10., 0.))
}
fn active() -> Input {
    (point(5., 6., 0.), point(5., 7., 0.75))
}
fn tail() -> Input {
    (point(5., 5., -2.), point(5., 6., -2.))
}
fn nearest5() -> Input {
    (point(5., 5., -3.), point(5., 6., -3.))
}
fn nearest10() -> Input {
    (point(5., -6., 0.), point(5., 5., 0.))
}
fn filter(include_flags: u16) -> SourceOwnerFilter {
    SourceOwnerFilter {
        include_flags,
        exclude_flags: 0,
    }
}
fn tiles() -> [TileInput<'static>; 1] {
    [TileInput {
        grid: [31, 31],
        bytes: PRECISION,
    }]
}
fn query(slot: SourceMapQuerySlot) -> SourceMapQuery {
    SourceMapQuery::load(slot, PARAMS, &tiles()).unwrap()
}
fn until() -> Instant {
    Instant::now() + Duration::from_secs(30)
}
fn execute(
    owner: &mut RetainedSourceQuery,
    prior: Option<&LoadedSourceSamePolygon>,
    input: Input,
    filter: SourceOwnerFilter,
) -> LoadedSourceSamePolygon {
    let selected = owner.select_for_calculate(SLOT, input.0, input.1).unwrap();
    let terminal = match prior {
        Some(prior) => selected.loaded_retained_same_polygon(
            prior,
            CONFIG,
            SourceSamePolygonBudget {
                lookup_primitives: SOURCE_RETAINED_SAME_POLYGON_MAX_LOOKUP_PRIMITIVES,
                terminal_primitives: 1,
            },
            until(),
        ),
        None => selected.loaded_cold_same_polygon_with_filter(
            SOURCE_COLD_DISTANCE_COMPATIBILITY_VERSION,
            filter,
            CONFIG,
            SourceSamePolygonBudget::default(),
            until(),
        ),
    }
    .unwrap();
    assert_eq!(terminal.check_query(&selected), Ok(()));
    assert_eq!(terminal.compatibility().filter(), filter);
    assert_normal(&terminal);
    terminal
}
fn reconstruct(
    recipe: &SourceChaseFixedCacheRecipe,
    challenge: &SourceChaseFixedCacheChallenge,
) -> Result<ReconstructedSourceChaseFixedCache, SourceChaseFixedCacheError> {
    SourceMapQuery::reconstruct_chase_fixed_cache(
        SLOT,
        PARAMS,
        &tiles(),
        recipe,
        challenge,
        until(),
    )
}
fn assert_normal(terminal: &LoadedSourceSamePolygon) {
    let state = terminal.compatibility();
    assert_eq!(terminal.query_capacity(), 1024);
    assert_eq!(terminal.config(), CONFIG);
    assert_eq!(
        state.compatibility_id(),
        SOURCE_COLD_DISTANCE_COMPATIBILITY_ID
    );
    assert_eq!(
        state.compatibility_version(),
        SOURCE_COLD_DISTANCE_COMPATIBILITY_VERSION
    );
    assert_eq!(
        state.source_revision(),
        SOURCE_COLD_DISTANCE_COMPATIBILITY_REVISION
    );
    assert_eq!(state.initial_storage(), &[0; 74]);
    assert_eq!(state.initial_active_len(), 0);
    assert_eq!(state.point_limit(), 74);
    assert_eq!(state.active_len(), 1);
    assert_eq!(state.phase(), SourceWarmPolyPhase::SamePolygon);
    assert_eq!(state.direct_status(), None);
    assert_eq!(terminal.path_type(), 1);
    assert_eq!(terminal.actual_end(), state.to());
    assert_eq!(terminal.points(), &[state.from(), state.to()]);
    assert_eq!(terminal.terminal_primitives(), 1);
    assert!(state.trace().iter().all(|event| event.endpoint < 2));
    assert!(state.trace().iter().all(|event| matches!(
        event.primitive,
        SourceWarmPrimitive::Active
            | SourceWarmPrimitive::Tail
            | SourceWarmPrimitive::Nearest5
            | SourceWarmPrimitive::Nearest10
    )));
}
/// Compare every exposed semantic field independently of the new projection, so a
/// projection accidentally omitting a field cannot make the differential pass.
fn assert_same_semantics(left: &LoadedSourceSamePolygon, right: &LoadedSourceSamePolygon) {
    let (a, b) = (left.compatibility(), right.compatibility());
    assert_eq!(left.slot(), right.slot());
    assert_eq!(left.query_capacity(), right.query_capacity());
    assert_eq!(left.config(), right.config());
    assert_eq!(left.points(), right.points());
    assert_eq!(left.actual_end(), right.actual_end());
    assert_eq!(left.path_type(), right.path_type());
    assert_eq!(left.terminal_primitives(), right.terminal_primitives());
    assert_eq!(a.compatibility_id(), b.compatibility_id());
    assert_eq!(a.compatibility_version(), b.compatibility_version());
    assert_eq!(a.source_revision(), b.source_revision());
    assert_eq!(a.is_retained(), b.is_retained());
    assert_eq!(a.initial_storage(), b.initial_storage());
    assert_eq!(a.initial_active_len(), b.initial_active_len());
    assert_eq!(a.point_limit(), b.point_limit());
    assert_eq!(a.filter(), b.filter());
    assert_eq!(a.storage(), b.storage());
    assert_eq!(a.active_len(), b.active_len());
    assert_eq!(a.from(), b.from());
    assert_eq!(a.to(), b.to());
    assert_eq!(a.phase(), b.phase());
    assert_eq!(a.direct_status(), b.direct_status());
    assert_eq!(left.chase_semantic_state(), right.chase_semantic_state());
}
fn assert_same_actual_operation(left: &LoadedSourceSamePolygon, right: &LoadedSourceSamePolygon) {
    assert_same_semantics(left, right);
    let (a, b) = (left.compatibility(), right.compatibility());
    assert_eq!(a.before_storage(), b.before_storage());
    assert_eq!(a.before_active_len(), b.before_active_len());
    assert_eq!(a.start(), b.start());
    assert_eq!(a.end(), b.end());
    assert_eq!(a.trace(), b.trace());
}
/// Source-generated observations are comparison outputs, never inputs to native
/// execution. Sequence steps differ after compaction; all native fields must agree.
fn assert_source_observation(label: &str, terminal: &LoadedSourceSamePolygon) {
    let fixture = include_str!("../tools/fixtures/retained-same-poly/comparisons.test-fixture");
    let headings: Vec<_> = fixture
        .lines()
        .find_map(|line| line.strip_prefix("# label\t"))
        .map(|rest| std::iter::once("label").chain(rest.split('\t')).collect())
        .unwrap();
    let values: Vec<_> = fixture
        .lines()
        .find(|line| line.split('\t').next() == Some(label))
        .unwrap_or_else(|| panic!("missing independent source observation: {label}"))
        .split('\t')
        .collect();
    assert_eq!(values.len(), headings.len());
    let get = |key: &str| values[headings.iter().position(|&heading| heading == key).unwrap()];
    let bits = |text: &str| -> [u32; 3] {
        text.split(',')
            .map(|word| u32::from_str_radix(word, 16).unwrap())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap()
    };
    let refs =
        |text: &str| -> Vec<u64> { text.split(',').map(|word| word.parse().unwrap()).collect() };
    let state = terminal.compatibility();
    assert_eq!(get("geometry"), "precision");
    assert_eq!(
        get("oracle"),
        if state.is_retained() {
            "original-retained-defined"
        } else {
            "compatibility-zero-v1"
        }
    );
    assert_eq!(state.from(), bits(get("from")), "{label}");
    assert_eq!(state.to(), bits(get("to")), "{label}");
    assert_eq!(
        u8::from(terminal.config().normalize_z),
        get("normalize_z").parse::<u8>().unwrap()
    );
    assert_eq!(
        u8::from(terminal.config().ignore_normalization),
        get("ignore_normalization").parse::<u8>().unwrap()
    );
    assert_eq!(
        terminal.query_capacity(),
        get("query_nodes").parse::<u32>().unwrap()
    );
    assert_eq!(
        state.filter().include_flags,
        get("include_filter").parse::<u16>().unwrap()
    );
    assert_eq!(
        state.filter().exclude_flags,
        get("exclude_filter").parse::<u16>().unwrap()
    );
    assert_eq!(
        state.point_limit(),
        get("point_limit").parse::<usize>().unwrap()
    );
    assert_eq!(
        state.before_active_len(),
        get("before_active").parse::<usize>().unwrap()
    );
    assert_eq!(
        state.before_storage(),
        refs(get("before_storage")),
        "{label}"
    );
    assert_eq!(
        state.start().polygon,
        get("start_ref").parse::<u64>().unwrap()
    );
    assert_eq!(
        state.start().distance_bits,
        u32::from_str_radix(get("start_distance"), 16).unwrap()
    );
    let mut lookup_storage = state.before_storage().to_vec();
    if state.start().appended {
        lookup_storage.push(state.start().polygon);
    }
    assert_eq!(lookup_storage, refs(get("after_start_storage")), "{label}");
    assert_eq!(state.end().polygon, get("end_ref").parse::<u64>().unwrap());
    assert_eq!(
        state.end().distance_bits,
        u32::from_str_radix(get("end_distance"), 16).unwrap()
    );
    if state.end().appended {
        lookup_storage.push(state.end().polygon);
    }
    assert_eq!(lookup_storage, refs(get("lookup_storage")), "{label}");
    assert_eq!(
        state.active_len(),
        get("after_active").parse::<usize>().unwrap()
    );
    assert_eq!(state.storage(), refs(get("after_storage")), "{label}");
    assert_eq!(
        terminal.path_type(),
        get("path_type").parse::<u32>().unwrap()
    );
    assert_eq!(terminal.actual_end(), bits(get("actual_end")), "{label}");
    let points: Vec<_> = get("points").split(';').map(bits).collect();
    assert_eq!(terminal.points(), points, "{label}");
}

fn two_operation_recipe() -> SourceChaseFixedCacheRecipe {
    let source = query(SLOT);
    let mut owner = source.retain_default();
    let first = execute(&mut owner, None, a(), filter(1));
    let last = execute(&mut owner, Some(&first), active(), filter(1));
    let mut witness = SourceChaseFixedCacheWitness::default();
    witness.record(&first).unwrap();
    witness.record(&last).unwrap();
    witness.recipe().unwrap()
}

#[test]
fn full_4097_operation_chase_replays_two_inputs_on_fresh_query_and_continues_exactly() {
    for flags in [1, 13] {
        let filter = filter(flags);
        let source = query(SLOT);
        assert_eq!(source.query_profile(), QueryProfile::SourceMapInstance1024);
        let mut original_owner = source.retain_default();
        assert!(!original_owner.current_selected());
        let mut history = Vec::new();
        let mut witness = SourceChaseFixedCacheWitness::default();
        let kinds = [
            (active(), SourceWarmLookupOrigin::Active),
            (tail(), SourceWarmLookupOrigin::Tail),
            (nearest5(), SourceWarmLookupOrigin::Nearest5),
            (nearest10(), SourceWarmLookupOrigin::Nearest10),
        ];
        for ordinal in 0..LONG_COUNT {
            let (input, origin) = if ordinal == 0 {
                (a(), SourceWarmLookupOrigin::Nearest5)
            } else if ordinal == LONG_COUNT - 1 {
                kinds[0]
            } else {
                kinds[(ordinal as usize - 1) % kinds.len()]
            };
            let terminal = execute(&mut original_owner, history.last(), input, filter);
            if let Some(first) = history.first() {
                let state = terminal.compatibility();
                assert_eq!(state.before_storage(), first.compatibility().storage());
                assert_eq!(state.storage(), first.compatibility().storage());
                assert_eq!(state.before_active_len(), 1);
                assert_eq!(state.start().origin, origin);
                assert!(!state.start().appended);
                assert!(!state.end().appended);
            }
            let source_suffix = match ordinal {
                0 => Some("cold"),
                1 => Some("active"),
                2 => Some("tail"),
                3 => Some("nearest5"),
                4 => Some("nearest10"),
                value if value == LONG_COUNT - 1 => Some("head"),
                _ => None,
            };
            if let Some(suffix) = source_suffix {
                assert_source_observation(&format!("fixed_full_{flags}_{suffix}"), &terminal);
            }
            witness.record(&terminal).unwrap();
            assert_eq!(witness.operation_count(), ordinal + 1);
            history.push(terminal);
        }
        assert_eq!(history.len() as u64, LONG_COUNT);
        assert!(history.len() > 2048);
        let final_head = history.last().unwrap();
        let recipe = witness.recipe().unwrap();
        assert_eq!(recipe.operation_count, LONG_COUNT);
        assert_eq!(recipe.operations.len(), 2);
        assert_eq!(recipe.operations[0].ordinal, 0);
        assert_eq!(recipe.operations[1].ordinal, LONG_COUNT - 1);
        assert_eq!((recipe.operations[0].from, recipe.operations[0].to), a());
        assert_eq!(recipe.operations[1].from, final_head.compatibility().from());
        assert_eq!(recipe.operations[1].to, final_head.compatibility().to());
        let bytes = recipe.encode().unwrap();
        assert!(bytes.len() <= SOURCE_CHASE_FIXED_CACHE_MAX_BYTES);
        assert_eq!(SourceChaseFixedCacheRecipe::decode(&bytes).unwrap(), recipe);
        let challenge = SourceChaseFixedCacheChallenge::new();
        let foreign_challenge = SourceChaseFixedCacheChallenge::new();
        let mut reached = Vec::new();
        let reconstructed = SourceMapQuery::reconstruct_chase_fixed_cache_observed(
            SLOT,
            PARAMS,
            &tiles(),
            &recipe,
            &challenge,
            until(),
            |stage| {
                match stage {
                    SourceChaseFixedCacheStage::Constructor { retained } => {
                        assert!(!retained.current_selected());
                        assert_eq!(retained.slot(), SLOT);
                        assert_eq!(retained.check_query(&source), Err(Error::Identity));
                        reached.push((0, None));
                    }
                    SourceChaseFixedCacheStage::PreSelection {
                        operation,
                        retained,
                    } => {
                        assert_eq!(retained.current_selected(), operation.ordinal != 0);
                        assert_eq!(retained.slot(), SLOT);
                        assert_eq!(retained.check_query(&source), Err(Error::Identity));
                        assert_eq!(
                            recipe
                                .operations
                                .iter()
                                .find(|saved| saved.ordinal == operation.ordinal),
                            Some(operation)
                        );
                        reached.push((1, Some(operation.ordinal)));
                    }
                    SourceChaseFixedCacheStage::Selected { ordinal, selected } => {
                        assert_eq!(selected.slot(), SLOT);
                        assert_eq!(final_head.check_query(selected), Err(Error::Identity));
                        let operation = recipe
                            .operations
                            .iter()
                            .find(|op| op.ordinal == ordinal)
                            .unwrap();
                        assert_eq!(
                            (selected.from(), selected.to()),
                            (operation.from, operation.to)
                        );
                        reached.push((2, Some(ordinal)));
                    }
                    SourceChaseFixedCacheStage::Ready { ordinal, presence } => {
                        assert!(presence.both_present());
                        reached.push((3, Some(ordinal)));
                    }
                    SourceChaseFixedCacheStage::Terminal { ordinal, terminal } => {
                        assert_normal(terminal);
                        let suffix = if ordinal == 0 { "cold" } else { "head" };
                        assert_source_observation(
                            &format!("fixed_replay_{flags}_{suffix}"),
                            terminal,
                        );
                        reached.push((4, Some(ordinal)));
                    }
                }
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            reached,
            vec![
                (0, None),
                (1, Some(0)),
                (2, Some(0)),
                (3, Some(0)),
                (4, Some(0)),
                (1, Some(LONG_COUNT - 1)),
                (2, Some(LONG_COUNT - 1)),
                (3, Some(LONG_COUNT - 1)),
                (4, Some(LONG_COUNT - 1)),
            ]
        );
        assert!(reconstructed.matches_challenge(&challenge));
        assert!(reconstructed.matches_challenge(&challenge.clone()));
        assert!(!reconstructed.matches_challenge(&foreign_challenge));
        assert!(reconstructed.retained_query().current_selected());
        assert!(original_owner.current_selected());
        assert_eq!(original_owner.check_query(&source), Ok(()));
        assert_eq!(
            reconstructed.retained_query().check_query(&source),
            Err(Error::Identity)
        );
        assert_eq!(reconstructed.witness().operation_count(), LONG_COUNT);
        assert_eq!(reconstructed.witness().recipe().unwrap(), recipe);
        assert_same_actual_operation(final_head, reconstructed.terminal());
        let selected = original_owner
            .select_for_calculate(
                SLOT,
                final_head.compatibility().from(),
                final_head.compatibility().to(),
            )
            .unwrap();
        assert_eq!(reconstructed.check_query(&selected), Err(Error::Identity));
        assert_eq!(final_head.check_query(&selected), Ok(()));
        let (mut replay_owner, mut replay_head, mut resumed) = reconstructed.into_parts();
        let selected = replay_owner
            .select_for_calculate(
                SLOT,
                replay_head.compatibility().from(),
                replay_head.compatibility().to(),
            )
            .unwrap();
        assert_eq!(replay_head.check_query(&selected), Ok(()));
        assert_eq!(final_head.check_query(&selected), Err(Error::Identity));
        // A normal fixed-cache successor resumes the original count, not replay count 2.
        let original_next = execute(&mut original_owner, history.last(), active(), filter);
        let replay_next = execute(&mut replay_owner, Some(&replay_head), active(), filter);
        assert_same_actual_operation(&original_next, &replay_next);
        witness.record(&original_next).unwrap();
        resumed.record(&replay_next).unwrap();
        assert_eq!(resumed.operation_count(), LONG_COUNT + 1);
        assert_eq!(resumed.recipe().unwrap(), witness.recipe().unwrap());
        history.push(original_next);
        replay_head = replay_next;
        // Live gameplay remains exact through later cache mutation, but capture is
        // permanently unavailable for this constructor lineage after the append.
        for (index, input) in [b(), a(), shared_edge()].into_iter().enumerate() {
            let original_next = execute(&mut original_owner, history.last(), input, filter);
            let replay_next = execute(&mut replay_owner, Some(&replay_head), input, filter);
            assert_same_actual_operation(&original_next, &replay_next);
            let suffix = ["append", "return", "edge"][index];
            assert_source_observation(&format!("fixed_full_{flags}_{suffix}"), &original_next);
            assert_source_observation(&format!("fixed_replay_{flags}_{suffix}"), &replay_next);
            let error = if index == 0 {
                SourceChaseFixedCacheError::UnsupportedLineage
            } else {
                SourceChaseFixedCacheError::Disabled
            };
            assert_eq!(witness.record(&original_next), Err(error));
            assert_eq!(resumed.record(&replay_next), Err(error));
            assert!(witness.is_disabled());
            assert!(resumed.is_disabled());
            assert_eq!(resumed.operation_count(), LONG_COUNT + 1);
            assert_eq!(resumed.recipe(), Err(SourceChaseFixedCacheError::Disabled));
            history.push(original_next);
            replay_head = replay_next;
        }
        assert!(original_owner.current_selected());
        assert!(replay_owner.current_selected());
        assert_eq!(source.query_capacity(), 1024);
    }
}

#[test]
fn one_cold_input_and_two_identical_inputs_preserve_distinct_original_ordinals() {
    for flags in [1, 13] {
        let source = query(SLOT);
        let mut owner = source.retain_default();
        let cold = execute(&mut owner, None, a(), filter(flags));
        let mut witness = SourceChaseFixedCacheWitness::default();
        assert_eq!(witness.recipe(), Err(SourceChaseFixedCacheError::Empty));
        witness.record(&cold).unwrap();
        let one = witness.recipe().unwrap();
        assert_eq!(one.operation_count, 1);
        assert_eq!(one.operations.len(), 1);
        assert_eq!(one.operations[0].ordinal, 0);
        let replay = reconstruct(&one, &SourceChaseFixedCacheChallenge::new()).unwrap();
        assert_same_actual_operation(&cold, replay.terminal());
        assert_eq!(replay.witness().operation_count(), 1);
        let same = execute(&mut owner, Some(&cold), a(), filter(flags));
        witness.record(&same).unwrap();
        let two = witness.recipe().unwrap();
        assert_eq!(two.operation_count, 2);
        assert_eq!(two.operations.len(), 2);
        assert_eq!(two.operations[0].ordinal, 0);
        assert_eq!(two.operations[1].ordinal, 1);
        assert_eq!(two.operations[0].from, two.operations[1].from);
        assert_eq!(two.operations[0].to, two.operations[1].to);
        let replay = reconstruct(&two, &SourceChaseFixedCacheChallenge::new()).unwrap();
        assert_same_actual_operation(&same, replay.terminal());
        assert_eq!(replay.witness().operation_count(), 2);
        assert_eq!(replay.witness().recipe().unwrap(), two);
    }
}

#[test]
fn active_polygon_flip_without_append_defeats_naive_appenders_plus_final() {
    // PathFinder.cpp:306-383 reads retained active refs before the ordered tail.
    // A and B share Y=10. The A return is not an appender, yet omitting it makes
    // the final exact-edge operation choose B instead of A on real loaded queries.
    for flags in [1, 13] {
        let filter = filter(flags);
        let source = query(SLOT);
        let mut owner = source.retain_default();
        let first_a = execute(&mut owner, None, a(), filter);
        let then_b = execute(&mut owner, Some(&first_a), b_zero(), filter);
        let return_a = execute(&mut owner, Some(&then_b), a(), filter);
        let full_edge = execute(&mut owner, Some(&return_a), shared_edge(), filter);
        assert!(then_b.compatibility().start().appended);
        assert!(!return_a.compatibility().start().appended);
        assert!(!return_a.compatibility().end().appended);
        assert_eq!(
            return_a.compatibility().storage().len(),
            then_b.compatibility().storage().len()
        );
        assert_eq!(
            &return_a.compatibility().storage()[1..],
            &then_b.compatibility().storage()[1..]
        );
        assert_ne!(
            return_a.compatibility().storage()[0],
            then_b.compatibility().storage()[0]
        );
        let fresh = query(SLOT);
        let mut naive_owner = fresh.retain_default();
        let naive_a = execute(&mut naive_owner, None, a(), filter);
        let naive_b = execute(&mut naive_owner, Some(&naive_a), b_zero(), filter);
        let naive_edge = execute(&mut naive_owner, Some(&naive_b), shared_edge(), filter);
        for (suffix, terminal) in [("full", &first_a), ("dropped", &naive_a)] {
            assert_source_observation(&format!("active_{suffix}_{flags}_cold"), terminal);
        }
        for (suffix, terminal) in [("full", &then_b), ("dropped", &naive_b)] {
            assert_source_observation(&format!("active_{suffix}_{flags}_append"), terminal);
        }
        assert_source_observation(&format!("active_full_{flags}_return"), &return_a);
        assert_source_observation(&format!("active_full_{flags}_edge"), &full_edge);
        assert_source_observation(&format!("active_dropped_{flags}_edge"), &naive_edge);
        assert_eq!(full_edge.points(), naive_edge.points());
        assert_eq!(full_edge.actual_end(), naive_edge.actual_end());
        assert_eq!(full_edge.path_type(), naive_edge.path_type());
        assert_eq!(
            &full_edge.compatibility().storage()[1..],
            &naive_edge.compatibility().storage()[1..]
        );
        assert_eq!(
            full_edge.compatibility().storage()[0],
            first_a.compatibility().storage()[0]
        );
        assert_eq!(
            naive_edge.compatibility().storage()[0],
            then_b.compatibility().storage()[0]
        );
        assert_ne!(
            full_edge.compatibility().storage()[0],
            naive_edge.compatibility().storage()[0]
        );
        assert_ne!(
            full_edge.chase_semantic_state(),
            naive_edge.chase_semantic_state()
        );
        for terminal in [&full_edge, &naive_edge] {
            assert_eq!(
                terminal.compatibility().start().origin,
                SourceWarmLookupOrigin::Active
            );
            assert_eq!(
                terminal.compatibility().end().origin,
                SourceWarmLookupOrigin::Active
            );
            assert_eq!(
                terminal.compatibility().start().distance_bits,
                0_f32.to_bits()
            );
            assert_eq!(
                terminal.compatibility().end().distance_bits,
                0_f32.to_bits()
            );
        }
        assert_ne!(
            full_edge.compatibility().trace(),
            naive_edge.compatibility().trace()
        );
        let mut witness = SourceChaseFixedCacheWitness::default();
        witness.record(&first_a).unwrap();
        assert_eq!(
            witness.record(&then_b),
            Err(SourceChaseFixedCacheError::UnsupportedLineage)
        );
        assert!(witness.is_disabled());
        for terminal in [&return_a, &full_edge, &first_a] {
            assert_eq!(
                witness.record(terminal),
                Err(SourceChaseFixedCacheError::Disabled)
            );
            assert_eq!(witness.recipe(), Err(SourceChaseFixedCacheError::Disabled));
            assert_eq!(witness.operation_count(), 1);
        }
    }
}

#[test]
fn equal_native_siblings_skips_duplicates_foreign_queries_and_stale_order_are_not_lineage() {
    let source = query(SLOT);
    let mut owner = source.retain_default();
    let root = execute(&mut owner, None, a(), filter(1));
    let first = execute(&mut owner, Some(&root), active(), filter(1));
    let sibling = execute(&mut owner, Some(&root), active(), filter(1));
    assert_same_actual_operation(&first, &sibling);
    let sibling_child = execute(&mut owner, Some(&sibling), tail(), filter(1));
    let child = execute(&mut owner, Some(&first), tail(), filter(1));
    assert_same_actual_operation(&child, &sibling_child);
    let skipped = execute(&mut owner, Some(&child), nearest5(), filter(1));
    let beyond_skip = execute(&mut owner, Some(&skipped), nearest10(), filter(1));
    let mut witness = SourceChaseFixedCacheWitness::default();
    witness.record(&root).unwrap();
    let after_root = witness.recipe().unwrap();
    assert_eq!(
        witness.record(&root),
        Err(SourceChaseFixedCacheError::UnsupportedLineage)
    );
    assert_eq!(witness.recipe().unwrap(), after_root);
    assert!(!witness.is_disabled());
    witness.record(&first).unwrap();
    let after_first = witness.recipe().unwrap();
    for wrong in [
        &root,
        &first,
        &sibling,
        &sibling_child,
        &skipped,
        &beyond_skip,
    ] {
        assert_eq!(
            witness.record(wrong),
            Err(SourceChaseFixedCacheError::UnsupportedLineage)
        );
        assert_eq!(witness.recipe().unwrap(), after_first);
        assert_eq!(witness.operation_count(), 2);
        assert!(!witness.is_disabled());
    }
    for slot in [
        SLOT,
        SourceMapQuerySlot {
            instance_id: 77,
            ..SLOT
        },
    ] {
        let foreign = query(slot);
        let mut lease = foreign.retain_default();
        let selected = lease.select_for_calculate(slot, a().0, a().1).unwrap();
        let foreign_cold = selected
            .loaded_cold_same_polygon_with_filter(
                SOURCE_COLD_DISTANCE_COMPATIBILITY_VERSION,
                filter(1),
                CONFIG,
                SourceSamePolygonBudget::default(),
                until(),
            )
            .unwrap();
        assert_eq!(
            witness.record(&foreign_cold),
            Err(SourceChaseFixedCacheError::UnsupportedLineage)
        );
        assert_eq!(witness.recipe().unwrap(), after_first);
        let selected = lease
            .select_for_calculate(slot, active().0, active().1)
            .unwrap();
        assert_eq!(
            selected
                .loaded_retained_same_polygon(
                    &first,
                    CONFIG,
                    SourceSamePolygonBudget::default(),
                    until(),
                )
                .unwrap_err(),
            SourceSamePolygonError::Boundary(Error::Identity)
        );
    }
    // A second real cold result on the same allocation is a different constructor
    // token even though its complete terminal state equals the first cold result.
    let other_cold = execute(&mut owner, None, a(), filter(1));
    assert_same_actual_operation(&root, &other_cold);
    assert_eq!(
        witness.record(&other_cold),
        Err(SourceChaseFixedCacheError::UnsupportedLineage)
    );
    assert_eq!(witness.recipe().unwrap(), after_first);
    witness.record(&child).unwrap();
    witness.record(&skipped).unwrap();
    witness.record(&beyond_skip).unwrap();
    assert_eq!(witness.operation_count(), 5);
    assert_eq!(witness.recipe().unwrap().operations[1].ordinal, 4);
}

#[test]
fn failed_native_proposals_do_not_disable_capture_but_explicit_accepted_unsupported_does() {
    let source = query(SLOT);
    let mut owner = source.retain_default();
    let first = execute(&mut owner, None, a(), filter(13));
    let mut witness = SourceChaseFixedCacheWitness::default();
    witness.record(&first).unwrap();
    let before = witness.recipe().unwrap();
    let storage = first.compatibility().storage().to_vec();
    let selected = owner.select_for_calculate(SLOT, b().0, b().1).unwrap();
    // Three primitives have already appended B privately before the end lookup
    // exhausts this work budget. No actual terminal is available to record.
    assert_eq!(
        selected
            .loaded_retained_same_polygon(
                &first,
                CONFIG,
                SourceSamePolygonBudget {
                    lookup_primitives: 3,
                    terminal_primitives: 1
                },
                until(),
            )
            .unwrap_err(),
        SourceSamePolygonError::Lookup(SourceWarmCacheError::Boundary(Error::Budget))
    );
    assert_eq!(
        selected
            .loaded_retained_same_polygon(
                &first,
                CONFIG,
                SourceSamePolygonBudget::default(),
                Instant::now(),
            )
            .unwrap_err(),
        SourceSamePolygonError::Boundary(Error::Deadline)
    );
    for config in [
        SourceSamePolygonConfig {
            normalize_z: true,
            ignore_normalization: false,
        },
        SourceSamePolygonConfig {
            normalize_z: false,
            ignore_normalization: true,
        },
    ] {
        assert_eq!(
            selected
                .loaded_retained_same_polygon(
                    &first,
                    config,
                    SourceSamePolygonBudget::default(),
                    until(),
                )
                .unwrap_err(),
            SourceSamePolygonError::ConfigurationIdentity
        );
    }
    let selected = owner
        .select_for_calculate(SLOT, point(5., -11., 0.), a().1)
        .unwrap();
    assert_eq!(
        selected
            .loaded_retained_same_polygon(
                &first,
                CONFIG,
                SourceSamePolygonBudget {
                    lookup_primitives: SOURCE_RETAINED_SAME_POLYGON_MAX_LOOKUP_PRIMITIVES,
                    terminal_primitives: 1
                },
                until(),
            )
            .unwrap_err(),
        SourceSamePolygonError::Lookup(SourceWarmCacheError::MissingPolygon)
    );
    let selected = owner.select_for_calculate(SLOT, a().0, b().1).unwrap();
    assert_eq!(
        selected
            .loaded_retained_same_polygon(
                &first,
                CONFIG,
                SourceSamePolygonBudget {
                    lookup_primitives: SOURCE_RETAINED_SAME_POLYGON_MAX_LOOKUP_PRIMITIVES,
                    terminal_primitives: 1
                },
                until(),
            )
            .unwrap_err(),
        SourceSamePolygonError::NonSamePolygonUnsupported
    );
    assert_eq!(first.compatibility().storage(), storage);
    assert_eq!(witness.recipe().unwrap(), before);
    assert_eq!(witness.operation_count(), 1);
    assert!(!witness.is_disabled());
    let next = execute(&mut owner, Some(&first), active(), filter(13));
    witness.record(&next).unwrap();
    assert_eq!(witness.operation_count(), 2);
    witness.disable();
    witness.disable();
    assert!(witness.is_disabled());
    assert_eq!(witness.recipe(), Err(SourceChaseFixedCacheError::Disabled));
    assert_eq!(
        witness.record(&next),
        Err(SourceChaseFixedCacheError::Disabled)
    );
    assert_eq!(
        witness.record(&first),
        Err(SourceChaseFixedCacheError::Disabled)
    );
    assert_eq!(witness.operation_count(), 2);
}

#[test]
fn malformed_recipe_metadata_ordinals_coordinates_and_bytes_fail_before_native_replay() {
    let recipe = two_operation_recipe();
    macro_rules! malformed {
        ($name:literal, $r:ident, $body:block) => {{
            let mut $r = recipe.clone();
            $body($name, $r)
        }};
    }
    let invalid = [
        malformed!("version", r, {
            r.version += 1;
        }),
        malformed!("compatibility", r, {
            r.compatibility_version += 1;
        }),
        malformed!("query capacity", r, {
            r.query_capacity = 512;
        }),
        malformed!("point limit", r, {
            r.point_limit = 73;
        }),
        malformed!("normalize z", r, {
            r.config.normalize_z = true;
        }),
        malformed!("ignore normalization", r, {
            r.config.ignore_normalization = true;
        }),
        malformed!("unknown filter", r, {
            r.filter.include_flags = 3;
        }),
        malformed!("excluded filter", r, {
            r.filter.exclude_flags = 1;
        }),
        malformed!("zero count", r, {
            r.operation_count = 0;
        }),
        malformed!("count mismatch", r, {
            r.operation_count += 1;
        }),
        malformed!("no operations", r, {
            r.operations.clear();
        }),
        malformed!("missing cold", r, {
            r.operations.remove(0);
        }),
        malformed!("missing final", r, {
            r.operations.pop();
        }),
        malformed!("cold ordinal", r, {
            r.operations[0].ordinal = 1;
        }),
        malformed!("duplicate ordinal", r, {
            r.operations[1].ordinal = 0;
        }),
        malformed!("reverse order", r, {
            r.operations.reverse();
        }),
        malformed!("extra operation", r, {
            r.operations.push(r.operations[1]);
        }),
        malformed!("ordinal overflow", r, {
            r.operations[1].ordinal = u64::MAX;
            r.operation_count = u64::MAX;
        }),
        malformed!("nan from", r, {
            r.operations[0].from[0] = f32::NAN.to_bits();
        }),
        malformed!("infinite destination", r, {
            r.operations[1].to[2] = f32::INFINITY.to_bits();
        }),
    ];
    let challenge = SourceChaseFixedCacheChallenge::new();
    for (name, bad) in invalid {
        assert!(bad.validate().is_err(), "{name}");
        assert!(bad.encode().is_err(), "{name}");
        let mut observed = 0;
        assert!(
            SourceMapQuery::reconstruct_chase_fixed_cache_observed(
                SLOT,
                PARAMS,
                &tiles(),
                &bad,
                &challenge,
                until(),
                |_| {
                    observed += 1;
                    Ok(())
                },
            )
            .is_err(),
            "{name}"
        );
        assert_eq!(
            observed, 0,
            "malformed recipe reached native replay: {name}"
        );
    }
    let encoded = recipe.encode().unwrap();
    assert!(encoded.len() <= SOURCE_CHASE_FIXED_CACHE_MAX_BYTES);
    for length in 0..encoded.len() {
        assert!(
            SourceChaseFixedCacheRecipe::decode(&encoded[..length]).is_err(),
            "truncated at {length}"
        );
    }
    // Wire-v1 header is 47 bytes, followed by 32-byte ordinal/input records.
    // Mutate encoded bytes directly so decode cannot rely on encode validation.
    for (label, offset, value) in [
        ("magic", 0, 0),
        ("version", 8, 2),
        ("compatibility", 12, 2),
        ("query capacity", 25, 2),
        ("noncanonical normalize", 32, 2),
        ("noncanonical ignore", 33, 2),
        ("unsupported filter", 34, 3),
        ("excluded filter", 36, 1),
        ("zero original count", 38, 0),
        ("wrong original count", 38, 3),
        ("zero record count", 46, 0),
        ("excess record count", 46, 3),
        ("huge record count", 46, 255),
        ("nonzero cold ordinal", 47, 1),
        ("duplicate final ordinal", 79, 0),
    ] {
        let mut bad = encoded.clone();
        bad[offset] = value;
        assert!(
            SourceChaseFixedCacheRecipe::decode(&bad).is_err(),
            "{label}"
        );
    }
    let mut overlong = encoded.clone();
    overlong.push(0);
    assert!(SourceChaseFixedCacheRecipe::decode(&overlong).is_err());
    assert!(
        SourceChaseFixedCacheRecipe::decode(&[0; SOURCE_CHASE_FIXED_CACHE_MAX_BYTES + 1]).is_err()
    );
    assert_eq!(
        SourceChaseFixedCacheRecipe::decode(&encoded).unwrap(),
        recipe
    );
}

#[test]
fn valid_but_changed_inputs_filter_query_slot_and_foreign_challenge_do_not_match_saved_head() {
    let recipe = two_operation_recipe();
    let challenge = SourceChaseFixedCacheChallenge::new();
    let genuine = reconstruct(&recipe, &challenge).unwrap();
    let saved = genuine.terminal().chase_semantic_state();
    let mut changed_filter = recipe.clone();
    changed_filter.filter = filter(13);
    changed_filter.validate().unwrap();
    let changed = reconstruct(&changed_filter, &challenge).unwrap();
    assert_ne!(saved, changed.terminal().chase_semantic_state());
    assert_eq!(genuine.terminal().points(), changed.terminal().points());
    assert_ne!(
        genuine.terminal().compatibility().filter(),
        changed.terminal().compatibility().filter()
    );
    let mut changed_end = recipe.clone();
    changed_end.operations[1].to = point(5., 8., 0.5);
    changed_end.validate().unwrap();
    let changed = reconstruct(&changed_end, &challenge).unwrap();
    assert_ne!(saved, changed.terminal().chase_semantic_state());
    let mut changes_cache = recipe.clone();
    changes_cache.operations[1].from = b().0;
    changes_cache.operations[1].to = b().1;
    changes_cache.validate().unwrap();
    assert_eq!(
        reconstruct(&changes_cache, &challenge).unwrap_err(),
        SourceChaseFixedCacheError::UnsupportedLineage
    );
    for slot in [
        SourceMapQuerySlot { map_id: 1, ..SLOT },
        SourceMapQuerySlot {
            instance_id: 1,
            ..SLOT
        },
    ] {
        let mut foreign_slot = recipe.clone();
        foreign_slot.slot = slot;
        foreign_slot.validate().unwrap();
        assert!(reconstruct(&foreign_slot, &challenge).is_err());
    }
    let foreign_challenge = SourceChaseFixedCacheChallenge::new();
    assert!(!challenge.same_attempt(&foreign_challenge));
    let foreign = reconstruct(&recipe, &foreign_challenge).unwrap();
    assert_same_actual_operation(genuine.terminal(), foreign.terminal());
    assert!(!foreign.matches_challenge(&challenge));
    assert!(!genuine.matches_challenge(&foreign_challenge));
    let (mut owner, terminal, _) = genuine.into_parts();
    let input = (
        terminal.compatibility().from(),
        terminal.compatibility().to(),
    );
    let selected = owner.select_for_calculate(SLOT, input.0, input.1).unwrap();
    assert_eq!(terminal.check_query(&selected), Ok(()));
    assert_eq!(foreign.check_query(&selected), Err(Error::Identity));
    let selected = owner
        .select_for_calculate(SLOT, input.0, point(5., 9., 0.))
        .unwrap();
    assert_eq!(terminal.check_query(&selected), Err(Error::Identity));
}

#[test]
fn deadline_and_observer_refusal_publish_no_certificate_and_fresh_retry_still_matches() {
    let recipe = two_operation_recipe();
    let challenge = SourceChaseFixedCacheChallenge::new();
    let expected = reconstruct(&recipe, &challenge).unwrap();
    let mut calls = 0;
    assert_eq!(
        SourceMapQuery::reconstruct_chase_fixed_cache_observed(
            SLOT,
            PARAMS,
            &tiles(),
            &recipe,
            &challenge,
            Instant::now(),
            |_| {
                calls += 1;
                Ok(())
            },
        )
        .unwrap_err(),
        SourceChaseFixedCacheError::Native(SourceSamePolygonError::Boundary(Error::Deadline))
    );
    assert_eq!(calls, 0);
    for rejected_stage in 0..1 + 4 * recipe.operations.len() {
        let mut calls = 0;
        assert_eq!(
            SourceMapQuery::reconstruct_chase_fixed_cache_observed(
                SLOT,
                PARAMS,
                &tiles(),
                &recipe,
                &challenge,
                until(),
                |_| {
                    let index = calls;
                    calls += 1;
                    if index == rejected_stage {
                        Err(SourceChaseFixedCacheError::UnsupportedLineage)
                    } else {
                        Ok(())
                    }
                },
            )
            .unwrap_err(),
            SourceChaseFixedCacheError::UnsupportedLineage
        );
        assert_eq!(calls, rejected_stage + 1);
        let retry = reconstruct(&recipe, &challenge).unwrap();
        assert_same_actual_operation(expected.terminal(), retry.terminal());
        assert!(retry.matches_challenge(&challenge));
        assert_eq!(retry.witness().recipe().unwrap(), recipe);
    }
}

#[test]
fn deadline_after_every_chase_observation_stops_before_the_next_stage() {
    let recipe = two_operation_recipe();
    let challenge = SourceChaseFixedCacheChallenge::new();
    let expected = reconstruct(&recipe, &challenge).unwrap();
    for expired_stage in 0..1 + 4 * recipe.operations.len() {
        let deadline = Instant::now() + Duration::from_millis(250);
        let mut calls = 0;
        let result = SourceMapQuery::reconstruct_chase_fixed_cache_observed(
            SLOT,
            PARAMS,
            &tiles(),
            &recipe,
            &challenge,
            deadline,
            |_| {
                let index = calls;
                calls += 1;
                if index == expired_stage {
                    std::thread::sleep(
                        deadline.saturating_duration_since(Instant::now())
                            + Duration::from_millis(1),
                    );
                }
                Ok(())
            },
        );
        assert_eq!(
            result.unwrap_err(),
            SourceChaseFixedCacheError::Native(SourceSamePolygonError::Boundary(Error::Deadline)),
            "expired stage {expired_stage}"
        );
        assert_eq!(
            calls,
            expired_stage + 1,
            "no observation after the expired stage"
        );
        let retry = reconstruct(&recipe, &challenge).unwrap();
        assert_same_actual_operation(expected.terminal(), retry.terminal());
        assert!(retry.matches_challenge(&challenge));
        assert_eq!(retry.witness().recipe().unwrap(), recipe);
    }
}

#[test]
fn initial_and_retained_gate_refusal_precede_selection_and_leave_inert_inputs_unchanged() {
    let recipe = two_operation_recipe();
    let original = recipe.encode().unwrap();
    let challenge = SourceChaseFixedCacheChallenge::new();
    let expected = reconstruct(&recipe, &challenge).unwrap();
    for (rejected_index, rejected) in recipe.operations.iter().enumerate() {
        let mut constructors = 0;
        let mut gates = Vec::new();
        let mut selections = Vec::new();
        let mut ready = Vec::new();
        let mut terminals = Vec::new();
        let result = SourceMapQuery::reconstruct_chase_fixed_cache_observed(
            SLOT,
            PARAMS,
            &tiles(),
            &recipe,
            &challenge,
            until(),
            |stage| {
                match stage {
                    SourceChaseFixedCacheStage::Constructor { retained } => {
                        constructors += 1;
                        assert!(!retained.current_selected());
                    }
                    SourceChaseFixedCacheStage::PreSelection {
                        operation,
                        retained,
                    } => {
                        assert_eq!(constructors, 1);
                        assert_eq!(operation, &recipe.operations[gates.len()]);
                        assert_eq!(retained.current_selected(), !terminals.is_empty());
                        assert_eq!(retained.slot(), recipe.slot);
                        gates.push(operation.ordinal);
                        if operation.ordinal == rejected.ordinal {
                            return Err(SourceChaseFixedCacheError::UnsupportedLineage);
                        }
                    }
                    SourceChaseFixedCacheStage::Selected { ordinal, selected } => {
                        assert_eq!(gates.last(), Some(&ordinal));
                        let operation = recipe.operations[selections.len()];
                        assert_eq!(
                            (selected.from(), selected.to()),
                            (operation.from, operation.to)
                        );
                        selections.push(ordinal);
                    }
                    SourceChaseFixedCacheStage::Ready { ordinal, .. } => ready.push(ordinal),
                    SourceChaseFixedCacheStage::Terminal { ordinal, .. } => terminals.push(ordinal),
                }
                Ok(())
            },
        );
        assert_eq!(
            result.unwrap_err(),
            SourceChaseFixedCacheError::UnsupportedLineage
        );
        let completed: Vec<_> = recipe.operations[..rejected_index]
            .iter()
            .map(|op| op.ordinal)
            .collect();
        assert_eq!(selections, completed);
        assert_eq!(ready, completed);
        assert_eq!(terminals, completed);
        assert_eq!(gates.len(), rejected_index + 1);
        assert_eq!(gates.last(), Some(&rejected.ordinal));
        assert_eq!(recipe.encode().unwrap(), original);
        let retry = reconstruct(&recipe, &challenge).unwrap();
        assert_same_actual_operation(expected.terminal(), retry.terminal());
        assert_eq!(retry.witness().recipe().unwrap(), recipe);
    }
}

#[test]
fn pinned_source_places_initial_constructor_then_each_straight_gate_before_calculate_selection() {
    fn assert_order(source: &str, steps: &[&str]) {
        let mut remaining = source;
        for step in steps {
            let position = remaining
                .find(step)
                .unwrap_or_else(|| panic!("missing source step {step}"));
            remaining = &remaining[position + step.len()..];
        }
    }
    // Existing guarded original-source fragments, independently of replay code.
    let dispatch = String::from_utf8_lossy(include_bytes!(
        "../tools/straight_path_source_fragments.test-fixture"
    ));
    assert_order(
        &dispatch,
        &[
            "bool ChaseMovementGenerator::DispatchSplineToPosition(",
            "owner.UpdateSplinePosition();",
            "if (!this->i_path)\n        this->i_path = new PathFinder(&owner);\n\n    bool gen = false;",
            "if (owner.IsWithinDist3d(x, y, z, 200.f)",
            "std::abs(owner.GetPositionZ() - z) < 5.f",
            "owner.IsWithinLOS(x, y, z + i_target->GetCollisionHeight())",
            "!owner.IsInWater() && !i_target->IsInWater()",
            "this->i_path->calculate(x, y, z, false, true);",
        ],
    );
    let calculate = String::from_utf8_lossy(include_bytes!(
        "../tools/tile_presence_source_fragments.test-fixture"
    ));
    assert_order(
        &calculate,
        &[
            "bool PathFinder::calculate(Vector3 const& start, Vector3 const& dest,",
            "IsValidMapCoord(dest.x, dest.y, dest.z)",
            "IsValidMapCoord(start.x, start.y, start.z)",
            "setStartPosition(start);",
            "setEndPosition(dest);",
            "m_straightLine = straightLine;",
            "SetCurrentNavMesh();",
            "hasUnitState(UNIT_STAT_IGNORE_PATHFINDING)",
            "!HaveTile(start) || !HaveTile(dest)",
            "updateFilter();",
            "BuildPolyPath(start, dest);",
        ],
    );
}

#[test]
fn original_count_overflow_rejects_atomically_without_disabling_or_resetting_lineage() {
    // Inert recipes assert an original count; they cannot prove owner acceptance.
    // Exercise checked continuation arithmetic without executing u64::MAX inputs.
    let mut recipe = two_operation_recipe();
    recipe.operation_count = u64::MAX;
    recipe.operations[1].ordinal = u64::MAX - 1;
    recipe.validate().unwrap();
    let bytes = recipe.encode().unwrap();
    assert_eq!(SourceChaseFixedCacheRecipe::decode(&bytes).unwrap(), recipe);
    let reconstructed = reconstruct(&recipe, &SourceChaseFixedCacheChallenge::new()).unwrap();
    assert_eq!(reconstructed.witness().operation_count(), u64::MAX);
    let (mut owner, prior, mut witness) = reconstructed.into_parts();
    let next = execute(&mut owner, Some(&prior), tail(), filter(1));
    for _ in 0..2 {
        assert_eq!(
            witness.record(&next),
            Err(SourceChaseFixedCacheError::UnsupportedLineage)
        );
        assert!(!witness.is_disabled());
        assert_eq!(witness.operation_count(), u64::MAX);
        assert_eq!(witness.recipe().unwrap(), recipe);
    }
}
