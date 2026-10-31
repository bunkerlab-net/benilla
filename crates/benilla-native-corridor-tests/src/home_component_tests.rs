// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored loaded-terrain combat/Home/default-Wander composition. The real
//! runtime selects combat and evade; no Route, Home or Reset facts are supplied.
//! This does not import entry 6's neutral AI, Northshire or production admission.
use super::wander_component_tests::{
    assert_dry_owner, assert_replay_through, calculation, launched, reject_late_and_retry,
    setup_with_profile, snapshot, step, wander, Consumer,
};
use benilla_sim::{creature_ai as ai, creature_ai::movement::Check, creature_motion as motion};
use benilla_world_runtime::native::{synthetic_fixture as f, *};

const HOME_FAILURE: &str = "authored failure after concrete Home calculation";

struct RejectHome<'a> {
    consumer: &'a mut Consumer,
    rejected: Option<AuthorityReply>,
}
impl Authority for RejectHome<'_> {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        let reply = self.consumer.resolve(request)?;
        if matches!(&request.query, Query::SourceMotion(MotionQuery::Route(route))
            if route.request.kind == motion::RouteKind::HomeForceDestination)
        {
            self.rejected = Some(reply);
            return Err(NativeError::AuthorityUnavailable(HOME_FAILURE));
        }
        Ok(reply)
    }
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        self.consumer.observe_derived(reply)
    }
}

fn route(reply: &AuthorityReply) -> &motion::RouteRequest {
    let Query::SourceMotion(MotionQuery::Route(route)) = &reply.request.query else {
        panic!("recorded route request")
    };
    &route.request
}

#[test]
fn loaded_combat_home_returns_to_combat_origin_then_resets_and_resumes_wander() {
    // Explicit short pursuit policy makes CombatManager's initially expired
    // check enter evade after the genuine first Chase update. The 800 ms sight
    // delay lets Wander publish a non-respawn combat origin first. All route
    // endpoints remain on the authored dry first polygon, with full body proof.
    let (mut world, mut consumer) = setup_with_profile(Some(0), Some(800), |profile| {
        profile.player.start = ai::Point::new(12., 12., 0.).unwrap();
        profile.npcs[0].ai.pursuit_ms = 1;
        profile.npcs[0].ai.leash_radius = f::real(0.01);
    });
    let genesis = world.checkpoint().unwrap();
    let npc = world.state().npcs[&2].id;
    let initialized = world.state().npcs[&2]
        .initialized_creature()
        .unwrap()
        .clone();
    let spawn = world.profile().npcs[0].ai.spawn;
    assert_eq!(initialized.unit_bytes_2, 0x1001);
    assert!(launched(&step(&mut world, &mut consumer)));
    while world.state().tick < 7 {
        step(&mut world, &mut consumer);
    }
    let before_combat = snapshot(&world);
    let retained = wander(&before_combat).clone();
    let prior_cache = retained.pathfinder_cache.clone().unwrap();
    assert_ne!(before_combat.map_pose.position, spawn);
    let moving = world.checkpoint().unwrap();
    consumer.allow_chase = true;
    consumer.other.visibility = Some(Check::Passed);

    // Reject only after the actual native Home terminal and LocalCorridor
    // certificates exist. Neither the staged Chase nor transient Home becomes
    // a committed head; retry must consume the identical live dispatch.
    let committed = consumer
        .providers
        .route
        .committed_candidate()
        .map(|(binding, cache)| (binding, cache.clone()));
    let records = world.records().to_vec();
    let queue = world.state().npcs[&2].events.queue_snapshot();
    let rng = world.state().rng_counter;
    let tick = world.state().tick + 1;
    let rejected = {
        let mut authority = RejectHome {
            consumer: &mut consumer,
            rejected: None,
        };
        assert_eq!(
            world.step(tick, None, &mut authority),
            Err(NativeError::AuthorityUnavailable(HOME_FAILURE))
        );
        authority
            .rejected
            .expect("real Home callback reached before rejection")
    };
    assert_eq!(world.checkpoint().unwrap(), moving);
    assert_eq!(world.records(), records);
    assert_eq!(world.state().npcs[&2].events.queue_snapshot(), queue);
    assert_eq!(world.state().rng_counter, rng);
    assert_eq!(
        consumer
            .providers
            .route
            .committed_candidate()
            .map(|(binding, cache)| (binding, cache.clone())),
        committed
    );
    let entering = step(&mut world, &mut consumer);
    assert!(entering.replies.contains(&rejected));
    assert_eq!(world.state().npcs[&2].ai.phase(), ai::Phase::ReturningHome);
    assert!(entering.events.iter().any(|event| matches!(event,
        Event::Npc { id, effect: ai::Effect::CombatStarted { .. } } if *id == npc)));
    assert!(entering.events.iter().any(|event| matches!(event,
        Event::Npc { id, effect: ai::Effect::EvadeStarted(ai::EvadeReason::PursuitLeash) }
            if *id == npc)));
    let routes: Vec<_> = entering
        .replies
        .iter()
        .filter(|reply| {
            matches!(
                reply.request.query,
                Query::SourceMotion(MotionQuery::Route(_))
            )
        })
        .collect();
    assert_eq!(
        routes
            .iter()
            .map(|reply| route(reply).kind)
            .collect::<Vec<_>>(),
        [
            motion::RouteKind::ChaseInitial,
            motion::RouteKind::HomeForceDestination
        ]
    );
    let chase = route(routes[0]);
    let home = route(routes[1]);
    let home_result = calculation(routes[1]);
    let current = snapshot(&world);
    let source = world.state().npcs[&2].source_motion.as_ref().unwrap();
    let combat_start = source.combat_start.unwrap();
    assert_eq!(combat_start, before_combat.map_pose);
    assert_eq!(home.goal, combat_start.position);
    assert_ne!(
        home.goal, spawn,
        "Home uses captured combat origin, not Wander center"
    );
    assert_eq!(
        source.motion.pose().position,
        world.state().npcs[&2].ai.position()
    );
    assert_eq!(current.suspended_wander.as_ref(), Some(&retained));
    assert!(
        matches!(current.generator, motion::Generator::Home { destination, .. }
        if destination == combat_start)
    );
    // Mutate(Home) interrupts the retained default Wander once more, forcing
    // source UpdateSplinePosition before Home computes its map-origin route.
    assert_eq!(home.origin, home.launch_origin);
    assert_ne!(
        home.origin, chase.origin,
        "the first Chase spline actually advanced"
    );
    assert!(entering.events.iter().any(|event| matches!(event,
        Event::NpcMotion { event: motion::Event::SplineInterrupted { pose, .. }, .. }
            if pose.position == home.origin)));
    assert_eq!(
        home.pathfinder_dispatch.unwrap().constructor_kind(),
        motion::PathfinderConstructorKind::HomeLocal
    );
    assert_eq!(
        home.pathfinder_dispatch.unwrap().construction_binding(),
        home.binding
    );
    assert!(home.prior_cache.is_none());
    assert_ne!(home.pathfinder_dispatch, chase.pathfinder_dispatch);
    assert_eq!(home_result.source, motion::RouteSource::OwnerPathFinder);
    assert_eq!(home_result.after_cache.include_flags, 13);
    assert_eq!(home_result.after_cache.revision, 1);
    let motion::RouteOutcome::Complete(path) = &home_result.outcome else {
        panic!("loaded same-polygon Home NORMAL path")
    };
    assert_eq!(path.points, [home.origin, combat_start.position]);
    assert_eq!(path.segments.len(), 1);
    assert_eq!(
        current.spline.as_ref().unwrap().local_home_cache.as_deref(),
        Some(&home_result.after_cache)
    );
    assert!(consumer
        .boundary
        .heads()
        .iter()
        .all(|head| head.cache().is_none() && head.construction().is_none()));
    assert!(
        consumer.providers.route.committed_candidate().is_none(),
        "Home local cache cannot become retained authority"
    );
    // Clear(false) retires Chase, but keeps the actual default RANDOM owner
    // under Home. The old zero-owner assertion incorrectly treated Home's
    // stack-local PathFinder as the only source of constructor ownership.
    let head = &consumer.boundary.heads()[0];
    let suspended = head
        .suspended_wander()
        .expect("Home retains default Wander");
    assert_eq!(
        suspended.construction(),
        Some(retained.pathfinder_construction)
    );
    assert_eq!(suspended.cache(), Some(prior_cache.as_ref()));
    assert_eq!(
        consumer
            .providers
            .route
            .committed_candidate_for_constructor(npc, retained.pathfinder_construction)
            .map(|(identity, cache)| (identity, cache.clone())),
        committed,
    );
    assert_eq!(consumer.providers.route.registered_generator_count(), 1);
    assert_eq!(consumer.providers.route.registered_constructor_count(), 1);
    assert_dry_owner(&world);
    let returning = world.checkpoint().unwrap();
    let route_floor = consumer.route_replies.len();
    consumer.other.visibility = Some(Check::Failed);

    let duration = *current
        .spline
        .as_ref()
        .unwrap()
        .spline
        .timestamps_ms
        .last()
        .unwrap();
    let mut arrived = false;
    for _ in 0..duration.div_ceil(world.profile().tick_ms) + 2 {
        let record = reject_late_and_retry(&mut world, &mut consumer);
        assert_dry_owner(&world);
        assert_eq!(world.state().npcs[&2].id, npc);
        assert_eq!(
            world.state().npcs[&2].initialized_creature(),
            Some(&initialized)
        );
        assert_eq!(
            world.state().npcs[&2].historical_initialized_creature(npc),
            Some(&initialized)
        );
        if record.events.iter().any(|event| {
            matches!(
                event,
                Event::NpcMotion {
                    event: motion::Event::HomeReached { .. },
                    ..
                }
            )
        }) {
            assert!(
                !launched(&record),
                "Reset initializes but does not pick a location"
            );
            assert!(!record.draws.iter().any(|draw| matches!(
                draw.purpose,
                DrawPurpose::WanderAngle { .. }
                    | DrawPurpose::WanderRadius { .. }
                    | DrawPurpose::WanderMoveCount { .. }
                    | DrawPurpose::WanderPause { .. }
            )));
            arrived = true;
            break;
        }
    }
    assert!(
        arrived,
        "source Home arrived within the recorded spline duration"
    );
    assert_eq!(consumer.route_replies.len(), route_floor);
    assert_eq!(world.state().npcs[&2].ai.phase(), ai::Phase::Idle);
    assert!(world.state().npcs[&2]
        .source_motion
        .as_ref()
        .unwrap()
        .combat_start
        .is_none());
    let reset = snapshot(&world);
    let reset_wander = wander(&reset);
    assert_eq!(reset.map_pose.position, combat_start.position);
    assert_eq!(reset_wander.center, spawn);
    assert_eq!(reset_wander.next_move_count, retained.next_move_count);
    assert_eq!(reset_wander.next_move_timer_ms, 0);
    assert!(reset_wander.pathfinder_cache.is_none());
    assert_ne!(
        reset_wander.pathfinder_construction,
        retained.pathfinder_construction
    );
    assert!(reset.suspended_wander.is_none());
    assert!(reset.spline.is_none());
    // Sheath-only addon reload is idempotent and keeps the generation/model owner.
    assert_eq!(
        world.state().npcs[&2].initialized_creature(),
        Some(&initialized)
    );
    let reset_checkpoint = world.checkpoint().unwrap();

    let resumed = reject_late_and_retry(&mut world, &mut consumer);
    assert!(
        launched(&resumed),
        "first Update after Reset launches a new RANDOM leg"
    );
    let resumed_route = consumer.routes.last().unwrap();
    let resumed_request = route(resumed_route);
    let resumed_result = calculation(resumed_route);
    assert_eq!(resumed_request.origin, combat_start.position);
    assert_eq!(resumed_request.wander.as_deref().unwrap().center, spawn);
    assert_eq!(consumer.providers.route.registered_generator_count(), 1);
    assert_eq!(consumer.providers.route.registered_constructor_count(), 1);
    assert!(resumed_result.prior_cache.is_none());
    assert_eq!(resumed_result.after_cache.revision, 1);
    assert_eq!(resumed_result.after_cache.include_flags, 13);
    assert_ne!(
        resumed_request
            .pathfinder_dispatch
            .unwrap()
            .construction_binding(),
        retained.pathfinder_construction
    );
    assert_ne!(
        resumed_request
            .pathfinder_dispatch
            .unwrap()
            .construction_binding(),
        home.pathfinder_dispatch.unwrap().construction_binding()
    );
    assert_eq!(
        prior_cache.binding.owner_generation,
        resumed_result.after_cache.binding.owner_generation
    );
    assert_eq!(world.state().npcs[&2].id, npc);
    assert!(
        consumer.initializations.is_empty(),
        "Home does not respawn/reselect the model"
    );
    assert_dry_owner(&world);
    assert_replay_through(&world, &genesis, &moving);
    assert_replay_through(&world, &genesis, &returning);
    // Reset is genuinely cache-free and directly restorable; the Home checkpoint
    // above and the new loaded Wander owner below remain non-rehydratable.
    let restored = NativeWorld::restore_synthetic_component_test(
        world.profile().clone(),
        reset_checkpoint.clone(),
    )
    .unwrap();
    assert_eq!(restored.checkpoint().unwrap(), reset_checkpoint);
    assert_replay_through(&world, &genesis, &world.checkpoint().unwrap());
}

#[test]
fn idle_home_certifies_both_source_origin_and_corrected_launch_segment() {
    use avian3d::prelude::{Collider, CollisionLayers, RigidBody};
    use bevy::prelude::{Entity, Transform};

    struct Probe<'a> {
        consumer: &'a mut Consumer,
        block_launch_only: bool,
        home: Option<motion::RouteRequest>,
        wall: Option<Entity>,
    }
    impl Authority for Probe<'_> {
        fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            if let Query::SourceMotion(MotionQuery::Route(route)) = &request.query {
                if route.request.kind == motion::RouteKind::HomeForceDestination {
                    let q = &route.request;
                    self.home = Some(q.clone());
                    assert_ne!(q.origin, q.launch_origin);
                    assert_eq!(q.origin.x, q.launch_origin.x);
                    assert_eq!(q.origin.z, q.launch_origin.z);
                    assert!(q.launch_origin.y.as_f32() > q.origin.y.as_f32());
                    assert!(q.goal.y.as_f32() < q.origin.y.as_f32());
                    let xyz = |p: ai::Point| [p.x.as_f32(), p.y.as_f32(), p.z.as_f32()];
                    let body = self.consumer.session.body(request).unwrap();
                    assert_eq!(
                        self.consumer
                            .geometry
                            .certify(xyz(q.origin), xyz(q.goal), body),
                        Ok(())
                    );
                    assert_eq!(
                        self.consumer
                            .geometry
                            .certify(xyz(q.launch_origin), xyz(q.goal), body),
                        Ok(())
                    );
                    if self.block_launch_only {
                        // Put real unclassified collision beyond the source segment's
                        // swept box, inside only the corrected launch segment's box.
                        let extra = q.launch_origin.y.as_f32() - q.origin.y.as_f32();
                        let at = [
                            q.origin.x.as_f32(),
                            q.origin.y.as_f32() + body.radius.as_f32() + extra * 0.5,
                            body.height.as_f32() * 0.5,
                        ];
                        self.wall = Some(
                            self.consumer
                                .geometry
                                .app
                                .world_mut()
                                .spawn((
                                    Collider::cuboid(
                                        extra * 0.25,
                                        body.height.as_f32(),
                                        extra * 0.25,
                                    ),
                                    RigidBody::Static,
                                    Transform::from_translation(
                                        benilla_assets::coords::wow_to_bevy(at),
                                    ),
                                    CollisionLayers::DEFAULT,
                                ))
                                .id(),
                        );
                        assert_eq!(
                            self.consumer
                                .geometry
                                .certify(xyz(q.origin), xyz(q.goal), body),
                            Ok(())
                        );
                        assert_eq!(
                            self.consumer
                                .geometry
                                .certify(xyz(q.launch_origin), xyz(q.goal), body),
                            Err(super::Failure::UnsupportedNavigation)
                        );
                    }
                }
            }
            self.consumer.resolve(request)
        }
        fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
            self.consumer.observe_derived(reply)
        }
    }

    for block_launch_only in [false, true] {
        // The actual initialized walk speed keeps this 3.5-yard Chase running
        // at the second 1200ms pursuit check, between map relocation updates.
        let (mut world, mut consumer) = setup_with_profile(Some(0), Some(800), |profile| {
            profile.player.start = ai::Point::new(5., 11., 0.).unwrap();
            profile.npcs[0].ai.pursuit_ms = 101;
            profile.npcs[0].ai.leash_radius = f::real(0.01);
            let motion = profile
                .source_motion
                .as_mut()
                .unwrap()
                .npcs
                .get_mut(&2)
                .unwrap();
            motion.default_movement = motion::DefaultMovement::Idle;
            motion.chase_walking = true;
        });
        consumer.allow_chase = true;
        consumer.other.visibility = Some(Check::Passed);
        let mut reached = false;
        let mut launched_walking_chase = false;
        for _ in 0..40 {
            let before = world.checkpoint().unwrap();
            let tick = world.state().tick + 1;
            let (result, home, wall) = {
                let mut probe = Probe {
                    consumer: &mut consumer,
                    block_launch_only,
                    home: None,
                    wall: None,
                };
                let result = world.step(tick, None, &mut probe).cloned();
                (result, probe.home, probe.wall)
            };
            let Some(home) = home else {
                let record = result.unwrap();
                if record.replies.iter().any(|reply| {
                    matches!(
                        &reply.request.query,
                        Query::SourceMotion(MotionQuery::Route(route))
                            if route.request.kind == motion::RouteKind::ChaseInitial
                    )
                }) {
                    let initialized = world.state().npcs[&2].initialized_creature().unwrap();
                    assert!(record.events.iter().any(|event| matches!(
                        event,
                        Event::NpcMotion {
                            event: motion::Event::SplineLaunched {
                                duration_ms,
                                velocity,
                                walking: true,
                                ..
                            },
                            ..
                        } if *duration_ms > 1200
                            && velocity.bits() == initialized.model.walk_speed.bits()
                    )));
                    assert_eq!(world.state().npcs[&2].ai.phase(), ai::Phase::Combat);
                    launched_walking_chase = true;
                }
                consumer.acknowledge(&world);
                continue;
            };
            assert!(launched_walking_chase);
            if block_launch_only {
                assert_eq!(
                    result,
                    Err(NativeError::AuthorityUnavailable(
                        "authored loaded Wander geometry"
                    ))
                );
                assert_eq!(world.checkpoint().unwrap(), before);
                consumer.geometry.app.world_mut().despawn(wall.unwrap());
                let retry = world.step(tick, None, &mut consumer).unwrap();
                assert!(retry.replies.iter().any(|reply| matches!(
                    &reply.request.query,
                    Query::SourceMotion(MotionQuery::Route(route)) if route.request == home
                )));
            } else {
                result.unwrap();
            }
            consumer.acknowledge(&world);
            let snapshot = snapshot(&world);
            assert!(snapshot.suspended_wander.is_none());
            assert!(matches!(snapshot.generator, motion::Generator::Home { .. }));
            assert_eq!(
                snapshot.spline.unwrap().spline.points[0],
                home.launch_origin
            );
            assert_eq!(consumer.providers.route.registered_generator_count(), 0);
            assert_eq!(consumer.providers.route.registered_constructor_count(), 0);
            reached = true;
            break;
        }
        assert!(
            reached,
            "actual Idle owner requested a distinct-origin Home route"
        );
    }
}
