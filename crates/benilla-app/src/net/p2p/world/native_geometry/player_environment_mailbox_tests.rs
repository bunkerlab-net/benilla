// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual loaded environment product facts through the exact bounded mailbox.
use super::super::source_player_environment::tests::{deadline, setup};
use super::*;
use benilla_world_runtime::{native::environment::LiquidStatus, NpcId};

#[test]
fn loaded_environment_mailbox_retains_exact_query_and_original_deadline() {
    let (_, session, request, provider) = setup();
    let (mut authority, service) = mailbox(session).unwrap();
    authority.ticket = 1;
    let original = deadline();
    let (reply, receive) = bounded(1);
    authority
        .tx
        .try_send(Work::new(
            1,
            request.clone(),
            original,
            reply,
            service.active.clone(),
        ))
        .unwrap();
    let mut force_slice_expiry = true;
    let mut deferred = false;
    let reply = loop {
        assert!(
            Instant::now() < original,
            "original environment job expired"
        );
        assert_eq!(
            service.serve_queries_until(1, |session, observed, body, until| {
                assert_eq!(observed, &request);
                assert!(until <= original);
                assert_eq!(body.height.bits(), 2f32.to_bits());
                if force_slice_expiry {
                    force_slice_expiry = false;
                    std::thread::sleep(
                        until.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
                    );
                }
                provider.resolve(session, observed, until)
            }),
            1
        );
        match receive.try_recv() {
            Ok(reply) => {
                assert!(
                    Instant::now() < original,
                    "environment reply arrived too late"
                );
                break authority.finish_reply(&request, reply).unwrap();
            }
            Err(crossbeam_channel::TryRecvError::Empty) => {
                // Drive the same queued Work on its next slice; never block
                // waiting for a reply while the only service driver is idle.
                deferred = true;
                std::thread::yield_now();
            }
            Err(error) => panic!("environment job terminated without a reply: {error:?}"),
        }
    };
    assert!(deferred, "the forced expired slice must retain the job");
    assert_eq!(reply.request, request);
    let Facts::PlayerEnvironment(facts) = reply.facts else {
        panic!()
    };
    assert_eq!(facts.result().status, LiquidStatus::AboveWater);
}

#[test]
fn unselected_environment_requests_never_enter_mailbox() {
    let (_, mut session, request, _) = setup();
    session.player_environment = None;
    let (mut authority, service) = mailbox(session).unwrap();
    assert!(matches!(
        authority.resolve(&request),
        Err(NativeError::AuthorityUnavailable(_))
    ));
    assert!(service.rx.is_empty());
}

#[test]
fn environment_session_requires_exact_profile_map_and_player_body() {
    let (_, session, request, _) = setup();
    for field in 0..8 {
        let mut changed = session.clone();
        match field {
            0 => changed.profile[0] ^= 1,
            1 => changed.policy[0] ^= 1,
            2 => changed.content[0] ^= 1,
            3 => changed.geometry[0] ^= 1,
            4 => changed.map += 1,
            5 => changed.player.as_mut().unwrap().body.height = ai::Real::ONE,
            6 => changed.player.as_mut().unwrap().body.radius = ai::Real::ONE,
            _ => changed.player = None,
        }
        changed.participants = Some(NonRenderParticipants::authored_fixture(
            changed.content,
            changed.map,
        ));
        assert!(!changed.validate(), "field {field}");
        assert!(changed.body(&request).is_err());
        assert!(matches!(mailbox(changed), Err(Failure::Invalid)));
    }
    for field in 0..6 {
        let mut changed = request.clone();
        match field {
            0 => changed.binding.world[0] ^= 1,
            1 => changed.binding.profile[0] ^= 1,
            2 => changed.binding.authority_policy[0] ^= 1,
            3 => changed.binding.geometry[0] ^= 1,
            4 => changed.binding.tick = 0,
            _ => {
                changed.binding.npc = Some(NpcId {
                    spawn: 2,
                    generation: 0,
                })
            }
        }
        assert_eq!(session.body(&changed), Err(Failure::Binding));
    }
}

#[test]
fn environment_reply_ticket_outer_query_kind_and_session_replacement_are_checked() {
    let (_, session, request, provider) = setup();
    let facts = provider.resolve(&session, &request, deadline()).unwrap();
    assert_eq!(validate_facts(&session, &request, &facts), Ok(()));
    let (mut authority, service) = mailbox(session).unwrap();
    authority.ticket = 1;
    let reply = || Reply {
        #[cfg(all(test, not(benilla_headless_geometry)))]
        resident: None,
        ticket: 1,
        request: request.clone(),
        result: Ok(facts.clone()),
    };
    for field in 0..4 {
        let mut forged = reply();
        match field {
            0 => forged.ticket += 1,
            1 => forged.request.binding.tick += 1,
            2 => forged.request.binding.world[0] ^= 1,
            _ => forged.result = Ok(Facts::PlayerMovement { accepted: true }),
        }
        assert_eq!(
            authority.finish_reply(&request, forged),
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
fn environment_expired_jobs_are_not_queried_or_delivered_after_original_deadline() {
    let (_, session, request, provider) = setup();
    let facts = provider.resolve(&session, &request, deadline()).unwrap();
    let (authority, service) = mailbox(session).unwrap();
    let (reply, receive) = bounded(1);
    authority
        .tx
        .try_send(Work::new(
            1,
            request.clone(),
            Instant::now() - WAIT,
            reply,
            service.active.clone(),
        ))
        .unwrap();
    assert_eq!(
        service.serve_queries_until(1, |_, _, _, _| panic!("expired request queried")),
        1
    );
    assert!(matches!(
        receive.try_recv(),
        Err(crossbeam_channel::TryRecvError::Disconnected)
    ));
    let (reply, receive) = bounded(1);
    let original = Instant::now() + Duration::from_millis(10);
    authority
        .tx
        .try_send(Work::new(
            2,
            request,
            original,
            reply,
            service.active.clone(),
        ))
        .unwrap();
    assert_eq!(
        service.serve_queries_until(1, |_, _, _, until| {
            assert!(until <= original);
            std::thread::sleep(
                original.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
            );
            Ok(facts.clone())
        }),
        1
    );
    assert!(matches!(
        receive.try_recv(),
        Err(crossbeam_channel::TryRecvError::Disconnected)
    ));
}

#[test]
fn forged_height_owner_snapshot_or_pose_fields_are_rejected() {
    let (_, session, request, provider) = setup();
    for field in [
        "native_display_id",
        "model_id",
        "stored_object_scale",
        "effective_object_scale",
        "raw_height",
        "height",
        "source_binding",
        "content",
        "profile",
        "geometry",
        "map",
        "world",
        "player",
    ] {
        let mut value = serde_json::to_value(&request).unwrap();
        let height = &mut value["query"]["PlayerEnvironment"]["owner"]["height"];
        match field {
            "source_binding" | "content" | "profile" | "geometry" | "world" => {
                height[field][0] = 99.into()
            }
            "player" => height[field]["owner"][0] = 99.into(),
            "stored_object_scale" | "effective_object_scale" | "raw_height" | "height" => {
                height[field] = 3f32.to_bits().into()
            }
            _ => height[field] = 999.into(),
        }
        let forged: AuthorityRequest = serde_json::from_value(value).unwrap();
        assert_eq!(
            session.body(&forged),
            Err(Failure::Binding),
            "height {field}"
        );
        assert_eq!(
            provider.resolve(&session, &forged, deadline()),
            Err(Failure::Binding),
            "height {field}"
        );
    }
    for field in 0..4 {
        let mut value = serde_json::to_value(&request).unwrap();
        let query = &mut value["query"]["PlayerEnvironment"];
        match field {
            0 => query["sequence"] = 0.into(),
            1 => query["query"]["ticket"] = 0.into(),
            2 => query["before"] = serde_json::to_value([0u8; 32]).unwrap(),
            _ => query["owner"]["height"]["player"]["character"][0] = 99.into(),
        }
        let forged: AuthorityRequest = serde_json::from_value(value).unwrap();
        assert_eq!(
            session.body(&forged),
            Err(Failure::Binding),
            "phase/owner {field}"
        );
    }
    for field in [
        "source",
        "map",
        "content",
        "source_epoch",
        "inventory",
        "policy",
        "liquid_catalog",
        "collision_height",
        "required_type",
        "position",
        "pose",
    ] {
        let mut value = serde_json::to_value(&request).unwrap();
        let query = &mut value["query"]["PlayerEnvironment"];
        match field {
            "source" => query["owner"]["source"][0] = 99.into(),
            "map" => query["query"]["snapshot"][field] = 999.into(),
            "content" | "source_epoch" | "inventory" | "policy" | "liquid_catalog" => {
                query["query"]["snapshot"][field][0] = 99.into()
            }
            "collision_height" => query["query"][field] = 3f32.to_bits().into(),
            "required_type" => query["query"][field] = 0.into(),
            "position" => query["query"][field][0] = (-1f32).to_bits().into(),
            _ => query["pose"]["position"]["x"] = (-1f32).to_bits().into(),
        }
        let forged: AuthorityRequest = serde_json::from_value(value).unwrap();
        assert_eq!(
            session.body(&forged),
            Err(Failure::Binding),
            "query {field}"
        );
        assert_eq!(
            provider.resolve(&session, &forged, deadline()),
            Err(Failure::Binding),
            "query {field}"
        );
    }
}

#[test]
fn full_environment_reply_echo_includes_phase_before_orientation_and_liquid_query() {
    let (_, session, request, provider) = setup();
    let facts = provider.resolve(&session, &request, deadline()).unwrap();
    for field in 0..8 {
        let mut value = serde_json::to_value(&facts).unwrap();
        let facts = &mut value["PlayerEnvironment"];
        match field {
            0 => facts["query"]["sequence"] = 99.into(),
            1 => facts["query"]["before"][0] = 99.into(),
            2 => facts["query"]["pose"]["orientation"] = 1f32.to_bits().into(),
            3 => facts["result"]["query"]["ticket"] = 99.into(),
            4 => facts["result"]["query"]["position"][0] = (-1f32).to_bits().into(),
            5 => facts["result"]["query"]["collision_height"] = 3f32.to_bits().into(),
            6 => facts["result"]["query"]["required_type"] = 0.into(),
            _ => facts["result"]["query"]["snapshot"]["source_epoch"][0] = 99.into(),
        }
        let forged: Facts = serde_json::from_value(value).unwrap();
        assert_eq!(
            validate_facts(&session, &request, &forged),
            Err(Failure::Binding),
            "field {field}"
        );
    }
}
