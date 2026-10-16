// SPDX-License-Identifier: GPL-3.0-or-later
//! Local-only geometry service. This is deliberately inert until a supported native
//! session is installed. See README.md for the restricted domain and remaining gates.
#![expect(
    dead_code,
    reason = "native production session admission remains closed"
)]
pub(super) mod core;
mod products;
#[cfg(test)]
pub(crate) mod real_corridor_fixture;
mod source_ground_height;
mod source_navigation;
mod source_near_point;
mod source_near_point_empty;
mod source_pathfinder_query;
mod source_player_environment;
mod source_route_chase_gate;
mod source_route_filter;
mod source_route_point_path;
mod source_route_same_polygon;
mod source_route_same_polygon_local;
mod source_route_smoothing;
mod source_route_straight_path;
mod source_route_warm_cache;
mod source_vmap;
pub(crate) use self::core::{Body, Failure, LocalAuthority, Session};
use self::core::{Corridor, Mailbox};
use avian3d::prelude::{Collider, ColliderAabb, PhysicsSystems, ShapeCastConfig, SpatialQuery};
use benilla_assets::coords::wow_to_bevy;
use benilla_world::{
    collision::WorldCollision,
    schedule::WorldLive,
    terrain_stream::{CoverageError, NativeTerrainCoverage},
};
use benilla_world_runtime::native::{Facts, Query};
use bevy::{diagnostic::FrameCount, prelude::*, transform::TransformSystems};
pub(crate) fn static_vmap_geometry_revision(
    raw_geometry: [u8; 32],
    snapshot: &products::vmap::VmapSnapshot,
) -> [u8; 32] {
    source_vmap::geometry_revision(raw_geometry, snapshot.binding())
}

pub(crate) fn navigation_geometry_revision(
    raw_geometry: [u8; 32],
    vmap: &products::vmap::VmapSnapshot,
    navigation: &products::navigation::NavigationSnapshot,
) -> [u8; 32] {
    source_navigation::geometry_revision(raw_geometry, vmap.binding(), navigation.binding())
}

/// Installed as a NonSend resource only by future audited native entry. Replacing/dropping
/// it closes the old queue; no renderer or explorer session silently becomes authority.
struct Service {
    mailbox: Mailbox,
    budget: core::FrameBudget,
    vmap: Option<source_vmap::SourceVmapProvider>,
    navigation: Option<source_navigation::SourceNavigationProvider>,
    primary_near_point: Option<source_near_point::SourceNearPointProvider>,
    empty_selector: Option<source_near_point_empty::SourceEmptySelectorNearPointProvider>,
    player_environment: Option<source_player_environment::SourcePlayerEnvironmentProvider>,
    same_polygon_route: Option<source_route_same_polygon::SourceSamePolygonRouteProvider>,
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GeometryQueries;

pub(crate) fn install(world: &mut World, session: Session) -> Result<LocalAuthority, Failure> {
    install_products(world, session, None, None, None, None, None)
}

/// Loaded static geometry adds a movement veto and explicitly selected source LOS.
/// Loading belongs off-thread. This neither opens admission nor replaces source-epoch proof.
pub(crate) fn install_with_static_vmap(
    world: &mut World,
    session: Session,
    snapshot: products::vmap::VmapSnapshot,
    raw_geometry: [u8; 32],
) -> Result<LocalAuthority, Failure> {
    let provider = source_vmap::SourceVmapProvider::bind(&session, snapshot, raw_geometry)?;
    install_products(world, session, Some(provider), None, None, None, None)
}
/// Additional dry navigation veto after raw support and static VMap. No slope or
/// owner PathFinder proof and no native session admission is introduced here.
pub(crate) fn install_with_navigation(
    world: &mut World,
    session: Session,
    vmap: products::vmap::VmapSnapshot,
    navigation: products::navigation::NavigationSnapshot,
    raw_geometry: [u8; 32],
) -> Result<LocalAuthority, Failure> {
    let provider = source_navigation::SourceNavigationProvider::bind(
        &session,
        vmap,
        navigation,
        raw_geometry,
    )?;
    install_products(world, session, None, Some(provider), None, None, None)
}
/// Owns already-loaded immutable products for the bounded original NearPoint branch.
/// Loading and participant/source-epoch proof remain the caller's responsibility.
/// This installs no movement/LOS fallback and does not open native session admission.
pub(crate) fn install_with_primary_near_point(
    world: &mut World,
    mut session: Session,
    profile: &benilla_world_runtime::native::Profile,
    snapshot: products::ground_height::MapHeightSnapshot<'static>,
    configuration: source_near_point::SourceNearPointConfiguration,
) -> Result<LocalAuthority, Failure> {
    session.primary_near_point = Some(core::PrimaryNearPointPolicy::from_profile(profile)?);
    let provider = source_near_point::SourceNearPointProvider::bind(
        &session,
        profile,
        snapshot,
        configuration,
    )?;
    install_products(world, session, None, None, Some(provider), None, None)
}

/// Separately selected empty-list source fallback, using already-loaded products.
/// Exhaustion returns the source point, not a clear walking route or admission.
pub(crate) fn install_with_empty_selector_near_point(
    world: &mut World,
    mut session: Session,
    profile: &benilla_world_runtime::native::Profile,
    snapshot: products::ground_height::MapHeightSnapshot<'static>,
    configuration: source_near_point::SourceNearPointConfiguration,
) -> Result<LocalAuthority, Failure> {
    session.empty_selector_near_point =
        Some(core::EmptySelectorNearPointPolicy::from_profile(profile)?);
    let provider = source_near_point_empty::SourceEmptySelectorNearPointProvider::bind(
        &session,
        profile,
        snapshot,
        configuration,
    )?;
    install_products(world, session, None, None, None, Some(provider), None)
}

/// Separately selected loaded environment observations. Unsupported callbacks remain
/// the runtime owner's responsibility; this does not admit a production session.
pub(crate) fn install_with_player_environment(
    world: &mut World,
    mut session: Session,
    profile: &benilla_world_runtime::native::Profile,
    player: benilla_world_runtime::PlayerId,
    snapshot: products::terrain_wet_liquid::TerrainLiquidSnapshot<'static>,
) -> Result<LocalAuthority, Failure> {
    session.player_environment = Some(core::PlayerEnvironmentPolicy::from_profile(
        profile, player,
    )?);
    let provider = source_player_environment::SourcePlayerEnvironmentProvider::bind(
        &session, profile, player, snapshot,
    )?;
    install_products(world, session, None, None, None, None, Some(provider))
}

fn install_products(
    world: &mut World,
    session: Session,
    vmap: Option<source_vmap::SourceVmapProvider>,
    navigation: Option<source_navigation::SourceNavigationProvider>,
    primary_near_point: Option<source_near_point::SourceNearPointProvider>,
    empty_selector: Option<source_near_point_empty::SourceEmptySelectorNearPointProvider>,
    player_environment: Option<source_player_environment::SourcePlayerEnvironmentProvider>,
) -> Result<LocalAuthority, Failure> {
    let (authority, mailbox) = core::mailbox(session)?;
    world.insert_non_send_resource(Service {
        mailbox,
        budget: default(),
        vmap,
        navigation,
        primary_near_point,
        empty_selector,
        player_environment,
        same_polygon_route: None,
    });
    Ok(authority)
}

/// Install already-loaded source products before driving the runtime. Explicit
/// session capabilities are local transport selection, never production admission.
pub(crate) fn install_with_same_polygon(
    world: &mut World,
    mut session: Session,
    profile: &benilla_world_runtime::native::Profile,
    player: benilla_world_runtime::PlayerId,
    products: source_route_same_polygon::SamePolygonProducts,
) -> Result<LocalAuthority, source_route_same_polygon::SamePolygonError> {
    if session.empty_selector_near_point.is_some() {
        return Err(Failure::UnsupportedQuery.into());
    }
    session.player_environment = Some(core::PlayerEnvironmentPolicy::from_profile(
        profile, player,
    )?);
    session.primary_near_point = Some(core::PrimaryNearPointPolicy::from_profile(profile)?);
    session.same_polygon_route = Some(core::SamePolygonRoutePolicy::from_profile(
        profile,
        player,
        products.binding()?,
    )?);
    let providers = products.bind(&session, profile, player)?;
    let (authority, mailbox) = core::mailbox(session)?;
    world.insert_non_send_resource(Service {
        mailbox,
        budget: default(),
        vmap: None,
        navigation: None,
        primary_near_point: Some(providers.near_point),
        empty_selector: None,
        player_environment: Some(providers.environment),
        same_polygon_route: Some(providers.route),
    });
    Ok(authority)
}

pub(crate) fn uninstall(world: &mut World) {
    world.remove_non_send_resource::<Service>();
}

pub(crate) fn plugin(app: &mut App) {
    // PhysicsPlugins::default uses FixedPostUpdate. Avian updates its collider trees during
    // StepSimulation and writes positions back before Last. Update's streamer runs afterward;
    // a later fixed step observes its attached colliders, rather than querying a stale tree.
    app.configure_sets(
        FixedPostUpdate,
        GeometryQueries
            .after(PhysicsSystems::Last)
            .after(TransformSystems::Propagate),
    );
    app.add_systems(
        FixedPostUpdate,
        process
            .in_set(GeometryQueries)
            .run_if(|world: &World| world.contains_non_send::<Service>()),
    );
}

fn process(
    mut service: Option<NonSendMut<Service>>,
    frame: Res<FrameCount>,
    live: Res<WorldLive>,
    terrain: NativeTerrainCoverage,
    collision: WorldCollision,
    spatial: SpatialQuery,
) {
    let Some(service) = service.as_mut() else {
        return;
    };
    let allowance = service.budget.remaining(frame.0);
    if allowance == 0 {
        return;
    }
    let Service {
        mailbox,
        vmap,
        navigation,
        primary_near_point,
        empty_selector,
        player_environment,
        same_polygon_route,
        ..
    } = &mut **service;
    let used = mailbox.serve_queries_until(allowance, |session, request, body, deadline| {
        if !live.0 {
            return Err(Failure::MissingGeometry);
        }
        if matches!(&request.query, Query::PlayerEnvironment(_)) {
            return player_environment
                .as_ref()
                .ok_or(Failure::MissingGeometry)?
                .resolve(session, request, deadline);
        }
        if matches!(
            &request.query,
            Query::SourceMotion(benilla_world_runtime::native::MotionQuery::NearPoint(_))
        ) {
            if let Some(provider) = empty_selector.as_ref() {
                return provider.resolve(session, request, deadline);
            }
            return primary_near_point
                .as_ref()
                .ok_or(Failure::MissingGeometry)?
                .resolve(session, request, deadline);
        }
        if matches!(
            &request.query,
            Query::SourceSpell(benilla_world_runtime::native::SourceSpellQuery::LineOfSight(_))
                | Query::SourceMotion(benilla_world_runtime::native::MotionQuery::Combat(
                    benilla_world_runtime::native::CombatQuery::NpcLineOfSight(_)
                ))
        ) {
            return if let Some(provider) = navigation.as_ref() {
                provider.line_of_sight(session, request, deadline)
            } else if let Some(provider) = vmap.as_ref() {
                provider.line_of_sight(session, request, deadline)
            } else {
                Err(Failure::MissingGeometry)
            };
        }
        if matches!(
            &request.query,
            Query::SourceMotion(benilla_world_runtime::native::MotionQuery::Route(_))
        ) {
            let provider = same_polygon_route
                .as_mut()
                .ok_or(Failure::MissingGeometry)?;
            let mut corridor = LocalCorridor {
                terrain: &terrain,
                collision: &collision,
                spatial: &spatial,
                map: session.map,
                revision: provider.raw_geometry(),
            };
            return source_route_same_polygon_local::resolve(
                provider,
                session,
                request,
                &mut corridor,
                1,
                deadline,
            )
            .map_err(|error| match error {
                source_route_same_polygon::SamePolygonError::Dependency(failure) => failure,
                _ => Failure::MissingGeometry,
            });
        }
        let mut corridor = LocalCorridor {
            terrain: &terrain,
            collision: &collision,
            spatial: &spatial,
            map: session.map,
            revision: navigation
                .as_ref()
                .map(|nav| nav.raw_geometry())
                .or_else(|| vmap.as_ref().map(|vmap| vmap.raw_geometry()))
                .unwrap_or(session.geometry),
        };
        if let Some(provider) = navigation.as_ref() {
            let mut corridor = provider.corridor(session, request, corridor, deadline)?;
            resolve_query(session, request, body, &mut corridor)
        } else if let Some(provider) = vmap.as_ref() {
            let mut corridor = provider.corridor(session, request, corridor, deadline)?;
            resolve_query(session, request, body, &mut corridor)
        } else {
            resolve_query(session, request, body, &mut corridor)
        }
    });
    service.budget.spend(used);
}

fn resolve_query(
    session: &Session,
    request: &benilla_world_runtime::native::AuthorityRequest,
    body: Body,
    corridor: &mut impl Corridor,
) -> Result<Facts, Failure> {
    match &request.query {
        Query::Movement(m) => core::solve(m, body, corridor).map(Facts::Movement),
        Query::PlayerMovement { .. } => core::solve_player(
            &request.query,
            session.player.ok_or(Failure::UnresolvedBody)?,
            corridor,
        ),
        _ => Err(Failure::UnsupportedQuery),
    }
}

struct LocalCorridor<'a, 'w, 's> {
    terrain: &'a NativeTerrainCoverage<'w, 's>,
    collision: &'a WorldCollision<'w, 's>,
    spatial: &'a SpatialQuery<'w, 's>,
    map: u32,
    revision: [u8; 32],
}
impl Corridor for LocalCorridor<'_, '_, '_> {
    fn certify(&mut self, from: [f32; 3], to: [f32; 3], body: Body) -> Result<(), Failure> {
        // Positive raw geometry proof precedes every no-hit interpretation. No endpoint ray
        // interpolation or floating-point tolerance is used as continuous ground evidence.
        let proof = self
            .terrain
            .prove(self.map, from, to, body.radius.as_f32())
            .map_err(coverage_failure)?;
        if core::geometry_revision(
            proof.map,
            proof.wdt_source_digest,
            &proof.tiles,
            &proof.models,
            proof.inventory_digest,
        ) != self.revision
        {
            return Err(Failure::Binding);
        }
        let feet = wow_to_bevy(from);
        let movement = wow_to_bevy(to) - feet;
        let height = body.height.as_f32();
        // Every intersecting collider must be the supporting terrain or an exact source-bound
        // hull. Uncatalogued dynamic/unsupported geometry is never interpreted as clear space.
        let end = wow_to_bevy(to);
        let radius3 = Vec3::new(body.radius.as_f32(), 0.0, body.radius.as_f32());
        let bounds = ColliderAabb {
            min: feet.min(end) - radius3,
            max: feet.max(end) + radius3 + Vec3::Y * height,
        };
        let (mut found_root, mut only_proven) = (false, true);
        self.spatial
            .aabb_intersections_with_aabb_callback(bounds, |id| {
                found_root |= id == proof.terrain_root;
                only_proven &= id == proof.terrain_root || proof.static_roots.contains(&id);
                only_proven
            });
        if !only_proven {
            return Err(Failure::UnsupportedNavigation);
        }
        if !found_root {
            return Err(Failure::MissingGeometry);
        }
        let radius = body.radius.as_f32();
        // Strict enclosing box, not a claim that native creatures use the client mover's shape.
        let shape = Collider::cuboid(radius * 2.0, height, radius * 2.0);
        // Supporting flat terrain is already proved continuously. Exclude only that root
        // from two-sided static hull checks so floor contact is not mistaken for an obstacle.
        // Trimeshes are hollow: conservatively reject an endpoint body whose AABB intersects
        // any individual source hull AABB, rather than declaring an enclosed body unblocked.
        for endpoint in [feet, end] {
            let min = endpoint - radius3;
            let max = endpoint + radius3 + Vec3::Y * height;
            if proof
                .static_bounds
                .iter()
                .any(|(a, b)| min.cmple(*b).all() && max.cmpge(*a).all())
            {
                return Err(Failure::Obstacle);
            }
        }
        let hull_filter =
            WorldCollision::body_filter().with_excluded_entities([proof.terrain_root]);
        let center = feet + Vec3::Y * (height * 0.5);
        for at in [center, end + Vec3::Y * (height * 0.5)] {
            let mut intersects = false;
            self.spatial.shape_intersections_callback(
                &shape,
                at,
                Quat::IDENTITY,
                &hull_filter,
                |_| {
                    intersects = true;
                    false
                },
            );
            if intersects {
                return Err(Failure::Obstacle);
            }
        }
        if let Ok(direction) = Dir3::new(movement) {
            if self
                .spatial
                .cast_shape(
                    &shape,
                    center,
                    Quat::IDENTITY,
                    direction,
                    &ShapeCastConfig::from_max_distance(movement.length()),
                    &hull_filter,
                )
                .is_some()
            {
                return Err(Failure::Obstacle);
            }
        }
        if self
            .collision
            .cast_body(&shape, feet + Vec3::Y * (height * 0.5), movement, 0.0)
            .is_some()
        {
            return Err(Failure::Obstacle);
        }
        if let Ok(dir) = Dir3::new(movement) {
            // Pinned WorldObject::IsWithinLOS raises origin/target by collision height. The
            // local static domain unions walking and camera faces, including camera-only
            // WMO DETAIL faces. This is not server MOD_M2/ignoreM2 or WMO face-mask parity.
            if self
                .spatial
                .cast_ray(
                    feet + Vec3::Y * height,
                    dir,
                    movement.length(),
                    true,
                    &WorldCollision::conservative_los_filter(),
                )
                .is_some()
            {
                return Err(Failure::Obstacle);
            }
        }
        Ok(())
    }
}
fn coverage_failure(error: CoverageError) -> Failure {
    // Unsupported is different from not-yet-loaded, but neither is an unreachable path.
    match error {
        CoverageError::InvalidInput | CoverageError::MalformedTerrain => Failure::Invalid,
        CoverageError::WrongMap => Failure::Binding,
        CoverageError::UnsupportedMap
        | CoverageError::UnsupportedSource
        | CoverageError::UnsupportedPlacements => Failure::UnsupportedNavigation,
        CoverageError::UnsupportedTerrain | CoverageError::OutsideSupportedTile => {
            Failure::UnsupportedGround
        }
        CoverageError::UnloadedMap
        | CoverageError::UnloadedTile
        | CoverageError::UnfurnishedTile
        | CoverageError::ColliderUnavailable => Failure::MissingGeometry,
    }
}

#[cfg(test)]
pub(crate) use products::authored_ground_height;
#[cfg(test)]
pub(crate) use source_near_point::{
    geometry_revision as authored_near_point_revision, SourceNearPointConfiguration,
    SourceNearPointProvider,
};

#[cfg(test)]
pub(crate) use products::authored_vmap;
#[cfg(test)]
pub(crate) use source_vmap::{geometry_revision as authored_vmap_revision, SourceVmapProvider};

#[cfg(test)]
pub(crate) use source_near_point_empty::{
    geometry_revision as authored_empty_near_point_revision, SourceEmptySelectorNearPointProvider,
};

#[cfg(test)]
pub(crate) use source_route_filter::{
    geometry_revision as authored_route_filter_revision, RouteFilterRequest,
    SourceRouteFilterProvider,
};

#[cfg(test)]
pub(crate) use products::authored_route_smoothing;
#[cfg(test)]
pub(crate) use source_route_smoothing::{
    geometry_revision as authored_route_smoothing_revision, RouteSmoothingRequest,
    SourceRouteSmoothingConfiguration, SourceRouteSmoothingProvider, SourceRouteSmoothingStage,
};

#[cfg(test)]
pub(crate) use products::navigation::{
    SourceSmoothRequest, SourceTileReply, SourceTileRequest, SourceWarmRequest,
};
#[cfg(test)]
pub(crate) use source_route_warm_cache::{
    geometry_revision as authored_warm_cache_revision, SourceRouteWarmCacheProvider,
    WarmCacheConfiguration, WarmCacheRequest,
};

#[cfg(test)]
pub(crate) use source_route_point_path::{
    geometry_revision as authored_point_path_revision, PointPathConfiguration, PointPathRequest,
    SourceRoutePointPathProvider,
};

#[cfg(test)]
pub(crate) use source_route_straight_path::{
    geometry_revision as authored_straight_path_revision, SourceRouteStraightPathProvider,
    StraightPathConfiguration, StraightPathDependency, StraightPathRequest,
};

#[cfg(test)]
pub(crate) use products::navigation::tests::straight_ground_products as authored_route_straight_ground;

#[cfg(test)]
pub(crate) use products::authored_player_environment;
#[cfg(test)]
pub(crate) use source_player_environment::SourcePlayerEnvironmentProvider;

#[cfg(test)]
pub(crate) use products::dry_route::tests::{
    environment as authored_dry_chase_environment,
    snake_products as authored_dry_chase_snake_products,
};
#[cfg(test)]
pub(crate) use source_route_chase_gate::{
    geometry_revision as authored_chase_gate_revision, ChaseGateConfiguration, ChaseGateRejection,
    ChaseGateRequest, ChaseGateSelection, SourceRouteChaseGateProvider,
};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn net_plugin_registers_the_same_inert_geometry_plugin() {
        assert!(include_str!("../../../../net.rs").contains("p2p::native_geometry_plugin(app);"));
        assert!(include_str!("../../../p2p.rs")
            .contains("use native_geometry::plugin as native_geometry_plugin;"));
        let mut app = App::new();
        plugin(&mut app);
        assert!(!app.world().contains_non_send::<Service>());
        app.world_mut().run_schedule(FixedPostUpdate);
    }
    #[derive(Resource, Default)]
    struct Order(Vec<u8>);
    #[test]
    fn production_geometry_registration_runs_after_physics_and_transform_producers() {
        let mut app = App::new();
        plugin(&mut app);
        app.init_resource::<Order>();
        // Insert consumer first so incidental registration order cannot make this pass.
        app.add_systems(
            FixedPostUpdate,
            (|mut o: ResMut<Order>| o.0.push(3)).in_set(GeometryQueries),
        );
        app.add_systems(
            FixedPostUpdate,
            (|mut o: ResMut<Order>| o.0.push(1)).in_set(PhysicsSystems::Last),
        );
        app.configure_sets(
            FixedPostUpdate,
            PhysicsSystems::Last.before(TransformSystems::Propagate),
        );
        app.add_systems(
            FixedPostUpdate,
            (|mut o: ResMut<Order>| o.0.push(2)).in_set(TransformSystems::Propagate),
        );
        app.world_mut().run_schedule(FixedPostUpdate);
        assert_eq!(app.world().resource::<Order>().0, [1, 2, 3]);
    }
}
