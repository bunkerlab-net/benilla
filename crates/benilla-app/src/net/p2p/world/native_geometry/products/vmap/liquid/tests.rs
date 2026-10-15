// SPDX-License-Identifier: GPL-3.0-or-later
use super::super::super::{
    envelopes, tests as product_tests, InventoriedProducts, InventorySpec, ProductId,
};
use super::super::{tests as vmap_tests, Model, MAX_ELEMENTS};
use super::*;
use std::{collections::BTreeMap, time::Duration};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
fn budget() -> QueryBudget {
    QueryBudget::new(deadline())
}
fn word(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}
fn payload(width: u32, height: u32, liquid_type: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    for value in [
        width,
        height,
        (-4.0f32).to_bits(),
        (-4.0f32).to_bits(),
        777.0f32.to_bits(),
        liquid_type,
    ] {
        word(&mut bytes, value);
    }
    for index in 0..(width + 1) * (height + 1) {
        word(&mut bytes, (index as f32 + 2.0).to_bits());
    }
    bytes.extend(std::iter::repeat_n(0, (width * height) as usize));
    bytes
}
fn decode(bytes: &[u8]) -> Result<WmoLiquid, Error> {
    let mut c = Cursor::new(bytes);
    let liquid = WmoLiquid::decode(&mut c, bytes.len().checked_sub(4).ok_or(Error::Framing)?)?;
    c.finish()?;
    Ok(liquid)
}
fn inputs() -> (InventorySpec, BTreeMap<String, Vec<u8>>) {
    let (mut spec, mut files) = vmap_tests::fixture();
    product_tests::mutate(
        &mut spec,
        &mut files,
        &ProductId::VmapModel("ramp.wmo.vmo".into()),
        |bytes| {
            let offset = bytes.windows(4).position(|word| word == b"LIQU").unwrap() + 4;
            let liquid = payload(2, 2, 1);
            bytes[offset..offset + 4].copy_from_slice(&((liquid.len() - 4) as u32).to_le_bytes());
            bytes.splice(offset + 4..offset + 4, liquid);
        },
    );
    (spec, files)
}
fn products(
    spec: InventorySpec,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<VmapLiquidProducts, Error> {
    Ok(VmapLiquidProducts {
        inventory: InventoriedProducts::materialize_profile(spec, true, |name, _| {
            files.get(name).cloned().ok_or(Error::Missing)
        })?,
    })
}
fn snapshot() -> LiquidSnapshot {
    let (spec, files) = inputs();
    products(spec, &files).unwrap().load_vmap_liquid().unwrap()
}
fn request(snapshot: &LiquidSnapshot) -> LiquidRequest {
    LiquidRequest {
        snapshot: snapshot.binding(),
        ticket: 1,
        position: to_internal([0.0, 0.0, 0.0]).map(f32::to_bits),
        required_mask: 0,
    }
}

#[test]
fn opt_in_inventory_and_wet_snapshot_cannot_be_substituted_for_dry_readers() {
    let (spec, files) = inputs();
    assert!(matches!(
        product_tests::load(spec.clone(), &files),
        Err(Error::Unsupported)
    ));
    let wet = products(spec, &files).unwrap();
    // Even its private inventory cannot enter old VMap or composite/dry constructors.
    assert!(matches!(
        wet.inventory.load_static_vmap(),
        Err(Error::Unsupported)
    ));
    assert!(matches!(
        wet.inventory.load_static_height(),
        Err(Error::Unsupported)
    ));
    assert!(matches!(
        wet.inventory.load_dry_liquid(),
        Err(Error::Unsupported)
    ));
    assert!(matches!(
        wet.inventory.load_navigation(),
        Err(Error::Unsupported)
    ));
    let empty_dbc = |fields: u32| {
        let mut bytes = b"WDBC".to_vec();
        for value in [0, fields, fields * 4, 1] {
            word(&mut bytes, value);
        }
        bytes.push(0);
        bytes
    };
    let types = empty_dbc(4);
    let areas = empty_dbc(25);
    assert!(matches!(
        wet.inventory.load_grid_liquid(&types),
        Err(Error::Unsupported)
    ));
    assert!(matches!(
        wet.inventory.load_grid_liquid_with_areas(&types, &areas),
        Err(Error::Unsupported)
    ));
    assert!(matches!(
        wet.inventory.load_terrain_liquid(&types),
        Err(Error::Unsupported)
    ));
    assert!(matches!(
        wet.inventory.load_terrain_liquid_with_areas(&types, &areas),
        Err(Error::Unsupported)
    ));
    assert!(matches!(
        wet.load_vmap_liquid()
            .unwrap()
            .geometry
            .require_complete_dry_map(),
        Err(Error::Unsupported)
    ));
    // The new wrapper has no navigation/ray/height/location/composite reader method.
    let (spec, files) = vmap_tests::fixture();
    let dry = product_tests::load(spec.clone(), &files).unwrap();
    let opt_in = products(spec, &files).unwrap();
    assert_ne!(dry.revision(), opt_in.inventory.revision());
    assert_ne!(
        dry.load_static_vmap().unwrap().binding(),
        opt_in.load_vmap_liquid().unwrap().binding()
    );
}

#[test]
fn source_size_omits_type_word_and_every_truncation_is_rejected() {
    let bytes = payload(3, 2, u32::MAX);
    let declared = bytes.len() - 4;
    assert_eq!(declared, 20 + 4 * 12 + 6);
    assert_eq!(decode(&bytes).unwrap().liquid_type, u32::MAX);
    for prefix in 0..bytes.len() {
        let mut c = Cursor::new(&bytes[..prefix]);
        assert!(
            WmoLiquid::decode(&mut c, declared).is_err(),
            "prefix {prefix}"
        );
    }
    for wrong in [0, 1, declared - 1, declared + 1, bytes.len(), usize::MAX] {
        assert!(WmoLiquid::decode(&mut Cursor::new(&bytes), wrong).is_err());
    }
    for (width, height) in [
        (u32::MAX, 0),
        (0, u32::MAX),
        (65536, 65536),
        (MAX_ELEMENTS as u32, 1),
    ] {
        assert_eq!(liquid_dimensions(width, height, 1), Err(Error::Budget));
    }
    for offset in [8, 12, 16, 24] {
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1_000_001.0] {
            let mut malformed = bytes.clone();
            malformed[offset..offset + 4].copy_from_slice(&invalid.to_bits().to_le_bytes());
            assert!(decode(&malformed).is_err());
        }
    }
}

#[test]
fn framing_and_decoder_both_reject_malformed_actual_model_bytes() {
    let (spec, files) = inputs();
    let id = ProductId::VmapModel("ramp.wmo.vmo".into());
    let bytes = &files[&id.filename(0).unwrap()];
    let at = bytes.windows(4).position(|part| part == b"LIQU").unwrap() + 4;
    assert!(envelopes::validate_with_vmap_liquid(&id, bytes).is_ok());
    assert!(Model::decode_profile(bytes, true).is_ok());
    assert!(matches!(Model::decode(bytes), Err(Error::Unsupported)));
    for (offset, value) in [
        (at, 1),
        (at, u32::MAX),
        (at + 4, u32::MAX),
        (at + 8, u32::MAX),
        (at + 12, f32::NAN.to_bits()),
        (at + 28, f32::INFINITY.to_bits()),
    ] {
        let mut changed = bytes.clone();
        changed[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        assert!(envelopes::validate_with_vmap_liquid(&id, &changed).is_err());
        assert!(Model::decode_profile(&changed, true).is_err());
    }
    for prefix in at + 4..bytes.len() {
        assert!(envelopes::validate_with_vmap_liquid(&id, &bytes[..prefix]).is_err());
        assert!(Model::decode_profile(&bytes[..prefix], true).is_err());
    }
    let mut appended = bytes.clone();
    appended.push(0);
    assert!(envelopes::validate_with_vmap_liquid(&id, &appended).is_err());
    assert!(Model::decode_profile(&appended, true).is_err());
    let mut changed = files.clone();
    changed.get_mut(&id.filename(0).unwrap()).unwrap()[at + 24] ^= 1;
    assert!(matches!(products(spec, &changed), Err(Error::Hash)));
}

#[test]
fn zero_dimensions_raw_type_and_corner_z_are_preserved_without_invented_water() {
    for (width, height) in [(0, 0), (0, 3), (3, 0)] {
        let liquid = decode(&payload(width, height, u32::MAX)).unwrap();
        assert_eq!(liquid.liquid_type, u32::MAX);
        assert_eq!(liquid.height([-4.0, -4.0, 0.0], &mut budget()), Ok(None));
    }
    let mut liquid = decode(&payload(1, 1, 0)).unwrap();
    let position = [-4.0 + TILE_SIZE * 0.75, -4.0 + TILE_SIZE * 0.25, -100.0];
    let before = liquid.height(position, &mut budget()).unwrap();
    liquid.corner[2] = -500.0;
    assert_eq!(
        liquid
            .height([position[0], position[1], 999.0], &mut budget())
            .unwrap(),
        before
    );
}

#[test]
fn only_low_nibble_f_disables_and_interpolation_uses_ordered_triangles() {
    let mut liquid = decode(&payload(1, 1, 1)).unwrap();
    liquid.corner = [0.0; 3];
    liquid.heights = vec![2.0, 5.0, 7.0, 17.0];
    for flag in 0..=255 {
        liquid.flags[0] = flag;
        for (dx, dy) in [(0.75, 0.25), (0.25, 0.75), (0.5, 0.5)] {
            let x = TILE_SIZE * dx;
            let y = TILE_SIZE * dy;
            let dx = x / TILE_SIZE;
            let dy = y / TILE_SIZE;
            let expected = if flag & 0xf == 0xf {
                None
            } else {
                Some(if dx > dy {
                    2.0 + dx * 3.0 + dy * 12.0
                } else {
                    2.0 + dx * 10.0 + dy * 5.0
                })
            };
            assert_eq!(
                liquid
                    .height([x, y, 0.0], &mut budget())
                    .unwrap()
                    .map(f32::to_bits),
                expected.map(f32::to_bits)
            );
        }
    }
}

#[test]
fn conversion_domain_and_x_before_y_short_circuit_match_defined_source_only() {
    assert_eq!(source_index(-0.999), Ok(0));
    assert_eq!(source_index(-0.0), Ok(0));
    assert_eq!(
        source_index(4_294_967_296.0_f32.next_down()),
        Ok(4_294_967_040)
    );
    for invalid in [-1.0, -2.0, 4_294_967_296.0_f32, f32::MAX] {
        assert_eq!(source_index(invalid), Err(Error::Unsupported));
    }
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(source_index(invalid), Err(Error::Format));
    }
    let mut liquid = decode(&payload(1, 1, 1)).unwrap();
    liquid.corner = [0.0; 3];
    assert_eq!(
        liquid.height([-TILE_SIZE * 0.5, f32::NAN, 0.0], &mut budget()),
        Ok(None)
    );
    assert_eq!(
        liquid.height([TILE_SIZE, f32::NAN, 0.0], &mut budget()),
        Ok(None)
    );
    assert_eq!(
        liquid.height([-TILE_SIZE, 0.0, 0.0], &mut budget()),
        Err(Error::Unsupported)
    );
    assert_eq!(
        liquid.height([0.0, -TILE_SIZE, 0.0], &mut budget()),
        Err(Error::Unsupported)
    );
}

#[test]
fn identities_positions_masks_and_ticket_are_echoed_and_stale_queries_rejected() {
    let snapshot = snapshot();
    let original = request(&snapshot);
    for field in 0..7 {
        let mut changed = original;
        match field {
            0 => changed.snapshot.map += 1,
            1 => changed.snapshot.content[0] ^= 1,
            2 => changed.snapshot.source_epoch[0] ^= 1,
            3 => changed.snapshot.inventory[0] ^= 1,
            4 => changed.snapshot.policy[0] ^= 1,
            5 => changed.ticket = 0,
            _ => changed.snapshot = snapshot.geometry.location_binding(),
        }
        assert_eq!(snapshot.query(changed, deadline()), Err(Error::Identity));
    }
    for mask in 0..=255 {
        let query = LiquidRequest {
            required_mask: mask,
            ..original
        };
        let answer = snapshot.query(query, deadline()).unwrap();
        assert_eq!(answer.request, query);
        assert!(
            matches!(answer.result, LiquidResult::Location { liquid_type: 1, level, .. } if level.is_some() == (mask == 0 || mask & 8 != 0))
        );
    }
    for position in [[0; 3], [(-0.0f32).to_bits(); 3], original.position] {
        let query = LiquidRequest {
            position,
            ticket: 42,
            ..original
        };
        assert_eq!(snapshot.query(query, deadline()).unwrap().request, query);
    }
    for axis in 0..3 {
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut query = original;
            query.position[axis] = invalid.to_bits();
            assert_eq!(snapshot.query(query, deadline()), Err(Error::Format));
        }
        let mut query = original;
        query.position[axis] = 100_001.0f32.to_bits();
        assert_eq!(snapshot.query(query, deadline()), Err(Error::Unsupported));
    }
}

#[test]
fn liquid_mask_is_raw_classic_mapping_including_zero_and_unknown() {
    for (kind, wanted) in [
        (0, 0),
        (1, 8),
        (2, 2),
        (3, 1),
        (4, 4),
        (21, 4),
        (41, 8),
        (61, 8),
        (5, 0),
        (65536, 0),
        (u32::MAX, 0),
    ] {
        assert_eq!(liquid_mask(kind), wanted);
    }
    let mut snapshot = snapshot();
    for model in &mut snapshot.geometry.models {
        for group in &mut model.groups {
            if let Some(liquid) = &mut group.liquid {
                liquid.liquid_type = u32::MAX;
                liquid.corner[0] = 100.0; // defined source conversion would fail
            }
        }
    }
    let query = LiquidRequest {
        required_mask: 1,
        ..request(&snapshot)
    };
    assert!(matches!(
        snapshot.query(query, deadline()).unwrap().result,
        LiquidResult::Location {
            liquid_type: u32::MAX,
            level: None,
            ..
        }
    ));
    assert_eq!(
        snapshot.query(
            LiquidRequest {
                required_mask: 0,
                ..query
            },
            deadline()
        ),
        Err(Error::Unsupported)
    );
}

#[test]
fn one_budget_covers_complete_location_and_liquid_and_expiry_never_becomes_absence() {
    let mut snapshot = snapshot();
    let query = request(&snapshot);
    assert_eq!(snapshot.query(query, Instant::now()), Err(Error::Budget));
    let position = query.position.map(f32::from_bits);
    let mut shared = budget();
    let before = shared.remaining;
    let result = snapshot.liquid(position, 0, &mut shared).unwrap();
    let used = before - shared.remaining;
    assert!(used > snapshot.geometry.spawns.len());
    shared.remaining = used - 1;
    assert_eq!(
        snapshot.liquid(position, 0, &mut shared),
        Err(Error::Budget)
    );
    shared.remaining = used;
    assert_eq!(snapshot.liquid(position, 0, &mut shared), Ok(result));
    assert_eq!(shared.remaining, 0);
    shared.deadline = Instant::now();
    assert_eq!(shared.complete(), Err(Error::Budget));
    snapshot.geometry.spawns[0] = None;
    assert_eq!(
        snapshot.query(
            LiquidRequest {
                position: [100_000.0f32.to_bits(); 3],
                ..query
            },
            deadline()
        ),
        Err(Error::Missing)
    );
}

// Authored geometry serialized and queried by the pinned original C++ source.
const SOURCE_MODELS: &[(&str, &[u8])] = &[
    ("dry-occluder.wmo.vmo", include_bytes!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedliquid/dry-occluder.wmo.vmo")),
    ("dry.wmo.vmo", include_bytes!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedliquid/dry.wmo.vmo")),
    ("empty-0x0.wmo.vmo", include_bytes!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedliquid/empty-0x0.wmo.vmo")),
    ("empty-0x2.wmo.vmo", include_bytes!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedliquid/empty-0x2.wmo.vmo")),
    ("empty-2x0.wmo.vmo", include_bytes!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedliquid/empty-2x0.wmo.vmo")),
    ("hole-occluder.wmo.vmo", include_bytes!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedliquid/hole-occluder.wmo.vmo")),
    ("type-0.wmo.vmo", include_bytes!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedliquid/type-0.wmo.vmo")),
    ("type-1.wmo.vmo", include_bytes!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedliquid/type-1.wmo.vmo")),
    ("type-2.wmo.vmo", include_bytes!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedliquid/type-2.wmo.vmo")),
    ("type-21.wmo.vmo", include_bytes!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedliquid/type-21.wmo.vmo")),
    ("type-3.wmo.vmo", include_bytes!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedliquid/type-3.wmo.vmo")),
    ("type-305419896.wmo.vmo", include_bytes!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedliquid/type-305419896.wmo.vmo")),
    ("type-4.wmo.vmo", include_bytes!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedliquid/type-4.wmo.vmo")),
    ("type-41.wmo.vmo", include_bytes!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedliquid/type-41.wmo.vmo")),
    ("type-61.wmo.vmo", include_bytes!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedliquid/type-61.wmo.vmo")),
    ("type-999.wmo.vmo", include_bytes!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedliquid/type-999.wmo.vmo")),
    ("wet.m2.vmo", include_bytes!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedliquid/wet.m2.vmo")),
    ("wet.wmo.vmo", include_bytes!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedliquid/wet.wmo.vmo")),
    ("zero-corner.wmo.vmo", include_bytes!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedliquid/zero-corner.wmo.vmo")),
];
pub(crate) fn source_fixture() -> (InventorySpec, BTreeMap<String, Vec<u8>>) {
    let (mut spec, mut files) = vmap_tests::fixture();
    for item in &spec.products {
        if matches!(item.id, ProductId::VmapModel(_)) {
            files.remove(&item.id.filename(0).unwrap());
        }
    }
    spec.products
        .retain(|item| !matches!(item.id, ProductId::VmapModel(_)));
    const TREE: &[u8] = include_bytes!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedliquid/000.vmtree");
    const TILE: &[u8] = include_bytes!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedliquid/000_32_32.vmtile");
    product_tests::mutate(&mut spec, &mut files, &ProductId::VmapTree, |bytes| {
        *bytes = TREE.to_vec()
    });
    let dependencies: Vec<_> = std::iter::once(ProductId::VmapTree)
        .chain(
            envelopes::validate_with_vmap_liquid(&ProductId::VmapTile(spec.tiles[0]), TILE)
                .unwrap(),
        )
        .collect();
    for tile in spec.tiles.clone() {
        let id = ProductId::VmapTile(tile);
        product_tests::mutate(&mut spec, &mut files, &id, |bytes| *bytes = TILE.to_vec());
        spec.products
            .iter_mut()
            .find(|item| item.id == id)
            .unwrap()
            .dependencies = dependencies.clone();
    }
    for &(name, bytes) in SOURCE_MODELS {
        let id = ProductId::VmapModel(name.into());
        files.insert(id.filename(0).unwrap(), bytes.to_vec());
        spec.products.push(super::super::super::ProductSpec {
            id,
            bytes: bytes.len() as u64,
            sha256: Sha256::digest(bytes).into(),
            dependencies: vec![],
        });
    }
    (spec, files)
}
fn bits(text: &str) -> u32 {
    u32::from_str_radix(text, 16).unwrap()
}

#[test]
fn exact_original_source_manager_queries_match_loaded_wet_bytes() {
    let (spec, files) = source_fixture();
    let snapshot = products(spec, &files).unwrap().load_vmap_liquid().unwrap();
    let mut count = 0;
    let mut wet = 0;
    let mut selected_without_level = 0;
    for line in include_str!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedliquid.tsv").lines().filter(|line| !line.starts_with('#')) {
        let row: Vec<_> = line.split('\t').collect();
        let query = LiquidRequest { snapshot: snapshot.binding(), ticket: count + 1, position: [bits(row[2]), bits(row[3]), bits(row[4])], required_mask: row[15].parse().unwrap() };
        let actual = snapshot.query(query, deadline()).unwrap_or_else(|error| panic!("{}: {error:?}", row[1]));
        assert_eq!(actual.request, query);
        let expected = if row[8] == "1" {
            let level = (row[16] == "1").then(|| bits(row[17]));
            wet += usize::from(level.is_some());
            selected_without_level += usize::from(level.is_none());
            assert_eq!(bits(row[9]), bits(row[18]), "{} selected floor write", row[1]);
            if level.is_none() { assert_eq!(bits(row[17]), 123.25f32.to_bits()); }
            LiquidResult::Location {
                location: LocationFloor { floor: bits(row[18]), root_id: row[11].parse::<i64>().unwrap() as u32, group_id: row[12].parse().unwrap(), group_flags: row[13].parse().unwrap(), spawn_id: row[10].parse().unwrap(), adt_id: row[14].parse().unwrap() },
                liquid_type: row[19].parse().unwrap(), level,
            }
        } else {
            assert_eq!(row[16], "0");
            assert_eq!(bits(row[17]), 123.25f32.to_bits());
            assert_eq!(bits(row[18]), (-100000.0f32).to_bits());
            assert_eq!(row[19], "305419896");
            LiquidResult::NoLocation
        };
        assert_eq!(actual.result, expected, "{}", row[1]);
        // Direct instance rows preserve source's independent transform/level formula.
        if row[22] == "1" {
            let position = query.position.map(f32::from_bits);
            let selected = snapshot.geometry.select_location(position, &mut budget()).unwrap().unwrap();
            let spawn = snapshot.geometry.spawns[selected.spawn].as_ref().unwrap();
            let group = &snapshot.geometry.models[spawn.model].groups[selected.group];
            let position = to_internal(position);
            let local = scale(apply(spawn.inverse, sub(position, spawn.position)), spawn.inverse_scale);
            let level = group.liquid.as_ref().and_then(|liquid| liquid.height(local, &mut budget()).unwrap()).map(|height| ((height - local[2]) * spawn.scale + position[2]).to_bits());
            assert_eq!(level.is_some(), row[20] == "1", "{} instance hit", row[1]);
            assert_eq!(level.unwrap_or(123.25f32.to_bits()), bits(row[21]), "{} instance level", row[1]);
        }
        count += 1;
    }
    assert_eq!(count, 5511);
    assert!(wet > 500 && selected_without_level > 500);
}

#[test]
fn exact_original_source_direct_heights_and_wire_sizes_match_loaded_models() {
    let models: BTreeMap<_, _> = SOURCE_MODELS
        .iter()
        .map(|&(name, bytes)| (name, Model::decode_profile(bytes, true).unwrap()))
        .collect();
    let mut count = 0;
    for line in include_str!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedliquid_direct.tsv").lines().filter(|line| !line.starts_with('#')) {
        let row: Vec<_> = line.split('\t').collect();
        let name = format!("{}.vmo", row[0]);
        let group = &models[name.as_str()].groups[row[1].parse::<usize>().unwrap()];
        assert_eq!(group.id, row[2].parse::<u32>().unwrap());
        let liquid = group.liquid.as_ref().unwrap();
        assert_eq!(liquid.liquid_type, row[11].parse::<u32>().unwrap());
        let position = [bits(row[4]), bits(row[5]), bits(row[6])].map(f32::from_bits);
        let result = liquid.height(position, &mut budget()).unwrap_or_else(|error| panic!("{} {}: {error:?}", name, row[3]));
        assert_eq!(result.is_some(), row[7] == "1", "{} {}", name, row[3]);
        assert_eq!(result.map_or(123.25f32.to_bits(), f32::to_bits), bits(row[8]), "{} {}", name, row[3]);
        assert_eq!(row[7], row[9]);
        assert_eq!(row[8], row[10]);
        count += 1;
    }
    assert_eq!(count, 1372);
    let mut count = 0;
    for line in include_str!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedliquid_sizes.tsv").lines().filter(|line| !line.starts_with('#')) {
        let row: Vec<_> = line.split('\t').collect();
        let name = format!("{}.vmo", row[0]);
        let index: usize = row[1].parse().unwrap();
        let group = &models[name.as_str()].groups[index];
        let liquid = group.liquid.as_ref().unwrap();
        assert_eq!(group.id, row[2].parse::<u32>().unwrap());
        assert_eq!((liquid.width, liquid.height), (row[3].parse::<usize>().unwrap(), row[4].parse::<usize>().unwrap()));
        assert_eq!(liquid.corner.map(f32::to_bits), [bits(row[5]), bits(row[6]), bits(row[7])]);
        assert_eq!(liquid.liquid_type, row[8].parse::<u32>().unwrap());
        assert_eq!(liquid_mask(liquid.liquid_type), row[9].parse::<u8>().unwrap());
        let declared: usize = row[10].parse().unwrap();
        let actual: usize = row[11].parse().unwrap();
        assert_eq!(declared + 4, actual);
        let bytes = SOURCE_MODELS.iter().find(|&&(key, _)| key == name).unwrap().1;
        let at = bytes.windows(4).enumerate().filter(|(_, word)| *word == b"LIQU").nth(index).unwrap().0;
        assert_eq!(u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize, declared);
        let mut cursor = Cursor::new(&bytes[at + 8..at + 8 + actual]);
        let parsed = WmoLiquid::decode(&mut cursor, declared).unwrap();
        cursor.finish().unwrap();
        assert_eq!(parsed.heights, liquid.heights);
        assert_eq!(parsed.flags, liquid.flags);
        count += 1;
    }
    assert_eq!(count, 19);
}

#[test]
fn filesystem_opt_in_owns_bytes_after_source_replacement_and_deletion() {
    let (spec, files) = inputs();
    let stage = product_tests::Stage::new(&files);
    let root = &stage.0;
    assert!(matches!(
        InventoriedProducts::load(root, spec.clone()),
        Err(Error::Unsupported)
    ));
    let products = VmapLiquidProducts::load(root, spec.clone()).unwrap();
    let snapshot = products.load_vmap_liquid().unwrap();
    let query = request(&snapshot);
    let original = snapshot.query(query, deadline()).unwrap();
    let model_path = root.join("vmaps/ramp.wmo.vmo");
    let mut changed = files["vmaps/ramp.wmo.vmo"].clone();
    changed[0] ^= 1;
    std::fs::write(&model_path, changed).unwrap();
    assert!(matches!(
        VmapLiquidProducts::load(root, spec.clone()),
        Err(Error::Hash)
    ));
    std::fs::remove_file(&model_path).unwrap();
    assert!(matches!(
        VmapLiquidProducts::load(root, spec.clone()),
        Err(Error::Missing)
    ));
    std::fs::write(&model_path, &files["vmaps/ramp.wmo.vmo"]).unwrap();
    std::fs::write(root.join("vmaps/extra.vmo"), [0; 4]).unwrap();
    assert!(matches!(
        VmapLiquidProducts::load(root, spec),
        Err(Error::Unexpected)
    ));
    assert_eq!(snapshot.query(query, deadline()).unwrap(), original);
    drop(stage);
    assert_eq!(snapshot.query(query, deadline()).unwrap(), original);
    assert_eq!(
        products
            .load_vmap_liquid()
            .unwrap()
            .query(query, deadline())
            .unwrap(),
        original
    );
}

#[test]
fn even_unreferenced_wet_models_are_validated_before_publication() {
    let (mut spec, mut files) = inputs();
    let mut bytes = files["vmaps/ramp.wmo.vmo"].clone();
    let at = bytes.windows(4).position(|part| part == b"MBIH").unwrap() + 4 + 24 + 4;
    bytes[at..at + 4].copy_from_slice(&0x2000_0000u32.to_le_bytes());
    let id = ProductId::VmapModel("unused.wmo.vmo".into());
    files.insert(id.filename(0).unwrap(), bytes.clone());
    spec.products.push(super::super::super::ProductSpec {
        id,
        bytes: bytes.len() as u64,
        sha256: Sha256::digest(&bytes).into(),
        dependencies: vec![],
    });
    assert!(matches!(products(spec, &files), Err(Error::Unexpected)));

    // A declared model outside the planned query still must validate globally.
    let (mut spec, mut files) = inputs();
    product_tests::mutate(
        &mut spec,
        &mut files,
        &ProductId::VmapModel("wall.wmo.vmo".into()),
        |bytes| {
            let at = bytes.windows(4).position(|part| part == b"MBIH").unwrap() + 4 + 24 + 4;
            bytes[at..at + 4].copy_from_slice(&0x2000_0000u32.to_le_bytes());
        },
    );
    let products = products(spec, &files).unwrap();
    assert!(products.load_vmap_liquid().is_err());
}

#[test]
fn unresolved_map_slots_prevent_wet_snapshot_publication() {
    let (mut spec, mut files) = inputs();
    for item in &spec.products {
        if matches!(item.id, ProductId::VmapModel(_)) {
            files.remove(&item.id.filename(0).unwrap());
        }
    }
    spec.products
        .retain(|item| !matches!(item.id, ProductId::VmapModel(_)));
    for tile in spec.tiles.clone() {
        let id = ProductId::VmapTile(tile);
        product_tests::mutate(&mut spec, &mut files, &id, |bytes| {
            *bytes = b"VMAP_7.0".to_vec();
            word(bytes, 0);
        });
        spec.products
            .iter_mut()
            .find(|item| item.id == id)
            .unwrap()
            .dependencies = vec![ProductId::VmapTree];
    }
    let products = products(spec, &files).unwrap();
    assert!(matches!(products.load_vmap_liquid(), Err(Error::Missing)));
}
