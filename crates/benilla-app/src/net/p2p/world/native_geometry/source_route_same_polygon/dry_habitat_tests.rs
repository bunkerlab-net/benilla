// SPDX-License-Identifier: GPL-3.0-or-later
//! One live habitat-3 owner and one loaded inventory across initialization,
//! primary NearPoint, constructor/update filters, and the real native terminal.
//! The existing strict production Corridor requirement remains unfulfilled.
use super::super::{
    core::{PrimaryNearPointPolicy, SamePolygonRoutePolicy},
    products::{
        dry_route::tests as fixtures,
        same_polygon_fixture::{self, RawMap, TerrainKind},
    },
    source_creature_initialization::{self as initialization, tests as initial},
};
use super::{tests::*, *};
use benilla_world_runtime::native::{
    synthetic_fixture as f, CreatureCapabilityCatalog, CreatureCapabilityRows,
    CreatureInitializationClosure, NativeWorld,
};
use std::sync::Arc;

fn setup() -> Setup {
    setup_with_model(0.5, 2.875)
}
fn setup_with_model(radius: f32, raw_height: f32) -> Setup {
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
    let (mut profile, _, mut inputs) =
        composed_inputs(Arc::clone(&products), revision, &raw.maps());
    install_profile_with_model(
        &mut profile,
        initialization::snapshot_binding(&inputs.height),
        &raw.maps(),
        radius,
        raw_height,
    );
    inputs.projection = products.load_source_query_projection(&profile).unwrap();
    let binding = inputs.binding().unwrap();
    profile.geometry_revision = binding.geometry_revision().unwrap();
    let mut session = source_session(&profile);
    session.primary_near_point = Some(PrimaryNearPointPolicy::from_profile(&profile).unwrap());
    session.same_polygon_route =
        Some(SamePolygonRoutePolicy::from_profile(&profile, f::player(), binding).unwrap());
    let initial_height = products
        .load_dry_map_height_owned(
            inputs.near_configuration.height,
            session.participants.as_ref(),
        )
        .unwrap();
    let bound = inputs.bind(&session, &profile, f::player()).unwrap();
    let world = initialized_world(&profile, &session, initial_height);
    Setup {
        world,
        session,
        routes: bound.route,
        environment: bound.environment,
        near_point: Some(bound.near_point),
    }
}

pub(crate) fn install_profile(
    profile: &mut Profile,
    snapshot: benilla_world_runtime::native::CreatureInitializationSnapshot,
    maps: &benilla_formats::MapCatalog,
) {
    install_profile_with_model(profile, snapshot, maps, 0.5, 2.875);
}
fn install_profile_with_model(
    profile: &mut Profile,
    snapshot: benilla_world_runtime::native::CreatureInitializationSnapshot,
    maps: &benilla_formats::MapCatalog,
    radius: f32,
    raw_height: f32,
) {
    let life = initial::profile_with_alternate_model(
        snapshot,
        profile.geometry_revision,
        2,
        radius,
        2.25,
        raw_height,
    );
    // Reuse the existing composed fixture's starter/player/environment/configuration
    // owners. Replace its entire authored creature initializer/model capability
    // unit together, rather than relabeling an existing ground-only witness.
    let respawn = profile.npcs[0].ai.spawn;
    let npc_combat = profile.npcs[0].combat.clone();
    let configuration = profile
        .pathfinder_configuration
        .take()
        .unwrap()
        .selected()
        .clone();
    profile.npcs = life.npcs;
    profile.npcs[0].combat = npc_combat;
    profile.npcs[0].ai.spawn = respawn;
    profile.source_motion.as_mut().unwrap().npcs = life.source_motion.unwrap().npcs;
    profile.creature_generations = life.creature_generations;
    profile.creature_collision_widths = life.creature_collision_widths;
    profile.creature_collision_heights = life.creature_collision_heights;
    let mut source = life
        .creature_capabilities
        .unwrap()
        .source(2)
        .unwrap()
        .clone();
    let CreatureInitializationClosure::DryWater {
        respawn_position, ..
    } = &mut source.initialization
    else {
        unreachable!()
    };
    *respawn_position = respawn;
    profile.creature_capabilities = Some(
        CreatureCapabilityCatalog::validate_sources(CreatureCapabilityRows::Complete(vec![source]))
            .unwrap(),
    );
    profile.interactions = None;
    benilla_world_runtime::native::synthetic_combat_fixture::install(profile);
    profile
        .initialize_pathfinder_configuration(configuration, maps)
        .unwrap();
    profile.validate().unwrap();
}

pub(crate) fn initialized_world(
    profile: &Profile,
    session: &Session,
    initial_height: MapHeightSnapshot<'static>,
) -> NativeWorld {
    let initializer = initialization::SourceCreatureInitializationProvider::bind(
        session.world,
        profile,
        initial_height,
    )
    .unwrap();
    let mut authority = initial::InitialAuthority {
        provider: initializer,
        initializations: vec![],
        motion: Default::default(),
    };
    // Select an actual seeded life whose model differs from the immutable initial
    // session scalar. No request/witness is edited to force that branch.
    let world = (0..32)
        .find_map(|seed| {
            authority.initializations.clear();
            let world = NativeWorld::synthetic_component_test_with_initialization(
                profile.clone(),
                session.world,
                f::player(),
                seed,
                None,
                &mut authority,
            )
            .unwrap();
            assert_eq!(authority.initializations.len(), 1);
            let selected = world.state().npcs[&2]
                .creature_collision_height()
                .unwrap()
                .witness()
                .height()
                .bits();
            (selected != session.bodies[&2].height.bits()).then_some(world)
        })
        .expect("both authored genders occur in the deterministic seed range");
    assert!(world.state().npcs[&2]
        .creature_capabilities()
        .unwrap()
        .witness()
        .capabilities()
        .unwrap()
        .can_swim());
    world
}

/// Original live dispatch and initialized model, without constructing a replacement receipt.
pub(crate) fn captured_initialized_route_request() -> (Session, AuthorityRequest) {
    let mut setup = setup();
    let (world, mut consumer) = consumer(&mut setup);
    drive_first(world, &mut consumer);
    assert!(consumer.errors.is_empty(), "{:?}", consumer.errors);
    (
        consumer.session.clone(),
        consumer.proposals[0].context.clone(),
    )
}

#[test]
fn loaded_habitat_three_composes_live_near_point_filter_thirteen_and_same_polygon_cache() {
    let mut setup = setup();
    let (world, mut consumer) = consumer(&mut setup);
    drive_first(world, &mut consumer);
    assert_eq!(consumer.near_queries, 1);
    assert!(consumer.errors.is_empty(), "{:?}", consumer.errors);
    assert_eq!(consumer.proposals.len(), 1);
    let proposal = &consumer.proposals[0];
    let Query::SourceMotion(MotionQuery::Route(route)) = &proposal.context.query else {
        unreachable!()
    };
    assert_eq!(route.owner.inhabit_type(), 3);
    let initial_model = world
        .profile()
        .creature_collision_widths
        .as_ref()
        .unwrap()
        .source(2)
        .unwrap()
        .selection();
    assert_ne!(
        proposal.model.selection.combat_reach(),
        initial_model.combat_reach()
    );
    assert_eq!(
        proposal.model.selection.combat_reach(),
        world.state().npcs[&2]
            .initialized_creature()
            .unwrap()
            .model
            .combat_reach
    );
    assert!(route.owner.capabilities().unwrap().can_swim());
    route
        .owner
        .validate_context(world.profile(), &proposal.context.binding)
        .unwrap();
    let body = consumer.session.body(&proposal.context).unwrap();
    assert_ne!(body.height, consumer.session.bodies[&2].height);
    assert_eq!(body.height, route.request.owner_collision_height);
    assert_eq!(
        body.height.bits(),
        route
            .owner
            .initialization()
            .unwrap()
            .request
            .collision_height
            .bits()
    );
    let mut wrong_height = proposal.context.clone();
    let Query::SourceMotion(MotionQuery::Route(wrong)) = &mut wrong_height.query else {
        unreachable!()
    };
    wrong.request.owner_collision_height = consumer.session.bodies[&2].height;
    assert!(consumer.session.body(&wrong_height).is_err());
    for field in 0..2 {
        let mut wire = serde_json::to_value(&proposal.context).unwrap();
        let height = &mut wire["query"]["SourceMotion"]["Route"]["collision_height"];
        if field == 0 {
            height["npc"]["generation"] = serde_json::json!(route.owner.npc().generation + 1);
        } else {
            height["height"] = serde_json::to_value(consumer.session.bodies[&2].height).unwrap();
        }
        let changed: AuthorityRequest = serde_json::from_value(wire).unwrap();
        assert!(consumer.session.body(&changed).is_err());
    }
    let constructor = proposal.constructor_filter.as_ref().unwrap();
    assert_eq!(constructor.filter().include_flags(), 13);
    assert_eq!(constructor.filter().exclude_flags(), 0);
    assert_eq!(
        constructor.filter().branch(),
        benilla_sim::route_filter::Branch::GroundWaterDry
    );
    assert_eq!(proposal.filter.filter().include_flags, 13);
    assert_eq!(
        proposal.terminal().compatibility().filter().include_flags,
        13
    );
    assert_eq!(proposal.after_cache().include_flags, 13);
    assert_eq!(proposal.after_cache().exclude_flags, 0);
    assert_eq!(
        proposal.after_cache().binding.owner_generation,
        route.owner.npc().generation
    );
    assert_eq!(
        proposal.terminal().compatibility().initial_storage(),
        &[0; 74]
    );
    assert_eq!(proposal.terminal().path_type(), 1);
    for observations in [
        constructor.liquid_observations(),
        proposal.filter.liquid_observations(),
    ] {
        assert_eq!(
            observations.map(|o| o.mask),
            [
                benilla_sim::route_filter::LiquidMask::AllLiquids,
                benilla_sim::route_filter::LiquidMask::WaterOrOcean,
            ]
        );
        assert!(observations
            .iter()
            .all(|o| o.reply.request.position == constructor.owner_position()));
    }
    let bound = consumer
        .routes
        .filter
        .begin(consumer.session, &proposal.context)
        .unwrap();
    let retained = proposal.filter.filter();
    let update = proposal.filter.request().clone();
    assert_eq!(
        bound.query_retained_update(update.clone(), SourceOwnerFilter::GROUND, None, until()),
        Err(Failure::Binding)
    );
    let first = bound
        .query_retained_update(update.clone(), retained, None, until())
        .unwrap();
    let mut next_request = update;
    next_request.first_ticket += 4;
    let next = bound
        .query_retained_update(next_request, retained, Some(&first), until())
        .unwrap();
    assert_eq!(next.filter(), retained);
    assert_eq!(
        next.continued_from_ticket(),
        Some(first.request().first_ticket)
    );

    // The same genuine request cannot borrow initialization evidence from another
    // world or from a future creation clock, even when its profile/NPC still match.
    for field in 0..2 {
        let mut wire = serde_json::to_value(&proposal.context).unwrap();
        let b = &mut wire["query"]["SourceMotion"]["Route"]["owner"]["initialization"]["request"]
            ["binding"];
        if field == 0 {
            b["world"] = serde_json::json!(vec![9u8; 16]);
        } else {
            b["tick"] = serde_json::json!(proposal.context.binding.tick + 1);
        }
        let changed: AuthorityRequest = serde_json::from_value(wire).unwrap();
        assert!(consumer
            .routes
            .filter
            .begin(consumer.session, &changed)
            .is_err());
    }
}

#[test]
fn selected_height_above_concrete_body_limit_cannot_be_certified() {
    // The explicit template scale is 1.25: raw height 20 produces a sourced
    // height of 25. Display/render scales do not participate in this predicate.
    let mut setup = setup_with_model(0.5, 20.0);
    let (world, mut consumer) = consumer(&mut setup);
    drive_first(world, &mut consumer);
    assert!(consumer.errors.is_empty(), "{:?}", consumer.errors);
    let context = &consumer.requests[0];
    let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
        unreachable!()
    };
    assert!(route.request.owner_collision_height.as_f32() > 16.0);
    assert!(consumer.session.bodies[&2].height.as_f32() <= 16.0);
    assert_eq!(consumer.session.body(context), Err(Failure::UnresolvedBody));
}

#[test]
fn selected_radius_outside_frozen_roster_is_rejected_before_near_point_or_route() {
    use benilla_world_runtime::native::{synthetic_motion_fixture, Authority, NativeError};
    let mut setup = setup_with_model(0.75, 2.875);
    struct Capture<'a> {
        session: &'a Session,
        environment: &'a SourcePlayerEnvironmentProvider,
        context: Option<AuthorityRequest>,
        other: synthetic_motion_fixture::SyntheticMotionAuthority,
    }
    impl Authority for Capture<'_> {
        fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            match &request.query {
                Query::PlayerEnvironment(_) => Ok(AuthorityReply {
                    request: request.clone(),
                    facts: self
                        .environment
                        .resolve(self.session, request, until())
                        .unwrap(),
                }),
                Query::SourceMotion(MotionQuery::NearPoint(_)) => {
                    self.context = Some(request.clone());
                    Err(NativeError::AuthorityUnavailable(
                        "capture unsupported selected radius",
                    ))
                }
                _ => self.other.resolve(request),
            }
        }
    }
    let mut capture = Capture {
        session: &setup.session,
        environment: &setup.environment,
        context: None,
        other: Default::default(),
    };
    for _ in 0..8 {
        let result = setup
            .world
            .step(setup.world.state().tick + 1, None, &mut capture);
        if capture.context.is_some() {
            assert!(result.is_err());
            break;
        }
        result.unwrap();
    }
    let context = capture.context.unwrap();
    let Query::SourceMotion(MotionQuery::NearPoint(near)) = &context.query else {
        unreachable!()
    };
    assert_ne!(
        near.request.owner_bounding_radius,
        setup.session.bodies[&2].radius
    );
    assert_eq!(
        setup
            .near_point
            .as_ref()
            .unwrap()
            .resolve(&setup.session, &context, until()),
        Err(Failure::UnresolvedBody)
    );
    assert_eq!(setup.routes.retained_candidate_count(), 0);
}

#[test]
fn initialized_chase_staged_gate_preserves_fresh_observations_and_deadline_retry() {
    use super::super::core::{self, ServiceQuery, ServiceStep};
    use crossbeam_channel::TryRecvError;
    let mut fixture = setup();
    let (context, baseline, expected) = {
        let (world, mut consumer) = consumer(&mut fixture);
        drive_first(world, &mut consumer);
        assert!(consumer.errors.is_empty());
        (
            consumer.requests[0].clone(),
            consumer.proposals.pop().unwrap(),
            consumer.observations,
        )
    };
    let (_authority, mailbox) = core::mailbox(fixture.session.clone()).unwrap();
    let (lease, reply) = mailbox.enqueue_service_test_request(1, context).unwrap();
    let original_expiry = lease.deadline();
    let mut observations = vec![];
    for _ in 0..3 {
        assert_eq!(
            mailbox.serve_service_test_slice(1, true, until(), |query, deadline| {
                let ServiceQuery::Route(job) = query else {
                    panic!("Route job")
                };
                assert!(!fixture
                    .routes
                    .advance_job_observed(&job, 1, deadline, |stage| observations.push(stage))
                    .map_err(SamePolygonError::failure)?);
                Ok(ServiceStep::Pending)
            }),
            1
        );
    }
    let before_gate = observations.len();
    let slice = Instant::now() + std::time::Duration::from_millis(20);
    assert!(slice < original_expiry);
    let mut late = vec![];
    assert_eq!(
        mailbox.serve_service_test_slice(1, true, slice, |query, deadline| {
            let ServiceQuery::Route(job) = query else {
                panic!("Route job")
            };
            let result = fixture
                .routes
                .advance_job_observed(&job, 1, deadline, |stage| {
                    late.push(stage);
                    if matches!(
                        stage,
                        Observation::Gate(
                            super::super::source_route_chase_gate::Observation::TargetWater(_)
                        )
                    ) {
                        std::thread::sleep(slice.saturating_duration_since(Instant::now()));
                    }
                });
            assert_eq!(result, Err(SamePolygonError::Dependency(Failure::Deadline)));
            result
                .map(|_| ServiceStep::Pending)
                .map_err(SamePolygonError::failure)
        }),
        1
    );
    assert!(matches!(
        fixture.routes.pending[0].phase,
        staged::Phase::Prepared(_)
    ));
    assert!(matches!(
        late.last(),
        Some(Observation::Gate(
            super::super::source_route_chase_gate::Observation::TargetWater(_)
        ))
    ));
    assert_eq!(late, expected[before_gate..before_gate + late.len()]);
    assert!(matches!(reply.try_result(), Err(TryRecvError::Empty)));
    assert!(lease.is_active());
    assert_eq!(lease.deadline(), original_expiry);
    for _ in 0..2 {
        assert_eq!(
            mailbox.serve_service_test_slice(1, true, until(), |query, deadline| {
                let ServiceQuery::Route(job) = query else {
                    panic!("Route job")
                };
                assert!(!fixture
                    .routes
                    .advance_job_observed(&job, 1, deadline, |stage| observations.push(stage))
                    .map_err(SamePolygonError::failure)?);
                Ok(ServiceStep::Pending)
            }),
            1
        );
    }
    assert_eq!(observations, expected);
    assert_eq!(
        mailbox.serve_service_test_slice(1, true, until(), |query, deadline| {
            let ServiceQuery::Route(job) = query else {
                panic!("Route job")
            };
            assert_eq!(job.recorded_phase(), "native_terminal");
            assert!(fixture.routes.advance_job(&job, 1, deadline).unwrap());
            let proposal = fixture.routes.terminal_for_job(&job).unwrap();
            assert_eq!(proposal.gate, baseline.gate);
            assert_eq!(proposal.model, baseline.model);
            Err(Failure::UnsupportedQuery) // A gate never substitutes for concrete clearance.
        }),
        1
    );
    assert!(matches!(
        reply.try_result(),
        Ok(Err(Failure::UnsupportedQuery))
    ));
    assert!(!lease.is_active());
}
