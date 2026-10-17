// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored entry-6-shaped RANDOM component, with the exact production corridor.
//! Loaded ADT/GridMap/VMap/MMap bytes and model owners are real fixture inputs;
//! positions, rows, visitor membership and visibility remain expressly authored.
//! The shared initialization scene selects authored Aggressive reaction/faction 2;
//! only entry-6-shaped initialization/addon/habitat fields are retained here.
//! This is neither the actual map-0 spawn import nor native gameplay admission.
use super::{
    core::{self, Body, PrimaryNearPointPolicy, SamePolygonRoutePolicy},
    real_corridor_fixture::Fixture,
    route_commit::{RouteCommitBoundary, RouteCommitNotice},
    source_creature_initialization::{
        self as initialization, SourceCreatureInitializationProvider,
    },
    source_route_same_polygon::{
        dry_habitat_tests, tests::composed_inputs, tests::source_session, SamePolygonProviders,
    },
    source_route_same_polygon_local, Session,
};
use benilla_sim::{
    creature_ai::{self as ai, movement::Check},
    creature_motion as motion,
};
use benilla_world_runtime::native::{
    synthetic_fixture as f, synthetic_two_owner_fixture as visitors, *,
};
use std::time::{Duration, Instant};

const LATE_FAILURE: &str = "authored Wander post-movement failure";

fn until() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

#[test]
fn thin_host_executes_exact_concrete_corridor() {
    let mut geometry = Fixture::new();
    let body = Body {
        radius: f::real(0.5),
        height: f::real(2.0),
    };
    assert_eq!(geometry.certify([5., 5., 0.], [5., 6., 0.], body), Ok(()));
}

struct Consumer {
    geometry: Fixture,
    session: Session,
    providers: SamePolygonProviders,
    initialization: SourceCreatureInitializationProvider<'static>,
    other: visitors::CompleteAuthority,
    boundary: RouteCommitBoundary,
    routes: Vec<AuthorityReply>,
    initializations: Vec<AuthorityRequest>,
    allow_chase: bool,
    fail_after_movement: bool,
    rejected: Option<AuthorityReply>,
}
impl Consumer {
    fn acknowledge(&mut self, world: &NativeWorld) {
        let notice = RouteCommitNotice::capture(world, &self.boundary).unwrap();
        self.providers
            .route
            .acknowledge(&self.session, &notice)
            .unwrap();
        self.boundary = notice.boundary().clone();
    }
}
impl Authority for Consumer {
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        if self.fail_after_movement
            && matches!(&reply.request.query,
                Query::SourceMotion(MotionQuery::Combat(CombatQuery::NpcSemantics(query)))
                    if query.phase == CombatPhase::AfterMovement)
        {
            self.rejected = Some(reply.clone());
            return Err(NativeError::AuthorityUnavailable(LATE_FAILURE));
        }
        self.other.observe_derived(reply)
    }
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        let deadline = until();
        let facts = match &request.query {
            Query::CreatureInitialization(_) => {
                self.initializations.push(request.clone());
                self.initialization.resolve(request, deadline).unwrap()
            }
            Query::PlayerEnvironment(_) => self
                .providers
                .environment
                .resolve(&self.session, request, deadline)
                .unwrap(),
            Query::SourceMotion(MotionQuery::Route(route)) => {
                let is_wander = matches!(
                    route.request.kind,
                    motion::RouteKind::WanderPrepare | motion::RouteKind::Wander
                );
                assert!(
                    is_wander
                        || (self.allow_chase
                            && matches!(
                                route.request.kind,
                                motion::RouteKind::ChaseInitial | motion::RouteKind::ChaseReplan
                            ))
                );
                assert_eq!(route.request.target.is_none(), is_wander);
                assert_eq!(route.owner.inhabit_type(), 3);
                assert!(route.owner.capabilities().unwrap().can_swim());
                assert_eq!(route.owner.swimming_movement_flags(), 0);
                let selected = route.owner.initialization().unwrap();
                assert_eq!(
                    route.request.owner_collision_height.bits(),
                    selected.request.collision_height.bits()
                );
                let facts = self
                    .geometry
                    .with_local_corridor(|corridor| {
                        source_route_same_polygon_local::resolve(
                            &mut self.providers.route,
                            &self.session,
                            request,
                            corridor,
                            1,
                            deadline,
                        )
                    })
                    .map_err(|error| {
                        eprintln!("Wander concrete geometry rejected: {error:?}");
                        NativeError::AuthorityUnavailable("authored loaded Wander geometry")
                    })?;
                core::validate_facts(&self.session, request, &facts).unwrap();
                if route.request.kind == motion::RouteKind::Wander {
                    // An idempotent inspection in this same live dispatch reads
                    // the retained native proposal; it cannot create clearance.
                    let proposal = self
                        .providers
                        .route
                        .calculate(&self.session, request, 1, until())
                        .unwrap();
                    let receipt = proposal
                        .wander_source_receipt()
                        .expect("distinct RANDOM receipt");
                    let Query::SourceMotion(MotionQuery::Route(prefix)) =
                        &receipt.preparation.query
                    else {
                        unreachable!()
                    };
                    assert_eq!(prefix.request.kind, motion::RouteKind::WanderPrepare);
                    assert_eq!(
                        prefix.request.binding.query_id + 1,
                        route.request.binding.query_id
                    );
                    assert_eq!(
                        route.request.wander.as_deref().unwrap().prepared,
                        prefix.request.binding
                    );
                    assert_eq!(
                        receipt.clear_before_active,
                        route
                            .request
                            .prior_cache
                            .as_ref()
                            .map_or(0, |cache| usize::from(cache.active_len))
                    );
                    assert_eq!(receipt.clear_after_active, 0);
                    assert!(receipt.selected);
                    if let Some(constructor) = receipt.constructor_filter {
                        assert_eq!(constructor.filter().include_flags(), 13);
                        assert_eq!(constructor.filter().exclude_flags(), 0);
                    }
                    assert_eq!(receipt.update_filter.filter().include_flags, 13);
                    assert_eq!(receipt.update_filter.filter().exclude_flags, 0);
                    let native = proposal.wander_terminal().unwrap();
                    assert_eq!(native.filter().include_flags, 13);
                    match native.path_type() {
                        1 => {
                            let tail = receipt.tail.expect("successful path executes source tail");
                            let owner = [
                                route.request.origin.x.bits(),
                                route.request.origin.y.bits(),
                                route.request.origin.z.bits(),
                            ];
                            assert_eq!(tail.water.request.position, owner);
                            assert_eq!(
                                tail.water.status,
                                super::products::liquid::LiquidStatus::NoWater
                            );
                            assert_eq!(tail.ground.request.position, owner);
                            assert_eq!(tail.ground.height, 0f32.to_bits());
                            assert!(!tail.above_ground);
                            assert_eq!(native.active_len(), 1);
                        }
                        8 => {
                            assert_eq!(
                                native.no_path_reason(),
                                Some(benilla_native_nav::SourceWanderNoPathReason::MinimumDistance),
                                "this radius-3 fixture selects the near-zero failure, not endpoint lookup/height failures"
                            );
                            assert!(
                                receipt.tail.is_none(),
                                "near-zero return precedes source tail"
                            );
                            let squared = route
                                .request
                                .origin
                                .distance_squared(route.request.goal)
                                .unwrap();
                            assert!(f32::from_bits(squared.bits()) < 0.01);
                            assert_eq!(native.active_len(), 0);
                            assert!(native.points().is_empty());
                            assert!(native.endpoint_lookup().is_none());
                            assert!(native.height_status().is_none());
                            assert!(native.build_trace().is_empty());
                        }
                        path => panic!("unsupported authored Wander terminal {path}"),
                    }
                }
                facts
            }
            Query::SourceMotion(MotionQuery::NearPoint(_)) => {
                if !self.allow_chase {
                    return Err(NativeError::UnsupportedMechanic(
                        "Wander fixture cannot substitute a Chase near-point",
                    ));
                }
                self.providers
                    .near_point
                    .resolve(&self.session, request, deadline)
                    .unwrap()
            }
            _ => return self.other.resolve(request),
        };
        let reply = AuthorityReply {
            request: request.clone(),
            facts,
        };
        if matches!(&request.query, Query::SourceMotion(MotionQuery::Route(route))
            if route.request.kind == motion::RouteKind::Wander)
        {
            self.routes.push(reply.clone());
        }
        Ok(reply)
    }
}

fn setup() -> (NativeWorld, Consumer) {
    setup_with_seed(None)
}
fn setup_with_seed(seed: Option<u64>) -> (NativeWorld, Consumer) {
    setup_with_schedule(seed, None)
}
fn setup_with_schedule(
    seed: Option<u64>,
    initial_aggro_delay_ms: Option<u32>,
) -> (NativeWorld, Consumer) {
    // Keep the shared source-consistent authored Aggressive/faction-2 profile.
    // This does not import actual entry 6's neutral world reaction/content.
    let geometry = Fixture::new();
    let products = geometry.products();
    let maps = &geometry
        .app
        .world()
        .resource::<benilla_assets::MapCatalogRes>()
        .0;
    let (mut profile, _, mut inputs) =
        composed_inputs(products.clone(), geometry.raw_geometry_revision(), maps);
    dry_habitat_tests::install_profile(
        &mut profile,
        initialization::snapshot_binding(&inputs.height),
        maps,
    );
    let center = profile.npcs[0].ai.spawn;
    assert_eq!(center, ai::Point::new(5., 5., 0.).unwrap());
    // This scene deliberately has no visible visitor. The selected source
    // MovementType remains RANDOM=1, never an IDLE relabeling of the real rows.
    profile.player.start = center;
    if let Some(delay) = initial_aggro_delay_ms {
        profile
            .source_motion
            .as_mut()
            .unwrap()
            .creature_initial_aggro_delay_ms = delay;
    }
    profile
        .source_motion
        .as_mut()
        .unwrap()
        .npcs
        .get_mut(&2)
        .unwrap()
        .default_movement = motion::DefaultMovement::Wander {
        center,
        radius: f::real(3.0),
    };
    let mut visitor_input = visitors::visitor_input(&profile, vec![2]);
    let SourceVisitorRows::Complete(owners) = &mut visitor_input.owners else {
        unreachable!()
    };
    owners[0].default_movement_type = 1;
    owners[0].addon_closure = SourceVisitorAddonClosure::GenerationSource {
        source_identity: profile
            .creature_generations
            .as_ref()
            .unwrap()
            .source(2)
            .unwrap()
            .identity(),
    };
    profile.source_visitors = Some(SourceVisitorCatalog::validate_sources(visitor_input).unwrap());
    profile.validate().unwrap();
    inputs.projection = products.load_source_query_projection(&profile).unwrap();
    profile.geometry_revision = inputs.binding().unwrap().geometry_revision().unwrap();
    let mut session = source_session(&profile);
    session.primary_near_point = Some(PrimaryNearPointPolicy::from_profile(&profile).unwrap());
    session.same_polygon_route = Some(
        SamePolygonRoutePolicy::from_profile(&profile, f::player(), inputs.binding().unwrap())
            .unwrap(),
    );
    let height = products
        .clone()
        .load_dry_map_height_owned(
            inputs.near_configuration.height,
            session.participants.as_ref(),
        )
        .unwrap();
    let initial_height = products
        .load_dry_map_height_owned(
            inputs.near_configuration.height,
            session.participants.as_ref(),
        )
        .unwrap();
    let mut providers = inputs.bind(&session, &profile, f::player()).unwrap();
    let world = if let Some(seed) = seed {
        let provider =
            SourceCreatureInitializationProvider::bind(session.world, &profile, initial_height)
                .unwrap();
        let mut authority = initialization::tests::InitialAuthority {
            provider,
            initializations: vec![],
            motion: Default::default(),
        };
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
        world
    } else {
        dry_habitat_tests::initialized_world(&profile, &session, initial_height)
    };
    let initialization =
        SourceCreatureInitializationProvider::bind(session.world, &profile, height).unwrap();
    let boundary = RouteCommitBoundary::capture_initial(&world).unwrap();
    providers
        .route
        .initialize_commit(&session, &boundary)
        .unwrap();
    let mut other = visitors::CompleteAuthority {
        visibility: Some(Check::Failed),
        ..Default::default()
    };
    other.base.base.critical_chance = Some(0);
    other.base.base.resist_chance = Some(0);
    (
        world,
        Consumer {
            geometry,
            session,
            providers,
            initialization,
            other,
            boundary,
            routes: vec![],
            initializations: vec![],
            allow_chase: false,
            fail_after_movement: false,
            rejected: None,
        },
    )
}

fn snapshot(world: &NativeWorld) -> motion::MotionSnapshot {
    world.state().npcs[&2]
        .source_motion
        .as_ref()
        .unwrap()
        .motion
        .snapshot()
}
fn wander(snapshot: &motion::MotionSnapshot) -> &motion::WanderSnapshot {
    let motion::Generator::Wander(wander) = &snapshot.generator else {
        panic!("the default RANDOM owner must stay selected")
    };
    wander
}
fn calculation(reply: &AuthorityReply) -> &motion::RouteCalculation {
    let Facts::SourceMotion(MotionFacts::Spatial(reply)) = &reply.facts else {
        panic!("source spatial route")
    };
    let motion::Fact::RouteCalculation(calculation) = &reply.fact else {
        panic!("loaded source calculation")
    };
    calculation
}
fn launched(record: &TickRecord) -> bool {
    record.events.iter().any(|event| {
        matches!(
            event,
            Event::NpcMotion {
                event: motion::Event::SplineLaunched { .. },
                ..
            }
        )
    })
}
fn assert_dry_owner(world: &NativeWorld) {
    let npc = &world.state().npcs[&2];
    assert_eq!(world.profile().npcs[0].entry, 6);
    assert_eq!(
        world
            .profile()
            .source_visitors
            .as_ref()
            .unwrap()
            .owner(2)
            .unwrap()
            .default_movement_type,
        1
    );
    assert!(
        matches!(world.profile().npc_motion(2).unwrap().default_movement,
        motion::DefaultMovement::Wander { radius, .. } if radius == f::real(3.0))
    );
    let capabilities = npc.creature_capabilities().unwrap().witness();
    assert_eq!(capabilities.inhabit_type(), 3);
    assert!(capabilities.capabilities().unwrap().can_swim());
    assert_eq!(capabilities.swimming_movement_flags(), 0);
    assert_eq!(npc.initialized_creature().unwrap().unit_bytes_2, 0x1001);
    assert_eq!(npc.initialized_creature().unwrap().unit_flags & 0x8000, 0);
    assert_eq!(npc.ai.position(), snapshot(world).map_pose.position);
}
fn step(world: &mut NativeWorld, consumer: &mut Consumer) -> TickRecord {
    let record = world
        .step(world.state().tick + 1, None, consumer)
        .unwrap()
        .clone();
    consumer.acknowledge(world);
    record
}

/// Reject after Unit motion and before the outer transaction commits. The exact
/// checkpoint includes pose, timer/count, RNG, pending events and retained path.
fn reject_late_and_retry(world: &mut NativeWorld, consumer: &mut Consumer) -> TickRecord {
    let before = world.checkpoint().unwrap();
    let motion = snapshot(world);
    let queue = world.state().npcs[&2].events.queue_snapshot();
    let counter = world.state().rng_counter;
    let records = world.records().to_vec();
    let routes = consumer.routes.len();
    let committed = consumer
        .providers
        .route
        .committed_candidate()
        .map(|(p, c)| (p, c.clone()));
    consumer.fail_after_movement = true;
    consumer.rejected = None;
    let tick = world.state().tick + 1;
    assert_eq!(
        world.step(tick, None, consumer),
        Err(NativeError::AuthorityUnavailable(LATE_FAILURE))
    );
    let rejected = consumer
        .rejected
        .take()
        .expect("actual AfterMovement callback");
    assert_eq!(world.checkpoint().unwrap(), before);
    assert_eq!(snapshot(world), motion);
    assert_eq!(world.state().npcs[&2].events.queue_snapshot(), queue);
    assert_eq!(world.state().rng_counter, counter);
    assert_eq!(world.records(), records);
    assert_eq!(
        consumer
            .providers
            .route
            .committed_candidate()
            .map(|(p, c)| (p, c.clone())),
        committed
    );
    let failed_route = consumer.routes.get(routes).cloned();
    consumer.fail_after_movement = false;
    let record = step(world, consumer);
    assert!(
        record.replies.contains(&rejected),
        "retry repeats the entire staged post-movement receipt"
    );
    if let Some(failed) = failed_route {
        assert_eq!(consumer.routes.last(), Some(&failed));
    }
    record
}

fn assert_replay_from(world: &NativeWorld, checkpoint: Checkpoint) {
    let tick = checkpoint.identity().tick;
    let mut replay =
        NativeWorld::restore_synthetic_component_test(world.profile().clone(), checkpoint).unwrap();
    let records: Vec<_> = world
        .records()
        .iter()
        .filter(|record| record.tick > tick)
        .cloned()
        .collect();
    replay.replay(&records).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), world.checkpoint().unwrap());
}

fn assert_loaded_checkpoint_rejected(world: &NativeWorld, checkpoint: &Checkpoint) {
    let encoded = checkpoint.encode().unwrap();
    let decoded = Checkpoint::decode(&encoded).unwrap();
    assert_eq!(&decoded, checkpoint);
    assert_eq!(
        NativeWorld::restore_synthetic_component_test(world.profile().clone(), decoded).err(),
        Some(NativeError::InvalidCheckpoint),
        "serialized source refs cannot rehydrate a loaded native owner"
    );
}

/// Recreate the exact loaded-cache checkpoint via recorded receipts from a
/// cache-free origin, then continue its suffix in the same replay world. No
/// checkpoint restore or live native-query ownership is inferred from replay.
fn assert_replay_through(world: &NativeWorld, genesis: &Checkpoint, checkpoint: &Checkpoint) {
    assert_loaded_checkpoint_rejected(world, checkpoint);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(world.profile().clone(), genesis.clone())
            .unwrap();
    let records: Vec<_> = world
        .records()
        .iter()
        .filter(|record| record.tick > genesis.identity().tick)
        .cloned()
        .collect();
    let split = records.partition_point(|record| record.tick <= checkpoint.identity().tick);
    replay.replay(&records[..split]).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), *checkpoint);
    assert_eq!(
        replay.validate_saved_session_reopen(),
        Err(NativeError::UnsupportedMechanic(
            "saved native PathFinder cache rehydration"
        ))
    );
    replay.replay(&records[split..]).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), world.checkpoint().unwrap());
}

#[test]
fn loaded_random_near_zero_commits_two_draws_then_retries_with_a_new_endpoint() {
    // The versioned runtime stream for world [8;16], seed8 has four genesis
    // draws, then radius107294332/2^32 *3 ~=0.07494. The next radius is
    // 2945402428/2^32 *3 ~=2.05734. No authority answer or RNG word is injected.
    let (mut world, mut consumer) = setup_with_seed(Some(8));
    let initial = world.checkpoint().unwrap();
    let before = snapshot(&world);
    let npc = world.state().npcs[&2].id;
    assert_eq!(world.state().rng_counter, 4);
    assert_eq!(world.profile().tick_ms, 100);
    let failed = reject_late_and_retry(&mut world, &mut consumer);
    assert!(!launched(&failed));
    assert_eq!(failed.draws.len(), 2);
    assert_eq!(failed.draws[0].purpose, DrawPurpose::WanderAngle { npc });
    assert_eq!(failed.draws[1].purpose, DrawPurpose::WanderRadius { npc });
    assert_eq!(failed.draws[0].value, 1_393_456_990);
    assert_eq!(failed.draws[1].value, 107_294_332);
    assert_eq!(world.state().rng_counter, 6);
    let failed_state = snapshot(&world);
    assert!(failed_state.spline.is_none());
    assert_eq!(failed_state.map_pose, before.map_pose);
    assert_eq!(wander(&failed_state).next_move_count, 1);
    assert_eq!(wander(&failed_state).next_move_timer_ms, 500);
    let failed_reply = consumer.routes.last().unwrap().clone();
    let no_path = calculation(&failed_reply);
    assert_eq!(no_path.source, motion::RouteSource::OwnerPathFinder);
    assert_eq!(no_path.outcome, motion::RouteOutcome::NoPath);
    assert!(no_path.prior_cache.is_none());
    assert_eq!(no_path.after_cache.path_type, 8);
    assert_eq!(no_path.after_cache.active_len, 0);
    assert!(no_path
        .after_cache
        .poly_refs
        .iter()
        .all(|reference| *reference == 0));
    assert_eq!(
        no_path.after_cache.requested_end,
        ai::Point::new(0., 0., 0.).unwrap()
    );
    assert_eq!(no_path.after_cache.include_flags, 13);
    assert_eq!(
        wander(&failed_state).pathfinder_cache.as_deref(),
        Some(&no_path.after_cache)
    );
    assert_dry_owner(&world);
    let waiting = world.checkpoint().unwrap();
    let route_count = consumer.routes.len();
    for remaining in [400, 300, 200, 100] {
        let record = reject_late_and_retry(&mut world, &mut consumer);
        assert!(record.draws.is_empty());
        assert!(!launched(&record));
        assert_eq!(consumer.routes.len(), route_count);
        assert_eq!(snapshot(&world).map_pose, before.map_pose);
        assert_eq!(wander(&snapshot(&world)).next_move_timer_ms, remaining);
        assert_eq!(world.state().rng_counter, 6);
    }
    let retry = reject_late_and_retry(&mut world, &mut consumer);
    assert!(launched(&retry));
    assert_eq!(retry.tick, failed.tick + 5);
    assert_eq!(retry.draws.len(), 4);
    assert_eq!(retry.draws[0].purpose, DrawPurpose::WanderAngle { npc });
    assert_eq!(retry.draws[1].purpose, DrawPurpose::WanderRadius { npc });
    assert_eq!(retry.draws[0].value, 831_613_664);
    assert_eq!(retry.draws[1].value, 2_945_402_428);
    assert_eq!(retry.draws[2].purpose, DrawPurpose::WanderMoveCount { npc });
    assert_eq!(retry.draws[3].purpose, DrawPurpose::WanderPause { npc });
    assert_eq!(world.state().rng_counter, 10);
    let success_reply = consumer.routes.last().unwrap();
    let success = calculation(success_reply);
    assert!(matches!(success.outcome, motion::RouteOutcome::Complete(_)));
    assert_eq!(success.prior_cache.as_deref(), Some(&no_path.after_cache));
    assert_eq!(
        success.after_cache.revision,
        no_path.after_cache.revision + 1
    );
    let route_request = |reply: &AuthorityReply| match &reply.request.query {
        Query::SourceMotion(MotionQuery::Route(route)) => route.request.clone(),
        _ => unreachable!(),
    };
    let first = route_request(&failed_reply);
    let second = route_request(success_reply);
    assert_ne!(first.goal, second.goal);
    assert!(second.binding.query_id > first.binding.query_id);
    assert_dry_owner(&world);
    assert_replay_through(&world, &initial, &waiting);
    assert_replay_through(&world, &initial, &world.checkpoint().unwrap());
}

#[test]
fn loaded_random_launch_progress_arrival_pause_second_launch_checkpoint_and_late_rollback() {
    let (mut world, mut consumer) = setup();
    let initial = world.checkpoint().unwrap();
    let before = snapshot(&world);
    assert_eq!(wander(&before).next_move_timer_ms, 0);
    assert_eq!(wander(&before).next_move_count, 1);
    assert!(wander(&before).pathfinder_cache.is_none());
    assert_eq!(
        wander(&before).pathfinder_construction.now_ms,
        world.profile().clock_origin_ms
    );
    assert!(before.spline.is_none());
    assert_dry_owner(&world);

    let first = reject_late_and_retry(&mut world, &mut consumer);
    assert!(
        launched(&first),
        "first Update, not Initialize, launches RANDOM"
    );
    let dispatches: Vec<_> = first
        .replies
        .iter()
        .filter_map(|reply| match &reply.request.query {
            Query::SourceMotion(MotionQuery::Route(route)) => Some(route.request.kind),
            _ => None,
        })
        .collect();
    assert_eq!(
        dispatches,
        [motion::RouteKind::WanderPrepare, motion::RouteKind::Wander]
    );
    let first_state = snapshot(&world);
    let first_spline = first_state.spline.as_ref().unwrap();
    assert!(!first_spline.spline.finalized);
    assert_eq!(first_spline.route_request.kind, motion::RouteKind::Wander);
    assert!(first_spline.walking);
    assert!(first_spline.spline.timestamps_ms.last().copied().unwrap() > 1);
    assert_ne!(
        first_spline.route.points.first(),
        first_spline.route.points.last()
    );
    let first_path = calculation(consumer.routes.last().unwrap());
    assert_eq!(first_path.source, motion::RouteSource::OwnerPathFinder);
    assert_eq!(first_path.after_cache.include_flags, 13);
    assert_eq!(first_path.after_cache.exclude_flags, 0);
    let first_cache = first_path.after_cache.clone();
    let npc = world.state().npcs[&2].id;
    let purposes: Vec<_> = first.draws.iter().map(|draw| draw.purpose).collect();
    assert_eq!(
        purposes,
        [
            DrawPurpose::WanderAngle { npc },
            DrawPurpose::WanderRadius { npc },
            DrawPurpose::WanderMoveCount { npc },
            DrawPurpose::WanderPause { npc }
        ]
    );
    let pause = wander(&first_state).next_move_timer_ms;
    assert!((3000..=10000).contains(&pause));
    assert_eq!(
        first.draws[2].value as u32,
        wander(&first_state).next_move_count
    );
    assert_eq!(first.draws[3].value as u32, pause);
    let moving = world.checkpoint().unwrap();
    assert_loaded_checkpoint_rejected(&world, &moving);

    let duration = *first_spline.spline.timestamps_ms.last().unwrap();
    let frame_limit = (duration + pause).div_ceil(world.profile().tick_ms) + 4;
    let mut arrival = None;
    let mut paused = None;
    let mut saw_progress = false;
    let mut second = None;
    for _ in 0..frame_limit {
        let old = snapshot(&world);
        let was_moving = old.spline.as_ref().is_some_and(|s| !s.spline.finalized);
        let record = if was_moving || paused.is_none() {
            reject_late_and_retry(&mut world, &mut consumer)
        } else {
            step(&mut world, &mut consumer)
        };
        let current = snapshot(&world);
        saw_progress |= current.map_pose.position != before.map_pose.position;
        assert_dry_owner(&world);
        if launched(&record) {
            second = Some(record);
            break;
        }
        if current.spline.as_ref().is_some_and(|s| !s.spline.finalized) {
            assert_eq!(
                wander(&current).next_move_timer_ms,
                pause,
                "moving spline freezes source pause"
            );
        }
        if record.events.iter().any(|event| {
            matches!(
                event,
                Event::NpcMotion {
                    event: motion::Event::SplineAdvanced {
                        event: motion::SplineEvent::Arrived,
                        ..
                    },
                    ..
                }
            )
        }) {
            arrival = Some(world.state().tick);
            assert_eq!(
                current.map_pose.position,
                *first_spline.route.points.last().unwrap()
            );
        }
        if !was_moving && wander(&current).next_move_timer_ms > 0 && paused.is_none() {
            assert!(arrival.is_some());
            assert_eq!(current.map_pose, old.map_pose);
            assert!(wander(&current).next_move_timer_ms < wander(&old).next_move_timer_ms);
            let checkpoint = world.checkpoint().unwrap();
            assert_loaded_checkpoint_rejected(&world, &checkpoint);
            paused = Some(checkpoint);
        }
    }
    assert!(saw_progress, "nonzero authoritative map relocation");
    assert!(arrival.is_some(), "source spline arrived");
    let paused = paused.expect("source timer paused after arrival");
    let second = second.expect("source pause expiration launched another RANDOM leg");
    assert!(second.tick > arrival.unwrap());
    let second_path = calculation(consumer.routes.last().unwrap());
    assert_eq!(second_path.prior_cache.as_deref(), Some(&first_cache));
    assert_eq!(second_path.after_cache.include_flags, 13);
    assert!(second_path.after_cache.revision > first_cache.revision);
    assert_replay_through(&world, &initial, &moving);
    assert_replay_through(&world, &initial, &paused);
}

#[test]
fn loaded_random_visibility_interrupts_at_stale_map_pose_and_replays() {
    // This authored notification delay puts the real initial visibility callback
    // after spline progress but before the 400 ms map relocation publication.
    // Aggressive reaction/faction in this shared component profile are authored,
    // not entry 6's actual neutral reaction closure. Given that explicit input,
    // only directional visibility is selected; source AI chooses AttackStart.
    let (mut world, mut consumer) = setup_with_schedule(Some(0), Some(400));
    assert_eq!(
        world.profile().npcs[0].ai.reaction,
        ai::Reaction::Aggressive
    );
    let initial = world.checkpoint().unwrap();
    let first = step(&mut world, &mut consumer);
    assert!(launched(&first));
    step(&mut world, &mut consumer);
    step(&mut world, &mut consumer);
    let old = snapshot(&world);
    let old_wander = wander(&old).clone();
    let computed = world.state().npcs[&2]
        .source_motion
        .as_ref()
        .unwrap()
        .motion
        .spline_position()
        .unwrap();
    assert_ne!(
        computed, old.map_pose.position,
        "interrupt must occur between map relocation publications"
    );
    assert!(!old.spline.as_ref().unwrap().spline.finalized);
    let moving = world.checkpoint().unwrap();
    consumer.allow_chase = true;
    consumer.other.visibility = Some(Check::Passed);
    let record = reject_late_and_retry(&mut world, &mut consumer);
    let current = snapshot(&world);
    assert!(matches!(current.generator, motion::Generator::Chase(_)));
    assert_eq!(current.suspended_wander.as_ref(), Some(&old_wander));
    assert_eq!(current.map_pose.position, computed);
    assert_dry_owner(&world);
    assert_eq!(
        world.state().npcs[&2].health,
        1,
        "visibility starts combat without damage"
    );
    assert!(
        !record.draws.iter().any(|draw| matches!(
            draw.purpose,
            DrawPurpose::WanderAngle { .. }
                | DrawPurpose::WanderRadius { .. }
                | DrawPurpose::WanderMoveCount { .. }
                | DrawPurpose::WanderPause { .. }
        )),
        "interrupt draws no Wander random values"
    );
    assert!(record.replies.iter().any(|reply| matches!(
        reply.facts,
        Facts::SourceMotion(MotionFacts::VisitorVisibility(Check::Passed))
    )));
    assert!(record.events.iter().any(|event| matches!(event,
        Event::NpcMotion { event: motion::Event::SplineInterrupted { pose, .. }, .. }
            if pose.position == computed)));
    assert!(record.events.iter().any(|event| matches!(event,
        Event::NpcMotion { event: motion::Event::Relocated { pose, notify_map: false, .. }, .. }
            if pose.position == computed)));
    assert_replay_through(&world, &initial, &moving);
    assert_replay_through(&world, &initial, &world.checkpoint().unwrap());
    // HomeForceDestination remains outside the loaded provider's admitted route
    // kinds. This concrete test stops at Chase; Home/Reset semantics have a
    // separate simulation proof, not a concrete-geometry completion claim.
}

#[test]
fn loaded_random_death_interrupts_and_respawn_constructs_a_fresh_owner() {
    let (mut world, mut consumer) = setup();
    let initial = world.checkpoint().unwrap();
    let first = step(&mut world, &mut consumer);
    assert!(launched(&first));
    let old_id = world.state().npcs[&2].id;
    let old_cache = wander(&snapshot(&world)).pathfinder_cache.clone().unwrap();
    let command = f::cast(&world, 60001);
    world
        .step(world.state().tick + 1, Some(command), &mut consumer)
        .unwrap();
    consumer.acknowledge(&world);
    // The authored low-health initializer and nearby caster make the actual
    // projectile lethal before the first two-second regeneration boundary.
    for _ in 0..18 {
        if world.state().npcs[&2].health == 0 {
            break;
        }
        step(&mut world, &mut consumer);
    }
    assert_eq!(
        world.state().npcs[&2].health,
        0,
        "actual lethal source spell"
    );
    assert!(world.records().iter().flat_map(|record| &record.events).any(|event|
        matches!(event, Event::NpcMotion { id, event: motion::Event::SplineInterrupted { .. } } if *id == old_id)),
        "death interrupts the real default spline");
    assert!(snapshot(&world).spline.is_none());
    assert_dry_owner(&world);
    let corpse = world.checkpoint().unwrap();
    let restored =
        NativeWorld::restore_synthetic_component_test(world.profile().clone(), corpse.clone())
            .unwrap();
    assert_eq!(restored.checkpoint().unwrap(), corpse);
    let route_floor = consumer.routes.len();
    // Retained authored source delay: corpse1s plus respawn in inclusive [1,2]s.
    for _ in 0..40 {
        if world.state().npcs[&2].id.generation != old_id.generation {
            break;
        }
        step(&mut world, &mut consumer);
    }
    let current_id = world.state().npcs[&2].id;
    assert_eq!(current_id.generation, old_id.generation + 1);
    assert_dry_owner(&world);
    assert_eq!(consumer.initializations.len(), 1);
    let Query::CreatureInitialization(request) = &consumer.initializations[0].query else {
        unreachable!()
    };
    assert_eq!(request.binding.npc, Some(current_id));
    assert_eq!(request.position, world.profile().npcs[0].ai.spawn);
    for _ in 0..2 {
        if consumer.routes.len() > route_floor {
            break;
        }
        step(&mut world, &mut consumer);
    }
    assert!(
        consumer.routes.len() > route_floor,
        "respawn starts another source RANDOM leg"
    );
    let fresh = calculation(consumer.routes.last().unwrap());
    assert!(fresh.prior_cache.is_none());
    assert_eq!(
        fresh.after_cache.binding.owner_generation,
        current_id.generation
    );
    assert_ne!(
        fresh.after_cache.binding.owner_generation,
        old_cache.binding.owner_generation
    );
    assert_eq!(fresh.after_cache.include_flags, 13);
    assert_replay_from(&world, initial);
    assert_replay_from(&world, corpse);
}
