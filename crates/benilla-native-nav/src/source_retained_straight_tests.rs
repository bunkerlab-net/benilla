// SPDX-License-Identifier: GPL-3.0-or-later
//! Loaded composition tests, not an independent original cold-to-straight oracle.
use super::*;
use crate::{
    SourceSamePolygonBudget, SourceSamePolygonError, SourceWarmCacheError, SourceWarmPrimitive,
    TileInput, SOURCE_RETAINED_SAME_POLYGON_MAX_LOOKUP_PRIMITIVES,
};
use std::time::Duration;

const SLOT: SourceMapQuerySlot = SourceMapQuerySlot {
    map_id: 0,
    instance_id: 0,
};
const CONFIG: SourceRetainedStraightConfig = SourceRetainedStraightConfig {
    normalization: SourceSamePolygonConfig {
        normalize_z: false,
        ignore_normalization: false,
    },
    optimize_path: true,
};
const WIDTH: u32 = 0.125_f32.to_bits();

fn query() -> SourceMapQuery {
    SourceMapQuery::load(
        SLOT,
        include_bytes!("../tools/fixtures/direct-corridor/params.test-fixture"),
        &[TileInput {
            grid: [31, 31],
            bytes: include_bytes!("../tools/fixtures/warm-cache/precision.test-fixture"),
        }],
    )
    .unwrap()
}
fn point(x: f32, y: f32, z: f32) -> [u32; 3] {
    [x, y, z].map(f32::to_bits)
}
fn until() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
fn cold(selected: &SelectedSourceQuery<'_>, filter: SourceOwnerFilter) -> LoadedSourceSamePolygon {
    selected
        .loaded_cold_same_polygon_with_filter(
            1,
            filter,
            CONFIG.normalization,
            SourceSamePolygonBudget::default(),
            until(),
        )
        .unwrap()
}
fn first(
    selected: &SelectedSourceQuery<'_>,
    prior: &LoadedSourceSamePolygon,
) -> LoadedSourceRetainedStraight {
    selected
        .loaded_retained_straight_from_same_polygon(
            prior,
            CONFIG,
            WIDTH,
            SourceStraightPathBudget::default(),
            until(),
        )
        .unwrap()
}
fn next(
    selected: &SelectedSourceQuery<'_>,
    prior: &LoadedSourceRetainedStraight,
) -> LoadedSourceRetainedStraight {
    selected
        .loaded_retained_straight(
            prior,
            CONFIG,
            WIDTH,
            SourceStraightPathBudget::default(),
            until(),
        )
        .unwrap()
}
fn exact_budget(result: &LoadedSourceRetainedStraight) -> SourceStraightPathBudget {
    let diagnostics = result.diagnostics();
    SourceStraightPathBudget {
        lookup_primitives: result.trace().len(),
        raycast_polygons: diagnostics.raycast_polygons,
        sampled_points: diagnostics.sampled_points,
        native_primitives: diagnostics.native_primitives,
    }
}
fn assert_origin(result: &LoadedSourceRetainedStraight, filter: SourceOwnerFilter) {
    assert_eq!(result.slot(), SLOT);
    assert_eq!(result.query_capacity(), 1024);
    assert_eq!(result.query_policy(), crate::SOURCE_MAP_QUERY_POLICY);
    assert_eq!(
        result.compatibility_id(),
        crate::SOURCE_COLD_DISTANCE_COMPATIBILITY_ID
    );
    assert_eq!(result.compatibility_version(), 1);
    assert_eq!(
        result.source_revision(),
        crate::SOURCE_COLD_DISTANCE_COMPATIBILITY_REVISION
    );
    assert_eq!(result.initial_storage(), &[0; 74]);
    assert_eq!(result.initial_active_len(), 0);
    assert_eq!(result.point_limit(), 74);
    assert_eq!(result.filter(), filter);
    assert_eq!(result.config(), CONFIG);
    assert_eq!(result.collision_width_bits(), WIDTH);
    assert!(!result.terminal.config().force_destination);
}

#[test]
fn genuine_cold_replacement_then_cut_and_same_preserve_complete_storage_and_policy() {
    for include_flags in [1, 13] {
        let filter = SourceOwnerFilter {
            include_flags,
            exclude_flags: 0,
        };
        let source = query();
        let mut owner = source.retain_default();
        let selected = owner
            .select_for_calculate(SLOT, point(5., 5., 0.), point(5., 6., 0.))
            .unwrap();
        let initial = cold(&selected, filter);
        // The first bridge also accepts an actual retained same-polygon result,
        // not only the original cold terminal (the resident epoch-four entry).
        let initial = if include_flags == 13 {
            selected
                .loaded_retained_same_polygon(
                    &initial,
                    CONFIG.normalization,
                    SourceSamePolygonBudget::default(),
                    until(),
                )
                .unwrap()
        } else {
            initial
        };
        assert_eq!(initial.compatibility().is_retained(), include_flags == 13);
        let initial_storage = initial.compatibility().storage().to_vec();
        assert_eq!(initial_storage.len(), 75);
        let a = initial_storage[0];
        assert_eq!(initial_storage[74], a);

        let selected = owner
            .select_for_calculate(SLOT, point(5., 5., 0.), point(5., 15., 0.))
            .unwrap();
        assert_eq!(
            selected
                .loaded_retained_same_polygon(
                    &initial,
                    CONFIG.normalization,
                    SourceSamePolygonBudget {
                        lookup_primitives: SOURCE_RETAINED_SAME_POLYGON_MAX_LOOKUP_PRIMITIVES,
                        terminal_primitives: 1,
                    },
                    until(),
                )
                .unwrap_err(),
            SourceSamePolygonError::NonSamePolygonUnsupported,
        );
        let replacement = first(&selected, &initial);
        assert_origin(&replacement, filter);
        assert_eq!(replacement.check_query(&selected), Ok(()));
        assert_eq!(replacement.check_same_polygon_predecessor(&initial), Ok(()));
        assert_eq!(replacement.phase(), SourceWarmPolyPhase::Replacement);
        assert_eq!(replacement.before_storage(), initial_storage);
        assert_eq!(replacement.before_active_len(), 1);
        assert_eq!(replacement.previous_path_type(), initial.path_type());
        assert_eq!(replacement.path_type(), 1);
        assert_eq!(replacement.active_len(), 2);
        assert_eq!(replacement.actual_end(), point(5., 15., 0.));
        assert_eq!(replacement.end(), replacement.requested_end());
        assert_eq!(
            replacement.points(),
            &[
                point(5., 5., 0.),
                point(5., 9., 0.),
                point(5., 13., 0.),
                point(5., 15., 0.),
            ]
        );
        let b = replacement.end_lookup().polygon;
        assert_ne!(a, b);
        let mut expected = initial_storage.clone();
        expected[1] = b;
        expected.push(b);
        assert_eq!(replacement.storage(), expected);
        let ray = replacement.ray().unwrap();
        assert_eq!(ray.status, crate::SUCCESS);
        assert_eq!(ray.hit_bits, f32::MAX.to_bits());
        assert_eq!(ray.adjusted_hit_bits, None);
        assert_eq!(ray.polygons, [a, b]);
        assert_eq!(replacement.diagnostics().raycast_polygons, 2);
        assert_eq!(replacement.diagnostics().sampled_points, 4);
        assert_eq!(replacement.diagnostics().heights.len(), 2);
        assert!(replacement.trace().iter().all(|event| !matches!(
            event.primitive,
            SourceWarmPrimitive::DirectCreate
                | SourceWarmPrimitive::DirectExpansion
                | SourceWarmPrimitive::DirectFinish
        )));
        assert_eq!(initial.compatibility().storage(), initial_storage);

        let selected = owner
            .select_for_calculate(SLOT, point(5., 7., 0.), point(5., 15., 0.))
            .unwrap();
        let cut = next(&selected, &replacement);
        assert_origin(&cut, filter);
        assert_eq!(cut.check_predecessor(&replacement), Ok(()));
        assert_eq!(cut.phase(), SourceWarmPolyPhase::Cut);
        assert_eq!(cut.before_storage(), replacement.storage());
        assert_eq!(cut.before_active_len(), replacement.active_len());
        assert_eq!(cut.storage(), replacement.storage());
        assert_eq!(cut.active_len(), 2);
        assert_eq!(cut.previous_path_type(), replacement.path_type());
        assert_eq!(cut.ray(), None);
        assert_eq!(cut.path_type(), 1);
        assert_eq!(
            cut.points(),
            &[point(5., 7., 0.), point(5., 11., 0.), point(5., 15., 0.)]
        );

        let selected = owner
            .select_for_calculate(SLOT, point(5., 15., 0.), point(5., 16., 0.))
            .unwrap();
        let same = next(&selected, &cut);
        assert_origin(&same, filter);
        assert_eq!(same.check_predecessor(&cut), Ok(()));
        assert_eq!(same.phase(), SourceWarmPolyPhase::SamePolygon);
        assert_eq!(same.active_len(), 1);
        assert_eq!(same.storage()[0], b);
        assert_eq!(&same.storage()[1..], &cut.storage()[1..]);
        assert_eq!(same.points(), &[point(5., 15., 0.), point(5., 16., 0.)]);
        assert_eq!(same.ray(), None);
        assert_eq!(same.diagnostics().sampled_points, 0);
        assert_eq!(same.diagnostics().native_primitives, 1);
    }
}

#[test]
fn raw_incomplete_and_nopath_are_not_promoted_or_silently_retried() {
    let source = query();
    let mut owner = source.retain_default();
    let initial = cold(
        &owner
            .select_for_calculate(SLOT, point(5., 5., 0.), point(5., 6., 0.))
            .unwrap(),
        SourceOwnerFilter::GROUND,
    );
    let selected = owner
        .select_for_calculate(SLOT, point(5., 5., 0.), point(25., 5., 0.))
        .unwrap();
    let clipped = first(&selected, &initial);
    assert_eq!(clipped.path_type(), 4);
    assert_eq!(clipped.end(), point(7.5, 5., 0.));
    assert_eq!(clipped.actual_end(), clipped.end());
    assert_ne!(clipped.actual_end(), clipped.requested_end());
    assert_eq!(clipped.ray().unwrap().adjusted_hit_bits, Some(WIDTH));
    let refused = selected
        .loaded_retained_straight_from_same_polygon(
            &initial,
            CONFIG,
            0.5_f32.to_bits(),
            Default::default(),
            until(),
        )
        .unwrap();
    assert_eq!(refused.path_type(), 8);
    assert!(refused.points().is_empty());
    assert_eq!(refused.actual_end(), refused.requested_end());
    assert_eq!(refused.diagnostics().sampled_points, 0);

    let selected = owner
        .select_for_calculate(SLOT, point(5., 5., 0.), point(5., 15., 0.))
        .unwrap();
    let inherited = next(&selected, &clipped);
    assert_eq!(inherited.previous_path_type(), 4);
    assert_eq!(inherited.path_type(), 4);
    assert_eq!(inherited.ray().unwrap().hit_bits, f32::MAX.to_bits());
    assert_eq!(clipped.path_type(), 4);
}

#[test]
fn predecessor_tokens_reject_siblings_and_do_not_retain_prior_result_chains() {
    let source = query();
    let mut owner = source.retain_default();
    let selected = owner
        .select_for_calculate(SLOT, point(5., 5., 0.), point(5., 6., 0.))
        .unwrap();
    let initial = cold(&selected, SourceOwnerFilter::GROUND);
    let sibling_initial = cold(&selected, SourceOwnerFilter::GROUND);
    let initial_token = Rc::downgrade(&initial.identity);
    let selected = owner
        .select_for_calculate(SLOT, point(5., 5., 0.), point(5., 15., 0.))
        .unwrap();
    let first = Rc::new(first(&selected, &initial));
    let first_object = Rc::downgrade(&first);
    let sibling = selected
        .loaded_retained_straight_from_same_polygon(
            &initial,
            CONFIG,
            WIDTH,
            Default::default(),
            until(),
        )
        .unwrap();
    assert_eq!(first.check_same_polygon_predecessor(&initial), Ok(()));
    assert_eq!(
        first.check_same_polygon_predecessor(&sibling_initial),
        Err(Error::Identity)
    );
    drop(sibling_initial);
    let first_token = Rc::downgrade(&first.identity);
    let second = next(&selected, &first);
    assert_eq!(second.check_predecessor(&first), Ok(()));
    assert_eq!(second.check_predecessor(&sibling), Err(Error::Identity));
    assert_eq!(
        second.check_same_polygon_predecessor(&initial),
        Err(Error::Identity)
    );
    drop(sibling);
    drop(initial);
    assert!(initial_token.upgrade().is_some());
    drop(first);
    assert!(first_object.upgrade().is_none());
    assert!(initial_token.upgrade().is_none());
    assert!(first_token.upgrade().is_some());
    let third = next(&selected, &second);
    drop(second);
    assert!(first_token.upgrade().is_none());
    drop(source);
    drop(owner);
    assert_eq!(third.query_capacity(), 1024);
    assert_eq!(third.initial_storage(), &[0; 74]);
}

#[test]
fn exact_and_one_short_budgets_discard_candidates_and_preserve_both_prior_kinds() {
    let source = query();
    let mut owner = source.retain_default();
    let initial = cold(
        &owner
            .select_for_calculate(SLOT, point(5., 5., 0.), point(5., 6., 0.))
            .unwrap(),
        SourceOwnerFilter::GROUND,
    );
    let initial_storage = initial.compatibility().storage().to_vec();
    let selected = owner
        .select_for_calculate(SLOT, point(5., 5., 0.), point(5., 15., 0.))
        .unwrap();
    let prior = first(&selected, &initial);
    let successor = next(&selected, &prior);
    for retained in [false, true] {
        let expected = if retained { &successor } else { &prior };
        let exact = exact_budget(expected);
        let calculate = |budget, deadline| {
            if retained {
                selected.loaded_retained_straight(&prior, CONFIG, WIDTH, budget, deadline)
            } else {
                selected.loaded_retained_straight_from_same_polygon(
                    &initial, CONFIG, WIDTH, budget, deadline,
                )
            }
        };
        assert_eq!(
            calculate(exact, until()).unwrap().terminal,
            expected.terminal
        );
        for field in 0..4 {
            let mut short = exact;
            let count = match field {
                0 => &mut short.lookup_primitives,
                1 => &mut short.raycast_polygons,
                2 => &mut short.sampled_points,
                3 => &mut short.native_primitives,
                _ => unreachable!(),
            };
            if *count == 0 {
                continue;
            }
            *count -= 1;
            assert!(matches!(
                calculate(short, until()).unwrap_err(),
                SourceRetainedStraightError::Straight(SourceStraightPathError::Boundary(
                    Error::Budget
                )) | SourceRetainedStraightError::Straight(SourceStraightPathError::WarmCache(
                    SourceWarmCacheError::Boundary(Error::Budget)
                ))
            ));
            assert_eq!(initial.compatibility().storage(), initial_storage);
            assert_eq!(
                calculate(exact, until()).unwrap().terminal,
                expected.terminal
            );
        }
        assert_eq!(
            calculate(exact, Instant::now() - Duration::from_millis(1)).unwrap_err(),
            SourceRetainedStraightError::Boundary(Error::Deadline)
        );
        assert_eq!(
            calculate(exact, until()).unwrap().terminal,
            expected.terminal
        );
    }
    assert_eq!(initial.compatibility().storage(), initial_storage);
    assert_eq!(first(&selected, &initial).terminal, prior.terminal);
}

#[test]
fn allocation_endpoints_configuration_width_and_cap_errors_do_not_mutate_priors() {
    let source = query();
    let mut owner = source.retain_default();
    let initial = cold(
        &owner
            .select_for_calculate(SLOT, point(5., 5., 0.), point(5., 6., 0.))
            .unwrap(),
        SourceOwnerFilter::GROUND,
    );
    let selected = owner
        .select_for_calculate(SLOT, point(5., 5., 0.), point(5., 15., 0.))
        .unwrap();
    let prior = first(&selected, &initial);
    let replacement_source = query();
    let mut replacement_owner = replacement_source.retain_default();
    let foreign = replacement_owner
        .select_for_calculate(SLOT, selected.from(), selected.to())
        .unwrap();
    assert_eq!(prior.check_query(&foreign), Err(Error::Identity));
    assert_eq!(
        foreign
            .loaded_retained_straight_from_same_polygon(
                &initial,
                CONFIG,
                WIDTH,
                Default::default(),
                until()
            )
            .unwrap_err(),
        SourceRetainedStraightError::Boundary(Error::Identity)
    );
    assert_eq!(
        foreign
            .loaded_retained_straight(&prior, CONFIG, WIDTH, Default::default(), until())
            .unwrap_err(),
        SourceRetainedStraightError::Boundary(Error::Identity)
    );
    let mut changed = CONFIG;
    changed.normalization.normalize_z = true;
    assert_eq!(
        selected
            .loaded_retained_straight_from_same_polygon(
                &initial,
                changed,
                WIDTH,
                Default::default(),
                until()
            )
            .unwrap_err(),
        SourceRetainedStraightError::ConfigurationIdentity
    );
    changed = CONFIG;
    changed.optimize_path = !changed.optimize_path;
    assert_eq!(
        selected
            .loaded_retained_straight(&prior, changed, WIDTH, Default::default(), until())
            .unwrap_err(),
        SourceRetainedStraightError::ConfigurationIdentity
    );
    assert_eq!(
        selected
            .loaded_retained_straight(
                &prior,
                CONFIG,
                0.25_f32.to_bits(),
                Default::default(),
                until()
            )
            .unwrap_err(),
        SourceRetainedStraightError::CollisionWidthIdentity
    );
    for width in [f32::NAN, f32::INFINITY, -1.] {
        assert_eq!(
            selected
                .loaded_retained_straight_from_same_polygon(
                    &initial,
                    CONFIG,
                    width.to_bits(),
                    Default::default(),
                    until()
                )
                .unwrap_err(),
            SourceRetainedStraightError::Straight(SourceStraightPathError::InvalidCollisionWidth)
        );
    }
    for field in 0..4 {
        let mut over = SourceStraightPathBudget::default();
        match field {
            0 => over.lookup_primitives += 1,
            1 => over.raycast_polygons += 1,
            2 => over.sampled_points += 1,
            3 => over.native_primitives += 1,
            _ => unreachable!(),
        }
        assert_eq!(
            selected
                .loaded_retained_straight_from_same_polygon(&initial, CONFIG, WIDTH, over, until())
                .unwrap_err(),
            SourceRetainedStraightError::Straight(SourceStraightPathError::Boundary(Error::Budget))
        );
    }
    assert_eq!(first(&selected, &initial).terminal, prior.terminal);
    let changed_endpoints = owner
        .select_for_calculate(SLOT, point(5., 6., 0.), point(5., 15., 0.))
        .unwrap();
    assert_eq!(prior.check_query(&changed_endpoints), Err(Error::Identity));
}
