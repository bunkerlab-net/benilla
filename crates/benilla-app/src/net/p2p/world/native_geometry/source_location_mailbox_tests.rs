// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual loaded location product facts through the exact bounded mailbox.
use super::super::source_location::tests::{deadline, setup};
use super::*;
use benilla_world_runtime::NpcId;

fn flip_byte(value: &mut serde_json::Value) {
    *value = serde_json::json!(value.as_u64().unwrap() ^ 1);
}

#[test]
fn loaded_location_mailbox_retains_exact_query_and_original_deadline() {
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
        assert!(Instant::now() < original, "original location job expired");
        assert_eq!(
            service.serve_queries_until(1, |session, observed, body, until| {
                assert_eq!(observed, &request);
                assert!(until <= original);
                assert_eq!(Some(body), session.player.map(|player| player.body));
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
                assert!(Instant::now() < original, "location reply arrived too late");
                break authority.finish_reply(&request, reply).unwrap();
            }
            Err(crossbeam_channel::TryRecvError::Empty) => {
                // Drive the same queued Work on its next slice; never block
                // waiting for a reply while the only service driver is idle.
                deferred = true;
                std::thread::yield_now();
            }
            Err(error) => panic!("location job terminated without a reply: {error:?}"),
        }
    };
    assert!(deferred, "the forced expired slice must retain the job");
    assert_eq!(reply.request, request);
    let Facts::SourceLocation(facts) = reply.facts else {
        panic!()
    };
    assert_eq!((facts.zone_id(), facts.area_id()), (12, 9));
}

#[test]
fn unselected_location_requests_never_enter_mailbox() {
    let (_, mut session, request, _) = setup();
    session.source_location = None;
    let (mut authority, service) = mailbox(session).unwrap();
    assert!(matches!(
        authority.resolve(&request),
        Err(NativeError::AuthorityUnavailable(_))
    ));
    assert!(service.rx.is_empty());
}

#[test]
fn location_session_requires_exact_profile_map_and_player_body() {
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
fn location_reply_ticket_outer_query_kind_and_session_replacement_are_checked() {
    let (_, session, request, provider) = setup();
    let facts = provider.resolve(&session, &request, deadline()).unwrap();
    assert_eq!(validate_facts(&session, &request, &facts), Ok(()));
    let (mut authority, service) = mailbox(session).unwrap();
    authority.ticket = 1;
    let reply = || Reply {
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
fn location_expired_jobs_are_not_queried_or_delivered_after_original_deadline() {
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
fn forged_location_rows_flags_query_echoes_and_missing_option_are_rejected() {
    let (_, session, request, provider) = setup();
    let facts = provider.resolve(&session, &request, deadline()).unwrap();
    for field in 0..6 {
        let mut value = serde_json::to_value(&facts).unwrap();
        let location = &mut value["SourceLocation"];
        match field {
            0 => location["area"]["id"] = serde_json::json!(12),
            1 => location["area"]["zone"] = serde_json::json!(9),
            2 => location["area"] = serde_json::Value::Null,
            3 => location["area_flag"] = serde_json::json!(0),
            4 => location["query"]["ticket"] = serde_json::json!(0),
            _ => flip_byte(&mut location["query"]["snapshot"]["policy"][0]),
        }
        let forged: Facts = serde_json::from_value(value).unwrap();
        assert_eq!(
            validate_facts(&session, &request, &forged),
            Err(Failure::Binding),
            "field {field}"
        );
    }
    let mut missing = serde_json::to_value(&facts).unwrap();
    missing["SourceLocation"]
        .as_object_mut()
        .unwrap()
        .remove("area");
    assert!(serde_json::from_value::<Facts>(missing).is_err());
    let mut unknown = serde_json::to_value(&facts).unwrap();
    unknown["SourceLocation"]["allowed"] = serde_json::json!(true);
    assert!(serde_json::from_value::<Facts>(unknown).is_err());
}

#[test]
fn forged_location_query_owner_source_and_snapshot_never_reach_reader() {
    let (_, session, request, provider) = setup();
    for field in 0..12 {
        let mut value = serde_json::to_value(&request).unwrap();
        let query = &mut value["query"]["SourceLocation"];
        match field {
            0 => flip_byte(&mut query["world"][0]),
            1 => flip_byte(&mut query["player"]["owner"][0]),
            2 => flip_byte(&mut query["player"]["character"][0]),
            3 => flip_byte(&mut query["profile"][0]),
            4 => flip_byte(&mut query["source"][0]),
            5 => query["ticket"] = serde_json::json!(0),
            6 => query["snapshot"]["map"] = serde_json::json!(77),
            7 => flip_byte(&mut query["snapshot"]["content"][0]),
            8 => flip_byte(&mut query["snapshot"]["source_epoch"][0]),
            9 => flip_byte(&mut query["snapshot"]["inventory"][0]),
            10 => flip_byte(&mut query["snapshot"]["catalog"][0]),
            _ => flip_byte(&mut query["spell_area_scope"][0]),
        }
        let changed: AuthorityRequest = serde_json::from_value(value).unwrap();
        assert_eq!(
            provider.resolve(&session, &changed, deadline()),
            Err(Failure::Binding),
            "field {field}"
        );
    }
}
