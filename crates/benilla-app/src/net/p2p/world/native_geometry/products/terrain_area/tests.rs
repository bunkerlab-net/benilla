// SPDX-License-Identifier: GPL-3.0-or-later
// Authored actual MAPS/WDBC/native VMap bytes; no installation data or caller predicates.
use super::super::{
    tests as product_tests, vmap, InventoriedProducts, InventorySpec, ProductId, ProductSpec,
    TileId,
};
use super::*;
use std::{collections::BTreeMap, time::Duration};

macro_rules! native {
    ($path:literal) => {
        include_bytes!(concat!(
            "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedarea/",
            $path
        ))
    };
}

pub(super) fn fixture(reverse: bool, terrain: &[u8]) -> (InventorySpec, BTreeMap<String, Vec<u8>>) {
    let (mut spec, mut files) = vmap::tests::fixture();
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
    for product in &spec.products {
        if matches!(product.id, ProductId::VmapModel(_)) {
            files.remove(&product.id.filename(0).unwrap());
        }
    }
    spec.products
        .retain(|product| !matches!(product.id, ProductId::VmapModel(_)));
    product_tests::mutate(&mut spec, &mut files, &ProductId::VmapTree, |bytes| {
        *bytes = if reverse {
            native!("reverse/000.vmtree").to_vec()
        } else {
            native!("forward/000.vmtree").to_vec()
        };
    });
    let models: [(&str, &[u8]); 6] = [
        ("high.wmo.vmo", native!("forward/high.wmo.vmo")),
        ("low.wmo.vmo", native!("forward/low.wmo.vmo")),
        ("equal.wmo.vmo", native!("forward/equal.wmo.vmo")),
        ("multi.wmo.vmo", native!("forward/multi.wmo.vmo")),
        ("wet.wmo.vmo", native!("forward/wet.wmo.vmo")),
        ("high.m2.vmo", native!("forward/high.m2.vmo")),
    ];
    let dependencies: Vec<_> = std::iter::once(ProductId::VmapTree)
        .chain(
            models
                .iter()
                .map(|(name, _)| ProductId::VmapModel((*name).into())),
        )
        .collect();
    for tile in spec.tiles.clone() {
        let id = ProductId::VmapTile(tile);
        product_tests::mutate(&mut spec, &mut files, &id, |bytes| {
            *bytes = native!("forward/000_32_32.vmtile").to_vec();
        });
        spec.products
            .iter_mut()
            .find(|product| product.id == id)
            .unwrap()
            .dependencies = dependencies.clone();
        product_tests::mutate(&mut spec, &mut files, &ProductId::Terrain(tile), |bytes| {
            *bytes = terrain.to_vec();
        });
    }
    for (name, bytes) in models {
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

pub(super) fn load(
    spec: InventorySpec,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<VmapLiquidProducts, Error> {
    Ok(VmapLiquidProducts {
        inventory: InventoriedProducts::materialize_profile(spec, true, |name, _| {
            files.get(name).cloned().ok_or(Error::Missing)
        })?,
    })
}

fn flat(height: f32, flag: u16) -> Vec<u8> {
    let mut bytes = product_tests::terrain_bytes(0);
    bytes[46..48].copy_from_slice(&flag.to_le_bytes());
    bytes[56..60].copy_from_slice(&height.to_le_bytes());
    bytes[60..64].copy_from_slice(&height.to_le_bytes());
    bytes
}
fn products(height: f32, flag: u16) -> VmapLiquidProducts {
    let (spec, files) = fixture(false, &flat(height, flag));
    load(spec, &files).unwrap()
}

fn dbc<const FIELDS: usize>(rows: &[[u32; FIELDS]], strings: &[u8]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for word in [
        rows.len() as u32,
        FIELDS as u32,
        FIELDS as u32 * 4,
        strings.len() as u32,
    ] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    for row in rows {
        for word in row {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
    }
    bytes.extend_from_slice(strings);
    bytes
}
fn area(id: u32, map: u32, zone: u32, flag: u32, liquid: u32) -> [u32; 25] {
    let mut row = [0; 25];
    row[0] = id;
    row[1] = map;
    row[2] = zone;
    row[3] = flag;
    row[24] = liquid;
    row
}
fn wmo(id: u32, area: u32) -> [u32; 20] {
    let mut row = [0; 20];
    row[0] = id;
    row[1] = 0xf222_2222;
    row[2] = 0x8101;
    row[3] = 0xe222_2222;
    row[10] = area;
    row
}
fn map(id: u32, linked: u32) -> [u32; 42] {
    let mut row = [0; 42];
    row[0] = id;
    row[19] = linked;
    row
}

#[derive(Clone)]
struct Dbcs {
    areas: Vec<u8>,
    wmos: Vec<u8>,
    maps: Vec<u8>,
}
impl Dbcs {
    fn snapshot<'a>(&self, products: &'a VmapLiquidProducts) -> TerrainAreaSnapshot<'a> {
        products
            .load_terrain_area(&self.areas, &self.wmos, &self.maps)
            .unwrap()
    }
}
fn dbcs() -> Dbcs {
    Dbcs {
        // Physical order is intentionally unlike indexed ID order.
        areas: dbc(
            &[
                area(300, 0, 0, 77, 3),
                area(101, 0, 200, 55, 0),
                area(100, 0, 0, 55, 1),
                area(200, 0, 0, 99, 3),
                area(20, 0, 0, 0x1_0037, 2),
                area(500, 9, 0, 888, 4),
                area(3, 7, 0, 66, 1),
                area(9, 8, 200, 66, 0),
            ],
            &[0],
        ),
        wmos: dbc(&[wmo(99, 20), wmo(1, 300), wmo(88, 999), wmo(80, 3)], &[0]),
        maps: dbc(&[map(0, 500)], &[0]),
    }
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
pub(super) fn position(internal: [f32; 3]) -> [u32; 3] {
    let mid = 0.5f32 * (64.0 * super::super::terrain::GRID_SIZE);
    [mid - internal[0], mid - internal[1], internal[2]].map(f32::to_bits)
}
fn request(snapshot: &TerrainAreaSnapshot<'_>) -> TerrainAreaRequest {
    TerrainAreaRequest {
        snapshot: snapshot.binding(),
        ticket: 1,
        position: position([0., 0., 7.]),
    }
}
fn result(products: &VmapLiquidProducts, dbcs: &Dbcs) -> TerrainAreaResult {
    let snapshot = dbcs.snapshot(products);
    snapshot
        .query(request(&snapshot), deadline())
        .unwrap()
        .result
}

#[test]
fn indexed_wmo_last_map_match_truncates_before_first_final_same_map_match() {
    let products = products(0., 66);
    let result = result(&products, &dbcs());
    assert_eq!(
        result.raw_area,
        AreaResult::Area {
            floor: 6f32.to_bits(),
            adt_id: 0x8101,
            root_id: 0xf222_2222,
            group_id: 0xe222_2222,
            flags: 0x4000_2000,
        }
    );
    assert_eq!(result.area_info, result.raw_area);
    assert_eq!(result.area_flag, 55);
    // WMO selected ID20 (flag 65591); final lookup first matches ID100's full flag55.
    assert_eq!(
        result.area.unwrap(),
        ResolvedArea {
            id: 100,
            map_id: 0,
            zone: 0,
            explore_flag: 55,
            liquid_override: 1,
        }
    );
    assert_eq!(result.parent_area, None);
}

#[test]
fn raw_terrain_between_rejection_is_strict_and_falls_back_to_last_wrong_map() {
    for (height, rejected) in [
        (6., false),
        (6f32.next_up(), true),
        (8., true),
        (9f32.next_down(), true),
        (9., false),
        (9f32.next_up(), false),
    ] {
        let products = products(height, 66);
        let result = result(&products, &dbcs());
        assert!(matches!(result.raw_area, AreaResult::Area { .. }));
        assert_eq!(result.area_info == AreaResult::None, rejected, "{height}");
        if rejected {
            assert_eq!(result.area_flag, 66);
            assert_eq!(result.area.unwrap().id, 9);
            assert_eq!(result.parent_area.unwrap().id, 200);
        } else {
            assert_eq!(result.area.unwrap().id, 100);
        }
    }
}

#[test]
fn exact_signed_triples_never_use_get_area_name_group_minus_one_fallback() {
    let products = products(0., 66);
    for field in 1..=3 {
        let mut different = wmo(1, 300);
        different[field] ^= 1;
        let mut fallback = wmo(2, 300);
        fallback[3] = u32::MAX;
        let dbcs = Dbcs {
            wmos: dbc(&[different, fallback], &[0]),
            ..dbcs()
        };
        let result = result(&products, &dbcs);
        assert_eq!(result.area_info, result.raw_area);
        assert_eq!(result.area_flag, 66);
        assert_eq!(result.area.unwrap().id, 9);
    }
    // uint16 ADT becomes a nonnegative int32; a sign-extended short is not its key.
    let mut row = wmo(1, 300);
    row[2] = 0xffff_8101;
    let dbcs = Dbcs {
        wmos: dbc(&[row], &[0]),
        ..dbcs()
    };
    assert_eq!(result(&products, &dbcs).area_flag, 66);
}

#[test]
fn zero_area_ids_and_linked_zone_zero_are_real_lookups_not_absence() {
    let products = products(0., 0);
    let mut inputs = dbcs();
    inputs.wmos = dbc(&[wmo(20, 0), wmo(1, 300)], &[0]);
    assert_eq!(result(&products, &inputs).area.unwrap().id, 300);
    inputs.areas = dbc(
        &[
            area(300, 0, 0, 77, 3),
            area(0, 0, 5, 22, 1),
            area(5, 0, 0, 33, 2),
        ],
        &[0],
    );
    let resolved = result(&products, &inputs);
    assert_eq!(resolved.area_flag, 22);
    assert_eq!(resolved.area.unwrap().id, 0);
    assert_eq!(resolved.parent_area.unwrap().id, 5);
    inputs.wmos = dbc::<20>(&[], &[0]);
    inputs.maps = dbc(&[map(0, 0)], &[0]);
    assert_eq!(result(&products, &inputs).area.unwrap().id, 0);
    inputs.areas = dbc(&[area(0, 0, 0, 0, 1)], &[0]);
    assert_eq!(result(&products, &inputs).area.unwrap().id, 0);
    inputs.areas = dbc::<25>(&[], &[0]);
    assert_eq!(result(&products, &inputs).area, None);
}

#[test]
fn zero_and_unmatched_flags_use_linked_zone_but_wrong_map_match_precedes_it() {
    for flag in [0, 444, 66] {
        let products = products(8., flag);
        let mut inputs = dbcs();
        for (maps, expected) in [
            (vec![map(0, 500)], Some(500)),
            (vec![map(0, 999)], None),
            (vec![], None),
        ] {
            inputs.maps = dbc(&maps, &[0]);
            let resolved = result(&products, &inputs);
            assert_eq!(
                resolved.area.map(|area| area.id),
                if flag == 66 { Some(9) } else { expected }
            );
        }
    }
    let products = products(0., 66);
    let inputs = Dbcs {
        areas: dbc(
            &[area(20, 0, 0, 0x1_0037, 2), area(500, 9, 0, 888, 4)],
            &[0],
        ),
        ..dbcs()
    };
    let resolved = result(&products, &inputs);
    assert_eq!(resolved.area_flag, 55);
    assert_eq!(resolved.area.unwrap().id, 500); // high flag does not match narrowed flag55
}

#[test]
fn all_complete_store_bytes_and_source_identity_are_bound_and_owned() {
    let products = products(0., 66);
    let mut inputs = dbcs();
    let snapshot = inputs.snapshot(&products);
    let query = request(&snapshot);
    let original = snapshot.query(query, deadline()).unwrap();
    assert_eq!(original.request, query);
    for field in 0..6 {
        let mut changed = query;
        match field {
            0 => changed.snapshot.map += 1,
            1 => changed.snapshot.content[0] ^= 1,
            2 => changed.snapshot.source_epoch[0] ^= 1,
            3 => changed.snapshot.inventory[0] ^= 1,
            4 => changed.snapshot.policy[0] ^= 1,
            _ => changed.ticket = 0,
        }
        assert_eq!(snapshot.query(changed, deadline()), Err(Error::Identity));
    }
    assert_ne!(snapshot.binding().policy, snapshot.raw.binding().policy);
    assert_ne!(
        snapshot.binding().policy,
        products.load_vmap_liquid().unwrap().binding().policy
    );
    for field in 0..3 {
        let mut changed = inputs.clone();
        let bytes = match field {
            0 => &mut changed.areas,
            1 => &mut changed.wmos,
            _ => &mut changed.maps,
        };
        // Ignored numeric field stays valid but still changes the complete-file hash.
        bytes[20 + 4 * 4] ^= 1;
        if field == 2 {
            bytes[20 + 4 * 4] ^= 1;
            bytes[20 + 3 * 4] ^= 1;
        }
        let other = changed.snapshot(&products);
        assert_ne!(snapshot.binding(), other.binding());
        assert_eq!(other.query(query, deadline()), Err(Error::Identity));
        assert_eq!(
            snapshot.query(
                TerrainAreaRequest {
                    snapshot: other.binding(),
                    ..query
                },
                deadline()
            ),
            Err(Error::Identity)
        );
    }
    inputs.areas.clear();
    inputs.wmos.clear();
    inputs.maps.clear();
    assert_eq!(snapshot.query(query, deadline()).unwrap(), original);
    for z in [0f32, -0f32, 7f32] {
        let query = TerrainAreaRequest {
            position: position([0., 0., z]),
            ticket: u64::MAX,
            ..query
        };
        assert_eq!(snapshot.query(query, deadline()).unwrap().request, query);
    }
}

#[test]
fn every_dbc_rejects_malformed_framing_strings_resource_limits_and_duplicate_ids() {
    let products = products(0., 66);
    for (kind, fields, locales) in [(0, 25, 11..=18), (1, 20, 11..=18), (2, 42, 4..=11)] {
        let row = vec![0u32; fields];
        let make = |rows: usize| {
            let mut bytes = b"WDBC".to_vec();
            for word in [rows as u32, fields as u32, (fields * 4) as u32, 1] {
                bytes.extend(word.to_le_bytes());
            }
            for _ in 0..rows {
                for word in &row {
                    bytes.extend(word.to_le_bytes());
                }
            }
            bytes.push(0);
            bytes
        };
        let original = make(1);
        let check = |bytes: &[u8]| {
            let mut inputs = dbcs();
            match kind {
                0 => inputs.areas = bytes.to_vec(),
                1 => inputs.wmos = bytes.to_vec(),
                _ => inputs.maps = bytes.to_vec(),
            }
            products
                .load_terrain_area(&inputs.areas, &inputs.wmos, &inputs.maps)
                .map(|_| ())
        };
        assert_eq!(check(&original), Ok(()));
        assert_eq!(check(&make(0)), Ok(()));
        for prefix in [0, 4, 19, 20, original.len() - 1] {
            assert!(check(&original[..prefix]).is_err());
        }
        assert_eq!(check(&make(2)), Err(Error::Duplicate));
        for (at, word, expected) in [
            (0, 0, Error::Format),
            (4, 65_537, Error::Budget),
            (16, 1_048_577, Error::Budget),
            (8, fields as u32 + 1, Error::Unsupported),
            (12, fields as u32 * 4 + 4, Error::Unsupported),
            (16, 0, Error::Framing),
            (20, u32::MAX, Error::Unsupported),
        ] {
            let mut bad = original.clone();
            bad[at..at + 4].copy_from_slice(&word.to_le_bytes());
            assert_eq!(check(&bad), Err(expected), "store {kind}, offset {at}");
        }
        for field in locales {
            let mut bad = original.clone();
            bad[20 + field * 4..24 + field * 4].copy_from_slice(&1u32.to_le_bytes());
            assert_eq!(
                check(&bad),
                Err(Error::Framing),
                "store {kind}, locale {field}"
            );
        }
        let mut trailing = original.clone();
        trailing.push(0);
        assert_eq!(check(&trailing), Err(Error::Framing));
        let mut no_nul = original.clone();
        *no_nul.last_mut().unwrap() = b'x';
        assert_eq!(check(&no_nul), Err(Error::Framing));
    }
    // Only this inclusive-index-scan profile adds the MAX-1 guard.
    let bytes = dbc(&[area(u32::MAX - 1, 0, 0, 7, 0)], &[0]);
    assert!(AreaTable::decode(&bytes).is_ok());
    let inputs = dbcs();
    assert!(matches!(
        products.load_terrain_area(&bytes, &inputs.wmos, &inputs.maps),
        Err(Error::Unsupported)
    ));
}

#[test]
fn signed_words_and_non_utf8_interior_strings_preserve_exact_complete_bytes() {
    let mut row = wmo(70_001, u32::MAX);
    row[1..=3].fill(u32::MAX);
    row[11..=18].fill(2);
    let bytes = dbc(&[row], &[0, b'a', 0xff, 0]);
    let store = WmoAreaTable::decode(&bytes).unwrap();
    let entry = store.ordered_entries().next().unwrap();
    assert_eq!(entry.triple, [-1; 3]);
    assert_eq!(entry.area_id, u32::MAX);
    assert_eq!(store.digest(), Hash::from(Sha256::digest(&bytes)));
    let mut row = map(70_001, u32::MAX);
    row[4..=11].fill(2);
    let bytes = dbc(&[row], &[0, b'a', 0xff, 0]);
    let store = MapTable::decode(&bytes).unwrap();
    assert_eq!(store.linked_zone(70_001), Some(u32::MAX));
    assert_eq!(store.digest(), Hash::from(Sha256::digest(&bytes)));
}

#[test]
fn one_budget_covers_geometry_terrain_all_store_scans_and_parent_lookup() {
    let products = products(8., 66);
    let inputs = dbcs();
    let snapshot = inputs.snapshot(&products);
    let query = request(&snapshot);
    let mut budget = QueryBudget::new(deadline());
    let expected = snapshot.query_with_budget(query, &mut budget).unwrap();
    let cost = vmap::MAX_QUERY_WORK - std::iter::from_fn(|| budget.take().ok()).count();
    assert!(cost > 10);
    for allowance in 0..cost {
        let mut budget = QueryBudget::new(deadline());
        for _ in allowance..vmap::MAX_QUERY_WORK {
            budget.take().unwrap();
        }
        assert_eq!(
            snapshot.query_with_budget(query, &mut budget),
            Err(Error::Budget),
            "{allowance}/{cost}"
        );
    }
    let mut budget = QueryBudget::new(deadline());
    for _ in cost..vmap::MAX_QUERY_WORK {
        budget.take().unwrap();
    }
    assert_eq!(
        snapshot.query_with_budget(query, &mut budget).unwrap(),
        expected
    );
    assert_eq!(budget.take(), Err(Error::Budget));
    assert_eq!(
        snapshot.query_with_budget(query, &mut budget),
        Err(Error::Budget)
    );
    assert_eq!(snapshot.query(query, Instant::now()), Err(Error::Deadline));
    // Exhaustion inside a complete DBC scan must not publish even raw area output.
    let rows: Vec<_> = (1..=vmap::MAX_QUERY_WORK as u32)
        .map(|id| area(id, 1, 0, 1, 0))
        .collect();
    let inputs = Dbcs {
        areas: dbc(&rows, &[0]),
        ..inputs
    };
    let large = inputs.snapshot(&products);
    assert_eq!(large.query(request(&large), deadline()), Err(Error::Budget));
}

#[test]
fn missing_terrain_is_not_source_null_grid_or_a_map_linked_zone_fallback() {
    let products = products(0., 66);
    let snapshot = dbcs().snapshot(&products);
    // Raw VMap has a valid no-hit outside the fixture, but this grid is uninventoried.
    let query = TerrainAreaRequest {
        position: [0f32.to_bits(); 3],
        ..request(&snapshot)
    };
    assert_eq!(snapshot.query(query, deadline()), Err(Error::Missing));
    for axis in 0..3 {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut query = request(&snapshot);
            query.position[axis] = value.to_bits();
            assert_eq!(snapshot.query(query, deadline()), Err(Error::Format));
        }
        let mut query = request(&snapshot);
        query.position[axis] = 100_001f32.to_bits();
        assert_eq!(snapshot.query(query, deadline()), Err(Error::Unsupported));
    }
}

#[test]
fn actual_filesystem_snapshot_is_epoch_bound_immutable_and_conservative_readers_stay_closed() {
    let (spec, files) = fixture(false, &flat(0., 66));
    let stage = product_tests::Stage::new(&files);
    let products = VmapLiquidProducts::load(&stage.0, spec.clone()).unwrap();
    let inputs = dbcs();
    let snapshot = inputs.snapshot(&products);
    let query = request(&snapshot);
    let expected = snapshot.query(query, deadline()).unwrap();
    let mut new_spec = spec.clone();
    new_spec.binding.source_epoch[0] ^= 1;
    let new_products = VmapLiquidProducts::load(&stage.0, new_spec).unwrap();
    let new_snapshot = inputs.snapshot(&new_products);
    assert_ne!(snapshot.binding(), new_snapshot.binding());
    assert_eq!(new_snapshot.query(query, deadline()), Err(Error::Identity));
    for closed in [
        products.inventory.load_static_vmap().map(|_| ()),
        products.inventory.load_static_height().map(|_| ()),
        products.inventory.load_dry_liquid().map(|_| ()),
        products.inventory.load_navigation().map(|_| ()),
    ] {
        assert_eq!(closed, Err(Error::Unsupported));
    }
    let path = stage.0.join("vmaps/wet.wmo.vmo");
    let mut changed = files["vmaps/wet.wmo.vmo"].clone();
    changed[16] ^= 1;
    std::fs::write(&path, changed).unwrap();
    assert!(matches!(
        VmapLiquidProducts::load(&stage.0, spec.clone()),
        Err(Error::Hash)
    ));
    std::fs::remove_file(path).unwrap();
    assert!(matches!(
        VmapLiquidProducts::load(&stage.0, spec),
        Err(Error::Missing)
    ));
    drop(stage);
    assert_eq!(snapshot.query(query, deadline()).unwrap(), expected);
    assert_eq!(
        inputs.snapshot(&products).query(query, deadline()).unwrap(),
        expected
    );
}

#[test]
fn corrupt_unloaded_or_malformed_geometry_never_becomes_an_area_fallback() {
    let (spec, files) = fixture(false, &flat(0., 66));
    let inputs = dbcs();
    for id in [
        ProductId::VmapTree,
        ProductId::VmapModel("wet.wmo.vmo".into()),
    ] {
        let mut bad = files.clone();
        bad.remove(&id.filename(0).unwrap());
        assert!(matches!(load(spec.clone(), &bad), Err(Error::Missing)));
        let mut bad = files.clone();
        bad.get_mut(&id.filename(0).unwrap()).unwrap()[0] ^= 1;
        assert!(matches!(load(spec.clone(), &bad), Err(Error::Hash)));
        let mut bad_spec = spec.clone();
        let mut bad = files.clone();
        product_tests::mutate(&mut bad_spec, &mut bad, &id, |bytes| {
            bytes.pop();
        });
        assert!(load(bad_spec, &bad)
            .and_then(|products| inputs.snapshot_result(&products))
            .is_err());
    }
    let mut spec = spec;
    let mut files = files;
    for tile in spec.tiles.clone() {
        let id = ProductId::VmapTile(tile);
        product_tests::mutate(&mut spec, &mut files, &id, |bytes| {
            bytes.truncate(12);
            bytes[8..12].fill(0);
        });
        spec.products
            .iter_mut()
            .find(|product| product.id == id)
            .unwrap()
            .dependencies = vec![ProductId::VmapTree];
    }
    spec.products
        .retain(|product| !matches!(product.id, ProductId::VmapModel(_)));
    let products = load(spec, &files).unwrap();
    assert_eq!(inputs.snapshot_result(&products), Err(Error::Missing));
}
impl Dbcs {
    fn snapshot_result(&self, products: &VmapLiquidProducts) -> Result<(), Error> {
        products
            .load_terrain_area(&self.areas, &self.wmos, &self.maps)
            .map(|_| ())
    }
}

#[test]
fn narrowed_zero_flag_uses_map_link_and_parent_resolution_is_one_id_lookup() {
    let products = products(0., 66);
    let mut inputs = Dbcs {
        areas: dbc(
            &[area(20, 0, 0, 0x1_0000, 2), area(500, 1, 999, 77, 0)],
            &[0],
        ),
        ..dbcs()
    };
    let resolved = result(&products, &inputs);
    assert_eq!(resolved.area_flag, 0);
    assert_eq!(resolved.area.unwrap().id, 500);
    assert_eq!(resolved.parent_area, None);
    // A self-cycle is one lookup, not recursion or a reason to invent absence.
    inputs.areas = dbc(
        &[area(20, 0, 0, 0x1_0000, 2), area(500, 1, 500, 77, 0)],
        &[0],
    );
    let resolved = result(&products, &inputs);
    assert_eq!(resolved.parent_area, resolved.area);
    // A grandparent with an override is outside this result's one-parent resolution.
    inputs.areas = dbc(
        &[
            area(20, 0, 0, 0x1_0000, 2),
            area(500, 1, 501, 77, 0),
            area(501, 1, 502, 78, 0),
            area(502, 1, 0, 79, 4),
        ],
        &[0],
    );
    let resolved = result(&products, &inputs);
    assert_eq!(resolved.parent_area.unwrap().id, 501);
    assert_eq!(resolved.parent_area.unwrap().liquid_override, 0);
}

#[test]
fn valid_geometry_and_terrain_changes_rebind_results_and_not_only_input_checks() {
    let (spec, files) = fixture(false, &flat(0., 66));
    let original_products = load(spec.clone(), &files).unwrap();
    let inputs = dbcs();
    let original = inputs.snapshot(&original_products);
    let query = request(&original);
    for id in [
        ProductId::Terrain(TileId { x: 0, y: 0 }),
        ProductId::VmapModel("low.wmo.vmo".into()),
    ] {
        let mut spec = spec.clone();
        let mut files = files.clone();
        product_tests::mutate(&mut spec, &mut files, &id, |bytes| {
            if matches!(id, ProductId::Terrain(_)) {
                bytes[56..60].copy_from_slice(&8f32.to_le_bytes());
                bytes[60..64].copy_from_slice(&8f32.to_le_bytes());
            } else {
                // Root ID changes only area metadata and its exact DBC triple lookup.
                bytes[16] ^= 1;
            }
        });
        let changed_products = load(spec, &files).unwrap();
        let changed = inputs.snapshot(&changed_products);
        assert_ne!(original.binding(), changed.binding());
        assert_eq!(changed.query(query, deadline()), Err(Error::Identity));
        assert_ne!(
            original.query(query, deadline()).unwrap().result,
            changed
                .query(
                    TerrainAreaRequest {
                        snapshot: changed.binding(),
                        ..query
                    },
                    deadline()
                )
                .unwrap()
                .result,
        );
    }
}

#[test]
fn public_reader_consumes_complete_filesystem_dbcs_and_survives_their_deletion() {
    let (spec, files) = fixture(false, &flat(0., 66));
    let stage = product_tests::Stage::new(&files);
    let products = VmapLiquidProducts::load(&stage.0, spec).unwrap();
    let inputs = dbcs();
    let dbc_files = BTreeMap::from([
        ("AreaTable.dbc".into(), inputs.areas),
        ("WMOAreaTable.dbc".into(), inputs.wmos),
        ("Map.dbc".into(), inputs.maps),
    ]);
    let dbc_stage = product_tests::Stage::new(&dbc_files);
    let read = |name: &str| std::fs::read(dbc_stage.0.join(name)).unwrap();
    let snapshot = products
        .load_terrain_area(
            &read("AreaTable.dbc"),
            &read("WMOAreaTable.dbc"),
            &read("Map.dbc"),
        )
        .unwrap();
    let query = request(&snapshot);
    let expected = snapshot.query(query, deadline()).unwrap();
    std::fs::write(dbc_stage.0.join("Map.dbc"), dbc::<42>(&[], &[0])).unwrap();
    let changed = products
        .load_terrain_area(
            &read("AreaTable.dbc"),
            &read("WMOAreaTable.dbc"),
            &read("Map.dbc"),
        )
        .unwrap();
    assert_ne!(snapshot.binding(), changed.binding());
    assert_eq!(changed.query(query, deadline()), Err(Error::Identity));
    drop(dbc_stage);
    drop(stage);
    assert_eq!(snapshot.query(query, deadline()).unwrap(), expected);
}

/// Original authored full-byte stores and terrain, independent of liquid status.
pub(crate) fn authored_source_location(
    map_id: u32,
    content: Hash,
    area_row: Option<(u32, u32)>,
) -> TerrainAreaSnapshot<'static> {
    let (mut spec, files) = fixture(false, &flat(0., 55));
    let old_map = spec.map;
    let old_tile = spec.tiles[0];
    let new_tile = TileId { x: 32, y: 32 };
    let remap = |id: ProductId| match id {
        ProductId::Terrain(tile) if tile == old_tile => ProductId::Terrain(new_tile),
        ProductId::VmapTile(tile) if tile == old_tile => ProductId::VmapTile(new_tile),
        ProductId::MmapTile(tile) if tile == old_tile => ProductId::MmapTile(new_tile),
        id => id,
    };
    spec.map = map_id;
    spec.binding.content = content;
    spec.tiles[0] = new_tile;
    let mut renamed = BTreeMap::new();
    for product in &mut spec.products {
        let bytes = files[&product.id.filename(old_map).unwrap()].clone();
        product.id = remap(product.id.clone());
        product.dependencies = product.dependencies.iter().cloned().map(remap).collect();
        renamed.insert(product.id.filename(map_id).unwrap(), bytes);
    }
    let products = Arc::new(load(spec, &renamed).unwrap());
    let rows: Vec<_> = area_row
        .map(|(id, zone)| area(id, map_id, zone, 55, 0))
        .into_iter()
        .collect();
    products
        .load_terrain_area_owned(
            &dbc(&rows, &[0]),
            &dbc::<20>(&[], &[0]),
            &dbc(&[map(map_id, 0)], &[0]),
        )
        .unwrap()
}

#[test]
fn get_zone_and_area_uses_row_zone_without_parent_lookup() {
    let products = products(8., 66);
    let inputs = Dbcs {
        areas: dbc(&[area(9, 7, 12, 66, 0)], &[0]),
        wmos: dbc::<20>(&[], &[0]),
        maps: dbc::<42>(&[], &[0]),
    };
    let snapshot = inputs.snapshot(&products);
    let query = request(&snapshot);
    let reply = snapshot.query_location(query, deadline()).unwrap();
    assert_eq!(reply.request, query);
    assert_eq!(reply.result.zone_id, 12);
    assert_eq!(reply.result.area_id, 9);
    assert_eq!(reply.result.area.unwrap().map_id, 7);
    assert_eq!(snapshot.area_by_id(12), None);
    let complete = snapshot.query(query, deadline()).unwrap();
    assert_eq!(complete.result.parent_area, None);
    assert_eq!(complete.result.area, reply.result.area);
}

#[test]
fn get_zone_and_area_preserves_real_zero_missing_and_zone_fallback() {
    let products = products(0., 0);
    for (rows, expected_row, zone, area_id) in [
        (vec![], None, 0, 0),
        (vec![area(0, 0, 0, 0, 0)], Some(0), 0, 0),
        (vec![area(0, 0, 12, 0, 0)], Some(0), 12, 0),
        (vec![area(9, 0, 0, 0, 0)], Some(9), 9, 9),
    ] {
        let inputs = Dbcs {
            areas: dbc(&rows, &[0]),
            wmos: dbc::<20>(&[], &[0]),
            maps: dbc(&[map(0, expected_row.unwrap_or(0))], &[0]),
        };
        let snapshot = inputs.snapshot(&products);
        let result = snapshot
            .query_location(request(&snapshot), deadline())
            .unwrap()
            .result;
        assert_eq!(result.area.map(|row| row.id), expected_row);
        assert_eq!((result.zone_id, result.area_id), (zone, area_id));
    }
}

#[test]
fn location_never_converts_missing_geometry_identity_or_deadline_to_zero() {
    let snapshot = authored_source_location(7, [3; 32], Some((9, 12)));
    let query = TerrainAreaRequest {
        snapshot: snapshot.binding(),
        ticket: 1,
        position: [0f32.to_bits(), 0f32.to_bits(), 7f32.to_bits()],
    };
    assert_eq!(
        snapshot.query_location(query, Instant::now()),
        Err(Error::Deadline)
    );
    for field in 0..6 {
        let mut changed = query;
        match field {
            0 => changed.snapshot.map += 1,
            1 => changed.snapshot.content[0] ^= 1,
            2 => changed.snapshot.source_epoch[0] ^= 1,
            3 => changed.snapshot.inventory[0] ^= 1,
            4 => changed.snapshot.policy[0] ^= 1,
            _ => changed.ticket = 0,
        }
        assert_eq!(
            snapshot.query_location(changed, deadline()),
            Err(Error::Identity)
        );
    }
    for position in [[0.125, 0., 7.], [f32::NAN, 0., 7.], [0., f32::INFINITY, 7.]] {
        assert!(snapshot
            .query_location(
                TerrainAreaRequest {
                    position: position.map(f32::to_bits),
                    ..query
                },
                deadline(),
            )
            .is_err());
    }
}
