// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::{SourceWarmLookupOrigin, SourceWarmPolyPhase, SourceWarmPrimitive, TileInput};
use std::time::Duration;

const SLOT: SourceMapQuerySlot = SourceMapQuerySlot {
    map_id: 0,
    instance_id: 0,
};
const CONFIG: SourceSamePolygonConfig = SourceSamePolygonConfig {
    normalize_z: false,
    ignore_normalization: false,
};
fn query(slot: SourceMapQuerySlot) -> SourceMapQuery {
    SourceMapQuery::load(
        slot,
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
fn budget() -> SourceSamePolygonBudget {
    SourceSamePolygonBudget {
        lookup_primitives: SOURCE_RETAINED_SAME_POLYGON_MAX_LOOKUP_PRIMITIVES,
        terminal_primitives: 1,
    }
}
fn cold(selected: &SelectedSourceQuery<'_>) -> LoadedSourceSamePolygon {
    selected
        .loaded_cold_same_polygon(1, CONFIG, SourceSamePolygonBudget::default(), until())
        .unwrap()
}
fn retained(
    selected: &SelectedSourceQuery<'_>,
    prior: &LoadedSourceSamePolygon,
) -> LoadedSourceSamePolygon {
    selected
        .loaded_retained_same_polygon(prior, CONFIG, budget(), until())
        .unwrap()
}
fn assert_same(a: &LoadedSourceSamePolygon, b: &LoadedSourceSamePolygon) {
    assert_eq!(a.compatibility(), b.compatibility());
    assert_eq!(a.points(), b.points());
    assert_eq!(a.actual_end(), b.actual_end());
    assert_eq!(a.path_type(), b.path_type());
    assert_eq!(a.config(), b.config());
    assert_eq!(a.slot(), b.slot());
    assert_eq!(a.query_capacity(), b.query_capacity());
}
fn assert_continuation(prior: &LoadedSourceSamePolygon, next: &LoadedSourceSamePolygon) {
    let (before, after) = (prior.compatibility(), next.compatibility());
    assert!(after.is_retained());
    assert_eq!(after.before_storage(), before.storage());
    assert_eq!(after.before_active_len(), before.active_len());
    assert_eq!(after.initial_storage(), &[0; 74]);
    assert_eq!(after.initial_active_len(), 0);
    assert_eq!(after.compatibility_id(), before.compatibility_id());
    assert_eq!(
        after.compatibility_version(),
        before.compatibility_version()
    );
    assert_eq!(after.source_revision(), before.source_revision());
    assert_eq!(after.point_limit(), 74);
    assert_eq!(after.filter(), before.filter());
    assert_eq!(after.active_len(), 1);
    assert_eq!(after.phase(), SourceWarmPolyPhase::SamePolygon);
    assert_eq!(after.direct_status(), None);
    assert!(after.trace().iter().all(|event| event.endpoint < 2));
    assert_eq!(next.points(), &[after.from(), after.to()]);
    assert_eq!(next.actual_end(), after.to());
    assert_eq!(next.path_type(), 1);
    assert_eq!(next.terminal_primitives(), 1);
}

#[test]
fn initial_then_repeated_retained_preserves_policy_full_tail_points_and_query() {
    let source = query(SLOT);
    let mut owner = source.retain_default();
    let (a, b) = (point(5., 5., 0.), point(5., 6., 0.));
    let initial = cold(&owner.select_for_calculate(SLOT, a, b).unwrap());
    let original_storage = initial.compatibility().storage().to_vec();
    assert!(!initial.compatibility().is_retained());
    assert_eq!(initial.compatibility().before_storage(), &[0; 74]);
    assert_eq!(initial.compatibility().before_active_len(), 0);

    let selected = owner.select_for_calculate(SLOT, b, a).unwrap();
    let first = retained(&selected, &initial);
    assert_continuation(&initial, &first);
    assert_eq!(first.check_query(&selected), Ok(()));
    assert_eq!(initial.check_query(&selected), Err(Error::Identity));
    assert_eq!(first.compatibility().storage(), original_storage);
    assert_eq!(
        first.compatibility().start().origin,
        SourceWarmLookupOrigin::Active
    );
    assert_eq!(
        first.compatibility().end().origin,
        SourceWarmLookupOrigin::Active
    );
    assert_eq!(first.compatibility().trace().len(), 2);
    assert!(first
        .compatibility()
        .trace()
        .iter()
        .all(|event| event.primitive == SourceWarmPrimitive::Active));

    let selected = owner
        .select_for_calculate(SLOT, point(5., 5., 2.), point(5., 6., 2.5))
        .unwrap();
    let second = retained(&selected, &first);
    assert_continuation(&first, &second);
    let compatibility_alias: &LoadedSourceColdSamePolygon = &second;
    assert!(compatibility_alias.compatibility().is_retained());
    assert_eq!(
        compatibility_alias.compatibility().compatibility_version(),
        1
    );
    assert_eq!(second.check_query(&selected), Ok(()));
    assert_eq!(second.compatibility().storage(), original_storage);
    assert_eq!(
        second.compatibility().start().origin,
        SourceWarmLookupOrigin::Tail
    );
    assert_eq!(
        second.compatibility().end().origin,
        SourceWarmLookupOrigin::Tail
    );
    assert_eq!(
        second.compatibility().start().distance_bits,
        2_f32.to_bits()
    );
    assert_eq!(
        second.compatibility().end().distance_bits,
        2.5_f32.to_bits()
    );
    assert_eq!(second.compatibility().trace().len(), 4);
    assert_eq!(initial.compatibility().storage(), original_storage);
    assert_same(&retained(&selected, &first), &second);
    drop(source);
    drop(owner);
    assert_eq!(second.query_capacity(), 1024);
}

#[test]
fn retained_nearest_append_changes_only_slot_zero_and_preserves_every_inactive_tail() {
    let source = query(SLOT);
    let mut owner = source.retain_default();
    let (a, b) = (point(5., 5., 0.), point(5., 6., 0.));
    let initial = cold(&owner.select_for_calculate(SLOT, a, b).unwrap());
    let initial_storage = initial.compatibility().storage().to_vec();
    let selected = owner
        .select_for_calculate(SLOT, point(5., 15., 0.), point(5., 16., 0.))
        .unwrap();
    let first = retained(&selected, &initial);
    assert_continuation(&initial, &first);
    let after = first.compatibility();
    assert_eq!(after.start().origin, SourceWarmLookupOrigin::Nearest5);
    assert!(after.start().appended);
    assert_eq!(after.end().origin, SourceWarmLookupOrigin::Tail);
    assert!(!after.end().appended);
    assert_eq!(after.storage().len(), 76);
    assert_eq!(&after.storage()[1..75], &initial_storage[1..]);
    assert_ne!(after.storage()[0], initial_storage[0]);
    assert_eq!(after.storage()[75], after.storage()[0]);

    let selected = owner.select_for_calculate(SLOT, a, b).unwrap();
    let second = retained(&selected, &first);
    assert_continuation(&first, &second);
    assert_eq!(
        second.compatibility().start().origin,
        SourceWarmLookupOrigin::Tail
    );
    assert_eq!(
        second.compatibility().end().origin,
        SourceWarmLookupOrigin::Tail
    );
    assert_eq!(second.compatibility().storage()[0], initial_storage[0]);
    assert_eq!(
        &second.compatibility().storage()[1..],
        &after.storage()[1..]
    );
    assert_eq!(initial.compatibility().storage(), initial_storage);
    // Selecting an earlier actual proposal does not adopt the uncommitted later proposal.
    let from_initial = retained(&selected, &initial);
    assert_eq!(
        from_initial.compatibility().before_storage(),
        initial_storage
    );
    assert_eq!(from_initial.compatibility().storage().len(), 75);
    assert_eq!(
        selected
            .loaded_retained_same_polygon(
                &first,
                CONFIG,
                SourceSamePolygonBudget {
                    lookup_primitives: 1,
                    terminal_primitives: 1,
                },
                until(),
            )
            .unwrap_err(),
        SourceSamePolygonError::Lookup(SourceWarmCacheError::Boundary(Error::Budget))
    );
    assert_eq!(
        first.compatibility().storage(),
        second.compatibility().before_storage()
    );
    assert_same(&retained(&selected, &first), &second);
}

#[test]
fn retained_query_config_and_exclusive_borrow_fail_without_mutating_prior() {
    let source = query(SLOT);
    let mut owner = source.retain_default();
    let (a, b) = (point(5., 5., 0.), point(5., 6., 0.));
    let initial = cold(&owner.select_for_calculate(SLOT, a, b).unwrap());
    let selected = owner.select_for_calculate(SLOT, b, a).unwrap();
    let expected = retained(&selected, &initial);
    for slot in [
        SLOT,
        SourceMapQuerySlot {
            instance_id: 7,
            ..SLOT
        },
    ] {
        let foreign = query(slot);
        let mut lease = foreign.retain_default();
        let different = lease.select_for_calculate(slot, b, a).unwrap();
        assert_eq!(
            different
                .loaded_retained_same_polygon(&initial, CONFIG, budget(), until())
                .unwrap_err(),
            SourceSamePolygonError::Boundary(Error::Identity)
        );
    }
    for (normalize_z, ignore_normalization) in [(true, false), (false, true), (true, true)] {
        assert_eq!(
            selected
                .loaded_retained_same_polygon(
                    &initial,
                    SourceSamePolygonConfig {
                        normalize_z,
                        ignore_normalization
                    },
                    budget(),
                    until()
                )
                .unwrap_err(),
            SourceSamePolygonError::ConfigurationIdentity
        );
    }
    let borrowed = selected.borrow_mesh().unwrap();
    assert_eq!(
        selected
            .loaded_retained_same_polygon(&initial, CONFIG, budget(), until())
            .unwrap_err(),
        SourceSamePolygonError::Boundary(Error::Unavailable)
    );
    drop(borrowed);
    assert_same(&retained(&selected, &initial), &expected);
    let mut another_lease = source.retain_default();
    let same = another_lease.select_for_calculate(SLOT, b, a).unwrap();
    assert_eq!(expected.check_query(&same), Ok(()));
    assert_same(&retained(&same, &initial), &expected);
}

#[test]
fn retained_failed_partial_lookup_deadline_invalid_args_and_retry_leave_prior_exact() {
    let source = query(SLOT);
    let mut owner = source.retain_default();
    let (a, b) = (point(5., 5., 0.), point(5., 6., 0.));
    let initial = cold(&owner.select_for_calculate(SLOT, a, b).unwrap());
    let prior_storage = initial.compatibility().storage().to_vec();
    let selected = owner
        .select_for_calculate(SLOT, point(5., 15., 0.), point(5., 16., 0.))
        .unwrap();
    let expected = retained(&selected, &initial);
    let needed = expected.compatibility().trace().len();
    // The start appends after three primitives; every incomplete candidate is discarded.
    for lookup_primitives in 0..needed {
        assert_eq!(
            selected
                .loaded_retained_same_polygon(
                    &initial,
                    CONFIG,
                    SourceSamePolygonBudget {
                        lookup_primitives,
                        terminal_primitives: 1
                    },
                    until()
                )
                .unwrap_err(),
            SourceSamePolygonError::Lookup(SourceWarmCacheError::Boundary(Error::Budget))
        );
        assert_eq!(initial.compatibility().storage(), prior_storage);
        assert_same(&retained(&selected, &initial), &expected);
    }
    let minimal = selected
        .loaded_retained_same_polygon(
            &initial,
            CONFIG,
            SourceSamePolygonBudget {
                lookup_primitives: needed,
                terminal_primitives: 1,
            },
            until(),
        )
        .unwrap();
    assert_same(&minimal, &expected);
    for invalid in [
        SourceSamePolygonBudget {
            lookup_primitives: SOURCE_RETAINED_SAME_POLYGON_MAX_LOOKUP_PRIMITIVES + 1,
            terminal_primitives: 1,
        },
        SourceSamePolygonBudget {
            lookup_primitives: usize::MAX,
            terminal_primitives: 1,
        },
        SourceSamePolygonBudget {
            lookup_primitives: needed,
            terminal_primitives: 0,
        },
        SourceSamePolygonBudget {
            lookup_primitives: needed,
            terminal_primitives: 2,
        },
    ] {
        assert_eq!(
            selected
                .loaded_retained_same_polygon(&initial, CONFIG, invalid, until())
                .unwrap_err(),
            SourceSamePolygonError::Boundary(Error::Budget)
        );
    }
    assert_eq!(
        selected
            .loaded_retained_same_polygon(&initial, CONFIG, budget(), Instant::now())
            .unwrap_err(),
        SourceSamePolygonError::Boundary(Error::Deadline)
    );
    for (from, to, expected_error) in [
        (
            point(5., -11., 0.),
            b,
            SourceSamePolygonError::Lookup(SourceWarmCacheError::MissingPolygon),
        ),
        (
            point(5., -8., 0.),
            b,
            SourceSamePolygonError::Lookup(SourceWarmCacheError::FarFromPolygonUnsupported),
        ),
        (
            a,
            point(5000., 5., 0.),
            SourceSamePolygonError::Lookup(SourceWarmCacheError::Boundary(Error::Budget)),
        ),
        (
            a,
            point(5., 15., 0.),
            SourceSamePolygonError::NonSamePolygonUnsupported,
        ),
        (
            point(5., 15., 0.),
            point(25., 5., 0.),
            SourceSamePolygonError::NonSamePolygonUnsupported,
        ),
    ] {
        let selected = owner.select_for_calculate(SLOT, from, to).unwrap();
        for _ in 0..2 {
            assert_eq!(
                selected
                    .loaded_retained_same_polygon(&initial, CONFIG, budget(), until())
                    .unwrap_err(),
                expected_error
            );
            assert_eq!(initial.compatibility().storage(), prior_storage);
        }
    }
    for bad in [f32::NAN, f32::INFINITY, -f32::INFINITY, f32::MAX] {
        assert!(owner
            .select_for_calculate(SLOT, point(bad, 5., 0.), b)
            .is_err());
        assert!(owner
            .select_for_calculate(SLOT, a, point(5., bad, 0.))
            .is_err());
    }
    let selected = owner
        .select_for_calculate(SLOT, point(5., 15., 0.), point(5., 16., 0.))
        .unwrap();
    assert_same(&retained(&selected, &initial), &expected);
    assert_eq!(initial.compatibility().storage(), prior_storage);
    assert_eq!(initial.points(), &[a, b]);
}

#[test]
fn repaired_initial_then_original_defined_retained_source_oracle_matches_exact_chain() {
    let fixture = include_str!("../tools/fixtures/retained-same-poly/cases.test-fixture");
    let headings: Vec<_> = fixture
        .lines()
        .find_map(|line| line.strip_prefix("# label\t"))
        .map(|rest| std::iter::once("label").chain(rest.split('\t')).collect())
        .unwrap();
    let parse_point = |value: &str| -> [u32; 3] {
        value
            .split(',')
            .map(|word| u32::from_str_radix(word, 16).unwrap())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap()
    };
    let parse_refs =
        |value: &str| -> Vec<u64> { value.split(',').map(|word| word.parse().unwrap()).collect() };
    let source = query(SLOT);
    let mut owner = source.retain_default();
    let mut history: Vec<LoadedSourceSamePolygon> = Vec::new();
    for row in fixture
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let values: Vec<_> = row.split('\t').collect();
        assert_eq!(values.len(), headings.len());
        let get = |name: &str| {
            values[headings
                .iter()
                .position(|heading| *heading == name)
                .unwrap()]
        };
        let step: usize = get("step").parse().unwrap();
        assert_eq!(step, history.len());
        assert_eq!(get("sequence"), "retained_main");
        assert_eq!(get("geometry"), "precision");
        assert_eq!(get("normalize_z"), "0");
        assert_eq!(get("ignore_normalization"), "0");
        let selected = owner
            .select_for_calculate(SLOT, parse_point(get("from")), parse_point(get("to")))
            .unwrap();
        let result = if let Some(prior) = history.last() {
            assert_eq!(get("oracle"), "original-retained-defined");
            let next = retained(&selected, prior);
            assert_continuation(prior, &next);
            next
        } else {
            assert_eq!(get("oracle"), "compatibility-zero-v1");
            cold(&selected)
        };
        assert_eq!(result.check_query(&selected), Ok(()));
        assert_eq!(
            result.query_capacity(),
            get("query_nodes").parse::<u32>().unwrap()
        );
        let observed = result.compatibility();
        assert_eq!(observed.is_retained(), step != 0);
        assert_eq!(observed.initial_storage(), &[0; 74]);
        assert_eq!(
            observed.before_active_len(),
            get("before_active").parse::<usize>().unwrap()
        );
        assert_eq!(observed.before_storage(), parse_refs(get("before_storage")));
        assert_eq!(
            observed.start().polygon,
            get("start_ref").parse::<u64>().unwrap()
        );
        assert_eq!(
            observed.end().polygon,
            get("end_ref").parse::<u64>().unwrap()
        );
        assert_eq!(
            observed.start().distance_bits,
            u32::from_str_radix(get("start_distance"), 16).unwrap()
        );
        assert_eq!(
            observed.end().distance_bits,
            u32::from_str_radix(get("end_distance"), 16).unwrap()
        );
        let mut lookup_storage = observed.before_storage().to_vec();
        if observed.start().appended {
            lookup_storage.push(observed.start().polygon);
        }
        assert_eq!(lookup_storage, parse_refs(get("after_start_storage")));
        if observed.end().appended {
            lookup_storage.push(observed.end().polygon);
        }
        assert_eq!(lookup_storage, parse_refs(get("lookup_storage")));
        assert_eq!(observed.storage(), parse_refs(get("after_storage")));
        assert_eq!(&observed.storage()[1..], &lookup_storage[1..]);
        assert_eq!(
            observed.active_len(),
            get("after_active").parse::<usize>().unwrap()
        );
        assert_eq!(result.path_type(), get("path_type").parse::<u32>().unwrap());
        assert_eq!(result.actual_end(), parse_point(get("actual_end")));
        let points: Vec<_> = get("points").split(';').map(parse_point).collect();
        assert_eq!(result.points(), points, "{}", get("label"));
        match get("label") {
            "retained_active_squared3_equal" => {
                assert_eq!(observed.start().origin, SourceWarmLookupOrigin::Active)
            }
            "retained_tail_squared4" | "retained_tail_squared7_equal" => {
                assert_eq!(observed.start().origin, SourceWarmLookupOrigin::Tail)
            }
            "retained_nearest_equal" => {
                assert_eq!(observed.start().origin, SourceWarmLookupOrigin::Nearest5);
                assert!(!observed.start().appended);
            }
            "retained_nearest10_equal" => {
                assert_eq!(observed.start().origin, SourceWarmLookupOrigin::Nearest10);
                assert!(!observed.start().appended);
            }
            _ => (),
        }
        history.push(result);
    }
    assert_eq!(history.len(), 11);
    for pair in history.windows(2) {
        assert_continuation(&pair[0], &pair[1]);
    }
}
