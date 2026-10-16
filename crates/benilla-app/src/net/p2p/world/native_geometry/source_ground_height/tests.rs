// SPDX-License-Identifier: GPL-3.0-or-later
//! Real staged runtime requests over authored product bytes, no startup admission.
use super::super::{
    core::{Body, NonRenderParticipants, PlayerMotion},
    products,
};
use super::*;
use benilla_sim::creature_ai as ai;
use benilla_world_runtime::native::{
    synthetic_fixture as f, synthetic_motion_fixture as fixture, NativeWorld,
};
use std::{collections::BTreeMap, time::Duration};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
fn setup(binding: SnapshotBinding) -> (NativeWorld, Session, AuthorityRequest) {
    setup_at(binding, None)
}
fn setup_at(
    binding: SnapshotBinding,
    positions: Option<([f32; 2], [f32; 2])>,
) -> (NativeWorld, Session, AuthorityRequest) {
    setup_for(binding, positions, false)
}
fn setup_for(
    binding: SnapshotBinding,
    positions: Option<([f32; 2], [f32; 2])>,
    absolute: bool,
) -> (NativeWorld, Session, AuthorityRequest) {
    setup_for_revision(binding, positions, absolute, geometry_revision(binding))
}
fn setup_for_revision(
    binding: SnapshotBinding,
    positions: Option<([f32; 2], [f32; 2])>,
    absolute: bool,
    revision: [u8; 32],
) -> (NativeWorld, Session, AuthorityRequest) {
    let mut profile = fixture::profile();
    profile.content_digest = binding.content;
    profile.map_id = binding.map;
    profile.geometry_revision = revision;
    profile.npcs[0].ai.map_id = binding.map;
    profile.npcs[0].ai.content_revision = ai::movement::Revision(binding.content);
    profile.player.start.z = ai::Real::new(10.).unwrap();
    profile.npcs[0].ai.spawn.z = ai::Real::new(10.).unwrap();
    if let Some((owner, target)) = positions {
        profile.npcs[0].ai.spawn.x = ai::Real::new(owner[0]).unwrap();
        profile.npcs[0].ai.spawn.y = ai::Real::new(owner[1]).unwrap();
        profile.player.start.x = ai::Real::new(target[0]).unwrap();
        profile.player.start.y = ai::Real::new(target[1]).unwrap();
    }
    profile
        .initialize_creature_capabilities(fixture::creature_capability_sources(&profile))
        .unwrap();
    let motion = profile.source_motion.as_ref().unwrap();
    let session = Session {
        world: [8; 16],
        profile: profile.digest(),
        policy: profile.authority_policy,
        content: profile.content_digest,
        geometry: profile.geometry_revision,
        map: profile.map_id,
        bodies: BTreeMap::from([(
            2,
            Body {
                radius: profile.npcs[0].ai.bounding_radius,
                height: motion.npc_policy.settings.collision_height,
            },
        )]),
        player: Some(PlayerMotion {
            body: Body {
                radius: profile.player.bounding_radius,
                height: motion.player_collision_height,
            },
            speed: ai::Real::new(7.).unwrap(),
        }),
        participants: Some(NonRenderParticipants::authored_fixture(
            binding.content,
            binding.map,
        )),
        static_los: None,
        primary_near_point: None,
        empty_selector_near_point: None,
        player_environment: None,
        same_polygon_route: None,
    };
    let mut world =
        NativeWorld::synthetic_component_test(profile, session.world, f::player(), 1234).unwrap();
    let mut authority = fixture::SyntheticMotionAuthority::default();
    for tick in 1..=if absolute { 30 } else { 2 } {
        world.step(tick, None, &mut authority).unwrap();
    }
    let context = authority
        .queries
        .into_iter()
        .find(|q| match &q.query {
            Query::SourceMotion(MotionQuery::NearPoint(near)) => {
                matches!(near.request.angle, NearPointAngle::Absolute(_)) == absolute
            }
            _ => false,
        })
        .unwrap();
    (world, session, context)
}

#[path = "empty_selector_tests.rs"]
mod empty_selector_tests;
fn request(
    provider: &SourceGroundHeightProvider<'_>,
    context: &AuthorityRequest,
) -> GroundHeightRequest {
    let mid = 0.5f32 * (64.0 * 533.333_3);
    GroundHeightRequest {
        snapshot: provider.binding(),
        context: context.clone(),
        ticket: 1,
        position: [mid - 21., mid - 1., 10.].map(f32::to_bits),
    }
}

#[test]
fn actual_catalog_bound_runtime_request_consumes_loaded_ground_products() {
    let (products, config) = products::authored_ground_height();
    let proof = NonRenderParticipants::authored_fixture(config.content, config.map);
    let snapshot = products.load_map_height(config, Some(&proof)).unwrap();
    let (world, session, context) = setup(snapshot.binding());
    let provider = SourceGroundHeightProvider::bind(&session, world.profile(), snapshot).unwrap();
    let before = world.checkpoint().unwrap();
    let bound = provider.begin(&session, &context).unwrap();
    let query = request(&provider, &context);
    let result = bound
        .query_initial_candidate(query.clone(), deadline())
        .unwrap();
    assert_eq!(result.request, query);
    assert_eq!(result.ground_height, 9f32.to_bits());
    assert_eq!(result.allowed_z, 9f32.to_bits());
    // The actual request's default origin is outside these authored products. A
    // derived query must fail there even though a caller-selected covered probe worked.
    assert_eq!(
        bound.derive_initial_candidate(2, deadline()),
        Err(Failure::MissingGeometry)
    );
    // This standalone dependency publishes no complete NearPoint or world mutation.
    assert_eq!(world.checkpoint().unwrap(), before);
}

#[test]
fn source_catalog_and_outer_session_identity_are_required_before_any_height() {
    let (products, config) = products::authored_ground_height();
    let proof = NonRenderParticipants::authored_fixture(config.content, config.map);
    let snapshot = products.load_map_height(config, Some(&proof)).unwrap();
    let (world, session, context) = setup(snapshot.binding());
    let provider = SourceGroundHeightProvider::bind(&session, world.profile(), snapshot).unwrap();
    for field in 0..8 {
        let mut wrong = context.clone();
        match field {
            0 => wrong.binding.world[0] ^= 1,
            1 => wrong.binding.profile[0] ^= 1,
            2 => wrong.binding.authority_policy[0] ^= 1,
            3 => wrong.binding.geometry[0] ^= 1,
            4 => wrong.binding.tick = 0,
            5 => wrong.binding.tick += 1,
            6 => wrong.binding.npc.as_mut().unwrap().generation += 1,
            _ => wrong.binding.npc = None,
        }
        assert!(provider.begin(&session, &wrong).is_err(), "field {field}");
    }
    for field in 0..5 {
        let mut wire = serde_json::to_value(&context).unwrap();
        let witness = &mut wire["query"]["SourceMotion"]["NearPoint"]["searcher"];
        match field {
            0 => {
                witness["source_binding"][0] =
                    (witness["source_binding"][0].as_u64().unwrap() ^ 1).into()
            }
            1 => witness["inhabit_type"] = 3.into(),
            2 => witness["capability_movement_flags"] = 0x400_u32.into(),
            3 => witness["npc"]["generation"] = 1.into(),
            _ => witness["npc"]["spawn"] = 3.into(),
        }
        let wrong: AuthorityRequest = serde_json::from_value(wire).unwrap();
        assert!(
            provider.begin(&session, &wrong).is_err(),
            "witness field {field}"
        );
    }
    let mut missing = session.clone();
    missing.participants = None;
    assert!(matches!(
        provider.begin(&missing, &context),
        Err(Failure::UnprovenParticipants)
    ));
    let mut profile = world.profile().clone();
    profile.npcs[0].entry += 1;
    let snapshot = products.load_map_height(config, Some(&proof)).unwrap();
    assert!(SourceGroundHeightProvider::bind(&session, &profile, snapshot).is_err());
}

#[test]
fn every_frozen_pose_query_identity_and_candidate_z_ticket_stays_bound() {
    let (products, config) = products::authored_ground_height();
    let proof = NonRenderParticipants::authored_fixture(config.content, config.map);
    let snapshot = products.load_map_height(config, Some(&proof)).unwrap();
    let (world, session, context) = setup(snapshot.binding());
    let provider = SourceGroundHeightProvider::bind(&session, world.profile(), snapshot).unwrap();
    let bound = provider.begin(&session, &context).unwrap();
    let query = request(&provider, &context);
    for field in 0..16 {
        let mut wrong = query.clone();
        let Query::SourceMotion(MotionQuery::NearPoint(near)) = &mut wrong.context.query else {
            unreachable!()
        };
        match field {
            0 => wrong.snapshot.map += 1,
            1 => wrong.snapshot.content[0] ^= 1,
            2 => wrong.snapshot.source_epoch[0] ^= 1,
            3 => wrong.snapshot.inventory[0] ^= 1,
            4 => wrong.snapshot.policy[0] ^= 1,
            5 => wrong.ticket = 0,
            6 => near.request.owner.position.x = ai::Real::new(99.).unwrap(),
            7 => near.request.target.pose.position.x = ai::Real::new(99.).unwrap(),
            8 => near.request.owner.orientation = ai::Real::ZERO,
            9 => near.request.binding.query_id += 1,
            10 => near.request.binding.now_ms += 1,
            11 => wrong.position[2] = 9f32.to_bits(),
            12 => near.request.distance = ai::Real::new(99.).unwrap(),
            13 => near.request.angle = NearPointAngle::Absolute(ai::Real::new(2.).unwrap()),
            14 => {
                near.request.target.movement_flags =
                    benilla_sim::creature_motion::GroundMovementFlags::from_bits(1).unwrap()
            }
            _ => near.request.owner_bounding_radius = ai::Real::new(99.).unwrap(),
        }
        assert_eq!(
            bound.query_initial_candidate(wrong, deadline()),
            Err(Failure::Binding)
        );
    }
    assert_eq!(
        bound.query_initial_candidate(query.clone(), Instant::now()),
        Err(Failure::Deadline)
    );
    let mut uncovered = query.clone();
    uncovered.position[0] = 0f32.to_bits();
    uncovered.position[1] = 0f32.to_bits();
    assert_eq!(
        bound.query_initial_candidate(uncovered, deadline()),
        Err(Failure::MissingGeometry)
    );
    let mut nonfinite = query;
    nonfinite.position[0] = f32::NAN.to_bits();
    assert_eq!(
        bound.query_initial_candidate(nonfinite, deadline()),
        Err(Failure::MissingGeometry)
    );
}

#[test]
fn actual_runtime_candidate_derives_xy_and_consumes_loaded_height_without_publishing_motion() {
    let (products, config) = products::authored_ground_height();
    let proof = NonRenderParticipants::authored_fixture(config.content, config.map);
    let snapshot = products.load_map_height(config, Some(&proof)).unwrap();
    let mid = 0.5f32 * (64.0 * 533.333_3);
    let (world, session, context) = setup_at(
        snapshot.binding(),
        Some(([mid - 34.5, mid - 1.], [mid - 18.5, mid - 1.])),
    );
    let provider = SourceGroundHeightProvider::bind(&session, world.profile(), snapshot).unwrap();
    let before = world.checkpoint().unwrap();
    let bound = provider.begin(&session, &context).unwrap();
    let result = bound.derive_initial_candidate(7, deadline()).unwrap();
    let Query::SourceMotion(MotionQuery::NearPoint(near)) = &context.query else {
        unreachable!()
    };
    assert_eq!(near.request.angle, NearPointAngle::CurrentTargetToOwner);
    assert_eq!(result.angle, benilla_sim::facing::PI_BITS);
    let target = near.request.target.pose.position;
    let expected = benilla_sim::near_point::candidate_bits(
        [target.x.as_f32().to_bits(), target.y.as_f32().to_bits()],
        near.request.distance.as_f32().to_bits(),
        result.angle,
    )
    .unwrap();
    assert_eq!(
        result.ground.request.position,
        [expected[0], expected[1], 10f32.to_bits()]
    );
    assert_eq!(result.ground.request.context, context);
    assert_eq!(result.ground.request.ticket, 7);
    assert_eq!(result.policy, initial_candidate::policy_digest());
    assert_eq!(result.ground.ground_height, 9f32.to_bits());
    assert_eq!(result.ground.allowed_z, 9f32.to_bits());
    assert_eq!(world.checkpoint().unwrap(), before);
    assert_eq!(
        bound.derive_initial_candidate(0, deadline()),
        Err(Failure::Binding)
    );
    assert_eq!(
        bound.derive_initial_candidate(7, Instant::now()),
        Err(Failure::Deadline)
    );
}

#[test]
fn initial_candidate_absolute_orientation_and_coincident_inputs_have_explicit_semantics() {
    let (products, config) = products::authored_ground_height();
    let proof = NonRenderParticipants::authored_fixture(config.content, config.map);
    let snapshot = products.load_map_height(config, Some(&proof)).unwrap();
    let (world, session, context) = setup(snapshot.binding());
    let provider = SourceGroundHeightProvider::bind(&session, world.profile(), snapshot).unwrap();
    let mid = 0.5f32 * (64.0 * 533.333_3);
    // These are explicitly authored derivative requests, not claims of live runtime poses.
    let mut request = context;
    let Query::SourceMotion(MotionQuery::NearPoint(near)) = &mut request.query else {
        unreachable!()
    };
    near.request.target.pose.position.x = ai::Real::new(mid - 21.).unwrap();
    near.request.target.pose.position.y = ai::Real::new(mid - 1.).unwrap();
    near.request.owner.position = near.request.target.pose.position;
    near.request.distance = ai::Real::ZERO;
    near.request.angle = NearPointAngle::CurrentTargetToOwner;
    let initial = provider
        .begin(&session, &request)
        .unwrap()
        .derive_initial_candidate(1, deadline())
        .unwrap();
    assert_eq!(initial.angle, 0);
    for orientation in [0., 1., 6.] {
        let mut changed = request.clone();
        let Query::SourceMotion(MotionQuery::NearPoint(near)) = &mut changed.query else {
            unreachable!()
        };
        near.request.owner.orientation = ai::Real::new(orientation).unwrap();
        near.request.target.pose.orientation = ai::Real::new(orientation).unwrap();
        let reply = provider
            .begin(&session, &changed)
            .unwrap()
            .derive_initial_candidate(1, deadline())
            .unwrap();
        assert_eq!(reply.angle, initial.angle);
        assert_eq!(
            reply.ground.request.position,
            initial.ground.request.position
        );
        assert_eq!(reply.ground.request.context, changed);
    }
    for angle in [0., f32::from_bits(0x40c9_0fdb)] {
        let mut absolute = request.clone();
        let Query::SourceMotion(MotionQuery::NearPoint(near)) = &mut absolute.query else {
            unreachable!()
        };
        near.request.angle = NearPointAngle::Absolute(ai::Real::new(angle).unwrap());
        let reply = provider
            .begin(&session, &absolute)
            .unwrap()
            .derive_initial_candidate(1, deadline())
            .unwrap();
        assert_eq!(reply.angle, angle.to_bits());
        assert_eq!(reply.ground.request.context, absolute);
    }
    for angle in [-1., f32::from_bits(0x40c9_0fdc)] {
        let mut invalid = request.clone();
        let Query::SourceMotion(MotionQuery::NearPoint(near)) = &mut invalid.query else {
            unreachable!()
        };
        near.request.angle = NearPointAngle::Absolute(ai::Real::new(angle).unwrap());
        let bound = provider.begin(&session, &invalid).unwrap();
        assert_eq!(
            bound.derive_initial_candidate(1, deadline()),
            Err(Failure::Invalid)
        );
    }
}

#[test]
fn real_runtime_backpedal_absolute_request_uses_the_same_bound_candidate_path() {
    let (products, config) = products::authored_ground_height();
    let proof = NonRenderParticipants::authored_fixture(config.content, config.map);
    let snapshot = products.load_map_height(config, Some(&proof)).unwrap();
    let mid = 0.5f32 * (64.0 * 533.333_3);
    let (world, session, context) = setup_for(
        snapshot.binding(),
        Some(([mid - 21., mid - 1.], [mid - 21., mid - 1.])),
        true,
    );
    let Query::SourceMotion(MotionQuery::NearPoint(near)) = &context.query else {
        unreachable!()
    };
    let NearPointAngle::Absolute(angle) = near.request.angle else {
        unreachable!()
    };
    let provider = SourceGroundHeightProvider::bind(&session, world.profile(), snapshot).unwrap();
    let before = world.checkpoint().unwrap();
    let result = provider
        .begin(&session, &context)
        .unwrap()
        .derive_initial_candidate(1, deadline())
        .unwrap();
    assert_eq!(result.angle, angle.as_f32().to_bits());
    assert_eq!(result.ground.request.context, context);
    assert_eq!(world.checkpoint().unwrap(), before);
    // Captured source query is not asserted to be current after later authored ticks.
    // The eventual owner must evaluate/accept this witness inside its live transaction.
}

#[test]
fn raw_subnormal_bearing_is_not_narrowed_through_persistent_real() {
    let (products, config) = products::authored_ground_height();
    let proof = NonRenderParticipants::authored_fixture(config.content, config.map);
    let snapshot = products.load_map_height(config, Some(&proof)).unwrap();
    let (_, _, context) = setup(snapshot.binding());
    let Query::SourceMotion(MotionQuery::NearPoint(near)) = context.query else {
        unreachable!()
    };
    let mut q = near.request;
    q.target.pose.position = ai::Point::new(0., 0., 10.).unwrap();
    q.owner.position = ai::Point::new(32760., f32::MIN_POSITIVE, 10.).unwrap();
    q.distance = ai::Real::new(1.).unwrap();
    q.angle = NearPointAngle::CurrentTargetToOwner;
    let (angle, position) = initial_candidate::initial_position(&q).unwrap();
    assert!(angle > 0 && angle < f32::MIN_POSITIVE.to_bits());
    assert!(ai::Real::new(f32::from_bits(angle)).is_err());
    assert_eq!(position, [1f32.to_bits(), angle, 10f32.to_bits()]);
    // This isolates raw arithmetic. No product coverage or admission at (0,0) is asserted.
}
