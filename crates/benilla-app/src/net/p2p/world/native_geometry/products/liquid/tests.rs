use super::super::{tests as products, vmap::tests as vmap};
use super::*;
use std::time::Duration;

fn snapshot() -> DryLiquidSnapshot {
    let (spec, files) = vmap::fixture();
    products::load(spec, &files)
        .unwrap()
        .load_dry_liquid()
        .unwrap()
}
fn request(snapshot: &DryLiquidSnapshot) -> LiquidRequest {
    LiquidRequest {
        snapshot: snapshot.binding(),
        ticket: 1,
        position: [0.0f32, 800.0, 7.0].map(f32::to_bits),
    }
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

#[test]
fn real_decoded_native_products_prove_only_the_exact_covered_dry_tile() {
    let snapshot = snapshot();
    let mut query = request(&snapshot);
    for z in [-1000.0f32, -0.0, 0.0, 7.0, 1000.0] {
        query.position[2] = z.to_bits();
        assert_eq!(
            snapshot.query(query, deadline()).unwrap(),
            LiquidReply {
                request: query,
                status: LiquidStatus::NoWater
            }
        );
    }
    // The full static map is known dry, but the queried terrain tile is absent.
    query.position = [0.0f32, 0.0, 7.0].map(f32::to_bits);
    assert_eq!(snapshot.query(query, deadline()), Err(Error::Missing));
}
#[test]
fn liquid_payloads_and_noncovering_unloaded_vmap_slots_never_mean_dry() {
    let (mut spec, mut files) = vmap::fixture();
    // An aliased liquid section is malformed, never an observation of no water.
    let id = ProductId::Terrain(spec.tiles[0]);
    products::mutate(&mut spec, &mut files, &id, |bytes| {
        bytes[24..28].copy_from_slice(&48u32.to_le_bytes());
        bytes[28..32].copy_from_slice(&16u32.to_le_bytes());
    });
    assert!(matches!(products::load(spec, &files), Err(Error::Framing)));
    let (mut spec, mut files) = vmap::fixture();
    let id = ProductId::VmapModel("ramp.wmo.vmo".into());
    products::mutate(&mut spec, &mut files, &id, |bytes| {
        let i = bytes.windows(4).position(|p| p == b"LIQU").unwrap();
        bytes[i + 4..i + 8].copy_from_slice(&1u32.to_le_bytes());
    });
    assert!(products::load(spec, &files)
        .and_then(|p| p.load_dry_liquid())
        .is_err());

    let (mut spec, mut files) = vmap::fixture();
    // Keep the real complete map tree, but no tile resolves its primitive slots. The
    // existing ray loader legitimately accepts this; all-map no-liquid proof cannot.
    for tile in spec.tiles.clone() {
        let id = ProductId::VmapTile(tile);
        products::mutate(&mut spec, &mut files, &id, |bytes| {
            bytes.truncate(12);
            bytes[8..12].copy_from_slice(&0u32.to_le_bytes());
        });
        spec.products
            .iter_mut()
            .find(|p| p.id == id)
            .unwrap()
            .dependencies = vec![ProductId::VmapTree];
    }
    spec.products
        .retain(|p| !matches!(p.id, ProductId::VmapModel(_)));
    let products = products::load(spec, &files).unwrap();
    assert!(products.load_static_vmap().is_ok());
    assert!(matches!(products.load_dry_liquid(), Err(Error::Missing)));
}
#[test]
fn missing_and_corrupt_products_reject_before_a_liquid_snapshot_exists() {
    for id in [
        ProductId::Terrain(TileId { x: 32, y: 30 }),
        ProductId::VmapTree,
        ProductId::VmapModel("wall.wmo.vmo".into()),
    ] {
        let (spec, mut files) = vmap::fixture();
        files.remove(&id.filename(spec.map).unwrap());
        assert!(products::load(spec, &files)
            .and_then(|p| p.load_dry_liquid())
            .is_err());
    }
    let (spec, mut files) = vmap::fixture();
    let key = ProductId::Terrain(TileId { x: 32, y: 30 })
        .filename(spec.map)
        .unwrap();
    files.get_mut(&key).unwrap()[0] ^= 1;
    assert!(matches!(products::load(spec, &files), Err(Error::Hash)));
}
#[test]
fn request_binds_map_content_epoch_inventory_policy_ticket_and_exact_pose() {
    let snapshot = snapshot();
    let original = request(&snapshot);
    for field in 0..6 {
        let mut changed = original;
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
    let mut moved = original;
    moved.position[0] = (-0.0f32).to_bits();
    moved.ticket = 2;
    assert_ne!(
        snapshot.query(original, deadline()).unwrap(),
        snapshot.query(moved, deadline()).unwrap()
    );
    assert_eq!(snapshot.query(moved, deadline()).unwrap().request, moved);
    assert_eq!(
        snapshot.query(original, Instant::now()),
        Err(Error::Deadline)
    );
    for position in [
        [f32::NAN, 800., 7.],
        [0., f32::INFINITY, 7.],
        [0., 800., f32::NEG_INFINITY],
        [100000., 800., 7.],
        [0., 800., 100001.],
    ] {
        let mut bad = original;
        bad.position = position.map(f32::to_bits);
        assert!(snapshot.query(bad, deadline()).is_err());
    }
}
#[test]
fn source_replacement_cannot_relabel_old_immutable_evidence() {
    let (spec, mut files) = vmap::fixture();
    let old = products::load(spec.clone(), &files)
        .unwrap()
        .load_dry_liquid()
        .unwrap();
    let mut changed = spec;
    changed.binding.source_epoch = [77; 32];
    let new = products::load(changed, &files)
        .unwrap()
        .load_dry_liquid()
        .unwrap();
    assert_ne!(old.binding(), new.binding());
    files.values_mut().for_each(Vec::clear);
    assert!(old.query(request(&old), deadline()).is_ok());
    assert_eq!(new.query(request(&old), deadline()), Err(Error::Identity));
    assert_eq!(old.query(request(&new), deadline()), Err(Error::Identity));
}

#[test]
fn pinned_source_dry_branch_and_grid_selection_match_the_loaded_query() {
    let snapshot = snapshot();
    let mut count = 0;
    for line in include_str!(
        "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/dry-liquid.tsv"
    )
    .lines()
    {
        if line.starts_with('#') {
            continue;
        }
        let row: Vec<_> = line.split('\t').collect();
        let bits = |i| u32::from_str_radix(row[i], 16).unwrap();
        let position = [bits(0), bits(1), bits(2)];
        let tile = TileId {
            x: row[5].parse().unwrap(),
            y: row[6].parse().unwrap(),
        };
        assert_eq!(row[4], "0", "source no-liquid branch changed");
        assert_eq!(
            world_tile(f32::from_bits(position[0]), f32::from_bits(position[1])),
            Ok(tile)
        );
        let request = LiquidRequest {
            snapshot: snapshot.binding(),
            ticket: count + 1,
            position,
        };
        let actual = snapshot.query(request, deadline());
        if snapshot.tiles.contains(&tile) {
            assert_eq!(
                actual,
                Ok(LiquidReply {
                    request,
                    status: LiquidStatus::NoWater
                })
            );
        } else {
            assert_eq!(actual, Err(Error::Missing));
        }
        count += 1;
    }
    assert_eq!(count, 1338);
}

#[test]
fn one_unresolved_slot_blocks_dry_proof_even_when_an_unrelated_ray_is_clear() {
    use super::super::vmap::{RayRequest, StaticRay};
    let (mut spec, mut files) = vmap::fixture();
    for tile in spec.tiles.clone() {
        products::mutate(&mut spec, &mut files, &ProductId::VmapTile(tile), |bytes| {
            let count = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
            assert_eq!(count, 5);
            // Exact authored native writer's first ramp spawn is 78 bytes. Another
            // spawn still references this model, preserving the model dependency set.
            bytes.drain(12..90);
            bytes[8..12].copy_from_slice(&(count - 1).to_le_bytes());
        });
    }
    let products = products::load(spec, &files).unwrap();
    let rays = products.load_static_vmap().unwrap();
    let request = RayRequest {
        snapshot: rays.binding(),
        ticket: 1,
        from: [1000.0f32, 1000., 1.].map(f32::to_bits),
        to: [1001.0f32, 1000., 1.].map(f32::to_bits),
        ignore_m2: true,
    };
    assert_eq!(
        rays.query(request, deadline()).unwrap().result,
        StaticRay::Miss
    );
    assert!(matches!(products.load_dry_liquid(), Err(Error::Missing)));
}
#[test]
fn outer_grid_boundaries_reject_before_unsigned_indexing_or_missing_tile_fallback() {
    let positive = 32.0f32 * GRID_SIZE;
    let negative = -positive;
    assert_eq!(world_tile(positive, 0.), Ok(TileId { x: 0, y: 32 }));
    assert_eq!(
        world_tile(positive.next_down(), 0.),
        Ok(TileId { x: 0, y: 32 })
    );
    assert_eq!(world_tile(positive.next_up(), 0.), Err(Error::OutsideTile));
    assert_eq!(world_tile(negative, 0.), Err(Error::OutsideTile));
    assert_eq!(
        world_tile(negative.next_down(), 0.),
        Err(Error::OutsideTile)
    );
    // The source float subtraction can round the immediately adjacent valid-world
    // point back to grid 64; this supported API refuses it rather than indexing 64.
    let grid = 32.0f32 - negative.next_up() / GRID_SIZE;
    let expected = if grid < 64. {
        Ok(TileId {
            x: grid as u8,
            y: 32,
        })
    } else {
        Err(Error::OutsideTile)
    };
    assert_eq!(world_tile(negative.next_up(), 0.), expected);
    assert_eq!(world_tile(0., positive.next_up()), Err(Error::OutsideTile));
    assert_eq!(world_tile(0., negative), Err(Error::OutsideTile));
}

#[test]
fn slopes_and_holes_do_not_turn_a_liquid_fact_into_a_support_fact() {
    for shape in [1, 6] {
        let (mut spec, mut files) = vmap::fixture();
        products::mutate(
            &mut spec,
            &mut files,
            &ProductId::Terrain(TileId { x: 32, y: 30 }),
            |bytes| {
                *bytes = products::terrain_bytes(shape);
            },
        );
        let products = products::load(spec, &files).unwrap();
        let (x, y) = (-1.0f32, 2.0f32 * GRID_SIZE - 1.0);
        let height = products
            .terrain(0, TileId { x: 32, y: 30 })
            .unwrap()
            .point_height(x, y)
            .unwrap();
        if shape == 6 {
            assert_eq!(height, super::super::PointHeight::Hole);
        } else {
            assert!(matches!(height, super::super::PointHeight::Height(_)));
        }
        let snapshot = products.load_dry_liquid().unwrap();
        let mut query = request(&snapshot);
        query.position = [x, y, 7.0].map(f32::to_bits);
        assert_eq!(
            snapshot.query(query, deadline()).unwrap().status,
            LiquidStatus::NoWater
        );
    }
}

// Authored native product/DBC bytes, not installed assets.
fn dbc(entry: u32, kind: u32) -> Vec<u8> {
    let mut b = b"WDBC".to_vec();
    for v in [1u32, 4, 16, 1, entry, 23, kind, 0] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.push(0);
    b
}
fn wet_terrain(bytes: &mut Vec<u8>, entry: u16) {
    let at = u32::from_le_bytes(bytes[32..36].try_into().unwrap()) as usize;
    let mut liquid = b"MLIQ".to_vec();
    liquid.extend_from_slice(&[3, 8]); // global type, flat liquid height
    liquid.extend_from_slice(&entry.to_le_bytes());
    liquid.extend_from_slice(&[0, 0, 129, 129]);
    liquid.extend_from_slice(&10f32.to_le_bytes());
    bytes.splice(at..at, liquid);
    bytes[24..28].copy_from_slice(&(at as u32).to_le_bytes());
    bytes[28..32].copy_from_slice(&16u32.to_le_bytes());
    bytes[32..36].copy_from_slice(&((at + 16) as u32).to_le_bytes());
}
fn wet_products(entry: u16) -> InventoriedProducts {
    let (mut spec, mut files) = vmap::fixture();
    products::mutate(
        &mut spec,
        &mut files,
        &ProductId::Terrain(TileId { x: 32, y: 30 }),
        |b| wet_terrain(b, entry),
    );
    products::load(spec, &files).unwrap()
}
fn grid_request(snapshot: &GridLiquidSnapshot<'_>) -> GridLiquidRequest {
    GridLiquidRequest {
        query: LiquidRequest {
            snapshot: snapshot.binding(),
            ticket: 7,
            position: [0f32, 800., 9.].map(f32::to_bits),
        },
        required_type: 8,
        collision_height: 2f32.to_bits(),
    }
}
#[test]
fn actual_wet_product_and_dbc_bytes_produce_bound_gridmap_status_only() {
    use super::super::terrain::{GridLiquidData, GridLiquidStatus};
    let products = wet_products(21);
    assert!(matches!(
        products.load_dry_liquid(),
        Err(Error::Unsupported)
    ));
    assert!(matches!(
        products.load_navigation(),
        Err(Error::Unsupported)
    ));
    assert!(products.load_static_vmap().is_ok());
    let snapshot = products.load_grid_liquid(&dbc(21, 3)).unwrap();
    let request = grid_request(&snapshot);
    let reply = snapshot.query(request, deadline()).unwrap();
    assert_eq!(reply.request, request);
    assert_eq!(reply.result.status, GridLiquidStatus::InWater);
    assert_eq!(
        reply.result.data,
        Some(GridLiquidData {
            entry: 21,
            type_flags: 8,
            level: 10f32.to_bits(),
            depth_level: 7f32.to_bits()
        })
    );
    let mut query = request;
    query.query.position[2] = 7f32.to_bits();
    assert_eq!(
        snapshot.query(query, deadline()).unwrap().result.status,
        GridLiquidStatus::UnderWater
    );
    query.query.position[2] = 10f32.to_bits();
    assert_eq!(
        snapshot.query(query, deadline()).unwrap().result.status,
        GridLiquidStatus::WaterWalk
    );
    query.query.position[2] = 11f32.to_bits();
    assert_eq!(
        snapshot.query(query, deadline()).unwrap().result.status,
        GridLiquidStatus::AboveWater
    );
    query.required_type = 1;
    assert_eq!(
        snapshot.query(query, deadline()).unwrap().result.status,
        GridLiquidStatus::NoWater
    );
}
#[test]
fn gridmap_replies_bind_all_geometry_dbc_identity_pose_and_query_inputs() {
    let products = wet_products(21);
    let snapshot = products.load_grid_liquid(&dbc(21, 3)).unwrap();
    let request = grid_request(&snapshot);
    let changed = products.load_grid_liquid(&dbc(21, 2)).unwrap();
    assert_ne!(snapshot.binding(), changed.binding());
    assert_eq!(changed.query(request, deadline()), Err(Error::Identity));
    for i in 0..5 {
        let mut query = request;
        match i {
            0 => query.query.snapshot.map += 1,
            1 => query.query.snapshot.content[0] ^= 1,
            2 => query.query.snapshot.source_epoch[0] ^= 1,
            3 => query.query.snapshot.inventory[0] ^= 1,
            _ => query.query.snapshot.policy[0] ^= 1,
        }
        assert_eq!(snapshot.query(query, deadline()), Err(Error::Identity));
    }
    let mut query = request;
    query.query.ticket = 0;
    assert_eq!(snapshot.query(query, deadline()), Err(Error::Identity));
    assert_eq!(
        snapshot.query(request, Instant::now()),
        Err(Error::Deadline)
    );
    query = request;
    query.query.position[0] = (-0f32).to_bits();
    query.collision_height = 0f32.to_bits();
    query.required_type = 0;
    assert_eq!(snapshot.query(query, deadline()).unwrap().request, query);
    query.query.position = [0f32, 0., 9.].map(f32::to_bits);
    assert_eq!(snapshot.query(query, deadline()), Err(Error::Missing));
    for value in [f32::NAN, f32::INFINITY] {
        query = request;
        query.collision_height = value.to_bits();
        assert_eq!(snapshot.query(query, deadline()), Err(Error::Format));
        query = request;
        query.query.position[2] = value.to_bits();
        assert_eq!(snapshot.query(query, deadline()), Err(Error::Format));
    }
}
#[test]
fn gridmap_area_dependencies_and_unresolved_vmap_never_become_liquid_answers() {
    let products = wet_products(1);
    let snapshot = products.load_grid_liquid(&dbc(1, 3)).unwrap();
    let request = grid_request(&snapshot);
    assert_eq!(snapshot.query(request, deadline()), Err(Error::Unsupported));
    // A true missing row from a complete decoded table follows source raw type flags.
    let snapshot = products.load_grid_liquid(&dbc(21, 3)).unwrap();
    assert!(snapshot.query(grid_request(&snapshot), deadline()).is_ok());
    let (mut spec, mut files) = vmap::fixture();
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
    let products = products::load(spec, &files).unwrap();
    assert!(matches!(
        products.load_grid_liquid(&dbc(21, 3)),
        Err(Error::Missing)
    ));
}
#[test]
fn changed_real_product_bytes_cannot_relabel_a_published_gridmap_reply() {
    let original = wet_products(21);
    let old = original.load_grid_liquid(&dbc(21, 3)).unwrap();
    let query = grid_request(&old);
    let before = old.query(query, deadline()).unwrap();
    let (mut spec, mut files) = vmap::fixture();
    products::mutate(
        &mut spec,
        &mut files,
        &ProductId::Terrain(TileId { x: 32, y: 30 }),
        |b| {
            wet_terrain(b, 21);
            let at = u32::from_le_bytes(b[24..28].try_into().unwrap()) as usize;
            b[at + 12..at + 16].copy_from_slice(&20f32.to_le_bytes());
        },
    );
    spec.binding.source_epoch[0] ^= 1;
    let changed_products = products::load(spec, &files).unwrap();
    let changed = changed_products.load_grid_liquid(&dbc(21, 3)).unwrap();
    assert_ne!(old.binding().inventory, changed.binding().inventory);
    assert_eq!(changed.query(query, deadline()), Err(Error::Identity));
    assert_eq!(old.query(query, deadline()).unwrap(), before);
    let new_query = grid_request(&changed);
    assert_ne!(
        changed.query(new_query, deadline()).unwrap().result,
        before.result
    );
}

#[test]
fn existing_valid_dry_navigation_rejects_newly_decodable_wet_terrain() {
    use super::super::navigation::tests as navigation;
    if benilla_native_nav::SUPPORTED_ABI {
        assert!(navigation::products().load_navigation().is_ok());
    }
    let (mut spec, mut files) = navigation::fixture();
    let tile = spec.tiles[0];
    products::mutate(&mut spec, &mut files, &ProductId::Terrain(tile), |b| {
        wet_terrain(b, 21)
    });
    let wet = products::load(spec, &files).unwrap();
    assert!(wet.terrain(wet.map, tile).unwrap().has_liquid());
    assert!(matches!(wet.load_navigation(), Err(Error::Unsupported)));
}

fn area_dbc(rows: &[(u32, u32, u32)]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for word in [rows.len() as u32, 25, 100, 1] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    for &(id, zone, liquid_override) in rows {
        let mut row = [0u32; 25];
        row[0] = id;
        row[1] = 999; // Source GridMap lookup does not filter by map ID.
        row[2] = zone;
        row[3] = id + 100; // Nor does it look up this exploreFlag.
        row[24] = liquid_override;
        for word in row {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
    }
    bytes.push(0);
    bytes
}
fn area_liquid_dbc() -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for word in [3u32, 4, 16, 1] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    for row in [[1u32, 23, 3, 0], [22, 35, 0, 0], [23, 41, 2, 0]] {
        for word in row {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
    }
    bytes.push(0);
    bytes
}
fn area_products(raw_area: u16) -> InventoriedProducts {
    let (mut spec, mut files) = vmap::fixture();
    products::mutate(
        &mut spec,
        &mut files,
        &ProductId::Terrain(TileId { x: 32, y: 30 }),
        |bytes| {
            wet_terrain(bytes, 1);
            bytes[46..48].copy_from_slice(&raw_area.to_le_bytes());
        },
    );
    products::load(spec, &files).unwrap()
}
#[test]
fn actual_area_bytes_enable_only_the_source_gridmap_override_branch() {
    let products = area_products(5);
    let liquid = area_liquid_dbc();
    let without = products.load_grid_liquid(&liquid).unwrap();
    assert_eq!(
        without.query(grid_request(&without), deadline()),
        Err(Error::Unsupported)
    );
    // Complete empty table proves a lookup miss; omitted table does not.
    let empty = products
        .load_grid_liquid_with_areas(&liquid, &area_dbc(&[]))
        .unwrap();
    let result = empty
        .query(grid_request(&empty), deadline())
        .unwrap()
        .result;
    assert_eq!(
        (result.data.unwrap().entry, result.data.unwrap().type_flags),
        (1, 8)
    );
    for (rows, expected) in [
        (vec![(5, 9, 0), (9, 0, 22)], (22, 1)),
        (vec![(5, 9, 23), (9, 0, 22)], (23, 4)),
        (vec![(105, 0, 22)], (1, 8)),
    ] {
        let snapshot = products
            .load_grid_liquid_with_areas(&liquid, &area_dbc(&rows))
            .unwrap();
        let mut request = grid_request(&snapshot);
        request.required_type = 0;
        let reply = snapshot.query(request, deadline()).unwrap();
        assert_eq!(reply.request, request);
        let data = reply.result.data.unwrap();
        assert_eq!((data.entry, data.type_flags), expected);
        assert_eq!(data.level, 10f32.to_bits());
        assert_eq!(data.depth_level, 7f32.to_bits());
    }
}
#[test]
fn changed_area_bytes_and_raw_area_geometry_cannot_relabel_old_replies() {
    let products = area_products(5);
    let liquid = area_liquid_dbc();
    let areas = area_dbc(&[(5, 9, 0), (9, 0, 22)]);
    let old = products
        .load_grid_liquid_with_areas(&liquid, &areas)
        .unwrap();
    let mut request = grid_request(&old);
    request.required_type = 0;
    let reply = old.query(request, deadline()).unwrap();
    let changed = products
        .load_grid_liquid_with_areas(&liquid, &area_dbc(&[(5, 0, 23)]))
        .unwrap();
    assert_ne!(old.binding().policy, changed.binding().policy);
    assert_eq!(changed.query(request, deadline()), Err(Error::Identity));
    assert_eq!(old.query(request, deadline()).unwrap(), reply);
    let changed_products = area_products(9);
    let new_geometry = changed_products
        .load_grid_liquid_with_areas(&liquid, &areas)
        .unwrap();
    assert_ne!(old.binding().inventory, new_geometry.binding().inventory);
    assert_eq!(
        new_geometry.query(request, deadline()),
        Err(Error::Identity)
    );
    // Hash even fields unused by this liquid query, not only a projection of rows.
    let mut unused_changed = areas.clone();
    unused_changed[24..28].copy_from_slice(&123u32.to_le_bytes());
    let changed = products
        .load_grid_liquid_with_areas(&liquid, &unused_changed)
        .unwrap();
    assert_ne!(old.binding().policy, changed.binding().policy);
    assert_eq!(changed.query(request, deadline()), Err(Error::Identity));
    let no_areas = products.load_grid_liquid(&liquid).unwrap();
    assert_ne!(old.binding().policy, no_areas.binding().policy);
    assert_eq!(no_areas.query(request, deadline()), Err(Error::Identity));
}
#[test]
fn corrupt_or_missing_area_bytes_fail_without_publishing_a_snapshot() {
    let products = area_products(5);
    let liquid = area_liquid_dbc();
    let areas = area_dbc(&[(5, 0, 22)]);
    for end in 0..areas.len() {
        assert!(products
            .load_grid_liquid_with_areas(&liquid, &areas[..end])
            .is_err());
    }
    let old = products
        .load_grid_liquid_with_areas(&liquid, &areas)
        .unwrap();
    let mut request = grid_request(&old);
    request.required_type = 0;
    let expected = old.query(request, deadline()).unwrap();
    let mut corrupt = areas;
    corrupt[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(products
        .load_grid_liquid_with_areas(&liquid, &corrupt)
        .is_err());
    assert_eq!(old.query(request, deadline()).unwrap(), expected);
}

#[test]
fn forged_dry_projection_proof_fields_or_tile_set_cannot_load_navigation() {
    let products = super::super::dry_route::tests::products();
    for field in 0..7 {
        let mut proof = products.inventory.load_dry_liquid().unwrap();
        match field {
            0 => proof.binding.map += 1,
            1 => proof.binding.content[0] ^= 1,
            2 => proof.binding.source_epoch[0] ^= 1,
            3 => proof.binding.inventory[0] ^= 1,
            4 => proof.binding.policy[0] ^= 1,
            5 => proof.tiles.clear(),
            _ => {
                proof.tiles.insert(TileId { x: 31, y: 31 });
            }
        }
        assert!(matches!(
            products.inventory.load_dry_route_navigation(&proof),
            Err(Error::Identity)
        ));
    }
}
