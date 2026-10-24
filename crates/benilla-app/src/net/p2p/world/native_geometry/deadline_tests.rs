// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn work(service: &Mailbox, ticket: u64, deadline: Instant) -> (Work, Receiver<Reply>) {
    let (reply, rx) = bounded(1);
    (
        Work::new(ticket, request(), deadline, reply, service.active.clone()),
        rx,
    )
}

fn facts() -> Facts {
    Facts::Movement(solve(&movement(), body(), &mut Proof(Ok(()))).unwrap())
}

#[test]
fn expired_slice_retains_exact_ticket_request_reply_and_absolute_job_expiry() {
    let (mut authority, service) = mailbox(session()).unwrap();
    authority.ticket = 7;
    let deadline = Instant::now() + WAIT;
    let (job, rx) = work(&service, 7, deadline);
    authority.tx.try_send(job).unwrap();
    assert_eq!(
        service.serve_query_slice(PER_FRAME, Instant::now(), |_, _, _, until| {
            assert!(Instant::now() >= until);
            Err(Failure::Deadline)
        }),
        1
    );
    assert!(rx.is_empty());
    let pending = service.rx.try_recv().unwrap();
    assert_eq!(pending.ticket, 7);
    assert_eq!(pending.request, request());
    assert_eq!(pending.deadline, deadline);
    service.retry.try_send(pending).unwrap();
    assert_eq!(
        service.serve_query_slice(PER_FRAME, deadline, |_, context, _, until| {
            assert_eq!(*context, request());
            assert_eq!(until, deadline);
            Ok(facts())
        }),
        1
    );
    let reply = receive_reply_until(&rx, &authority.cancelled, deadline).unwrap();
    assert_eq!(
        authority.finish_reply(&request(), reply).unwrap().facts,
        facts()
    );
    assert!(service.rx.is_empty());
    assert!(rx.is_empty());
}

#[test]
fn deferred_work_goes_behind_other_jobs_and_every_attempt_spends_frame_count() {
    let (authority, service) = mailbox(session()).unwrap();
    let deadline = Instant::now() + WAIT;
    let (first, first_rx) = work(&service, 1, deadline);
    let (second, _second_rx) = work(&service, 2, deadline);
    authority.tx.try_send(first).unwrap();
    authority.tx.try_send(second).unwrap();
    let mut frame = FrameBudget::default();
    for attempt in 0..PER_FRAME {
        let used =
            service.serve_query_slice(frame.remaining(1), Instant::now(), |_, _, _, until| {
                assert!(Instant::now() >= until);
                Err(Failure::Deadline)
            });
        assert_eq!(used, 1);
        frame.spend(used);
        let next = service.rx.try_recv().unwrap();
        assert_eq!(next.ticket, if attempt % 2 == 0 { 2 } else { 1 });
        // Put it back at the head by restoring the two-item FIFO in order.
        let last = service.rx.try_recv().unwrap();
        service.retry.try_send(next).unwrap();
        service.retry.try_send(last).unwrap();
    }
    assert_eq!(frame.remaining(1), 0);
    assert_eq!(
        service.serve_query_slice(0, deadline, |_, _, _, _| panic!("frame exhausted")),
        0
    );
    assert_eq!(frame.remaining(2), PER_FRAME);
    assert_eq!(service.rx.len(), 2);
    assert!(first_rx.is_empty());
}

#[test]
fn concurrent_refill_cannot_expand_queue_or_discard_another_job() {
    let (authority, service) = mailbox(session()).unwrap();
    let deadline = Instant::now() + WAIT;
    let (deferred, rx) = work(&service, 99, deadline);
    let mut replies = vec![];
    for ticket in 0..QUEUE_CAPACITY {
        let (job, reply) = work(&service, ticket as u64, deadline);
        authority.tx.try_send(job).unwrap();
        replies.push(reply);
    }
    assert!(!service.finish_query(deferred, Err(Failure::Deadline), Instant::now()));
    assert_eq!(service.rx.len(), QUEUE_CAPACITY);
    assert_eq!(rx.try_recv().unwrap().result, Err(Failure::Deadline));
    for ticket in 0..QUEUE_CAPACITY {
        assert_eq!(service.rx.try_recv().unwrap().ticket, ticket as u64);
    }
    assert!(replies.iter().all(Receiver::is_empty));
}

#[test]
fn real_failures_are_never_reclassified_or_retried_when_the_slice_also_expired() {
    let (_, service) = mailbox(session()).unwrap();
    for failure in [
        Failure::MissingGeometry,
        Failure::Binding,
        Failure::Invalid,
        Failure::UnsupportedNavigation,
        Failure::Obstacle,
    ] {
        let (job, rx) = work(&service, 1, Instant::now() + WAIT);
        assert!(!service.finish_query(job, Err(failure), Instant::now()));
        assert_eq!(rx.try_recv().unwrap().result, Err(failure));
        assert!(service.rx.is_empty());
    }
}

#[test]
fn actual_unloaded_product_result_remains_missing_when_delivery_slice_expires() {
    use super::super::super::products::{self, liquid::LiquidRequest};
    let products = products::authored_route_smoothing();
    let snapshot = products.load_dry_liquid().unwrap();
    let deadline = Instant::now() + WAIT;
    let missing = snapshot.query(
        LiquidRequest {
            snapshot: snapshot.binding(),
            ticket: 1,
            position: [10_000f32, 10_000., 0.].map(f32::to_bits),
        },
        deadline,
    );
    assert_eq!(missing, Err(products::Error::Missing));
    let result = missing.map(|_| facts()).map_err(Failure::product);
    let (_, service) = mailbox(session()).unwrap();
    let (job, rx) = work(&service, 1, deadline);
    assert!(!service.finish_query(job, result, Instant::now()));
    assert_eq!(rx.try_recv().unwrap().result, Err(Failure::MissingGeometry));
    assert!(service.rx.is_empty());
}

#[test]
fn successful_generic_callback_retains_original_job_deadline_semantics() {
    let (_, service) = mailbox(session()).unwrap();
    let (job, rx) = work(&service, 1, Instant::now() + WAIT);
    assert!(!service.finish_query(job, Ok(facts()), Instant::now()));
    assert_eq!(rx.try_recv().unwrap().result, Ok(facts()));
    assert!(service.rx.is_empty());
}

#[test]
fn deadline_without_actual_slice_expiry_is_terminal_and_cannot_spin() {
    let (_, service) = mailbox(session()).unwrap();
    let deadline = Instant::now() + WAIT;
    let (job, rx) = work(&service, 1, deadline);
    assert!(!service.finish_query(job, Err(Failure::Deadline), deadline));
    assert_eq!(rx.try_recv().unwrap().result, Err(Failure::Deadline));
    assert!(service.rx.is_empty());
}

#[test]
fn absolute_job_expiry_stops_deferred_work_without_geometry_or_reply() {
    let (authority, service) = mailbox(session()).unwrap();
    let expired = Instant::now();
    let (job, rx) = work(&service, 1, expired);
    authority.tx.try_send(job).unwrap();
    assert_eq!(
        service.serve_query_slice(PER_FRAME, Instant::now() + WAIT, |_, _, _, _| {
            panic!("expired job")
        }),
        1
    );
    assert!(service.rx.is_empty());
    assert!(rx.try_recv().is_err());
    let (job, rx) = work(&service, 2, expired);
    assert!(!service.finish_query(job, Err(Failure::Deadline), expired));
    assert!(service.rx.is_empty());
    assert!(rx.try_recv().is_err());
    let (reply, rx) = bounded(1);
    reply
        .try_send(Reply {
            #[cfg(all(test, not(benilla_headless_geometry)))]
            resident: None,
            ticket: 1,
            request: request(),
            result: Ok(facts()),
        })
        .unwrap();
    assert!(matches!(
        receive_reply_until(&rx, &authority.cancelled, expired),
        Err(NativeError::AuthorityUnavailable(
            "local geometry request deadline exceeded"
        ))
    ));
}

#[test]
fn cancellation_between_attempts_drops_pending_work_and_cannot_transfer_to_replacement() {
    let (mut authority, service) = mailbox(session()).unwrap();
    let deadline = Instant::now() + WAIT;
    let (job, rx) = work(&service, 1, deadline);
    authority.tx.try_send(job).unwrap();
    service.serve_query_slice(1, Instant::now(), |_, _, _, until| {
        assert!(Instant::now() >= until);
        Err(Failure::Deadline)
    });
    assert_eq!(service.rx.len(), 1);
    drop(service);
    assert!(receive_reply_until(&rx, &authority.cancelled, deadline).is_err());
    assert!(authority.resolve(&request()).is_err());
    let (_, replacement) = mailbox(session()).unwrap();
    assert!(replacement.rx.is_empty());
}

#[test]
fn only_explicit_deadline_provenance_is_retryable() {
    use super::super::super::{
        products, source_pathfinder_query::ConstructorQueryError,
        source_route_same_polygon::SamePolygonError,
    };
    assert_eq!(
        Failure::product(products::Error::Deadline),
        Failure::Deadline
    );
    for error in [
        products::Error::Budget,
        products::Error::Missing,
        products::Error::Identity,
    ] {
        assert_eq!(Failure::product(error), Failure::MissingGeometry);
    }
    assert_eq!(
        Failure::navigation(benilla_native_nav::Error::Deadline),
        Failure::Deadline
    );
    assert_eq!(
        Failure::navigation(benilla_native_nav::Error::Budget),
        Failure::MissingGeometry
    );
    assert_eq!(
        SamePolygonError::Constructor(ConstructorQueryError::Native(
            benilla_native_nav::Error::Deadline
        ))
        .failure(),
        Failure::Deadline
    );
    assert_eq!(
        SamePolygonError::Native(benilla_native_nav::SourceSamePolygonError::Boundary(
            benilla_native_nav::Error::Deadline
        ))
        .failure(),
        Failure::Deadline
    );
    assert_eq!(
        SamePolygonError::Native(benilla_native_nav::SourceSamePolygonError::Boundary(
            benilla_native_nav::Error::Budget
        ))
        .failure(),
        Failure::MissingGeometry
    );
}
