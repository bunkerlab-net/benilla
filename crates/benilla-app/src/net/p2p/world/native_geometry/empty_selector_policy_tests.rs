// SPDX-License-Identifier: GPL-3.0-or-later
//! Route-selection envelope tests; authored replies do not establish source parity.
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

fn setup() -> (Profile, Session, AuthorityRequest) {
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
                height: source.npc_policy.settings.collision_height,
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
        primary_near_point: None,
        empty_selector_near_point: None,
        player_environment: None,
    };
    let mut world = NativeWorld::synthetic_component_test(
        profile.clone(),
        session.world,
        fixture::player(),
        1234,
    )
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
    (profile, session, request)
}

fn inner(request: &mut AuthorityRequest) -> &mut motion::NearPointRequest {
    let Query::SourceMotion(MotionQuery::NearPoint(near)) = &mut request.query else {
        panic!("expected selected near-point request")
    };
    &mut near.request
}

fn facts(request: &AuthorityRequest) -> Facts {
    let Query::SourceMotion(MotionQuery::NearPoint(near)) = &request.query else {
        panic!("expected selected near-point request")
    };
    Facts::SourceMotion(MotionFacts::Spatial(Box::new(motion::Reply {
        request: motion::Query::NearPoint(near.request.clone()),
        fact: motion::Fact::NearPoint(ai::Point::new(1., 2., 3.).unwrap()),
    })))
}

#[test]
fn empty_selector_policy_uses_the_validated_source_owner_contract() {
    assert_eq!(
        EmptySelectorNearPointPolicy::from_profile(&profile())
            .unwrap()
            .0,
        PrimaryNearPointPolicy::from_profile(&profile()).unwrap()
    );
    assert_eq!(
        EmptySelectorNearPointPolicy::from_profile(&source::profile()),
        Err(Failure::UnsupportedQuery)
    );
    let mut invalid = profile();
    invalid.tick_ms = 0;
    assert_eq!(
        EmptySelectorNearPointPolicy::from_profile(&invalid),
        Err(Failure::Binding)
    );
    let mut unresolved = profile();
    unresolved
        .source_motion
        .as_mut()
        .unwrap()
        .player_collision_height = ai::Real::ZERO;
    assert_eq!(
        EmptySelectorNearPointPolicy::from_profile(&unresolved),
        Err(Failure::UnresolvedBody)
    );
}

#[test]
fn near_point_mailbox_requires_exactly_one_selected_route() {
    let (profile, session, request) = setup();
    assert!(session.validate());
    assert_eq!(session.body(&request), Err(Failure::UnsupportedQuery));
    let (mut ordinary, ordinary_service) = mailbox(session.clone()).unwrap();
    assert!(matches!(
        ordinary.resolve(&request),
        Err(NativeError::AuthorityUnavailable(_))
    ));
    assert!(ordinary_service.rx.is_empty());

    for empty_selector in [false, true] {
        let mut selected = session.clone();
        if empty_selector {
            selected.empty_selector_near_point =
                Some(EmptySelectorNearPointPolicy::from_profile(&profile).unwrap());
        } else {
            selected.primary_near_point =
                Some(PrimaryNearPointPolicy::from_profile(&profile).unwrap());
        }
        assert!(selected.validate());
        let body = selected.body(&request).unwrap();
        let (mut authority, service) = mailbox(selected).unwrap();
        authority.ticket = 1;
        let (reply, receive) = bounded(1);
        authority
            .tx
            .try_send(Work {
                ticket: 1,
                request: request.clone(),
                deadline: Instant::now() + WAIT,
                reply,
            })
            .unwrap();
        assert_eq!(
            service.serve_queries_until(1, |session, actual, actual_body, _| {
                assert_eq!(session.empty_selector_near_point.is_some(), empty_selector);
                assert_eq!(session.primary_near_point.is_some(), !empty_selector);
                assert_eq!(actual, &request);
                assert_eq!(actual_body, body);
                Ok(facts(actual))
            }),
            1
        );
        assert_eq!(
            authority
                .finish_reply(&request, receive.try_recv().unwrap())
                .unwrap(),
            AuthorityReply {
                request: request.clone(),
                facts: facts(&request),
            }
        );
    }

    let mut conflicting = session;
    conflicting.primary_near_point = Some(PrimaryNearPointPolicy::from_profile(&profile).unwrap());
    conflicting.empty_selector_near_point =
        Some(EmptySelectorNearPointPolicy::from_profile(&profile).unwrap());
    assert!(!conflicting.validate());
    assert_eq!(conflicting.body(&request), Err(Failure::Invalid));
    assert!(matches!(mailbox(conflicting), Err(Failure::Invalid)));
}

#[test]
fn empty_selector_policy_rejects_malformed_sessions_and_retains_participant_proof() {
    let (profile, mut session, request) = setup();
    session.empty_selector_near_point =
        Some(EmptySelectorNearPointPolicy::from_profile(&profile).unwrap());
    for field in 0..3 {
        let mut changed = session.clone();
        match field {
            0 => changed.profile = [99; 32],
            1 => changed.player.as_mut().unwrap().body.height = ai::Real::ONE,
            _ => changed.bodies.clear(),
        }
        assert!(!changed.validate(), "field {field}");
        assert!(matches!(mailbox(changed), Err(Failure::Invalid)));
    }
    session.participants = None;
    assert_eq!(session.body(&request), Err(Failure::UnprovenParticipants));
    assert!(matches!(
        mailbox(session),
        Err(Failure::UnprovenParticipants)
    ));
}

#[test]
fn empty_selector_route_retains_shared_request_guards() {
    let (profile, mut primary, request) = setup();
    primary.primary_near_point = Some(PrimaryNearPointPolicy::from_profile(&profile).unwrap());
    let mut empty_selector = primary.clone();
    empty_selector.primary_near_point = None;
    empty_selector.empty_selector_near_point =
        Some(EmptySelectorNearPointPolicy::from_profile(&profile).unwrap());
    for field in 0..5 {
        let mut changed = request.clone();
        let expected = match field {
            0 => {
                changed.binding.npc.as_mut().unwrap().generation += 1;
                Failure::Binding
            }
            1 => {
                inner(&mut changed).binding.now_ms += 1;
                Failure::Binding
            }
            2 => {
                inner(&mut changed).target.collision_height = ai::Real::ONE;
                Failure::UnresolvedBody
            }
            3 => {
                changed.binding.tick = u64::MAX;
                Failure::Invalid
            }
            _ => {
                let near = inner(&mut changed).clone();
                changed.query =
                    Query::SourceMotion(MotionQuery::Spatial(motion::Query::NearPoint(near)));
                Failure::UnsupportedQuery
            }
        };
        assert_eq!(primary.body(&changed), Err(expected), "field {field}");
        assert_eq!(
            empty_selector.body(&changed),
            Err(expected),
            "field {field}"
        );
    }
}

#[test]
fn empty_selector_replies_preserve_exact_echo_and_cancellation() {
    let (profile, mut session, request) = setup();
    session.empty_selector_near_point =
        Some(EmptySelectorNearPointPolicy::from_profile(&profile).unwrap());
    let (mut authority, service) = mailbox(session).unwrap();
    authority.ticket = 1;
    let reply = || Reply {
        ticket: 1,
        request: request.clone(),
        result: Ok(facts(&request)),
    };
    for field in 0..4 {
        let mut wrong = reply();
        match field {
            0 => wrong.ticket += 1,
            1 => wrong.request.binding.tick += 1,
            2 => {
                let mut altered = request.clone();
                inner(&mut altered).binding.query_id += 1;
                wrong.result = Ok(facts(&altered));
            }
            _ => wrong.result = Ok(Facts::PlayerMovement { accepted: true }),
        }
        assert_eq!(
            authority.finish_reply(&request, wrong),
            Err(NativeError::AuthorityBinding),
            "field {field}"
        );
    }
    assert!(authority.finish_reply(&request, reply()).is_ok());
    drop(service);
    assert!(matches!(
        authority.finish_reply(&request, reply()),
        Err(NativeError::AuthorityUnavailable(_))
    ));
    assert!(matches!(
        authority.resolve(&request),
        Err(NativeError::AuthorityUnavailable(_))
    ));
}
