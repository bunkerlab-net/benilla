// SPDX-License-Identifier: GPL-3.0-or-later
// Authored native bytes only; no installed terrain or server state.
use super::super::{
    tests as products, vmap::tests as vmap_tests, InventorySpec, ProductId, TileId,
};
use super::*;
use std::{collections::BTreeMap, time::Duration};

fn fixture() -> (InventorySpec, BTreeMap<String, Vec<u8>>) {
    terrain_coverage(vmap_tests::fixture())
}
fn terrain_coverage(
    (mut spec, mut files): (InventorySpec, BTreeMap<String, Vec<u8>>),
) -> (InventorySpec, BTreeMap<String, Vec<u8>>) {
    // Add terrain coverage at the actual source fixture's world-space model position.
    let old = spec.tiles[0];
    let tile = TileId { x: 0, y: 0 };
    spec.tiles[0] = tile;
    let remap = |id: ProductId| match id {
        ProductId::Terrain(t) if t == old => ProductId::Terrain(tile),
        ProductId::VmapTile(t) if t == old => ProductId::VmapTile(tile),
        ProductId::MmapTile(t) if t == old => ProductId::MmapTile(tile),
        id => id,
    };
    for product in &mut spec.products {
        let before = product.id.filename(spec.map).unwrap();
        product.id = remap(product.id.clone());
        let after = product.id.filename(spec.map).unwrap();
        if before != after {
            let bytes = files.remove(&before).unwrap();
            files.insert(after, bytes);
        }
        product.dependencies = product.dependencies.iter().cloned().map(remap).collect();
    }
    (spec, files)
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
fn request(snapshot: &StaticHeightSnapshot<'_>, z: f32) -> HeightRequest {
    // Source convertPositionToInternalRep uses 0.5*(64*533.33333333f).
    let mid = 0.5f32 * (64.0 * super::super::terrain::GRID_SIZE);
    HeightRequest {
        snapshot: snapshot.binding(),
        ticket: 1,
        position: [mid - 21.0, mid - 1.0, z].map(f32::to_bits),
        max_search: 50f32.to_bits(),
    }
}
#[test]
fn actual_gridmap_and_stacked_vmap_bytes_select_source_static_ground() {
    let (spec, files) = fixture();
    let products = products::load(spec, &files).unwrap();
    let snapshot = products.load_static_height().unwrap();
    for (z, height) in [(10f32, 9f32), (6., 6.), (7., 7.), (8., 9.), (-10., 6.)] {
        let request = request(&snapshot, z);
        assert_eq!(
            snapshot.query(request, deadline()).unwrap(),
            HeightReply {
                request,
                height: height.to_bits()
            }
        );
    }
    // No source model at this real covered point: both downward searches miss,
    // so the exact raw GridMap value wins rather than invented support.
    let mut query = request(&snapshot, 20.);
    query.position = [0f32, 800., 20.].map(f32::to_bits);
    assert_eq!(
        snapshot.query(query, deadline()).unwrap().height,
        7f32.to_bits()
    );
}
#[test]
fn loaded_geometry_performs_upward_fallback_without_segment_approximation() {
    let (mut spec, mut files) = fixture();
    products::mutate(
        &mut spec,
        &mut files,
        &ProductId::Terrain(TileId { x: 0, y: 0 }),
        |bytes| {
            bytes[56..60].copy_from_slice(&100f32.to_le_bytes());
            bytes[60..64].copy_from_slice(&100f32.to_le_bytes());
        },
    );
    let products = products::load(spec, &files).unwrap();
    let snapshot = products.load_static_height().unwrap();
    // Both downward probes from -8 miss; upward source search hits the actual floor0.
    let query = request(&snapshot, -10.);
    assert_eq!(
        snapshot.query(query, deadline()).unwrap().height,
        0f32.to_bits()
    );
}
#[test]
fn static_height_binds_every_input_and_rejects_stale_or_unavailable_geometry() {
    let (spec, files) = fixture();
    let products = products::load(spec.clone(), &files).unwrap();
    let snapshot = products.load_static_height().unwrap();
    let query = request(&snapshot, 10.);
    for i in 0..6 {
        let mut wrong = query;
        match i {
            0 => wrong.snapshot.map += 1,
            1 => wrong.snapshot.content[0] ^= 1,
            2 => wrong.snapshot.source_epoch[0] ^= 1,
            3 => wrong.snapshot.inventory[0] ^= 1,
            4 => wrong.snapshot.policy[0] ^= 1,
            _ => wrong.ticket = 0,
        }
        assert_eq!(snapshot.query(wrong, deadline()), Err(Error::Identity));
    }
    assert_eq!(snapshot.query(query, Instant::now()), Err(Error::Budget));
    let mut missing = query;
    missing.position = [0f32, 0., 10.].map(f32::to_bits);
    assert_eq!(snapshot.query(missing, deadline()), Err(Error::Missing));
    for bad in [f32::NAN, f32::INFINITY] {
        let mut invalid = query;
        invalid.max_search = bad.to_bits();
        assert_eq!(snapshot.query(invalid, deadline()), Err(Error::Format));
        invalid = query;
        invalid.position[2] = bad.to_bits();
        assert_eq!(snapshot.query(invalid, deadline()), Err(Error::Format));
    }
    let mut changed_spec = spec;
    let mut changed_files = files;
    products::mutate(
        &mut changed_spec,
        &mut changed_files,
        &ProductId::Terrain(TileId { x: 0, y: 0 }),
        |bytes| {
            bytes[56..60].copy_from_slice(&20f32.to_le_bytes());
            bytes[60..64].copy_from_slice(&20f32.to_le_bytes());
        },
    );
    let changed = products::load(changed_spec, &changed_files).unwrap();
    let new = changed.load_static_height().unwrap();
    assert_eq!(new.query(query, deadline()), Err(Error::Identity));
    assert_eq!(
        snapshot.query(query, deadline()).unwrap().height,
        9f32.to_bits()
    );
}
#[test]
fn derived_probes_outside_bounded_vmap_profile_never_publish_a_clamped_height() {
    let (spec, files) = fixture();
    let products = products::load(spec, &files).unwrap();
    let snapshot = products.load_static_height().unwrap();
    for z in [20_000f32, 99_999.] {
        assert_eq!(
            snapshot.query(request(&snapshot, z), deadline()),
            Err(Error::Unsupported)
        );
    }
    let mut query = request(&snapshot, 10.);
    query.max_search = 10_001f32.to_bits();
    assert_eq!(snapshot.query(query, deadline()), Err(Error::Unsupported));
    let mut calls = vec![];
    assert_eq!(
        select_height(0., 20_000., 50., |z, d| {
            calls.push((z, d));
            Err(Error::Unsupported)
        }),
        Err(Error::Unsupported)
    );
    assert_eq!(calls, vec![(20_002., 20_003.)]);
}
#[test]
fn incomplete_static_map_and_wmo_liquid_remain_unavailable() {
    let (mut spec, mut files) = fixture();
    for tile in spec.tiles.clone() {
        products::mutate(&mut spec, &mut files, &ProductId::VmapTile(tile), |b| {
            b.truncate(12);
            b[8..12].copy_from_slice(&0u32.to_le_bytes());
        });
        spec.products
            .iter_mut()
            .find(|p| p.id == ProductId::VmapTile(tile))
            .unwrap()
            .dependencies = vec![ProductId::VmapTree];
    }
    spec.products
        .retain(|p| !matches!(p.id, ProductId::VmapModel(_)));
    assert!(matches!(
        products::load(spec, &files).unwrap().load_static_height(),
        Err(Error::Missing)
    ));
    let (mut spec, mut files) = fixture();
    products::mutate(
        &mut spec,
        &mut files,
        &ProductId::VmapModel("ramp.wmo.vmo".into()),
        |b| {
            let offset = b.windows(4).position(|p| p == b"LIQU").unwrap();
            b[offset + 4..offset + 8].copy_from_slice(&1u32.to_le_bytes());
        },
    );
    assert!(products::load(spec, &files)
        .and_then(|p| p.load_static_height().map(|_| ()))
        .is_err());
}
#[test]
fn real_float_hole_is_invalid_map_height_not_a_missing_tile_or_dry_fact() {
    let (mut spec, mut files) = fixture();
    products::mutate(
        &mut spec,
        &mut files,
        &ProductId::Terrain(TileId { x: 0, y: 0 }),
        |b| *b = products::terrain_bytes(6),
    );
    let products = products::load(spec, &files).unwrap();
    let snapshot = products.load_static_height().unwrap();
    let mid = 0.5f32 * (64.0 * super::super::terrain::GRID_SIZE);
    let mut query = request(&snapshot, 10.);
    query.position = [mid - 1., mid - 1., 10.].map(f32::to_bits);
    assert_eq!(
        products
            .terrain(0, TileId { x: 0, y: 0 })
            .unwrap()
            .point_height(mid - 1., mid - 1.)
            .unwrap(),
        PointHeight::Hole
    );
    // The actual authored ramp supplies .5*x+.25*y=.75 despite raw terrain hole.
    assert_eq!(
        snapshot.query(query, deadline()).unwrap().height,
        0.75f32.to_bits()
    );
    query.position[2] = (-10f32).to_bits();
    // Invalid raw height suppresses the near-map fallback: zero would be wrong.
    assert_eq!(
        snapshot.query(query, deadline()).unwrap().height,
        (-200_000f32).to_bits()
    );
}
#[test]
fn actual_height_uses_the_long_fallback_after_short_search_misses() {
    let (mut spec, mut files) = fixture();
    products::mutate(
        &mut spec,
        &mut files,
        &ProductId::Terrain(TileId { x: 0, y: 0 }),
        |bytes| {
            bytes[56..60].copy_from_slice(&200f32.to_le_bytes());
            bytes[60..64].copy_from_slice(&200f32.to_le_bytes());
        },
    );
    let products = products::load(spec, &files).unwrap();
    let snapshot = products.load_static_height().unwrap();
    // Origin102/search50 misses the stack; the source10000 fallback reaches floor9.
    // Original z100 is below map200, so that actual VMap floor is selected.
    let query = request(&snapshot, 100.);
    assert_eq!(
        snapshot.query(query, deadline()).unwrap().height,
        9f32.to_bits()
    );
}

#[test]
fn actual_dry_location_floor_differs_from_source_static_height_below_ceiling() {
    let (mut spec, mut files) = terrain_coverage(vmap::location_fixture());
    products::mutate(
        &mut spec,
        &mut files,
        &ProductId::Terrain(TileId { x: 0, y: 0 }),
        |bytes| {
            bytes[56..60].copy_from_slice(&0f32.to_le_bytes());
            bytes[60..64].copy_from_slice(&0f32.to_le_bytes());
        },
    );
    let products = products::load(spec, &files).unwrap();
    let height = products.load_static_height().unwrap();
    let location = products.load_static_vmap().unwrap();
    let mid = 0.5f32 * (64.0 * super::super::terrain::GRID_SIZE);
    let position = [mid, mid, 5f32].map(f32::to_bits);
    let height_request = HeightRequest {
        snapshot: height.binding(),
        ticket: 1,
        position,
        max_search: 50f32.to_bits(),
    };
    let location_request = vmap::LocationRequest {
        snapshot: location.location_binding(),
        ticket: 1,
        position,
    };
    let selected_height = height.query(height_request, deadline()).unwrap();
    let selected_location = location
        .query_location(location_request, deadline())
        .unwrap();
    assert_eq!(selected_height.request, height_request);
    assert_eq!(selected_location.request, location_request);
    assert_eq!(selected_height.height, 6f32.to_bits());
    let vmap::StaticLocation::Floor(floor) = selected_location.result else {
        panic!("authored thick WMO must select its actual contained floor");
    };
    assert_eq!(floor.floor, 4f32.to_bits());
    // Original GetLiquidLevel writes this location floor even though its group is
    // dry. A later composite water query cannot substitute the z+2 static height.
    // This test establishes the two loaded spatial facts, not a water result.
    assert_ne!(floor.floor, selected_height.height);
}
