// SPDX-License-Identifier: GPL-3.0-or-later
//! Envelope tests only. Geometry facts here do not establish product/source parity.
use super::*;
use benilla_world_runtime::native::{
    synthetic_fixture as fixture, synthetic_motion_fixture as source, NativeWorld,
};

fn profile() -> Profile {
    let mut profile = source::profile();
    profile
        .initialize_creature_capabilities(source::creature_capability_sources(&profile))
        .unwrap();
    profile
}
pub(super) fn setup() -> (Session, AuthorityRequest) {
    let profile = profile();
    let source = profile.source_motion.as_ref().unwrap();
    let session = Session {
        world: [8; 16],
        profile: profile.digest(),
        policy: profile.authority_policy,
        content: profile.content_digest,
        geometry: profile.geometry_revision,
        map: profile.map_id,
        bodies: BTreeMap::from([(
            profile.npcs[0].ai.creature_id,
            Body {
                radius: profile.npcs[0].ai.bounding_radius,
                height: source
                    .npc(profile.npcs[0].ai.creature_id)
                    .unwrap()
                    .npc_policy
                    .settings
                    .collision_height,
            },
        )]),
        player: Some(PlayerMotion {
            body: Body {
                radius: profile.player.bounding_radius,
                height: source.player_collision_height,
            },
            speed: ai::Real::new(7.).unwrap(),
        }),
        participants: Some(NonRenderParticipants::authored_fixture(
            profile.content_digest,
            profile.map_id,
        )),
        static_los: None,
        primary_near_point: Some(PrimaryNearPointPolicy::from_profile(&profile).unwrap()),
        empty_selector_near_point: None,
        source_location: None,
        player_environment: None,
        same_polygon_route: None,
    };
    let mut world =
        NativeWorld::synthetic_component_test(profile, session.world, fixture::player(), 1234)
            .unwrap();
    let mut authority = source::SyntheticMotionAuthority::default();
    for tick in 1..=2 {
        world.step(tick, None, &mut authority).unwrap();
    }
    let request = authority
        .queries
        .into_iter()
        .find(|r| matches!(r.query, Query::SourceMotion(MotionQuery::NearPoint(_))))
        .unwrap();
    (session, request)
}
fn inner(request: &AuthorityRequest) -> &motion::NearPointRequest {
    let Query::SourceMotion(MotionQuery::NearPoint(near)) = &request.query else {
        panic!("expected selected near-point request")
    };
    &near.request
}
fn inner_mut(request: &mut AuthorityRequest) -> &mut motion::NearPointRequest {
    let Query::SourceMotion(MotionQuery::NearPoint(near)) = &mut request.query else {
        panic!("expected selected near-point request")
    };
    &mut near.request
}
pub(super) fn facts(request: &AuthorityRequest) -> Facts {
    Facts::SourceMotion(MotionFacts::Spatial(Box::new(motion::Reply {
        request: motion::Query::NearPoint(inner(request).clone()),
        fact: motion::Fact::NearPoint(ai::Point::new(1., 2., 3.).unwrap()),
    })))
}
fn spatial_mut(facts: &mut Facts) -> &mut motion::Reply {
    let Facts::SourceMotion(MotionFacts::Spatial(reply)) = facts else {
        panic!("expected spatial fact")
    };
    reply
}

#[test]
fn primary_policy_requires_valid_selected_source_owners() {
    assert!(PrimaryNearPointPolicy::from_profile(&profile()).is_ok());
    assert_eq!(
        PrimaryNearPointPolicy::from_profile(&source::profile()),
        Err(Failure::UnsupportedQuery)
    );
    let mut p = profile();
    p.creature_capabilities = None;
    p.source_motion = None;
    assert_eq!(
        PrimaryNearPointPolicy::from_profile(&p),
        Err(Failure::UnsupportedQuery)
    );
    p = profile();
    p.tick_ms = 0;
    assert_eq!(
        PrimaryNearPointPolicy::from_profile(&p),
        Err(Failure::Binding)
    );
    p = profile();
    p.source_motion.as_mut().unwrap().player_collision_height = ai::Real::ZERO;
    assert_eq!(
        PrimaryNearPointPolicy::from_profile(&p),
        Err(Failure::UnresolvedBody)
    );
}

#[test]
fn primary_policy_must_match_session_identity_and_actual_bodies() {
    let (session, request) = setup();
    assert!(session.validate());
    assert!(session.body(&request).is_ok());
    for field in 0..10 {
        let mut changed = session.clone();
        match field {
            0 => changed.profile = [99; 32],
            1 => changed.policy = [99; 32],
            2 => changed.content = [99; 32],
            3 => changed.geometry = [99; 32],
            4 => changed.map += 1,
            5 => changed.player.as_mut().unwrap().body.radius = ai::Real::ONE,
            6 => changed.player.as_mut().unwrap().body.height = ai::Real::ONE,
            7 => changed.bodies.values_mut().next().unwrap().radius = ai::Real::ONE,
            8 => changed.bodies.values_mut().next().unwrap().height = ai::Real::ONE,
            _ => changed.bodies.clear(),
        }
        changed.participants = Some(NonRenderParticipants::authored_fixture(
            changed.content,
            changed.map,
        ));
        assert!(!changed.validate(), "field {field}");
        assert!(matches!(mailbox(changed), Err(Failure::Invalid)));
    }
    let mut changed = session;
    changed.participants = None;
    assert_eq!(changed.body(&request), Err(Failure::UnprovenParticipants));
    assert!(matches!(
        mailbox(changed),
        Err(Failure::UnprovenParticipants)
    ));
}

#[test]
fn primary_route_is_explicit_and_never_admits_bare_spatial_queries() {
    let (mut session, request) = setup();
    session.primary_near_point = None;
    let (mut authority, service) = mailbox(session).unwrap();
    assert!(matches!(
        authority.resolve(&request),
        Err(NativeError::AuthorityUnavailable(_))
    ));
    assert!(service.rx.is_empty());
    let (session, mut request) = setup();
    let q = inner(&request).clone();
    request.query = Query::SourceMotion(MotionQuery::Spatial(motion::Query::NearPoint(q)));
    let (mut authority, service) = mailbox(session).unwrap();
    assert!(matches!(
        authority.resolve(&request),
        Err(NativeError::AuthorityUnavailable(_))
    ));
    assert!(service.rx.is_empty());
}

#[test]
fn primary_request_checks_outer_and_inner_identity_clock_and_dimensions() {
    let (session, request) = setup();
    for field in 0..16 {
        let mut changed = request.clone();
        match field {
            0 => changed.binding.world = [99; 16],
            1 => changed.binding.profile = [99; 32],
            2 => changed.binding.authority_policy = [99; 32],
            3 => changed.binding.geometry = [99; 32],
            4 => changed.binding.tick = 0,
            5 => changed.binding.npc = None,
            6 => changed.binding.npc.as_mut().unwrap().spawn += 1,
            7 => changed.binding.npc.as_mut().unwrap().generation += 1,
            8 => inner_mut(&mut changed).binding.identity.creature += 1,
            9 => inner_mut(&mut changed).binding.identity.map += 1,
            10 => inner_mut(&mut changed).binding.identity.content = Revision([99; 32]),
            11 => inner_mut(&mut changed).binding.identity.geometry = Revision([99; 32]),
            12 => inner_mut(&mut changed).binding.query_id = 0,
            13 => inner_mut(&mut changed).binding.now_ms += 1,
            14 => changed.binding.tick += 1,
            _ => inner_mut(&mut changed).target.id += 1,
        }
        assert_eq!(
            session.body(&changed),
            Err(Failure::Binding),
            "field {field}"
        );
    }
    for field in 0..4 {
        let mut changed = request.clone();
        let q = inner_mut(&mut changed);
        match field {
            0 => q.owner_bounding_radius = ai::Real::new(0.125).unwrap(),
            1 => q.target.bounding_radius = ai::Real::new(0.125).unwrap(),
            2 => q.target.collision_height = ai::Real::new(0.125).unwrap(),
            _ => q.target.combat_reach = ai::Real::new(0.125).unwrap(),
        }
        assert_eq!(session.body(&changed), Err(Failure::UnresolvedBody));
    }
    let mut changed = request.clone();
    changed.binding.tick = u64::MAX;
    assert_eq!(session.body(&changed), Err(Failure::Invalid));
    inner_mut(&mut changed).distance = ai::Real::new(-1.).unwrap();
    changed.binding.tick = request.binding.tick;
    assert_eq!(session.body(&changed), Err(Failure::Invalid));
}

#[test]
fn primary_facts_require_exact_inner_echo_kind_and_persistent_point() {
    let (session, request) = setup();
    let correct = facts(&request);
    assert_eq!(validate_facts(&session, &request, &correct), Ok(()));
    for field in 0..6 {
        let mut wrong = correct.clone();
        let reply = spatial_mut(&mut wrong);
        let motion::Query::NearPoint(q) = &mut reply.request else {
            unreachable!()
        };
        match field {
            0 => q.binding.query_id += 1,
            1 => q.owner.position.x = ai::Real::new(100.).unwrap(),
            2 => q.target.pose.position.y = ai::Real::new(100.).unwrap(),
            3 => q.distance = ai::Real::new(100.).unwrap(),
            4 => q.angle = motion::NearPointAngle::Absolute(ai::Real::ONE),
            _ => q.target.id += 1,
        }
        assert_eq!(
            validate_facts(&session, &request, &wrong),
            Err(Failure::Binding)
        );
    }
    for fact in [
        motion::Fact::Missing,
        motion::Fact::ObjectPosition(Some(ai::Point::new(1., 2., 3.).unwrap())),
        motion::Fact::Route(motion::RouteOutcome::NoPath),
        motion::Fact::Bearing(ai::Real::ONE),
        motion::Fact::Collider(None),
        motion::Fact::NearPoint(ai::Point {
            x: ai::Real::new(32761.).unwrap(),
            y: ai::Real::ZERO,
            z: ai::Real::ZERO,
        }),
    ] {
        let mut wrong = correct.clone();
        spatial_mut(&mut wrong).fact = fact;
        assert_eq!(
            validate_facts(&session, &request, &wrong),
            Err(Failure::Binding)
        );
    }
    let mut wrong = correct;
    spatial_mut(&mut wrong).request = motion::Query::FacingTarget(motion::FacingTargetRequest {
        binding: inner(&request).binding,
        target: SOLO_UNIT,
    });
    assert_eq!(
        validate_facts(&session, &request, &wrong),
        Err(Failure::Binding)
    );
    assert_eq!(
        validate_facts(
            &session,
            &request,
            &Facts::PlayerMovement { accepted: true }
        ),
        Err(Failure::Binding)
    );
}

#[test]
fn primary_replies_retain_ticket_outer_binding_and_replacement_checks() {
    let (session, request) = setup();
    let (mut authority, service) = mailbox(session).unwrap();
    authority.ticket = 1;
    let reply = || Reply {
        #[cfg(all(test, not(benilla_headless_geometry)))]
        resident: None,
        ticket: 1,
        request: request.clone(),
        result: Ok(facts(&request)),
    };
    for field in 0..5 {
        let mut wrong = reply();
        match field {
            0 => wrong.ticket += 1,
            1 => wrong.request.binding.npc.as_mut().unwrap().generation += 1,
            2 => wrong.request.binding.tick += 1,
            3 => inner_mut(&mut wrong.request).owner.position.x = ai::Real::ONE,
            _ => wrong.result = Ok(Facts::PlayerMovement { accepted: true }),
        }
        assert_eq!(
            authority.finish_reply(&request, wrong),
            Err(NativeError::AuthorityBinding)
        );
    }
    assert!(authority.finish_reply(&request, reply()).is_ok());
    drop(service);
    assert!(matches!(
        authority.finish_reply(&request, reply()),
        Err(NativeError::AuthorityUnavailable(_))
    ));
}

#[test]
fn primary_queue_cancellation_expiry_and_frame_limit_are_unchanged() {
    let (session, request) = setup();
    let (mut authority, service) = mailbox(session).unwrap();
    let (reply, receive) = bounded(1);
    for ticket in 0..QUEUE_CAPACITY {
        authority
            .tx
            .try_send(Work::new(
                ticket as u64,
                request.clone(),
                Instant::now() - WAIT,
                reply.clone(),
                service.active.clone(),
            ))
            .unwrap();
    }
    drop(reply);
    assert!(matches!(
        authority.resolve(&request),
        Err(NativeError::AuthorityUnavailable(_))
    ));
    assert_eq!(
        service.serve_queries_until(usize::MAX, |_, _, _, _| panic!("expired request")),
        PER_FRAME
    );
    assert_eq!(service.rx.len(), QUEUE_CAPACITY - PER_FRAME);
    drop(service);
    assert!(matches!(
        receive.try_recv(),
        Err(crossbeam_channel::TryRecvError::Disconnected)
    ));
    assert!(authority.resolve(&request).is_err());
}
