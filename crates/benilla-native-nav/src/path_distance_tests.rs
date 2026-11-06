// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::time::Duration;

const FIRST: u64 = 4503599627370496;
const SECOND: u64 = FIRST + 1;
const DISCONNECTED: u64 = FIRST + 3;

fn work() -> Work {
    Work {
        budget: SourceWarmCacheBudget::default(),
        until: Instant::now() + Duration::from_secs(5),
        lookups: 0,
        expansions: 0,
        trace: Vec::new(),
    }
}
fn empty(mesh: &Mesh) -> WarmCache {
    WarmCache::authored_fixture(mesh, &[0; 74], 0).unwrap()
}
fn position(y: f32, z: f32) -> [f32; 3] {
    [y, z, 5.]
}

#[test]
fn every_fresh_scalar_keeps_near_and_far_nearest_without_storage_or_rng_inputs() {
    let mesh = crate::cold_distance_tests::loaded_mesh("precision");
    // Five scalar identities at four declarations (PathFinder.cpp:410,717,830,1400).
    for (caller, endpoint) in [
        ("start", 0),
        ("end", 1),
        ("ray-hit", 3),
        ("step", 4),
        ("random", 3),
    ] {
        for (y, origin, count, distance) in [
            (5., SourceWarmLookupOrigin::Nearest5, 1, 0.0f32),
            (-6., SourceWarmLookupOrigin::Nearest10, 2, 6.),
        ] {
            let mut cache = empty(&mesh);
            let mut local = None;
            let mut work = work();
            let found = lookup_with_distance(
                &mesh,
                &mut cache,
                &position(y, 0.),
                endpoint,
                &mut work,
                &mut local,
            )
            .unwrap();
            assert_eq!(found.polygon, FIRST, "{caller}");
            assert_eq!(found.origin, origin, "{caller}");
            assert!(found.appended, "{caller}");
            assert_eq!(
                local.map(f32::to_bits),
                Some(distance.to_bits()),
                "{caller}"
            );
            assert_eq!(cache.storage()[74..], [FIRST], "{caller}");
            assert_eq!(cache.active_len(), 0, "{caller}");
            assert_eq!(work.trace.len(), count, "{caller}");
            assert_eq!(work.lookups, count, "{caller}");
        }
    }
}

#[test]
fn defined_zero_negative_zero_equal_and_closer_predicates_are_not_absence() {
    let mesh = crate::cold_distance_tests::loaded_mesh("precision");
    for (prior, append) in [
        (0., true),
        (-0., true),
        (1., false),
        (2., false),
        (3., true),
    ] {
        let mut cache = empty(&mesh);
        let mut local = Some(prior);
        let found = lookup_with_distance(
            &mesh,
            &mut cache,
            &position(5., -2.),
            4,
            &mut work(),
            &mut local,
        )
        .unwrap();
        assert_eq!(found.distance_bits, 2f32.to_bits());
        assert_eq!(local, Some(2.));
        assert_eq!(found.appended, append, "prior {prior}");
        assert_eq!(cache.storage().len(), 74 + usize::from(append));
    }
}

#[test]
fn active_and_tail_scans_overwrite_the_actual_scalar_and_keep_first_equal_polygon() {
    let mesh = crate::cold_distance_tests::loaded_mesh("precision");
    let mut storage = vec![0; 74];
    storage[0] = FIRST;
    storage[1] = SECOND;
    let mut cache = WarmCache::authored_fixture(&mesh, &storage, 2).unwrap();
    let mut local = Some(99.);
    let found = lookup_with_distance(
        &mesh,
        &mut cache,
        &position(10., 0.),
        3,
        &mut work(),
        &mut local,
    )
    .unwrap();
    assert_eq!(found.origin, SourceWarmLookupOrigin::Active);
    assert_eq!(found.polygon, FIRST);
    assert_eq!(local, Some(0.));
    assert!(!found.appended);

    // Active distance 2 remains equal to nearest distance 2 (PathFinder.cpp:362).
    let mut local = None;
    let found = lookup_with_distance(
        &mesh,
        &mut cache,
        &position(5., -2.),
        3,
        &mut work(),
        &mut local,
    )
    .unwrap();
    assert_eq!(found.origin, SourceWarmLookupOrigin::Nearest5);
    assert_eq!(local, Some(2.));
    assert!(!found.appended);

    // Tail replaces, not minimizes with, the active distance (PathFinder.cpp:349).
    storage.push(DISCONNECTED);
    let mut cache = WarmCache::authored_fixture(&mesh, &storage, 2).unwrap();
    let mut local = None;
    let found = lookup_with_distance(
        &mesh,
        &mut cache,
        &position(5., -3.),
        4,
        &mut work(),
        &mut local,
    )
    .unwrap();
    assert!(found.appended);
    assert_eq!(local, Some(3.));
    assert_eq!(cache.storage()[74..], [DISCONNECTED, FIRST]);
}

#[test]
fn stepped_local_survives_failed_queries_and_receives_later_source_writes() {
    let mesh = crate::cold_distance_tests::loaded_mesh("precision");
    let mut cache = empty(&mesh);
    let mut distance = None;
    let mut work = work();
    assert_eq!(
        lookup_with_distance(
            &mesh,
            &mut cache,
            &position(-20., 0.),
            4,
            &mut work,
            &mut distance
        ),
        Err(SourceWarmCacheError::MissingPolygon)
    );
    assert_eq!(distance, None);
    let first = lookup_with_distance(
        &mesh,
        &mut cache,
        &position(-6., 0.),
        4,
        &mut work,
        &mut distance,
    )
    .unwrap();
    assert!(first.appended);
    assert_eq!(distance, Some(6.));
    let second = lookup_with_distance(
        &mesh,
        &mut cache,
        &position(5., -2.),
        4,
        &mut work,
        &mut distance,
    )
    .unwrap();
    assert_eq!(second.origin, SourceWarmLookupOrigin::Tail);
    assert_eq!(distance, Some(2.));
    assert!(!second.appended);
    let third = lookup_with_distance(
        &mesh,
        &mut cache,
        &position(5., -3.),
        4,
        &mut work,
        &mut distance,
    )
    .unwrap();
    assert_eq!(third.origin, SourceWarmLookupOrigin::Nearest5);
    assert_eq!(distance, Some(3.));
    assert!(!third.appended);
    assert_eq!(cache.storage()[74..], [FIRST]);
}

#[test]
fn random_point_clear_ignores_inactive_slots_but_preserves_searchable_tail() {
    let mut mesh = crate::cold_distance_tests::loaded_mesh("precision");
    for tail in [false, true] {
        let mut storage = vec![FIRST; 74];
        if tail {
            storage.push(SECOND);
        }
        let mut before = WarmCache::authored_fixture(&mesh, &storage, 1).unwrap();
        before.clear_active();
        let (after, found, trace) = wander_endpoint(
            &mut mesh,
            &before,
            [5., 15., 0.].map(f32::to_bits),
            100,
            work().until,
        )
        .unwrap();
        let found = found.unwrap();
        assert_eq!(found.polygon, SECOND);
        assert_eq!(found.appended, !tail);
        assert_eq!(
            found.origin,
            if tail {
                SourceWarmLookupOrigin::Tail
            } else {
                SourceWarmLookupOrigin::Nearest5
            }
        );
        assert_eq!(after.storage().len(), 75);
        assert_eq!(after.storage()[74], SECOND);
        assert_eq!(trace.len(), 1);
    }
}

#[test]
fn failed_nonempty_scans_write_maximum_distance_even_when_both_nearest_searches_miss() {
    let mesh = crate::cold_distance_tests::loaded_mesh("precision");
    for (active, length) in [(1, 74), (0, 75)] {
        let mut cache = WarmCache::authored_fixture(&mesh, &vec![0; length], active).unwrap();
        let mut distance = Some(7.);
        assert_eq!(
            lookup_with_distance(
                &mesh,
                &mut cache,
                &position(-20., 0.),
                4,
                &mut work(),
                &mut distance
            ),
            Err(SourceWarmCacheError::MissingPolygon),
        );
        assert_eq!(distance.map(f32::to_bits), Some(0x5f7fffff));
        assert_eq!(cache.storage(), vec![0; length]);
    }
}
