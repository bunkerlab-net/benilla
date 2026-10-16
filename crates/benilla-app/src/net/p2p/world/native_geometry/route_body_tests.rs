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
            RouteBodyPhase::Context => RouteBodyPhase::Model,
            RouteBodyPhase::Model => RouteBodyPhase::OwnerState,
            RouteBodyPhase::OwnerState => panic!("final body is not partial"),
        }
    );
}

#[test]
fn completed_subparts_survive_and_only_the_oversized_subpart_repeats() {
    let (session, request) = captured_route_request();
    let (_, service) = mailbox(session.clone()).unwrap();
    let (mut work, reply) = work(&service, request.clone());
    let lease = work.lease();
    let expiry = work.deadline;
    pause_after(&service, &mut work, RouteBodyPhase::Context);
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
    for boundary in [RouteBodyPhase::Context, RouteBodyPhase::Model] {
        for field in 0..11 {
            let (_, mut service) = mailbox(session.clone()).unwrap();
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
                8 => {
                    progress.request =
                        serde_json::from_value(serde_json::to_value(&request).unwrap()).unwrap();
                    assert_eq!(progress.request, request);
                }
                9 => progress.request = fresh_request.clone(),
                10 => {
                    let policy = service.session.player_environment.as_mut().unwrap();
                    let mut profile = (*policy.profile).clone();
                    profile.tick_ms += 1;
                    policy.profile = Arc::new(profile);
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
    for boundary in [RouteBodyPhase::Context, RouteBodyPhase::Model] {
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
    let (_, service) = mailbox(session).unwrap();
    let (mut work, reply) = work(&service, request);
    let lease = work.lease();
    pause_after(&service, &mut work, RouteBodyPhase::Model);
    std::thread::sleep(lease.deadline().saturating_duration_since(Instant::now()));
    service.retry.try_send(work).unwrap();
    service.serve_service_query_slice(1, true, Instant::now() + WAIT, |_, _| {
        panic!("expired body")
    });
    assert!(!lease.is_active());
    assert!(reply.try_recv().is_err());
    assert!(service.rx.is_empty());
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
