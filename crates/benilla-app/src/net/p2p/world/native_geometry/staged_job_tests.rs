// SPDX-License-Identifier: GPL-3.0-or-later
//! Scheduling and receipt boundaries using an actual unclaimed runtime Route.
//! Positive source proposals and final physics certification live in provider/app tests.
use super::super::source_route_same_polygon::tests::captured_route_request;
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
        service.serve_service_query_slice(PER_FRAME, true, Instant::now() + WAIT, |_, _| {
            panic!("phase1 must yield without a provider callback")
        }),
        1
    );
}
fn pending(query: ServiceQuery<'_>, _: Instant) -> Result<ServiceStep, Failure> {
    assert!(matches!(query, ServiceQuery::Route(_)));
    Ok(ServiceStep::Pending)
}

#[test]
fn phase1_yields_once_and_every_resume_keeps_the_original_owner_and_expiry() {
    fn send<T: Send>() {}
    send::<Work>();
    let (session, request) = captured_route_request();
    let (_, service) = mailbox(session).unwrap();
    let (lease, reply) = enqueue(&service, 41, request.clone());
    validate_first(&service);
    assert!(lease.is_active());
    assert!(reply.is_empty());
    let job = service.rx.try_recv().unwrap();
    assert!(job.route_validation.is_some());
    assert!(job.lease().same_job(&lease));
    let original_deadline = lease.deadline();
    service.retry.try_send(job).unwrap();
    let mut calls = 0;
    for _ in 0..3 {
        assert_eq!(
            service.serve_service_query_slice(
                PER_FRAME,
                true,
                Instant::now() + WAIT,
                |query, until| {
                    let ServiceQuery::Route(job) = query else {
                        panic!("Route lost staging")
                    };
                    calls += 1;
                    assert!(job.lease().same_job(&lease));
                    assert_eq!(job.lease().request(), &request);
                    assert_eq!(job.lease().ticket(), 41);
                    assert_eq!(job.lease().deadline(), original_deadline);
                    assert_eq!(job.request(), &request);
                    assert_eq!(
                        job.session().bodies[&request.binding.npc.unwrap().spawn],
                        job.body()
                    );
                    assert_eq!(until, original_deadline);
                    Ok(ServiceStep::Pending)
                }
            ),
            1
        );
        assert!(reply.is_empty(), "partial phase published a reply");
        assert_eq!(service.rx.len(), 1);
    }
    assert_eq!(calls, 3);
    drop(service.rx.try_recv().unwrap());
    assert!(!lease.is_active(), "a cloned lease kept the owner alive");
    assert!(reply.try_recv().is_err());
}

#[test]
fn whole_session_mutations_fail_before_the_resumed_provider_callback() {
    let (session, request) = captured_route_request();
    for field in 0..16 {
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
            6 => {
                service
                    .session
                    .bodies
                    .get_mut(&request.binding.npc.unwrap().spawn)
                    .unwrap()
                    .height = ai::Real::ONE
            }
            7 => service.session.player.as_mut().unwrap().speed = ai::Real::ONE,
            8 => service.session.participants = None,
            9 => service.session.primary_near_point = None,
            10 => service.session.player_environment = None,
            11 => service.session.same_polygon_route = None,
            12 => {
                let profile = &service.session.player_environment.as_ref().unwrap().profile;
                service.session.static_los =
                    Some(StaticLosPolicy::from_profile(profile, true).unwrap());
            }
            13 => {
                let profile = &service.session.player_environment.as_ref().unwrap().profile;
                service.session.empty_selector_near_point =
                    Some(EmptySelectorNearPointPolicy::from_profile(profile).unwrap());
            }
            14 => {
                service
                    .session
                    .same_polygon_route
                    .as_mut()
                    .unwrap()
                    .binding
                    .navigation
                    .source_epoch[0] ^= 1
            }
            15 => {
                let policy = service.session.player_environment.as_mut().unwrap();
                let mut profile = (*policy.profile).clone();
                profile.tick_ms += 1;
                policy.profile = Arc::new(profile);
            }
            _ => unreachable!(),
        }
        assert_eq!(
            service.serve_service_query_slice(1, true, Instant::now() + WAIT, |_, _| {
                panic!("changed session reached provider: {field}")
            }),
            1
        );
        assert_eq!(
            reply.try_recv().unwrap().result,
            Err(Failure::Binding),
            "field {field}"
        );
        assert!(!lease.is_active());
    }
}

#[test]
fn changed_request_snapshot_and_equal_byte_decoded_seal_cannot_reuse_validation() {
    let (session, request) = captured_route_request();
    for field in 0..10 {
        let (_, service) = mailbox(session.clone()).unwrap();
        let (lease, reply) = enqueue(&service, 1, request.clone());
        validate_first(&service);
        let mut work = service.rx.try_recv().unwrap();
        let saved = &mut work.route_validation.as_mut().unwrap().request;
        match field {
            0 => saved.binding.tick += 1,
            1 => saved.binding.npc.as_mut().unwrap().generation += 1,
            2 => saved.binding.world[0] ^= 1,
            3 => saved.binding.profile[0] ^= 1,
            4 => saved.binding.authority_policy[0] ^= 1,
            5 => saved.binding.geometry[0] ^= 1,
            6..=8 => {
                let Query::SourceMotion(MotionQuery::Route(route)) = &mut saved.query else {
                    unreachable!()
                };
                match field {
                    6 => route.request.binding.query_id += 1,
                    7 => route.request.goal.x = ai::Real::new(99.).unwrap(),
                    8 => route.pathfinder_dispatch = None,
                    _ => unreachable!(),
                }
            }
            9 => {
                *saved = serde_json::from_value(serde_json::to_value(&request).unwrap()).unwrap();
                assert_eq!(*saved, request, "wire equality alone must not grant reuse");
            }
            _ => unreachable!(),
        }
        service.retry.try_send(work).unwrap();
        assert_eq!(
            service.serve_service_query_slice(1, true, Instant::now() + WAIT, |_, _| {
                panic!("changed receipt reached provider: {field}")
            }),
            1
        );
        assert_eq!(reply.try_recv().unwrap().result, Err(Failure::Binding));
        assert!(!lease.is_active());
    }
}

#[test]
fn equal_byte_new_job_or_live_dispatch_cannot_adopt_an_old_receipt() {
    let (session, request) = captured_route_request();
    let (_, fresh) = captured_route_request();
    assert_eq!(request, fresh);
    let (_, service) = mailbox(session).unwrap();
    let (old_lease, old_reply) = enqueue(&service, 1, request.clone());
    validate_first(&service);
    let mut old_work = service.rx.try_recv().unwrap();
    let (reply, receive) = bounded(1);
    let mut replacement = Work::new(
        1,
        request.clone(),
        old_lease.deadline(),
        reply,
        service.active.clone(),
    );
    assert!(!replacement.lease().same_job(&old_lease));
    replacement.route_validation = old_work.route_validation.take();
    service.retry.try_send(replacement).unwrap();
    service.serve_service_query_slice(1, true, Instant::now() + WAIT, |_, _| {
        panic!("replaced job")
    });
    assert_eq!(receive.try_recv().unwrap().result, Err(Failure::Binding));
    drop(old_work);
    assert!(!old_lease.is_active());
    assert!(old_reply.try_recv().is_err());

    let (_, reply) = enqueue(&service, 2, request);
    validate_first(&service);
    let mut work = service.rx.try_recv().unwrap();
    work.route_validation.as_mut().unwrap().request = fresh;
    service.retry.try_send(work).unwrap();
    service.serve_service_query_slice(1, true, Instant::now() + WAIT, |_, _| panic!("fresh seal"));
    assert_eq!(reply.try_recv().unwrap().result, Err(Failure::Binding));
}

#[test]
fn route_pending_yields_fifo_and_same_frame_attempts_stop_at_eight() {
    let (session, request) = captured_route_request();
    let (_, service) = mailbox(session).unwrap();
    let (first, first_reply) = enqueue(&service, 1, request.clone());
    let (second, second_reply) = enqueue(&service, 2, request);
    let mut frame = FrameBudget::default();
    let mut observed = Vec::new();
    for attempt in 0..PER_FRAME {
        let used = service.serve_service_query_slice(
            frame.remaining(9),
            true,
            Instant::now() + WAIT,
            |query, _| {
                let ServiceQuery::Route(job) = query else {
                    panic!("immediate Route")
                };
                observed.push(job.lease().ticket());
                Ok(ServiceStep::Pending)
            },
        );
        assert_eq!(used, 1);
        frame.spend(used);
        let next = service.rx.try_recv().unwrap();
        assert_eq!(next.ticket, if attempt % 2 == 0 { 2 } else { 1 });
        let last = service.rx.try_recv().unwrap();
        service.retry.try_send(next).unwrap();
        service.retry.try_send(last).unwrap();
    }
    assert_eq!(observed, [1, 2, 1, 2, 1, 2]);
    assert_eq!(frame.remaining(9), 0);
    assert_eq!(
        service.serve_service_query_slice(
            frame.remaining(9),
            true,
            Instant::now() + WAIT,
            |_, _| panic!("frame exhausted")
        ),
        0
    );
    assert_eq!(frame.remaining(10), PER_FRAME);
    assert!(first.is_active() && second.is_active());
    assert!(first_reply.is_empty() && second_reply.is_empty());
}

#[test]
fn concurrent_full_tail_terminates_pending_without_discarding_another_work() {
    let (session, request) = captured_route_request();
    let (_, service) = mailbox(session).unwrap();
    let (lease, reply) = enqueue(&service, 99, request.clone());
    validate_first(&service);
    let mut others = Vec::new();
    assert_eq!(
        service.serve_service_query_slice(PER_FRAME, true, Instant::now() + WAIT, |query, _| {
            let ServiceQuery::Route(job) = query else {
                panic!()
            };
            assert!(job.lease().same_job(&lease));
            for ticket in 0..QUEUE_CAPACITY {
                others.push(enqueue(&service, ticket as u64, request.clone()));
            }
            Ok(ServiceStep::Pending)
        }),
        1
    );
    assert_eq!(
        reply.try_recv().unwrap().result,
        Err(Failure::QueueCapacity)
    );
    assert!(!lease.is_active());
    assert_eq!(service.rx.len(), QUEUE_CAPACITY);
    for ticket in 0..QUEUE_CAPACITY {
        assert_eq!(service.rx.try_recv().unwrap().ticket, ticket as u64);
    }
    assert!(others
        .iter()
        .all(|(lease, reply)| !lease.is_active() && reply.try_recv().is_err()));
}

#[test]
fn cancellation_and_foreign_attachments_never_resume_or_publish() {
    let (session, request) = captured_route_request();
    for during in [false, true] {
        let (_, service) = mailbox(session.clone()).unwrap();
        let (lease, reply) = enqueue(&service, 1, request.clone());
        validate_first(&service);
        if !during {
            service.active.store(false, Ordering::Release);
        }
        let mut called = false;
        service.serve_service_query_slice(1, true, Instant::now() + WAIT, |_, _| {
            called = true;
            service.active.store(false, Ordering::Release);
            Ok(ServiceStep::Pending)
        });
        assert_eq!(called, during);
        assert!(!lease.is_active());
        assert!(reply.try_recv().is_err());
        assert!(service.rx.is_empty());
    }
    let (_, service) = mailbox(session.clone()).unwrap();
    let (lease, reply) = enqueue(&service, 1, request.clone());
    validate_first(&service);
    let (_, replacement) = mailbox(session).unwrap();
    replacement
        .retry
        .try_send(service.rx.try_recv().unwrap())
        .unwrap();
    replacement.serve_service_query_slice(1, true, Instant::now() + WAIT, |_, _| {
        panic!("foreign attachment")
    });
    assert!(!lease.is_active());
    assert!(reply.try_recv().is_err());
    let (lease, reply) = enqueue(&service, 2, request);
    validate_first(&service);
    drop(service);
    assert!(!lease.is_active());
    assert!(reply.try_recv().is_err());
}

#[test]
fn original_expiry_retires_a_validated_job_and_does_not_renew_with_a_new_slice() {
    let (session, request) = captured_route_request();
    let (_, service) = mailbox(session).unwrap();
    let (lease, reply) = enqueue(&service, 1, request);
    validate_first(&service);
    std::thread::sleep(lease.deadline().saturating_duration_since(Instant::now()));
    assert!(!lease.is_active());
    assert_eq!(
        service.serve_service_query_slice(1, true, Instant::now() + WAIT, |_, _| panic!(
            "expired Route"
        )),
        1
    );
    assert!(service.rx.is_empty());
    assert!(reply.try_recv().is_err());
}

#[test]
fn expired_phase1_does_not_mint_a_receipt_and_late_final_delivery_retries_exact_work() {
    let (session, request) = captured_route_request();
    let (_, service) = mailbox(session).unwrap();
    let (lease, reply) = enqueue(&service, 1, request);
    assert_eq!(
        service.serve_service_query_slice(1, true, Instant::now(), |_, _| panic!("expired phase1")),
        1
    );
    let work = service.rx.try_recv().unwrap();
    assert!(work.route_validation.is_none());
    assert!(work.lease().same_job(&lease));
    service.retry.try_send(work).unwrap();
    validate_first(&service);
    let work = service.rx.try_recv().unwrap();
    // Exercise only the final transport guard, without pretending to certify a Route.
    // The invalid facts cannot reach a receiver because this slice has already expired.
    assert_eq!(
        service.finish_query_result(
            work,
            Ok(Facts::PlayerMovement { accepted: false }),
            Instant::now(),
            true
        ),
        QueryDisposition::Deferred
    );
    assert!(reply.is_empty());
    let work = service.rx.try_recv().unwrap();
    assert!(work.route_validation.is_some());
    assert!(work.lease().same_job(&lease));
    assert_eq!(work.deadline, lease.deadline());
    service.retry.try_send(work).unwrap();
    let mut calls = 0;
    service.serve_service_query_slice(1, true, Instant::now() + WAIT, |query, until| {
        calls += 1;
        pending(query, until)
    });
    assert_eq!(calls, 1, "late completion must repeat the final callback");
    assert!(reply.is_empty());
}

#[test]
fn invalid_complete_and_real_provider_failure_remain_terminal() {
    let (session, request) = captured_route_request();
    for error in [
        None,
        Some(Failure::MissingGeometry),
        Some(Failure::UnsupportedNavigation),
    ] {
        let (_, service) = mailbox(session.clone()).unwrap();
        let (lease, reply) = enqueue(&service, 1, request.clone());
        validate_first(&service);
        service.serve_service_query_slice(1, true, Instant::now() + WAIT, |_, _| {
            if let Some(error) = error {
                Err(error)
            } else {
                Ok(ServiceStep::Complete(Box::new(Facts::PlayerMovement {
                    accepted: false,
                })))
            }
        });
        assert_eq!(
            reply.try_recv().unwrap().result,
            Err(error.unwrap_or(Failure::Binding))
        );
        assert!(!lease.is_active());
        assert!(service.rx.is_empty());
    }
}

#[test]
fn non_route_and_unselected_route_still_use_borrowed_immediate_validation() {
    let (_, service) = mailbox(tests::session()).unwrap();
    let (lease, reply) = enqueue(&service, 1, tests::request());
    assert_eq!(
        service.serve_service_queries_until(1, true, |query, _| {
            let ServiceQuery::Immediate(validated) = query else {
                panic!("staged Movement")
            };
            assert!(std::ptr::eq(validated.session(), &service.session));
            let Query::Movement(movement) = &validated.request().query else {
                panic!()
            };
            solve(movement, validated.body(), &mut tests::Proof(Ok(())))
                .map(|outcome| ServiceStep::Complete(Box::new(Facts::Movement(outcome))))
        }),
        1
    );
    assert!(reply.try_recv().unwrap().result.is_ok());
    assert!(!lease.is_active());
    let (_, reply) = enqueue(&service, 2, tests::request());
    service.serve_service_queries_until(1, true, |query, _| {
        assert!(matches!(query, ServiceQuery::Immediate(_)));
        Ok(ServiceStep::Pending)
    });
    assert_eq!(
        reply.try_recv().unwrap().result,
        Err(Failure::UnsupportedQuery)
    );
    assert!(service.rx.is_empty());
    let (session, request) = captured_route_request();
    let (_, service) = mailbox(session).unwrap();
    let (_, reply) = enqueue(&service, 1, request);
    service.serve_service_query_slice(1, false, Instant::now() + WAIT, |query, _| {
        assert!(matches!(query, ServiceQuery::Immediate(_)));
        Err(Failure::MissingGeometry)
    });
    assert_eq!(
        reply.try_recv().unwrap().result,
        Err(Failure::MissingGeometry)
    );
    assert!(service.rx.is_empty());
}

#[test]
fn staged_deadline_full_tail_reports_capacity_without_replacing_other_jobs() {
    let (session, request) = captured_route_request();
    let (_, service) = mailbox(session).unwrap();
    let (lease, reply) = enqueue(&service, 99, request.clone());
    validate_first(&service);
    let work = service.rx.try_recv().unwrap();
    let _others: Vec<_> = (0..QUEUE_CAPACITY)
        .map(|ticket| enqueue(&service, ticket as u64, request.clone()))
        .collect();
    assert_eq!(
        service.finish_query_result(work, Err(Failure::Deadline), Instant::now(), true),
        QueryDisposition::Failed
    );
    assert_eq!(
        reply.try_recv().unwrap().result,
        Err(Failure::QueueCapacity)
    );
    assert!(!lease.is_active());
    assert_eq!(service.rx.len(), QUEUE_CAPACITY);
    for ticket in 0..QUEUE_CAPACITY {
        assert_eq!(service.rx.try_recv().unwrap().ticket, ticket as u64);
    }
}

#[test]
fn provider_binding_is_per_original_job_and_foreign_rejection_cannot_replace_it() {
    let (session, request) = captured_route_request();
    let (_, service) = mailbox(session).unwrap();
    let a = Arc::new(());
    let b = Arc::new(());
    let (first, first_reply) = enqueue(&service, 1, request.clone());
    validate_first(&service);
    let bind = |expected: &JobLease, chosen: &Arc<()>, foreign: &Arc<()>| {
        assert_eq!(
            service.serve_service_test_slice(1, true, Instant::now() + WAIT, |query, _| {
                let ServiceQuery::Route(job) = query else {
                    panic!("unstaged Route")
                };
                assert!(job.lease().same_job(expected));
                assert_eq!(job.bind_provider(chosen), Ok(()));
                assert_eq!(job.bind_provider(foreign), Err(Failure::Binding));
                assert_eq!(job.bind_provider(chosen), Ok(()));
                Ok(ServiceStep::Pending)
            }),
            1
        );
    };
    bind(&first, &a, &b);
    let original = service.rx.try_recv().unwrap();
    // The second Work uses the same genuine immutable request and attachment;
    // only its core-owned allocation differs. Provider binding cannot transfer.
    let (second, second_reply) = enqueue(&service, 2, request);
    assert!(!first.same_job(&second));
    validate_first(&service);
    bind(&second, &b, &a);
    service.retry.try_send(original).unwrap();
    bind(&second, &b, &a);
    bind(&first, &a, &b);
    assert!(first.is_active() && second.is_active());
    assert!(first_reply.is_empty() && second_reply.is_empty());
    service.active.store(false, Ordering::Release);
    let retired = service.rx.try_recv().unwrap();
    let job = RouteJob {
        validation: retired.route_validation.as_ref().unwrap(),
    };
    assert_eq!(job.bind_provider(&a), Err(Failure::Deadline));
    assert_eq!(job.bind_provider(&b), Err(Failure::Deadline));
}

#[test]
fn owned_route_fact_validation_rejects_wrong_shapes_and_revoked_original_lease() {
    let (session, request) = captured_route_request();
    let (_, service) = mailbox(session).unwrap();
    let (lease, reply) = enqueue(&service, 1, request.clone());
    validate_first(&service);
    assert_eq!(
        service.serve_service_test_slice(1, true, Instant::now() + WAIT, |query, _| {
            let ServiceQuery::Route(job) = query else {
                panic!("unstaged Route")
            };
            assert!(job.lease().same_job(&lease));
            let wrong_kind = Facts::PlayerMovement { accepted: false };
            assert_eq!(job.validate_facts(&wrong_kind), Err(Failure::Binding));
            let Query::SourceMotion(MotionQuery::Route(route)) = &request.query else {
                unreachable!()
            };
            // Deliberately wrong fact shape, never a manufactured successful Route.
            let wrong_fact = Facts::SourceMotion(MotionFacts::Spatial(Box::new(motion::Reply {
                request: motion::Query::Route(route.request.clone()),
                fact: motion::Fact::NearPoint(route.request.goal),
            })));
            assert_eq!(job.validate_facts(&wrong_fact), Err(Failure::Binding));
            let mut changed = route.request.clone();
            changed.binding.query_id += 1;
            let wrong_request =
                Facts::SourceMotion(MotionFacts::Spatial(Box::new(motion::Reply {
                    request: motion::Query::Route(changed),
                    fact: motion::Fact::NearPoint(route.request.goal),
                })));
            assert_eq!(job.validate_facts(&wrong_request), Err(Failure::Binding));
            assert!(reply.is_empty(), "validation published a partial reply");
            service.active.store(false, Ordering::Release);
            assert_eq!(job.validate_facts(&wrong_kind), Err(Failure::Deadline));
            assert_eq!(job.validate_facts(&wrong_fact), Err(Failure::Deadline));
            Ok(ServiceStep::Pending)
        }),
        1
    );
    assert!(!lease.is_active());
    assert!(service.rx.is_empty());
    assert!(reply.try_recv().is_err());
}

#[test]
fn source_observation_work_retains_exact_resident_request_without_transport_admission() {
    let (mut session, request) = captured_route_request();
    let profile = Arc::clone(
        &session
            .same_polygon_route
            .as_ref()
            .unwrap()
            .environment
            .profile,
    );
    session.same_polygon_route = None;
    let (_, service) = mailbox(session).unwrap();
    let deadline = Instant::now() + WAIT;
    assert!(matches!(
        service.enqueue_service_test_request_until(701, request.clone(), deadline),
        Err(Failure::UnsupportedQuery)
    ));
    let work = service
        .capture_source_observation_test_work(&profile, 701, request.clone(), deadline)
        .unwrap();
    let lease = work.lease();
    assert!(lease.is_active());
    assert_eq!(lease.request(), &request);
    assert_eq!(lease.ticket(), 701);
    assert_eq!(lease.deadline(), deadline);
    assert!(service.rx.is_empty() && service.retry.is_empty());
    assert!(matches!(
        service.enqueue_service_test_request_until(701, request.clone(), deadline),
        Err(Failure::UnsupportedQuery)
    ));
    let mut foreign = request.clone();
    foreign.binding.geometry[0] ^= 1;
    assert!(service
        .capture_source_observation_test_work(&profile, 701, foreign, deadline)
        .is_err());
    drop(work);
    assert!(!lease.is_active());
    assert!(service.rx.is_empty() && service.retry.is_empty());
    let work = service
        .capture_source_observation_test_work(&profile, 701, request, deadline)
        .unwrap();
    let attached = work.lease();
    assert!(!lease.same_job(&attached));
    drop(service);
    assert!(!attached.is_active());
}
