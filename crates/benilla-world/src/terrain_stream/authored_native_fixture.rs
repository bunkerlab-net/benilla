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
