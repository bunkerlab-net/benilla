// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::{
    QueryProfile, SourceOwnerFilter, SourceWarmLookupOrigin, SourceWarmPolyPhase,
    SourceWarmPrimitive, TileInput,
};
use std::time::Duration;

const SLOT: SourceMapQuerySlot = SourceMapQuerySlot {
    map_id: 0,
    instance_id: 0,
};
const CONFIG: SourceColdSamePolygonConfig = SourceColdSamePolygonConfig {
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
fn run(selected: &SelectedSourceQuery<'_>) -> LoadedSourceColdSamePolygon {
    selected
        .loaded_cold_same_polygon(1, CONFIG, SourceColdSamePolygonBudget::default(), until())
        .unwrap()
}
fn assert_same(a: &LoadedSourceColdSamePolygon, b: &LoadedSourceColdSamePolygon) {
    assert_eq!(a.compatibility(), b.compatibility());
    assert_eq!(a.config(), b.config());
    assert_eq!(a.points(), b.points());
    assert_eq!(a.actual_end(), b.actual_end());
    assert_eq!(a.path_type(), b.path_type());
    assert_eq!(a.slot(), b.slot());
}

#[test]
fn repaired_source_same_polygon_oracle_matches_exact_terminal_and_retained_storage() {
    let fixture = include_str!("../tools/fixtures/cold-same-poly/cases.test-fixture");
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
    let mut count = 0;
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
        assert_eq!(get("oracle"), "compatibility-zero-v1");
        assert_eq!(get("geometry"), "precision");
        assert_eq!(get("normalize_z"), "0");
        assert_eq!(get("ignore_normalization"), "0");
        let source = query(SLOT);
        let mut retained = source.retain_default();
        let selected = retained
            .select_for_calculate(SLOT, parse_point(get("from")), parse_point(get("to")))
            .unwrap();
        let terminal = run(&selected);
        let cold = terminal.compatibility();
        assert_eq!(
            terminal.query_capacity(),
            get("query_nodes").parse::<u32>().unwrap()
        );
        assert_eq!(
            cold.initial_storage(),
            parse_refs(get("constructor_storage"))
        );
        assert_eq!(
            cold.initial_active_len(),
            get("constructor_active").parse::<usize>().unwrap()
        );
        assert_eq!(
            cold.start().polygon,
            get("start_ref").parse::<u64>().unwrap()
        );
        assert_eq!(cold.end().polygon, get("end_ref").parse::<u64>().unwrap());
        assert_eq!(
            cold.start().distance_bits,
            u32::from_str_radix(get("start_distance"), 16).unwrap()
        );
        assert_eq!(
            cold.end().distance_bits,
            u32::from_str_radix(get("end_distance"), 16).unwrap()
        );
        assert!(cold.start().appended);
        assert!(!cold.end().appended);
        assert_eq!(cold.storage(), parse_refs(get("after_storage")));
        assert_eq!(
            cold.active_len(),
            get("after_active").parse::<usize>().unwrap()
        );
        // The terminal only installs slot zero; all inactive/tail storage is preserved.
        let lookup = parse_refs(get("lookup_storage"));
        assert_eq!(lookup, parse_refs(get("after_start_storage")));
        assert_eq!(&cold.storage()[1..], &lookup[1..]);
        assert_eq!(
            terminal.path_type(),
            get("path_type").parse::<u32>().unwrap()
        );
        assert_eq!(terminal.actual_end(), parse_point(get("actual_end")));
        let points: Vec<_> = get("points").split(';').map(parse_point).collect();
        assert_eq!(terminal.points(), points, "{}", get("label"));
        assert_eq!(cold.direct_status(), None);
        assert!(cold.trace().iter().all(|event| event.endpoint < 2));
        count += 1;
    }
    assert_eq!(count, 4);
}

#[test]
fn selected_real_query_runs_zero_lookup_then_exact_same_polygon_terminal() {
    let source = query(SLOT);
    let mut retained = source.retain_default();
    let from = point(5., 5., 0.);
    let to = point(5., 6., 0.);
    let selected = retained.select_for_calculate(SLOT, from, to).unwrap();
    assert_eq!(
        selected.query_profile(),
        QueryProfile::SourceMapInstance1024
    );
    let presence = selected.tile_presence(until()).unwrap();
    assert!(presence.both_present());
    let terminal = run(&selected);
    let cold = terminal.compatibility();
    assert_eq!(terminal.check_query(&selected), Ok(()));
    assert_eq!(terminal.query_capacity(), 1024);
    assert_eq!(terminal.slot(), SLOT);
    assert_eq!(terminal.config(), CONFIG);
    assert_eq!(terminal.points(), &[from, to]);
    assert_eq!(terminal.actual_end(), to);
    assert_eq!(terminal.path_type(), 1);
    assert_eq!(terminal.terminal_primitives(), 1);
    assert_eq!(
        cold.compatibility_id(),
        "cmangos-buildpolypath-zero-distance-v1"
    );
    assert_eq!(cold.compatibility_version(), 1);
    assert_eq!(cold.initial_storage(), &[0; 74]);
    assert_eq!(cold.initial_active_len(), 0);
    assert_eq!(cold.point_limit(), 74);
    assert_eq!(
        cold.filter(),
        SourceOwnerFilter {
            include_flags: 1,
            exclude_flags: 0
        }
    );
    assert_eq!(cold.phase(), SourceWarmPolyPhase::SamePolygon);
    assert_eq!(cold.start().origin, SourceWarmLookupOrigin::Nearest5);
    assert_eq!(cold.start().distance_bits, 0.0_f32.to_bits());
    assert!(cold.start().appended);
    assert_eq!(cold.end().origin, SourceWarmLookupOrigin::Tail);
    assert!(!cold.end().appended);
    assert_eq!(cold.start().polygon, cold.end().polygon);
    assert_eq!(cold.storage()[0], cold.start().polygon);
    assert_eq!(&cold.storage()[1..74], &[0; 73]);
    assert_eq!(cold.storage()[74], cold.start().polygon);
    assert_eq!(cold.storage().len(), 75);
    assert_eq!(cold.active_len(), 1);
    assert_eq!(cold.direct_status(), None);
    assert_eq!(cold.trace().len(), 2);
    assert_eq!(cold.trace()[0].primitive, SourceWarmPrimitive::Nearest5);
    assert_eq!(cold.trace()[1].primitive, SourceWarmPrimitive::Tail);
    assert!(cold.trace().iter().all(|event| event.endpoint < 2));
    assert_eq!(selected.check_tile_presence(&presence), Ok(()));
    assert_same(&run(&selected), &terminal);
}

#[test]
fn exact_source_allocation_and_endpoints_are_required_even_for_identical_bytes() {
    let source = query(SLOT);
    let mut owner = source.retain_default();
    let from = point(5., 5., 0.);
    let to = point(5., 6., 0.);
    let selected = owner.select_for_calculate(SLOT, from, to).unwrap();
    let result = run(&selected);
    // The actual selected allocation is exclusively borrowed; an independent hidden
    // legacy/direct allocation could evade this refusal, but this API cannot.
    let active = selected.borrow_mesh().unwrap();
    assert_eq!(
        selected
            .loaded_cold_same_polygon(1, CONFIG, SourceColdSamePolygonBudget::default(), until())
            .unwrap_err(),
        SourceColdSamePolygonError::Boundary(Error::Unavailable)
    );
    drop(active);
    assert_same(&run(&selected), &result);
    for slot in [
        SLOT,
        SourceMapQuerySlot {
            instance_id: 7,
            ..SLOT
        },
    ] {
        let other = query(slot);
        let mut retained = other.retain_default();
        let selected = retained.select_for_calculate(slot, from, to).unwrap();
        assert_eq!(result.check_query(&selected), Err(Error::Identity));
    }
    let swapped = owner.select_for_calculate(SLOT, to, from).unwrap();
    assert_eq!(result.check_query(&swapped), Err(Error::Identity));
    let mut second_lease = source.retain_default();
    let selected = second_lease.select_for_calculate(SLOT, from, to).unwrap();
    assert_eq!(result.check_query(&selected), Ok(()));
    drop(source);
    assert_same(&run(&selected), &result);
    drop(second_lease);
    drop(owner);
    // The result itself retains the genuine query, not a serialized/hash identity.
    assert_eq!(result.query_capacity(), 1024);
}

#[test]
fn explicit_normalization_selection_is_required_and_never_defaulted_or_bypassed() {
    let source = query(SLOT);
    let mut retained = source.retain_default();
    let p = point(5., 5., -0.);
    let selected = retained.select_for_calculate(SLOT, p, p).unwrap();
    let expected = run(&selected);
    assert_eq!(expected.points(), &[p, p]);
    for (normalize_z, ignore_normalization) in [(true, false), (true, true), (false, true)] {
        assert_eq!(
            selected
                .loaded_cold_same_polygon(
                    1,
                    SourceColdSamePolygonConfig {
                        normalize_z,
                        ignore_normalization
                    },
                    SourceColdSamePolygonBudget::default(),
                    until()
                )
                .unwrap_err(),
            SourceColdSamePolygonError::NormalizationConfigurationUnsupported
        );
        assert_same(&run(&selected), &expected);
    }
    for version in [0, 2, u32::MAX] {
        assert_eq!(
            selected
                .loaded_cold_same_polygon(
                    version,
                    CONFIG,
                    SourceColdSamePolygonBudget::default(),
                    until()
                )
                .unwrap_err(),
            SourceColdSamePolygonError::Boundary(Error::Unsupported)
        );
    }
}

#[test]
fn different_polygons_reject_before_direct_work_and_never_publish_a_candidate() {
    let source = query(SLOT);
    let mut retained = source.retain_default();
    for to in [point(5., 15., 0.), point(25., 5., 0.)] {
        let selected = retained
            .select_for_calculate(SLOT, point(5., 5., 0.), to)
            .unwrap();
        for _ in 0..2 {
            assert_eq!(
                selected
                    .loaded_cold_same_polygon(
                        1,
                        CONFIG,
                        SourceColdSamePolygonBudget::default(),
                        until()
                    )
                    .unwrap_err(),
                SourceColdSamePolygonError::NonSamePolygonUnsupported
            );
        }
    }
    let selected = retained
        .select_for_calculate(SLOT, point(5., 5., 0.), point(5., 6., 0.))
        .unwrap();
    let terminal = run(&selected);
    assert_eq!(terminal.compatibility().initial_storage(), &[0; 74]);
    assert_eq!(terminal.compatibility().storage().len(), 75);
}

#[test]
fn work_budgets_deadline_missing_and_far_fail_without_poisoning_retry() {
    let source = query(SLOT);
    let mut retained = source.retain_default();
    let from = point(5., 5., 0.);
    let to = point(5., 6., 0.);
    let selected = retained.select_for_calculate(SLOT, from, to).unwrap();
    let expected = run(&selected);
    assert_eq!(
        selected
            .loaded_cold_same_polygon(
                1,
                CONFIG,
                SourceColdSamePolygonBudget::default(),
                Instant::now()
            )
            .unwrap_err(),
        SourceColdSamePolygonError::Boundary(Error::Deadline)
    );
    for lookup_primitives in [0, 1, SOURCE_COLD_SAME_POLYGON_MAX_LOOKUP_PRIMITIVES + 1] {
        let error = selected
            .loaded_cold_same_polygon(
                1,
                CONFIG,
                SourceColdSamePolygonBudget {
                    lookup_primitives,
                    terminal_primitives: 1,
                },
                until(),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            SourceColdSamePolygonError::Boundary(Error::Budget)
                | SourceColdSamePolygonError::Lookup(SourceWarmCacheError::Boundary(Error::Budget))
        ));
        assert_same(&run(&selected), &expected);
    }
    for terminal_primitives in [0, 2, usize::MAX] {
        assert_eq!(
            selected
                .loaded_cold_same_polygon(
                    1,
                    CONFIG,
                    SourceColdSamePolygonBudget {
                        lookup_primitives: 2,
                        terminal_primitives
                    },
                    until()
                )
                .unwrap_err(),
            SourceColdSamePolygonError::Boundary(Error::Budget)
        );
        assert_same(&run(&selected), &expected);
    }
    // Exactly two lookup calls suffice; operational budgets never become point limits.
    let minimal = selected
        .loaded_cold_same_polygon(
            1,
            CONFIG,
            SourceColdSamePolygonBudget {
                lookup_primitives: 2,
                terminal_primitives: 1,
            },
            until(),
        )
        .unwrap();
    assert_same(&minimal, &expected);
    for (from, error) in [
        (point(5., -11., 0.), SourceWarmCacheError::MissingPolygon),
        (
            point(5., -8., 0.),
            SourceWarmCacheError::FarFromPolygonUnsupported,
        ),
    ] {
        let selected = retained.select_for_calculate(SLOT, from, to).unwrap();
        assert_eq!(
            selected
                .loaded_cold_same_polygon(
                    1,
                    CONFIG,
                    SourceColdSamePolygonBudget::default(),
                    until()
                )
                .unwrap_err(),
            SourceColdSamePolygonError::Lookup(error)
        );
    }
    let selected = retained.select_for_calculate(SLOT, from, to).unwrap();
    assert_same(&run(&selected), &expected);
}
