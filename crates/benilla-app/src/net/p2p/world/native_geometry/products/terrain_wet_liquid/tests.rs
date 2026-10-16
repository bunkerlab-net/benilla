// SPDX-License-Identifier: GPL-3.0-or-later
// Authored native products and complete WDBC bytes; no installed assets.
use super::super::{
    tests as products, vmap, InventoriedProducts, InventorySpec, ProductId, TileId,
};
use super::*;
use std::{collections::BTreeMap, time::Duration};

pub(super) fn remap(
    mut spec: InventorySpec,
    mut files: BTreeMap<String, Vec<u8>>,
    tile: TileId,
) -> (InventorySpec, BTreeMap<String, Vec<u8>>) {
    let old = spec.tiles[0];
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
pub(super) fn fixture() -> (InventorySpec, BTreeMap<String, Vec<u8>>) {
    let (spec, files) = vmap::liquid_fixture();
    let (mut spec, mut files) = remap(spec, files, TileId { x: 0, y: 0 });
    products::mutate(
        &mut spec,
        &mut files,
        &ProductId::Terrain(TileId { x: 0, y: 0 }),
        |bytes| {
            bytes[46..48].copy_from_slice(&5u16.to_le_bytes());
            bytes[56..60].copy_from_slice(&(-10f32).to_le_bytes());
            bytes[60..64].copy_from_slice(&(-10f32).to_le_bytes());
            let at = u32::from_le_bytes(bytes[32..36].try_into().unwrap()) as usize;
            let mut liquid = b"MLIQ".to_vec();
            liquid.extend_from_slice(&[3, 8]);
            liquid.extend_from_slice(&1u16.to_le_bytes());
            liquid.extend_from_slice(&[0, 0, 129, 129]);
            liquid.extend_from_slice(&30f32.to_le_bytes());
            bytes.splice(at..at, liquid);
            bytes[24..28].copy_from_slice(&(at as u32).to_le_bytes());
            bytes[28..32].copy_from_slice(&16u32.to_le_bytes());
            bytes[32..36].copy_from_slice(&((at + 16) as u32).to_le_bytes());
        },
    );
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
fn dbc<const N: usize>(rows: &[[u32; N]]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for word in [rows.len() as u32, N as u32, N as u32 * 4, 1] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    for row in rows {
        for word in row {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
    }
    bytes.push(0);
    bytes
}
fn area(id: u32, zone: u32, flag: u32, liquid: u32) -> [u32; 25] {
    let mut row = [0; 25];
    row[0] = id;
    row[2] = zone;
    row[3] = flag;
    row[24] = liquid;
    row
}
#[derive(Clone)]
struct Dbcs {
    types: Vec<u8>,
    areas: Vec<u8>,
    wmos: Vec<u8>,
    maps: Vec<u8>,
}
impl Dbcs {
    fn empty() -> Self {
        Self {
            types: dbc::<4>(&[]),
            areas: dbc::<25>(&[]),
            wmos: dbc::<20>(&[]),
            maps: dbc::<42>(&[]),
        }
    }
    fn snapshot<'a>(
        &self,
        products: &'a VmapLiquidProducts,
    ) -> Result<TerrainLiquidSnapshot<'a>, Error> {
        products.load_terrain_wet_liquid(&self.types, &self.areas, &self.wmos, &self.maps)
    }
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
fn request(snapshot: &TerrainLiquidSnapshot<'_>) -> TerrainLiquidRequest {
    TerrainLiquidRequest {
        snapshot: snapshot.binding(),
        ticket: 1,
        // Source scopedliquid model0 tile2,0: actual level 6+ulp, floor near zero.
        position: [0x46854a95, 0x46855515, 0],
        required_type: 0,
        collision_height: 2.03128f32.to_bits(),
    }
}
fn result(products: &VmapLiquidProducts, stores: &Dbcs) -> Result<GridLiquidResult, Error> {
    let snapshot = stores.snapshot(products)?;
    Ok(snapshot.query(request(&snapshot), deadline())?.result)
}
fn wet_model(
    spec: &mut InventorySpec,
    files: &mut BTreeMap<String, Vec<u8>>,
    kind: u32,
    height: Option<f32>,
    disabled: bool,
) {
    products::mutate(
        spec,
        files,
        &ProductId::VmapModel("wet.wmo.vmo".into()),
        |bytes| {
            let at = bytes.windows(4).position(|x| x == b"LIQU").unwrap();
            bytes[at + 28..at + 32].copy_from_slice(&kind.to_le_bytes());
            let width = u32::from_le_bytes(bytes[at + 8..at + 12].try_into().unwrap()) as usize;
            let height_count =
                u32::from_le_bytes(bytes[at + 12..at + 16].try_into().unwrap()) as usize;
            let end = at + 32 + (width + 1) * (height_count + 1) * 4;
            if let Some(height) = height {
                for chunk in bytes[at + 32..end].as_chunks_mut::<4>().0 {
                    chunk.copy_from_slice(&height.to_le_bytes());
                }
            }
            if disabled {
                bytes[end..end + width * height_count].fill(15);
            }
        },
    );
}

#[test]
fn wet_override_runs_without_base_row_and_does_not_recheck_required_mask() {
    let (spec, files) = fixture();
    let products = load(spec, &files).unwrap();
    let stores = Dbcs {
        types: dbc(&[[70_000, 0, 2, 0]]),
        areas: dbc(&[
            area(100, 200, 5, 0),
            area(200, 300, 0, 70_000),
            area(300, 0, 0, 999),
        ]),
        ..Dbcs::empty()
    };
    let snapshot = stores.snapshot(&products).unwrap();
    for mask in 0..=255 {
        let query = TerrainLiquidRequest {
            required_type: mask,
            ..request(&snapshot)
        };
        let actual = snapshot.query(query, deadline()).unwrap().result;
        if mask == 0 || mask & 8 != 0 {
            let data = actual.data.unwrap();
            assert_eq!(data.entry, 70_000);
            assert_eq!(data.type_flags, 4);
            assert_eq!(data.level, 0x40c00001);
            assert_eq!(data.depth_level, 0x32000000);
        } else {
            // Raw GridMap also requires mask8, and no base row means no raw override.
            assert_eq!(actual, GridLiquidResult::no_water());
        }
    }
}

#[test]
fn one_parent_zero_override_and_nonzero_absent_override_are_distinct() {
    let (spec, files) = fixture();
    let products = load(spec, &files).unwrap();
    for (areas, expected) in [
        (
            vec![
                area(100, 200, 5, 0),
                area(200, 300, 0, 0),
                area(300, 0, 0, 22),
            ],
            0,
        ),
        (vec![area(100, 999, 5, 0)], 0),
        (vec![area(100, 200, 5, 777), area(200, 0, 0, 22)], 1),
        (vec![], 1),
    ] {
        let stores = Dbcs {
            types: dbc(&[[0, 0, 1, 0], [1, 0, 3, 0], [22, 0, 2, 0]]),
            areas: dbc(&areas),
            ..Dbcs::empty()
        };
        assert_eq!(
            result(&products, &stores).unwrap().data.unwrap().entry,
            expected
        );
    }
}

#[test]
fn wet_uint32_entries_flags_and_shift_execution_domain_are_preserved() {
    for (entry, kind, expected) in [
        (0, 31, Some(1u32 << 31)),
        (999, 8, Some(256)),
        (0x12345678, 30, Some(1u32 << 30)),
        (1, 32, None),
        (999, u32::MAX, None),
    ] {
        let (mut spec, mut files) = fixture();
        wet_model(&mut spec, &mut files, entry, None, false);
        let products = load(spec, &files).unwrap();
        let stores = Dbcs {
            types: dbc(&[[entry, 0, kind, 0], [22, 0, 1, 0]]),
            areas: dbc(&[area(100, 0, 5, 22)]),
            ..Dbcs::empty()
        };
        let actual = result(&products, &stores);
        // raw ID1 gets an override that avoids evaluating the invalid base shift.
        if entry == 1 {
            let data = actual.unwrap().data.unwrap();
            assert_eq!((data.entry, data.type_flags), (22, 2));
        } else if let Some(flags) = expected {
            let data = actual.unwrap().data.unwrap();
            assert_eq!((data.entry, data.type_flags), (entry, flags));
        } else {
            assert_eq!(actual, Err(Error::Unsupported));
        }
    }
    let (spec, files) = fixture();
    let products = load(spec, &files).unwrap();
    let stores = Dbcs {
        types: dbc(&[[999, 0, u32::MAX, 0]]),
        ..Dbcs::empty()
    };
    assert_eq!(
        result(&products, &stores).unwrap().data.unwrap().type_flags,
        1
    );
    let stores = Dbcs {
        types: dbc(&[[1, 0, 32, 0]]),
        ..Dbcs::empty()
    };
    assert_eq!(result(&products, &stores), Err(Error::Unsupported));
}

#[test]
fn wet_success_with_invalid_level_suppresses_raw_fallback() {
    let (mut spec, mut files) = fixture();
    wet_model(&mut spec, &mut files, 1, Some(-1.), false);
    let products = load(spec, &files).unwrap();
    let stores = Dbcs::empty();
    assert_eq!(
        result(&products, &stores).unwrap(),
        GridLiquidResult::no_water()
    );
    // The very same geometry with disabled liquid cells returns false and falls back.
    let (mut spec, mut files) = fixture();
    wet_model(&mut spec, &mut files, 1, Some(-1.), true);
    let products = load(spec, &files).unwrap();
    let data = result(&products, &stores).unwrap().data.unwrap();
    assert_eq!(data.level, 30f32.to_bits());
    assert_eq!(data.depth_level, (-10f32).to_bits());
}

#[test]
fn raw_fallback_keeps_uint32_override_and_narrows_only_flags() {
    let (mut spec, mut files) = fixture();
    wet_model(&mut spec, &mut files, 1, None, true);
    let products = load(spec, &files).unwrap();
    for (kind, flags) in [(3, Some(8)), (7, Some(128)), (8, None), (31, None)] {
        let stores = Dbcs {
            types: dbc(&[[1, 0, 3, 0], [70_000, 0, kind, 0]]),
            areas: dbc(&[area(5, 0, 999, 70_000)]),
            ..Dbcs::empty()
        };
        let actual = result(&products, &stores).unwrap();
        if let Some(flags) = flags {
            let data = actual.data.unwrap();
            assert_eq!((data.entry, data.type_flags), (70_000, flags));
            assert_eq!(data.depth_level, (-10f32).to_bits());
        } else {
            assert_eq!(actual, GridLiquidResult::no_water());
        }
    }
    let stores = Dbcs {
        types: dbc(&[[1, 0, 32, 0], [70_000, 0, 2, 0]]),
        areas: dbc(&[area(5, 0, 999, 70_000)]),
        ..Dbcs::empty()
    };
    assert_eq!(
        result(&products, &stores).unwrap().data.unwrap().entry,
        70_000
    );
}

#[test]
fn exact_wet_delta_and_collision_thresholds_are_strict() {
    let (spec, files) = fixture();
    let products = load(spec, &files).unwrap();
    let snapshot = Dbcs::empty().snapshot(&products).unwrap();
    let level = f32::from_bits(0x40c00001);
    for (z, collision, status) in [
        (level - 3., 2f32, GridLiquidStatus::UnderWater),
        (level - 2., 2., GridLiquidStatus::InWater),
        (level, 2., GridLiquidStatus::WaterWalk),
        (level + 0.5, 2., GridLiquidStatus::WaterWalk),
        (level + 1., 2., GridLiquidStatus::AboveWater),
        (level, -1., GridLiquidStatus::UnderWater),
    ] {
        let mut query = request(&snapshot);
        query.position[2] = z.to_bits();
        query.collision_height = collision.to_bits();
        assert_eq!(
            snapshot.query(query, deadline()).unwrap().result.status,
            status
        );
    }
}

#[test]
fn all_complete_dbc_bytes_and_source_bindings_are_identity_bound() {
    let (spec, files) = fixture();
    let products = load(spec, &files).unwrap();
    let stores = Dbcs::empty();
    let snapshot = stores.snapshot(&products).unwrap();
    let query = request(&snapshot);
    for which in 0..4 {
        let mut changed = stores.clone();
        let bytes = match which {
            0 => &mut changed.types,
            1 => &mut changed.areas,
            2 => &mut changed.wmos,
            _ => &mut changed.maps,
        };
        // Even unused string bytes are bound, never a row-subset digest.
        bytes[16..20].copy_from_slice(&2u32.to_le_bytes());
        bytes.push(0);
        let next = changed.snapshot(&products).unwrap();
        assert_ne!(next.binding(), snapshot.binding());
        assert_eq!(next.query(query, deadline()), Err(Error::Identity));
    }
    for bad in [query; 6].into_iter().enumerate().map(|(i, mut q)| {
        match i {
            0 => q.ticket = 0,
            1 => q.snapshot.map ^= 1,
            2 => q.snapshot.content[0] ^= 1,
            3 => q.snapshot.source_epoch[0] ^= 1,
            4 => q.snapshot.inventory[0] ^= 1,
            _ => q.snapshot.policy[0] ^= 1,
        };
        q
    }) {
        assert_eq!(snapshot.query(bad, deadline()), Err(Error::Identity));
    }
}

#[test]
fn missing_or_malformed_stores_are_never_empty_and_old_profile_is_unchanged() {
    let (spec, files) = fixture();
    let products = load(spec, &files).unwrap();
    let stores = Dbcs::empty();
    for which in 0..4 {
        let original = match which {
            0 => &stores.types,
            1 => &stores.areas,
            2 => &stores.wmos,
            _ => &stores.maps,
        };
        for size in 0..original.len() {
            let mut broken = stores.clone();
            let target = match which {
                0 => &mut broken.types,
                1 => &mut broken.areas,
                2 => &mut broken.wmos,
                _ => &mut broken.maps,
            };
            target.truncate(size);
            assert!(
                broken.snapshot(&products).is_err(),
                "store{which} prefix{size}"
            );
        }
    }
    for bytes in [dbc(&[[70_000, 0, 1, 0]]), dbc(&[[1, 0, 8, 0]])] {
        assert!(matches!(
            LiquidTypes::decode(&bytes),
            Err(Error::Unsupported)
        ));
        assert!(LiquidTypes::decode_complete(&bytes).is_ok());
    }
    assert!(matches!(
        LiquidTypes::decode_complete(&dbc(&[[u32::MAX, 0, 0, 0]])),
        Err(Error::Unsupported)
    ));
    assert!(matches!(
        LiquidTypes::decode_complete(&dbc(&[[1, 0, 0, 0], [1, 0, 1, 0]])),
        Err(Error::Duplicate)
    ));
}

#[test]
fn every_budget_cut_rejects_without_partial_output_in_wet_and_fallback_paths() {
    for fallback in [false, true] {
        let (mut spec, mut files) = fixture();
        if fallback {
            wet_model(&mut spec, &mut files, 1, None, true);
        }
        let products = load(spec, &files).unwrap();
        let stores = Dbcs {
            types: dbc(&[[1, 0, 3, 0], [22, 0, 2, 0]]),
            areas: dbc(&[
                area(100, 200, 5, 0),
                area(200, 0, 0, 22),
                area(5, 200, 999, 0),
            ]),
            ..Dbcs::empty()
        };
        let snapshot = stores.snapshot(&products).unwrap();
        let query = request(&snapshot);
        let mut budget = QueryBudget::new(deadline());
        let expected = snapshot.query_with_budget(query, &mut budget).unwrap();
        let cost = vmap::MAX_QUERY_WORK - std::iter::from_fn(|| budget.take().ok()).count();
        assert!(cost > 10);
        for allowance in 0..=cost {
            let mut budget = QueryBudget::new(deadline());
            for _ in allowance..vmap::MAX_QUERY_WORK {
                budget.take().unwrap();
            }
            let actual = snapshot.query_with_budget(query, &mut budget);
            if allowance == cost {
                assert_eq!(actual, Ok(expected));
                assert_eq!(budget.take(), Err(Error::Budget));
            } else {
                assert_eq!(
                    actual,
                    Err(Error::Budget),
                    "fallback{fallback} cut{allowance}"
                );
            }
        }
        assert_eq!(snapshot.query(query, Instant::now()), Err(Error::Deadline));
    }
}

#[test]
fn filesystem_vertical_slice_is_immutable_hash_bound_and_old_readers_stay_closed() {
    let (spec, files) = fixture();
    let stage = products::Stage::new(&files);
    let products = VmapLiquidProducts::load(&stage.0, spec.clone()).unwrap();
    let stores = Dbcs::empty();
    let snapshot = stores.snapshot(&products).unwrap();
    let query = request(&snapshot);
    let expected = snapshot.query(query, deadline()).unwrap();
    assert_eq!(expected.result.data.unwrap().entry, 1);
    let mut epoch = spec.clone();
    epoch.binding.source_epoch[0] ^= 1;
    let next = VmapLiquidProducts::load(&stage.0, epoch).unwrap();
    let next = stores.snapshot(&next).unwrap();
    assert_ne!(snapshot.binding(), next.binding());
    assert_eq!(next.query(query, deadline()), Err(Error::Identity));
    for closed in [
        products.inventory.load_static_vmap().map(|_| ()),
        products.inventory.load_static_height().map(|_| ()),
        products.inventory.load_dry_liquid().map(|_| ()),
        products.inventory.load_navigation().map(|_| ()),
        products
            .inventory
            .load_terrain_liquid(&stores.types)
            .map(|_| ()),
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
}

#[test]
fn missing_geometry_invalid_inputs_and_uninventoried_terrain_fail_closed() {
    let (spec, files) = fixture();
    let stores = Dbcs::empty();
    for id in [
        ProductId::VmapTree,
        ProductId::VmapModel("wet.wmo.vmo".into()),
    ] {
        let mut broken = files.clone();
        broken.remove(&id.filename(0).unwrap());
        assert!(matches!(load(spec.clone(), &broken), Err(Error::Missing)));
        let mut broken = files.clone();
        broken.get_mut(&id.filename(0).unwrap()).unwrap()[0] ^= 1;
        assert!(matches!(load(spec.clone(), &broken), Err(Error::Hash)));
        let mut broken = files.clone();
        let mut spec = spec.clone();
        products::mutate(&mut spec, &mut broken, &id, |bytes| {
            bytes.pop();
        });
        assert!(load(spec, &broken)
            .and_then(|p| stores.snapshot(&p).map(|_| ()))
            .is_err());
    }
    let products = load(spec, &files).unwrap();
    let snapshot = stores.snapshot(&products).unwrap();
    let mut query = request(&snapshot);
    query.position = [0; 3];
    assert_eq!(snapshot.query(query, deadline()), Err(Error::Missing));
    for i in 0..4 {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut query = request(&snapshot);
            if i < 3 {
                query.position[i] = value.to_bits();
            } else {
                query.collision_height = value.to_bits();
            }
            assert_eq!(snapshot.query(query, deadline()), Err(Error::Format));
        }
    }
}

#[test]
fn valid_changed_geometry_and_terrain_bytes_cannot_reuse_a_previous_request() {
    let (spec, files) = fixture();
    let products = load(spec.clone(), &files).unwrap();
    let stores = Dbcs::empty();
    let snapshot = stores.snapshot(&products).unwrap();
    let query = request(&snapshot);
    for change_terrain in [false, true] {
        let mut spec = spec.clone();
        let mut files = files.clone();
        if change_terrain {
            products::mutate(
                &mut spec,
                &mut files,
                &ProductId::Terrain(TileId { x: 0, y: 0 }),
                |bytes| {
                    bytes[46..48].copy_from_slice(&6u16.to_le_bytes());
                },
            );
        } else {
            wet_model(&mut spec, &mut files, 999, None, false);
        }
        let changed = load(spec, &files).unwrap();
        let changed = stores.snapshot(&changed).unwrap();
        assert_ne!(changed.binding(), snapshot.binding());
        assert_eq!(changed.query(query, deadline()), Err(Error::Identity));
        assert!(changed.query(request(&changed), deadline()).is_ok());
    }
}

#[test]
fn unloaded_static_tree_slots_cannot_be_treated_as_dry_or_missing_water() {
    let (mut spec, mut files) = fixture();
    for tile in spec.tiles.clone() {
        let id = ProductId::VmapTile(tile);
        products::mutate(&mut spec, &mut files, &id, |bytes| {
            bytes.truncate(12);
            bytes[8..12].fill(0);
        });
        spec.products
            .iter_mut()
            .find(|product| product.id == id)
            .unwrap()
            .dependencies = vec![ProductId::VmapTree];
    }
    let names: Vec<_> = spec
        .products
        .iter()
        .filter(|product| matches!(product.id, ProductId::VmapModel(_)))
        .map(|product| product.id.filename(spec.map).unwrap())
        .collect();
    for name in names {
        files.remove(&name);
    }
    spec.products
        .retain(|product| !matches!(product.id, ProductId::VmapModel(_)));
    let products = load(spec, &files).unwrap();
    assert!(matches!(
        Dbcs::empty().snapshot(&products),
        Err(Error::Missing)
    ));
}

/// Real loaded products and complete authored WDBC stores for mailbox/controller tests.
pub(crate) fn authored_player_environment(
    map: u32,
    content: Hash,
) -> TerrainLiquidSnapshot<'static> {
    authored_player_environment_with_spell(map, content, 0)
}
pub(crate) fn authored_player_environment_with_spell(
    map: u32,
    content: Hash,
    spell: u32,
) -> TerrainLiquidSnapshot<'static> {
    let (spec, files) = fixture();
    let (mut spec, files) = remap(spec, files, TileId { x: 32, y: 32 });
    let old_map = spec.map;
    spec.map = map;
    spec.binding.content = content;
    let files = spec
        .products
        .iter()
        .map(|p| {
            (
                p.id.filename(map).unwrap(),
                files[&p.id.filename(old_map).unwrap()].clone(),
            )
        })
        .collect();
    let stage = products::Stage::new(&files);
    let products = Arc::new(VmapLiquidProducts::load(&stage.0, spec).unwrap());
    drop(stage);
    products
        .load_terrain_wet_liquid_owned(
            &dbc(&[[1, 0, 3, spell]]),
            &dbc::<25>(&[]),
            &dbc::<20>(&[]),
            &dbc::<42>(&[]),
        )
        .unwrap()
}
