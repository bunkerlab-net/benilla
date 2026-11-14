// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
fn mesh() -> MapMesh {
    let mut mesh = MapMesh::new(include_bytes!(
        "../tools/fixtures/direct-corridor/params.test-fixture"
    ))
    .unwrap();
    mesh.load_instance(7).unwrap();
    mesh.add_tile(TileInput {
        grid: [31, 31],
        bytes: include_bytes!("../tools/fixtures/warm-cache/precision.test-fixture"),
    })
    .unwrap();
    mesh
}
const FIRST: u64 = 0x0010_0000_0000_0000;
fn lookup(
    mesh: &MapMesh,
    cache: &mut PathCache,
    y: f32,
    z: f32,
    distance: &mut PathDistance,
) -> Option<SourceWarmLookup> {
    mesh.pathfinder_lookup(
        &mesh.query(7).unwrap(),
        cache,
        SourceOwnerFilter {
            include_flags: 1,
            exclude_flags: 0,
        },
        [y, z, 5.],
        distance,
    )
    .unwrap()
}
#[test]
fn all_five_scalars_at_four_declarations_use_absence_and_near_then_far_boxes() {
    let mesh = mesh();
    for site in [
        DistanceDeclaration::BuildPolyStart,
        DistanceDeclaration::BuildPolyEnd,
        DistanceDeclaration::RayHit,
        DistanceDeclaration::SteppedHeight,
        DistanceDeclaration::RandomPoint,
    ] {
        for (y, origin, value) in [
            (5., SourceWarmLookupOrigin::Nearest5, 0.),
            (-6., SourceWarmLookupOrigin::Nearest10, 6.),
        ] {
            let mut cache = PathCache::new();
            let mut distance = PathDistance::new(site);
            let found = lookup(&mesh, &mut cache, y, 0., &mut distance).unwrap();
            assert_eq!(found.polygon, FIRST);
            assert_eq!(found.origin, origin);
            assert!(found.appended);
            assert_eq!(distance.value(), Some(value));
            assert_eq!(&cache.storage()[74..], &[FIRST]);
            assert_eq!(cache.active_len(), 0);
        }
    }
}
#[test]
fn defined_priors_and_retained_loop_lifetime_preserve_source_append_predicate() {
    let mesh = mesh();
    for (prior, appended) in [
        (0., true),
        (-0., true),
        (1., false),
        (2., false),
        (3., true),
    ] {
        let mut cache = PathCache::new();
        let mut distance = PathDistance::new(DistanceDeclaration::SteppedHeight);
        distance.value = Some(prior);
        let found = lookup(&mesh, &mut cache, 5., -2., &mut distance).unwrap();
        assert_eq!(found.appended, appended);
        assert_eq!(distance.value(), Some(2.));
    }
    let mut cache = PathCache::new();
    let mut distance = PathDistance::new(DistanceDeclaration::SteppedHeight);
    assert!(lookup(&mesh, &mut cache, -20., 0., &mut distance).is_none());
    assert_eq!(distance.value(), None);
    assert!(
        lookup(&mesh, &mut cache, -6., 0., &mut distance)
            .unwrap()
            .appended
    );
    assert_eq!(distance.value(), Some(6.));
    assert_eq!(
        lookup(&mesh, &mut cache, 5., -2., &mut distance)
            .unwrap()
            .origin,
        SourceWarmLookupOrigin::Tail
    );
    assert!(
        !lookup(&mesh, &mut cache, 5., -3., &mut distance)
            .unwrap()
            .appended
    );
}
#[test]
fn active_then_tail_ties_and_clear_keep_real_storage() {
    let mesh = mesh();
    let mut cache = PathCache::new();
    cache.install_prefix(&[FIRST, FIRST + 1]).unwrap();
    let mut distance = PathDistance::new(DistanceDeclaration::BuildPolyStart);
    let found = lookup(&mesh, &mut cache, 10., 0., &mut distance).unwrap();
    assert_eq!(found.origin, SourceWarmLookupOrigin::Active);
    assert_eq!(found.polygon, FIRST);
    let found = lookup(&mesh, &mut cache, 5., -2., &mut distance).unwrap();
    assert_eq!(found.origin, SourceWarmLookupOrigin::Nearest5);
    assert!(!found.appended);
    cache.storage.push(FIRST + 3);
    assert!(
        lookup(&mesh, &mut cache, 5., -3., &mut distance)
            .unwrap()
            .appended
    );
    let before = cache.storage.clone();
    cache.clear();
    assert_eq!(cache.active_len(), 0);
    assert_eq!(cache.storage(), before);
    assert_eq!(
        lookup(&mesh, &mut cache, 5., 0., &mut distance)
            .unwrap()
            .origin,
        SourceWarmLookupOrigin::Tail
    );
}
#[test]
fn per_instance_identity_and_removed_polygon_refs_are_not_reconstituted() {
    let mut mesh = mesh();
    let old = mesh.query(7).unwrap();
    mesh.unload_instance(7);
    mesh.load_instance(7).unwrap();
    let mut cache = PathCache::new();
    let mut distance = PathDistance::new(DistanceDeclaration::RandomPoint);
    assert_eq!(
        mesh.pathfinder_lookup(
            &old,
            &mut cache,
            SourceOwnerFilter {
                include_flags: 1,
                exclude_flags: 0
            },
            [5., 0., 5.],
            &mut distance
        ),
        Err(LiveError::StaleQuery)
    );
    cache.install_prefix(&[FIRST]).unwrap();
    mesh.remove_tile([31, 31]).unwrap();
    assert!(lookup(&mesh, &mut cache, 5., 0., &mut distance).is_none());
    assert!(distance.value().unwrap() > 1e18);
}

#[test]
fn nearest_ties_keep_source_visit_order_and_use_the_actual_filter() {
    let mesh = mesh();
    let mut cache = PathCache::new();
    let mut distance = PathDistance::new(DistanceDeclaration::BuildPolyStart);
    let found = lookup(&mesh, &mut cache, 10., 0., &mut distance).unwrap();
    assert_eq!(found.origin, SourceWarmLookupOrigin::Nearest5);
    assert_eq!(found.polygon, FIRST);
    for (include_flags, exclude_flags) in [(0, 0), (1, 1)] {
        let mut cache = PathCache::new();
        assert!(mesh
            .pathfinder_lookup(
                &mesh.query(7).unwrap(),
                &mut cache,
                SourceOwnerFilter {
                    include_flags,
                    exclude_flags
                },
                [10., 0., 5.],
                &mut PathDistance::new(DistanceDeclaration::RandomPoint)
            )
            .unwrap()
            .is_none());
        assert_eq!(cache.storage().len(), 74);
    }
}
