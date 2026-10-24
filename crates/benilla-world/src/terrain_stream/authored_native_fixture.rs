//! Authored integration-fixture residency, enabled only by a test dependency.
//! Actual loaders supply the assets; production collider construction and coverage
//! validation remain unchanged. This creates neither authority nor coverage proofs.

use super::*;
use avian3d::prelude::{Collider, CollisionLayers, RigidBody};
use std::collections::BTreeMap;

/// Install a complete, genuinely placement-free authored map after its assets load.
/// The caller must advance Avian and transform propagation before querying it.
pub fn install_empty_placement_map(
    world: &mut World,
    directory: &str,
    wdt: Handle<WdtIndex>,
    handles: &BTreeMap<(i32, i32), Handle<AdtTile>>,
    inventory: benilla_formats::NativeM2MapInventory,
) -> BTreeMap<(i32, i32), Entity> {
    assert_eq!(inventory.directory(), directory);
    assert_eq!(inventory.placements().count(), 0);
    assert_eq!(inventory.wmo_placements().count(), 0);
    let mut streamer = TerrainStreamer {
        map_dir: Some(directory.into()),
        wdt: Some(wdt),
        ..default()
    };
    let mut roots = BTreeMap::new();
    for (&tile, handle) in handles {
        let adt = world.resource::<Assets<AdtTile>>().get(handle).unwrap();
        assert!(adt.native_static_coverage_safe);
        assert!(adt.doodads.is_empty() && adt.wmos.is_empty());
        assert_eq!(inventory.tile_digest(tile), Some(adt.native_source_digest));
        let (vertices, triangles) = terrain_collider_data(&adt.chunks).unwrap();
        let entity = world
            .spawn((
                Collider::trimesh(vertices, triangles),
                RigidBody::Static,
                CollisionLayers::DEFAULT,
                Transform::IDENTITY,
                GroundDecalSurface,
                PickOccluder,
            ))
            .id();
        roots.insert(tile, entity);
        streamer.tiles.insert(
            tile,
            TileState {
                handle: handle.clone(),
                entity: Some(entity),
                material: None,
                next_cell: 16,
                furnished: true,
                placements: vec![],
                liquid: vec![],
                wall: None,
                clutter: vec![],
                welds: vec![],
                merged: vec![],
            },
        );
    }
    world.insert_resource(streamer);
    world.insert_resource(Placements::default());
    world.insert_resource(NativeStaticMapIndex::from_inventory(inventory).unwrap());
    roots
}

/// Install only a placement-free resident 3×3 guard from a complete authored map.
/// Exterior placements remain in the original inventory consumed by the index;
/// the ordinary coverage reader must exclude their actual transformed hulls.
/// The caller must advance Avian and transform propagation before querying it.
pub fn install_placement_free_guard(
    world: &mut World,
    directory: &str,
    wdt: Handle<WdtIndex>,
    handles: &BTreeMap<(i32, i32), Handle<AdtTile>>,
    inventory: benilla_formats::NativeM2MapInventory,
) -> BTreeMap<(i32, i32), Entity> {
    assert_eq!(inventory.directory(), directory);
    assert_eq!(handles.len(), 9);
    let min_x = handles.keys().map(|tile| tile.0).min().unwrap();
    let min_y = handles.keys().map(|tile| tile.1).min().unwrap();
    assert!((0..62).contains(&min_x) && (0..62).contains(&min_y));
    let loaded_wdt = world.resource::<Assets<WdtIndex>>().get(&wdt).unwrap();
    assert!(loaded_wdt.native_terrain_coverage_safe());
    assert!(loaded_wdt.global_wmo().is_none());
    assert_eq!(loaded_wdt.native_source_digest(), inventory.wdt_digest());
    for y in min_y..min_y + 3 {
        for x in min_x..min_x + 3 {
            assert!(handles.contains_key(&(x, y)));
            assert!(loaded_wdt.has_tile(x as u32, y as u32));
        }
    }
    let mut streamer = TerrainStreamer {
        map_dir: Some(directory.into()),
        wdt: Some(wdt),
        ..default()
    };
    let mut roots = BTreeMap::new();
    for (&tile, handle) in handles {
        let adt = world.resource::<Assets<AdtTile>>().get(handle).unwrap();
        assert!(adt.native_static_coverage_safe);
        assert!(adt.doodads.is_empty() && adt.wmos.is_empty());
        assert_eq!(inventory.tile_digest(tile), Some(adt.native_source_digest));
        assert_eq!(adt.chunks.len(), 256);
        assert!(adt.chunks.iter().all(|chunk| chunk.liquids.is_empty()));
        let (vertices, triangles) = terrain_collider_data(&adt.chunks).unwrap();
        let entity = world
            .spawn((
                Collider::trimesh(vertices, triangles),
                RigidBody::Static,
                CollisionLayers::DEFAULT,
                Transform::IDENTITY,
                GroundDecalSurface,
                PickOccluder,
            ))
            .id();
        roots.insert(tile, entity);
        streamer.tiles.insert(
            tile,
            TileState {
                handle: handle.clone(),
                entity: Some(entity),
                material: None,
                next_cell: 16,
                furnished: true,
                placements: vec![],
                liquid: vec![],
                wall: None,
                clutter: vec![],
                welds: vec![],
                merged: vec![],
            },
        );
    }
    world.insert_resource(streamer);
    world.insert_resource(Placements::default());
    world.insert_resource(NativeStaticMapIndex::from_inventory(inventory).unwrap());
    roots
}

/// Withdraw the authored guard's furnishing state to exercise the live reader.
pub fn unfurnish_guard_tile(world: &mut World, tile: (i32, i32)) {
    world
        .resource_mut::<TerrainStreamer>()
        .tiles
        .get_mut(&tile)
        .unwrap()
        .furnished = false;
}
