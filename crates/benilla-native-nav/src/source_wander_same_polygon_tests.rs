// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::{SourceWarmLookupOrigin, SourceWarmPrimitive, TileInput};
use std::time::Duration;

#[path = "source_wander_reconstruction_witness_tests.rs"]
mod reconstruction_witness;

const SLOT: SourceMapQuerySlot = SourceMapQuerySlot {
    map_id: 0,
    instance_id: 0,
};
const CONFIG: SourceSamePolygonConfig = SourceSamePolygonConfig {
    normalize_z: false,
    ignore_normalization: false,
};
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
fn run(
    selected: &SelectedSourceQuery<'_>,
    prior: Option<&LoadedSourceWanderTerminal>,
    filter: SourceOwnerFilter,
) -> LoadedSourceWanderTerminal {
    selected
        .loaded_wander_same_polygon(
            prior,
            1,
            filter,
            CONFIG,
            SourceWanderBudget::default(),
            until(),
        )
        .unwrap()
}
#[test]
fn first_wander_uses_endpoint_append_then_height_then_original_tail_build() {
    for filter in [
        SourceOwnerFilter::GROUND,
        SourceOwnerFilter {
            include_flags: 13,
            exclude_flags: 0,
        },
    ] {
        let source = query();
        let mut retained = source.retain_default();
        let selected = retained
            .select_for_wander(SLOT, point(5., 5., 0.), point(6., 5., 0.25))
            .unwrap();
        let result = run(&selected, None, filter);
        result.check_query(&selected).unwrap();
        assert_eq!(result.before_storage(), &[0; 74]);
        assert_eq!(result.before_active_len(), 0);
        assert_eq!(
            result.endpoint_lookup().unwrap().origin,
            SourceWarmLookupOrigin::Nearest5
        );
        assert!(result.endpoint_lookup().unwrap().appended);
        assert_eq!(result.endpoint_trace().len(), 1);
        assert_eq!(result.endpoint_trace()[0].endpoint, 3);
        assert_eq!(
            result.endpoint_trace()[0].primitive,
            SourceWarmPrimitive::Nearest5
        );
        assert_eq!(result.height_before(), point(6., 5., 0.25));
        assert_eq!(result.height_after(), point(6., 5., 0.));
        assert_eq!(result.height_status(), Some(crate::SUCCESS));
        assert_eq!(
            result.start_lookup().unwrap().origin,
            SourceWarmLookupOrigin::Tail
        );
        assert_eq!(
            result.end_lookup().unwrap().origin,
            SourceWarmLookupOrigin::Tail
        );
        assert_eq!(result.build_trace().len(), 2);
        assert!(result
            .build_trace()
            .iter()
            .all(|step| step.primitive == SourceWarmPrimitive::Tail));
        assert_eq!(result.storage().len(), 75);
        assert_eq!(result.storage()[0], result.storage()[74]);
        assert_eq!(result.active_len(), 1);
        assert_eq!(result.points(), &[point(5., 5., 0.), point(6., 5., 0.)]);
        assert_eq!(result.filter(), filter);
        assert_eq!(result.path_type(), 1);
    }
}
#[test]
fn retained_wander_clears_active_but_keeps_tail_and_original_allocation() {
    let source = query();
    let mut retained = source.retain_default();
    let first = {
        let selected = retained
            .select_for_wander(SLOT, point(5., 5., 0.), point(6., 5., 0.))
            .unwrap();
        run(&selected, None, SourceOwnerFilter::GROUND)
    };
    let storage = first.storage().to_vec();
    let selected = retained
        .select_for_wander(SLOT, point(6., 5., 0.), point(7., 5., 0.))
        .unwrap();
    let second = run(&selected, Some(&first), SourceOwnerFilter::GROUND);
    assert_eq!(second.before_storage(), storage);
    assert_eq!(second.before_active_len(), 1);
    assert_eq!(
        second.endpoint_lookup().unwrap().origin,
        SourceWarmLookupOrigin::Tail
    );
    assert!(!second.endpoint_lookup().unwrap().appended);
    assert!(second
        .endpoint_trace()
        .iter()
        .all(|step| step.primitive == SourceWarmPrimitive::Tail));
    assert_eq!(second.storage(), storage);
    assert_eq!(first.storage(), storage);
    let reloaded = query();
    let mut unrelated = reloaded.retain_default();
    let other = unrelated
        .select_for_wander(SLOT, point(6., 5., 0.), point(7., 5., 0.))
        .unwrap();
    assert!(matches!(
        other.loaded_wander_same_polygon(
            Some(&first),
            1,
            SourceOwnerFilter::GROUND,
            CONFIG,
            SourceWanderBudget::default(),
            until()
        ),
        Err(SourceWanderError::Boundary(Error::Identity))
    ));
}
#[test]
fn unsupported_or_exhausted_work_never_changes_the_prior_cache() {
    let source = query();
    let mut retained = source.retain_default();
    let first = {
        let selected = retained
            .select_for_wander(SLOT, point(5., 5., 0.), point(6., 5., 0.))
            .unwrap();
        run(&selected, None, SourceOwnerFilter::GROUND)
    };
    let before = first.storage().to_vec();
    let selected = retained
        .select_for_wander(SLOT, point(6., 5., 0.), point(7., 5., 0.))
        .unwrap();
    for budget in [
        SourceWanderBudget {
            endpoint_lookup_primitives: 0,
            ..SourceWanderBudget::default()
        },
        SourceWanderBudget {
            build_lookup_primitives: 0,
            ..SourceWanderBudget::default()
        },
    ] {
        assert!(matches!(
            selected.loaded_wander_same_polygon(
                Some(&first),
                1,
                SourceOwnerFilter::GROUND,
                CONFIG,
                budget,
                until()
            ),
            Err(SourceWanderError::Lookup(SourceWarmCacheError::Boundary(
                Error::Budget
            )))
        ));
        assert_eq!(first.storage(), before);
    }
    assert!(matches!(
        selected.loaded_wander_same_polygon(
            Some(&first),
            0,
            SourceOwnerFilter::GROUND,
            CONFIG,
            SourceWanderBudget::default(),
            until()
        ),
        Err(SourceWanderError::Boundary(Error::Unsupported))
    ));
    assert!(matches!(
        selected.loaded_wander_same_polygon(
            Some(&first),
            1,
            SourceOwnerFilter::GROUND,
            CONFIG,
            SourceWanderBudget::default(),
            Instant::now()
        ),
        Err(SourceWanderError::Boundary(Error::Deadline))
    ));
    assert_eq!(
        run(&selected, Some(&first), SourceOwnerFilter::GROUND).storage(),
        before
    );
}

#[test]
fn explicit_prefix_clears_before_selection_and_continues_without_second_selection() {
    let source = query();
    let mut retained = source.retain_default();
    let mut prefix = retained
        .prepare_wander_same_polygon(
            None,
            1,
            SourceOwnerFilter::GROUND,
            CONFIG,
            SourceWanderBudget::default(),
            until(),
        )
        .unwrap();
    assert!(!retained.current_selected());
    assert!(!prefix.current_selected());
    assert_eq!(prefix.cleared_active_len(), 0);
    assert!(matches!(
        retained.continue_wander_prefix(&prefix, point(5., 5., 0.), point(6., 5., 0.)),
        Err(Error::Identity)
    ));
    retained.select_wander_prefix(&mut prefix, until()).unwrap();
    assert!(retained.current_selected());
    assert!(prefix.current_selected());
    assert_eq!(
        retained.select_wander_prefix(&mut prefix, until()),
        Err(Error::Identity)
    );
    let selected = retained
        .continue_wander_prefix(&prefix, point(5., 5., 0.), point(6., 5., 0.))
        .unwrap();
    let result = selected
        .loaded_prepared_wander_same_polygon(&prefix, until())
        .unwrap();
    assert_eq!(result.points(), &[point(5., 5., 0.), point(6., 5., 0.)]);
    assert_eq!(prefix.cleared_active_len(), 0);
    assert_eq!(prefix.before_storage(), &[0; 74]);
}

#[test]
fn cross_polygon_and_wet_filter_stay_unsupported_but_endpoint_misses_are_defined_no_path() {
    let source = query();
    let mut retained = source.retain_default();
    for to in [point(5., 15., 0.), point(25., 5., 0.)] {
        let selected = retained
            .select_for_wander(SLOT, point(5., 5., 0.), to)
            .unwrap();
        assert!(matches!(
            selected.loaded_wander_same_polygon(
                None,
                1,
                SourceOwnerFilter::GROUND,
                CONFIG,
                SourceWanderBudget::default(),
                until()
            ),
            Err(SourceWanderError::NonSamePolygonUnsupported)
        ));
    }
    let selected = retained
        .select_for_wander(SLOT, point(5., 5., 0.), point(6., 5., 0.))
        .unwrap();
    assert!(matches!(
        selected.loaded_wander_same_polygon(
            None,
            1,
            SourceOwnerFilter {
                include_flags: 4,
                exclude_flags: 0
            },
            CONFIG,
            SourceWanderBudget::default(),
            until()
        ),
        Err(SourceWanderError::Boundary(Error::Unsupported))
    ));
    let close = retained
        .select_for_wander(SLOT, point(5., 5., 0.), point(5., 5., 0.))
        .unwrap();
    assert!(matches!(
        close.loaded_wander_same_polygon(
            None,
            1,
            SourceOwnerFilter::GROUND,
            CONFIG,
            SourceWanderBudget::default(),
            until()
        ),
        Err(SourceWanderError::TooClose)
    ));
    let outside = retained
        .select_for_wander(SLOT, point(5., 5., 0.), point(31., 5., 0.))
        .unwrap();
    assert!(outside.tile_presence(until()).unwrap().both_present());
    let height_failure = run(&outside, None, SourceOwnerFilter::GROUND);
    assert_eq!(
        height_failure.no_path_reason(),
        Some(SourceWanderNoPathReason::EndpointHeight)
    );
    assert_eq!(height_failure.path_type(), 8);
    assert_eq!(height_failure.active_len(), 0);
    assert_eq!(height_failure.storage().len(), 75);
    assert_eq!(height_failure.storage()[0], 0);
    assert_ne!(height_failure.storage()[74], 0);
    assert_eq!(height_failure.actual_end(), point(31., 5., 0.));
    assert_ne!(height_failure.height_status(), Some(crate::SUCCESS));
    assert!(height_failure.tail_required());
    assert!(height_failure.build_trace().is_empty());
    assert!(height_failure.points().is_empty());
    let missing = retained
        .select_for_wander(SLOT, point(5., 5., 0.), point(200., 200., 0.))
        .unwrap();
    assert!(missing.tile_presence(until()).unwrap().both_present());
    let missing = run(&missing, None, SourceOwnerFilter::GROUND);
    assert_eq!(
        missing.no_path_reason(),
        Some(SourceWanderNoPathReason::EndpointMissing)
    );
    assert_eq!(missing.storage(), &[0; 74]);
    assert_eq!(missing.active_len(), 0);
    assert_eq!(missing.actual_end(), point(200., 200., 0.));
    assert_eq!(missing.height_status(), None);
    assert!(missing.tail_required());
    assert_eq!(
        missing
            .endpoint_trace()
            .iter()
            .map(|t| t.primitive)
            .collect::<Vec<_>>(),
        vec![
            SourceWarmPrimitive::Nearest5,
            SourceWarmPrimitive::Nearest10
        ]
    );
    assert!(missing.build_trace().is_empty());
    for failed in [&height_failure, &missing] {
        let mut prefix = retained
            .prepare_wander_same_polygon(
                Some(failed),
                1,
                SourceOwnerFilter::GROUND,
                CONFIG,
                SourceWanderBudget::default(),
                until(),
            )
            .unwrap();
        retained.select_wander_prefix(&mut prefix, until()).unwrap();
        let selected = retained
            .continue_wander_prefix(&prefix, point(5., 5., 0.), point(6., 5., 0.))
            .unwrap();
        let result = selected
            .loaded_prepared_wander_same_polygon(&prefix, until())
            .unwrap();
        assert_eq!(result.path_type(), 1);
        assert_eq!(result.before_storage(), failed.storage());
        assert_eq!(result.active_len(), 1);
    }
}

#[test]
fn exact_minimum_distance_no_path_keeps_defined_clear_and_initial_end_without_queries() {
    let source = query();
    let mut retained = source.retain_default();
    let mut prefix = retained
        .prepare_wander_same_polygon(
            None,
            1,
            SourceOwnerFilter::GROUND,
            CONFIG,
            SourceWanderBudget::default(),
            until(),
        )
        .unwrap();
    retained.select_wander_prefix(&mut prefix, until()).unwrap();
    let from = point(0., 0., 0.);
    let below = [0x3dcc_cccc, 0, 0]; // RNE32(x*x) is threshold minus one ULP.
    let exact = [0x3dcc_cccc, 0x3800_0000, 0]; // RNE32(x*x+y*y) is exactly 0x3c23d70a.
    let selected = retained
        .continue_wander_prefix(&prefix, from, below)
        .unwrap();
    let no_path = selected
        .loaded_prepared_wander_no_path(&prefix, until())
        .unwrap();
    no_path.check_query(&selected).unwrap();
    assert_eq!(no_path.path_type(), 8);
    assert_eq!(no_path.active_len(), 0);
    assert_eq!(no_path.storage(), &[0; 74]);
    assert_eq!(no_path.actual_end(), [0; 3]);
    assert!(no_path.points().is_empty());
    assert!(no_path.endpoint_trace().is_empty());
    assert!(no_path.build_trace().is_empty());
    assert_eq!(no_path.endpoint_lookup(), None);
    assert_eq!(no_path.height_status(), None);
    assert!(!no_path.tail_required());
    assert_eq!(
        no_path.no_path_reason(),
        Some(SourceWanderNoPathReason::MinimumDistance)
    );
    let selected = retained
        .continue_wander_prefix(&prefix, from, exact)
        .unwrap();
    assert!(matches!(
        selected.loaded_prepared_wander_no_path(&prefix, until()),
        Err(SourceWanderError::NotTooClose)
    ));
    let selected = retained
        .continue_wander_prefix(&prefix, from, [0x3dcc_cccd, 0, 0])
        .unwrap();
    assert!(matches!(
        selected.loaded_prepared_wander_no_path(&prefix, until()),
        Err(SourceWanderError::NotTooClose)
    ));
}

#[test]
fn no_path_then_normal_then_no_path_preserves_each_actual_retained_lineage() {
    let source = query();
    let mut retained = source.retain_default();
    let mut prefix = retained
        .prepare_wander_same_polygon(
            None,
            1,
            SourceOwnerFilter::GROUND,
            CONFIG,
            SourceWanderBudget::default(),
            until(),
        )
        .unwrap();
    retained.select_wander_prefix(&mut prefix, until()).unwrap();
    let selected = retained
        .continue_wander_prefix(&prefix, point(5., 5., 0.), point(5., 5., 0.))
        .unwrap();
    let no_path = selected
        .loaded_prepared_wander_no_path(&prefix, until())
        .unwrap();
    let mut prefix = retained
        .prepare_wander_same_polygon(
            Some(&no_path),
            1,
            SourceOwnerFilter::GROUND,
            CONFIG,
            SourceWanderBudget::default(),
            until(),
        )
        .unwrap();
    retained.select_wander_prefix(&mut prefix, until()).unwrap();
    let selected = retained
        .continue_wander_prefix(&prefix, point(5., 5., 0.), point(6., 5., 0.))
        .unwrap();
    let normal = selected
        .loaded_prepared_wander_same_polygon(&prefix, until())
        .unwrap();
    assert_eq!(normal.before_storage(), no_path.storage());
    assert_eq!(normal.path_type(), 1);
    assert_eq!(normal.storage().len(), 75);
    let mut prefix = retained
        .prepare_wander_same_polygon(
            Some(&normal),
            1,
            SourceOwnerFilter::GROUND,
            CONFIG,
            SourceWanderBudget::default(),
            until(),
        )
        .unwrap();
    retained.select_wander_prefix(&mut prefix, until()).unwrap();
    let selected = retained
        .continue_wander_prefix(&prefix, point(6., 5., 0.), point(6., 5., 0.))
        .unwrap();
    let second = selected
        .loaded_prepared_wander_no_path(&prefix, until())
        .unwrap();
    assert_eq!(second.before_active_len(), 1);
    assert_eq!(second.active_len(), 0);
    assert_eq!(second.storage(), normal.storage());
    assert_eq!(second.actual_end(), normal.actual_end());
    assert_eq!(normal.active_len(), 1);
    assert!(second.points().is_empty());
}

#[test]
fn complete_source_observer_matches_same_loaded_precision_geometry_and_full_native_state() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../scripts/native_wander_source_oracle/vectors.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["compatibility_policy"],
        SOURCE_WANDER_ENDPOINT_COMPATIBILITY_ID
    );
    let bits = |value: &serde_json::Value| -> [u32; 3] {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u32)
            .collect::<Vec<_>>()
            .try_into()
            .unwrap()
    };
    let mut count = 0;
    for row in fixture["snapshots"].as_array().unwrap() {
        if row["geometry"] != "unchanged-existing-precision-fixture" {
            continue;
        }
        let source = query();
        let mut retained = source.retain_default();
        let mut prefix = retained
            .prepare_wander_same_polygon(
                None,
                1,
                SourceOwnerFilter::GROUND,
                CONFIG,
                SourceWanderBudget::default(),
                until(),
            )
            .unwrap();
        retained.select_wander_prefix(&mut prefix, until()).unwrap();
        // These source rows sample their ground-height center, so raw and corrected Z
        // are both zero. Sampling itself has a separate software/source differential.
        let selected = retained
            .continue_wander_prefix(
                &prefix,
                bits(&row["current_bits"]),
                bits(&row["endpoint_bits"]),
            )
            .unwrap();
        assert!(selected.tile_presence(until()).unwrap().both_present());
        let result = selected
            .loaded_prepared_wander_same_polygon(&prefix, until())
            .unwrap();
        assert_eq!(result.query_capacity(), 1024);
        assert_eq!(
            result.path_type(),
            row["path_type"].as_u64().unwrap() as u32,
            "{}",
            row["case"]
        );
        assert_eq!(
            result.active_len(),
            row["active_polys"].as_u64().unwrap() as usize
        );
        assert_eq!(result.actual_end(), bits(&row["endpoint_bits"]));
        let expected_refs: Vec<u64> = row["storage"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap())
            .collect();
        assert_eq!(result.storage(), expected_refs, "{}", row["case"]);
        let expected_points: Vec<[u32; 3]> = row["points_bits"]
            .as_array()
            .unwrap()
            .iter()
            .map(bits)
            .collect();
        assert_eq!(result.points(), expected_points);
        let primitive = |step: &crate::SourceWarmTrace| match step.primitive {
            SourceWarmPrimitive::Nearest5 => "detour:nearest:5",
            SourceWarmPrimitive::Nearest10 => "detour:nearest:10",
            SourceWarmPrimitive::Tail => "detour:closest-retained",
            _ => panic!("unsupported operation in Wander same-poly fixture"),
        };
        let mut actual_trace: Vec<&str> = result.endpoint_trace().iter().map(primitive).collect();
        if result.height_status().is_some() {
            actual_trace.push("detour:height");
        }
        actual_trace.extend(result.build_trace().iter().map(primitive));
        let expected_trace: Vec<&str> = row["trace"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str().filter(|word| word.starts_with("detour:")))
            .collect();
        assert_eq!(actual_trace, expected_trace, "{}", row["case"]);
        count += 1;
    }
    assert_eq!(count, 3);
}
