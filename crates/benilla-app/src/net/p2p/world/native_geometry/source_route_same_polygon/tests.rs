// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual loaded products and live runtime callback; synthetic non-Route world
//! setup remains explicit. No successful Route fact is supplied by this harness.
use super::super::{
    core::{Body, NonRenderParticipants, PlayerMotion},
    products::{
        dry_route::tests as fixtures,
        same_polygon_fixture::{self, RawMap, TerrainKind},
    },
    source_player_environment::SourcePlayerEnvironmentProvider,
};
use super::*;
use benilla_sim::creature_ai as ai;
use benilla_world_runtime::native::{
    synthetic_creature_model_fixture as models, synthetic_fixture as f,
    synthetic_motion_fixture as motion_fixture, synthetic_player_model_fixture as player_models,
    Authority, AuthorityReply, CreatureCollisionWidthRows, CreatureCollisionWidthSource,
    EffectiveDisabledMMapSet, NativeError, NativeWorld, SelectedMMapConfiguration,
};
use std::{sync::Arc, time::Duration};
fn point(x: f32, y: f32, z: f32) -> Point {
    Point::new(x, y, z).unwrap()
}
pub(super) fn until() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
pub(super) fn config() -> SamePolygonConfiguration {
    SamePolygonConfiguration {
        normalization: SourceSamePolygonConfig {
            normalize_z: false,
            ignore_normalization: false,
        },
        line_of_sight_enabled: true,
        cold_budget: SourceSamePolygonBudget::default(),
        retained_budget: SourceSamePolygonBudget {
            lookup_primitives: SOURCE_RETAINED_SAME_POLYGON_MAX_LOOKUP_PRIMITIVES,
            terminal_primitives: 1,
        },
    }
}
pub(super) struct Setup {
    pub(super) world: NativeWorld,
    pub(super) session: Session,
    pub(super) routes: SourceSamePolygonRouteProvider,
    pub(super) environment: SourcePlayerEnvironmentProvider,
    pub(super) near_point: Option<SourceNearPointProvider>,
}
fn setup(configuration: SamePolygonConfiguration) -> Setup {
    let raw_map = RawMap::new(TerrainKind::Flat);
    let (spec, files) = same_polygon_fixture::fixture(&raw_map);
    let products = fixtures::load(spec, &files);
    let environment = fixtures::environment(Arc::clone(&products));
    let mut p = source_profile(&environment, &raw_map.maps());
    let projection = products.load_source_query_projection(&p).unwrap();
    // Read the actual raw inventory. This expected revision is not a Corridor
    // certificate; only the real physics/coverage path may establish that proof.
    let inventory = raw_map.inventory();
    let tiles: Vec<_> = (30..=32)
        .flat_map(|y| (30..=32).map(move |x| (x, y)))
        .map(|tile| (tile, inventory.tile_digest(tile).unwrap()))
        .collect();
    let raw = super::super::core::geometry_revision(
        p.map_id,
        inventory.wdt_digest(),
        &tiles.try_into().unwrap(),
        &[],
        inventory.source_digest(),
    );
    p.geometry_revision = SamePolygonBinding::from_projection(raw, &projection, configuration)
        .unwrap()
        .geometry_revision()
        .unwrap();
    let session = source_session(&p);
    // Load and bind before constructing/driving the runtime. Every consume below
    // occurs inside the real callback, never from a captured unclaimed event.
    let routes = SourceSamePolygonRouteProvider::bind(
        &session,
        &p,
        f::player(),
        projection,
        raw,
        configuration,
    )
    .unwrap();
    let environment =
        SourcePlayerEnvironmentProvider::bind(&session, &p, f::player(), environment).unwrap();
    let world = NativeWorld::synthetic_component_test(p, session.world, f::player(), 1234).unwrap();
    Setup {
        world,
        session,
        routes,
        environment,
        near_point: None,
    }
}
pub(super) struct Consumer<'a> {
    pub(super) session: &'a Session,
    pub(super) routes: &'a mut SourceSamePolygonRouteProvider,
    pub(super) environment: &'a SourcePlayerEnvironmentProvider,
    pub(super) near_point: Option<&'a SourceNearPointProvider>,
    pub(super) near_queries: usize,
    pub(super) base: motion_fixture::SyntheticMotionAuthority,
    pub(super) proposals: Vec<SamePolygonProposal>,
    pub(super) observations: Vec<Observation>,
    pub(super) requests: Vec<AuthorityRequest>,
    pub(super) errors: Vec<SamePolygonError>,
    pub(super) expire_after_terminal: bool,
    pub(super) retry_same_dispatch: bool,
}
impl Authority for Consumer<'_> {
    fn resolve(&mut self, context: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        match &context.query {
            Query::PlayerEnvironment(_) => {
                let facts = self
                    .environment
                    .resolve(self.session, context, until())
                    .unwrap();
                Ok(AuthorityReply {
                    request: context.clone(),
                    facts,
                })
            }
            Query::SourceMotion(MotionQuery::NearPoint(_)) if self.near_point.is_some() => {
                self.near_queries += 1;
                let facts = self
                    .near_point
                    .unwrap()
                    .resolve(self.session, context, until())
                    .unwrap();
                Ok(AuthorityReply {
                    request: context.clone(),
                    facts,
                })
            }
            Query::SourceMotion(MotionQuery::Route(_)) => {
                self.requests.push(context.clone());
                // Use the ordinary direct-provider window to reach Terminal;
                // the failure hook then deliberately crosses that exact deadline.
                let deadline = until();
                match self
                    .routes
                    .calculate_observed(self.session, context, 1, deadline, |o| {
                        self.observations.push(o);
                        if self.expire_after_terminal && o == Observation::Terminal {
                            std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
                        }
                    }) {
                    Ok(proposal) => {
                        if self.retry_same_dispatch {
                            let repeated = self
                                .routes
                                .calculate(self.session, context, 1, until())
                                .expect("same live callback must remain retryable");
                            assert!(Rc::ptr_eq(&proposal.candidate, &repeated.candidate));
                            self.proposals.push(repeated);
                        }
                        self.proposals.push(proposal);
                    }
                    Err(error) => self.errors.push(error),
                }
                // No fabricated route advances the runtime past missing strict proof.
                Err(NativeError::AuthorityUnavailable(
                    "strict production Corridor unavailable",
                ))
            }
            _ => self.base.resolve(context),
        }
    }
}
pub(super) fn consumer<'a>(setup: &'a mut Setup) -> (&'a mut NativeWorld, Consumer<'a>) {
    (
        &mut setup.world,
        Consumer {
            session: &setup.session,
            routes: &mut setup.routes,
            environment: &setup.environment,
            near_point: setup.near_point.as_ref(),
            near_queries: 0,
            base: Default::default(),
            proposals: vec![],
            observations: vec![],
            requests: vec![],
            errors: vec![],
            expire_after_terminal: false,
            retry_same_dispatch: false,
        },
    )
}
pub(super) fn drive_first(world: &mut NativeWorld, consumer: &mut Consumer<'_>) {
    for _ in 0..8 {
        let before = world.checkpoint().unwrap();
        let result = world.step(world.state().tick + 1, None, consumer);
        if !consumer.requests.is_empty() {
            assert!(result.is_err());
            assert_eq!(world.checkpoint().unwrap(), before);
            return;
        }
        result.unwrap();
    }
    panic!("no genuine runtime Route callback");
}
#[test]
fn preloaded_callback_orders_constructor_gate_have_tile_update_and_real_terminal() {
    let mut setup = setup(config());
    let (world, mut consumer) = consumer(&mut setup);
    drive_first(world, &mut consumer);
    assert!(consumer.errors.is_empty(), "{:?}", consumer.errors);
    assert_eq!(consumer.proposals.len(), 1);
    let p = &consumer.proposals[0];
    assert!(p.dispatch.first_construction());
    assert!(p
        .context
        .require_pathfinder_dispatch(world.profile())
        .is_ok());
    assert_eq!(p.terminal().query_capacity(), 1024);
    assert_eq!(p.terminal().compatibility().initial_storage(), &[0; 74]);
    assert_eq!(p.terminal().compatibility().initial_active_len(), 0);
    assert_eq!(p.terminal().compatibility().storage().len(), 75);
    assert_eq!(p.terminal().path_type(), 1);
    assert_eq!(p.after_cache().revision, 1);
    assert_eq!(p.after_cache().source, motion::RouteSource::OwnerPathFinder);
    assert!(p.tiles.as_ref().unwrap().both_present());
    assert!(f32::from_bits(p.model.owner_height.height().bits()) > 0.);
    assert!(!p.owner_ignore.as_ref().unwrap().ignore_pathfinding());
    let ctor = p.constructor_filter.as_ref().unwrap();
    assert_eq!(ctor.request().first_ticket, 1);
    assert_eq!(p.filter.request().first_ticket, 5);
    assert_eq!(
        consumer.observations,
        vec![
            Observation::ConstructorQuery,
            Observation::ConstructorFilter,
            Observation::Gate(GateObservation::LineOfSight(
                p.gate.as_ref().unwrap().line_of_sight.unwrap()
            )),
            Observation::Gate(GateObservation::OwnerWater(
                p.gate.as_ref().unwrap().owner_water.unwrap()
            )),
            Observation::Gate(GateObservation::TargetWater(false)),
            Observation::SelectedQuery,
            Observation::OwnerIgnore,
            Observation::HaveTile,
            Observation::CalculateFilter,
            Observation::Terminal,
        ]
    );
    let first = Rc::clone(&p.candidate);
    let before = world.checkpoint().unwrap();
    let tick = world.state().tick + 1;
    assert!(world.step(tick, None, &mut consumer).is_err());
    assert_eq!(world.checkpoint().unwrap(), before);
    assert_eq!(consumer.requests[0], consumer.requests[1]);
    assert!(!consumer.requests[0]
        .require_pathfinder_dispatch(world.profile())
        .unwrap()
        .same_live_dispatch(
            consumer.requests[1]
                .require_pathfinder_dispatch(world.profile())
                .unwrap()
        ));
    assert_eq!(consumer.proposals.len(), 2);
    assert!(Rc::ptr_eq(&first, &consumer.proposals[1].candidate));
    assert_eq!(consumer.routes.candidates[0].proposals.len(), 1);
    assert_eq!(
        first
            .terminal
            .chase()
            .unwrap()
            .compatibility()
            .storage()
            .len(),
        75
    );
}
#[test]
fn source_terminal_still_returns_unavailable_and_cannot_become_a_route_fact() {
    let mut setup = setup(config());
    let (world, mut consumer) = consumer(&mut setup);
    drive_first(world, &mut consumer);
    assert!(consumer.errors.is_empty(), "{:?}", consumer.errors);
    assert_eq!(
        consumer
            .routes
            .resolve(consumer.session, &consumer.requests[0], 10, until()),
        Err(SamePolygonError::StrictProductionCorridorUnavailable)
    );
    assert_eq!(consumer.routes.candidates[0].proposals.len(), 1);
}
#[test]
fn changed_or_serialized_requests_cannot_select_native_candidates() {
    let mut setup = setup(config());
    let (world, mut consumer) = consumer(&mut setup);
    drive_first(world, &mut consumer);
    assert!(consumer.errors.is_empty(), "{:?}", consumer.errors);
    let context = &consumer.requests[0];
    let restored: AuthorityRequest =
        serde_json::from_value(serde_json::to_value(context).unwrap()).unwrap();
    assert!(consumer
        .routes
        .calculate(consumer.session, &restored, 1, until())
        .is_err());
    let mut forged = context.clone();
    if let Query::SourceMotion(MotionQuery::Route(route)) = &mut forged.query {
        route.request.prior_cache = Some(Box::new(consumer.proposals[0].after_cache().clone()));
    }
    assert!(consumer
        .routes
        .calculate(consumer.session, &forged, 1, until())
        .is_err());
    assert_eq!(consumer.routes.candidates[0].proposals.len(), 1);
}
#[test]
fn exhausted_lookup_and_deadline_leave_zero_candidate_and_retry_is_cold() {
    let mut c = config();
    c.cold_budget.lookup_primitives = 0;
    let mut setup = setup(c);
    let (world, mut consumer) = consumer(&mut setup);
    drive_first(world, &mut consumer);
    assert!(consumer.proposals.is_empty());
    assert!(!consumer.errors.is_empty());
    assert!(consumer.routes.candidates[0].proposals.is_empty());
    assert_eq!(
        consumer
            .routes
            .calculate(consumer.session, &consumer.requests[0], 1, Instant::now())
            .err(),
        Some(SamePolygonError::Dependency(Failure::Deadline))
    );
    assert!(consumer.routes.candidates[0].proposals.is_empty());
}
#[test]
fn explicit_normalization_raw_geometry_and_tickets_are_bound() {
    let mut setup = setup(config());
    let original = setup.routes.binding;
    assert_eq!(setup.routes.raw_geometry(), original.raw_geometry);
    for field in 0..4 {
        let mut changed = original;
        match field {
            0 => changed.raw_geometry[0] ^= 1,
            1 => changed.selected_configuration[0] ^= 1,
            2 => changed.vmap.policy[0] ^= 1,
            _ => changed.configuration.retained_budget.lookup_primitives -= 1,
        }
        assert_ne!(
            changed.geometry_revision().unwrap(),
            original.geometry_revision().unwrap()
        );
    }
    for ignore in [false, true] {
        let mut changed = original;
        if ignore {
            changed.configuration.normalization.ignore_normalization = true
        } else {
            changed.configuration.normalization.normalize_z = true
        }
        assert_eq!(changed.geometry_revision(), Err(Failure::UnsupportedQuery));
    }
    let (world, mut consumer) = consumer(&mut setup);
    drive_first(world, &mut consumer);
    for ticket in [0, u64::MAX] {
        assert!(consumer
            .routes
            .calculate(consumer.session, &consumer.requests[0], ticket, until())
            .is_err());
    }
    assert_eq!(consumer.routes.candidates[0].proposals.len(), 1);
}

// Shared authored owner setup; geometry and Route success are never supplied here.
pub(crate) fn source_profile(
    environment: &super::super::products::terrain_wet_liquid::TerrainLiquidSnapshot<'_>,
    maps: &benilla_formats::MapCatalog,
) -> Profile {
    let mut p = player_models::unowned_profile();
    let b = environment.binding();
    p.map_id = b.map;
    p.content_digest = b.content;
    p.npcs[0].ai.map_id = p.map_id;
    p.npcs[0].ai.content_revision = ai::movement::Revision(p.content_digest);
    p.npcs[0].ai.spawn = point(5., 5., 0.);
    p.npcs[0].display_id = 34;
    p.npcs[0].faction_template = 2;
    p.npcs[0].ai.reaction = ai::Reaction::Aggressive;
    p.player.start = point(5., 13., 0.);
    p.initialize_creature_capabilities(motion_fixture::creature_capability_sources(&p))
        .unwrap();
    let catalog = models::catalog_with_height_fields(1.375, 1.6, 2.5, 1.75, false);
    p.initialize_creature_collision_widths(CreatureCollisionWidthRows::Complete(vec![
        CreatureCollisionWidthSource::from_initial_model(2, models::selection(), &catalog).unwrap(),
    ]))
    .unwrap();
    p.initialize_creature_collision_heights().unwrap();
    p.initialize_player_collision_height(&player_models::catalog(), &catalog)
        .unwrap();
    p.initialize_player_environment(
        environment.environment_binding(),
        environment.environment_catalog().clone(),
    )
    .unwrap();
    benilla_world_runtime::native::synthetic_combat_fixture::install(&mut p);
    p.initialize_pathfinder_configuration(
        SelectedMMapConfiguration {
            enabled: true,
            effective_disabled_maps: EffectiveDisabledMMapSet::Complete(Default::default()),
            source_instance_id: 0,
        },
        maps,
    )
    .unwrap();
    p
}

pub(crate) fn source_session(p: &Profile) -> Session {
    let source = p.source_motion.as_ref().unwrap();
    Session {
        world: [8; 16],
        profile: p.digest(),
        policy: p.authority_policy,
        content: p.content_digest,
        geometry: p.geometry_revision,
        map: p.map_id,
        bodies: p
            .npcs
            .iter()
            .map(|npc| {
                let spawn = npc.ai.creature_id;
                (
                    spawn,
                    Body {
                        radius: npc.ai.bounding_radius,
                        height: source
                            .npc(spawn)
                            .unwrap()
                            .npc_policy
                            .settings
                            .collision_height,
                    },
                )
            })
            .collect(),
        player: Some(PlayerMotion {
            body: Body {
                radius: p.player.bounding_radius,
                height: source.player_collision_height,
            },
            speed: ai::Real::new(7.).unwrap(),
        }),
        participants: Some(NonRenderParticipants::authored_fixture(
            p.content_digest,
            p.map_id,
        )),
        static_los: None,
        primary_near_point: None,
        empty_selector_near_point: None,
        source_location: None,
        player_environment: Some(PlayerEnvironmentPolicy::from_profile(p, f::player()).unwrap()),
        same_polygon_route: None,
    }
}

pub(crate) fn composed_inputs(
    products: Arc<super::super::products::VmapLiquidProducts>,
    raw_geometry: [u8; 32],
    maps: &benilla_formats::MapCatalog,
) -> (Profile, Session, SamePolygonProducts) {
    let environment = fixtures::environment(Arc::clone(&products));
    let mut profile = source_profile(&environment, maps);
    let participants =
        NonRenderParticipants::authored_fixture(profile.content_digest, profile.map_id);
    let height_configuration = same_polygon_fixture::height_configuration(&products);
    let height = Arc::clone(&products)
        .load_dry_map_height_owned(height_configuration, Some(&participants))
        .unwrap();
    let inputs = SamePolygonProducts {
        projection: products.load_source_query_projection(&profile).unwrap(),
        environment,
        height,
        raw_geometry,
        configuration: config(),
        near_configuration: SourceNearPointConfiguration {
            height: height_configuration,
            detect_position_collision: true,
            vmap_line_of_sight_enabled: true,
        },
    };
    let binding = inputs.binding().unwrap();
    profile.geometry_revision = binding.geometry_revision().unwrap();
    let mut session = source_session(&profile);
    session.primary_near_point =
        Some(super::super::core::PrimaryNearPointPolicy::from_profile(&profile).unwrap());
    session.same_polygon_route = Some(
        super::super::core::SamePolygonRoutePolicy::from_profile(&profile, f::player(), binding)
            .unwrap(),
    );
    (profile, session, inputs)
}

pub(super) fn composed_setup() -> Setup {
    let raw = RawMap::new(TerrainKind::Flat);
    let (spec, files) = same_polygon_fixture::fixture(&raw);
    let products = fixtures::load(spec, &files);
    let inventory = raw.inventory();
    let tiles: Vec<_> = (30..=32)
        .flat_map(|y| (30..=32).map(move |x| (x, y)))
        .map(|tile| (tile, inventory.tile_digest(tile).unwrap()))
        .collect();
    let revision = super::super::core::geometry_revision(
        0,
        inventory.wdt_digest(),
        &tiles.try_into().unwrap(),
        &[],
        inventory.source_digest(),
    );
    let (profile, session, inputs) = composed_inputs(products, revision, &raw.maps());
    let bound = inputs.bind(&session, &profile, f::player()).unwrap();
    Setup {
        world: NativeWorld::synthetic_component_test(profile, session.world, f::player(), 1234)
            .unwrap(),
        session,
        routes: bound.route,
        environment: bound.environment,
        near_point: Some(bound.near_point),
    }
}

#[test]
fn shared_components_execute_genuine_near_point_before_source_ordered_route() {
    let mut setup = composed_setup();
    let (world, mut consumer) = consumer(&mut setup);
    drive_first(world, &mut consumer);
    assert_eq!(consumer.near_queries, 1);
    assert!(consumer.errors.is_empty(), "{:?}", consumer.errors);
    let proposal = &consumer.proposals[0];
    assert!(consumer.session.body(&proposal.context).is_ok());
    let ray = consumer
        .routes
        .segment_line_of_sight(consumer.session, proposal, 7, until())
        .unwrap();
    let q = match &proposal.context.query {
        Query::SourceMotion(MotionQuery::Route(route)) => &route.request,
        _ => unreachable!(),
    };
    let height = q.owner_collision_height.as_f32();
    assert_eq!(
        ray.request.from[2],
        (q.origin.z.as_f32() + height).to_bits()
    );
    assert_eq!(ray.request.to[2], (q.goal.z.as_f32() + height).to_bits());
    assert!(!ray.request.ignore_m2);
    assert_ne!(
        ray.request.to[2],
        proposal
            .gate
            .as_ref()
            .unwrap()
            .line_of_sight
            .unwrap()
            .request
            .to[2]
    );
    let mut closed = consumer.session.clone();
    closed.same_polygon_route = None;
    assert_eq!(
        closed.body(&proposal.context),
        Err(Failure::UnsupportedQuery)
    );
    let mut changed = proposal.context.clone();
    changed.binding.tick += 1;
    assert!(consumer.session.body(&changed).is_err());
    let mut changed = proposal.context.clone();
    if let Query::SourceMotion(MotionQuery::Route(route)) = &mut changed.query {
        route.request.binding.query_id += 1;
    }
    assert!(consumer.session.body(&changed).is_err());
}

#[test]
fn commit_boundary_tracks_no_route_records_and_rejects_other_attachments_atomically() {
    let mut setup = setup(config());
    let boundary = RouteCommitBoundary::capture_initial(&setup.world).unwrap();
    setup
        .routes
        .initialize_commit(&setup.session, &boundary)
        .unwrap();
    setup
        .routes
        .initialize_commit(&setup.session, &boundary)
        .unwrap();
    let other = RouteCommitBoundary::capture_initial(&setup.world).unwrap();
    assert_ne!(boundary.session(), other.session());
    assert_eq!(
        setup.routes.initialize_commit(&setup.session, &other),
        Err(Failure::Binding)
    );
    let (world, mut consumer) = consumer(&mut setup);
    world.step(1, None, &mut consumer).unwrap();
    assert!(
        consumer.requests.is_empty(),
        "initial activation tick is route-free"
    );
    let notice = RouteCommitNotice::capture(world, &boundary).unwrap();
    let wrong_session = RouteCommitNotice::capture(world, &other).unwrap();
    assert!(notice.routes().is_empty());
    assert_eq!(
        consumer
            .routes
            .acknowledge(consumer.session, &wrong_session),
        Err(Failure::Binding)
    );
    assert_eq!(consumer.routes.committed_boundary.as_ref(), Some(&boundary));
    consumer
        .routes
        .acknowledge(consumer.session, &notice)
        .unwrap();
    consumer
        .routes
        .acknowledge(consumer.session, &notice)
        .unwrap();
    assert_eq!(
        consumer.routes.committed_boundary.as_ref(),
        Some(notice.boundary())
    );
    assert_eq!(consumer.routes.retained_candidate_count(), 0);
    assert!(consumer.routes.committed_candidate().is_none());
}

#[test]
fn earlier_uncommitted_and_monolithic_geometry_identities_cannot_initialize_commit_session() {
    let mut setup = setup(config());
    let boundary = RouteCommitBoundary::capture_initial(&setup.world).unwrap();
    for (domain, policy) in [
        (b"benilla-owned-dry-same-polygon-chase-proposal-v1".as_slice(),
         b"ordinary-zero-offset-chase;straight-true;force-false;exact-two-point-endpoints;uncommitted".as_slice()),
        (b"benilla-owned-dry-same-polygon-chase-commit-v2".as_slice(),
         b"ordinary-zero-offset-chase;straight-true;force-false;exact-two-point-endpoints;outer-commit-ack;total-cap4".as_slice()),
    ] {
        let legacy = setup.routes.binding.geometry_revision_for_policy(domain, policy).unwrap();
        assert_ne!(legacy, setup.session.geometry);
        let mut legacy_session = setup.session.clone();
        legacy_session.geometry = legacy;
        assert_eq!(setup.routes.initialize_commit(&legacy_session, &boundary), Err(Failure::Binding));
        assert!(setup.routes.committed_boundary.is_none());
    }
}

#[test]
fn deadline_after_actual_terminal_keeps_world_uncommitted_and_retry_claims_fresh_dispatch() {
    let mut setup = setup(config());
    let (world, mut consumer) = consumer(&mut setup);
    consumer.expire_after_terminal = true;
    drive_first(world, &mut consumer);
    assert_eq!(consumer.observations.last(), Some(&Observation::Terminal));
    assert_eq!(
        consumer.errors,
        [SamePolygonError::Dependency(Failure::Deadline)]
    );
    assert!(consumer.proposals.is_empty());
    assert_eq!(consumer.routes.candidates.len(), 1);
    assert!(consumer.routes.candidates[0].proposals.is_empty());
    let first = consumer.requests[0].clone();
    let before = world.checkpoint().unwrap();
    consumer.expire_after_terminal = false;
    // This still refuses Route publication: only the real LocalCorridor may
    // convert the actual candidate. The second native calculation is genuine.
    assert!(world
        .step(world.state().tick + 1, None, &mut consumer)
        .is_err());
    assert_eq!(world.checkpoint().unwrap(), before);
    assert_eq!(consumer.proposals.len(), 1);
    assert_eq!(consumer.routes.candidates[0].proposals.len(), 1);
    assert_eq!(consumer.proposals[0].after_cache().revision, 1);
    let retry = consumer.requests.last().unwrap();
    assert_eq!(retry, &first);
    assert!(!first
        .require_pathfinder_dispatch(world.profile())
        .unwrap()
        .same_live_dispatch(retry.require_pathfinder_dispatch(world.profile()).unwrap()));
}

#[test]
fn same_live_dispatch_repeats_real_calculation_without_another_constructor_or_candidate() {
    let mut setup = setup(config());
    let (world, mut consumer) = consumer(&mut setup);
    consumer.retry_same_dispatch = true;
    drive_first(world, &mut consumer);
    assert!(consumer.errors.is_empty());
    assert_eq!(consumer.proposals.len(), 2);
    assert_eq!(consumer.routes.queries.constructor_count(), 1);
    assert_eq!(consumer.routes.candidates.len(), 1);
    assert_eq!(consumer.routes.candidates[0].proposals.len(), 1);
    assert!(Rc::ptr_eq(
        &consumer.proposals[0].candidate,
        &consumer.proposals[1].candidate
    ));
    assert_eq!(
        consumer.proposals[0].after_cache(),
        consumer.proposals[1].after_cache()
    );
}

#[test]
fn attempt_validation_retains_exact_route_binding_and_rejects_mutation_or_decoding() {
    let mut fixture = composed_setup();
    let (world, mut consumer) = consumer(&mut fixture);
    consumer.retry_same_dispatch = true;
    drive_first(world, &mut consumer);
    assert!(consumer.errors.is_empty());
    // Timing is retained without formatting inside the provider's slice.
    let proposal = &consumer.proposals[0];
    let timing = proposal.timing.as_ref().unwrap();
    assert!(timing.count > 0);
    assert_eq!(timing.stages[timing.count - 1].unwrap().0, "terminal");
    proposal.report_timing();
    let context = &consumer.requests[0];
    consumer
        .session
        .with_validated_request(context, |validated| {
            assert!(std::ptr::eq(validated.session(), consumer.session));
            assert!(std::ptr::eq(validated.request(), context));
            let npc = context.binding.npc.unwrap().spawn;
            assert_eq!(validated.body(), consumer.session.bodies[&npc]);
            for mutation in 0..6 {
                let mut changed = consumer.session.clone();
                match mutation {
                    0 => changed.world[0] ^= 1,
                    1 => changed.profile[0] ^= 1,
                    2 => changed.geometry[0] ^= 1,
                    3 => changed.policy[0] ^= 1,
                    4 => changed.bodies.get_mut(&npc).unwrap().height = ai::Real::new(99.).unwrap(),
                    5 => changed.participants = None,
                    _ => unreachable!(),
                }
                assert!(
                    changed.with_validated_request(context, |_| ()).is_err(),
                    "session mutation {mutation}"
                );
            }
            for mutation in 0..6 {
                let mut changed = context.clone();
                match mutation {
                    0 => changed.binding.tick += 1,
                    1 => changed.binding.npc.as_mut().unwrap().generation += 1,
                    2 => changed.binding.world[0] ^= 1,
                    3 => changed.binding.geometry[0] ^= 1,
                    4 | 5 => {
                        let Query::SourceMotion(MotionQuery::Route(route)) = &mut changed.query
                        else {
                            unreachable!()
                        };
                        if mutation == 4 {
                            route.request.binding.query_id += 1;
                        } else {
                            route.request.goal.x = ai::Real::new(99.).unwrap();
                        }
                    }
                    _ => unreachable!(),
                }
                assert!(
                    consumer
                        .session
                        .with_validated_request(&changed, |_| ())
                        .is_err(),
                    "request mutation {mutation}"
                );
            }
            let decoded: AuthorityRequest =
                serde_json::from_value(serde_json::to_value(context).unwrap()).unwrap();
            assert!(consumer
                .session
                .with_validated_request(&decoded, |_| ())
                .is_err());

            // The body receipt grants nothing to a provider bound to another policy.
            let mut foreign = setup(config());
            assert!(foreign
                .routes
                .calculate(validated.session(), validated.request(), 1, until())
                .is_err());
            assert_eq!(foreign.routes.queries.constructor_count(), 0);
            assert!(foreign.routes.candidates.is_empty());
        })
        .unwrap();
}

/// Genuine unclaimed runtime dispatch for mailbox identity/continuation tests.
/// No Route result or private source receipt is authored here.
pub(crate) fn captured_route_request() -> (Session, AuthorityRequest) {
    let mut setup = composed_setup();
    struct Capture<'a> {
        setup: &'a Setup,
        request: Option<AuthorityRequest>,
        other: motion_fixture::SyntheticMotionAuthority,
    }
    impl Authority for Capture<'_> {
        fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            let facts = match &request.query {
                Query::PlayerEnvironment(_) => self
                    .setup
                    .environment
                    .resolve(&self.setup.session, request, until())
                    .unwrap(),
                Query::SourceMotion(MotionQuery::NearPoint(_)) => self
                    .setup
                    .near_point
                    .as_ref()
                    .unwrap()
                    .resolve(&self.setup.session, request, until())
                    .unwrap(),
                Query::SourceMotion(MotionQuery::Route(_)) => {
                    self.request = Some(request.clone());
                    return Err(NativeError::AuthorityUnavailable(
                        "capture genuine staged Route",
                    ));
                }
                _ => return self.other.resolve(request),
            };
            Ok(AuthorityReply {
                request: request.clone(),
                facts,
            })
        }
    }
    // Keep provider ownership before world construction exactly as composed_setup;
    // move only the independently owned world so Capture can borrow its products.
    let profile = setup.world.profile().clone();
    let mut world = std::mem::replace(
        &mut setup.world,
        NativeWorld::synthetic_component_test(profile, setup.session.world, f::player(), 1234)
            .unwrap(),
    );
    let mut capture = Capture {
        setup: &setup,
        request: None,
        other: Default::default(),
    };
    for _ in 0..8 {
        let result = world.step(world.state().tick + 1, None, &mut capture);
        if let Some(request) = capture.request.take() {
            assert!(result.is_err());
            return (setup.session.clone(), request);
        }
        result.unwrap();
    }
    panic!("no genuine Route dispatch");
}

#[test]
fn retained_filter_context_is_exact_provider_owned_and_raw_validation_still_rejects_changes() {
    let fixture = composed_setup();
    let (session, context) = captured_route_request();
    let prepared = fixture
        .routes
        .filter
        .prepare_context(&session, &context)
        .unwrap();
    assert!(fixture.routes.filter.reborrow(&prepared).is_ok());
    let foreign = composed_setup();
    assert!(foreign.routes.filter.begin(&session, &context).is_ok());
    assert!(matches!(
        foreign.routes.filter.reborrow(&prepared),
        Err(Failure::Binding)
    ));
    let mut changed = context.clone();
    changed.binding.tick += 1;
    assert!(fixture
        .routes
        .filter
        .prepare_context(&session, &changed)
        .is_err());
    let mut changed = session.clone();
    changed.bodies.clear();
    assert!(fixture
        .routes
        .filter
        .prepare_context(&changed, &context)
        .is_err());
    // Failed raw attempts cannot mutate or replace the already-checked owned context.
    assert!(fixture.routes.filter.reborrow(&prepared).is_ok());
}
