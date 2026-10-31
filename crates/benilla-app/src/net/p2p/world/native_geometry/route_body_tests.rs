// SPDX-License-Identifier: GPL-3.0-or-later
//! Immutable Route-body continuations, with a deterministic slice clock. The
//! original Work lease always uses the real absolute deadline and attachment.
use super::super::source_route_same_polygon::tests::captured_route_request;
use super::*;
use std::cell::Cell;

fn work(service: &Mailbox, request: AuthorityRequest) -> (Work, Receiver<Reply>) {
    let (reply, receive) = bounded(1);
    (
        Work::new(
            43,
            request,
            Instant::now() + WAIT,
            reply,
            service.active.clone(),
        ),
        receive,
    )
}

fn pause_after(service: &Mailbox, work: &mut Work, phase: RouteBodyPhase) {
    let start = Instant::now();
    let slice = start + Duration::from_millis(2);
    let now = Cell::new(start);
    assert_eq!(
        service.advance_route_body(
            work,
            slice,
            || now.get(),
            |completed, saved| {
                if completed == phase && saved {
                    now.set(slice);
                }
            }
        ),
        Err(Failure::Deadline)
    );
    assert!(work.route_validation.is_none());
    assert_eq!(
        work.route_body.as_ref().unwrap().next,
        match phase {
            RouteBodyPhase::Context => RouteBodyPhase::ContextWidth,
            RouteBodyPhase::ContextWidth => RouteBodyPhase::ContextSelection,
            RouteBodyPhase::ContextSelection => RouteBodyPhase::ContextHeight,
            RouteBodyPhase::ContextHeight => RouteBodyPhase::ContextBody,
            RouteBodyPhase::ContextBody => RouteBodyPhase::Dispatch,
            RouteBodyPhase::Dispatch => RouteBodyPhase::Model,
            RouteBodyPhase::Model => RouteBodyPhase::OwnerState,
            RouteBodyPhase::OwnerState => panic!("final body is not partial"),
        }
    );
}

/// Instrument the exact loaded quest request used by the service regression.
/// Real clock/deadline checks remain active; this never supplies geometry facts.
pub(crate) fn assert_loaded_quest_body_real_slice_progress(
    session: &Session,
    request: &AuthorityRequest,
) {
    let direct = session.body(request).unwrap();
    let (_, service) = mailbox(session.clone()).unwrap();
    let (mut work, reply) = work(&service, request.clone());
    let lease = work.lease();
    let expiry = work.deadline;
    let mut attempts = 0;
    let mut samples = Vec::with_capacity(8);
    loop {
        attempts += 1;
        let start = Instant::now();
        let last_check = Cell::new(start);
        let result = service.advance_route_body(
            &mut work,
            start + Duration::from_millis(2),
            || {
                let now = Instant::now();
                last_check.set(now);
                now
            },
            |phase, saved| {
                if saved {
                    let sample: &mut (RouteBodyPhase, Duration, bool) = samples.last_mut().unwrap();
                    assert_eq!(sample.0, phase);
                    sample.2 = true;
                } else {
                    samples.push((phase, last_check.get().elapsed(), false));
                }
            },
        );
        assert!(work.lease().same_job(&lease));
        assert_eq!(work.deadline, expiry);
        assert!(reply.is_empty());
        match result {
            Ok(()) => break,
            Err(Failure::Deadline) if lease.is_active() => {
                assert!(work.route_validation.is_none());
            }
            other => panic!(
                "loaded quest body failed under real 2ms/500ms budgets: {other:?}; \
                 attempts={attempts}; samples={samples:?}"
            ),
        }
    }
    assert_eq!(
        samples
            .iter()
            .filter_map(|(phase, _, saved)| saved.then_some(*phase))
            .collect::<Vec<_>>(),
        [
            RouteBodyPhase::Context,
            RouteBodyPhase::ContextWidth,
            RouteBodyPhase::ContextSelection,
            RouteBodyPhase::ContextHeight,
            RouteBodyPhase::ContextBody,
            RouteBodyPhase::Dispatch,
            RouteBodyPhase::Model,
            RouteBodyPhase::OwnerState,
        ]
    );
    assert_eq!(work.route_validation.as_ref().unwrap().body, direct);
    assert!(work.route_body.is_none());
    // Log outside every timed slice, without publishing a provider result.
    eprintln!("loaded quest real body subparts: attempts={attempts}; samples={samples:?}");
}

#[test]
fn context_subparts_make_progress_when_only_their_sum_exceeds_the_slice() {
    let (session, request) = captured_route_request();
    let (_, service) = mailbox(session.clone()).unwrap();
    let (mut work, reply) = work(&service, request.clone());
    let lease = work.lease();
    let expiry = work.deadline;
    assert_eq!(WAIT, Duration::from_millis(500));
    let start = Instant::now();
    let slice = start + Duration::from_millis(2);
    let now = Cell::new(start);
    let mut attempts = vec![];
    assert_eq!(
        service.advance_route_body(
            &mut work,
            slice,
            || now.get(),
            |phase, saved| {
                attempts.push((phase, saved));
                if !saved {
                    // Each part fits, but the old indivisible Context did not.
                    now.set(now.get() + Duration::from_micros(1100));
                }
            },
        ),
        Err(Failure::Deadline)
    );
    assert_eq!(
        attempts,
        [
            (RouteBodyPhase::Context, false),
            (RouteBodyPhase::Context, true),
            (RouteBodyPhase::ContextWidth, false),
        ]
    );
    let progress = work.route_body.as_ref().unwrap();
    assert_eq!(progress.next, RouteBodyPhase::ContextWidth);
    assert!(progress.model.is_none());
    assert!(work.route_validation.is_none());
    assert!(reply.is_empty());
    assert!(work.lease().same_job(&lease));
    assert_eq!(work.deadline, expiry);

    attempts.clear();
    now.set(start);
    service
        .advance_route_body(
            &mut work,
            slice,
            || now.get(),
            |phase, saved| {
                attempts.push((phase, saved));
                if !saved {
                    now.set(
                        now.get()
                            + Duration::from_micros(if phase == RouteBodyPhase::ContextWidth {
                                1100
                            } else {
                                100
                            }),
                    );
                }
            },
        )
        .unwrap();
    assert_eq!(
        attempts,
        [
            (RouteBodyPhase::ContextWidth, false),
            (RouteBodyPhase::ContextWidth, true),
            (RouteBodyPhase::ContextSelection, false),
            (RouteBodyPhase::ContextSelection, true),
            (RouteBodyPhase::ContextHeight, false),
            (RouteBodyPhase::ContextHeight, true),
            (RouteBodyPhase::ContextBody, false),
            (RouteBodyPhase::ContextBody, true),
            (RouteBodyPhase::Dispatch, false),
            (RouteBodyPhase::Dispatch, true),
            (RouteBodyPhase::Model, false),
            (RouteBodyPhase::Model, true),
            (RouteBodyPhase::OwnerState, false),
            (RouteBodyPhase::OwnerState, true),
        ]
    );
    assert!(work.route_body.is_none());
    let validation = work.route_validation.as_ref().unwrap();
    assert_eq!(validation.body, session.body(&request).unwrap());
    assert!(validation.lease.same_job(&lease));
    assert_eq!(validation.lease.deadline(), expiry);
    // Body completion has neither invoked a provider nor published a reply.
    assert!(reply.is_empty());
    service.retry.try_send(work).unwrap();
    let mut calls = 0;
    service.serve_service_query_slice(1, true, Instant::now() + WAIT, |query, until| {
        calls += 1;
        let ServiceQuery::Route(job) = query else {
            panic!("lost Route staging")
        };
        assert!(job.lease().same_job(&lease));
        assert_eq!(until, expiry);
        Err(Failure::MissingGeometry)
    });
    assert_eq!(calls, 1);
    assert_eq!(
        reply.try_recv().unwrap().result,
        Err(Failure::MissingGeometry)
    );
    assert!(!lease.is_active());
}

#[test]
fn completed_subparts_survive_and_only_the_oversized_subpart_repeats() {
    let (session, request) = captured_route_request();
    let (_, service) = mailbox(session.clone()).unwrap();
    let (mut work, reply) = work(&service, request.clone());
    let lease = work.lease();
    let expiry = work.deadline;
    pause_after(&service, &mut work, RouteBodyPhase::Dispatch);
    assert!(reply.is_empty());
    let start = Instant::now();
    let slice = start + Duration::from_millis(2);
    let now = Cell::new(start);
    let mut attempted = vec![];
    assert_eq!(
        service.advance_route_body(
            &mut work,
            slice,
            || now.get(),
            |phase, saved| {
                attempted.push((phase, saved));
                // A completed computation that missed its slice cannot become a receipt.
                if phase == RouteBodyPhase::Model && !saved {
                    now.set(slice);
                }
            }
        ),
        Err(Failure::Deadline)
    );
    assert_eq!(attempted, [(RouteBodyPhase::Model, false)]);
    assert_eq!(
        work.route_body.as_ref().unwrap().next,
        RouteBodyPhase::Model
    );
    assert!(work.route_validation.is_none());
    assert!(work.lease().same_job(&lease));
    assert_eq!(work.deadline, expiry);
    assert!(reply.is_empty());
    let mut completed = vec![];
    service
        .advance_route_body(
            &mut work,
            slice,
            || start,
            |phase, saved| {
                if saved {
                    completed.push(phase);
                }
            },
        )
        .unwrap();
    assert_eq!(
        completed,
        [RouteBodyPhase::Model, RouteBodyPhase::OwnerState]
    );
    assert!(work.route_body.is_none());
    let validation = work.route_validation.as_ref().unwrap();
    assert_eq!(validation.body, session.body(&request).unwrap());
    assert!(validation.lease.same_job(&lease));
    assert_eq!(validation.lease.deadline(), expiry);
    service.retry.try_send(work).unwrap();
    service.serve_service_query_slice(1, true, Instant::now() + WAIT, |query, until| {
        let ServiceQuery::Route(job) = query else {
            panic!("lost Route staging")
        };
        assert!(job.lease().same_job(&lease));
        assert_eq!(job.lease().ticket(), 43);
        assert_eq!(until, expiry);
        // No source proposal or native fact was created by partial body validation.
        Err(Failure::MissingGeometry)
    });
    assert_eq!(
        reply.try_recv().unwrap().result,
        Err(Failure::MissingGeometry)
    );
    assert!(!lease.is_active());
}

#[test]
fn partial_body_rejects_changed_session_request_and_equal_byte_dispatch() {
    let (session, request) = captured_route_request();
    let (_, fresh_request) = captured_route_request();
    assert_eq!(fresh_request, request);
    for boundary in [
        RouteBodyPhase::Context,
        RouteBodyPhase::ContextWidth,
        RouteBodyPhase::ContextSelection,
        RouteBodyPhase::ContextHeight,
        RouteBodyPhase::ContextBody,
        RouteBodyPhase::Dispatch,
        RouteBodyPhase::Model,
    ] {
        for field in 0..18 {
            let (_, mut service) = mailbox(session.clone()).unwrap();
            // Prepare mutations before the real Work lease starts. JSON/deep-clone fixture
            // work is not provider work and must not spend the unchanged 500ms lease.
            let replacement_request = match field {
                8 => {
                    let roundtrip: AuthorityRequest =
                        serde_json::from_value(serde_json::to_value(&request).unwrap()).unwrap();
                    assert_eq!(roundtrip, request);
                    Some(roundtrip)
                }
                9 => Some(fresh_request.clone()),
                _ => None,
            };
            let replacement_profile = (field == 10).then(|| {
                let policy = service.session.player_environment.as_ref().unwrap();
                let mut profile = (*policy.profile).clone();
                profile.tick_ms += 1;
                Arc::new(profile)
            });
            let (mut work, reply) = work(&service, request.clone());
            let lease = work.lease();
            pause_after(&service, &mut work, boundary);
            let progress = work.route_body.as_mut().unwrap();
            match field {
                0 => service.session.world[0] ^= 1,
                1 => service.session.profile[0] ^= 1,
                2 => service.session.participants = None,
                3 => service.session.bodies.clear(),
                4 => service.session.same_polygon_route = None,
                5 => progress.request.binding.tick += 1,
                6 => progress.request.binding.npc.as_mut().unwrap().generation += 1,
                7 => {
                    let Query::SourceMotion(MotionQuery::Route(route)) =
                        &mut progress.request.query
                    else {
                        unreachable!()
                    };
                    route.request.goal.x = ai::Real::ONE;
                }
                8 | 9 => progress.request = replacement_request.unwrap(),
                10 => {
                    service.session.player_environment.as_mut().unwrap().profile =
                        replacement_profile.unwrap();
                }
                11 => service.session.geometry[0] ^= 1,
                12 => progress.request.binding.world[0] ^= 1,
                13 => progress.request.binding.npc.as_mut().unwrap().spawn += 1,
                14..=17 => {
                    let Query::SourceMotion(MotionQuery::Route(route)) =
                        &mut progress.request.query
                    else {
                        unreachable!()
                    };
                    match field {
                        14 => route.request.target.as_mut().unwrap().id += 1,
                        15 => {
                            route.request.target.as_mut().unwrap().pose.position.x = ai::Real::ONE
                        }
                        16 => {
                            route.request.target.as_mut().unwrap().collision_height = ai::Real::ONE
                        }
                        17 => route.target_player_height = None,
                        _ => unreachable!(),
                    }
                }
                _ => unreachable!(),
            }
            service.retry.try_send(work).unwrap();
            service.serve_service_query_slice(1, true, Instant::now() + WAIT, |_, _| {
                panic!("partial invalid body reached provider")
            });
            assert_eq!(
                reply.try_recv().unwrap().result,
                Err(Failure::Binding),
                "boundary={boundary:?} field={field}"
            );
            assert!(!lease.is_active());
            assert!(service.rx.is_empty());
        }
    }
}

#[test]
fn partial_body_cannot_move_to_equal_work_foreign_attachment_or_survive_cancellation() {
    let (session, request) = captured_route_request();
    for boundary in [
        RouteBodyPhase::Context,
        RouteBodyPhase::ContextWidth,
        RouteBodyPhase::ContextSelection,
        RouteBodyPhase::ContextHeight,
        RouteBodyPhase::ContextBody,
        RouteBodyPhase::Dispatch,
        RouteBodyPhase::Model,
    ] {
        for mode in 0..3 {
            let (_, service) = mailbox(session.clone()).unwrap();
            let (mut original, old_reply) = work(&service, request.clone());
            let lease = original.lease();
            pause_after(&service, &mut original, boundary);
            match mode {
                0 => {
                    let (mut replacement, reply) = work(&service, request.clone());
                    replacement.route_body = original.route_body.take();
                    service.retry.try_send(replacement).unwrap();
                    service.serve_service_query_slice(1, true, Instant::now() + WAIT, |_, _| {
                        panic!("foreign Work")
                    });
                    assert_eq!(reply.try_recv().unwrap().result, Err(Failure::Binding));
                    drop(original);
                }
                1 => {
                    let (_, other) = mailbox(session.clone()).unwrap();
                    other.retry.try_send(original).unwrap();
                    other.serve_service_query_slice(1, true, Instant::now() + WAIT, |_, _| {
                        panic!("foreign attachment")
                    });
                }
                2 => {
                    service.active.store(false, Ordering::Release);
                    service.retry.try_send(original).unwrap();
                    service.serve_service_query_slice(1, true, Instant::now() + WAIT, |_, _| {
                        panic!("cancelled body")
                    });
                }
                _ => unreachable!(),
            }
            assert!(!lease.is_active());
            assert!(old_reply.try_recv().is_err());
        }
    }
}

#[test]
fn partial_body_original_expiry_is_absolute_and_has_no_provider_or_reply() {
    let (session, request) = captured_route_request();
    for phase in [
        RouteBodyPhase::Context,
        RouteBodyPhase::ContextWidth,
        RouteBodyPhase::ContextSelection,
        RouteBodyPhase::ContextHeight,
        RouteBodyPhase::ContextBody,
        RouteBodyPhase::Dispatch,
        RouteBodyPhase::Model,
    ] {
        let (_, service) = mailbox(session.clone()).unwrap();
        let (mut work, reply) = work(&service, request.clone());
        let lease = work.lease();
        pause_after(&service, &mut work, phase);
        std::thread::sleep(lease.deadline().saturating_duration_since(Instant::now()));
        service.retry.try_send(work).unwrap();
        service.serve_service_query_slice(1, true, Instant::now() + WAIT, |_, _| {
            panic!("expired body")
        });
        assert!(!lease.is_active());
        assert!(reply.try_recv().is_err());
        assert!(service.rx.is_empty());
    }
}

#[test]
fn staged_validation_preserves_direct_error_order_across_context_and_dispatch() {
    let (session, request) = captured_route_request();
    for case in 0..6 {
        let (_, service) = mailbox(session.clone()).unwrap();
        let mut request = request.clone();
        let Query::SourceMotion(MotionQuery::Route(route)) = &mut request.query else {
            unreachable!()
        };
        let expected = match case {
            0 => {
                route.request.point_limit = 1;
                route.collision_width = None;
                Failure::Invalid
            }
            1 => {
                route.request.target.as_mut().unwrap().id = 0;
                route.request.kind = motion::RouteKind::HomeForceDestination;
                Failure::Binding
            }
            2 => {
                route.request.launch_origin.x = ai::Real::ONE;
                route.collision_width = None;
                Failure::UnsupportedQuery
            }
            3 => {
                route.request.goal.x = ai::Real::ONE;
                route.collision_width = None;
                Failure::Binding
            }
            4 => {
                route.request.pathfinder_dispatch = None;
                route.collision_width = None;
                Failure::Binding
            }
            5 => {
                route.collision_width = None;
                route.owner_ignore_pathfinding = None;
                Failure::MissingGeometry
            }
            _ => unreachable!(),
        };
        assert_eq!(session.body(&request), Err(expected), "direct case={case}");
        let (mut work, reply) = work(&service, request);
        let lease = work.lease();
        let error = service
            .advance_route_body(&mut work, Instant::now() + WAIT, Instant::now, |_, _| {})
            .unwrap_err();
        assert_eq!(error, expected, "staged case={case}");
        assert!(work.route_validation.is_none());
        assert_eq!(
            service.finish_query_result(work, Err(error), Instant::now(), true),
            QueryDisposition::Failed
        );
        assert_eq!(reply.try_recv().unwrap().result, Err(expected));
        assert!(!lease.is_active());
        assert!(service.rx.is_empty());
    }
}

#[test]
fn missing_source_witness_is_terminal_even_after_prior_body_subparts() {
    let (session, request) = captured_route_request();
    for missing in 0..4 {
        let (_, service) = mailbox(session.clone()).unwrap();
        let mut request = request.clone();
        let Query::SourceMotion(MotionQuery::Route(route)) = &mut request.query else {
            unreachable!()
        };
        match missing {
            0 => route.collision_width = None,
            1 => route.collision_height = None,
            2 => route.target_player_height = None,
            3 => route.owner_ignore_pathfinding = None,
            _ => unreachable!(),
        }
        let expected = session.body(&request).unwrap_err();
        assert_eq!(
            expected,
            if missing == 3 {
                Failure::Binding
            } else {
                Failure::MissingGeometry
            }
        );
        // Raw malformed inputs must still execute every retained validation check.
        let (mut work, reply) = work(&service, request);
        let lease = work.lease();
        pause_after(
            &service,
            &mut work,
            if missing == 3 {
                RouteBodyPhase::Model
            } else {
                RouteBodyPhase::Context
            },
        );
        let error = service
            .advance_route_body(&mut work, Instant::now() + WAIT, Instant::now, |_, _| {})
            .unwrap_err();
        assert_eq!(error, expected);
        assert!(work.route_validation.is_none());
        // Real source failure must not be changed into a slice retry by late handling.
        assert_eq!(
            service.finish_query_result(work, Err(error), Instant::now(), true),
            QueryDisposition::Failed
        );
        assert_eq!(reply.try_recv().unwrap().result, Err(expected));
        assert!(!lease.is_active());
        assert!(service.rx.is_empty());
    }
}

#[test]
fn cancellation_inside_every_body_subpart_retires_without_provider_or_reply() {
    let (session, request) = captured_route_request();
    for phase in [
        RouteBodyPhase::Context,
        RouteBodyPhase::ContextWidth,
        RouteBodyPhase::ContextSelection,
        RouteBodyPhase::ContextHeight,
        RouteBodyPhase::ContextBody,
        RouteBodyPhase::Dispatch,
        RouteBodyPhase::Model,
        RouteBodyPhase::OwnerState,
    ] {
        for saved in [false, true] {
            let (_, service) = mailbox(session.clone()).unwrap();
            let (mut work, reply) = work(&service, request.clone());
            let lease = work.lease();
            let result = service.advance_route_body(
                &mut work,
                Instant::now() + WAIT,
                Instant::now,
                |current, committed| {
                    if current == phase && committed == saved {
                        service.active.store(false, Ordering::Release);
                    }
                },
            );
            assert!(result.is_ok() || result == Err(Failure::Deadline));
            assert!(!lease.is_active());
            assert_eq!(service.defer_route_query(work), QueryDisposition::Retired);
            assert!(service.rx.is_empty());
            assert!(reply.try_recv().is_err());
        }
    }
}

#[test]
fn late_completion_of_each_body_subpart_never_certifies_it_or_restarts_prior_work() {
    let (session, request) = captured_route_request();
    let phases = [
        RouteBodyPhase::Context,
        RouteBodyPhase::ContextWidth,
        RouteBodyPhase::ContextSelection,
        RouteBodyPhase::ContextHeight,
        RouteBodyPhase::ContextBody,
        RouteBodyPhase::Dispatch,
        RouteBodyPhase::Model,
        RouteBodyPhase::OwnerState,
    ];
    for (index, late) in phases.into_iter().enumerate() {
        let (_, service) = mailbox(session.clone()).unwrap();
        let (mut work, reply) = work(&service, request.clone());
        let lease = work.lease();
        let expiry = lease.deadline();
        let start = Instant::now();
        let slice = start + Duration::from_millis(2);
        let now = Cell::new(start);
        let mut completed = vec![];
        assert_eq!(
            service.advance_route_body(
                &mut work,
                slice,
                || now.get(),
                |phase, saved| {
                    if saved {
                        completed.push(phase);
                    }
                    if phase == late && !saved {
                        now.set(slice);
                    }
                }
            ),
            Err(Failure::Deadline)
        );
        assert_eq!(completed, phases[..index]);
        assert_eq!(work.route_body.as_ref().unwrap().next, late);
        assert!(
            work.route_validation.is_none(),
            "late {late:?} minted a whole-body receipt"
        );
        assert!(reply.is_empty());
        assert!(work.lease().same_job(&lease));
        assert_eq!(work.deadline, expiry);
        let mut resumed = vec![];
        service
            .advance_route_body(
                &mut work,
                slice,
                || start,
                |phase, saved| {
                    if saved {
                        resumed.push(phase);
                    }
                },
            )
            .unwrap();
        assert_eq!(resumed, phases[index..]);
        assert_eq!(
            work.route_validation.as_ref().unwrap().body,
            session.body(&request).unwrap()
        );
        assert!(work.route_body.is_none());
        assert!(reply.is_empty());
    }
}

#[test]
fn completed_job_exposes_only_exact_profile_player_composition_and_live_receipts() {
    let (session, request) = captured_route_request();
    let (_, service) = mailbox(session.clone()).unwrap();
    let (mut work, _) = work(&service, request.clone());
    service
        .advance_route_body(&mut work, Instant::now() + WAIT, Instant::now, |_, _| {})
        .unwrap();
    let validation = work.route_validation.as_ref().unwrap();
    let job = RouteJob { validation };
    let policy = session.same_polygon_route.as_ref().unwrap();
    let profile = &policy.environment.profile;
    let model = job
        .validated_model(profile, policy.environment.player, policy.binding)
        .unwrap();
    assert!(std::ptr::eq(model, &validation.model));
    assert!(job
        .validated_dispatch(profile)
        .unwrap()
        .same_live_dispatch(request.require_pathfinder_dispatch(profile).unwrap()));
    let Query::SourceMotion(MotionQuery::Route(route)) = &request.query else {
        unreachable!()
    };
    assert_eq!(
        job.validated_owner_ignore(profile).unwrap(),
        route.require_owner_ignore_pathfinding(profile).unwrap()
    );
    let mut changed = (**profile).clone();
    changed.tick_ms += 1;
    assert_eq!(job.require_profile(&changed), Err(Failure::Binding));
    assert_eq!(job.validated_dispatch(&changed), Err(Failure::Binding));
    assert_eq!(job.validated_owner_ignore(&changed), Err(Failure::Binding));
    let mut player = policy.environment.player;
    player.character[0] ^= 1;
    assert_eq!(
        job.validated_model(profile, player, policy.binding),
        Err(Failure::Binding)
    );
    let mut binding = policy.binding;
    binding.raw_geometry[0] ^= 1;
    assert_eq!(
        job.validated_model(profile, policy.environment.player, binding),
        Err(Failure::Binding)
    );
    service.active.store(false, Ordering::Release);
    assert_eq!(job.require_profile(profile), Err(Failure::Deadline));
    assert_eq!(job.validated_dispatch(profile), Err(Failure::Deadline));
    assert_eq!(job.validated_owner_ignore(profile), Err(Failure::Deadline));
    assert_eq!(
        job.validated_model(profile, policy.environment.player, policy.binding),
        Err(Failure::Deadline)
    );
}

#[test]
fn partial_or_late_model_never_installs_a_reusable_model_receipt() {
    let (session, request) = captured_route_request();
    let (_, service) = mailbox(session).unwrap();
    let (mut work, _) = work(&service, request);
    pause_after(&service, &mut work, RouteBodyPhase::Context);
    assert!(work.route_body.as_ref().unwrap().model.is_none());
    let start = Instant::now();
    let slice = start + Duration::from_millis(2);
    let now = Cell::new(start);
    assert_eq!(
        service.advance_route_body(
            &mut work,
            slice,
            || now.get(),
            |phase, saved| {
                if phase == RouteBodyPhase::Model && !saved {
                    now.set(slice);
                }
            }
        ),
        Err(Failure::Deadline)
    );
    assert!(work.route_body.as_ref().unwrap().model.is_none());
    assert!(work.route_validation.is_none());
    pause_after(&service, &mut work, RouteBodyPhase::Model);
    assert!(work.route_body.as_ref().unwrap().model.is_some());
    assert!(work.route_validation.is_none());
}

#[test]
fn initialized_route_retains_only_timely_model_subparts_for_the_exact_work() {
    let (session, request) = super::super::source_route_same_polygon::dry_habitat_tests::captured_initialized_route_request();
    let direct = session.body(&request).unwrap();
    for late in [
        RouteBodyPhase::ContextWidth,
        RouteBodyPhase::ContextSelection,
        RouteBodyPhase::ContextHeight,
    ] {
        let (_, service) = mailbox(session.clone()).unwrap();
        let (mut work, reply) = work(&service, request.clone());
        let lease = work.lease();
        let expiry = lease.deadline();
        let start = Instant::now();
        let slice = start + Duration::from_millis(2);
        let now = Cell::new(start);
        assert_eq!(
            service.advance_route_body(
                &mut work,
                slice,
                || now.get(),
                |phase, saved| {
                    if phase == late && !saved {
                        now.set(slice);
                    }
                }
            ),
            Err(Failure::Deadline)
        );
        let progress = work.route_body.as_ref().unwrap();
        assert_eq!(progress.next, late);
        assert!(progress.model.is_none());
        assert!(work.route_validation.is_none());
        let initialized = progress.initialized.as_ref().unwrap();
        let profile = &session
            .same_polygon_route
            .as_ref()
            .unwrap()
            .environment
            .profile;
        let Query::SourceMotion(MotionQuery::Route(route)) = &request.query else {
            unreachable!()
        };
        assert!(initialized.for_route(profile, route).is_err());
        assert!(reply.is_empty());
        assert!(work.lease().same_job(&lease));
        assert_eq!(work.deadline, expiry);
        service
            .advance_route_body(&mut work, slice, || start, |_, _| {})
            .unwrap();
        assert_eq!(work.route_validation.as_ref().unwrap().body, direct);
        assert!(work.route_body.is_none());
        assert!(reply.is_empty());
    }
}

#[test]
fn initialized_model_receipt_checks_order_and_exact_inputs() {
    let (session, request) = super::super::source_route_same_polygon::dry_habitat_tests::captured_initialized_route_request();
    let profile = &session
        .same_polygon_route
        .as_ref()
        .unwrap()
        .environment
        .profile;
    let Query::SourceMotion(MotionQuery::Route(route)) = &request.query else {
        unreachable!()
    };
    let begun = super::super::source_route_filter::validate_initialized_route_context(
        &session,
        &session.same_polygon_route.as_ref().unwrap().model_profile,
        &request,
    )
    .unwrap()
    .unwrap();
    assert!(begun.validate_selection().is_err());
    assert!(begun.validate_height().is_err());
    assert!(begun.for_route(profile, route).is_err());
    assert!(begun.owner_ignore_for(profile, route).is_err());
    assert!(begun
        .dispatch_for(profile, route, &request.binding)
        .is_err());
    let width = begun.validate_width().unwrap();
    assert!(width.validate_width().is_err());
    assert!(width.validate_height().is_err());
    let selection = width.validate_selection().unwrap();
    assert!(selection.validate_selection().is_err());
    let complete = selection.validate_height().unwrap();
    assert!(complete.validate_height().is_err());
    let validated = complete.for_route(profile, route).unwrap();
    assert_eq!(
        validated.width,
        route.require_collision_width(profile).unwrap()
    );
    assert_eq!(
        validated.height,
        route.require_collision_height(profile).unwrap()
    );
    assert_eq!(
        complete.owner_ignore_for(profile, route).unwrap(),
        route.require_owner_ignore_pathfinding(profile).unwrap()
    );
    assert_eq!(
        complete
            .dispatch_for(profile, route, &request.binding)
            .unwrap(),
        request.require_pathfinder_dispatch(profile).unwrap()
    );
    let player = session
        .same_polygon_route
        .as_ref()
        .unwrap()
        .environment
        .player;
    assert_eq!(
        complete.target_height_for(profile, route, session.world, player),
        route.require_target_collision_height(profile, session.world, player)
    );
    assert_eq!(
        complete.target_water_for(profile, route, session.world, player),
        route.require_target_water(profile, session.world, player)
    );
    let mut changed_profile = (**profile).clone();
    changed_profile.tick_ms += 1;
    assert!(complete.for_route(&changed_profile, route).is_err());
    assert!(complete.owner_ignore_for(&changed_profile, route).is_err());
    assert!(complete
        .dispatch_for(&changed_profile, route, &request.binding)
        .is_err());
    assert!(complete
        .target_height_for(&changed_profile, route, session.world, player)
        .is_err());
    assert!(complete
        .target_water_for(&changed_profile, route, session.world, player)
        .is_err());
    let mut changed_route = route.clone();
    changed_route.request.goal.x = ai::Real::ONE;
    assert!(complete.for_route(profile, &changed_route).is_err());
    assert!(complete.owner_ignore_for(profile, &changed_route).is_err());
    assert!(complete
        .dispatch_for(profile, &changed_route, &request.binding)
        .is_err());
    assert!(complete
        .target_height_for(profile, &changed_route, session.world, player)
        .is_err());
    assert!(complete
        .target_water_for(profile, &changed_route, session.world, player)
        .is_err());
    let mut changed_owner = route.clone();
    let mut owner_wire = serde_json::to_value(&changed_owner.owner).unwrap();
    owner_wire["npc"]["generation"] = 1.into();
    changed_owner.owner = serde_json::from_value(owner_wire).unwrap();
    assert!(complete.for_route(profile, &changed_owner).is_err());
    assert!(complete.owner_ignore_for(profile, &changed_owner).is_err());
    assert!(complete
        .dispatch_for(profile, &changed_owner, &request.binding)
        .is_err());
    assert!(complete
        .target_height_for(profile, &changed_owner, session.world, player)
        .is_err());
    assert!(complete
        .target_water_for(profile, &changed_owner, session.world, player)
        .is_err());
    let deserialized = serde_json::from_value(serde_json::to_value(route).unwrap()).unwrap();
    assert_eq!(&deserialized, route.as_ref());
    assert!(complete.for_route(profile, &deserialized).is_err());
    assert!(complete.owner_ignore_for(profile, &deserialized).is_err());
    assert!(complete
        .dispatch_for(profile, &deserialized, &request.binding)
        .is_err());
    assert!(complete
        .target_height_for(profile, &deserialized, session.world, player)
        .is_err());
    assert!(complete
        .target_water_for(profile, &deserialized, session.world, player)
        .is_err());
    let (_, fresh) = super::super::source_route_same_polygon::dry_habitat_tests::captured_initialized_route_request();
    assert_eq!(fresh, request);
    let Query::SourceMotion(MotionQuery::Route(fresh_route)) = &fresh.query else {
        unreachable!()
    };
    assert!(complete.for_route(profile, fresh_route).is_err());
    assert!(complete.owner_ignore_for(profile, fresh_route).is_err());
    assert!(complete
        .dispatch_for(profile, fresh_route, &request.binding)
        .is_err());
    assert!(complete
        .target_height_for(profile, fresh_route, session.world, player)
        .is_err());
    assert!(complete
        .target_water_for(profile, fresh_route, session.world, player)
        .is_err());
}

#[test]
fn initialized_route_staged_checks_preserve_direct_failure_order() {
    let (session, request) = super::super::source_route_same_polygon::dry_habitat_tests::captured_initialized_route_request();
    for case in 0..7 {
        let mut request = request.clone();
        let Query::SourceMotion(MotionQuery::Route(route)) = &mut request.query else {
            unreachable!()
        };
        let expected = match case {
            0 => {
                route.collision_width = None;
                route.request.point_limit = 1;
                Failure::Invalid
            }
            1 => {
                route.collision_width = None;
                route.request.target.as_mut().unwrap().id = 0;
                Failure::Binding
            }
            2 => {
                route.collision_height = None;
                route.request.target.as_mut().unwrap().id = 0;
                Failure::Binding
            }
            3 => {
                route.request.target.as_mut().unwrap().collision_height = ai::Real::ONE;
                route.owner_ignore_pathfinding = None;
                Failure::UnresolvedBody
            }
            4 => {
                route.owner_ignore_pathfinding = None;
                Failure::Binding
            }
            5 | 6 => {
                let mut height = serde_json::to_value(&route.collision_height).unwrap();
                if case == 5 {
                    height["width_source_binding"][0] =
                        (height["width_source_binding"][0].as_u64().unwrap() ^ 1).into();
                } else {
                    height["generation_selection"]["first_counter"] = 1.into();
                }
                route.collision_height = serde_json::from_value(height).unwrap();
                route.request.target.as_mut().unwrap().collision_height = ai::Real::ONE;
                Failure::Binding
            }
            _ => unreachable!(),
        };
        assert_eq!(session.body(&request), Err(expected), "direct case={case}");
        let (_, service) = mailbox(session.clone()).unwrap();
        let (mut work, reply) = work(&service, request);
        let lease = work.lease();
        let start = Instant::now();
        let result = service.advance_route_body(
            &mut work,
            start + Duration::from_millis(2),
            || start,
            |_, _| {},
        );
        assert_eq!(result, Err(expected), "staged case={case}");
        assert!(work.route_validation.is_none());
        assert!(reply.is_empty());
        assert!(work.lease().same_job(&lease));
    }
}

#[test]
fn initialized_dispatch_reuse_preserves_outer_binding_and_direct_errors() {
    let (session, request) = super::super::source_route_same_polygon::dry_habitat_tests::captured_initialized_route_request();
    let policy = session.same_polygon_route.as_ref().unwrap();
    let profile = &policy.environment.profile;
    let Query::SourceMotion(MotionQuery::Route(route)) = &request.query else {
        unreachable!()
    };
    let initialized = super::super::source_route_filter::validate_initialized_route_context(
        &session,
        &policy.model_profile,
        &request,
    )
    .unwrap()
    .unwrap()
    .validate_width()
    .unwrap()
    .validate_selection()
    .unwrap()
    .validate_height()
    .unwrap();
    for case in 0..8 {
        let mut changed = request.clone();
        match case {
            0 => changed.binding.tick += 1,
            1 => changed.binding.world[0] ^= 1,
            2 => changed.binding.npc.as_mut().unwrap().generation += 1,
            3 => changed.binding.npc = None,
            4 => changed.binding.profile[0] ^= 1,
            5 => changed.binding.authority_policy[0] ^= 1,
            6 => changed.binding.geometry[0] ^= 1,
            7 => {}
            _ => unreachable!(),
        }
        let direct = policy.validate_dispatch(&session, &changed);
        let owned =
            policy.validate_dispatch_with_initialized(&session, &changed, Some(&initialized));
        assert_eq!(owned, direct, "case={case}");
        if case == 7 {
            assert_eq!(owned, Ok(()));
        } else {
            assert_eq!(owned, Err(Failure::Binding));
        }
        assert_eq!(
            initialized
                .dispatch_for(profile, route, &changed.binding)
                .map(|_| ()),
            changed
                .require_pathfinder_dispatch(profile)
                .map(|_| ())
                .map_err(|_| Failure::Binding),
        );
    }
    // Diagnostic only: no wall-clock assertion and no budget change. CI's actual
    // mailbox regression below still has the original 2ms slice and 500ms lease.
    let start = Instant::now();
    for _ in 0..16 {
        policy.validate_dispatch(&session, &request).unwrap();
    }
    let direct = start.elapsed();
    let start = Instant::now();
    for _ in 0..16 {
        policy
            .validate_dispatch_with_initialized(&session, &request, Some(&initialized))
            .unwrap();
    }
    eprintln!(
        "initialized dispatch exact-proof comparison: direct16={direct:?} owned16={:?}",
        start.elapsed()
    );
}
