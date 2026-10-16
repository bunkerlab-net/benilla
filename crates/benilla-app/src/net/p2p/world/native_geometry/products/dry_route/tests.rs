// SPDX-License-Identifier: GPL-3.0-or-later
// Authored native products and original source-oracle model triangles, no install data.
use super::super::{
    liquid::{LiquidRequest, LiquidStatus},
    navigation::{self, RouteRequest},
    terrain_wet_liquid::TerrainLiquidSnapshot,
    tests as product_tests,
    vmap::{RayRequest, StaticRay},
    InventorySpec, ProductId, TileId,
};
use super::*;
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

fn word(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}
fn floats(bytes: &mut Vec<u8>, values: impl IntoIterator<Item = f32>) {
    for value in values {
        word(bytes, value.to_bits());
    }
}

/// Flat terrain/ground nav plus actual source-fixture wall and M2 triangles.
/// Wall: x=-400, y=-404..-396, z=0..8. M2 ramp: centered at (-420,-400,2).
/// The original model geometry is unchanged; only authored placements/tree differ.
pub(crate) fn fixture() -> (InventorySpec, BTreeMap<String, Vec<u8>>) {
    let (mut spec, mut files) = navigation::tests::fixture();
    let tile = TileId { x: 32, y: 32 };
    spec.tiles = vec![tile];
    spec.products.retain(|product| match &product.id {
        ProductId::Terrain(t) | ProductId::VmapTile(t) | ProductId::MmapTile(t) => *t == tile,
        ProductId::VmapModel(name) => name == "wall.wmo.vmo" || name == "ramp.m2.vmo",
        _ => true,
    });
    const MID: f32 = 0.5f32 * 64.0f32 * 533.333_3_f32;
    let mut tree = b"VMAP_7.0\x01NODE".to_vec();
    floats(
        &mut tree,
        [MID + 400., MID + 396., 0., MID + 422., MID + 404., 8.],
    );
    for value in [3, 3 << 30, 2, 0, 2, 0, 1] {
        word(&mut tree, value);
    }
    tree.extend_from_slice(b"GOBJ");
    product_tests::mutate(&mut spec, &mut files, &ProductId::VmapTree, |data| {
        *data = tree
    });
    let mut spawns = b"VMAP_7.0".to_vec();
    word(&mut spawns, 2);
    for (index, name, flags, position, scale, low, high) in [
        (
            0,
            "wall.wmo",
            4,
            [MID + 400., MID + 400., 0.],
            1.,
            [0., -4., 0.],
            [0., 4., 8.],
        ),
        (
            1,
            "ramp.m2",
            5,
            [MID + 420., MID + 400., 2.],
            0.5,
            [-2., -2., -1.5],
            [2., 2., 1.5],
        ),
    ] {
        word(&mut spawns, flags);
        spawns.extend_from_slice(&7u16.to_le_bytes());
        word(&mut spawns, 101 + index);
        floats(&mut spawns, position);
        floats(&mut spawns, [0.; 3]);
        floats(&mut spawns, [scale]);
        floats(&mut spawns, (0..3).map(|i| position[i] + low[i]));
        floats(&mut spawns, (0..3).map(|i| position[i] + high[i]));
        word(&mut spawns, name.len() as u32);
        spawns.extend_from_slice(name.as_bytes());
        word(&mut spawns, index);
    }
    let id = ProductId::VmapTile(tile);
    product_tests::mutate(&mut spec, &mut files, &id, |data| *data = spawns);
    spec.products
        .iter_mut()
        .find(|p| p.id == id)
        .unwrap()
        .dependencies = vec![
        ProductId::VmapTree,
        ProductId::VmapModel("wall.wmo.vmo".into()),
        ProductId::VmapModel("ramp.m2.vmo".into()),
    ];
    files.retain(|name, _| {
        spec.products
            .iter()
            .any(|p| p.id.filename(spec.map).unwrap() == *name)
    });
    (spec, files)
}

pub(crate) fn load(
    spec: InventorySpec,
    files: &BTreeMap<String, Vec<u8>>,
) -> Arc<VmapLiquidProducts> {
    let stage = product_tests::Stage::new(files);
    Arc::new(VmapLiquidProducts::load(&stage.0, spec).unwrap())
}
pub(crate) fn products() -> Arc<VmapLiquidProducts> {
    let (spec, files) = fixture();
    load(spec, &files)
}
pub(crate) fn snake_products() -> Arc<VmapLiquidProducts> {
    let (mut spec, mut files) = fixture();
    let tile = spec.tiles[0];
    for (id, bytes) in [
        (ProductId::MmapParams, include_bytes!(
            "../../../../../../../../benilla-native-nav/tools/fixtures/smoothing/params.test-fixture"
        ).as_slice()),
        (ProductId::MmapTile(tile), include_bytes!(
            "../../../../../../../../benilla-native-nav/tools/fixtures/smoothing/ground.test-fixture"
        ).as_slice()),
    ] {
        product_tests::mutate(&mut spec, &mut files, &id, |data| *data = bytes.to_vec());
    }
    load(spec, &files)
}
fn empty_dbc(fields: u32) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for value in [0, fields, fields * 4, 1] {
        word(&mut bytes, value);
    }
    bytes.push(0);
    bytes
}
pub(crate) fn environment(products: Arc<VmapLiquidProducts>) -> TerrainLiquidSnapshot<'static> {
    products
        .load_terrain_wet_liquid_owned(
            &empty_dbc(4),
            &empty_dbc(25),
            &empty_dbc(20),
            &empty_dbc(42),
        )
        .unwrap()
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

#[test]
fn actual_dry_products_share_original_environment_liquid_vmap_and_nav_identity() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let products = products();
    let environment = environment(Arc::clone(&products));
    let (liquid, navigation, vmap) = products.load_dry_route_projection().unwrap().into_parts();
    let source = environment.binding();
    for (map, content, epoch, inventory) in [
        (
            liquid.binding().map,
            liquid.binding().content,
            liquid.binding().source_epoch,
            liquid.binding().inventory,
        ),
        (
            navigation.binding().map,
            navigation.binding().content,
            navigation.binding().source_epoch,
            navigation.binding().inventory,
        ),
        (
            vmap.binding().map,
            vmap.binding().content,
            vmap.binding().source_epoch,
            vmap.binding().inventory,
        ),
    ] {
        assert_eq!(
            (map, content, epoch, inventory),
            (
                source.map,
                source.content,
                source.source_epoch,
                source.inventory
            )
        );
    }
    assert_eq!(source.inventory, products.inventory.revision());
    let pose = [-400f32, -430., 0.].map(f32::to_bits);
    let dry_request = LiquidRequest {
        snapshot: liquid.binding(),
        ticket: 1,
        position: pose,
    };
    assert_eq!(
        liquid.query(dry_request, deadline()).unwrap().status,
        LiquidStatus::NoWater
    );
    let wet_request = benilla_world_runtime::native::environment::EnvironmentQuery {
        snapshot: environment.environment_binding(),
        ticket: 1,
        position: pose,
        required_type: 0x0f,
        collision_height: 2f32.to_bits(),
    };
    let reply = environment
        .query_environment(wet_request, deadline())
        .unwrap();
    assert_eq!(
        reply.status,
        benilla_world_runtime::native::environment::LiquidStatus::NoWater
    );
    assert_eq!(reply.data, None);
    let request = RouteRequest {
        snapshot: navigation.binding(),
        ticket: 1,
        from: pose,
        to: [-400f32, -420., 0.].map(f32::to_bits),
    };
    let result = navigation.query(request, deadline()).unwrap();
    assert_eq!(result.request, request);
    assert!(!result.route.corners.is_empty());
    // All published readers own their products/decoded dependencies after source removal.
    drop(products);
    assert_eq!(
        liquid.query(dry_request, deadline()).unwrap().status,
        LiquidStatus::NoWater
    );
    assert_eq!(
        environment
            .query_environment(wet_request, deadline())
            .unwrap(),
        reply
    );
}

#[test]
fn projected_static_los_uses_real_wall_and_m2_triangles_with_fail_closed_queries() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let (_, _, vmap) = products().load_dry_route_projection().unwrap().into_parts();
    let mut request = RayRequest {
        snapshot: vmap.binding(),
        ticket: 1,
        from: [-395f32, -400., 4.].map(f32::to_bits),
        to: [-405f32, -400., 4.].map(f32::to_bits),
        ignore_m2: false,
    };
    assert!(matches!(
        vmap.query(request, deadline()).unwrap().result,
        StaticRay::Hit { .. }
    ));
    request.from[2] = 10f32.to_bits();
    request.to[2] = 10f32.to_bits();
    assert_eq!(
        vmap.query(request, deadline()).unwrap().result,
        StaticRay::Miss
    );
    request.from = [-420f32, -400., 10.].map(f32::to_bits);
    request.to = [-420f32, -400., -5.].map(f32::to_bits);
    assert!(matches!(
        vmap.query(request, deadline()).unwrap().result,
        StaticRay::Hit { .. }
    ));
    request.ignore_m2 = true;
    assert_eq!(
        vmap.query(request, deadline()).unwrap().result,
        StaticRay::Miss
    );
    request.ignore_m2 = false;
    assert_eq!(vmap.query(request, Instant::now()), Err(Error::Budget));
    request.snapshot.inventory[0] ^= 1;
    assert_eq!(vmap.query(request, deadline()), Err(Error::Identity));
}

#[test]
fn ordinary_profile_restrictions_and_cross_inventory_proof_rejection_are_preserved() {
    let (spec, files) = fixture();
    let ordinary = product_tests::load(spec.clone(), &files).unwrap();
    let products = load(spec.clone(), &files);
    assert_ne!(ordinary.revision(), products.inventory.revision());
    assert!(matches!(
        products.inventory.load_navigation(),
        Err(Error::Unsupported)
    ));
    let proof = ordinary.load_dry_liquid().unwrap();
    assert!(matches!(
        products.inventory.load_dry_route_navigation(&proof),
        Err(Error::Identity)
    ));
    assert!(matches!(
        ordinary.load_dry_route_navigation(&proof),
        Err(Error::Unsupported)
    ));
    let proof = products.inventory.load_dry_liquid().unwrap();
    for field in 0..4 {
        let mut other = spec.clone();
        match field {
            0 => other.binding.content[0] ^= 1,
            1 => other.binding.source_epoch[0] ^= 1,
            2 => other.binding.effective_config[0] ^= 1,
            _ => other.binding.raw_static_inventory[0] ^= 1,
        }
        assert!(matches!(
            load(other, &files)
                .inventory
                .load_dry_route_navigation(&proof),
            Err(Error::Identity)
        ));
    }
    if benilla_native_nav::SUPPORTED_ABI {
        let (_, projected, _) = products.load_dry_route_projection().unwrap().into_parts();
        let conservative = ordinary.load_navigation().unwrap();
        assert_ne!(projected.binding().policy, conservative.binding().policy);
        let mut expected = sha2::Sha256::new();
        use sha2::Digest;
        expected.update(b"benilla-pinned-detour-complete-dry-liquid-projection-v1");
        expected.update(products.inventory.revision());
        expected.update(super::super::SOURCE_PIN);
        expected.update(super::super::DETOUR_TREE);
        expected.update(proof.binding().policy);
        assert_eq!(
            projected.binding().policy,
            <[u8; 32]>::from(expected.finalize())
        );
    }
}

#[test]
fn actual_wet_terrain_or_wmo_payload_cannot_enter_dry_projection() {
    let (mut spec, mut files) = fixture();
    let tile = spec.tiles[0];
    product_tests::mutate(&mut spec, &mut files, &ProductId::Terrain(tile), |bytes| {
        let at = u32::from_le_bytes(bytes[32..36].try_into().unwrap()) as usize;
        let mut liquid = b"MLIQ".to_vec();
        liquid.extend_from_slice(&[3, 8, 1, 0, 0, 0, 129, 129]);
        liquid.extend_from_slice(&10f32.to_le_bytes());
        bytes.splice(at..at, liquid);
        bytes[24..28].copy_from_slice(&(at as u32).to_le_bytes());
        bytes[28..32].copy_from_slice(&16u32.to_le_bytes());
        bytes[32..36].copy_from_slice(&((at + 16) as u32).to_le_bytes());
    });
    let wet = load(spec.clone(), &files);
    assert!(wet.inventory.terrain(spec.map, tile).unwrap().has_liquid());
    assert!(matches!(
        wet.load_dry_route_projection(),
        Err(Error::Unsupported)
    ));
    assert!(matches!(
        wet.inventory.load_navigation(),
        Err(Error::Unsupported)
    ));
    assert!(matches!(
        product_tests::load(spec, &files).unwrap().load_navigation(),
        Err(Error::Unsupported)
    ));

    let (mut spec, mut files) = fixture();
    product_tests::mutate(
        &mut spec,
        &mut files,
        &ProductId::VmapModel("wall.wmo.vmo".into()),
        |bytes| {
            let at = bytes.windows(4).position(|word| word == b"LIQU").unwrap() + 4;
            let mut payload = vec![];
            for value in [
                1,
                1,
                0,
                0,
                0,
                1,
                10f32.to_bits(),
                10f32.to_bits(),
                10f32.to_bits(),
                10f32.to_bits(),
            ] {
                word(&mut payload, value);
            }
            payload.push(0);
            bytes[at..at + 4].copy_from_slice(&((payload.len() - 4) as u32).to_le_bytes());
            bytes.splice(at + 4..at + 4, payload);
        },
    );
    let wet = load(spec.clone(), &files);
    assert!(wet.load_vmap_liquid().is_ok());
    assert!(matches!(
        wet.load_dry_route_projection(),
        Err(Error::Unsupported)
    ));
    assert!(matches!(
        wet.inventory.load_navigation(),
        Err(Error::Unsupported)
    ));
    assert!(matches!(
        product_tests::load(spec, &files),
        Err(Error::Unsupported)
    ));
}

#[test]
fn unresolved_actual_tree_slots_cannot_be_promoted_by_a_clear_ray() {
    let (mut spec, mut files) = fixture();
    product_tests::mutate(&mut spec, &mut files, &ProductId::VmapTree, |bytes| {
        // A new real tree slot is never supplied by any actual spawn.
        bytes[45..49].copy_from_slice(&3u32.to_le_bytes());
        bytes[53..57].copy_from_slice(&3u32.to_le_bytes());
        bytes.splice(65..65, 2u32.to_le_bytes());
    });
    let products = load(spec, &files);
    assert!(products.inventory.load_static_vmap().is_ok());
    assert!(matches!(
        products.load_dry_route_projection(),
        Err(Error::Missing)
    ));
}
