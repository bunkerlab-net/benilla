// SPDX-License-Identifier: GPL-3.0-or-later
//! Real Route geometry with authored owner/combat setup. Non-geometry callbacks
//! stay explicit component fixtures; this is not production native admission.
use super::super::{
    core,
    real_corridor_fixture::Fixture,
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
}
impl Authority for Consumer {
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
                .unwrap(),
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
    let providers = products.bind(&session, &profile, f::player()).unwrap();
    let world =
        NativeWorld::synthetic_component_test(profile, session.world, f::player(), 1234).unwrap();
    (
        world,
        Consumer {
            geometry,
            session,
            providers,
            other: Default::default(),
            replies: vec![],
            fail_after_route: false,
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
    let local = super::super::install_with_same_polygon(
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
        }
        assert_eq!(authority.replies.len(), 1);
        authority.fail_after_route = false;
        world
            .step(world.state().tick + 1, None, &mut authority)
            .unwrap();
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
