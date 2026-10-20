// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact installed raw owner, genuine source observations and the unchanged corridor.
//! Authored, synchronous test evidence only; no Route or future source stability.
use super::*;
use benilla_formats::{Chain, NativeM2MapInventory};
use benilla_world::terrain_stream::{CoverageError, CoverageProof, NativeStaticMapIndex};
use products::{
    source_height::{
        inventory::InventoryCompatibility, CompositionError, HeightRequest, PreparedSourceHeight,
    },
    vmap::{
        installed_source_fixture as input, tile_prepared::PreparedTileCollision, QueryBudget,
        SnapshotBinding,
    },
    Error, TileId,
};
use std::{cell::Cell, path::Path};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}

/// Owners outlive every source view and the live app created inside this callback.
fn with_sources<T>(
    install: &input::Install,
    test: impl FnOnce(&PreparedSourceHeight<'_, '_, '_>, SnapshotBinding, NativeM2MapInventory) -> T,
) -> T {
    let inventory = input::raw_inventory(install);
    let selected = input::guard_source_tiles();
    let terrain = benilla_native_terrain::prepare(install.path(), MAP, &selected).unwrap();
    let capture = benilla_native_vmap_source::prepare(install.path(), MAP, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(input::profile(), || false)
        .unwrap()
        .unwrap();
    assert_eq!(
        bundle.models().len(),
        1,
        "the exterior WMO remains in the full closure"
    );
    assert_eq!(inventory.wmo_placements().count(), 1);
    assert_eq!(inventory.placements().count(), 0);
    let selected: Vec<_> = selected
        .into_iter()
        .map(|tile| TileId {
            x: tile.x,
            y: tile.y,
        })
        .collect();
    let source_binding = PreparedTileCollision::from_source(&bundle, &selected, || false)
        .unwrap()
        .binding();
    let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    test(&view, source_binding, inventory)
}

/// Inspect the same complete walking/camera vertex transforms used by the raw index.
fn exterior_intersects_sweep(inventory: &NativeM2MapInventory) -> bool {
    use benilla_assets::coords::{bevy_to_wow, placement_rotation, wow_to_bevy};
    let (placement, model) = inventory.wmo_placements().next().unwrap();
    assert!(model
        .collision()
        .root()
        .selected_doodads(placement.doodad_set)
        .unwrap()
        .is_empty());
    let transform = Transform {
        translation: wow_to_bevy(placement.position),
        rotation: placement_rotation(placement.rotation),
        scale: Vec3::ONE,
    };
    let mut bounds: Option<([f32; 2], [f32; 2])> = None;
    for hull in [model.collision().walking(), model.collision().camera()] {
        if hull.indices.is_empty() {
            continue;
        }
        for &point in &hull.positions {
            let point = bevy_to_wow(transform.transform_point(wow_to_bevy(point)));
            assert!(point.into_iter().all(f32::is_finite));
            let xy = [point[0], point[1]];
            let (min, max) = bounds.get_or_insert((xy, xy));
            for axis in 0..2 {
                min[axis] = min[axis].min(xy[axis]);
                max[axis] = max[axis].max(xy[axis]);
            }
        }
    }
    let (min, max) = bounds.expect("the full inventory contains an emitting WMO");
    let (from, to) = input::support();
    (0..2).all(|axis| {
        f64::from(min[axis])
            <= f64::from(from[axis].max(to[axis])) + f64::from(body().radius.as_f32())
            && f64::from(max[axis])
                >= f64::from(from[axis].min(to[axis])) - f64::from(body().radius.as_f32())
    })
}

struct InstalledFixture {
    app: App,
    tiles: BTreeMap<(i32, i32), Handle<AdtTile>>,
    roots: BTreeMap<(i32, i32), Entity>,
}
impl InstalledFixture {
    fn new(path: &Path, inventory: NativeM2MapInventory) -> Self {
        let mut app = App::new();
        benilla_assets::register_mpq_source(&mut app, path).unwrap();
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
        app.insert_resource(MapCatalogRes(
            benilla_formats::load_map_catalog(&mut Chain::open(path).unwrap()).unwrap(),
        ));
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            32,
        )));
        app.add_systems(
            FixedPostUpdate,
            (|mut observed: ResMut<PhysicsObserved>| observed.0 += 1)
                .after(PhysicsSystems::Last)
                .after(TransformSystems::Propagate),
        );
        let directory = input::MAP_DIRECTORY;
        let server = app.world().resource::<AssetServer>();
        let wdt: Handle<WdtIndex> =
            server.load(format!("mpq://World/Maps/{directory}/{directory}.wdt"));
        let tiles: BTreeMap<_, Handle<AdtTile>> = input::guard_tiles()
            .into_iter()
            .map(|(x, y)| {
                (
                    (x as i32, y as i32),
                    server.load(format!(
                        "mpq://World/Maps/{directory}/{directory}_{x}_{y}.adt"
                    )),
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
                && tiles.values().all(|handle| {
                    app.world()
                        .resource::<Assets<AdtTile>>()
                        .get(handle)
                        .is_some()
                })
            {
                break;
            }
            assert!(
                Instant::now() < until,
                "source-backed WDT/ADT loaders must finish"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        let roots = authored_native_fixture::install_placement_free_guard(
            app.world_mut(),
            directory,
            wdt,
            &tiles,
            inventory,
        );
        app.insert_resource(WorldLive(true));
        Self { app, tiles, roots }
    }

    fn advance_physics(&mut self) {
        let before = self.app.world().resource::<PhysicsObserved>().0;
        self.app.update();
        self.app.update();
        assert!(self.app.world().resource::<PhysicsObserved>().0 > before);
    }

    fn observe(
        &mut self,
        view: &PreparedSourceHeight<'_, '_, '_>,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        source_binding: SnapshotBinding,
    ) -> Result<Observation, ObservationError> {
        self.observe_with(
            view,
            compatibility,
            source_binding,
            deadline(),
            || false,
            |_| {},
        )
    }

    /// No yielding or mutable world access occurs between the two owner checks.
    /// The final hook changes only authored source files/cancellation in negatives.
    fn observe_with(
        &mut self,
        view: &PreparedSourceHeight<'_, '_, '_>,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        source_binding: SnapshotBinding,
        until: Instant,
        cancelled: impl Fn() -> bool,
        after: impl FnOnce(bool),
    ) -> Result<Observation, ObservationError> {
        self.advance_physics();
        let mut state = SystemState::<(
            Res<NativeStaticMapIndex>,
            NativeTerrainCoverage,
            WorldCollision,
            SpatialQuery,
        )>::new(self.app.world_mut());
        let (installed, terrain, collision, spatial) = state.get(self.app.world());
        // Fetch the exact current resource from the same World as all live readers.
        let owner = installed.source_provenance().unwrap();
        compatibility
            .verify_current_owner(owner, until, &cancelled)
            .map_err(ObservationError::Source)?;
        let result = (|| {
            let (from, to) = input::support();
            let mut budget = QueryBudget::new(until);
            let mut feet = [[0.; 3]; 2];
            for (slot, point) in [from, to].into_iter().enumerate() {
                let request = HeightRequest {
                    binding: view.binding(),
                    ticket: slot as u64 + 1,
                    position: [point[0], point[1], point[2] + 1.].map(f32::to_bits),
                    max_search: 10_f32.to_bits(),
                };
                let reply = view
                    .query_with_budget(request, &mut budget)
                    .map_err(|error| ObservationError::Source(error.into()))?;
                assert_eq!(reply.request, request);
                assert_eq!(reply.height, point[2].to_bits());
                feet[slot] = [point[0], point[1], f32::from_bits(reply.height)];
                assert_eq!(feet[slot], point);
            }
            let eye = |point: [f32; 3]| {
                [point[0], point[1], point[2] + body().height.as_f32()].map(f32::to_bits)
            };
            let request = RayRequest {
                snapshot: source_binding,
                ticket: 3,
                from: eye(feet[0]),
                to: eye(feet[1]),
                ignore_m2: false,
            };
            let ray = view
                .source_ray_with_budget(
                    request.ticket,
                    request.from,
                    request.to,
                    &mut budget,
                    &cancelled,
                )
                .map_err(ObservationError::Source)?;
            assert_eq!(ray.request, request);
            assert_eq!(ray.result, StaticRay::Miss);
            let proof = terrain
                .prove(MAP, feet[0], feet[1], body().radius.as_f32())
                .map_err(ObservationError::Coverage)?;
            // Raw geometry has an independent domain, derived from this live proof.
            let raw_revision = core::geometry_revision(
                proof.map,
                proof.wdt_source_digest,
                &proof.tiles,
                &proof.models,
                proof.inventory_digest,
            );
            let mut corridor = LocalCorridor {
                terrain: &terrain,
                collision: &collision,
                spatial: &spatial,
                map: MAP,
                revision: raw_revision,
            };
            corridor
                .certify(feet[0], feet[1], body())
                .map_err(ObservationError::Corridor)?;
            budget
                .complete()
                .map_err(|error| ObservationError::Source(error.into()))?;
            Ok(Observation {
                proof,
                raw_revision,
            })
        })();
        after(result.is_ok());
        compatibility
            .verify_current_owner(owner, until, &cancelled)
            .map_err(ObservationError::Source)?;
        result
    }
}

#[derive(Debug)]
struct Observation {
    proof: CoverageProof,
    raw_revision: [u8; 32],
}
#[derive(Debug)]
enum ObservationError {
    Source(CompositionError),
    Coverage(CoverageError),
    Corridor(Failure),
}

#[test]
fn installed_source_inventory_owner_certifies_concrete_corridor() {
    let install = input::install();
    let before = install.bytes();
    with_sources(&install, |view, source_binding, inventory| {
        assert!(!exterior_intersects_sweep(&inventory));
        let full_digest = inventory.source_digest();
        assert!(inventory.tile_digest((30, 31)).is_some());
        let compatibility = view
            .match_raw_inventory(&inventory, deadline(), || false)
            .unwrap();
        let mut fixture = InstalledFixture::new(install.path(), inventory);
        let observed = fixture
            .observe(view, &compatibility, source_binding)
            .unwrap();
        assert_eq!(observed.proof.inventory_digest, full_digest);
        assert_eq!(
            observed.proof.terrain_root,
            fixture.roots[&(input::CENTER.0 as i32, input::CENTER.1 as i32)]
        );
        assert!(observed.proof.models.is_empty());
        assert!(observed.proof.static_roots.is_empty());
        assert_eq!(observed.proof.tiles.len(), 9);
        assert!(!fixture.tiles.contains_key(&(30, 31)));
        assert_ne!(observed.raw_revision, [0; 32]);
        assert_ne!(observed.raw_revision, view.binding().revision);
        assert_ne!(observed.raw_revision, compatibility.revision());
    });
    assert_eq!(
        install.bytes(),
        before,
        "loaders and observations do not alter source archives"
    );
}

#[test]
fn installed_source_rejects_equal_bytes_replacement_in_the_actual_world() {
    let install = input::install();
    with_sources(&install, |view, source_binding, inventory| {
        let replacement = input::raw_inventory(&install);
        assert_eq!(inventory.source_digest(), replacement.source_digest());
        assert_eq!(
            inventory.source_provenance().unwrap().receipt_digest(),
            replacement.source_provenance().unwrap().receipt_digest()
        );
        let compatibility = view
            .match_raw_inventory(&inventory, deadline(), || false)
            .unwrap();
        let replacement_compatibility = view
            .match_raw_inventory(&replacement, deadline(), || false)
            .unwrap();
        assert_eq!(
            compatibility.revision(),
            replacement_compatibility.revision()
        );
        let mut fixture = InstalledFixture::new(install.path(), inventory);
        let original = fixture
            .observe(view, &compatibility, source_binding)
            .unwrap();
        fixture
            .app
            .insert_resource(NativeStaticMapIndex::from_inventory(replacement).unwrap());
        assert!(matches!(
            fixture.observe(view, &compatibility, source_binding),
            Err(ObservationError::Source(CompositionError::Validation(
                Error::Identity
            )))
        ));
        let current = fixture
            .observe(view, &replacement_compatibility, source_binding)
            .unwrap();
        assert_eq!(
            original.raw_revision, current.raw_revision,
            "content revision cannot establish exact owner identity"
        );
    });
}

#[test]
fn installed_source_rechecks_guard_assets_furnishing_and_complete_support_collider() {
    let install = input::install();
    with_sources(&install, |view, source_binding, inventory| {
        let compatibility = view
            .match_raw_inventory(&inventory, deadline(), || false)
            .unwrap();
        let mut fixture = InstalledFixture::new(install.path(), inventory);
        fixture
            .observe(view, &compatibility, source_binding)
            .unwrap();
        let neighbor = (input::CENTER.0 as i32 - 1, input::CENTER.1 as i32 - 1);
        let handle = fixture.tiles[&neighbor].id();
        let removed = fixture
            .app
            .world_mut()
            .resource_mut::<Assets<AdtTile>>()
            .remove(handle)
            .unwrap();
        assert!(matches!(
            fixture.observe(view, &compatibility, source_binding),
            Err(ObservationError::Coverage(CoverageError::UnloadedTile))
        ));
        fixture
            .app
            .world_mut()
            .resource_mut::<Assets<AdtTile>>()
            .insert(handle, removed)
            .unwrap();
        fixture
            .observe(view, &compatibility, source_binding)
            .unwrap();
        {
            let mut assets = fixture.app.world_mut().resource_mut::<Assets<AdtTile>>();
            assets.get_mut(handle).unwrap().native_source_digest[31] ^= 1;
        }
        assert!(matches!(
            fixture.observe(view, &compatibility, source_binding),
            Err(ObservationError::Coverage(CoverageError::UnsupportedSource))
        ));
        fixture
            .app
            .world_mut()
            .resource_mut::<Assets<AdtTile>>()
            .get_mut(handle)
            .unwrap()
            .native_source_digest[31] ^= 1;
        let root = fixture.roots[&(input::CENTER.0 as i32, input::CENTER.1 as i32)];
        let original = fixture.app.world().get::<Collider>(root).unwrap().clone();
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
            assert!(
                matches!(
                    fixture.observe(view, &compatibility, source_binding),
                    Err(ObservationError::Coverage(
                        CoverageError::ColliderUnavailable
                    ))
                ),
                "mutation {mutation}"
            );
            fixture
                .app
                .world_mut()
                .entity_mut(root)
                .insert(original.clone());
            fixture
                .observe(view, &compatibility, source_binding)
                .unwrap();
        }
        let neighbor_root = fixture.roots[&neighbor];
        let neighbor_collider = fixture
            .app
            .world_mut()
            .entity_mut(neighbor_root)
            .take::<Collider>()
            .unwrap();
        assert!(matches!(
            fixture.observe(view, &compatibility, source_binding),
            Err(ObservationError::Coverage(
                CoverageError::ColliderUnavailable
            ))
        ));
        fixture
            .app
            .world_mut()
            .entity_mut(neighbor_root)
            .insert(neighbor_collider);
        fixture
            .observe(view, &compatibility, source_binding)
            .unwrap();
        authored_native_fixture::unfurnish_guard_tile(fixture.app.world_mut(), neighbor);
        assert!(matches!(
            fixture.observe(view, &compatibility, source_binding),
            Err(ObservationError::Coverage(CoverageError::UnfurnishedTile))
        ));
    });
}

#[test]
fn installed_source_corridor_rejects_unknown_intersecting_physics_geometry() {
    let install = input::install();
    with_sources(&install, |view, source_binding, inventory| {
        let compatibility = view
            .match_raw_inventory(&inventory, deadline(), || false)
            .unwrap();
        let mut fixture = InstalledFixture::new(install.path(), inventory);
        fixture
            .observe(view, &compatibility, source_binding)
            .unwrap();
        let (from, to) = input::support();
        let midpoint = [
            (from[0] + to[0]) * 0.5,
            (from[1] + to[1]) * 0.5,
            from[2] + 1.,
        ];
        let wall = fixture
            .app
            .world_mut()
            .spawn((
                Collider::cuboid(2., 3., 0.25),
                RigidBody::Static,
                CollisionLayers::DEFAULT,
                Transform::from_translation(benilla_assets::coords::wow_to_bevy(midpoint)),
            ))
            .id();
        assert!(matches!(
            fixture.observe(view, &compatibility, source_binding),
            Err(ObservationError::Corridor(Failure::UnsupportedNavigation))
        ));
        fixture.app.world_mut().despawn(wall);
        fixture
            .observe(view, &compatibility, source_binding)
            .unwrap();
    });
}

#[test]
fn installed_source_checks_cancellation_deadline_and_source_changes_after_corridor() {
    let install = input::install();
    with_sources(&install, |view, source_binding, inventory| {
        let compatibility = view
            .match_raw_inventory(&inventory, deadline(), || false)
            .unwrap();
        let mut fixture = InstalledFixture::new(install.path(), inventory);
        assert!(matches!(
            fixture.observe_with(
                view,
                &compatibility,
                source_binding,
                Instant::now(),
                || false,
                |_| {}
            ),
            Err(ObservationError::Source(CompositionError::Validation(
                Error::Deadline
            )))
        ));
        assert!(matches!(
            fixture.observe_with(
                view,
                &compatibility,
                source_binding,
                deadline(),
                || true,
                |_| {}
            ),
            Err(ObservationError::Source(CompositionError::Cancelled))
        ));
        let cancelled = Cell::new(false);
        assert!(matches!(
            fixture.observe_with(
                view,
                &compatibility,
                source_binding,
                deadline(),
                || cancelled.get(),
                |completed| {
                    assert!(completed, "cancellation occurs after the concrete corridor");
                    cancelled.set(true);
                }
            ),
            Err(ObservationError::Source(CompositionError::Cancelled))
        ));
        fixture
            .observe(view, &compatibility, source_binding)
            .unwrap();
        let result = fixture.observe_with(
            view,
            &compatibility,
            source_binding,
            deadline(),
            || false,
            |completed| {
                assert!(
                    completed,
                    "source mutation occurs after the concrete corridor"
                );
                let path = install.path().join("model.MPQ");
                let mut bytes = std::fs::read(&path).unwrap();
                bytes.push(0);
                std::fs::write(path, bytes).unwrap();
            },
        );
        assert!(
            matches!(
                result,
                Err(ObservationError::Source(
                    CompositionError::TerrainSource | CompositionError::Collision(_)
                ))
            ),
            "{result:?}"
        );
    });
}

#[test]
fn installed_source_rejects_new_winner_after_the_concrete_corridor() {
    let install = input::install();
    with_sources(&install, |view, source_binding, inventory| {
        let compatibility = view
            .match_raw_inventory(&inventory, deadline(), || false)
            .unwrap();
        let mut fixture = InstalledFixture::new(install.path(), inventory);
        let result = fixture.observe_with(
            view,
            &compatibility,
            source_binding,
            deadline(),
            || false,
            |completed| {
                assert!(
                    completed,
                    "winner mutation occurs after the concrete corridor"
                );
                std::fs::copy(
                    install.path().join("model.MPQ"),
                    install.path().join("patch.MPQ"),
                )
                .unwrap();
            },
        );
        assert!(
            matches!(
                result,
                Err(ObservationError::Source(
                    CompositionError::TerrainSource | CompositionError::Collision(_)
                ))
            ),
            "{result:?}"
        );
    });
}

#[test]
fn installed_source_rebuilt_exterior_overlap_has_no_live_coverage() {
    // Rebuild raw inventory, source capture and assembly from changed source inputs.
    // Never edit a detached proof or substitute a fabricated exterior index.
    let install = input::Install::new(&input::overlapping_entries());
    let before = install.bytes();
    with_sources(&install, |view, _, inventory| {
        assert!(exterior_intersects_sweep(&inventory));
        let compatibility = view
            .match_raw_inventory(&inventory, deadline(), || false)
            .unwrap();
        let mut fixture = InstalledFixture::new(install.path(), inventory);
        fixture.advance_physics();
        let mut state = SystemState::<(Res<NativeStaticMapIndex>, NativeTerrainCoverage)>::new(
            fixture.app.world_mut(),
        );
        let (installed, terrain) = state.get(fixture.app.world());
        let owner = installed.source_provenance().unwrap();
        let until = deadline();
        compatibility
            .verify_current_owner(owner, until, || false)
            .unwrap();
        let (from, to) = input::support();
        assert_eq!(
            terrain
                .prove(MAP, from, to, body().radius.as_f32())
                .unwrap_err(),
            CoverageError::ColliderUnavailable
        );
        let request = HeightRequest {
            binding: view.binding(),
            ticket: 1,
            position: [from[0], from[1], from[2] + 1.].map(f32::to_bits),
            max_search: 10_f32.to_bits(),
        };
        assert!(
            matches!(view.query(request, until), Err(Error::Missing)),
            "encountered unloaded WMO cannot produce a terrain-only height"
        );
        compatibility
            .verify_current_owner(owner, until, || false)
            .unwrap();
    });
    assert_eq!(install.bytes(), before);
}

#[cfg(test)]
#[path = "installed_source/joined.rs"]
mod joined;

#[cfg(test)]
#[path = "installed_source/cold_commit.rs"]
mod cold_commit;

#[cfg(test)]
#[path = "installed_source/cold_refusals.rs"]
mod cold_refusals;
