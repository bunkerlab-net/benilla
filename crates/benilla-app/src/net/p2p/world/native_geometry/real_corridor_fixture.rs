// SPDX-License-Identifier: GPL-3.0-or-later
//! Real parsed raw-terrain coverage and Avian corridor checks on authored data.
//! Production session/participant admission stays closed. No Corridor implementation,
//! success facts, synthetic SegmentChecks, or replacement collision algorithm lives here.

use super::{core, products, Body, Corridor, Failure, LocalCorridor};
use avian3d::prelude::*;
use benilla_assets::{
    AdtLoader, AdtTile, M2Model, MapCatalogRes, WdtIndex, WdtIndexLoader, WmoModel,
};
use benilla_world::{
    collision::{MoverTraceExclusions, WorldCollision},
    schedule::WorldLive,
    terrain_stream::{authored_native_fixture, NativeTerrainCoverage},
    world_map::CurrentMap,
};
use bevy::{
    ecs::system::{RunSystemOnce, SystemState},
    prelude::*,
    time::TimeUpdateStrategy,
    transform::TransformSystems,
};
use products::{
    same_polygon_fixture::{self as source, RawMap, TerrainKind, DIRECTORY, FROM, MAP, TO},
    vmap::{RayRequest, StaticRay},
};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Resource, Default)]
struct PhysicsObserved(u64);

/// The app is dropped before its archive; the asset reader retains the open MPQ.
pub(crate) struct Fixture {
    pub(crate) app: App,
    pub(crate) tiles: BTreeMap<(i32, i32), Handle<AdtTile>>,
    pub(crate) roots: BTreeMap<(i32, i32), Entity>,
    raw: RawMap,
    revision: [u8; 32],
}

impl Fixture {
    pub(crate) fn new() -> Self {
        Self::with_terrain(TerrainKind::Flat)
    }

    fn with_terrain(kind: TerrainKind) -> Self {
        let raw = RawMap::new(kind);
        let inventory = raw.inventory();
        let tiles: Vec<_> = (30..=32)
            .flat_map(|y| (30..=32).map(move |x| (x, y)))
            .collect();
        let bound_tiles: [_; 9] = tiles
            .iter()
            .map(|&tile| (tile, inventory.tile_digest(tile).unwrap()))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let revision = core::geometry_revision(
            MAP,
            inventory.wdt_digest(),
            &bound_tiles,
            &[],
            inventory.source_digest(),
        );
        let mut app = App::new();
        benilla_assets::register_mpq_source(&mut app, &raw.path).unwrap();
        app.add_plugins((
            MinimalPlugins,
            bevy::transform::TransformPlugin,
            bevy::asset::AssetPlugin::default(),
            bevy::scene::ScenePlugin,
            PhysicsPlugins::default(),
        ));
        app.init_asset::<Mesh>();
        app.init_asset::<Image>();
        app.init_asset::<AdtTile>();
        app.init_asset::<WdtIndex>();
        app.init_asset::<M2Model>();
        app.init_asset::<WmoModel>();
        app.register_asset_loader(AdtLoader);
        app.register_asset_loader(WdtIndexLoader);
        app.init_resource::<MoverTraceExclusions>();
        app.init_resource::<PhysicsObserved>();
        app.insert_resource(CurrentMap(MAP));
        app.insert_resource(MapCatalogRes(raw.maps()));
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            32,
        )));
        app.add_systems(
            FixedPostUpdate,
            (|mut observed: ResMut<PhysicsObserved>| observed.0 += 1)
                .after(PhysicsSystems::Last)
                .after(TransformSystems::Propagate),
        );
        let server = app.world().resource::<AssetServer>();
        let wdt: Handle<WdtIndex> = server.load("mpq://World/Maps/Fixture/Fixture.wdt");
        let tiles: BTreeMap<_, Handle<AdtTile>> = tiles
            .into_iter()
            .map(|(x, y)| {
                (
                    (x, y),
                    server.load(format!("mpq://World/Maps/Fixture/Fixture_{x}_{y}.adt")),
                )
            })
            .collect();
        app.finish();
        app.cleanup();
        let until = Instant::now() + Duration::from_secs(20);
        loop {
            app.update();
            if app
                .world()
                .resource::<Assets<WdtIndex>>()
                .get(&wdt)
                .is_some()
                && tiles
                    .values()
                    .all(|h| app.world().resource::<Assets<AdtTile>>().get(h).is_some())
            {
                break;
            }
            assert!(
                Instant::now() < until,
                "authored WDT/ADT loaders must finish"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        let roots = authored_native_fixture::install_empty_placement_map(
            app.world_mut(),
            DIRECTORY,
            wdt,
            &tiles,
            inventory,
        );
        app.insert_resource(WorldLive(true));
        let mut result = Self {
            app,
            tiles,
            roots,
            raw,
            revision,
        };
        result.advance_physics();
        if matches!(kind, TerrainKind::Flat) {
            let proof = result
                .app
                .world_mut()
                .run_system_once(|terrain: NativeTerrainCoverage| terrain.prove(MAP, FROM, TO, 0.5))
                .unwrap()
                .unwrap();
            assert_eq!(proof.models, vec![]);
            assert_eq!(proof.static_roots, vec![]);
            assert_eq!(proof.terrain_root, result.roots[&(31, 31)]);
            assert_eq!(
                core::geometry_revision(
                    proof.map,
                    proof.wdt_source_digest,
                    &proof.tiles,
                    &proof.models,
                    proof.inventory_digest
                ),
                result.revision
            );
        }
        result
    }

    pub(crate) fn raw_geometry_revision(&self) -> [u8; 32] {
        self.revision
    }

    pub(crate) fn products(&self) -> Arc<products::VmapLiquidProducts> {
        let (spec, files) = source::fixture(&self.raw);
        products::dry_route::tests::load(spec, &files)
    }

    pub(super) fn advance_physics(&mut self) {
        let before = self.app.world().resource::<PhysicsObserved>().0;
        self.app.update();
        self.app.update();
        assert!(self.app.world().resource::<PhysicsObserved>().0 > before);
    }

    /// Let a production resolver consume the actual concrete corridor in its ECS
    /// world. This accepts non-Send loaded source-query owners without fabricating
    /// a certificate token or requiring a test-local Corridor implementation.
    pub(super) fn with_local_corridor<T>(
        &mut self,
        query: impl FnOnce(&mut LocalCorridor<'_, '_, '_>) -> T,
    ) -> T {
        self.advance_physics();
        let mut state = SystemState::<(NativeTerrainCoverage, WorldCollision, SpatialQuery)>::new(
            self.app.world_mut(),
        );
        let (terrain, collision, spatial) = state.get(self.app.world());
        query(&mut LocalCorridor {
            terrain: &terrain,
            collision: &collision,
            spatial: &spatial,
            map: MAP,
            revision: self.revision,
        })
    }

    /// Only the concrete production corridor may certify these actual ECS assets.
    /// A loaded source VMap ray is an additional veto using owner-height endpoints.
    pub(crate) fn certify(
        &mut self,
        from: [f32; 3],
        to: [f32; 3],
        body: Body,
    ) -> Result<(), Failure> {
        self.with_local_corridor(|corridor| corridor.certify(from, to, body))?;
        let vmap = source::static_vmap(&self.raw);
        let eye = |p: [f32; 3]| [p[0], p[1], p[2] + body.height.as_f32()].map(f32::to_bits);
        let request = RayRequest {
            snapshot: vmap.binding(),
            ticket: 1,
            from: eye(from),
            to: eye(to),
            ignore_m2: false,
        };
        let reply = vmap
            .query(request, Instant::now() + Duration::from_secs(5))
            .map_err(|_| Failure::MissingGeometry)?;
        if reply.request != request {
            return Err(Failure::Binding);
        }
        match reply.result {
            StaticRay::Miss => Ok(()),
            StaticRay::Hit { .. } => Err(Failure::Obstacle),
        }
    }
}

fn body() -> Body {
    Body {
        radius: benilla_sim::creature_ai::Real::new(0.5).unwrap(),
        height: benilla_sim::creature_ai::Real::new(2.).unwrap(),
    }
}

#[test]
fn real_flat_corridor_proves_all_local_predicates_and_loaded_source_los() {
    let mut fixture = Fixture::new();
    assert_ne!(fixture.raw_geometry_revision(), [0; 32]);
    assert_eq!(fixture.certify(FROM, TO, body()), Ok(()));
    // The longer runtime owner/goal neighborhood uses the same raw support universe.
    assert_eq!(fixture.certify(FROM, [5., 9., 0.], body()), Ok(()));
}

#[test]
fn real_corridor_rechecks_the_last_live_collider_vertex_and_triangle() {
    let mut fixture = Fixture::new();
    let root = fixture.roots[&(31, 31)];
    let original = fixture.app.world().get::<Collider>(root).unwrap().clone();
    assert_eq!(fixture.certify(FROM, TO, body()), Ok(()));
    for mutation in 0..3 {
        let mesh = original.shape_scaled().as_trimesh().unwrap();
        let mut vertices = mesh.vertices().to_vec();
        let mut triangles = mesh.indices().to_vec();
        match mutation {
            0 => vertices.last_mut().unwrap().y += 0.25,
            1 => triangles.last_mut().unwrap().swap(0, 1),
            _ => {
                triangles.pop();
            }
        }
        fixture
            .app
            .world_mut()
            .entity_mut(root)
            .insert(Collider::trimesh(vertices, triangles));
        assert_eq!(
            fixture.certify(FROM, TO, body()),
            Err(Failure::MissingGeometry),
            "mutation {mutation}"
        );
        fixture
            .app
            .world_mut()
            .entity_mut(root)
            .insert(original.clone());
        assert_eq!(fixture.certify(FROM, TO, body()), Ok(()));
    }
}

#[test]
fn real_corridor_rejects_raw_hole_and_nonflat_fan_center() {
    for kind in [TerrainKind::Hole, TerrainKind::Nonflat] {
        let mut fixture = Fixture::with_terrain(kind);
        assert_eq!(
            fixture.certify(FROM, TO, body()),
            Err(Failure::UnsupportedGround),
            "{kind:?}"
        );
    }
}

#[test]
fn real_corridor_requires_guard_neighbor_and_source_bound_transforms_layers() {
    let mut fixture = Fixture::new();
    let neighbor = fixture.tiles[&(30, 30)].id();
    let removed = fixture
        .app
        .world_mut()
        .resource_mut::<Assets<AdtTile>>()
        .remove(neighbor)
        .unwrap();
    assert_eq!(
        fixture.certify(FROM, TO, body()),
        Err(Failure::MissingGeometry)
    );
    fixture
        .app
        .world_mut()
        .resource_mut::<Assets<AdtTile>>()
        .insert(neighbor, removed)
        .unwrap();
    assert_eq!(fixture.certify(FROM, TO, body()), Ok(()));

    let root = fixture.roots[&(31, 31)];
    fixture
        .app
        .world_mut()
        .entity_mut(root)
        .insert(Transform::from_xyz(0., 0.25, 0.));
    assert_eq!(
        fixture.certify(FROM, TO, body()),
        Err(Failure::MissingGeometry)
    );
    fixture
        .app
        .world_mut()
        .entity_mut(root)
        .insert(Transform::IDENTITY);
    assert_eq!(fixture.certify(FROM, TO, body()), Ok(()));
    fixture
        .app
        .world_mut()
        .entity_mut(root)
        .insert(CollisionLayers::new(0b10, u32::MAX));
    assert_eq!(
        fixture.certify(FROM, TO, body()),
        Err(Failure::MissingGeometry)
    );
}

#[test]
fn real_avian_overlap_never_treats_an_uncatalogued_obstacle_as_empty_space() {
    let mut fixture = Fixture::new();
    let goal = [5., 9., 0.];
    let wall = fixture
        .app
        .world_mut()
        .spawn((
            Collider::cuboid(0.25, 3., 2.),
            RigidBody::Static,
            Transform::from_translation(benilla_assets::coords::wow_to_bevy([5., 7., 1.5])),
            CollisionLayers::DEFAULT,
        ))
        .id();
    assert_eq!(
        fixture.certify(FROM, goal, body()),
        Err(Failure::UnsupportedNavigation)
    );
    // LocalCorridor rejects incomplete membership before narrowphase. Independently
    // establish that the real query trees also see this actual body/LOS obstacle;
    // this does not relabel it as a source-catalogued static placement.
    fixture.with_local_corridor(|corridor| {
        let from = benilla_assets::coords::wow_to_bevy(FROM);
        let movement = benilla_assets::coords::wow_to_bevy(goal) - from;
        let shape = Collider::cuboid(1., 2., 1.);
        let filter = WorldCollision::body_filter().with_excluded_entities([corridor
            .terrain
            .prove(MAP, FROM, goal, 0.5)
            .unwrap()
            .terrain_root]);
        assert!(corridor
            .spatial
            .cast_shape(
                &shape,
                from + Vec3::Y,
                Quat::IDENTITY,
                Dir3::new(movement).unwrap(),
                &ShapeCastConfig::from_max_distance(movement.length()),
                &filter
            )
            .is_some());
        assert!(corridor
            .collision
            .cast_body(&shape, from + Vec3::Y, movement, 0.)
            .is_some());
        assert!(corridor
            .spatial
            .cast_ray(
                from + Vec3::Y * 2.,
                Dir3::new(movement).unwrap(),
                movement.length(),
                true,
                &WorldCollision::conservative_los_filter()
            )
            .is_some());
    });
    fixture.app.world_mut().despawn(wall);
    assert_eq!(fixture.certify(FROM, goal, body()), Ok(()));
}

#[test]
fn static_index_retains_only_its_actual_inventory_provenance_owner() {
    let raw = RawMap::new(TerrainKind::Flat);
    let inventory = raw.inventory();
    let owner = inventory.source_provenance().unwrap().clone();
    let replacement = raw.inventory();
    assert_eq!(inventory.source_digest(), replacement.source_digest());
    assert_eq!(
        owner.receipt_digest(),
        replacement.source_provenance().unwrap().receipt_digest()
    );
    assert!(!owner.same_owner(replacement.source_provenance().unwrap()));
    let index =
        benilla_world::terrain_stream::NativeStaticMapIndex::from_inventory(inventory).unwrap();
    assert!(owner.same_owner(index.source_provenance().unwrap()));
    let other =
        benilla_world::terrain_stream::NativeStaticMapIndex::from_inventory(replacement).unwrap();
    assert!(!owner.same_owner(other.source_provenance().unwrap()));
}

#[path = "real_corridor_fixture/installed_source.rs"]
#[cfg(test)]
mod installed_source;
