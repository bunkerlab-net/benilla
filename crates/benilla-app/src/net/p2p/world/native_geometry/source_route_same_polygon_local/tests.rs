// SPDX-License-Identifier: GPL-3.0-or-later
//! Real Route geometry with authored owner/combat setup. Non-geometry callbacks
//! stay explicit component fixtures; this is not production native admission.
use super::super::{
    core,
    real_corridor_fixture::Fixture,
    route_commit::{RouteCommitBoundary, RouteCommitNotice, RouteCommitSink},
    source_route_same_polygon::{tests::composed_inputs, SamePolygonProviders},
};
use super::*;
use benilla_world_runtime::native::{
    synthetic_fixture as f, synthetic_motion_fixture as motion_fixture, Authority, AuthorityReply,
    Command, Input, NativeError, NativeWorld, OwnedInput,
};
use std::time::Duration;

struct Consumer {
    geometry: Fixture,
    session: Session,
    providers: SamePolygonProviders,
    other: motion_fixture::SyntheticMotionAuthority,
    replies: Vec<AuthorityReply>,
    fail_after_route: bool,
    boundary: RouteCommitBoundary,
    failure_reply_floor: usize,
    route_errors: Vec<SamePolygonError>,
}
impl Consumer {
    fn acknowledge(&mut self, world: &NativeWorld) -> RouteCommitNotice {
        let notice = RouteCommitNotice::capture(world, &self.boundary).unwrap();
        let candidate = notice
            .routes()
            .last()
            .and_then(|reply| self.providers.route.candidate_identity(reply));
        self.providers
            .route
            .acknowledge(&self.session, &notice)
            .unwrap();
        let committed = self.providers.route.committed_candidate();
        if let Some(candidate) = candidate {
            let head = notice
                .boundary()
                .heads()
                .iter()
                .find_map(|head| head.cache());
            if let Some(head) = head {
                assert_eq!(committed, Some((candidate, head)));
            }
        }
        assert!(self.providers.route.retained_candidate_count() <= 1);
        let before = committed.map(|(pointer, cache)| (pointer, cache.clone()));
        self.providers
            .route
            .acknowledge(&self.session, &notice)
            .unwrap();
        assert_eq!(
            self.providers
                .route
                .committed_candidate()
                .map(|(pointer, cache)| (pointer, cache.clone())),
            before
        );
        self.boundary = notice.boundary().clone();
        self.failure_reply_floor = self.replies.len();
        notice
    }
}
impl Authority for Consumer {
    fn observe_derived(&mut self, _: &AuthorityReply) -> Result<(), NativeError> {
        if self.fail_after_route && self.replies.len() > self.failure_reply_floor {
            Err(NativeError::AuthorityUnavailable(
                "authored later-frame failure",
            ))
        } else {
            Ok(())
        }
    }
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if self.fail_after_route && self.replies.len() > self.failure_reply_floor {
            return Err(NativeError::AuthorityUnavailable(
                "authored later-frame failure",
            ));
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        let facts = match &request.query {
            Query::PlayerEnvironment(_) => self
                .providers
                .environment
                .resolve(&self.session, request, deadline)
                .unwrap(),
            Query::SourceMotion(MotionQuery::NearPoint(_)) => self
                .providers
                .near_point
                .resolve(&self.session, request, deadline)
                .unwrap(),
            Query::SourceMotion(MotionQuery::Route(_)) => self
                .geometry
                .with_local_corridor(|corridor| {
                    super::resolve(
                        &mut self.providers.route,
                        &self.session,
                        request,
                        corridor,
                        1,
                        deadline,
                    )
                })
                .map_err(|error| {
                    self.route_errors.push(error);
                    NativeError::AuthorityUnavailable("authored real Route refusal")
                })?,
            _ => return self.other.resolve(request),
        };
        core::validate_facts(&self.session, request, &facts).unwrap();
        let reply = AuthorityReply {
            request: request.clone(),
            facts,
        };
        if matches!(request.query, Query::SourceMotion(MotionQuery::Route(_))) {
            self.replies.push(reply.clone());
        }
        Ok(reply)
    }
}
fn setup() -> (NativeWorld, Consumer) {
    setup_with(|_| {})
}
fn setup_with(
    configure: impl FnOnce(&mut benilla_world_runtime::native::Profile),
) -> (NativeWorld, Consumer) {
    let geometry = Fixture::new();
    let (mut profile, _, products) = composed_inputs(
        geometry.products(),
        geometry.raw_geometry_revision(),
        &geometry
            .app
            .world()
            .resource::<benilla_assets::MapCatalogRes>()
            .0,
    );
    // Keep both source-calculated goals inside the precision fixture's first polygon.
    profile.player.start = benilla_sim::creature_ai::Point::new(5., 11., 0.).unwrap();
    configure(&mut profile);
    let mut session = super::super::source_route_same_polygon::tests::source_session(&profile);
    session.primary_near_point =
        Some(core::PrimaryNearPointPolicy::from_profile(&profile).unwrap());
    session.same_polygon_route = Some(
        core::SamePolygonRoutePolicy::from_profile(
            &profile,
            f::player(),
            products.binding().unwrap(),
        )
        .unwrap(),
    );
    // Actual registry and all providers exist before the runtime is constructed.
    let mut providers = products.bind(&session, &profile, f::player()).unwrap();
    let world =
        NativeWorld::synthetic_component_test(profile, session.world, f::player(), 1234).unwrap();
    let boundary = RouteCommitBoundary::capture_initial(&world).unwrap();
    providers
        .route
        .initialize_commit(&session, &boundary)
        .unwrap();
    (
        world,
        Consumer {
            geometry,
            session,
            providers,
            other: Default::default(),
            replies: vec![],
            fail_after_route: false,
            boundary,
            failure_reply_floor: 0,
            route_errors: vec![],
        },
    )
}
fn calculation(reply: &AuthorityReply) -> &motion::RouteCalculation {
    let Facts::SourceMotion(MotionFacts::Spatial(inner)) = &reply.facts else {
        panic!("Route reply")
    };
    let motion::Fact::RouteCalculation(calculation) = &inner.fact else {
        panic!("calculation")
    };
    calculation
}
#[test]
fn real_corridor_first_route_survives_later_frame_failure_retry_and_retained_replan() {
    let (mut world, mut consumer) = setup();
    consumer.fail_after_route = true;
    for _ in 0..8 {
        let before = world.checkpoint().unwrap();
        let result = world.step(world.state().tick + 1, None, &mut consumer);
        if !consumer.replies.is_empty() {
            assert_eq!(
                result,
                Err(NativeError::AuthorityUnavailable(
                    "authored later-frame failure"
                ))
            );
            assert_eq!(world.checkpoint().unwrap(), before);
            break;
        }
        result.unwrap();
        consumer.acknowledge(&world);
    }
    assert_eq!(consumer.replies.len(), 1);
    let first = calculation(&consumer.replies[0]).clone();
    assert_eq!(first.source, motion::RouteSource::OwnerPathFinder);
    assert_eq!(first.after_cache.revision, 1);
    assert!(first.prior_cache.is_none());
    consumer.fail_after_route = false;
    world
        .step(world.state().tick + 1, None, &mut consumer)
        .unwrap();
    consumer.acknowledge(&world);
    assert_eq!(calculation(&consumer.replies[1]), &first);
    // Move the live player in small valid frame increments; the real source
    // NearPoint selects each goal, and the native owner decides same-polygon reuse.
    for _ in 0..12 {
        let current = world.state().position;
        let destination = benilla_sim::creature_ai::Point::new(
            current.x.as_f32() + 0.5,
            current.y.as_f32(),
            current.z.as_f32(),
        )
        .unwrap();
        let input = OwnedInput {
            authenticated_owner: world.state().player.owner,
            input: Input {
                world: world.state().world,
                tick: world.state().tick + 1,
                player: world.state().player,
                sequence: world.state().next_sequence,
                command: Command::MovementPose {
                    destination,
                    orientation: world.state().orientation,
                    movement_flags: motion::GroundMovementFlags::STOPPED,
                },
            },
        };
        world
            .step(world.state().tick + 1, Some(input), &mut consumer)
            .unwrap();
        consumer.acknowledge(&world);
        if consumer.replies.len() > 2 {
            break;
        }
    }
    assert!(consumer.replies.len() > 2, "no retained replan");
    let retained = calculation(consumer.replies.last().unwrap());
    assert_eq!(retained.prior_cache.as_deref(), Some(&first.after_cache));
    assert_eq!(retained.after_cache.revision, 2);
    assert_eq!(
        retained.after_cache.poly_refs.len(),
        first.after_cache.poly_refs.len()
    );
    assert_eq!(retained.after_cache.binding, first.after_cache.binding);
    let accepted = consumer.replies.last().unwrap();
    for mutation in 0..15 {
        let mut wrong = accepted.clone();
        if mutation == 0 {
            wrong.request.binding.world[0] ^= 1;
        }
        if let Facts::SourceMotion(MotionFacts::Spatial(inner)) = &mut wrong.facts {
            if mutation == 1 {
                if let motion::Query::Route(request) = &mut inner.request {
                    request.binding.query_id += 1;
                }
            }
            if let motion::Fact::RouteCalculation(calculation) = &mut inner.fact {
                match mutation {
                    2 => calculation.source = motion::RouteSource::SyntheticFixture,
                    3 => calculation.after_cache.binding.owner_generation += 1,
                    4 => calculation.after_cache.binding.inventory[0] ^= 1,
                    5 => calculation.after_cache.binding.source_epoch[0] ^= 1,
                    6 => calculation.after_cache.binding.navigation_policy[0] ^= 1,
                    7 => calculation.after_cache.revision += 1,
                    8 => calculation.prior_cache = None,
                    _ => (),
                }
                if let motion::RouteOutcome::Complete(path) = &mut calculation.outcome {
                    match mutation {
                        9 => path.points[1] = path.points[0],
                        10 => path.segments[0].loaded_coverage = Check::Unknown,
                        11 => path.segments[0].line_of_sight = Check::Unknown,
                        12 => path.segments[0].body_sweep = Check::Unknown,
                        13 => path.segments[0].continuous_ground_support = Check::Unknown,
                        14 => path.segments[0].dry_ground = Check::Unknown,
                        _ => (),
                    }
                }
            }
        }
        assert!(
            core::validate_facts(&consumer.session, &wrong.request, &wrong.facts).is_err(),
            "mutation {mutation}"
        );
    }
}

struct TransportConsumer {
    local: core::LocalAuthority,
    other: motion_fixture::SyntheticMotionAuthority,
    replies: Vec<AuthorityReply>,
    fail_after_route: bool,
}
impl Authority for TransportConsumer {
    fn observe_derived(&mut self, _: &AuthorityReply) -> Result<(), NativeError> {
        if self.fail_after_route && !self.replies.is_empty() {
            Err(NativeError::AuthorityUnavailable(
                "authored later-frame failure",
            ))
        } else {
            Ok(())
        }
    }
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if self.fail_after_route && !self.replies.is_empty() {
            return Err(NativeError::AuthorityUnavailable(
                "authored later-frame failure",
            ));
        }
        if matches!(
            request.query,
            Query::PlayerEnvironment(_)
                | Query::SourceMotion(MotionQuery::NearPoint(_) | MotionQuery::Route(_))
        ) {
            let started = Instant::now();
            let reply = self.local.resolve(request).inspect_err(|error| {
                eprintln!(
                    "native geometry transport failure: tick={} query={:?} route_replies={} elapsed={:?} error={error:?}",
                    request.binding.tick,
                    request.query,
                    self.replies.len(),
                    started.elapsed(),
                );
            })?;
            if matches!(request.query, Query::SourceMotion(MotionQuery::Route(_))) {
                self.replies.push(reply.clone());
            }
            Ok(reply)
        } else {
            self.other.resolve(request)
        }
    }
}

#[test]
fn installed_service_mailbox_executes_fresh_dispatch_retry_and_retained_route() {
    let mut geometry = Fixture::new();
    let (mut profile, _, products) = composed_inputs(
        geometry.products(),
        geometry.raw_geometry_revision(),
        &geometry
            .app
            .world()
            .resource::<benilla_assets::MapCatalogRes>()
            .0,
    );
    profile.player.start = benilla_sim::creature_ai::Point::new(5., 11., 0.).unwrap();
    let session = super::super::source_route_same_polygon::tests::source_session(&profile);
    let world_id = session.world;
    super::super::plugin(&mut geometry.app);
    // Loading/binding/installing the non-Send registry precedes NativeWorld creation.
    let (local, mut sink) = super::super::install_with_same_polygon(
        geometry.app.world_mut(),
        session,
        &profile,
        f::player(),
        products,
    )
    .unwrap();
    let worker = std::thread::spawn(move || {
        let mut world =
            NativeWorld::synthetic_component_test(profile, world_id, f::player(), 1234).unwrap();
        let mut boundary = RouteCommitBoundary::capture_initial(&world).unwrap();
        sink.initialize(&boundary).unwrap();
        let mut authority = TransportConsumer {
            local,
            other: Default::default(),
            replies: vec![],
            fail_after_route: true,
        };
        for _ in 0..8 {
            let before = world.checkpoint().unwrap();
            let result = world.step(world.state().tick + 1, None, &mut authority);
            if !authority.replies.is_empty() {
                assert!(result.is_err());
                assert_eq!(world.checkpoint().unwrap(), before);
                break;
            }
            result.unwrap();
            let notice = RouteCommitNotice::capture(&world, &boundary).unwrap();
            sink.deliver(&notice).unwrap();
            boundary = notice.boundary().clone();
        }
        assert_eq!(authority.replies.len(), 1);
        authority.fail_after_route = false;
        world
            .step(world.state().tick + 1, None, &mut authority)
            .unwrap();
        let notice = RouteCommitNotice::capture(&world, &boundary).unwrap();
        sink.deliver(&notice).unwrap();
        boundary = notice.boundary().clone();
        assert_eq!(authority.replies.len(), 2);
        assert_eq!(authority.replies[0], authority.replies[1]);
        let first = authority.replies[0]
            .request
            .require_pathfinder_dispatch(world.profile())
            .unwrap();
        let retry = authority.replies[1]
            .request
            .require_pathfinder_dispatch(world.profile())
            .unwrap();
        assert!(!first.same_live_dispatch(retry));
        for _ in 0..12 {
            let current = world.state().position;
            let destination = benilla_sim::creature_ai::Point::new(
                current.x.as_f32() + 0.5,
                current.y.as_f32(),
                current.z.as_f32(),
            )
            .unwrap();
            let input = OwnedInput {
                authenticated_owner: world.state().player.owner,
                input: Input {
                    world: world_id,
                    tick: world.state().tick + 1,
                    player: world.state().player,
                    sequence: world.state().next_sequence,
                    command: Command::MovementPose {
                        destination,
                        orientation: world.state().orientation,
                        movement_flags: motion::GroundMovementFlags::STOPPED,
                    },
                },
            };
            world
                .step(world.state().tick + 1, Some(input), &mut authority)
                .unwrap();
            let notice = RouteCommitNotice::capture(&world, &boundary).unwrap();
            sink.deliver(&notice).unwrap();
            boundary = notice.boundary().clone();
            if authority.replies.len() > 2 {
                break;
            }
        }
        assert_eq!(authority.replies.len(), 3);
        assert_eq!(
            calculation(&authority.replies[2]).prior_cache.as_deref(),
            Some(&calculation(&authority.replies[0]).after_cache)
        );
        assert_eq!(calculation(&authority.replies[2]).after_cache.revision, 2);
        (world, authority.replies, authority.local)
    });
    let until = Instant::now() + Duration::from_secs(45);
    while !worker.is_finished() && Instant::now() < until {
        geometry.advance_physics();
        std::thread::sleep(Duration::from_millis(1));
    }
    if !worker.is_finished() {
        super::super::uninstall(geometry.app.world_mut());
    }
    let (world, replies, mut retired_authority) = worker.join().unwrap();
    assert_eq!(replies.len(), 3);
    // Serialization is distinct from native rehydration. This is an in-memory
    // runtime loop; it does not demonstrate controller/store durability or reopen.
    super::super::uninstall(geometry.app.world_mut());
    let old_request = replies.last().unwrap().request.clone();
    assert!(
        std::thread::spawn(move || retired_authority.resolve(&old_request))
            .join()
            .unwrap()
            .is_err()
    );
    let checkpoint = world.checkpoint().unwrap();
    assert!(checkpoint.encode().is_ok());
    assert!(
        NativeWorld::restore_synthetic_component_test(world.profile().clone(), checkpoint).is_err()
    );
}

#[test]
fn committed_replans_sustain_more_than_four_native_candidates_without_cold_restart() {
    let (mut world, mut consumer) = setup();
    let mut last_route = None;
    let mut notices = vec![];
    let mut retried_later_frame = false;
    let mut waypoints: Vec<benilla_sim::creature_ai::Point> = vec![];
    let tick_ms = world.profile().tick_ms;
    // At most two rectangle-axis traversals, one 250ms source recheck, and a
    // full polygon-diagonal spline per accepted calculation. The initial target
    // starts at y=11, so include its extra 1.75 units above the rectangle.
    let travel_ticks = ((17.0_f32 + 1.75) / 0.5).ceil() as u32;
    let spline_ticks = ((200.0_f32.sqrt() / world.profile().npcs[0].ai.run_speed.as_f32()) * 1000.0
        / tick_ms as f32)
        .ceil() as u32;
    let recheck_ticks = 250_u32.div_ceil(tick_ms);
    let initial_ticks = world
        .profile()
        .source_motion
        .as_ref()
        .unwrap()
        .creature_initial_aggro_delay_ms
        .div_ceil(tick_ms)
        + 2;
    let frame_bound = initial_ticks + 8 * (travel_ticks + spline_ticks + recheck_ticks + 2);
    let reach = benilla_sim::creature_ai::combat_reach(
        world.profile().npcs[0].ai.combat_reach,
        world.profile().player.combat_reach,
    )
    .unwrap()
    .as_f32();
    assert_eq!(reach, 5.0);
    // Keep the genuine StaticFlags4=0 combat host, including backpedal=true.
    assert!(
        world
            .profile()
            .source_motion
            .as_ref()
            .unwrap()
            .npc(2)
            .unwrap()
            .npc_policy
            .settings
            .backpedal_enabled
    );
    let backpedal_distance = (world.profile().npcs[0].ai.combat_reach.as_f32()
        + world.profile().player.combat_reach.as_f32())
    .min(3.75);
    let backpedal_threshold_squared = backpedal_distance * backpedal_distance * 0.33;
    // Fanning stays enabled too. Its non-Route Collider(None) callback remains
    // the explicitly authored SyntheticMotionAuthority component fixture.
    for _ in 0..frame_bound {
        let source = &world.state().npcs[&2]
            .source_motion
            .as_ref()
            .unwrap()
            .motion;
        let snapshot = source.snapshot();
        // Freeze the target through the real spline's arrival. At the next
        // movement leg the owner is exactly the old destination, not a guessed
        // future pose. Initial target (5,11) is also held through first arrival.
        let arrived = snapshot.spline.as_ref().is_some_and(|s| s.spline.finalized);
        let position = world.state().position;
        let input = if last_route.is_some() && arrived {
            let owner = source.pose().position;
            if waypoints.is_empty() {
                let dx = position.x.as_f32() - owner.x.as_f32();
                let dy = position.y.as_f32() - owner.y.as_f32();
                // Source NearPoint selected a half-reach endpoint. Preserve the
                // larger component while traversing the perpendicular axis;
                // this keeps distance >= 2.5/sqrt(2), above backpedal's threshold.
                assert!((dx * dx + dy * dy - 6.25).abs() < 0.001);
                let far = |coordinate: f32| if coordinate < 5.0 { 9.25 } else { 0.75 };
                let x = far(owner.x.as_f32());
                let y = far(owner.y.as_f32());
                let (first_x, first_y, fixed_squared) = if dx.abs() >= dy.abs() {
                    (position.x.as_f32(), y, dx * dx)
                } else {
                    (x, position.y.as_f32(), dy * dy)
                };
                assert!(fixed_squared > backpedal_threshold_squared);
                let far_squared = (x - owner.x.as_f32()).powi(2) + (y - owner.y.as_f32()).powi(2);
                assert!(far_squared > reach * reach);
                // Once the first axis reaches its far edge, its separation is
                // >=4.25 throughout the other axis. All later target positions
                // are inside the first authored convex polygon. On the initial
                // outside-y leg, distance <=sqrt(4.25^2+2.5^2)<5 prevents Route.
                waypoints = vec![
                    benilla_sim::creature_ai::Point::new(first_x, first_y, 0.0).unwrap(),
                    benilla_sim::creature_ai::Point::new(x, y, 0.0).unwrap(),
                ];
            }
            while waypoints.len() > 1 && waypoints[0] == position {
                waypoints.remove(0);
            }
            let goal = waypoints[0];
            let destination = benilla_sim::creature_ai::Point::new(
                position.x.as_f32() + (goal.x.as_f32() - position.x.as_f32()).clamp(-0.5, 0.5),
                position.y.as_f32() + (goal.y.as_f32() - position.y.as_f32()).clamp(-0.5, 0.5),
                0.0,
            )
            .unwrap();
            let separation = (destination.x.as_f32() - owner.x.as_f32()).powi(2)
                + (destination.y.as_f32() - owner.y.as_f32()).powi(2);
            assert!(separation > backpedal_threshold_squared);
            Some(OwnedInput {
                authenticated_owner: world.state().player.owner,
                input: Input {
                    world: world.state().world,
                    tick: world.state().tick + 1,
                    player: world.state().player,
                    sequence: world.state().next_sequence,
                    command: Command::MovementPose {
                        destination,
                        orientation: world.state().orientation,
                        // Authored stopped-target pose samples, not a claim of
                        // running-target or arbitrary physics admission.
                        movement_flags: motion::GroundMovementFlags::STOPPED,
                    },
                },
            })
        } else {
            None
        };
        consumer.fail_after_route = !retried_later_frame
            && last_route
                .as_ref()
                .is_some_and(|route: &motion::RouteCalculation| route.after_cache.revision == 2);
        let checkpoint = world.checkpoint().unwrap();
        let committed = consumer
            .providers
            .route
            .committed_candidate()
            .map(|(pointer, cache)| (pointer, cache.clone()));
        let replies_before = consumer.replies.len();
        let result = world.step(world.state().tick + 1, input.clone(), &mut consumer);
        if result.is_err() && consumer.fail_after_route && consumer.replies.len() > replies_before {
            assert_eq!(world.checkpoint().unwrap(), checkpoint);
            assert_eq!(
                consumer
                    .providers
                    .route
                    .committed_candidate()
                    .map(|(pointer, cache)| (pointer, cache.clone())),
                committed
            );
            assert_eq!(consumer.providers.route.retained_candidate_count(), 2);
            let failed = consumer.replies.last().unwrap().clone();
            consumer.fail_after_route = false;
            world
                .step(world.state().tick + 1, input, &mut consumer)
                .unwrap();
            assert_eq!(consumer.replies.last(), Some(&failed));
            assert!(!failed
                .request
                .require_pathfinder_dispatch(world.profile())
                .unwrap()
                .same_live_dispatch(
                    consumer
                        .replies
                        .last()
                        .unwrap()
                        .request
                        .require_pathfinder_dispatch(world.profile())
                        .unwrap()
                ));
            retried_later_frame = true;
        } else {
            result.unwrap();
        }
        // No callback, reply delivery, or returned success may promote itself.
        let before_ack = consumer
            .providers
            .route
            .committed_candidate()
            .map(|(_, cache)| cache.clone());
        assert_eq!(
            before_ack,
            last_route
                .as_ref()
                .map(|route: &motion::RouteCalculation| route.after_cache.clone())
        );
        let notice = consumer.acknowledge(&world);
        assert!(
            notice.routes().len() <= 1,
            "bounded source frame has one dispatch"
        );
        for reply in notice.routes() {
            waypoints.clear();
            let Query::SourceMotion(MotionQuery::Route(route)) = &reply.request.query else {
                panic!("captured Route");
            };
            assert!(matches!(
                route.request.kind,
                motion::RouteKind::ChaseInitial | motion::RouteKind::ChaseReplan
            ));
            for point in [route.request.origin, route.request.goal] {
                assert!((0.0..=10.0).contains(&point.x.as_f32()));
                assert!((0.0..=10.0).contains(&point.y.as_f32()));
                assert_eq!(point.z.as_f32(), 0.0);
            }
            let current = calculation(reply);
            if let Some(previous) = &last_route {
                assert_eq!(current.prior_cache.as_deref(), Some(&previous.after_cache));
                assert_eq!(
                    current.after_cache.revision,
                    previous.after_cache.revision + 1
                );
                assert_eq!(
                    current.after_cache.poly_refs.len(),
                    previous.after_cache.poly_refs.len()
                );
                assert_eq!(current.after_cache.binding, previous.after_cache.binding);
            } else {
                assert!(current.prior_cache.is_none());
                assert_eq!(current.after_cache.revision, 1);
                assert_eq!(current.after_cache.poly_refs.len(), 75);
            }
            last_route = Some(current.clone());
        }
        notices.push(notice);
        if last_route
            .as_ref()
            .is_some_and(|route| route.after_cache.revision >= 8)
        {
            break;
        }
    }
    assert!(
        retried_later_frame,
        "later committed ancestry rollback/retry"
    );
    let last = last_route.expect("real first route");
    assert!(
        last.after_cache.revision >= 8,
        "sustain eight actual calculations, including seven retained replans"
    );
    assert_eq!(consumer.providers.route.retained_candidate_count(), 1);
    let head = consumer
        .providers
        .route
        .committed_candidate()
        .map(|(pointer, cache)| (pointer, cache.clone()));
    // Earlier committed notices cannot demote the head, even though their source facts were valid.
    assert!(consumer
        .providers
        .route
        .acknowledge(&consumer.session, &notices[0])
        .is_err());
    assert_eq!(
        consumer
            .providers
            .route
            .committed_candidate()
            .map(|(pointer, cache)| (pointer, cache.clone())),
        head
    );
    // A delayed callback cannot recreate pruned speculation after its tick committed.
    let delayed = consumer.replies[0].request.clone();
    assert!(consumer
        .providers
        .route
        .calculate(
            &consumer.session,
            &delayed,
            1,
            Instant::now() + Duration::from_secs(10)
        )
        .is_err());
    assert_eq!(consumer.providers.route.retained_candidate_count(), 1);
}

#[test]
fn uncommitted_candidate_cap_refuses_without_eviction_then_exact_retry_commits() {
    let (mut world, mut consumer) = setup();
    consumer.fail_after_route = true;
    for _ in 0..8 {
        let result = world.step(world.state().tick + 1, None, &mut consumer);
        if !consumer.replies.is_empty() {
            assert!(result.is_err());
            break;
        }
        result.unwrap();
        consumer.acknowledge(&world);
    }
    assert_eq!(consumer.replies.len(), 1);
    let first = consumer.replies[0].clone();
    let first_identity = consumer.providers.route.candidate_identity(&first).unwrap();
    let checkpoint = world.checkpoint().unwrap();
    for attempt in 1..=4 {
        consumer.failure_reply_floor = consumer.replies.len();
        let position = world.state().position;
        let input = OwnedInput {
            authenticated_owner: world.state().player.owner,
            input: Input {
                world: world.state().world,
                tick: world.state().tick + 1,
                player: world.state().player,
                sequence: world.state().next_sequence,
                command: Command::MovementPose {
                    destination: benilla_sim::creature_ai::Point::new(
                        position.x.as_f32() + attempt as f32 * 0.125,
                        position.y.as_f32(),
                        0.0,
                    )
                    .unwrap(),
                    orientation: world.state().orientation,
                    movement_flags: motion::GroundMovementFlags::STOPPED,
                },
            },
        };
        assert!(world
            .step_batch(world.state().tick + 1, vec![input], &mut consumer)
            .is_err());
        assert_eq!(world.checkpoint().unwrap(), checkpoint);
        assert_eq!(
            consumer.providers.route.retained_candidate_count(),
            (attempt + 1).min(4)
        );
        assert_eq!(
            consumer.providers.route.candidate_identity(&first),
            Some(first_identity)
        );
        assert!(consumer.providers.route.committed_candidate().is_none());
    }
    assert_eq!(
        consumer.route_errors,
        vec![SamePolygonError::CandidateBudget]
    );
    consumer.fail_after_route = false;
    world
        .step(world.state().tick + 1, None, &mut consumer)
        .unwrap();
    assert_eq!(consumer.replies.last(), Some(&first));
    consumer.acknowledge(&world);
    assert_eq!(consumer.providers.route.retained_candidate_count(), 1);
    assert_eq!(
        consumer.providers.route.committed_candidate().unwrap().0,
        first_identity
    );
}

#[test]
fn actual_corridor_failure_stages_but_never_certifies_or_promotes_the_candidate() {
    let (mut world, mut consumer) = setup();
    let root = consumer.geometry.roots[&(31, 31)];
    let collider = consumer
        .geometry
        .app
        .world_mut()
        .entity_mut(root)
        .take::<avian3d::prelude::Collider>()
        .unwrap();
    for _ in 0..8 {
        let before = world.checkpoint().unwrap();
        let result = world.step(world.state().tick + 1, None, &mut consumer);
        if !consumer.route_errors.is_empty() {
            assert!(result.is_err());
            assert_eq!(world.checkpoint().unwrap(), before);
            break;
        }
        result.unwrap();
        consumer.acknowledge(&world);
    }
    assert_eq!(consumer.providers.route.retained_candidate_count(), 1);
    assert_eq!(consumer.providers.route.certified_candidate_count(), 0);
    assert!(consumer.providers.route.committed_candidate().is_none());
    assert!(consumer.replies.is_empty());
    consumer
        .geometry
        .app
        .world_mut()
        .entity_mut(root)
        .insert(collider);
    world
        .step(world.state().tick + 1, None, &mut consumer)
        .unwrap();
    assert_eq!(consumer.providers.route.certified_candidate_count(), 1);
    assert!(consumer.providers.route.committed_candidate().is_none());
    consumer.acknowledge(&world);
    assert_eq!(consumer.providers.route.retained_candidate_count(), 1);
    assert_eq!(
        consumer
            .providers
            .route
            .committed_candidate()
            .unwrap()
            .1
            .revision,
        1
    );
}

#[test]
fn committed_notice_requires_whole_certified_chain_current_cache_and_exact_session_order() {
    use super::super::route_commit::authored_conflicts;
    let (mut world, mut consumer) = setup();
    let (notice, foreign) = loop {
        assert!(world.state().tick < 8);
        let foreign_boundary = RouteCommitBoundary::capture_initial(&world).unwrap();
        world
            .step(world.state().tick + 1, None, &mut consumer)
            .unwrap();
        if !consumer.replies.is_empty() {
            break (
                RouteCommitNotice::capture(&world, &consumer.boundary).unwrap(),
                RouteCommitNotice::capture(&world, &foreign_boundary).unwrap(),
            );
        }
        consumer.acknowledge(&world);
    };
    let reply = notice.routes().last().unwrap();
    let candidate = consumer.providers.route.candidate_identity(reply).unwrap();
    assert!(consumer.providers.route.committed_candidate().is_none());
    for invalid in [
        foreign,
        authored_conflicts::wrong_current_cache(&notice),
        authored_conflicts::without_routes(&notice),
        authored_conflicts::duplicate_route(&notice),
    ] {
        assert!(consumer
            .providers
            .route
            .acknowledge(&consumer.session, &invalid)
            .is_err());
        assert_eq!(
            consumer.providers.route.candidate_identity(reply),
            Some(candidate)
        );
        assert_eq!(consumer.providers.route.retained_candidate_count(), 1);
        assert_eq!(consumer.providers.route.certified_candidate_count(), 1);
        assert!(consumer.providers.route.committed_candidate().is_none());
    }
    // Deliberately bypass the controller barrier for this negative provider test.
    // The next actual tick is route-free; its notice still cannot skip the first.
    world
        .step(world.state().tick + 1, None, &mut consumer)
        .unwrap();
    let second = RouteCommitNotice::capture(&world, notice.boundary()).unwrap();
    assert!(second.routes().is_empty());
    assert!(consumer
        .providers
        .route
        .acknowledge(&consumer.session, &second)
        .is_err());
    consumer
        .providers
        .route
        .acknowledge(&consumer.session, &notice)
        .unwrap();
    assert_eq!(
        consumer.providers.route.committed_candidate().unwrap().0,
        candidate
    );
    assert!(consumer
        .providers
        .route
        .acknowledge(
            &consumer.session,
            &authored_conflicts::wrong_record(&notice)
        )
        .is_err());
    consumer
        .providers
        .route
        .acknowledge(&consumer.session, &notice)
        .unwrap();
    consumer
        .providers
        .route
        .acknowledge(&consumer.session, &second)
        .unwrap();
    assert!(consumer
        .providers
        .route
        .acknowledge(&consumer.session, &notice)
        .is_err());
    assert_eq!(
        consumer.providers.route.committed_candidate().unwrap().0,
        candidate
    );
}

#[test]
fn actual_death_and_cache_free_respawn_prune_the_committed_native_owner() {
    let (mut world, mut consumer) = setup_with(|profile| {
        // Authored health only: keep source combat, motion and raw StaticFlags4
        // policies intact. A real scheduled spell hit performs the death.
        profile.npcs[0].max_health = 1;
    });
    for _ in 0..8 {
        world
            .step(world.state().tick + 1, None, &mut consumer)
            .unwrap();
        consumer.acknowledge(&world);
        if !consumer.replies.is_empty() {
            break;
        }
    }
    assert_eq!(consumer.replies.len(), 1);
    let committed = consumer
        .providers
        .route
        .committed_candidate()
        .map(|(pointer, cache)| (pointer, cache.clone()))
        .unwrap();
    let old_npc = world.state().npcs[&2].id;
    let old_boundary = consumer.boundary.clone();
    let cast = f::cast(&world, 60001);
    world
        .step(world.state().tick + 1, Some(cast), &mut consumer)
        .unwrap();
    let before_death = consumer.acknowledge(&world);
    let benilla_world_runtime::native::AbilityAdmission::Available(spec) =
        &world.profile().abilities[0].admission
    else {
        panic!("fixture spell");
    };
    let owner = world.state().npcs[&2]
        .source_motion
        .as_ref()
        .unwrap()
        .motion
        .pose()
        .position;
    let player = world.state().position;
    let distance = ((player.x.as_f32() - owner.x.as_f32()).powi(2)
        + (player.y.as_f32() - owner.y.as_f32()).powi(2))
    .sqrt();
    let flight_ms =
        (distance / f32::from_bits(spec.projectile_speed.bits()) * 1000.0).ceil() as u32;
    let death_bound = (spec.cast_ms + flight_ms).div_ceil(world.profile().tick_ms) + 4;
    let mut death_notice = None;
    for _ in 0..death_bound {
        world
            .step(world.state().tick + 1, None, &mut consumer)
            .unwrap();
        assert_eq!(
            consumer
                .providers
                .route
                .committed_candidate()
                .map(|(pointer, cache)| (pointer, cache.clone())),
            Some(committed.clone())
        );
        let died = world.state().npcs[&2].health == 0;
        if died {
            assert!(world
                .last_tick()
                .unwrap()
                .events
                .iter()
                .any(|event| matches!(
                    event,
                    benilla_world_runtime::native::Event::Npc {
                        effect: benilla_sim::creature_ai::Effect::Died,
                        ..
                    }
                )));
            assert!(matches!(
                world.state().npcs[&2]
                    .source_motion
                    .as_ref()
                    .unwrap()
                    .motion
                    .snapshot()
                    .generator,
                motion::Generator::Idle
            ));
        }
        let notice = consumer.acknowledge(&world);
        if died {
            assert!(notice.routes().is_empty());
            assert!(notice
                .boundary()
                .heads()
                .iter()
                .all(|head| head.cache().is_none() && head.construction().is_none()));
            assert!(consumer.providers.route.committed_candidate().is_none());
            assert_eq!(consumer.providers.route.retained_candidate_count(), 0);
            assert_eq!(consumer.providers.route.certified_candidate_count(), 0);
            death_notice = Some(notice);
            break;
        }
    }
    let death_notice = death_notice.expect("scheduled lethal hit cleared the actual Chase");
    assert!(consumer
        .providers
        .route
        .acknowledge(&consumer.session, &before_death)
        .is_err());
    assert!(consumer
        .providers
        .route
        .initialize_commit(&consumer.session, &old_boundary)
        .is_err());
    assert_eq!(consumer.providers.route.retained_candidate_count(), 0);
    let npc_profile = &world.profile().npcs[0].ai;
    let respawn_bound =
        (npc_profile.corpse_decay_ms + npc_profile.respawn_delay_seconds * 1000 + 1000)
            .div_ceil(world.profile().tick_ms)
            + 2;
    for _ in 0..respawn_bound {
        world
            .step(world.state().tick + 1, None, &mut consumer)
            .unwrap();
        let notice = consumer.acknowledge(&world);
        assert!(notice.routes().is_empty());
        assert_eq!(consumer.providers.route.retained_candidate_count(), 0);
        if world.state().npcs[&2].id != old_npc {
            let head = notice
                .boundary()
                .heads()
                .iter()
                .find(|head| head.npc().spawn == 2)
                .unwrap();
            assert_eq!(head.npc().generation, old_npc.generation + 1);
            assert!(head.cache().is_none() && head.construction().is_none());
            assert!(consumer
                .providers
                .route
                .acknowledge(&consumer.session, &death_notice)
                .is_err());
            // The cache-free replacement boundary is accepted, but this test
            // stops before a new constructor. Registry churn remains unsupported.
            return;
        }
    }
    panic!("real corpse/respawn timers did not replace the NPC generation");
}

#[test]
fn direct_route_resolver_still_rejects_replaced_session_and_decoded_request() {
    let (mut world, mut consumer) = setup();
    consumer.fail_after_route = true;
    for _ in 0..8 {
        let result = world.step(world.state().tick + 1, None, &mut consumer);
        if !consumer.replies.is_empty() {
            assert!(result.is_err());
            break;
        }
        result.unwrap();
        consumer.acknowledge(&world);
    }
    assert_eq!(consumer.replies.len(), 1);
    let context = consumer.replies[0].request.clone();
    let before = world.checkpoint().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    for mutation in 0..4 {
        let mut changed = consumer.session.clone();
        match mutation {
            0 => changed.world[0] ^= 1,
            1 => changed.profile[0] ^= 1,
            2 => changed.geometry[0] ^= 1,
            3 => changed.bodies.clear(),
            _ => unreachable!(),
        }
        let result = consumer.geometry.with_local_corridor(|corridor| {
            super::resolve(
                &mut consumer.providers.route,
                &changed,
                &context,
                corridor,
                1,
                deadline,
            )
        });
        assert!(result.is_err(), "session mutation {mutation}");
    }
    let decoded = serde_json::from_value(serde_json::to_value(&context).unwrap()).unwrap();
    let result = consumer.geometry.with_local_corridor(|corridor| {
        super::resolve(
            &mut consumer.providers.route,
            &consumer.session,
            &decoded,
            corridor,
            1,
            deadline,
        )
    });
    assert!(result.is_err());
    assert_eq!(world.checkpoint().unwrap(), before);
}
