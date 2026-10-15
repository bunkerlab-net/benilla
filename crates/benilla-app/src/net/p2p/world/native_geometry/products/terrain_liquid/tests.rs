// SPDX-License-Identifier: GPL-3.0-or-later
// Authored native products and complete DBC bytes only; no installed assets.
use super::super::{tests as products, vmap, InventorySpec, PointHeight, ProductId, TileId};
use super::*;
use std::{collections::BTreeMap, time::Duration};

pub(super) fn fixture() -> (InventorySpec, BTreeMap<String, Vec<u8>>) {
    let (mut spec, mut files) = vmap::location_fixture();
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

fn dbc(rows: &[(u32, u32)]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for word in [rows.len() as u32, 4, 16, 1] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    for &(id, kind) in rows {
        for word in [id, 23, kind, 0] {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
    }
    bytes.push(0);
    bytes
}

fn area_dbc(rows: &[(u32, u32, u32)]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for word in [rows.len() as u32, 25, 100, 1] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    for &(id, zone, liquid_override) in rows {
        let mut row = [0u32; 25];
        row[0] = id;
        row[1] = 999;
        row[2] = zone;
        row[3] = id + 100;
        row[24] = liquid_override;
        for word in row {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
    }
    bytes.push(0);
    bytes
}

fn wet_terrain(bytes: &mut Vec<u8>, level: f32, entry: u16) {
    let at = u32::from_le_bytes(bytes[32..36].try_into().unwrap()) as usize;
    let mut liquid = b"MLIQ".to_vec();
    liquid.extend_from_slice(&[3, 8]);
    liquid.extend_from_slice(&entry.to_le_bytes());
    liquid.extend_from_slice(&[0, 0, 129, 129]);
    liquid.extend_from_slice(&level.to_le_bytes());
    bytes.splice(at..at, liquid);
    bytes[24..28].copy_from_slice(&(at as u32).to_le_bytes());
    bytes[28..32].copy_from_slice(&16u32.to_le_bytes());
    bytes[32..36].copy_from_slice(&((at + 16) as u32).to_le_bytes());
}

fn inputs(ground: f32, level: f32, entry: u16) -> (InventorySpec, BTreeMap<String, Vec<u8>>) {
    let (mut spec, mut files) = fixture();
    products::mutate(
        &mut spec,
        &mut files,
        &ProductId::Terrain(TileId { x: 0, y: 0 }),
        |bytes| {
            bytes[56..60].copy_from_slice(&ground.to_le_bytes());
            bytes[60..64].copy_from_slice(&ground.to_le_bytes());
            bytes[46..48].copy_from_slice(&5u16.to_le_bytes());
            wet_terrain(bytes, level, entry);
        },
    );
    (spec, files)
}

fn wet_products(ground: f32, level: f32, entry: u16) -> InventoriedProducts {
    let (spec, files) = inputs(ground, level, entry);
    products::load(spec, &files).unwrap()
}

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

fn position(internal: [f32; 3]) -> [u32; 3] {
    let mid = 0.5f32 * (64.0 * super::super::terrain::GRID_SIZE);
    [mid - internal[0], mid - internal[1], internal[2]].map(f32::to_bits)
}

fn request(snapshot: &TerrainLiquidSnapshot<'_>) -> TerrainLiquidRequest {
    TerrainLiquidRequest {
        snapshot: snapshot.binding(),
        ticket: 1,
        position: position([0., 0., 5.]),
        required_type: 8,
        collision_height: 2f32.to_bits(),
    }
}

#[test]
fn dry_location_replaces_static_ceiling_but_keeps_raw_gridmap_depth() {
    let products = wet_products(0., 5., 21);
    let snapshot = products.load_terrain_liquid(&dbc(&[(21, 3)])).unwrap();
    let request = request(&snapshot);
    let height = products.load_static_height().unwrap();
    let height_request = height::HeightRequest {
        snapshot: height.binding(),
        ticket: request.ticket,
        position: request.position,
        max_search: DEFAULT_WATER_SEARCH.to_bits(),
    };
    assert_eq!(
        height.query(height_request, deadline()).unwrap().height,
        6f32.to_bits()
    );
    // The factor preserves the existing standalone height policy byte-for-byte.
    let mut policy = Sha256::new();
    policy.update(b"cmangos-8ec338a1-static-height-enabled-complete-dry-vmap-v1");
    policy.update(snapshot.vmap.binding().policy);
    assert_eq!(height.binding().policy, Hash::from(policy.finalize()));
    let mut budget = QueryBudget::new(deadline());
    let StaticLocation::Floor(floor) = snapshot
        .vmap
        .location(request.position.map(f32::from_bits), &mut budget)
        .unwrap()
    else {
        panic!("authored WMO must contain this point");
    };
    assert_eq!(floor.floor, 4f32.to_bits());
    let result = snapshot.query(request, deadline()).unwrap();
    assert_eq!(result.request, request);
    assert_eq!(result.result.status, GridLiquidStatus::WaterWalk);
    let data = result.result.data.unwrap();
    assert_eq!((data.entry, data.type_flags), (21, 8));
    assert_eq!(data.level, 5f32.to_bits());
    assert_eq!(data.depth_level, 0f32.to_bits());
}

#[test]
fn shared_height_helper_rejects_mixed_source_geometry() {
    let products = wet_products(0., 5., 21);
    let snapshot = products.load_terrain_liquid(&dbc(&[(21, 3)])).unwrap();
    let p = request(&snapshot).position.map(f32::from_bits);
    let (mut spec, files) = inputs(0., 5., 21);
    spec.binding.source_epoch[0] ^= 1;
    let changed = products::load(spec, &files).unwrap();
    let mut budget = QueryBudget::new(deadline());
    assert_eq!(
        height::height(
            &changed,
            &snapshot.vmap,
            p,
            DEFAULT_WATER_SEARCH,
            &mut budget
        ),
        Err(Error::Identity)
    );
}

#[test]
fn outer_selection_is_strict_and_never_publishes_suppressed_data() {
    for (level, expected) in [
        (4f32.next_down(), GridLiquidStatus::NoWater),
        (4., GridLiquidStatus::NoWater),
        (4f32.next_up(), GridLiquidStatus::WaterWalk),
        (5., GridLiquidStatus::WaterWalk),
        (5f32.next_up(), GridLiquidStatus::InWater),
        (7., GridLiquidStatus::InWater),
        (7f32.next_up(), GridLiquidStatus::UnderWater),
    ] {
        let products = wet_products(0., level, 21);
        let snapshot = products.load_terrain_liquid(&dbc(&[(21, 3)])).unwrap();
        let query = request(&snapshot);
        let result = snapshot.query(query, deadline()).unwrap().result;
        assert_eq!(result.status, expected, "level={level}");
        assert_eq!(result.data.is_some(), expected != GridLiquidStatus::NoWater);
        if let Some(data) = result.data {
            assert_eq!(data.level, level.to_bits());
            assert_eq!(data.depth_level, 0f32.to_bits());
        }
    }
}

#[test]
fn no_location_retains_static_ground_and_raw_status_including_above_water() {
    for (level, expected) in [
        (0., GridLiquidStatus::NoWater),
        (1., GridLiquidStatus::AboveWater),
    ] {
        let products = wet_products(0., level, 21);
        let snapshot = products.load_terrain_liquid(&dbc(&[(21, 3)])).unwrap();
        let query = TerrainLiquidRequest {
            position: position([100., 100., 5.]),
            ..request(&snapshot)
        };
        let mut budget = QueryBudget::new(deadline());
        assert_eq!(
            snapshot
                .vmap
                .location(query.position.map(f32::from_bits), &mut budget),
            Ok(StaticLocation::None)
        );
        let result = snapshot.query(query, deadline()).unwrap().result;
        assert_eq!(result.status, expected);
        assert_eq!(result.data.is_some(), expected != GridLiquidStatus::NoWater);
    }
}

#[test]
fn actual_float_hole_preserves_source_invalid_depth_after_location_selection() {
    let (mut spec, mut files) = fixture();
    let tile = TileId { x: 0, y: 0 };
    products::mutate(&mut spec, &mut files, &ProductId::Terrain(tile), |bytes| {
        *bytes = products::terrain_bytes(6);
        wet_terrain(bytes, 5., 21);
    });
    let products = products::load(spec, &files).unwrap();
    let snapshot = products.load_terrain_liquid(&dbc(&[(21, 3)])).unwrap();
    let query = request(&snapshot);
    let p = query.position.map(f32::from_bits);
    assert_eq!(
        products.terrain(0, tile).unwrap().point_height(p[0], p[1]),
        Ok(PointHeight::Hole)
    );
    let result = snapshot.query(query, deadline()).unwrap().result;
    assert_eq!(result.status, GridLiquidStatus::WaterWalk);
    assert_eq!(result.data.unwrap().depth_level, (-200_000f32).to_bits());
    // An invalid map height does not extend the first search beyond the local
    // distance bound, unlike the valid-height case below. Both bounded rays miss.
    let high = TerrainLiquidRequest {
        position: position([0., 0., 20_000.]),
        ..query
    };
    assert_eq!(
        snapshot.query(high, deadline()).unwrap().result.status,
        GridLiquidStatus::AboveWater
    );
}

#[test]
fn dry_gridmap_does_not_bypass_source_static_height_or_missing_coverage() {
    let (spec, files) = fixture();
    let products = products::load(spec, &files).unwrap();
    let snapshot = products.load_terrain_liquid(&dbc(&[])).unwrap();
    let query = TerrainLiquidRequest {
        position: position([0., 0., 20_000.]),
        ..request(&snapshot)
    };
    assert_eq!(snapshot.query(query, deadline()), Err(Error::Unsupported));
    let query = TerrainLiquidRequest {
        position: [0f32, 0., 5.].map(f32::to_bits),
        ..query
    };
    assert_eq!(snapshot.query(query, deadline()), Err(Error::Missing));
    assert_eq!(
        snapshot
            .query(request(&snapshot), deadline())
            .unwrap()
            .result,
        GridLiquidResult {
            status: GridLiquidStatus::NoWater,
            data: None
        }
    );
}

#[test]
fn raw_gridmap_dependencies_execute_before_outer_level_suppression() {
    let products = wet_products(0., 4., 1);
    let types = dbc(&[(1, 3), (22, 0), (23, 2)]);
    let without = products.load_terrain_liquid(&types).unwrap();
    assert_eq!(
        without.query(request(&without), deadline()),
        Err(Error::Unsupported)
    );
    // Even a mask that could reject the water does not skip source area lookup.
    let masked = TerrainLiquidRequest {
        required_type: 0x80,
        ..request(&without)
    };
    assert_eq!(without.query(masked, deadline()), Err(Error::Unsupported));
    let empty = products
        .load_terrain_liquid_with_areas(&types, &area_dbc(&[]))
        .unwrap();
    assert_eq!(
        empty.query(request(&empty), deadline()).unwrap().result,
        GridLiquidResult {
            status: GridLiquidStatus::NoWater,
            data: None
        }
    );
    // Missing IDs in a complete LiquidType table follow raw flags without areas.
    let missing_row = products.load_terrain_liquid(&dbc(&[(21, 3)])).unwrap();
    assert_eq!(
        missing_row
            .query(request(&missing_row), deadline())
            .unwrap()
            .result
            .status,
        GridLiquidStatus::NoWater
    );
}

#[test]
fn exact_area_override_mask_and_collision_height_survive_composition() {
    let products = wet_products(0., 5., 1);
    let types = dbc(&[(1, 3), (22, 0), (23, 2)]);
    for (rows, entry, mask) in [
        (vec![(5, 9, 0), (9, 0, 22)], 22, 1),
        (vec![(5, 9, 23), (9, 0, 22)], 23, 4),
    ] {
        let snapshot = products
            .load_terrain_liquid_with_areas(&types, &area_dbc(&rows))
            .unwrap();
        let query = TerrainLiquidRequest {
            required_type: mask,
            collision_height: (-0.5f32).to_bits(),
            ..request(&snapshot)
        };
        let reply = snapshot.query(query, deadline()).unwrap();
        assert_eq!(reply.request, query);
        assert_eq!(reply.result.status, GridLiquidStatus::UnderWater);
        let data = reply.result.data.unwrap();
        assert_eq!(
            (data.entry, data.type_flags, data.depth_level),
            (entry, u32::from(mask), 0f32.to_bits())
        );
        let rejected = TerrainLiquidRequest {
            required_type: 8,
            ..query
        };
        assert_eq!(
            snapshot.query(rejected, deadline()).unwrap().result,
            GridLiquidResult {
                status: GridLiquidStatus::NoWater,
                data: None
            }
        );
    }
}

#[test]
fn binding_and_echo_cover_every_snapshot_and_query_input() {
    let products = wet_products(0., 5., 21);
    let snapshot = products.load_terrain_liquid(&dbc(&[(21, 3)])).unwrap();
    let original = request(&snapshot);
    for field in 0..6 {
        let mut query = original;
        match field {
            0 => query.snapshot.map += 1,
            1 => query.snapshot.content[0] ^= 1,
            2 => query.snapshot.source_epoch[0] ^= 1,
            3 => query.snapshot.inventory[0] ^= 1,
            4 => query.snapshot.policy[0] ^= 1,
            _ => query.ticket = 0,
        }
        assert_eq!(snapshot.query(query, deadline()), Err(Error::Identity));
    }
    for field in 0..6 {
        let mut query = original;
        match field {
            0..=2 => {
                query.position[field] = f32::from_bits(query.position[field]).next_down().to_bits()
            }
            3 => query.required_type = 0,
            4 => query.collision_height = (-0f32).to_bits(),
            _ => query.ticket += 1,
        }
        let reply = snapshot.query(query, deadline()).unwrap();
        assert_eq!(reply.request, query);
        assert_ne!(reply.request, original);
    }
    assert_eq!(snapshot.query(original, Instant::now()), Err(Error::Budget));
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for field in 0..4 {
            let mut query = original;
            if field < 3 {
                query.position[field] = bad.to_bits();
            } else {
                query.collision_height = bad.to_bits();
            }
            assert_eq!(snapshot.query(query, deadline()), Err(Error::Format));
        }
    }
    let query = TerrainLiquidRequest {
        collision_height: 1_000_001f32.to_bits(),
        ..original
    };
    assert_eq!(snapshot.query(query, deadline()), Err(Error::Format));
}

#[test]
fn all_phases_share_one_nonrenewing_work_budget_and_deadline() {
    let products = wet_products(0., 5., 21);
    let snapshot = products.load_terrain_liquid(&dbc(&[(21, 3)])).unwrap();
    let query = request(&snapshot);
    let p = query.position.map(f32::from_bits);
    let mut height_budget = QueryBudget::new(deadline());
    height::height(
        &products,
        &snapshot.vmap,
        p,
        DEFAULT_WATER_SEARCH,
        &mut height_budget,
    )
    .unwrap();
    let remaining = std::iter::from_fn(|| height_budget.take().ok()).count();
    let height_cost = vmap::MAX_QUERY_WORK - remaining;
    let mut location_budget = QueryBudget::new(deadline());
    snapshot.vmap.location(p, &mut location_budget).unwrap();
    let remaining = std::iter::from_fn(|| location_budget.take().ok()).count();
    let location_cost = vmap::MAX_QUERY_WORK - remaining;
    let total = 1 + height_cost + location_cost + 2;
    // Fail during height, during location, before GridMap, and before final selection.
    for allowance in [
        1,
        1 + height_cost,
        1 + height_cost + location_cost,
        total - 1,
    ] {
        let mut budget = QueryBudget::new(deadline());
        for _ in 0..vmap::MAX_QUERY_WORK - allowance {
            budget.take().unwrap();
        }
        assert_eq!(
            snapshot.query_with_budget(query, &mut budget),
            Err(Error::Budget)
        );
    }
    let mut budget = QueryBudget::new(deadline());
    for _ in 0..vmap::MAX_QUERY_WORK - total {
        budget.take().unwrap();
    }
    assert_eq!(
        snapshot.query_with_budget(query, &mut budget).unwrap(),
        snapshot.query(query, deadline()).unwrap()
    );
    assert_eq!(budget.take(), Err(Error::Budget));
    // A new phase cannot replace the absolute deadline after work has begun.
    let expired = Instant::now();
    let mut budget = QueryBudget::new(expired);
    assert_eq!(
        snapshot.query_with_budget(query, &mut budget),
        Err(Error::Budget)
    );
}

#[test]
fn complete_dry_vmap_and_actual_dbc_bytes_are_required_before_publication() {
    let (mut spec, mut files) = fixture();
    for tile in spec.tiles.clone() {
        products::mutate(&mut spec, &mut files, &ProductId::VmapTile(tile), |bytes| {
            bytes.truncate(12);
            bytes[8..12].copy_from_slice(&0u32.to_le_bytes());
        });
        spec.products
            .iter_mut()
            .find(|p| p.id == ProductId::VmapTile(tile))
            .unwrap()
            .dependencies = vec![ProductId::VmapTree];
    }
    spec.products
        .retain(|p| !matches!(p.id, ProductId::VmapModel(_)));
    let products = products::load(spec, &files).unwrap();
    assert!(matches!(
        products.load_terrain_liquid(&dbc(&[])),
        Err(Error::Missing)
    ));
    let (mut spec, mut files) = fixture();
    products::mutate(
        &mut spec,
        &mut files,
        &ProductId::VmapModel("volume.wmo.vmo".into()),
        |bytes| {
            let at = bytes.windows(4).position(|p| p == b"LIQU").unwrap();
            bytes[at + 4..at + 8].copy_from_slice(&1u32.to_le_bytes());
        },
    );
    assert!(products::load(spec, &files)
        .and_then(|p| p.load_terrain_liquid(&dbc(&[])).map(|_| ()))
        .is_err());
    let products = wet_products(0., 5., 21);
    let types = dbc(&[(21, 3)]);
    let areas = area_dbc(&[(5, 0, 21)]);
    for end in 0..types.len() {
        assert!(products.load_terrain_liquid(&types[..end]).is_err());
    }
    for end in 0..areas.len() {
        assert!(products
            .load_terrain_liquid_with_areas(&types, &areas[..end])
            .is_err());
    }
}

#[test]
fn source_replacements_and_dbc_changes_cannot_relabel_existing_replies() {
    let (spec, mut files) = inputs(0., 5., 21);
    let products = products::load(spec.clone(), &files).unwrap();
    let types = dbc(&[(21, 3)]);
    let areas = area_dbc(&[(5, 0, 21)]);
    let old = products
        .load_terrain_liquid_with_areas(&types, &areas)
        .unwrap();
    let query = request(&old);
    let reply = old.query(query, deadline()).unwrap();
    let changed_types = products
        .load_terrain_liquid_with_areas(&dbc(&[(21, 2)]), &areas)
        .unwrap();
    assert_eq!(changed_types.query(query, deadline()), Err(Error::Identity));
    let mut unused = areas.clone();
    unused[24..28].copy_from_slice(&123u32.to_le_bytes());
    let changed_area = products
        .load_terrain_liquid_with_areas(&types, &unused)
        .unwrap();
    assert_eq!(changed_area.query(query, deadline()), Err(Error::Identity));
    let no_area = products.load_terrain_liquid(&types).unwrap();
    assert_eq!(no_area.query(query, deadline()), Err(Error::Identity));
    let mut changed_spec = spec;
    changed_spec.binding.source_epoch[0] ^= 1;
    products::mutate(
        &mut changed_spec,
        &mut files,
        &ProductId::Terrain(TileId { x: 0, y: 0 }),
        |bytes| {
            let at = u32::from_le_bytes(bytes[24..28].try_into().unwrap()) as usize;
            bytes[at + 12..at + 16].copy_from_slice(&4f32.to_le_bytes());
        },
    );
    let changed_products = products::load(changed_spec, &files).unwrap();
    let changed = changed_products
        .load_terrain_liquid_with_areas(&types, &areas)
        .unwrap();
    assert_eq!(changed.query(query, deadline()), Err(Error::Identity));
    assert_ne!(changed.binding().inventory, old.binding().inventory);
    assert_ne!(changed.binding().source_epoch, old.binding().source_epoch);
    files.values_mut().for_each(Vec::clear);
    assert_eq!(old.query(query, deadline()).unwrap(), reply);
    assert_eq!(
        changed
            .query(request(&changed), deadline())
            .unwrap()
            .result
            .status,
        GridLiquidStatus::NoWater
    );
}
