// SPDX-License-Identifier: GPL-3.0-or-later
// Authored products only. Exact original CMaNGOS/G3D C++ output, no game assets.
use super::super::{tests as product_tests, InventorySpec, ProductSpec};
use super::*;
use std::time::Duration;

const TREE: &[u8] = include_bytes!(
    "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/000.vmtree"
);
const TILE:&[u8]=include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/000_32_32.vmtile");
const MODELS:[(&str,&[u8]);4]=[
    ("ramp.wmo.vmo",include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/ramp.wmo.vmo")),
    ("stack.wmo.vmo",include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/stack.wmo.vmo")),
    ("wall.wmo.vmo",include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/wall.wmo.vmo")),
    ("ramp.m2.vmo",include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/ramp.m2.vmo")),
];
pub(crate) fn fixture() -> (InventorySpec, BTreeMap<String, Vec<u8>>) {
    let (mut spec, mut files) = product_tests::fixture();
    spec.products
        .retain(|p| !matches!(p.id, ProductId::VmapModel(_)));
    files.remove("vmaps/authored.m2.vmo");
    product_tests::mutate(&mut spec, &mut files, &ProductId::VmapTree, |b| {
        *b = TREE.to_vec()
    });
    let dependencies: Vec<_> = std::iter::once(ProductId::VmapTree)
        .chain(
            MODELS
                .iter()
                .map(|(name, _)| ProductId::VmapModel((*name).into())),
        )
        .collect();
    for tile in spec.tiles.clone() {
        let id = ProductId::VmapTile(tile);
        product_tests::mutate(&mut spec, &mut files, &id, |b| *b = TILE.to_vec());
        spec.products
            .iter_mut()
            .find(|p| p.id == id)
            .unwrap()
            .dependencies = dependencies.clone();
    }
    for (name, bytes) in MODELS {
        let id = ProductId::VmapModel(name.into());
        files.insert(id.filename(0).unwrap(), bytes.to_vec());
        spec.products.push(ProductSpec {
            id,
            bytes: bytes.len() as u64,
            sha256: Sha256::digest(bytes).into(),
            dependencies: vec![],
        });
    }
    (spec, files)
}
pub(crate) fn snapshot() -> VmapSnapshot {
    let (spec, files) = fixture();
    product_tests::load(spec, &files)
        .unwrap()
        .load_static_vmap()
        .unwrap()
}
fn budget() -> QueryBudget {
    QueryBudget {
        remaining: MAX_QUERY_WORK,
        deadline: Instant::now() + Duration::from_secs(10),
    }
}
fn f(s: &str) -> f32 {
    f32::from_bits(u32::from_str_radix(s, 16).unwrap())
}
#[test]
fn exact_pinned_cpp_rays_hit_loaded_authored_geometry() {
    let snapshot = snapshot();
    let mut count = 0;
    for line in include_str!(
        "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/rays.tsv"
    )
    .lines()
    {
        if line.starts_with('#') {
            continue;
        }
        let row: Vec<_> = line.split('\t').collect();
        let ray = Ray {
            origin: [f(row[1]), f(row[2]), f(row[3])],
            direction: [f(row[4]), f(row[5]), f(row[6])],
        };
        let mut distance = f(row[7]);
        let actual = snapshot
            .intersect_internal(ray, &mut distance, row[8] == "1", &mut budget())
            .unwrap();
        let expected = if row[9] == "1" {
            StaticRay::Hit {
                distance: f(row[10]).to_bits(),
            }
        } else {
            StaticRay::Miss
        };
        assert_eq!(actual, expected, "{}", row[0]);
        assert_eq!(
            distance.to_bits(),
            f(row[10]).to_bits(),
            "{} distance",
            row[0]
        );
        count += 1;
    }
    assert!(count >= 2508);
}
#[test]
fn source_bound_queries_reject_stale_epoch_content_revision_and_deadline() {
    let snapshot = snapshot();
    let from = to_internal([0.0, 0.0, 15.0]);
    let to = to_internal([0.0, 0.0, -5.0]);
    let query = RayRequest {
        snapshot: snapshot.binding(),
        ticket: 1,
        from: from.map(f32::to_bits),
        to: to.map(f32::to_bits),
        ignore_m2: false,
    };
    let answer = snapshot.query(query, budget().deadline).unwrap();
    assert_eq!(answer.request, query);
    assert!(matches!(answer.result, StaticRay::Hit { .. }));
    for field in 0..6 {
        let mut q = query;
        match field {
            0 => q.snapshot.map += 1,
            1 => q.snapshot.content = [0; 32],
            2 => q.snapshot.source_epoch = [0; 32],
            3 => q.snapshot.inventory = [0; 32],
            4 => q.snapshot.policy = [0; 32],
            _ => q.ticket = 0,
        }
        assert_eq!(snapshot.query(q, budget().deadline), Err(Error::Identity));
    }
    assert_eq!(snapshot.query(query, Instant::now()), Err(Error::Budget));
    let mut bad = query;
    bad.from[0] = f32::NAN.to_bits();
    assert_eq!(snapshot.query(bad, budget().deadline), Err(Error::Format));
    let mut distance = 20.0;
    assert_eq!(
        snapshot.intersect_internal(
            Ray {
                origin: [0., 0., 15.],
                direction: [0., 0., -1.]
            },
            &mut distance,
            false,
            &mut QueryBudget {
                remaining: 0,
                ..budget()
            }
        ),
        Err(Error::Budget)
    );
}
#[test]
fn unknown_intermediate_map_primitive_never_returns_clear() {
    let mut snapshot = snapshot();
    snapshot.spawns[0] = None;
    assert_eq!(
        snapshot.segment_internal([0., 0., 15.], [0., 0., -5.], false, &mut budget()),
        Err(Error::Missing)
    );
}
#[test]
fn product_framing_does_not_accept_invalid_loaded_references_or_hiding_bounds() {
    for (id, offset, value) in [
        (ProductId::VmapTree, 13 + 24 + 4, 0x2000_0000u32), // cycle/invalid root
        (ProductId::VmapTree, 13, 100.0f32.to_bits()),      // inverted/hidden root bound
        (
            ProductId::VmapTile(super::super::TileId { x: 31, y: 30 }),
            TILE.len() - 4,
            9999u32,
        ),
        (
            ProductId::VmapModel("ramp.wmo.vmo".into()),
            28,
            100.0f32.to_bits(),
        ),
    ] {
        let (mut spec, mut files) = fixture();
        product_tests::mutate(&mut spec, &mut files, &id, |b| {
            b[offset..offset + 4].copy_from_slice(&value.to_le_bytes())
        });
        let loaded = product_tests::load(spec, &files).and_then(|p| p.load_static_vmap());
        assert!(loaded.is_err(), "{id:?} mutation accepted");
    }
}
#[test]
fn conflicting_cross_tile_spawn_identity_is_rejected() {
    let (mut spec, mut files) = fixture();
    let tile = ProductId::VmapTile(spec.tiles[1]);
    product_tests::mutate(&mut spec, &mut files, &tile, |b| {
        b[18..22].copy_from_slice(&999u32.to_le_bytes())
    });
    assert!(matches!(
        product_tests::load(spec, &files)
            .unwrap()
            .load_static_vmap(),
        Err(Error::Dependency)
    ));
}
#[test]
fn immutable_loaded_snapshot_survives_input_replacement_without_rebinding() {
    let (spec, mut files) = fixture();
    let original = product_tests::load(spec.clone(), &files)
        .unwrap()
        .load_static_vmap()
        .unwrap();
    files.values_mut().for_each(Vec::clear);
    assert!(original
        .segment_internal([0., 0., 15.], [0., 0., -5.], false, &mut budget())
        .is_ok());
    let (mut changed, files) = fixture();
    changed.binding.source_epoch = [77; 32];
    let replacement = product_tests::load(changed, &files)
        .unwrap()
        .load_static_vmap()
        .unwrap();
    assert_ne!(original.binding(), replacement.binding());
}

#[test]
fn exact_pinned_cpp_endpoint_and_world_conversion_queries() {
    let snapshot = snapshot();
    let mut count = 0;
    for line in include_str!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/segments.tsv").lines() {
        if line.starts_with('#') {continue;}
        let row:Vec<_>=line.split('\t').collect();
        let from=[f(row[1]),f(row[2]),f(row[3])];
        let to=[f(row[4]),f(row[5]),f(row[6])];
        let delta=sub(to,from);
        let distance=dot(delta,delta).sqrt();
        assert_eq!(distance.to_bits(),f(row[10]).to_bits(),"{} length",row[0]);
        assert_eq!(scale(delta,1.0/distance).map(f32::to_bits),[f(row[7]),f(row[8]),f(row[9])].map(f32::to_bits),"{} normalization",row[0]);
        let expected=if row[12]=="1" {StaticRay::Hit {distance:f(row[13]).to_bits()}} else {StaticRay::Miss};
        assert_eq!(snapshot.segment_internal(from,to,row[11]=="1",&mut budget()).unwrap(),expected,"{} segment",row[0]);
        count+=1;
    }
    assert_eq!(count, 2470);
    let mut count = 0;
    for line in include_str!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/world-segments.tsv").lines() {
        if line.starts_with('#') {continue;}
        let row:Vec<_>=line.split('\t').collect();
        let from=[f(row[1]),f(row[2]),f(row[3])];
        let to=[f(row[4]),f(row[5]),f(row[6])];
        assert_eq!(to_internal(from).map(f32::to_bits),[f(row[7]),f(row[8]),f(row[9])].map(f32::to_bits));
        assert_eq!(to_internal(to).map(f32::to_bits),[f(row[10]),f(row[11]),f(row[12])].map(f32::to_bits));
        let query=RayRequest {snapshot:snapshot.binding(),ticket:1,from:from.map(f32::to_bits),to:to.map(f32::to_bits),ignore_m2:row[17]=="1"};
        let result=snapshot.query(query,budget().deadline).unwrap();
        let expected=if row[18]=="1" {StaticRay::Hit {distance:f(row[19]).to_bits()}} else {StaticRay::Miss};
        assert_eq!(result,RayReply {request:query,result:expected},"{} world",row[0]);
        count+=1;
    }
    assert_eq!(count, 78);
}

#[test]
fn decoder_rejects_every_truncated_native_product_without_panics() {
    for (_, bytes) in MODELS {
        for length in 0..bytes.len() {
            // A no-GMOD model is a distinct valid source-empty model, exactly 20 bytes.
            if length == 20 {
                continue;
            }
            assert!(Model::decode(&bytes[..length]).is_err(), "prefix {length}");
        }
    }
    let models = BTreeMap::from([
        ("ramp.wmo.vmo", 0),
        ("stack.wmo.vmo", 1),
        ("wall.wmo.vmo", 2),
        ("ramp.m2.vmo", 3),
    ]);
    // First spawn has its entire count/model closure already checked by inventory.
    let spawn = &TILE[12..90];
    for length in 0..spawn.len() {
        assert!(Spawn::decode(&mut Cursor::new(&spawn[..length]), &models).is_err());
    }
}

#[test]
fn already_computed_ray_result_cannot_extend_an_absolute_deadline() {
    let snapshot = snapshot();
    let request = RayRequest {
        snapshot: snapshot.binding(),
        ticket: 1,
        from: [0; 3],
        to: [0; 3],
        ignore_m2: false,
    };
    for result in [
        StaticRay::Miss,
        StaticRay::Hit {
            distance: 1.0f32.to_bits(),
        },
    ] {
        assert_eq!(
            complete_query(request, result, Instant::now()),
            Err(Error::Budget)
        );
        assert_eq!(
            complete_query(request, result, budget().deadline),
            Ok(RayReply { request, result })
        );
    }
}

#[test]
fn empty_groups_cannot_amplify_per_placement_validation_work() {
    let mut bytes = b"VMAP_7.0WMOD".to_vec();
    let word = |bytes: &mut Vec<u8>, v: u32| bytes.extend_from_slice(&v.to_le_bytes());
    word(&mut bytes, 8);
    word(&mut bytes, 0);
    bytes.extend_from_slice(b"GMOD");
    word(&mut bytes, 4096);
    for _ in 0..4096 {
        bytes.extend_from_slice(&[0; 32]); // finite zero bounds, flags and WMO ID
        bytes.extend_from_slice(b"VERT");
        word(&mut bytes, 4);
        word(&mut bytes, 0);
    }
    bytes.extend_from_slice(b"GBIH");
    bytes.extend_from_slice(&[0; 24]);
    for v in [3, 3 << 30, 4096, 0, 4096] {
        word(&mut bytes, v);
    }
    for v in 0..4096 {
        word(&mut bytes, v);
    }
    let model = Model::decode(&bytes).unwrap();
    assert_eq!(model.validation_work, 4097);
    let mut work = 0;
    for _ in 0..255 {
        charge_placement(&mut work, &model).unwrap();
    }
    assert_eq!(charge_placement(&mut work, &model), Err(Error::Budget));
}

#[test]
fn source_trig_seam_preserves_strict_transformed_vertex_containment() {
    // The source float sin at 13 degrees is one ULP above other libm results.
    // That difference moves this rotated vertex two ULPs below the native bound.
    assert_eq!(
        trig::sin_cos_degrees(13.0).unwrap().0.to_bits(),
        0x3e66_5992
    );
    let mut snapshot = snapshot();
    let spawn = snapshot
        .spawns
        .iter_mut()
        .flatten()
        .find(|s| s.id == 104)
        .unwrap();
    let world = apply(
        rotation(spawn.angles).unwrap(),
        scale([-4.0, -4.0, -3.0], spawn.scale),
    );
    assert_eq!((world[2] + spawn.position[2]).to_bits(), 0x3fcb_69e8);
    assert_eq!(spawn.bounds.low[2].to_bits(), 0x3fcb_69e8);
    let (_, c13) = trig::sin_cos_degrees(13.0).unwrap();
    let (s9, c9) = trig::sin_cos_degrees(-9.0).unwrap();
    let other_libm_row = [-f32::from_bits(0x3e66_5991), c13 * s9, c13 * c9];
    let outside = dot(other_libm_row, scale([-4.0, -4.0, -3.0], spawn.scale)) + spawn.position[2];
    assert_eq!(outside.to_bits(), 0x3fcb_69e6);
    assert!(outside < spawn.bounds.low[2]);
    let model = &snapshot.models[spawn.model];
    assert_eq!(spawn.validate_model_bounds(model), Ok(()));
    // No padding/tolerance: even one ULP of genuine vertex exclusion still rejects.
    spawn.bounds.low[2] = spawn.bounds.low[2].next_up();
    assert_eq!(spawn.validate_model_bounds(model), Err(Error::Format));
}
