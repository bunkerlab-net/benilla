// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored native products, not installed geometry or participant admission.
use super::super::{height::tests::fixture, tests as products, ProductId, TileId};
use super::*;
use std::time::Duration;

pub(in super::super) fn config(products: &InventoriedProducts) -> SourceHeightConfiguration {
    SourceHeightConfiguration {
        map: products.map,
        content: products.content,
        source_epoch: products.source_epoch,
        effective_config: products.effective_config,
        vmap_height_enabled: true,
    }
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
fn position(z: f32) -> [u32; 3] {
    let mid = 0.5f32 * (64.0 * super::super::terrain::GRID_SIZE);
    [mid - 21.0, mid - 1.0, z].map(f32::to_bits)
}
fn request(snapshot: &MapHeightSnapshot<'_>, z: f32) -> MapHeightRequest {
    MapHeightRequest {
        snapshot: snapshot.binding(),
        ticket: 1,
        position: position(z),
        swim: false,
    }
}

#[test]
fn loaded_static_products_and_positive_empty_dynamic_proof_supply_source_map_height() {
    let (spec, files) = fixture();
    let products = products::load(spec, &files).unwrap();
    let proof = NonRenderParticipants::authored_fixture(products.content, products.map);
    let snapshot = products
        .load_map_height(config(&products), Some(&proof))
        .unwrap();
    for (z, expected) in [(10f32, 9f32), (6., 6.), (7., 7.), (8., 9.), (-10., 6.)] {
        for swim in [false, true] {
            let query = MapHeightRequest {
                swim,
                ..request(&snapshot, z)
            };
            assert_eq!(
                snapshot.query(query, deadline()).unwrap(),
                MapHeightReply {
                    request: query,
                    height: expected.to_bits(),
                }
            );
        }
    }
}

#[test]
fn effective_config_and_independent_participant_proof_cannot_be_invented_from_geometry() {
    let (spec, files) = fixture();
    let products = products::load(spec, &files).unwrap();
    let proof = NonRenderParticipants::authored_fixture(products.content, products.map);
    assert!(matches!(
        products.load_map_height(config(&products), None),
        Err(Error::Profile)
    ));
    for wrong in [
        NonRenderParticipants::authored_fixture([8; 32], products.map),
        NonRenderParticipants::authored_fixture(products.content, products.map + 1),
    ] {
        assert!(matches!(
            products.load_map_height(config(&products), Some(&wrong)),
            Err(Error::Profile)
        ));
    }
    for field in 0..5 {
        let mut config = config(&products);
        match field {
            0 => config.map += 1,
            1 => config.content[0] ^= 1,
            2 => config.source_epoch[0] ^= 1,
            3 => config.effective_config[0] ^= 1,
            _ => config.vmap_height_enabled = false,
        }
        assert!(products.load_map_height(config, Some(&proof)).is_err());
    }
}

#[test]
fn exact_identity_coverage_budget_and_existing_numeric_bounds_precede_publication() {
    let (spec, files) = fixture();
    let products = products::load(spec, &files).unwrap();
    let proof = NonRenderParticipants::authored_fixture(products.content, products.map);
    let snapshot = products
        .load_map_height(config(&products), Some(&proof))
        .unwrap();
    let query = request(&snapshot, 10.);
    for field in 0..6 {
        let mut bad = query;
        match field {
            0 => bad.snapshot.map += 1,
            1 => bad.snapshot.content[0] ^= 1,
            2 => bad.snapshot.source_epoch[0] ^= 1,
            3 => bad.snapshot.inventory[0] ^= 1,
            4 => bad.snapshot.policy[0] ^= 1,
            _ => bad.ticket = 0,
        }
        assert_eq!(snapshot.query(bad, deadline()), Err(Error::Identity));
    }
    assert_eq!(snapshot.query(query, Instant::now()), Err(Error::Budget));
    for z in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            snapshot.query(request(&snapshot, z), deadline()),
            Err(Error::Format)
        );
        for axis in 0..2 {
            let mut bad = query;
            bad.position[axis] = z.to_bits();
            assert_eq!(snapshot.query(bad, deadline()), Err(Error::Format));
        }
    }
    for z in [20_000., 99_999., f32::MAX] {
        assert_eq!(
            snapshot.query(request(&snapshot, z), deadline()),
            Err(Error::Unsupported)
        );
    }
    let missing = MapHeightRequest {
        position: [0f32, 0., 10.].map(f32::to_bits),
        ..query
    };
    assert_eq!(snapshot.query(missing, deadline()), Err(Error::Missing));
}

#[test]
fn source_hole_remains_raw_invalid_and_allowed_z_preserves_original_bits() {
    let (mut spec, mut files) = fixture();
    products::mutate(
        &mut spec,
        &mut files,
        &ProductId::Terrain(TileId { x: 0, y: 0 }),
        |b| *b = products::terrain_bytes(6),
    );
    let products = products::load(spec, &files).unwrap();
    let proof = NonRenderParticipants::authored_fixture(products.content, products.map);
    let snapshot = products
        .load_map_height(config(&products), Some(&proof))
        .unwrap();
    let mid = 0.5f32 * (64.0 * super::super::terrain::GRID_SIZE);
    let query = MapHeightRequest {
        position: [mid - 1., mid - 1., -10.].map(f32::to_bits),
        ..request(&snapshot, -10.)
    };
    let height = snapshot.query(query, deadline()).unwrap().height;
    assert_eq!(height, (-200_000f32).to_bits());
    assert_eq!(
        allowed_ground_z(-10., f32::from_bits(height)).to_bits(),
        (-10f32).to_bits()
    );
    for z in [0f32, -0f32] {
        for height in [0f32, -0f32, -100_000., -200_000.] {
            assert_eq!(allowed_ground_z(z, height).to_bits(), z.to_bits());
        }
    }
}

#[test]
fn changing_effective_config_or_product_bytes_cannot_relabel_a_frozen_answer() {
    let (spec, files) = fixture();
    let original = products::load(spec.clone(), &files).unwrap();
    let proof = NonRenderParticipants::authored_fixture(original.content, original.map);
    let snapshot = original
        .load_map_height(config(&original), Some(&proof))
        .unwrap();
    let query = request(&snapshot, 10.);
    for change in 0..2 {
        let mut spec = spec.clone();
        let mut files = files.clone();
        if change == 0 {
            spec.binding.effective_config[0] ^= 1;
        } else {
            products::mutate(
                &mut spec,
                &mut files,
                &ProductId::Terrain(TileId { x: 0, y: 0 }),
                |b| {
                    b[56..60].copy_from_slice(&20f32.to_le_bytes());
                    b[60..64].copy_from_slice(&20f32.to_le_bytes());
                },
            );
        }
        let changed = products::load(spec, &files).unwrap();
        let newer = changed
            .load_map_height(config(&changed), Some(&proof))
            .unwrap();
        assert_eq!(newer.query(query, deadline()), Err(Error::Identity));
        assert_eq!(
            snapshot.query(query, deadline()).unwrap().height,
            9f32.to_bits()
        );
    }
}

#[test]
fn overflow_and_callback_failure_never_publish_partial_height() {
    let mut dynamic_calls = 0;
    assert_eq!(
        map_height(
            [0., 0., f32::MAX],
            false,
            |_, _, _| Ok(-f32::MAX),
            |_, _| {
                dynamic_calls += 1;
                Ok(f32::NEG_INFINITY)
            }
        ),
        Err(Error::Unsupported)
    );
    assert_eq!(dynamic_calls, 0);
    assert_eq!(
        map_height(
            [0., 0., 0.],
            false,
            |_, _, _| Err(Error::Missing),
            |_, _| panic!("static failure must stop the query")
        ),
        Err(Error::Missing)
    );
    assert_eq!(
        map_height(
            [0., 0., 0.],
            false,
            |_, _, _| Ok(7.),
            |_, _| Err(Error::Budget)
        ),
        Err(Error::Budget)
    );
}
