// SPDX-License-Identifier: GPL-3.0-or-later
//! Same-fresh-allocation composition of the independently proved native lineages.
//! Authored precision geometry and genuine operations only. This proves neither
//! world-owner acceptance/revisions nor RNG, timers, dispatch, ACKs or app restore.
use crate::*;
use std::{
    cell::RefCell,
    time::{Duration, Instant},
};

const SLOT: SourceMapQuerySlot = SourceMapQuerySlot {
    map_id: 0,
    instance_id: 0,
};
const CONFIG: SourceSamePolygonConfig = SourceSamePolygonConfig {
    normalize_z: false,
    ignore_normalization: false,
};
const WANDER_FILTER: SourceOwnerFilter = SourceOwnerFilter {
    include_flags: 13,
    exclude_flags: 0,
};
const CHASE_FILTER: SourceOwnerFilter = SourceOwnerFilter::GROUND;
const PARAMS: &[u8] = include_bytes!("../tools/fixtures/direct-corridor/params.test-fixture");
const PRECISION: &[u8] = include_bytes!("../tools/fixtures/warm-cache/precision.test-fixture");
type Input = ([u32; 3], [u32; 3]);

fn point(x: f32, y: f32, z: f32) -> [u32; 3] {
    [x, y, z].map(f32::to_bits)
}
fn normal_a() -> Input {
    (point(5., 5., 0.), point(6., 5., 0.25))
}
fn normal_b() -> Input {
    (point(5., 15., 0.), point(6., 15., 0.75))
}
fn height_failure() -> Input {
    (point(25., 5., 0.), point(31., 5., 0.5))
}
fn missing_endpoint() -> Input {
    (point(5., 5., 0.), point(200., 200., 2.))
}
fn minimum_distance() -> Input {
    (point(7., 5., 0.), point(7., 5., 0.))
}
fn chase_cold() -> Input {
    (point(5., 5., 0.), point(5., 6., 0.))
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
fn tiles() -> [TileInput<'static>; 1] {
    [TileInput {
        grid: [31, 31],
        bytes: PRECISION,
    }]
}
fn query() -> SourceMapQuery {
    SourceMapQuery::load(SLOT, PARAMS, &tiles()).unwrap()
}
fn until() -> Instant {
    Instant::now() + Duration::from_secs(30)
}

fn wander_execute(
    owner: &mut RetainedSourceQuery,
    prior: Option<&LoadedSourceWanderTerminal>,
    input: Input,
) -> LoadedSourceWanderTerminal {
    let mut prefix = owner
        .prepare_wander_same_polygon(
            prior,
            SOURCE_WANDER_ENDPOINT_COMPATIBILITY_VERSION,
            WANDER_FILTER,
            CONFIG,
            SourceWanderBudget {
                endpoint_lookup_primitives: SOURCE_WANDER_MAX_ENDPOINT_LOOKUP_PRIMITIVES,
                build_lookup_primitives: SOURCE_WANDER_MAX_BUILD_LOOKUP_PRIMITIVES,
                terminal_primitives: 1,
            },
            until(),
        )
        .unwrap();
    owner.select_wander_prefix(&mut prefix, until()).unwrap();
    let selected = owner
        .continue_wander_prefix(&prefix, input.0, input.1)
        .unwrap();
    let terminal = match selected.loaded_prepared_wander_no_path(&prefix, until()) {
        Err(SourceWanderError::NotTooClose) => {
            assert!(selected.tile_presence(until()).unwrap().both_present());
            selected.loaded_prepared_wander_same_polygon(&prefix, until())
        }
        result => result,
    }
    .unwrap();
    assert_eq!(terminal.check_query(&selected), Ok(()));
    terminal
}

fn chase_execute(
    owner: &mut RetainedSourceQuery,
    prior: Option<&LoadedSourceSamePolygon>,
    input: Input,
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
            CHASE_FILTER,
            CONFIG,
            SourceSamePolygonBudget::default(),
            until(),
        ),
    }
    .unwrap();
    assert_eq!(terminal.check_query(&selected), Ok(()));
    assert_eq!(terminal.path_type(), 1);
    terminal
}

struct Original {
    query: SourceMapQuery,
    wander_owner: RetainedSourceQuery,
    wander: LoadedSourceWanderTerminal,
    wander_witness: SourceWanderWitness,
    chase_owner: RetainedSourceQuery,
    chase: LoadedSourceSamePolygon,
    chase_witness: SourceChaseFixedCacheWitness,
}
fn original(long: bool) -> Original {
    let query = query();
    let mut wander_owner = query.retain_default();
    let mut chase_owner = query.retain_default();
    assert!(!wander_owner.current_selected());
    assert!(!chase_owner.current_selected());
    let mut wander = None;
    let mut wander_witness = SourceWanderWitness::default();
    for input in [normal_a(), normal_b(), height_failure()]
        .into_iter()
        .chain((0..if long { 2051 } else { 0 }).map(|index| {
            [
                normal_a(),
                minimum_distance(),
                missing_endpoint(),
                height_failure(),
            ][index % 4]
        }))
        .chain([normal_a(), missing_endpoint(), minimum_distance()])
    {
        let terminal = wander_execute(&mut wander_owner, wander.as_ref(), input);
        wander_witness.record(&terminal).unwrap();
        wander = Some(terminal);
    }
    assert!(
        !chase_owner.current_selected(),
        "another lease's selection cannot select this constructor"
    );
    let mut chase = None;
    let mut chase_witness = SourceChaseFixedCacheWitness::default();
    for input in std::iter::once(chase_cold())
        .chain(
            (0..if long { 2052 } else { 0 })
                .map(|index| [active(), tail(), nearest5(), nearest10()][index % 4]),
        )
        .chain([active()])
    {
        let terminal = chase_execute(&mut chase_owner, chase.as_ref(), input);
        chase_witness.record(&terminal).unwrap();
        chase = Some(terminal);
    }
    Original {
        query,
        wander_owner,
        wander: wander.unwrap(),
        wander_witness,
        chase_owner,
        chase: chase.unwrap(),
        chase_witness,
    }
}

// Compare the exposed native fields as well as the projection, so an omitted
// projection field cannot silently weaken the full-state differential.
fn assert_wander_semantics(left: &LoadedSourceWanderTerminal, right: &LoadedSourceWanderTerminal) {
    assert_eq!(left.slot(), right.slot());
    assert_eq!(left.query_capacity(), right.query_capacity());
    assert_eq!(left.config(), right.config());
    assert_eq!(left.filter(), right.filter());
    assert_eq!(left.from(), right.from());
    assert_eq!(left.raw_end(), right.raw_end());
    assert_eq!(left.actual_end(), right.actual_end());
    assert_eq!(left.storage(), right.storage());
    assert_eq!(left.active_len(), right.active_len());
    assert_eq!(left.path_type(), right.path_type());
    assert_eq!(left.points(), right.points());
    assert_eq!(left.no_path_reason(), right.no_path_reason());
    assert_eq!(left.tail_required(), right.tail_required());
    assert_eq!(left.semantic_state(), right.semantic_state());
}
fn assert_wander_operation(left: &LoadedSourceWanderTerminal, right: &LoadedSourceWanderTerminal) {
    assert_wander_semantics(left, right);
    assert_eq!(left.before_storage(), right.before_storage());
    assert_eq!(left.before_active_len(), right.before_active_len());
    assert_eq!(left.endpoint_lookup(), right.endpoint_lookup());
    assert_eq!(left.endpoint_trace(), right.endpoint_trace());
    assert_eq!(left.height_status(), right.height_status());
    assert_eq!(left.height_before(), right.height_before());
    assert_eq!(left.height_after(), right.height_after());
    assert_eq!(left.start_lookup(), right.start_lookup());
    assert_eq!(left.end_lookup(), right.end_lookup());
    assert_eq!(left.build_trace(), right.build_trace());
}
fn assert_chase_operation(left: &LoadedSourceSamePolygon, right: &LoadedSourceSamePolygon) {
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
    assert_eq!(a.before_storage(), b.before_storage());
    assert_eq!(a.before_active_len(), b.before_active_len());
    assert_eq!(a.start(), b.start());
    assert_eq!(a.end(), b.end());
    assert_eq!(a.trace(), b.trace());
    assert_eq!(left.chase_semantic_state(), right.chase_semantic_state());
}

fn reconstruct(
    original: &Original,
    wander_challenge: &SourceWanderReconstructionChallenge,
    chase_challenge: &SourceChaseFixedCacheChallenge,
) -> ReconstructedSourceWanderAndChase {
    SourceMapQuery::reconstruct_wander_and_chase(
        SLOT,
        PARAMS,
        &tiles(),
        &original.wander_witness.recipe().unwrap(),
        wander_challenge,
        &original.chase_witness.recipe().unwrap(),
        chase_challenge,
        until(),
    )
    .unwrap()
}

fn assert_pair(original: &Original, pair: &ReconstructedSourceWanderAndChase) {
    assert_eq!(pair.check_query(pair.query()), Ok(()));
    assert_eq!(pair.check_query(&original.query), Err(Error::Identity));
    assert_eq!(
        pair.wander().retained_query().check_query(pair.query()),
        Ok(())
    );
    assert_eq!(
        pair.chase().retained_query().check_query(pair.query()),
        Ok(())
    );
    assert_eq!(original.wander_owner.check_query(&original.query), Ok(()));
    assert_eq!(original.chase_owner.check_query(&original.query), Ok(()));
    assert_wander_semantics(&original.wander, pair.wander().terminal());
    assert_chase_operation(&original.chase, pair.chase().terminal());
    assert_eq!(
        pair.wander().witness().recipe().unwrap(),
        original.wander_witness.recipe().unwrap()
    );
    assert_eq!(
        pair.chase().witness().recipe().unwrap(),
        original.chase_witness.recipe().unwrap()
    );
    let mut retained = pair.query().retain_default();
    let selected = retained
        .select_for_wander(SLOT, original.wander.from(), original.wander.raw_end())
        .unwrap();
    assert_eq!(pair.wander().check_query(&selected), Ok(()));
    assert_eq!(original.wander.check_query(&selected), Err(Error::Identity));
    let selected = retained
        .select_for_calculate(
            SLOT,
            original.chase.compatibility().from(),
            original.chase.compatibility().to(),
        )
        .unwrap();
    assert_eq!(pair.chase().check_query(&selected), Ok(()));
    assert_eq!(original.chase.check_query(&selected), Err(Error::Identity));
}

#[test]
fn dual_replay_shares_one_fresh_query_with_independent_constructors_and_exact_continuation() {
    let mut original = original(true);
    let wander_recipe = original.wander_witness.recipe().unwrap();
    let chase_recipe = original.chase_witness.recipe().unwrap();
    assert!(wander_recipe.operation_count > 2048);
    assert!(chase_recipe.operation_count > 2048);
    assert_eq!(wander_recipe.operations.len(), 6);
    assert_eq!(chase_recipe.operations.len(), 2);
    let wander_challenge = SourceWanderReconstructionChallenge::new();
    let chase_challenge = SourceChaseFixedCacheChallenge::new();
    let observed_query = RefCell::new(None::<SourceMapQuery>);
    let events = RefCell::new(Vec::new());
    let pair = SourceMapQuery::reconstruct_wander_and_chase_observed(
        SLOT,
        PARAMS,
        &tiles(),
        &wander_recipe,
        &wander_challenge,
        &chase_recipe,
        &chase_challenge,
        until(),
        |stage| {
            let (kind, ordinal) = match stage {
                SourceWanderReconstructionStage::Constructor => (0, None),
                SourceWanderReconstructionStage::Prepared { ordinal, prefix } => {
                    assert!(!prefix.current_selected());
                    assert_eq!(prefix.cleared_active_len(), 0);
                    assert_eq!(prefix.path_type(), 8);
                    assert!(prefix.straight_line());
                    assert!(!prefix.force_destination());
                    assert!(prefix.points().is_empty());
                    if ordinal == 0 {
                        assert_eq!(prefix.before_storage(), &[0; SOURCE_WARM_CACHE_POINT_LIMIT]);
                        assert_eq!(prefix.before_active_len(), 0);
                        *observed_query.borrow_mut() =
                            Some(prefix.query.retain_default().retain_allocation());
                    }
                    assert_eq!(
                        prefix
                            .query
                            .retain_default()
                            .check_query(observed_query.borrow().as_ref().unwrap()),
                        Ok(())
                    );
                    assert_eq!(
                        prefix.query.retain_default().check_query(&original.query),
                        Err(Error::Identity)
                    );
                    (1, Some(ordinal))
                }
                SourceWanderReconstructionStage::Selected { ordinal } => (2, Some(ordinal)),
                SourceWanderReconstructionStage::Terminal { ordinal, terminal } => {
                    assert_eq!(terminal.filter(), WANDER_FILTER);
                    (3, Some(ordinal))
                }
            };
            events.borrow_mut().push((0, kind, ordinal));
            Ok(())
        },
        |stage| {
            let (kind, ordinal) = match stage {
                SourceChaseFixedCacheStage::Constructor { retained } => {
                    assert!(!retained.current_selected());
                    assert_eq!(
                        retained.check_query(observed_query.borrow().as_ref().unwrap()),
                        Ok(())
                    );
                    assert_eq!(retained.check_query(&original.query), Err(Error::Identity));
                    (0, None)
                }
                SourceChaseFixedCacheStage::Selected { ordinal, selected } => {
                    assert_eq!(
                        selected.check_allocation(observed_query.borrow().as_ref().unwrap()),
                        Ok(())
                    );
                    (1, Some(ordinal))
                }
                SourceChaseFixedCacheStage::Ready { ordinal, presence } => {
                    assert!(presence.both_present());
                    (2, Some(ordinal))
                }
                SourceChaseFixedCacheStage::Terminal { ordinal, terminal } => {
                    let state = terminal.compatibility();
                    assert_eq!(state.filter(), CHASE_FILTER);
                    assert_eq!(state.initial_storage(), &[0; SOURCE_WARM_CACHE_POINT_LIMIT]);
                    if ordinal == 0 {
                        assert_eq!(state.before_storage(), &[0; SOURCE_WARM_CACHE_POINT_LIMIT]);
                        assert_eq!(state.before_active_len(), 0);
                    }
                    assert_eq!(terminal.path_type(), 1);
                    (3, Some(ordinal))
                }
            };
            events.borrow_mut().push((1, kind, ordinal));
            Ok(())
        },
    )
    .unwrap();
    let expected: Vec<_> =
        std::iter::once((0, 0, None))
            .chain(wander_recipe.operations.iter().flat_map(|operation| {
                (1..=3).map(move |stage| (0, stage, Some(operation.ordinal)))
            }))
            .chain(std::iter::once((1, 0, None)))
            .chain(chase_recipe.operations.iter().flat_map(|operation| {
                (1..=3).map(move |stage| (1, stage, Some(operation.ordinal)))
            }))
            .collect();
    assert_eq!(*events.borrow(), expected);
    assert_eq!(
        pair.check_query(observed_query.borrow().as_ref().unwrap()),
        Ok(())
    );
    assert!(pair.matches_challenges(&wander_challenge, &chase_challenge));
    assert_pair(&original, &pair);
    assert_eq!(pair.wander().terminal().storage().len(), 77);
    assert_eq!(pair.wander().terminal().active_len(), 0);
    assert_eq!(pair.chase().terminal().compatibility().storage().len(), 75);
    assert_eq!(pair.chase().terminal().compatibility().active_len(), 1);

    let (fresh, wander, chase) = pair.into_parts();
    let (mut wander_owner, mut wander, mut wander_witness) = wander.into_parts();
    let (mut chase_owner, mut chase, mut chase_witness) = chase.into_parts();
    for (wander_input, chase_input) in [
        minimum_distance(),
        normal_b(),
        missing_endpoint(),
        height_failure(),
        normal_a(),
        normal_b(),
    ]
    .into_iter()
    .zip([active(), tail(), nearest5(), nearest10(), tail(), active()])
    {
        let chase_before = chase.chase_semantic_state();
        let chase_recipe_before = chase_witness.recipe().unwrap();
        original.wander = wander_execute(
            &mut original.wander_owner,
            Some(&original.wander),
            wander_input,
        );
        let next = wander_execute(&mut wander_owner, Some(&wander), wander_input);
        assert_wander_operation(&original.wander, &next);
        original.wander_witness.record(&original.wander).unwrap();
        wander_witness.record(&next).unwrap();
        wander = next;
        assert_eq!(chase.chase_semantic_state(), chase_before);
        assert_eq!(chase_witness.recipe().unwrap(), chase_recipe_before);

        let wander_before = wander.semantic_state();
        let wander_recipe_before = wander_witness.recipe().unwrap();
        original.chase = chase_execute(
            &mut original.chase_owner,
            Some(&original.chase),
            chase_input,
        );
        let next = chase_execute(&mut chase_owner, Some(&chase), chase_input);
        assert_chase_operation(&original.chase, &next);
        original.chase_witness.record(&original.chase).unwrap();
        chase_witness.record(&next).unwrap();
        chase = next;
        assert_eq!(wander.semantic_state(), wander_before);
        assert_eq!(wander_witness.recipe().unwrap(), wander_recipe_before);
        assert_eq!(
            wander_witness.recipe().unwrap(),
            original.wander_witness.recipe().unwrap()
        );
        assert_eq!(
            chase_witness.recipe().unwrap(),
            original.chase_witness.recipe().unwrap()
        );
        assert_eq!(wander_owner.check_query(&fresh), Ok(()));
        assert_eq!(chase_owner.check_query(&fresh), Ok(()));
        assert_eq!(original.wander_owner.check_query(&original.query), Ok(()));
        assert_eq!(original.chase_owner.check_query(&original.query), Ok(()));
    }
}

#[test]
fn dual_certificates_reject_old_equal_byte_foreign_queries_and_wrong_typed_challenges() {
    let original = original(false);
    let wander_challenge = SourceWanderReconstructionChallenge::new();
    let chase_challenge = SourceChaseFixedCacheChallenge::new();
    let pair = reconstruct(&original, &wander_challenge, &chase_challenge);
    assert_pair(&original, &pair);
    let equal_byte_foreign = query();
    for foreign in [&original.query, &equal_byte_foreign] {
        assert_eq!(foreign.slot(), pair.query().slot());
        assert_eq!(foreign.query_capacity(), pair.query().query_capacity());
        assert_eq!(pair.check_query(foreign), Err(Error::Identity));
        assert_eq!(
            pair.wander().retained_query().check_query(foreign),
            Err(Error::Identity)
        );
        assert_eq!(
            pair.chase().retained_query().check_query(foreign),
            Err(Error::Identity)
        );
        let mut lease = foreign.retain_default();
        let selected = lease
            .select_for_wander(SLOT, original.wander.from(), original.wander.raw_end())
            .unwrap();
        assert_eq!(pair.wander().check_query(&selected), Err(Error::Identity));
        let selected = lease
            .select_for_calculate(
                SLOT,
                original.chase.compatibility().from(),
                original.chase.compatibility().to(),
            )
            .unwrap();
        assert_eq!(pair.chase().check_query(&selected), Err(Error::Identity));
    }
    let wrong_wander = SourceWanderReconstructionChallenge::new();
    let wrong_chase = SourceChaseFixedCacheChallenge::new();
    assert!(pair.matches_challenges(&wander_challenge.clone(), &chase_challenge.clone()));
    assert!(!pair.matches_challenges(&wrong_wander, &chase_challenge));
    assert!(!pair.matches_challenges(&wander_challenge, &wrong_chase));
    assert!(!pair.matches_challenges(&wrong_wander, &wrong_chase));
    let foreign = reconstruct(&original, &wrong_wander, &wrong_chase);
    assert_pair(&original, &foreign);
    assert!(!foreign.matches_challenges(&wander_challenge, &chase_challenge));
    assert!(!foreign.wander().matches_challenge(&wander_challenge));
    assert!(!foreign.chase().matches_challenge(&chase_challenge));
    assert_eq!(pair.check_query(foreign.query()), Err(Error::Identity));

    // Even genuine equal state on the exact shared query cannot satisfy another
    // typed attempt. This is local attempt binding, not creature-owner authority.
    let same_query_wrong_wander = pair
        .query()
        .reconstruct_wander(
            &original.wander_witness.recipe().unwrap(),
            &wrong_wander,
            until(),
        )
        .unwrap();
    assert_wander_semantics(pair.wander().terminal(), same_query_wrong_wander.terminal());
    let mut lease = pair.query().retain_default();
    let selected = lease
        .select_for_wander(SLOT, original.wander.from(), original.wander.raw_end())
        .unwrap();
    assert_eq!(same_query_wrong_wander.check_query(&selected), Ok(()));
    assert!(!same_query_wrong_wander.matches_challenge(&wander_challenge));
    let selected = lease
        .select_for_wander(SLOT, original.wander.from(), normal_a().1)
        .unwrap();
    assert_eq!(pair.wander().check_query(&selected), Err(Error::Identity));
    let selected = lease
        .select_for_calculate(
            SLOT,
            original.chase.compatibility().from(),
            point(5., 8., 0.),
        )
        .unwrap();
    assert_eq!(pair.chase().check_query(&selected), Err(Error::Identity));
    assert_pair(&original, &pair);
}

#[test]
fn every_observer_rejection_cancels_the_pair_without_partial_output_and_retry_is_exact() {
    let original = original(false);
    let wander_recipe = original.wander_witness.recipe().unwrap();
    let chase_recipe = original.chase_witness.recipe().unwrap();
    let wander_challenge = SourceWanderReconstructionChallenge::new();
    let chase_challenge = SourceChaseFixedCacheChallenge::new();
    let expected = reconstruct(&original, &wander_challenge, &chase_challenge);
    let wander_stages = 1 + 3 * wander_recipe.operations.len();
    let total_stages = wander_stages + 1 + 3 * chase_recipe.operations.len();
    for rejected in 0..total_stages {
        let calls = RefCell::new(0);
        let reached = || {
            let index = *calls.borrow();
            *calls.borrow_mut() += 1;
            index != rejected
        };
        let result = SourceMapQuery::reconstruct_wander_and_chase_observed(
            SLOT,
            PARAMS,
            &tiles(),
            &wander_recipe,
            &wander_challenge,
            &chase_recipe,
            &chase_challenge,
            until(),
            |_| {
                if reached() {
                    Ok(())
                } else {
                    Err(SourceWanderReconstructionError::UnsupportedLineage)
                }
            },
            |_| {
                if reached() {
                    Ok(())
                } else {
                    Err(SourceChaseFixedCacheError::UnsupportedLineage)
                }
            },
        );
        assert_eq!(
            result.unwrap_err(),
            if rejected < wander_stages {
                SourceWanderAndChaseReconstructionError::Wander(
                    SourceWanderReconstructionError::UnsupportedLineage,
                )
            } else {
                SourceWanderAndChaseReconstructionError::Chase(
                    SourceChaseFixedCacheError::UnsupportedLineage,
                )
            },
            "rejected stage {rejected}"
        );
        assert_eq!(*calls.borrow(), rejected + 1);
        assert_pair(&original, &expected);
        let retry = reconstruct(&original, &wander_challenge, &chase_challenge);
        assert_pair(&original, &retry);
        assert!(retry.matches_challenges(&wander_challenge, &chase_challenge));
        assert_eq!(retry.check_query(expected.query()), Err(Error::Identity));
        assert_eq!(original.wander_witness.recipe().unwrap(), wander_recipe);
        assert_eq!(original.chase_witness.recipe().unwrap(), chase_recipe);
    }
}

fn is_deadline(error: SourceWanderAndChaseReconstructionError) -> bool {
    matches!(
        error,
        SourceWanderAndChaseReconstructionError::Native(Error::Deadline)
            | SourceWanderAndChaseReconstructionError::Wander(
                SourceWanderReconstructionError::Native(SourceWanderError::Boundary(
                    Error::Deadline
                ))
            )
            | SourceWanderAndChaseReconstructionError::Chase(SourceChaseFixedCacheError::Native(
                SourceSamePolygonError::Boundary(Error::Deadline)
            ))
    )
}

#[test]
fn deadline_before_allocation_and_after_every_observed_stage_never_publishes_a_partial_pair() {
    let original = original(false);
    let wander_recipe = original.wander_witness.recipe().unwrap();
    let chase_recipe = original.chase_witness.recipe().unwrap();
    let wander_challenge = SourceWanderReconstructionChallenge::new();
    let chase_challenge = SourceChaseFixedCacheChallenge::new();
    let calls = RefCell::new(0);
    let expired = SourceMapQuery::reconstruct_wander_and_chase_observed(
        SLOT,
        PARAMS,
        &tiles(),
        &wander_recipe,
        &wander_challenge,
        &chase_recipe,
        &chase_challenge,
        Instant::now(),
        |_| {
            *calls.borrow_mut() += 1;
            Ok(())
        },
        |_| {
            *calls.borrow_mut() += 1;
            Ok(())
        },
    )
    .unwrap_err();
    assert_eq!(
        expired,
        SourceWanderAndChaseReconstructionError::Native(Error::Deadline)
    );
    assert_eq!(*calls.borrow(), 0);
    let total_stages = 2 + 3 * (wander_recipe.operations.len() + chase_recipe.operations.len());
    for expired_stage in 0..total_stages {
        let deadline = Instant::now() + Duration::from_millis(250);
        let calls = RefCell::new(0);
        let expire = || {
            let index = *calls.borrow();
            *calls.borrow_mut() += 1;
            if index == expired_stage {
                std::thread::sleep(
                    deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
                );
            }
        };
        let result = SourceMapQuery::reconstruct_wander_and_chase_observed(
            SLOT,
            PARAMS,
            &tiles(),
            &wander_recipe,
            &wander_challenge,
            &chase_recipe,
            &chase_challenge,
            deadline,
            |_| {
                expire();
                Ok(())
            },
            |_| {
                expire();
                Ok(())
            },
        );
        let error = result.unwrap_err();
        assert!(is_deadline(error), "stage {expired_stage}: {error:?}");
        assert_eq!(
            *calls.borrow(),
            expired_stage + 1,
            "deadline must stop subsequent observation"
        );
        let retry = reconstruct(&original, &wander_challenge, &chase_challenge);
        assert_pair(&original, &retry);
        assert!(retry.matches_challenges(&wander_challenge, &chase_challenge));
    }
}

#[test]
fn missing_tiles_and_malformed_inputs_stop_at_the_reached_boundary_and_keep_originals_usable() {
    let mut original = original(false);
    let wander_recipe = original.wander_witness.recipe().unwrap();
    let chase_recipe = original.chase_witness.recipe().unwrap();
    let wander_challenge = SourceWanderReconstructionChallenge::new();
    let chase_challenge = SourceChaseFixedCacheChallenge::new();
    for lineage in 0..2 {
        let mut wander = wander_recipe.clone();
        let mut chase = chase_recipe.clone();
        // Valid world coordinates on a different unloaded tile, not an endpoint
        // polygon miss inside the loaded tile (which is a defined Wander NOPATH).
        if lineage == 0 {
            wander.operations[0].from = point(600., 5., 0.);
            wander.operations[0].raw_end = point(600., 6., 0.);
        } else {
            chase.operations[0].from = point(600., 5., 0.);
            chase.operations[0].to = point(600., 6., 0.);
        }
        wander.validate().unwrap();
        chase.validate().unwrap();
        let calls = RefCell::new([0, 0]);
        let result = SourceMapQuery::reconstruct_wander_and_chase_observed(
            SLOT,
            PARAMS,
            &tiles(),
            &wander,
            &wander_challenge,
            &chase,
            &chase_challenge,
            until(),
            |_| {
                calls.borrow_mut()[0] += 1;
                Ok(())
            },
            |_| {
                calls.borrow_mut()[1] += 1;
                Ok(())
            },
        );
        assert_eq!(
            result.unwrap_err(),
            if lineage == 0 {
                SourceWanderAndChaseReconstructionError::Wander(
                    SourceWanderReconstructionError::UnsupportedLineage,
                )
            } else {
                SourceWanderAndChaseReconstructionError::Chase(
                    SourceChaseFixedCacheError::UnsupportedLineage,
                )
            }
        );
        assert_eq!(
            *calls.borrow(),
            if lineage == 0 {
                [3, 0]
            } else {
                [1 + 3 * wander.operations.len(), 2]
            }
        );
        assert_pair(
            &original,
            &reconstruct(&original, &wander_challenge, &chase_challenge),
        );
    }
    for invalid in 0..5 {
        let mut wander = wander_recipe.clone();
        let mut chase = chase_recipe.clone();
        let mut params = PARAMS;
        match invalid {
            0 => wander.version += 1,
            1 => chase.version += 1,
            2 => wander.slot.instance_id += 1,
            3 => chase.slot.instance_id += 1,
            4 => params = &[],
            _ => unreachable!(),
        }
        let calls = RefCell::new(0);
        let result = SourceMapQuery::reconstruct_wander_and_chase_observed(
            SLOT,
            params,
            &tiles(),
            &wander,
            &wander_challenge,
            &chase,
            &chase_challenge,
            until(),
            |_| {
                *calls.borrow_mut() += 1;
                Ok(())
            },
            |_| {
                *calls.borrow_mut() += 1;
                Ok(())
            },
        );
        assert!(result.is_err(), "invalid case {invalid}");
        assert_eq!(
            *calls.borrow(),
            0,
            "both recipes and inventory must validate before replay"
        );
        assert_pair(
            &original,
            &reconstruct(&original, &wander_challenge, &chase_challenge),
        );
    }
    let pair = reconstruct(&original, &wander_challenge, &chase_challenge);
    let (_fresh, wander, chase) = pair.into_parts();
    let (mut wander_owner, wander, _) = wander.into_parts();
    let (mut chase_owner, chase, _) = chase.into_parts();
    let wander_next = wander_execute(&mut wander_owner, Some(&wander), normal_b());
    original.wander = wander_execute(
        &mut original.wander_owner,
        Some(&original.wander),
        normal_b(),
    );
    assert_wander_operation(&original.wander, &wander_next);
    let chase_next = chase_execute(&mut chase_owner, Some(&chase), nearest10());
    original.chase = chase_execute(
        &mut original.chase_owner,
        Some(&original.chase),
        nearest10(),
    );
    assert_chase_operation(&original.chase, &chase_next);
}
