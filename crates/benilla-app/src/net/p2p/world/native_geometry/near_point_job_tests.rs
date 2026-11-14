// SPDX-License-Identifier: GPL-3.0-or-later
//! Original-Work ownership and publication guards, not NearPoint geometry parity.
use super::near_point_mailbox_tests::{facts, setup};
use super::*;

fn enqueue(
    service: &Mailbox,
    ticket: u64,
    request: AuthorityRequest,
) -> (JobLease, Receiver<Reply>) {
    let (lease, reply) = service
        .enqueue_service_test_request(ticket, request)
        .unwrap();
    (lease, reply.0)
}
fn validate_first(service: &Mailbox) {
    assert_eq!(
        service.serve_service_query_slice(PER_FRAME, false, Instant::now() + WAIT, |_, _| {
            panic!("NearPoint body completion must yield before any provider")
        }),
        1
    );
}

#[test]
fn primary_near_point_retains_body_and_original_lease_independently_of_route_selection() {
    fn send<T: Send>() {}
    send::<Work>();
    let (session, request) = setup();
    let expected_body = session.body(&request).unwrap();
    let (_, service) = mailbox(session).unwrap();
    let (lease, reply) = enqueue(&service, 71, request.clone());
    let expiry = lease.deadline();
    validate_first(&service);
    assert!(reply.is_empty());
    let work = service.rx.try_recv().unwrap();
    assert!(work.near_point_validation.is_some());
    assert!(work.route_validation.is_none());
    assert!(work.route_body.is_none());
    assert!(work.lease().same_job(&lease));
    service.retry.try_send(work).unwrap();
    let identity = Arc::new(());
    let mut calls = 0;
    for stage_routes in [false, true, false] {
        service.serve_service_query_slice(
            PER_FRAME,
            stage_routes,
            Instant::now() + WAIT,
            |query, until| {
                let ServiceQuery::NearPoint(job) = query else {
                    panic!("NearPoint lost staging")
                };
                assert_eq!(job.request(), &request);
                assert_eq!(job.body(), expected_body);
                assert_eq!(job.lease().ticket(), 71);
                assert!(job.lease().same_job(&lease));
                assert_eq!(job.lease().deadline(), expiry);
                assert_eq!(until, expiry);
                assert_eq!(job.validation.selected_provider.get().is_some(), calls > 0);
                job.bind_provider(&identity).unwrap();
                calls += 1;
                Ok(ServiceStep::Pending)
            },
        );
        assert!(reply.is_empty());
        assert_eq!(service.rx.len(), 1);
    }
    assert_eq!(calls, 3);
    drop(service.rx.try_recv().unwrap());
    assert!(!lease.is_active());
    assert!(reply.try_recv().is_err());
}

#[test]
fn retained_near_point_rejects_changed_session_and_profile_before_provider() {
    let (session, request) = setup();
    for field in 0..12 {
        let (_, mut service) = mailbox(session.clone()).unwrap();
        let (lease, reply) = enqueue(&service, 1, request.clone());
        validate_first(&service);
        match field {
            0 => service.session.world[0] ^= 1,
            1 => service.session.profile[0] ^= 1,
            2 => service.session.policy[0] ^= 1,
            3 => service.session.content[0] ^= 1,
            4 => service.session.geometry[0] ^= 1,
            5 => service.session.map += 1,
            6 => service.session.bodies.values_mut().next().unwrap().height = ai::Real::ONE,
            7 => service.session.player.as_mut().unwrap().speed = ai::Real::ONE,
            8 => service.session.participants = None,
            9 => service.session.primary_near_point = None,
            10 => {
                let policy = service.session.primary_near_point.as_mut().unwrap();
                let mut changed = (*policy.profile).clone();
                changed.tick_ms += 1;
                policy.profile = Arc::new(changed);
            }
            11 => {
                service.session.empty_selector_near_point = Some(EmptySelectorNearPointPolicy(
                    service.session.primary_near_point.clone().unwrap(),
                ));
            }
            _ => unreachable!(),
        }
        service.serve_service_query_slice(1, false, Instant::now() + WAIT, |_, _| {
            panic!("changed session reached provider: {field}")
        });
        assert_eq!(
            reply.try_recv().unwrap().result,
            Err(Failure::Binding),
            "field {field}"
        );
        assert!(!lease.is_active());
    }
}

#[test]
fn retained_near_point_rejects_changed_request_generation_and_equal_new_work() {
    let (session, request) = setup();
    for field in 0..9 {
        let (_, service) = mailbox(session.clone()).unwrap();
        let (_, reply) = enqueue(&service, 1, request.clone());
        validate_first(&service);
        let mut work = service.rx.try_recv().unwrap();
        let saved = &mut work.near_point_validation.as_mut().unwrap().request;
        match field {
            0 => saved.binding.tick += 1,
            1 => saved.binding.npc.as_mut().unwrap().generation += 1,
            2 => saved.binding.world[0] ^= 1,
            3 => saved.binding.profile[0] ^= 1,
            4 => saved.binding.authority_policy[0] ^= 1,
            5 => saved.binding.geometry[0] ^= 1,
            6..=7 => {
                let Query::SourceMotion(MotionQuery::NearPoint(near)) = &mut saved.query else {
                    unreachable!()
                };
                if field == 6 {
                    near.request.binding.query_id += 1;
                } else {
                    near.request.owner.position.x = ai::Real::new(99.).unwrap();
                }
            }
            8 => {
                saved.query = Query::SourceMotion(MotionQuery::Spatial(motion::Query::NearPoint(
                    match &request.query {
                        Query::SourceMotion(MotionQuery::NearPoint(near)) => near.request.clone(),
                        _ => unreachable!(),
                    },
                )))
            }
            _ => unreachable!(),
        }
        service.retry.try_send(work).unwrap();
        service.serve_service_query_slice(1, false, Instant::now() + WAIT, |_, _| {
            panic!("changed request reached provider: {field}")
        });
        assert_eq!(
            reply.try_recv().unwrap().result,
            Err(Failure::Binding),
            "field {field}"
        );
    }
    let (_, service) = mailbox(session).unwrap();
    let (old_lease, old_reply) = enqueue(&service, 1, request.clone());
    validate_first(&service);
    let mut old = service.rx.try_recv().unwrap();
    let (reply, receive) = bounded(1);
    let equal: AuthorityRequest =
        serde_json::from_value(serde_json::to_value(&request).unwrap()).unwrap();
    assert_eq!(equal, request);
    let mut replacement = Work::new(
        1,
        equal,
        old_lease.deadline(),
        reply,
        service.active.clone(),
    );
    replacement.near_point_validation = old.near_point_validation.take();
    assert!(!replacement.lease().same_job(&old_lease));
    service.retry.try_send(replacement).unwrap();
    service.serve_service_query_slice(1, false, Instant::now() + WAIT, |_, _| {
        panic!("equal new Work")
    });
    assert_eq!(receive.try_recv().unwrap().result, Err(Failure::Binding));
    drop(old);
    assert!(!old_lease.is_active());
    assert!(old_reply.try_recv().is_err());
}

#[test]
fn near_point_profile_and_selected_provider_are_exact_and_revoked_on_cancel() {
    let (session, request) = setup();
    let (_, service) = mailbox(session).unwrap();
    let (_, receive) = enqueue(&service, 1, request.clone());
    validate_first(&service);
    let work = service.rx.try_recv().unwrap();
    let job = NearPointJob {
        validation: work.near_point_validation.as_ref().unwrap(),
    };
    let profile = &job.session().primary_near_point.as_ref().unwrap().profile;
    assert_eq!(job.require_profile(profile), Ok(()));
    let mut changed = (**profile).clone();
    changed.tick_ms += 1;
    assert_eq!(job.require_profile(&changed), Err(Failure::Binding));
    let first = Arc::new(());
    assert_eq!(job.bind_provider(&first), Ok(()));
    assert_eq!(job.bind_provider(&Arc::clone(&first)), Ok(()));
    assert_eq!(job.bind_provider(&Arc::new(())), Err(Failure::Binding));
    assert_eq!(job.validate_facts(&facts(&request)), Ok(()));
    let mut changed = facts(&request);
    let Facts::SourceMotion(MotionFacts::Spatial(reply)) = &mut changed else {
        unreachable!()
    };
    let motion::Query::NearPoint(near) = &mut reply.request else {
        unreachable!()
    };
    near.binding.query_id += 1;
    assert_eq!(job.validate_facts(&changed), Err(Failure::Binding));
    assert_eq!(
        job.validate_facts(&Facts::PlayerMovement { accepted: false }),
        Err(Failure::Binding)
    );
    service.active.store(false, Ordering::Release);
    assert_eq!(job.require_profile(profile), Err(Failure::Deadline));
    assert_eq!(job.bind_provider(&first), Err(Failure::Deadline));
    assert_eq!(job.validate_facts(&facts(&request)), Err(Failure::Deadline));
    drop(work);
    assert!(receive.try_recv().is_err());
}

#[test]
fn near_point_cancellation_foreign_attachment_and_original_expiry_retire_work() {
    let (session, request) = setup();
    let (_, service) = mailbox(session.clone()).unwrap();
    let (_, other) = mailbox(session).unwrap();
    let (lease, reply) = enqueue(&service, 1, request.clone());
    validate_first(&service);
    other
        .retry
        .try_send(service.rx.try_recv().unwrap())
        .unwrap();
    other.serve_service_query_slice(1, false, Instant::now() + WAIT, |_, _| {
        panic!("foreign attachment")
    });
    assert!(!lease.is_active());
    assert!(reply.try_recv().is_err());
    let (lease, reply) = enqueue(&service, 2, request.clone());
    validate_first(&service);
    drop(service);
    assert!(!lease.is_active());
    assert!(reply.try_recv().is_err());

    let (lease, reply) = enqueue(&other, 3, request);
    let expiry = lease.deadline();
    validate_first(&other);
    std::thread::sleep(expiry.saturating_duration_since(Instant::now()));
    assert_eq!(lease.deadline(), expiry);
    assert!(!lease.is_active());
    other.serve_service_query_slice(1, false, Instant::now() + WAIT, |_, _| {
        panic!("expired original lease")
    });
    assert!(other.rx.is_empty());
    assert!(reply.try_recv().is_err());
}

#[test]
fn late_near_point_body_never_installs_a_receipt_or_hides_real_failure() {
    let (session, request) = setup();
    let (_, service) = mailbox(session).unwrap();
    let (lease, reply) = enqueue(&service, 1, request);
    let mut work = service.rx.try_recv().unwrap();
    let start = Instant::now();
    let slice = start + Duration::from_millis(2);
    let mut calls = 0;
    assert_eq!(
        service.advance_near_point_body(&mut work, slice, || {
            calls += 1;
            if calls == 1 {
                start
            } else {
                slice
            }
        }),
        Err(Failure::Deadline)
    );
    assert!(work.near_point_validation.is_none());
    assert!(work.lease().same_job(&lease));
    assert!(reply.is_empty());
    let (tx, _) = bounded(1);
    let mut invalid = work.request.clone();
    invalid.binding.tick = 0;
    let mut invalid = Work::new(2, invalid, lease.deadline(), tx, service.active.clone());
    calls = 0;
    assert_eq!(
        service.advance_near_point_body(&mut invalid, slice, || {
            calls += 1;
            if calls == 1 {
                start
            } else {
                slice
            }
        }),
        Err(Failure::Binding)
    );
    assert!(invalid.near_point_validation.is_none());
}

#[test]
fn late_near_point_result_retries_same_work_and_calls_provider_again() {
    let (session, request) = setup();
    let (_, service) = mailbox(session).unwrap();
    let (lease, reply) = enqueue(&service, 1, request.clone());
    validate_first(&service);
    let expiry = lease.deadline();
    let mut calls = 0;
    service.serve_service_query_slice(
        1,
        false,
        Instant::now() + Duration::from_millis(10),
        |query, until| {
            let ServiceQuery::NearPoint(job) = query else {
                panic!("expected NearPoint job")
            };
            assert!(job.lease().same_job(&lease));
            calls += 1;
            let result = facts(job.request());
            std::thread::sleep(
                until.saturating_duration_since(Instant::now()) + Duration::from_micros(100),
            );
            Ok(ServiceStep::Complete(Box::new(result)))
        },
    );
    assert!(reply.is_empty());
    let work = service.rx.try_recv().unwrap();
    assert!(work.lease().same_job(&lease));
    assert_eq!(work.deadline, expiry);
    assert!(work.near_point_validation.is_some());
    // The final transport guard independently refuses late success too.
    assert_eq!(
        service.finish_query_result(work, Ok(facts(&request)), Instant::now(), true),
        QueryDisposition::Deferred
    );
    assert!(reply.is_empty());
    service.serve_service_query_slice(1, false, Instant::now() + WAIT, |query, _| {
        let ServiceQuery::NearPoint(job) = query else {
            panic!("expected retried NearPoint")
        };
        assert!(job.lease().same_job(&lease));
        assert_eq!(job.lease().deadline(), expiry);
        calls += 1;
        Ok(ServiceStep::Complete(Box::new(facts(job.request()))))
    });
    assert_eq!(calls, 2);
    assert_eq!(reply.try_recv().unwrap().result, Ok(facts(&request)));
    assert!(!lease.is_active());
    assert!(service.rx.is_empty());
}

#[test]
fn late_near_point_provider_and_reply_errors_preserve_provenance() {
    let (session, request) = setup();
    for failure in [
        None,
        Some(Failure::MissingGeometry),
        Some(Failure::UnsupportedNearPointFallback),
    ] {
        let (_, service) = mailbox(session.clone()).unwrap();
        let (_, reply) = enqueue(&service, 1, request.clone());
        validate_first(&service);
        service.serve_service_query_slice(
            1,
            false,
            Instant::now() + Duration::from_millis(10),
            |_, until| {
                std::thread::sleep(
                    until.saturating_duration_since(Instant::now()) + Duration::from_micros(100),
                );
                if let Some(error) = failure {
                    Err(error)
                } else {
                    Ok(ServiceStep::Complete(Box::new(Facts::PlayerMovement {
                        accepted: false,
                    })))
                }
            },
        );
        assert_eq!(
            reply.try_recv().unwrap().result,
            Err(failure.unwrap_or(Failure::Binding))
        );
        assert!(service.rx.is_empty());
    }
}

#[test]
fn empty_selector_near_point_still_uses_immediate_validation() {
    let (mut session, request) = setup();
    session.empty_selector_near_point = Some(EmptySelectorNearPointPolicy(
        session.primary_near_point.take().unwrap(),
    ));
    let (_, service) = mailbox(session).unwrap();
    let (_, reply) = enqueue(&service, 1, request.clone());
    service.serve_service_query_slice(1, true, Instant::now() + WAIT, |query, _| {
        let ServiceQuery::Immediate(validated) = query else {
            panic!("empty-selector staging expanded")
        };
        Ok(ServiceStep::Complete(Box::new(facts(validated.request()))))
    });
    assert_eq!(reply.try_recv().unwrap().result, Ok(facts(&request)));
}
