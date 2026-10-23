// SPDX-License-Identifier: GPL-3.0-or-later
use super::super::super::super::{
    products::vmap::installed_source_fixture as input,
    source_route_filter::resident::ResidentFilterError,
};
use super::super::tests::{no_native_constructors, with_request};
use super::*;
use benilla_world::terrain_stream::NativeStaticMapIndex;
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use std::thread::ThreadId;

fn until() -> Instant {
    Instant::now() + Duration::from_secs(60)
}
struct GateControl {
    entered: Receiver<ThreadId>,
    release: Sender<()>,
    published: Receiver<()>,
}
impl GateControl {
    fn entered(&self) -> ThreadId {
        self.entered.recv_timeout(Duration::from_secs(60)).unwrap()
    }
    fn release(&self) {
        self.release.try_send(()).unwrap();
    }
    fn published(&self) {
        self.published
            .recv_timeout(Duration::from_secs(60))
            .unwrap();
    }
}
fn held(mailbox: &Mailbox) -> (ResidentFifoSelection, GateControl) {
    let (entered, observe) = bounded(1);
    let (release, resume) = bounded(1);
    let (published, observed) = bounded(1);
    (
        ResidentFifoSelection::with_worker(mailbox, || {
            CaptureFreshnessWorker::with_test_gate(entered, resume, Some(published))
        })
        .unwrap(),
        GateControl {
            entered: observe,
            release,
            published: observed,
        },
    )
}
fn serve_closed(
    selector: &mut ResidentFifoSelection,
    mailbox: &Mailbox,
    owner: &mut OwnedResident,
    limit: usize,
    deadline: Instant,
) -> usize {
    selector.serve(
        mailbox,
        owner,
        limit,
        deadline,
        |_, _, _| panic!("unexpected resident continuation"),
        |_, _| panic!("resident entered ordinary provider"),
    )
}
fn drain_retired(selector: &mut ResidentFifoSelection) {
    let watchdog = until();
    while selector.selected.is_some() {
        assert!(Instant::now() < watchdog);
        selector.maintain();
        std::thread::yield_now();
    }
}
fn finish(
    selector: &mut ResidentFifoSelection,
    mailbox: &Mailbox,
    owner: &mut OwnedResident,
    mut resumed: impl for<'call> FnMut(
        &mut OwnedResident,
        ResidentDispatchJob<'call>,
        Result<FreshnessObservation, CompositionError>,
    ),
) {
    let watchdog = until();
    let mut called = false;
    while !called {
        assert!(Instant::now() < watchdog);
        assert_eq!(
            selector.serve(
                mailbox,
                owner,
                PER_FRAME,
                until(),
                |owner, job, observation| {
                    assert!(!called);
                    called = true;
                    resumed(owner, job, observation);
                },
                |_, _| panic!("resident entered ordinary provider")
            ),
            1
        );
        std::thread::yield_now();
    }
    assert!(selector.selected.is_none());
}

#[test]
fn resident_fifo_preserves_original_identity_capacity_and_attempt_accounting() {
    fn send<T: Send>() {}
    send::<Work>();
    with_request(|_, owner, geometry, mailbox, _, request| {
        assert!(matches!(
            mailbox.enqueue_service_test_request(1, request.clone()),
            Err(Failure::UnsupportedQuery)
        ));
        let decoded = serde_json::from_value(serde_json::to_value(request).unwrap()).unwrap();
        assert!(mailbox
            .enqueue_resident_test_request(owner, 0, decoded, until())
            .is_err());
        assert!(mailbox.rx.is_empty());
        let deadline = until();
        let mut queued = Vec::new();
        for ticket in 1..=QUEUE_CAPACITY {
            queued.push(
                mailbox
                    .enqueue_resident_test_request(owner, ticket as u64, request.clone(), deadline)
                    .unwrap(),
            );
        }
        assert!(matches!(
            mailbox.enqueue_resident_test_request(owner, 99, request.clone(), deadline),
            Err(Failure::QueueCapacity)
        ));
        let first = &queued[0].0;
        let (mut selector, control) = held(mailbox);
        let spawned = std::cell::Cell::new(false);
        assert!(ResidentFifoSelection::with_worker(mailbox, || {
            spawned.set(true);
            CaptureFreshnessWorker::new()
        })
        .is_err());
        assert!(
            !spawned.get(),
            "competing selector must refuse before worker creation"
        );
        assert!(mailbox.active.load(Ordering::Acquire));
        assert_eq!(
            serve_closed(&mut selector, mailbox, owner, usize::MAX, until()),
            1
        );
        control.entered();
        assert_eq!(mailbox.rx.len(), QUEUE_CAPACITY);
        assert!(selector
            .selected
            .as_ref()
            .unwrap()
            .validation
            .lease
            .same_job(first));
        assert_eq!(
            serve_closed(&mut selector, mailbox, owner, usize::MAX, until()),
            PER_FRAME
        );
        for (offset, (_, reply)) in queued.iter().enumerate().skip(1).take(PER_FRAME) {
            let reply = reply.0.try_recv().unwrap();
            assert_eq!(reply.ticket, offset as u64 + 1);
            assert_eq!(&reply.request, request);
            assert!(matches!(reply.result, Err(Failure::QueueCapacity)));
        }
        assert_eq!(mailbox.rx.len(), QUEUE_CAPACITY - PER_FRAME);
        assert_eq!(
            serve_closed(&mut selector, mailbox, owner, usize::MAX, until()),
            PER_FRAME
        );
        assert_eq!(mailbox.rx.len(), 1);
        assert!(first.is_active());
        assert!(matches!(queued[0].1.try_result(), Err(TryRecvError::Empty)));
        control.release();
        finish(&mut selector, mailbox, owner, |_, job, observation| {
            assert!(observation.is_ok());
            assert!(job.lease().same_job(first));
            assert!(std::ptr::eq(job.request(), first.request()));
            assert_eq!(job.lease().ticket(), 1);
            assert_eq!(job.lease().deadline(), deadline);
            assert!(job.deadline() <= deadline);
        });
        let reply = queued[0].1 .0.try_recv().unwrap();
        assert_eq!(reply.ticket, 1);
        assert_eq!(&reply.request, request);
        assert!(matches!(reply.result, Err(Failure::UnsupportedQuery)));
        assert!(!first.is_active());
        assert!(mailbox.rx.is_empty());
        assert_eq!(
            serve_closed(&mut selector, mailbox, owner, usize::MAX, until()),
            0
        );
        no_native_constructors(owner, geometry);
    });
}

#[test]
fn resident_fifo_tail_fill_retires_exact_work_without_releasing_busy_worker() {
    with_request(|_, owner, geometry, mailbox, _, request| {
        let mut queued = Vec::new();
        for ticket in 1..=QUEUE_CAPACITY {
            queued.push(
                mailbox
                    .enqueue_resident_test_request(owner, ticket as u64, request.clone(), until())
                    .unwrap(),
            );
        }
        let original = queued[0].0.clone();
        let (mut selector, control) = held(mailbox);
        let mut filled = false;
        let mut extra = None;
        let mut resume =
            |_: &mut OwnedResident,
             _: ResidentDispatchJob<'_>,
             _: Result<FreshnessObservation, CompositionError>| {
                panic!("tail fill must not resume")
            };
        assert_eq!(
            mailbox.serve_service_query_slice_selected(
                PER_FRAME,
                true,
                until(),
                |_, _| panic!("ordinary provider"),
                Some(&mut |work, slice| {
                    assert!(work.lease().same_job(&original));
                    assert!(std::ptr::eq(&work.request, original.request()));
                    let result = selector.advance(mailbox, work, owner, slice, &mut resume);
                    assert!(!filled);
                    extra = Some(
                        mailbox
                            .enqueue_resident_test_request(owner, 99, request.clone(), until())
                            .unwrap(),
                    );
                    filled = true;
                    result
                })
            ),
            1
        );
        control.entered();
        assert_eq!(mailbox.rx.len(), QUEUE_CAPACITY);
        assert!(!original.is_active());
        let reply = queued[0].1 .0.try_recv().unwrap();
        assert_eq!(reply.ticket, 1);
        assert!(matches!(reply.result, Err(Failure::QueueCapacity)));
        selector.maintain();
        assert!(selector.selected.as_ref().unwrap().occupied);
        assert!(selector.selected.as_ref().unwrap().cancelled);
        while let Ok(work) = mailbox.rx.try_recv() {
            drop(work);
        }
        assert_eq!(
            serve_closed(&mut selector, mailbox, owner, PER_FRAME, until()),
            0
        );
        assert!(selector.selected.as_ref().unwrap().occupied);
        assert!(control.entered.try_recv().is_err());
        control.release();
        drain_retired(&mut selector);
        assert!(!selector.stopped);
        assert!(!extra.unwrap().0.is_active());
        no_native_constructors(owner, geometry);
    });
}

#[test]
fn resident_fifo_cancellation_holds_slot_until_drain_and_reuses_one_worker() {
    with_request(|_, owner, geometry, mailbox, _, request| {
        let (mut selector, control) = held(mailbox);
        let (lease, reply) = mailbox
            .enqueue_resident_test_request(owner, 1, request.clone(), until())
            .unwrap();
        assert_eq!(
            serve_closed(&mut selector, mailbox, owner, PER_FRAME, until()),
            1
        );
        let thread = control.entered();
        selector.cancel();
        assert_eq!(
            serve_closed(&mut selector, mailbox, owner, PER_FRAME, until()),
            1
        );
        assert!(matches!(reply.try_result(), Ok(Err(Failure::Binding))));
        assert!(!lease.is_active());
        assert!(selector.selected.as_ref().unwrap().occupied);
        let (replacement, reply) = mailbox
            .enqueue_resident_test_request(owner, 2, request.clone(), until())
            .unwrap();
        assert_eq!(
            serve_closed(&mut selector, mailbox, owner, PER_FRAME, until()),
            1
        );
        assert!(matches!(
            reply.try_result(),
            Ok(Err(Failure::QueueCapacity))
        ));
        assert!(!replacement.is_active());
        assert!(control.entered.try_recv().is_err());
        control.release();
        drain_retired(&mut selector);
        control.published();
        let (lease, reply) = mailbox
            .enqueue_resident_test_request(owner, 3, request.clone(), until())
            .unwrap();
        assert_eq!(
            serve_closed(&mut selector, mailbox, owner, PER_FRAME, until()),
            1
        );
        assert_eq!(thread, control.entered());
        control.release();
        control.published();
        finish(&mut selector, mailbox, owner, |_, job, result| {
            assert!(job.lease().same_job(&lease));
            assert!(result.is_ok());
        });
        assert!(matches!(
            reply.try_result(),
            Ok(Err(Failure::UnsupportedQuery))
        ));
        let (lease, reply) = mailbox
            .enqueue_resident_test_request(owner, 4, request.clone(), until())
            .unwrap();
        assert_eq!(
            serve_closed(&mut selector, mailbox, owner, PER_FRAME, until()),
            1
        );
        assert_eq!(thread, control.entered());
        control.release();
        control.published();
        assert!(selector.selected.as_ref().unwrap().occupied);
        assert!(matches!(reply.try_result(), Err(TryRecvError::Empty)));
        // Completion alone cannot free the slot or admit a continuation. Cancel
        // before the exact queued Work polls its completed-but-undrained result.
        selector.cancel();
        assert_eq!(
            serve_closed(&mut selector, mailbox, owner, PER_FRAME, until()),
            1
        );
        assert!(matches!(reply.try_result(), Ok(Err(Failure::Binding))));
        assert!(!lease.is_active());
        drain_retired(&mut selector);
        let (_, attachment) = super::super::super::mailbox(mailbox.session.clone()).unwrap();
        let (foreign_lease, foreign_reply) = attachment
            .enqueue_resident_test_request(owner, 5, request.clone(), until())
            .unwrap();
        assert_eq!(
            serve_closed(&mut selector, &attachment, owner, PER_FRAME, until()),
            1
        );
        assert!(matches!(
            foreign_reply.try_result(),
            Ok(Err(Failure::Binding))
        ));
        assert!(!foreign_lease.is_active());
        assert!(control.entered.try_recv().is_err());
        let (mut detached, detached_control) = held(&attachment);
        let (lease, reply) = attachment
            .enqueue_resident_test_request(owner, 4, request.clone(), until())
            .unwrap();
        assert_eq!(
            serve_closed(&mut detached, &attachment, owner, PER_FRAME, until()),
            1
        );
        detached_control.entered();
        drop(attachment);
        assert!(!lease.is_active());
        assert!(matches!(
            reply.try_result(),
            Err(TryRecvError::Disconnected)
        ));
        detached.maintain();
        assert!(detached.selected.as_ref().unwrap().occupied);
        detached_control.release();
        drain_retired(&mut detached);
        no_native_constructors(owner, geometry);
    });
}

#[test]
fn resident_fifo_expiry_foreign_owner_and_attachment_never_resume() {
    with_request(|_, owner, geometry, mailbox, _, request| {
        let (mut selector, control) = held(mailbox);
        let deadline = Instant::now() + Duration::from_secs(1);
        let (lease, _) = mailbox
            .enqueue_resident_test_request(owner, 1, request.clone(), deadline)
            .unwrap();
        assert_eq!(
            serve_closed(&mut selector, mailbox, owner, PER_FRAME, until()),
            1
        );
        control.entered();
        assert_eq!(
            serve_closed(&mut selector, mailbox, owner, PER_FRAME, Instant::now()),
            1
        );
        assert_eq!(mailbox.rx.len(), 1);
        assert_eq!(lease.deadline(), deadline);
        std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        assert_eq!(
            serve_closed(&mut selector, mailbox, owner, PER_FRAME, until()),
            1
        );
        assert!(!lease.is_active());
        control.release();
        drain_retired(&mut selector);

        let (lease, reply) = mailbox
            .enqueue_resident_test_request(owner, 2, request.clone(), until())
            .unwrap();
        assert_eq!(
            serve_closed(&mut selector, mailbox, owner, PER_FRAME, until()),
            1
        );
        control.entered();
        with_request(|_, foreign, _, _, _, _| {
            assert_eq!(
                serve_closed(&mut selector, mailbox, foreign, PER_FRAME, until()),
                1
            );
        });
        assert!(matches!(reply.try_result(), Ok(Err(Failure::Binding))));
        assert!(!lease.is_active());
        control.release();
        drain_retired(&mut selector);

        let (lease, reply) = mailbox
            .enqueue_resident_test_request(owner, 3, request.clone(), until())
            .unwrap();
        let (_, foreign_mailbox) = super::super::super::mailbox(mailbox.session.clone()).unwrap();
        // Move the original queued Work, never construct a replacement. A foreign
        // attachment must retire it before either selector or ordinary callback.
        foreign_mailbox
            .retry
            .try_send(mailbox.rx.try_recv().unwrap())
            .ok()
            .unwrap();
        assert_eq!(
            serve_closed(&mut selector, &foreign_mailbox, owner, PER_FRAME, until()),
            1
        );
        assert!(!lease.is_active());
        assert!(matches!(
            reply.try_result(),
            Err(TryRecvError::Disconnected)
        ));
        assert!(selector.selected.is_none());
        no_native_constructors(owner, geometry);
    });
}

#[test]
fn resident_fifo_disconnect_stays_terminal_and_drop_revokes_without_joining() {
    with_request(|_, owner, geometry, mailbox, _, request| {
        let (mut selector, control) = held(mailbox);
        let (_, reply) = mailbox
            .enqueue_resident_test_request(owner, 1, request.clone(), until())
            .unwrap();
        assert_eq!(
            serve_closed(&mut selector, mailbox, owner, PER_FRAME, until()),
            1
        );
        control.entered();
        drop(control.release);
        let watchdog = until();
        while !selector.stopped {
            assert!(Instant::now() < watchdog);
            assert_eq!(
                serve_closed(&mut selector, mailbox, owner, PER_FRAME, until()),
                1
            );
            std::thread::yield_now();
        }
        assert!(matches!(reply.try_result(), Ok(Err(Failure::Binding))));
        let (_, reply) = mailbox
            .enqueue_resident_test_request(owner, 2, request.clone(), until())
            .unwrap();
        assert_eq!(
            serve_closed(&mut selector, mailbox, owner, PER_FRAME, until()),
            1
        );
        assert!(matches!(reply.try_result(), Ok(Err(Failure::Binding))));
        assert!(matches!(
            control.entered.try_recv(),
            Err(TryRecvError::Disconnected)
        ));
        let (_, attachment) = super::super::super::mailbox(mailbox.session.clone()).unwrap();
        let (mut detached, held_control) = held(&attachment);
        let (lease, reply) = attachment
            .enqueue_resident_test_request(owner, 3, request.clone(), until())
            .unwrap();
        assert_eq!(
            serve_closed(&mut detached, &attachment, owner, PER_FRAME, until()),
            1
        );
        held_control.entered();
        // Drop returns while the worker is still gated and immediately retires
        // this installation. No timing claim or manual Work removal substitutes
        // for teardown: replacement refuses before spawning; the ordinary loop
        // only retires this exact old queued Work.
        drop(detached);
        assert!(!lease.is_active());
        assert!(
            mailbox.active.load(Ordering::Acquire),
            "other installation remains live"
        );
        assert_eq!(attachment.rx.len(), 1);
        assert!(attachment
            .enqueue_resident_test_request(owner, 4, request.clone(), until())
            .is_err());
        let spawned = std::cell::Cell::new(false);
        assert!(ResidentFifoSelection::with_worker(&attachment, || {
            spawned.set(true);
            CaptureFreshnessWorker::new()
        })
        .is_err());
        assert!(
            !spawned.get(),
            "retired installation must refuse before worker creation"
        );
        assert_eq!(
            attachment.serve_service_test_slice(PER_FRAME, true, until(), |_, _| panic!(
                "retired Work entered ordinary service"
            )),
            1
        );
        assert!(attachment.rx.is_empty());
        assert!(matches!(
            reply.try_result(),
            Err(TryRecvError::Disconnected)
        ));
        held_control.release();
        let (_, failed_attachment) = super::super::super::mailbox(mailbox.session.clone()).unwrap();
        let called = std::cell::Cell::new(0);
        assert!(ResidentFifoSelection::with_worker(&failed_attachment, || {
            called.set(called.get() + 1);
            Err(std::io::Error::other("fixture worker creation failure"))
        })
        .is_err());
        assert_eq!(called.get(), 1);
        assert!(!failed_attachment.active.load(Ordering::Acquire));
        assert!(failed_attachment.resident_selector.get().is_some());
        assert!(ResidentFifoSelection::with_worker(&failed_attachment, || {
            called.set(called.get() + 1);
            CaptureFreshnessWorker::new()
        })
        .is_err());
        assert_eq!(
            called.get(),
            1,
            "failed creation cannot renew the attachment"
        );
        assert!(
            mailbox.active.load(Ordering::Acquire),
            "failed creation does not retire another attachment"
        );
        no_native_constructors(owner, geometry);
    });
}

#[test]
fn resident_fifo_ready_rechecks_foreign_source_owner_and_changed_source_without_ack() {
    with_request(|install, owner, geometry, mailbox, _, request| {
        let installed = geometry
            .app
            .world()
            .resource::<NativeStaticMapIndex>()
            .source_provenance()
            .unwrap()
            .clone();
        let before = owner
            .inspect(
                &installed,
                until(),
                || false,
                |_, _, ledger| Ok(ledger.diagnostic()),
            )
            .unwrap();
        let mut selector = ResidentFifoSelection::new(mailbox).unwrap();
        for mode in 0..2 {
            let (_, reply) = mailbox
                .enqueue_resident_test_request(owner, mode + 1, request.clone(), until())
                .unwrap();
            assert_eq!(
                serve_closed(&mut selector, mailbox, owner, PER_FRAME, until()),
                1
            );
            finish(&mut selector, mailbox, owner, |owner, job, observation| {
                let observation = observation.unwrap();
                if mode == 0 {
                    let foreign =
                        NativeStaticMapIndex::from_inventory(input::raw_inventory(install))
                            .unwrap();
                    assert!(!installed.same_owner(foreign.source_provenance().unwrap()));
                    let foreign_owner = foreign.source_provenance().unwrap().clone();
                    geometry.advance_physics();
                    let result = owner.certify_observed(
                        observation,
                        job,
                        geometry.app.world_mut(),
                        &foreign_owner,
                    );
                    assert!(matches!(
                        result,
                        Err(ResidentFilterError::Context(Failure::Binding))
                    ));
                } else {
                    let path = install.path().join("model.MPQ");
                    let original = std::fs::read(&path).unwrap();
                    install.write(&input::overlapping_entries());
                    geometry.advance_physics();
                    let result = owner.certify_observed(
                        observation,
                        job,
                        geometry.app.world_mut(),
                        &installed,
                    );
                    std::fs::write(path, original).unwrap();
                    assert!(matches!(result, Err(ResidentFilterError::Source(_))));
                }
            });
            assert!(matches!(
                reply.try_result(),
                Ok(Err(Failure::UnsupportedQuery))
            ));
            owner
                .inspect(
                    &installed,
                    until(),
                    || false,
                    |provider, _, ledger| {
                        assert_eq!(ledger.diagnostic(), before);
                        assert_eq!(ledger.counts(), (0, 0));
                        assert_eq!(provider.retained_constructor_count(), 0);
                        Ok(())
                    },
                )
                .unwrap();
        }
        // A genuine verification failure remains distinguishable from a
        // cancelled result when the actual queued Work consumes it.
        let (_, failed_attachment) = super::super::super::mailbox(mailbox.session.clone()).unwrap();
        let (mut failed, control) = held(&failed_attachment);
        let (_, reply) = failed_attachment
            .enqueue_resident_test_request(owner, 3, request.clone(), until())
            .unwrap();
        assert_eq!(
            serve_closed(&mut failed, &failed_attachment, owner, PER_FRAME, until()),
            1
        );
        control.entered();
        let path = install.path().join("model.MPQ");
        let original = std::fs::read(&path).unwrap();
        install.write(&input::overlapping_entries());
        control.release();
        control.published();
        finish(&mut failed, &failed_attachment, owner, |_, _, result| {
            let error = result.err().expect("changed source must fail verification");
            assert!(!matches!(error, CompositionError::Cancelled));
        });
        std::fs::write(path, original).unwrap();
        assert!(matches!(
            reply.try_result(),
            Ok(Err(Failure::UnsupportedQuery))
        ));
        no_native_constructors(owner, geometry);
    });
}
