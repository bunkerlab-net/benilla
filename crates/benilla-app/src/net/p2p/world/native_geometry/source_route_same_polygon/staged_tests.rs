// SPDX-License-Identifier: GPL-3.0-or-later
//! Real source calculations; these tests deliberately never fabricate Route facts.
use super::super::core::{self, ServiceQuery, ServiceStep};
use super::{tests::*, *};
use crossbeam_channel::TryRecvError;

fn tick_stage(
    mailbox: &core::Mailbox,
    routes: &mut SourceSamePolygonRouteProvider,
    observations: &mut Vec<Observation>,
) {
    assert_eq!(
        mailbox.serve_service_test_slice(1, true, until(), |query, deadline| {
            let ServiceQuery::Route(job) = query else {
                panic!("Route job")
            };
            assert!(!routes
                .advance_job_observed(&job, 1, deadline, |stage| observations.push(stage))
                .map_err(SamePolygonError::failure)?);
            Ok(ServiceStep::Pending)
        }),
        1
    );
}

#[test]
fn staged_source_order_matches_direct_calculation_and_reuses_actual_candidate() {
    let mut fixture = composed_setup();
    let (context, baseline, expected) = {
        let (world, mut consumer) = consumer(&mut fixture);
        drive_first(world, &mut consumer);
        assert!(consumer.errors.is_empty());
        (
            consumer.requests[0].clone(),
            consumer.proposals.pop().unwrap(),
            consumer.observations,
        )
    };
    let (_authority, mailbox) = core::mailbox(fixture.session.clone()).unwrap();
    let (lease, reply) = mailbox.enqueue_service_test_request(17, context).unwrap();
    let mut observed = vec![];
    for phase in 0..5 {
        tick_stage(&mailbox, &mut fixture.routes, &mut observed);
        assert!(
            matches!(reply.try_result(), Err(TryRecvError::Empty)),
            "phase {phase}"
        );
        assert!(lease.is_active());
        assert_eq!(fixture.routes.queries.constructor_count(), 1);
    }
    assert_eq!(observed, expected);
    assert_eq!(fixture.routes.pending_job_count(), 1);
    assert_eq!(
        mailbox.serve_service_test_slice(1, true, until(), |query, deadline| {
            let ServiceQuery::Route(job) = query else {
                panic!("Route job")
            };
            assert!(fixture.routes.advance_job(&job, 1, deadline).unwrap());
            let staged = fixture.routes.terminal_for_job(&job).unwrap();
            assert!(Rc::ptr_eq(&baseline.candidate, &staged.candidate));
            assert_eq!(baseline.after_cache(), staged.after_cache());
            assert_eq!(baseline.constructor_filter, staged.constructor_filter);
            assert_eq!(baseline.gate, staged.gate);
            assert_eq!(baseline.filter, staged.filter);
            assert_eq!(baseline.tiles, staged.tiles);
            Err(Failure::UnsupportedQuery) // Only concrete app physics may publish.
        }),
        1
    );
    assert!(matches!(
        reply.try_result(),
        Ok(Err(Failure::UnsupportedQuery))
    ));
    assert!(!lease.is_active());
    fixture.routes.reap_jobs();
    assert!(fixture.routes.pending.is_empty());
    assert_eq!(fixture.routes.candidates[0].proposals.len(), 1);
}

#[test]
fn uninstall_at_every_boundary_reaps_jobs_without_evicting_candidates() {
    for boundary in 0..=5 {
        let mut fixture = composed_setup();
        let (session, request) = captured_route_request();
        let (_authority, mailbox) = core::mailbox(session).unwrap();
        let (lease, reply) = mailbox.enqueue_service_test_request(1, request).unwrap();
        for _ in 0..boundary {
            tick_stage(&mailbox, &mut fixture.routes, &mut vec![]);
        }
        assert_eq!(
            fixture.routes.queries.constructor_count(),
            usize::from(boundary >= 3)
        );
        if boundary == 2 {
            assert!(fixture.routes.candidates.is_empty());
        }
        let candidates: usize = fixture
            .routes
            .candidates
            .iter()
            .map(|entry| entry.proposals.len())
            .sum();
        assert_eq!(candidates, usize::from(boundary == 5));
        drop(mailbox);
        assert!(!lease.is_active());
        fixture.routes.reap_jobs();
        assert!(fixture.routes.pending.is_empty());
        assert!(matches!(
            reply.try_result(),
            Err(TryRecvError::Disconnected)
        ));
        assert_eq!(
            fixture
                .routes
                .candidates
                .iter()
                .map(|entry| entry.proposals.len())
                .sum::<usize>(),
            candidates
        );
    }
}

#[test]
fn separate_equal_byte_jobs_reserve_cap_four_until_actual_candidate_is_shared() {
    let mut fixture = composed_setup();
    let (session, request) = captured_route_request();
    let (_authority, mailbox) = core::mailbox(session).unwrap();
    let mut replies = vec![];
    for ticket in 1..=5 {
        replies.push(
            mailbox
                .enqueue_service_test_request(ticket, request.clone())
                .unwrap(),
        );
    }
    // The FIFO validates five bodies and contexts, then prepares four constructors.
    for _ in 0..14 {
        tick_stage(&mailbox, &mut fixture.routes, &mut vec![]);
    }
    assert_eq!(fixture.routes.pending_job_count(), 5);
    assert_eq!(
        fixture.routes.reservations(
            fixture.routes.candidates[0].npc,
            fixture.routes.candidates[0].construction,
        ),
        4
    );
    assert!(fixture.routes.candidates[0].proposals.is_empty());
    assert_eq!(fixture.routes.queries.constructor_count(), 1);
    assert_eq!(
        mailbox.serve_service_test_slice(1, true, until(), |query, deadline| {
            let ServiceQuery::Route(job) = query else {
                panic!("Route job")
            };
            assert_eq!(
                fixture.routes.advance_job(&job, 1, deadline),
                Err(SamePolygonError::CandidateBudget)
            );
            Err(Failure::UnsupportedQuery)
        }),
        1
    );
    assert!(matches!(
        replies[4].1.try_result(),
        Ok(Err(Failure::UnsupportedQuery))
    ));
    assert!(!replies[4].0.is_active());
    // Four gate phases and four native phases retain exactly one real allocation.
    for _ in 0..8 {
        tick_stage(&mailbox, &mut fixture.routes, &mut vec![]);
    }
    assert_eq!(fixture.routes.pending_job_count(), 4);
    assert_eq!(fixture.routes.candidates[0].proposals.len(), 1);
    drop(mailbox);
    fixture.routes.reap_jobs();
    assert!(fixture.routes.pending.is_empty());
    assert_eq!(fixture.routes.candidates[0].proposals.len(), 1);
}

#[test]
fn completed_context_cannot_resume_in_an_equal_byte_foreign_provider() {
    let mut fixture = composed_setup();
    let mut foreign = composed_setup();
    let (session, request) = captured_route_request();
    let (_authority, mailbox) = core::mailbox(session).unwrap();
    let (_lease, reply) = mailbox.enqueue_service_test_request(1, request).unwrap();
    for _ in 0..2 {
        tick_stage(&mailbox, &mut fixture.routes, &mut vec![]);
    }
    assert_eq!(
        mailbox.serve_service_test_slice(1, true, until(), |query, deadline| {
            let ServiceQuery::Route(job) = query else {
                panic!("Route job")
            };
            assert_eq!(
                foreign.routes.advance_job(&job, 1, deadline),
                Err(SamePolygonError::Dependency(Failure::Binding))
            );
            assert!(!fixture.routes.advance_job(&job, 1, deadline).unwrap());
            Ok(ServiceStep::Pending)
        }),
        1
    );
    assert_eq!(fixture.routes.pending_job_count(), 1);
    assert!(foreign.routes.pending.is_empty());
    assert_eq!(foreign.routes.queries.constructor_count(), 0);
    assert!(matches!(reply.try_result(), Err(TryRecvError::Empty)));
}

#[test]
fn expired_gate_attempt_retains_only_successful_prepare_and_original_job_expiry() {
    let mut fixture = composed_setup();
    let (session, request) = captured_route_request();
    let (_authority, mailbox) = core::mailbox(session).unwrap();
    let (lease, reply) = mailbox.enqueue_service_test_request(1, request).unwrap();
    let original_expiry = lease.deadline();
    for _ in 0..3 {
        tick_stage(&mailbox, &mut fixture.routes, &mut vec![]);
    }
    let slice = Instant::now() + std::time::Duration::from_millis(200);
    assert!(slice < original_expiry);
    let mut expired_after_target_water = false;
    assert_eq!(
        mailbox.serve_service_test_slice(1, true, slice, |query, deadline| {
            let ServiceQuery::Route(job) = query else {
                panic!("Route job")
            };
            let result = fixture
                .routes
                .advance_job_observed(&job, 1, deadline, |stage| {
                    if stage == Observation::Gate(GateObservation::TargetWater(false)) {
                        expired_after_target_water = true;
                        std::thread::sleep(slice.saturating_duration_since(Instant::now()));
                    }
                });
            assert_eq!(result, Err(SamePolygonError::Dependency(Failure::Deadline)));
            assert_eq!(job.lease().deadline(), original_expiry);
            result
                .map(|_| ServiceStep::Pending)
                .map_err(SamePolygonError::failure)
        }),
        1
    );
    assert!(expired_after_target_water);
    assert_eq!(fixture.routes.pending_job_count(), 1);
    assert_eq!(fixture.routes.queries.constructor_count(), 1);
    assert!(fixture.routes.candidates[0].proposals.is_empty());
    assert!(matches!(reply.try_result(), Err(TryRecvError::Empty)));
    let mut observed = vec![];
    tick_stage(&mailbox, &mut fixture.routes, &mut observed);
    assert_eq!(observed.first(), Some(&Observation::ConstructorFilter));
    assert_eq!(
        observed.last(),
        Some(&Observation::Gate(GateObservation::TargetWater(false)))
    );
    assert!(fixture.routes.candidates[0].proposals.is_empty());
    std::thread::sleep(original_expiry.saturating_duration_since(Instant::now()));
    fixture.routes.reap_jobs();
    assert!(fixture.routes.pending.is_empty());
    assert!(!lease.is_active());
    assert_eq!(
        mailbox.serve_service_test_slice(1, true, until(), |_, _| panic!("expired callback")),
        1
    );
    assert!(matches!(
        reply.try_result(),
        Err(TryRecvError::Disconnected)
    ));
}

#[test]
fn validated_context_yields_before_constructor_without_reserving_native_capacity() {
    let mut fixture = composed_setup();
    let (session, request) = captured_route_request();
    let (_authority, mailbox) = core::mailbox(session).unwrap();
    let (lease, reply) = mailbox.enqueue_service_test_request(1, request).unwrap();
    let original_expiry = lease.deadline();
    let mut observed = vec![];
    for _ in 0..2 {
        tick_stage(&mailbox, &mut fixture.routes, &mut observed);
    }
    assert!(observed.is_empty());
    assert_eq!(fixture.routes.pending_job_count(), 1);
    assert!(matches!(
        fixture.routes.pending[0].phase,
        staged::Phase::Validated(_)
    ));
    assert!(fixture.routes.candidates.is_empty());
    assert_eq!(fixture.routes.queries.constructor_count(), 0);
    assert_eq!(lease.deadline(), original_expiry);
    assert!(matches!(reply.try_result(), Err(TryRecvError::Empty)));
    tick_stage(&mailbox, &mut fixture.routes, &mut observed);
    assert_eq!(observed, vec![Observation::ConstructorQuery]);
    assert!(matches!(
        fixture.routes.pending[0].phase,
        staged::Phase::Prepared(_)
    ));
    assert_eq!(fixture.routes.queries.constructor_count(), 1);
    assert!(fixture.routes.candidates[0].proposals.is_empty());
    let entry = &fixture.routes.candidates[0];
    assert_eq!(
        fixture.routes.reservations(entry.npc, entry.construction),
        1
    );
    assert_eq!(lease.deadline(), original_expiry);
}

#[test]
fn expired_constructor_retains_only_validated_context_and_reuses_original_claim() {
    let mut fixture = composed_setup();
    let (session, request) = captured_route_request();
    let (_authority, mailbox) = core::mailbox(session).unwrap();
    let (lease, reply) = mailbox.enqueue_service_test_request(1, request).unwrap();
    let original_expiry = lease.deadline();
    for _ in 0..2 {
        tick_stage(&mailbox, &mut fixture.routes, &mut vec![]);
    }
    let slice = Instant::now() + std::time::Duration::from_millis(200);
    assert!(slice < original_expiry);
    let mut expired_after_constructor = false;
    assert_eq!(
        mailbox.serve_service_test_slice(1, true, slice, |query, deadline| {
            let ServiceQuery::Route(job) = query else {
                panic!("Route job")
            };
            let result = fixture
                .routes
                .advance_job_observed(&job, 1, deadline, |stage| {
                    assert_eq!(stage, Observation::ConstructorQuery);
                    expired_after_constructor = true;
                    std::thread::sleep(slice.saturating_duration_since(Instant::now()));
                });
            assert_eq!(result, Err(SamePolygonError::Dependency(Failure::Deadline)));
            result
                .map(|_| ServiceStep::Pending)
                .map_err(SamePolygonError::failure)
        }),
        1
    );
    assert!(expired_after_constructor);
    assert!(matches!(
        fixture.routes.pending[0].phase,
        staged::Phase::Validated(_)
    ));
    assert_eq!(fixture.routes.queries.constructor_count(), 1);
    let entry = &fixture.routes.candidates[0];
    assert!(entry.proposals.is_empty());
    assert_eq!(
        fixture.routes.reservations(entry.npc, entry.construction),
        0
    );
    assert!(matches!(reply.try_result(), Err(TryRecvError::Empty)));
    let mut observed = vec![];
    tick_stage(&mailbox, &mut fixture.routes, &mut observed);
    assert_eq!(observed, vec![Observation::ConstructorQuery]);
    assert!(matches!(
        fixture.routes.pending[0].phase,
        staged::Phase::Prepared(_)
    ));
    assert_eq!(fixture.routes.queries.constructor_count(), 1);
    let entry = &fixture.routes.candidates[0];
    assert_eq!(
        fixture.routes.reservations(entry.npc, entry.construction),
        1
    );
    assert_eq!(lease.deadline(), original_expiry);
    assert!(lease.is_active());
}

#[test]
fn job_adoption_rejects_foreign_profile_player_and_composition_before_native_work() {
    for mutation in 0..3 {
        let mut fixture = composed_setup();
        let (session, request) = captured_route_request();
        let (_authority, mailbox) = core::mailbox(session).unwrap();
        let (_lease, reply) = mailbox.enqueue_service_test_request(1, request).unwrap();
        tick_stage(&mailbox, &mut fixture.routes, &mut vec![]);
        match mutation {
            0 => fixture.routes.profile.tick_ms += 1,
            1 => fixture.routes.player.character[0] ^= 1,
            2 => fixture.routes.binding.raw_geometry[0] ^= 1,
            _ => unreachable!(),
        }
        assert_eq!(
            mailbox.serve_service_test_slice(1, true, until(), |query, deadline| {
                let ServiceQuery::Route(job) = query else {
                    panic!("Route job")
                };
                assert_eq!(
                    fixture.routes.advance_job(&job, 1, deadline),
                    Err(SamePolygonError::Dependency(Failure::Binding))
                );
                Err(Failure::Binding)
            }),
            1
        );
        assert!(matches!(reply.try_result(), Ok(Err(Failure::Binding))));
        assert!(fixture.routes.pending.is_empty());
        assert!(fixture.routes.candidates.is_empty());
        assert_eq!(fixture.routes.queries.constructor_count(), 0);
    }
}
