// SPDX-License-Identifier: GPL-3.0-or-later
//! Genuine synchronous join; no certificate escapes this immutable ECS observation.
use super::super::super::{
    source_pathfinder_query::resident_joined_tests as runtime,
    source_route_filter::resident::{
        chase_los::water::ResidentChaseWaterDependency,
        joined::{ResidentJoinedColdDependency, ResidentJoinedColdObservation},
        ResidentFilterDependency, ResidentFilterError, ResidentFilterProvider,
    },
};
use super::*;
use benilla_native_nav::SourceSamePolygonBudget;
use benilla_world_runtime::native::AuthorityRequest;

#[derive(Debug)]
struct JoinedObservation {
    source: ResidentJoinedColdObservation,
    raw_revision: [u8; 32],
    owner: benilla_formats::NativeInventorySource,
    segment: products::vmap::RayReply,
    guard_count: usize,
}
#[derive(Debug)]
enum JoinedError {
    Source(ResidentFilterError),
    Coverage(CoverageError),
    Corridor(Failure),
}
impl From<ResidentFilterError> for JoinedError {
    fn from(error: ResidentFilterError) -> Self {
        Self::Source(error)
    }
}
impl InstalledFixture {
    /// Physics advances before borrowing. The installed owner, cold native work,
    /// raw coverage, concrete corridor and independent segment ray share one borrow.
    #[allow(clippy::too_many_arguments)]
    fn join(
        &mut self,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        constructor: ResidentFilterDependency,
        water: ResidentChaseWaterDependency,
        ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        after: impl FnOnce(),
    ) -> Result<(ResidentJoinedColdDependency, JoinedObservation), JoinedError> {
        self.advance_physics();
        let mut state = SystemState::<(
            Res<NativeStaticMapIndex>,
            NativeTerrainCoverage,
            WorldCollision,
            SpatialQuery,
        )>::new(self.app.world_mut());
        let (installed, terrain, collision, spatial) = state.get(self.app.world());
        let owner = installed
            .source_provenance()
            .ok_or(JoinedError::Corridor(Failure::Binding))?;
        provider.verify_installed_owner(compatibility, owner, deadline, &cancelled)?;
        let dependency = provider.observe_joined_cold(
            constructor,
            water,
            context,
            ticket,
            SourceSamePolygonBudget::default(),
            deadline,
            &cancelled,
        )?;
        let deadline = dependency.deadline(deadline);
        let observation = observe(
            &terrain,
            &collision,
            &spatial,
            owner,
            provider,
            &dependency,
            context,
            ticket
                .checked_add(2)
                .ok_or(JoinedError::Corridor(Failure::Invalid))?,
            deadline,
            &cancelled,
        )?;
        after();
        // Recheck BOTH source and exact installed owner after BOTH corridor and ray.
        provider.joined_for(&dependency, context, deadline, &cancelled)?;
        provider.verify_installed_owner(compatibility, owner, deadline, &cancelled)?;
        Ok((dependency, observation))
    }
    #[allow(clippy::too_many_arguments)]
    fn reobserve_joined(
        &mut self,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &ResidentFilterProvider<'_, '_, '_, '_>,
        dependency: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
        ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        after: impl FnOnce(),
    ) -> Result<JoinedObservation, JoinedError> {
        self.advance_physics();
        let deadline = dependency.deadline(deadline);
        let mut state = SystemState::<(
            Res<NativeStaticMapIndex>,
            NativeTerrainCoverage,
            WorldCollision,
            SpatialQuery,
        )>::new(self.app.world_mut());
        let (installed, terrain, collision, spatial) = state.get(self.app.world());
        let owner = installed
            .source_provenance()
            .ok_or(JoinedError::Corridor(Failure::Binding))?;
        provider.verify_installed_owner(compatibility, owner, deadline, &cancelled)?;
        let result = observe(
            &terrain, &collision, &spatial, owner, provider, dependency, context, ticket, deadline,
            &cancelled,
        )?;
        after();
        provider.joined_for(dependency, context, deadline, &cancelled)?;
        provider.verify_installed_owner(compatibility, owner, deadline, &cancelled)?;
        Ok(result)
    }
}
#[allow(clippy::too_many_arguments)]
fn observe<'w, 's>(
    terrain: &NativeTerrainCoverage<'w, 's>,
    collision: &WorldCollision<'w, 's>,
    spatial: &SpatialQuery<'w, 's>,
    owner: &benilla_formats::NativeInventorySource,
    provider: &ResidentFilterProvider<'_, '_, '_, '_>,
    dependency: &ResidentJoinedColdDependency,
    context: &AuthorityRequest,
    ticket: u64,
    deadline: Instant,
    cancelled: impl Fn() -> bool,
) -> Result<JoinedObservation, JoinedError> {
    let source = provider.joined_for(dependency, context, deadline, &cancelled)?;
    let [from, to] = source.terminal.points.map(|p| p.map(f32::from_bits));
    let proof = terrain
        .prove(MAP, from, to, source.body.radius.as_f32())
        .map_err(JoinedError::Coverage)?;
    let raw_revision = core::geometry_revision(
        proof.map,
        proof.wdt_source_digest,
        &proof.tiles,
        &proof.models,
        proof.inventory_digest,
    );
    LocalCorridor {
        terrain,
        collision,
        spatial,
        map: MAP,
        revision: raw_revision,
    }
    .certify(from, to, source.body)
    .map_err(JoinedError::Corridor)?;
    let segment = provider.joined_segment_los(dependency, context, ticket, deadline, cancelled)?;
    Ok(JoinedObservation {
        source,
        raw_revision,
        owner: owner.clone(),
        segment,
        guard_count: proof.tiles.len(),
    })
}

#[test]
fn resident_joined_actual_installed_owner_and_corridor() {
    runtime::with_fixture(
        |install, height, compatibility, inventory, policy, p, query, other| {
            let s = runtime::session(&p);
            let mut provider =
                ResidentFilterProvider::bind_with_policy(&s, &p, height, query, policy.clone())
                    .unwrap();
            let other =
                ResidentFilterProvider::bind_with_policy(&s, &p, height, other, policy).unwrap();
            let mut fixture = InstalledFixture::new(install.path(), inventory);
            let (context, (dependency, observed)) = runtime::runtime_with_route(
                &p,
                &s,
                height,
                compatibility,
                &mut provider,
                |provider, context| {
                    let (constructor, water) =
                        runtime::prerequisites(provider, context, 101, runtime::until());
                    fixture
                        .join(
                            compatibility,
                            provider,
                            context,
                            constructor,
                            water,
                            105,
                            runtime::until(),
                            || false,
                            || {},
                        )
                        .unwrap()
                },
            );
            eprintln!(
                "native114 installed joined: guards={} raw_revision={:?} points={:?} body={:?} segment_ticket={}",
                observed.guard_count,
                observed.raw_revision,
                observed.source.terminal.points,
                observed.source.body,
                observed.segment.request.ticket
            );
            assert_eq!(observed.guard_count, 9);
            assert_ne!(observed.raw_revision, height.binding().revision);
            assert_ne!(observed.raw_revision, compatibility.revision());
            assert!(observed.owner.same_owner(compatibility.owner()));
            assert_eq!(observed.segment.request.ticket, 107);
            assert!(!observed.segment.request.ignore_m2);
            assert_eq!(observed.source.terminal.liquid_tickets, [105, 106]);
            assert!(fixture
                .reobserve_joined(
                    compatibility,
                    &other,
                    &dependency,
                    &context,
                    109,
                    runtime::until(),
                    || false,
                    || {}
                )
                .is_err());

            let replacement = input::raw_inventory(install);
            assert_eq!(
                replacement.source_digest(),
                observed.owner.inventory_digest()
            );
            let replacement_compatibility = height
                .match_raw_inventory(&replacement, runtime::until(), || false)
                .unwrap();
            assert_eq!(
                replacement_compatibility.revision(),
                compatibility.revision()
            );
            fixture
                .app
                .insert_resource(NativeStaticMapIndex::from_inventory(replacement).unwrap());
            for compatibility in [compatibility, &replacement_compatibility] {
                assert!(
                    matches!(
                        fixture.reobserve_joined(
                            compatibility,
                            &provider,
                            &dependency,
                            &context,
                            111,
                            runtime::until(),
                            || false,
                            || {}
                        ),
                        Err(JoinedError::Source(_))
                    ),
                    "equal bytes never replace installed owner"
                );
            }
        },
    );
}

#[test]
fn resident_joined_rechecks_live_geometry_and_final_source_after_segment() {
    runtime::with_fixture(
        |install, height, compatibility, inventory, policy, p, query, _| {
            let s = runtime::session(&p);
            let mut provider =
                ResidentFilterProvider::bind_with_policy(&s, &p, height, query, policy).unwrap();
            let mut fixture = InstalledFixture::new(install.path(), inventory);
            let (context, (dependency, observed)) = runtime::runtime_with_route(
                &p,
                &s,
                height,
                compatibility,
                &mut provider,
                |provider, context| {
                    let (constructor, water) =
                        runtime::prerequisites(provider, context, 101, runtime::until());
                    fixture
                        .join(
                            compatibility,
                            provider,
                            context,
                            constructor,
                            water,
                            105,
                            runtime::until(),
                            || false,
                            || {},
                        )
                        .unwrap()
                },
            );
            let again = |fixture: &mut InstalledFixture| {
                fixture.reobserve_joined(
                    compatibility,
                    &provider,
                    &dependency,
                    &context,
                    109,
                    runtime::until(),
                    || false,
                    || {},
                )
            };
            let neighbor = (input::CENTER.0 as i32 - 1, input::CENTER.1 as i32 - 1);
            let handle = fixture.tiles[&neighbor].id();
            let removed = fixture
                .app
                .world_mut()
                .resource_mut::<Assets<AdtTile>>()
                .remove(handle)
                .unwrap();
            assert!(matches!(
                again(&mut fixture),
                Err(JoinedError::Coverage(CoverageError::UnloadedTile))
            ));
            fixture
                .app
                .world_mut()
                .resource_mut::<Assets<AdtTile>>()
                .insert(handle, removed)
                .unwrap();
            let root = fixture.roots[&(input::CENTER.0 as i32, input::CENTER.1 as i32)];
            let collider = fixture.app.world().get::<Collider>(root).unwrap().clone();
            let mesh = collider.shape_scaled().as_trimesh().unwrap();
            let mut indices = mesh.indices().to_vec();
            indices.pop();
            fixture
                .app
                .world_mut()
                .entity_mut(root)
                .insert(Collider::trimesh(mesh.vertices().to_vec(), indices));
            assert!(matches!(
                again(&mut fixture),
                Err(JoinedError::Coverage(CoverageError::ColliderUnavailable))
            ));
            fixture.app.world_mut().entity_mut(root).insert(collider);
            let [a, b] = observed
                .source
                .terminal
                .points
                .map(|p| p.map(f32::from_bits));
            let middle = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, a[2] + 1.];
            let obstacle = fixture
                .app
                .world_mut()
                .spawn((
                    Collider::cuboid(2., 3., 0.25),
                    RigidBody::Static,
                    CollisionLayers::DEFAULT,
                    Transform::from_translation(benilla_assets::coords::wow_to_bevy(middle)),
                ))
                .id();
            assert!(matches!(
                again(&mut fixture),
                Err(JoinedError::Corridor(Failure::UnsupportedNavigation))
            ));
            fixture.app.world_mut().despawn(obstacle);
            again(&mut fixture).unwrap();
            assert!(fixture
                .reobserve_joined(
                    compatibility,
                    &provider,
                    &dependency,
                    &context,
                    111,
                    Instant::now(),
                    || false,
                    || {}
                )
                .is_err());
            let cancelled = Cell::new(false);
            assert!(fixture
                .reobserve_joined(
                    compatibility,
                    &provider,
                    &dependency,
                    &context,
                    113,
                    runtime::until(),
                    || cancelled.get(),
                    || cancelled.set(true)
                )
                .is_err());
            let entries = input::navigation_entries();
            let mut changed = entries.clone();
            changed
                .iter_mut()
                .find(|(name, _)| name == input::WDT)
                .unwrap()
                .1[20] ^= 1;
            let reached = Cell::new(false);
            assert!(fixture
                .reobserve_joined(
                    compatibility,
                    &provider,
                    &dependency,
                    &context,
                    115,
                    runtime::until(),
                    || false,
                    || {
                        reached.set(true);
                        install.write(&changed);
                    }
                )
                .is_err());
            assert!(
                reached.get(),
                "source mutation follows corridor AND segment ray"
            );
            install.write(&entries);
            again(&mut fixture).unwrap();
            authored_native_fixture::unfurnish_guard_tile(fixture.app.world_mut(), neighbor);
            assert!(matches!(
                again(&mut fixture),
                Err(JoinedError::Coverage(CoverageError::UnfurnishedTile))
            ));
        },
    );
}
