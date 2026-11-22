// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual authored product bytes and source call receipts; no wet admission.
use super::super::super::{height::tests::fixture, tests as products, ProductId, TileId};
use super::super::tests::{config, deadline, position};
use super::*;

fn request(snapshot: &MapHeightSnapshot<'_>, z: f32, height: f32) -> SwimmingAllowedZRequest {
    SwimmingAllowedZRequest {
        height: MapHeightRequest {
            snapshot: snapshot.binding(),
            ticket: 7,
            position: position(z),
            swim: true,
        },
        collision_height: height.to_bits(),
    }
}

#[test]
fn swimming_branch_queries_real_dry_bytes_at_ground_with_separate_model_height() {
    let (spec, files) = fixture();
    let products = products::load(spec, &files).unwrap();
    let proof = NonRenderParticipants::authored_fixture(products.content, products.map);
    let snapshot = products
        .load_map_height(config(&products), Some(&proof))
        .unwrap();
    for height in [0.5, 2.03128, 3.75] {
        for (z, expected) in [(10f32, 9f32), (6., 6.), (7., 7.), (8., 9.), (-10., 6.)] {
            let query = request(&snapshot, z, height);
            let reply = snapshot
                .query_swimming_allowed_z_with_budget(
                    query,
                    &mut vmap::QueryBudget::new(deadline()),
                )
                .unwrap();
            assert_eq!(reply.request, query);
            assert_eq!(reply.ground.request, query.height);
            assert!(reply.ground.request.swim);
            let water = reply.water_or_ground;
            assert_eq!(water.liquid.request.snapshot, snapshot.dry_liquid_binding());
            assert_eq!(water.liquid.request.ticket, query.height.ticket);
            assert_eq!(
                water.liquid.request.position,
                [
                    query.height.position[0],
                    query.height.position[1],
                    reply.ground.height
                ]
            );
            assert_eq!(water.liquid.status, LiquidStatus::NoWater);
            assert_eq!(water.classification_height, 2.03128f32.to_bits());
            assert_eq!(water.min_water_depth, height.to_bits());
            assert_eq!(water.required_type, 15);
            assert!(water.swim);
            assert_eq!(reply.ground.height, expected.to_bits());
            assert_eq!(water.max_z, reply.ground.height);
            assert_eq!(reply.allowed_z, reply.ground.height);
        }
    }
}

#[test]
fn source_invalid_ground_still_queries_liquid_and_preserves_z_bits() {
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
    let mid = 0.5f32 * (64.0 * super::super::super::terrain::GRID_SIZE);
    for z in [-10., -0., 0.] {
        let mut query = request(&snapshot, z, 3.75);
        query.height.position = [mid - 5., mid - 1., z].map(f32::to_bits);
        let reply = snapshot
            .query_swimming_allowed_z_with_budget(query, &mut vmap::QueryBudget::new(deadline()))
            .unwrap();
        assert_eq!(reply.ground.height, (-200000f32).to_bits());
        assert_eq!(
            reply.water_or_ground.liquid.request.position[2],
            (-200000f32).to_bits()
        );
        assert_eq!(reply.water_or_ground.liquid.status, LiquidStatus::NoWater);
        assert_eq!(reply.allowed_z, z.to_bits());
    }
}

#[test]
fn swimming_query_rejects_changed_binding_ground_branch_missing_coverage_and_deadlines() {
    let (spec, files) = fixture();
    let products = products::load(spec, &files).unwrap();
    let proof = NonRenderParticipants::authored_fixture(products.content, products.map);
    let snapshot = products
        .load_map_height(config(&products), Some(&proof))
        .unwrap();
    let query = request(&snapshot, 10., 3.75);
    for field in 0..7 {
        let mut changed = query;
        match field {
            0 => changed.height.snapshot.map += 1,
            1 => changed.height.snapshot.content[0] ^= 1,
            2 => changed.height.snapshot.source_epoch[0] ^= 1,
            3 => changed.height.snapshot.inventory[0] ^= 1,
            4 => changed.height.snapshot.policy[0] ^= 1,
            5 => changed.height.ticket = 0,
            _ => changed.height.swim = false,
        }
        assert_eq!(
            snapshot.query_swimming_allowed_z_with_budget(
                changed,
                &mut vmap::QueryBudget::new(deadline())
            ),
            Err(Error::Identity)
        );
    }
    assert_eq!(
        snapshot.query_swimming_allowed_z_with_budget(
            query,
            &mut vmap::QueryBudget::new(Instant::now())
        ),
        Err(Error::Deadline)
    );
    let mut changed = query;
    changed.height.position = [0f32, 0., 10.].map(f32::to_bits);
    assert_eq!(
        snapshot
            .query_swimming_allowed_z_with_budget(changed, &mut vmap::QueryBudget::new(deadline())),
        Err(Error::Missing)
    );
    for height in [
        -1.,
        -0.,
        0.,
        101.,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ] {
        changed = query;
        changed.collision_height = height.to_bits();
        assert_eq!(
            snapshot.query_swimming_allowed_z_with_budget(
                changed,
                &mut vmap::QueryBudget::new(deadline())
            ),
            Err(Error::Unsupported)
        );
    }
}

#[test]
fn exact_invalid_ground_is_the_only_extension_to_normal_dry_query_bounds() {
    let (spec, files) = fixture();
    let products = products::load(spec, &files).unwrap();
    let dry = products.load_dry_liquid().unwrap();
    let request = LiquidRequest {
        snapshot: dry.binding(),
        ticket: 9,
        position: position(-200000.),
    };
    assert_eq!(
        dry.query_at_ground(request, deadline()).unwrap().status,
        LiquidStatus::NoWater
    );
    assert_eq!(dry.query(request, deadline()), Err(Error::Format));
    for z in [
        f32::from_bits((-200000f32).to_bits() - 1),
        f32::from_bits((-200000f32).to_bits() + 1),
        -100001.,
        100001.,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ] {
        let changed = LiquidRequest {
            position: position(z),
            ..request
        };
        assert_eq!(dry.query_at_ground(changed, deadline()), Err(Error::Format));
    }
    assert_eq!(
        dry.query_at_ground(request, Instant::now()),
        Err(Error::Deadline)
    );
    let mut changed = request;
    changed.snapshot.source_epoch[0] ^= 1;
    assert_eq!(
        dry.query_at_ground(changed, deadline()),
        Err(Error::Identity)
    );
    changed = request;
    changed.position[0] = 0;
    changed.position[1] = 0;
    assert_eq!(
        dry.query_at_ground(changed, deadline()),
        Err(Error::Missing)
    );
}

#[test]
fn pinned_swimming_source_matches_calls_default_height_and_strict_clamps() {
    let mut rows = 0;
    let mut no_water = 0;
    let mut nonzero = std::collections::BTreeSet::new();
    for line in include_str!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/swimming-allowed-z.test-fixture")
        .lines().filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 17, "{line}");
        let bits = |i| u32::from_str_radix(fields[i], 16).unwrap();
        let number = |i: usize| fields[i].parse::<u32>().unwrap();
        let value = |i| f32::from_bits(bits(i));
        let z = value(1);
        let ground = map_height([3., 7., z], true, |p, enabled, search| {
            assert_eq!(p.map(f32::to_bits), [3f32.to_bits(), 7f32.to_bits(), bits(1)], "{line}");
            assert!(enabled);
            assert_eq!(search.to_bits(), bits(8), "{line}");
            assert_eq!(search, 50.);
            Ok(value(2))
        }, |p, range| {
            assert_eq!(p.map(f32::to_bits), [3f32.to_bits(), 7f32.to_bits(), bits(9)], "{line}");
            assert_eq!(range.to_bits(), bits(10), "{line}");
            Ok(f32::NEG_INFINITY)
        }).unwrap();
        assert_eq!(ground.to_bits(), bits(2), "{line}");
        let can_check = number(7) == 1;
        assert_eq!(number(11), u32::from(can_check), "{line}");
        if can_check {
            assert_eq!(bits(12), ground.to_bits(), "liquid must query groundZ: {line}");
            assert_eq!(number(13), u32::from(ALL_LIQUIDS), "{line}");
            assert_eq!(bits(14), DEFAULT_CLASSIFICATION_HEIGHT.to_bits(), "{line}");
        }
        let liquid = if number(4) == 0 {
            no_water += 1;
            None
        } else {
            nonzero.insert(number(4));
            Some(value(3))
        };
        let max_z = water_or_ground_level(can_check, ground, number(6) == 0, value(5), liquid);
        assert_eq!(max_z.to_bits(), bits(15), "{line}");
        assert_eq!(allowed_position_z(z, ground, max_z).to_bits(), bits(16), "{line}");
        rows += 1;
    }
    assert_eq!(rows, 865);
    assert_eq!(no_water, 265);
    assert_eq!(nonzero, [1, 2, 4, 8, 15].into_iter().collect());
}

#[test]
fn equal_dry_result_cannot_skip_liquid_when_the_shared_budget_expires_after_height() {
    let (spec, files) = fixture();
    let products = products::load(spec, &files).unwrap();
    let proof = NonRenderParticipants::authored_fixture(products.content, products.map);
    let snapshot = products
        .load_map_height(config(&products), Some(&proof))
        .unwrap();
    let query = request(&snapshot, 7., 3.75);
    let mut measured = vmap::QueryBudget::new(deadline());
    let initial = measured.remaining_work_for_test();
    let ground = snapshot
        .query_with_budget(query.height, &mut measured)
        .unwrap();
    assert_eq!(ground.height, query.height.position[2]);
    let height_work = initial - measured.remaining_work_for_test();
    let mut only_height = vmap::QueryBudget::new(deadline());
    only_height.restrict_work_for_test(height_work);
    assert_eq!(
        snapshot.query_swimming_allowed_z_with_budget(query, &mut only_height),
        Err(Error::Budget)
    );
    let mut including_liquid = vmap::QueryBudget::new(deadline());
    including_liquid.restrict_work_for_test(height_work + 1);
    assert_eq!(
        snapshot
            .query_swimming_allowed_z_with_budget(query, &mut including_liquid)
            .unwrap()
            .allowed_z,
        query.height.position[2]
    );
}

#[test]
fn valid_wet_gridmap_does_not_become_a_dry_allowed_z_inventory() {
    let (mut spec, mut files) = fixture();
    products::mutate(
        &mut spec,
        &mut files,
        &ProductId::Terrain(TileId { x: 0, y: 0 }),
        |bytes| {
            let at = u32::from_le_bytes(bytes[32..36].try_into().unwrap()) as usize;
            let mut liquid = b"MLIQ".to_vec();
            liquid.extend_from_slice(&[3, 8]);
            liquid.extend_from_slice(&21u16.to_le_bytes());
            liquid.extend_from_slice(&[0, 0, 129, 129]);
            liquid.extend_from_slice(&9f32.to_le_bytes());
            bytes.splice(at..at, liquid);
            bytes[24..28].copy_from_slice(&(at as u32).to_le_bytes());
            bytes[28..32].copy_from_slice(&16u32.to_le_bytes());
            bytes[32..36].copy_from_slice(&((at + 16) as u32).to_le_bytes());
        },
    );
    let products = Arc::new(products::load(spec, &files).unwrap());
    let proof = NonRenderParticipants::authored_fixture(products.content, products.map);
    assert!(products.load_static_height().is_ok());
    assert!(matches!(
        products.load_map_height(config(&products), Some(&proof)),
        Err(Error::Unsupported)
    ));
    assert!(matches!(
        Arc::clone(&products).load_map_height_owned(config(&products), Some(&proof)),
        Err(Error::Unsupported)
    ));
}
