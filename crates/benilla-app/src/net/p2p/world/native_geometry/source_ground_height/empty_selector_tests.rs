// SPDX-License-Identifier: GPL-3.0-or-later
//! Sequential source probes over the actual loaded authored ground reader.
use super::super::super::{
    core::EmptySelectorNearPointPolicy, products::vmap::QueryBudget,
    source_near_point::SourceNearPointConfiguration, source_near_point_empty,
};
use super::*;
use benilla_sim::near_point::{self, selector};

fn candidate_context(
    mut context: AuthorityRequest,
    target: [f32; 2],
    distance: f32,
) -> AuthorityRequest {
    let Query::SourceMotion(MotionQuery::NearPoint(near)) = &mut context.query else {
        unreachable!()
    };
    // Explicit authored derivative of an actual staged request; begin freezes it
    // for subqueries without asserting committed runtime pose liveness.
    near.request.target.pose.position.x = ai::Real::new(target[0]).unwrap();
    near.request.target.pose.position.y = ai::Real::new(target[1]).unwrap();
    near.request.distance = ai::Real::new(distance).unwrap();
    near.request.angle = NearPointAngle::Absolute(ai::Real::ZERO);
    context
}

#[test]
fn exhausted_sequence_carries_last_allowed_z_back_to_first_xy_on_actual_stacked_reader() {
    let (products, config) = products::authored_ground_height();
    let proof = NonRenderParticipants::authored_fixture(config.content, config.map);
    let snapshot = products.load_map_height(config, Some(&proof)).unwrap();
    let (world, session, context) = setup(snapshot.binding());
    let provider = SourceGroundHeightProvider::bind(&session, world.profile(), snapshot).unwrap();
    let mid = 0.5f32 * (64.0 * 533.333_3);
    let context = candidate_context(context, [mid - 31., mid - 1.], 10.);
    let Query::SourceMotion(MotionQuery::NearPoint(near)) = &context.query else {
        unreachable!()
    };
    let q = &near.request;
    let offsets: Vec<_> = selector::EmptySelector::new(
        q.owner_bounding_radius.as_f32().to_bits(),
        q.distance.as_f32().to_bits(),
    )
    .unwrap()
    .collect();
    assert!(!offsets.is_empty());
    let bound = provider.begin(&session, &context).unwrap();
    let mut sequence: EmptySelectorGroundSequence<'_, '_> =
        bound.empty_selector_sequence().unwrap();
    let mut budget = QueryBudget::new(deadline());
    let first: SequentialGroundReply = sequence.next_with_budget(&mut budget).unwrap().unwrap();
    assert_eq!(first.phase, SequentialGroundPhase::Primary);
    assert_eq!(first.ground.request.ticket, 1);
    assert_eq!(first.los_ticket, Some(2));
    assert_eq!(first.ground.ground_height, 9f32.to_bits());
    assert_eq!(first.ground.allowed_z, 9f32.to_bits());
    let first_xy = [
        first.ground.request.position[0],
        first.ground.request.position[1],
    ];
    assert_eq!(first_xy, [mid - 21., mid - 1.].map(f32::to_bits));
    let mut previous_z = first.ground.allowed_z;
    for (index, offset) in offsets.iter().copied().enumerate() {
        let reply = sequence.next_with_budget(&mut budget).unwrap().unwrap();
        assert_eq!(reply.phase, SequentialGroundPhase::Free);
        assert_eq!(reply.ground.request.snapshot, provider.binding());
        assert_eq!(reply.ground.request.context, context);
        assert_eq!(reply.ground.request.ticket, 2 * (index as u64 + 1) + 1);
        assert_eq!(reply.los_ticket, Some(2 * (index as u64 + 1) + 2));
        let angle = selector::candidate_angle_bits(0, offset).unwrap();
        let xy = near_point::fallback_candidate_bits(
            [mid - 31., mid - 1.].map(f32::to_bits),
            10f32.to_bits(),
            angle,
        )
        .unwrap();
        assert_eq!(
            reply.ground.request.position,
            [xy[0], xy[1], 10f32.to_bits()]
        );
        previous_z = reply.ground.allowed_z;
    }
    assert_eq!(previous_z, 7f32.to_bits());
    let final_reply = sequence.next_with_budget(&mut budget).unwrap().unwrap();
    assert_eq!(final_reply.phase, SequentialGroundPhase::Final);
    assert_eq!(final_reply.ground.request.snapshot, provider.binding());
    assert_eq!(final_reply.ground.request.context, context);
    assert_eq!(final_reply.ground.request.ticket, 1259);
    assert_eq!(final_reply.los_ticket, None);
    assert_eq!(
        final_reply.ground.request.position,
        [first_xy[0], first_xy[1], previous_z]
    );
    assert_eq!(final_reply.ground.ground_height, 7f32.to_bits());
    assert_eq!(final_reply.ground.allowed_z, 7f32.to_bits());
    assert_eq!(sequence.next_with_budget(&mut budget), Ok(None));
    assert_eq!(sequence.next_with_budget(&mut budget), Ok(None));

    // Same XY with target Z selects 9, carried Z selects 7. The general initial
    // API cannot be used to invent the latter phase or supply its predecessor.
    let bound = provider.begin(&session, &context).unwrap();
    assert_eq!(
        bound.query_initial_candidate(final_reply.ground.request, deadline()),
        Err(Failure::Binding)
    );
    assert_eq!(
        bound
            .query_initial_candidate(first.ground.request, deadline())
            .unwrap()
            .ground_height,
        9f32.to_bits()
    );
}

#[test]
fn missing_free_probe_poison_is_not_source_exhaustion_or_a_skipped_predecessor() {
    let (products, config) = products::authored_ground_height();
    let proof = NonRenderParticipants::authored_fixture(config.content, config.map);
    let snapshot = products.load_map_height(config, Some(&proof)).unwrap();
    let (world, session, context) = setup(snapshot.binding());
    let provider = SourceGroundHeightProvider::bind(&session, world.profile(), snapshot).unwrap();
    let mid = 0.5f32 * (64.0 * 533.333_3);
    let context = candidate_context(context, [mid - 532., mid - 10.], 4.);
    let mut sequence = provider
        .begin(&session, &context)
        .unwrap()
        .empty_selector_sequence()
        .unwrap();
    let mut budget = QueryBudget::new(deadline());
    assert_eq!(
        sequence
            .next_with_budget(&mut budget)
            .unwrap()
            .unwrap()
            .phase,
        SequentialGroundPhase::Primary
    );
    let mut free_count = 0;
    loop {
        match sequence.next_with_budget(&mut budget) {
            Ok(Some(reply)) => {
                assert_eq!(reply.phase, SequentialGroundPhase::Free);
                free_count += 1;
                assert!(free_count <= selector::MAX_OFFSETS);
            }
            Err(failure) => {
                assert_eq!(failure, Failure::MissingGeometry);
                break;
            }
            Ok(None) => panic!("missing geometry must never become exhausted selection"),
        }
    }
    assert!(free_count > 0);
    for _ in 0..3 {
        let mut fresh_budget = QueryBudget::new(deadline());
        assert_eq!(
            sequence.next_with_budget(&mut fresh_budget),
            Err(Failure::MissingGeometry)
        );
    }
}

#[test]
fn expired_budget_poison_cannot_be_repaired_by_skipping_the_failed_probe() {
    let (products, config) = products::authored_ground_height();
    let proof = NonRenderParticipants::authored_fixture(config.content, config.map);
    let snapshot = products.load_map_height(config, Some(&proof)).unwrap();
    let (world, session, context) = setup(snapshot.binding());
    let provider = SourceGroundHeightProvider::bind(&session, world.profile(), snapshot).unwrap();
    let mid = 0.5f32 * (64.0 * 533.333_3);
    let context = candidate_context(context, [mid - 31., mid - 1.], 10.);
    for fail_primary in [false, true] {
        let mut sequence = provider
            .begin(&session, &context)
            .unwrap()
            .empty_selector_sequence()
            .unwrap();
        if !fail_primary {
            assert_eq!(
                sequence
                    .next_with_budget(&mut QueryBudget::new(deadline()))
                    .unwrap()
                    .unwrap()
                    .phase,
                SequentialGroundPhase::Primary
            );
        }
        assert_eq!(
            sequence.next_with_budget(&mut QueryBudget::new(Instant::now())),
            Err(Failure::Deadline)
        );
        assert_eq!(
            sequence.next_with_budget(&mut QueryBudget::new(deadline())),
            Err(Failure::Deadline)
        );
    }
}

#[test]
fn empty_selector_typed_binding_retains_configuration_epoch_context_and_session_policy() {
    let (products, height) = products::authored_ground_height();
    let proof = NonRenderParticipants::authored_fixture(height.content, height.map);
    let config = SourceNearPointConfiguration {
        height,
        detect_position_collision: true,
        vmap_line_of_sight_enabled: true,
    };
    let snapshot = products.load_map_height(height, Some(&proof)).unwrap();
    let revision = source_near_point_empty::geometry_revision(&snapshot, config).unwrap();
    let (world, mut session, context) =
        setup_for_revision(snapshot.binding(), None, false, revision);
    session.empty_selector_near_point =
        Some(EmptySelectorNearPointPolicy::from_profile(world.profile()).unwrap());
    let provider = SourceGroundHeightProvider::bind_with_empty_selector(
        &session,
        world.profile(),
        snapshot,
        config,
    )
    .unwrap();
    let mid = 0.5f32 * (64.0 * 533.333_3);
    let context = candidate_context(context, [mid - 31., mid - 1.], 10.);
    let mut sequence = provider
        .begin(&session, &context)
        .unwrap()
        .empty_selector_sequence()
        .unwrap();
    let first = sequence
        .next_with_budget(&mut QueryBudget::new(deadline()))
        .unwrap()
        .unwrap();
    assert_eq!(first.ground.request.context, context);
    assert_eq!(first.ground.request.snapshot, provider.binding());
    for field in 0..4 {
        let mut changed = config;
        match field {
            0 => changed.height.source_epoch[0] ^= 1,
            1 => changed.height.effective_config[0] ^= 1,
            2 => changed.detect_position_collision = false,
            _ => changed.vmap_line_of_sight_enabled = false,
        }
        let snapshot = products.load_map_height(height, Some(&proof)).unwrap();
        assert!(SourceGroundHeightProvider::bind_with_empty_selector(
            &session,
            world.profile(),
            snapshot,
            changed,
        )
        .is_err());
    }
    let snapshot = products.load_map_height(height, Some(&proof)).unwrap();
    assert!(SourceGroundHeightProvider::bind(&session, world.profile(), snapshot).is_err());
    let mut wrong_session = session.clone();
    wrong_session.empty_selector_near_point = None;
    assert!(matches!(
        provider.begin(&wrong_session, &context),
        Err(Failure::Binding)
    ));
    let mut wrong_context = context.clone();
    wrong_context.binding.geometry[0] ^= 1;
    assert!(matches!(
        provider.begin(&session, &wrong_context),
        Err(Failure::Binding)
    ));
    for field in 0..3 {
        let mut request = first.ground.request.clone();
        match field {
            0 => request.snapshot.source_epoch[0] ^= 1,
            1 => {
                let Query::SourceMotion(MotionQuery::NearPoint(near)) = &mut request.context.query
                else {
                    unreachable!()
                };
                near.request.binding.query_id += 1;
            }
            _ => request.position[2] = 7f32.to_bits(),
        }
        assert_eq!(
            provider
                .begin(&session, &context)
                .unwrap()
                .query_initial_candidate(request, deadline()),
            Err(Failure::Binding)
        );
    }
}
