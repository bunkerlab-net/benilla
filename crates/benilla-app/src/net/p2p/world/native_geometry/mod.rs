// SPDX-License-Identifier: GPL-3.0-or-later
//! Local-only geometry service. This is deliberately inert until a supported native
//! session is installed. See README.md for the restricted domain and remaining gates.
#![expect(
    dead_code,
    reason = "native production session admission remains closed"
)]
pub(super) mod core;
#[cfg(all(test, not(benilla_headless_geometry)))]
mod frame_cadenced_controller_tests;
mod local_corridor;
mod products;
#[cfg(not(benilla_headless_geometry))]
#[cfg(test)]
mod resident_result_controller_tests;
pub(crate) fn run_collision_diagnostics(
    args: &[std::ffi::OsString],
    output: &mut impl std::io::Write,
) -> std::io::Result<()> {
    products::vmap::diagnostic::run(args, output)
}

use local_corridor::LocalCorridor;
#[cfg(test)]
pub(crate) mod real_corridor_fixture;
pub(crate) mod route_commit;
mod source_cache_reconstruction;
#[cfg(all(test, not(benilla_headless_geometry)))]
pub(crate) use source_cache_reconstruction::CompletedReconstruction;
mod source_creature_initialization;
mod source_ground_height;
mod source_location;
mod source_navigation;
mod source_near_point;
mod source_near_point_empty;
mod source_pathfinder_query;
mod source_player_environment;
mod source_route_chase_gate;
mod source_route_filter;
mod source_route_point_path;
// Component-only certification; runtime admission requires a separate reviewed boundary.
#[cfg(test)]
mod source_route_resident_continuation;
#[cfg(test)]
mod source_route_resident_controller;
#[cfg(all(test, not(benilla_headless_geometry)))]
mod source_route_resident_endpoint_refusals;
#[cfg(test)]
mod source_route_resident_finite_loop;
#[cfg(test)]
mod source_route_resident_finite_owner;
#[cfg(test)]
mod source_route_resident_history;
#[cfg(test)]
mod source_route_resident_local;
#[cfg(all(test, not(benilla_headless_geometry)))]
mod source_route_resident_pair;
#[cfg(test)]
mod source_route_resident_planar_refusals;
#[cfg(test)]
mod source_route_resident_repeated;
#[cfg(all(test, not(benilla_headless_geometry)))]
mod source_route_resident_restart;
#[cfg(test)]
mod source_route_resident_retained;
#[cfg(test)]
mod source_route_resident_retained_refusals;
#[cfg(all(test, not(benilla_headless_geometry)))]
mod source_route_resident_source_spline_refusals;
#[cfg(test)]
mod source_route_resident_straight_clearance;
#[cfg(test)]
mod source_route_resident_straight_refusals;
#[cfg(test)]
mod source_route_resident_supported_clearance;
#[cfg(test)]
mod source_route_resident_supported_refusals;
#[cfg(test)]
mod source_route_resident_third_history;
#[cfg(test)]
mod source_route_resident_three_epoch;
mod source_route_same_polygon;
mod source_route_same_polygon_local;
mod source_route_smoothing;
mod source_route_straight_path;
mod source_route_warm_cache;
mod source_vmap;
pub(crate) use self::core::{Body, Failure, LocalAuthority, Session};
use self::core::{Corridor, Mailbox, ServiceQuery, ServiceStep};
use avian3d::prelude::{PhysicsSystems, SpatialQuery};
use benilla_world::{
    collision::WorldCollision, schedule::WorldLive, terrain_stream::NativeTerrainCoverage,
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
    source_location: Option<source_location::SourceLocationProvider>,
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

/// Select an independent owned GetZoneAndAreaId reader. This has no liquid
/// shortcut and cannot construct participant, spell closure or admission proof.
pub(crate) fn install_with_source_location(
    world: &mut World,
    mut session: Session,
    profile: &benilla_world_runtime::native::Profile,
    player: benilla_world_runtime::PlayerId,
    snapshot: products::terrain_area::TerrainAreaSnapshot<'static>,
) -> Result<LocalAuthority, Failure> {
    session.source_location = Some(core::SourceLocationPolicy::from_profile(profile, player)?);
    let provider =
        source_location::SourceLocationProvider::bind(&session, profile, player, snapshot)?;
    let (authority, mailbox) = core::mailbox(session)?;
    world.insert_non_send_resource(Service {
        mailbox,
        budget: default(),
        vmap: None,
        navigation: None,
        primary_near_point: None,
        empty_selector: None,
        player_environment: None,
        source_location: Some(provider),
        same_polygon_route: None,
    });
    Ok(authority)
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
        source_location: None,
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
) -> Result<
    (LocalAuthority, core::MailboxRouteCommitSink),
    source_route_same_polygon::SamePolygonError,
> {
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
    let (mut authority, mailbox) = core::mailbox(session)?;
    let sink = authority.take_route_commit_sink().ok_or(Failure::Binding)?;
    world.insert_non_send_resource(Service {
        mailbox,
        budget: default(),
        vmap: None,
        navigation: None,
        primary_near_point: Some(providers.near_point),
        empty_selector: None,
        player_environment: Some(providers.environment),
        source_location: None,
        same_polygon_route: Some(providers.route),
    });
    Ok((authority, sink))
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
    if let Some(provider) = service.same_polygon_route.as_mut() {
        provider.reap_jobs();
    }
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
        source_location,
        same_polygon_route,
        ..
    } = &mut **service;
    let controls = mailbox.serve_route_commits(allowance, |session, command| {
        let provider = same_polygon_route
            .as_mut()
            .ok_or(Failure::UnsupportedQuery)?;
        match command {
            core::RouteCommitCommand::Initialize(boundary) => {
                provider.initialize_commit(session, boundary)
            }
            core::RouteCommitCommand::Commit(notice) => provider.acknowledge(session, notice),
        }
    });
    let stage_routes = same_polygon_route.is_some();
    let queries = mailbox.serve_service_queries_until(allowance - controls, stage_routes, |query, deadline| {
        let validated = match query {
            ServiceQuery::Immediate(validated) => validated,
            ServiceQuery::NearPoint(job) => {
                if !live.0 { return Err(Failure::MissingGeometry); }
                return primary_near_point.as_ref().ok_or(Failure::MissingGeometry)?
                    .resolve_job(job, deadline)
                    .map(|facts| core::ServiceStep::Complete(Box::new(facts)));
            }
            ServiceQuery::Route(job) => {
                if !live.0 { return Err(Failure::MissingGeometry); }
                let provider = same_polygon_route.as_mut().ok_or(Failure::MissingGeometry)?;
                #[cfg(test)]
                let tick = job.request().binding.tick;
                let mut corridor = LocalCorridor {
                    terrain: &terrain, collision: &collision, spatial: &spatial,
                    map: job.session().map, revision: provider.raw_geometry(),
                };
                return source_route_same_polygon_local::resolve_job(provider, job, &mut corridor,
                    1, deadline).map_err(|error| {
                    #[cfg(test)]
                    eprintln!("native geometry staged route failure: tick={tick} error={error:?} deadline_expired={}",
                        std::time::Instant::now() >= deadline);
                    error.failure()
                });
            }
        };
        (|| {
        let session = validated.session();
        let request = validated.request();
        let body = validated.body();
        if !live.0 {
            return Err(Failure::MissingGeometry);
        }
        if matches!(&request.query, Query::SourceLocation(_)) {
            return source_location
                .as_ref()
                .ok_or(Failure::MissingGeometry)?
                .resolve(session, request, deadline);
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
            return source_route_same_polygon_local::resolve_validated(
                provider,
                validated,
                &mut corridor,
                1,
                deadline,
            )
            .map_err(|error| {
                #[cfg(test)]
                eprintln!(
                    "native geometry service route failure: tick={} error={error:?} deadline_expired={}",
                    request.binding.tick,
                    std::time::Instant::now() >= deadline,
                );
                error.failure()
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
        })().map(|facts| ServiceStep::Complete(Box::new(facts)))
    });
    if let Some(provider) = same_polygon_route.as_mut() {
        provider.reap_jobs();
    }
    service.budget.spend(controls + queries);
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

#[cfg(all(test, not(benilla_headless_geometry)))]
mod retained_phased_controller_tests;
