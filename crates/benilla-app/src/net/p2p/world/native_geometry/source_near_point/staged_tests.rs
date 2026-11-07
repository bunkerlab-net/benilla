// SPDX-License-Identifier: GPL-3.0-or-later
//! Typed jobs consume the same loaded geometry under the original mailbox lease.
use super::core::{ServiceQuery, ServiceStep};
use super::*;
use crossbeam_channel::TryRecvError;

#[test]
fn typed_primary_retries_fresh_geometry_and_matches_the_raw_fact() {
    let (snapshot, config) = loaded();
    let (world, mut session, request) = setup(&snapshot, config);
    session.primary_near_point =
        Some(PrimaryNearPointPolicy::from_profile(world.profile()).unwrap());
    let provider =
        SourceNearPointProvider::bind(&session, world.profile(), snapshot, config).unwrap();
    let expected = provider.resolve(&session, &request, deadline()).unwrap();
    let before = world.checkpoint().unwrap();
    let (_authority, mailbox) = core::mailbox(session.clone()).unwrap();
    let (lease, reply) = mailbox
        .enqueue_service_test_request(1, request.clone())
        .unwrap();
    let original_deadline = lease.deadline();
    assert_eq!(
        mailbox.serve_service_test_slice(1, true, deadline(), |_, _| {
            panic!("completed body validation must yield before geometry")
        }),
        1
    );
    assert!(matches!(reply.try_result(), Err(TryRecvError::Empty)));
    assert_eq!(
        mailbox.serve_service_test_slice(1, true, deadline(), |query, _| {
            let ServiceQuery::NearPoint(job) = query else {
                panic!("NearPoint job")
            };
            assert!(job.session() == &session);
            assert_eq!(job.request(), &request);
            assert!(job.lease().same_job(&lease));
            assert_eq!(job.lease().deadline(), original_deadline);
            assert_eq!(
                provider.resolve_job(job, Instant::now()),
                Err(Failure::Deadline)
            );
            Ok(ServiceStep::Pending)
        }),
        1
    );
    assert!(lease.is_active());
    assert!(matches!(reply.try_result(), Err(TryRecvError::Empty)));
    assert_eq!(
        mailbox.serve_service_test_slice(1, true, deadline(), |query, until| {
            let ServiceQuery::NearPoint(job) = query else {
                panic!("NearPoint job")
            };
            assert!(job.lease().same_job(&lease));
            assert_eq!(job.lease().deadline(), original_deadline);
            let facts = provider.resolve_job(job, until)?;
            assert_eq!(facts, expected);
            Ok(ServiceStep::Complete(Box::new(facts)))
        }),
        1
    );
    let result = reply.try_result().unwrap().unwrap();
    assert_eq!(result, expected);
    assert!(!lease.is_active());
    assert_eq!(world.checkpoint().unwrap(), before);
}

#[test]
fn typed_primary_pins_the_provider_even_when_its_first_geometry_budget_is_expired() {
    let (snapshot, config) = loaded();
    let (world, mut session, request) = setup(&snapshot, config);
    session.primary_near_point =
        Some(PrimaryNearPointPolicy::from_profile(world.profile()).unwrap());
    let provider =
        SourceNearPointProvider::bind(&session, world.profile(), snapshot, config).unwrap();
    let (foreign_snapshot, foreign_config) = loaded();
    let foreign =
        SourceNearPointProvider::bind(&session, world.profile(), foreign_snapshot, foreign_config)
            .unwrap();
    assert!(!Arc::ptr_eq(&provider.identity, &foreign.identity));
    let (_authority, mailbox) = core::mailbox(session).unwrap();
    let (lease, reply) = mailbox.enqueue_service_test_request(1, request).unwrap();
    assert_eq!(
        mailbox.serve_service_test_slice(1, true, deadline(), |_, _| {
            panic!("completed body validation must yield before geometry")
        }),
        1
    );
    assert_eq!(
        mailbox.serve_service_test_slice(1, true, deadline(), |query, _| {
            let ServiceQuery::NearPoint(job) = query else {
                panic!("NearPoint job")
            };
            assert_eq!(
                provider.resolve_job(job, Instant::now()),
                Err(Failure::Deadline)
            );
            Ok(ServiceStep::Pending)
        }),
        1
    );
    assert_eq!(
        mailbox.serve_service_test_slice(1, true, deadline(), |query, until| {
            let ServiceQuery::NearPoint(job) = query else {
                panic!("NearPoint job")
            };
            let result = foreign.resolve_job(job, until);
            assert_eq!(result, Err(Failure::Binding));
            result.map(|facts| ServiceStep::Complete(Box::new(facts)))
        }),
        1
    );
    assert_eq!(reply.try_result().unwrap(), Err(Failure::Binding));
    assert!(!lease.is_active());
}
