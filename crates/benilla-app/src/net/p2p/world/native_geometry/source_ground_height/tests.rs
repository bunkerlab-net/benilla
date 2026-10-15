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
    let mut profile = fixture::profile();
    profile.content_digest = binding.content;
    profile.map_id = binding.map;
    profile.geometry_revision = geometry_revision(binding);
    profile.npcs[0].ai.map_id = binding.map;
    profile.npcs[0].ai.content_revision = ai::movement::Revision(binding.content);
    profile.player.start.z = ai::Real::new(10.).unwrap();
    profile.npcs[0].ai.spawn.z = ai::Real::new(10.).unwrap();
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
    };
    let mut world =
        NativeWorld::synthetic_component_test(profile, session.world, f::player(), 1234).unwrap();
    let mut authority = fixture::SyntheticMotionAuthority::default();
    for tick in 1..=2 {
        world.step(tick, None, &mut authority).unwrap();
    }
    let context = authority
        .queries
        .into_iter()
        .find(|q| matches!(q.query, Query::SourceMotion(MotionQuery::NearPoint(_))))
        .unwrap();
    (world, session, context)
}
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
    for field in 0..12 {
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
            _ => wrong.position[2] = 9f32.to_bits(),
        }
        assert_eq!(
            bound.query_initial_candidate(wrong, deadline()),
            Err(Failure::Binding)
        );
    }
    assert_eq!(
        bound.query_initial_candidate(query.clone(), Instant::now()),
        Err(Failure::MissingGeometry)
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
