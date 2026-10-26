// SPDX-License-Identifier: GPL-3.0-or-later
use super::super::super::super::super::{
    core::{mailbox, Mailbox, ServiceTestReply},
    source_route_same_polygon::tests::captured_route_request,
};
use super::super::super::super::{
    vmap::{installed_source_fixture as input, prepared::PrepareError},
    Error,
};
use super::*;
use benilla_native_vmap_source::{MapBundleError, SourceError};
use std::time::{Duration, Instant};

const WATCHDOG: Duration = Duration::from_secs(30);

fn capture(install: &input::Install) -> Arc<OwnedSourceProducts> {
    let terrain =
        benilla_native_terrain::prepare(install.path(), 0, &input::guard_source_tiles()).unwrap();
    let collision = benilla_native_vmap_source::prepare(install.path(), 0, || false)
        .unwrap()
        .into_map_bundle(input::profile(), || false)
        .unwrap()
        .unwrap();
    Arc::new(OwnedSourceProducts::new(terrain, collision, || false).unwrap())
}

fn request(lifetime: Duration) -> (Mailbox, JobLease, ServiceTestReply) {
    let (session, request) = captured_route_request();
    let (_, mailbox) = mailbox(session).unwrap();
    let (lease, reply) = mailbox
        .enqueue_service_test_request_until(17, request, Instant::now() + lifetime)
        .unwrap();
    (mailbox, lease, reply)
}

struct Control {
    entered: Receiver<thread::ThreadId>,
    release: Sender<()>,
}
fn gate() -> (Gate, Control) {
    let (entered, ready) = bounded(1);
    let (release, resume) = bounded(1);
    (
        Gate {
            entered,
            release: resume,
        },
        Control {
            entered: ready,
            release,
        },
    )
}
impl Control {
    fn wait(&self) -> thread::ThreadId {
        self.entered.recv_timeout(WATCHDOG).unwrap()
    }
    fn resume(&self) {
        self.release.try_send(()).unwrap();
    }
}

fn ready(worker: &mut CaptureFreshnessWorker) -> Result<FreshnessObservation, CompositionError> {
    match worker.try_poll() {
        FreshnessPoll::Ready(result) => result,
        _ => panic!("published completion was not ready"),
    }
}

fn observed(
    products: &Arc<OwnedSourceProducts>,
    lease: &JobLease,
) -> (CaptureFreshnessWorker, FreshnessObservation) {
    let (published, receive) = bounded(1);
    let mut worker = CaptureFreshnessWorker::with_hooks(WorkerHooks {
        published: Some(published),
        ..Default::default()
    })
    .unwrap();
    worker
        .try_enqueue(Arc::clone(products), lease.clone())
        .unwrap();
    receive.recv_timeout(WATCHDOG).unwrap();
    let observation = ready(&mut worker).unwrap();
    (worker, observation)
}

#[test]
fn captures_are_send_sync_and_caller_operations_finish_while_worker_is_held() {
    fn send_sync_static<T: Send + Sync + 'static>() {}
    send_sync_static::<benilla_native_terrain::PreparedTerrain>();
    send_sync_static::<benilla_native_vmap_source::AssembledSourceMap<'static>>();
    send_sync_static::<OwnedSourceProducts>();
    send_sync_static::<Arc<OwnedSourceProducts>>();
    send_sync_static::<JobLease>();
    let install = input::install();
    let before = install.bytes();
    let products = capture(&install);
    let (_mailbox, lease, reply) = request(WATCHDOG);
    let deadline = lease.deadline();
    let (hold_before, control) = gate();
    let (counts, measured) = bounded(1);
    let (published, receive) = bounded(1);
    let worker = CaptureFreshnessWorker::with_hooks(WorkerHooks {
        before_verify: Some(hold_before),
        preparation_counts: Some(counts),
        published: Some(published),
        ..Default::default()
    })
    .unwrap();
    let (returned, done) = bounded(1);
    let caller_products = Arc::clone(&products);
    let caller_lease = lease.clone();
    thread::spawn(move || {
        let mut worker = worker;
        worker.try_enqueue(caller_products, caller_lease).unwrap();
        returned.send(worker).unwrap();
    });
    control.wait();
    // The caller completed enqueue even though source verification cannot proceed.
    let mut worker = done.recv_timeout(WATCHDOG).unwrap();
    let (returned, done) = bounded(1);
    let caller_products = Arc::clone(&products);
    let caller_lease = lease.clone();
    thread::spawn(move || {
        assert!(matches!(worker.try_poll(), FreshnessPoll::Pending));
        assert_eq!(
            worker.try_enqueue(caller_products, caller_lease),
            Err(EnqueueError::Busy)
        );
        returned.send(worker).unwrap();
    });
    // Both nonblocking caller operations finish before releasing verification.
    let mut worker = done.recv_timeout(WATCHDOG).unwrap();
    assert!(
        reply.try_result().is_err(),
        "freshness published authority facts"
    );
    control.resume();
    receive.recv_timeout(WATCHDOG).unwrap();
    assert_eq!(measured.recv_timeout(WATCHDOG).unwrap(), Default::default());
    ready(&mut worker)
        .unwrap()
        .consume(&products, &lease)
        .unwrap();
    assert!(matches!(worker.try_poll(), FreshnessPoll::Idle));
    assert_eq!(lease.deadline(), deadline);
    assert!(
        reply.try_result().is_err(),
        "freshness published authority facts"
    );
    assert_eq!(install.bytes(), before);
}

#[test]
fn one_slot_includes_completed_undrained_work_and_reuses_the_same_worker() {
    let install = input::install();
    let products = capture(&install);
    let (_mailbox, lease, _) = request(WATCHDOG);
    let (hold_before, control) = gate();
    let (published, receive) = bounded(1);
    let mut worker = CaptureFreshnessWorker::with_hooks(WorkerHooks {
        before_verify: Some(hold_before),
        published: Some(published),
        ..Default::default()
    })
    .unwrap();
    assert!(matches!(worker.try_poll(), FreshnessPoll::Idle));
    let mut first_thread = None;
    for iteration in 0..3 {
        worker
            .try_enqueue(Arc::clone(&products), lease.clone())
            .unwrap();
        let worker_thread = control.wait();
        assert_ne!(worker_thread, thread::current().id());
        assert_eq!(*first_thread.get_or_insert(worker_thread), worker_thread);
        for _ in 0..8 {
            assert!(matches!(worker.try_poll(), FreshnessPoll::Pending));
            assert_eq!(
                worker.try_enqueue(Arc::clone(&products), lease.clone()),
                Err(EnqueueError::Busy)
            );
        }
        if iteration == 1 {
            worker.cancel();
        }
        control.resume();
        receive.recv_timeout(WATCHDOG).unwrap();
        assert_eq!(
            worker.try_enqueue(Arc::clone(&products), lease.clone()),
            Err(EnqueueError::Busy)
        );
        if iteration == 1 {
            assert!(matches!(
                ready(&mut worker),
                Err(CompositionError::Cancelled)
            ));
        } else {
            ready(&mut worker)
                .unwrap()
                .consume(&products, &lease)
                .unwrap();
        }
    }
}

#[test]
fn cancellation_retirement_and_original_expiry_do_not_release_a_held_slot() {
    let install = input::install();
    let products = capture(&install);
    for mode in 0..3 {
        let (mailbox, lease, _) = request(if mode == 2 {
            Duration::from_millis(500)
        } else {
            WATCHDOG
        });
        let mut mailbox = Some(mailbox);
        let deadline = lease.deadline();
        let (hold_before, control) = gate();
        let (published, receive) = bounded(1);
        let mut worker = CaptureFreshnessWorker::with_hooks(WorkerHooks {
            before_verify: Some(hold_before),
            published: Some(published),
            ..Default::default()
        })
        .unwrap();
        worker
            .try_enqueue(Arc::clone(&products), lease.clone())
            .unwrap();
        control.wait();
        match mode {
            0 => worker.cancel(),
            1 => drop(mailbox.take()),
            2 => thread::sleep(deadline.saturating_duration_since(Instant::now())),
            _ => unreachable!(),
        }
        for _ in 0..8 {
            assert!(matches!(worker.try_poll(), FreshnessPoll::Pending));
            assert_eq!(
                worker.try_enqueue(Arc::clone(&products), lease.clone()),
                Err(EnqueueError::Busy)
            );
        }
        control.resume();
        receive.recv_timeout(WATCHDOG).unwrap();
        assert_eq!(
            worker.try_enqueue(Arc::clone(&products), lease.clone()),
            Err(EnqueueError::Busy)
        );
        assert!(matches!(
            ready(&mut worker),
            Err(CompositionError::Cancelled)
        ));
        assert!(matches!(worker.try_poll(), FreshnessPoll::Idle));
        assert_eq!(lease.deadline(), deadline);
        if mode != 0 {
            assert!(!lease.is_active());
            assert_eq!(
                worker.try_enqueue(Arc::clone(&products), lease.clone()),
                Err(EnqueueError::Inactive)
            );
        }
    }
}

#[test]
fn cancellation_after_success_and_after_publication_cannot_escape_the_token() {
    let install = input::install();
    let products = capture(&install);
    let (_mailbox, lease, _) = request(WATCHDOG);
    for after_publication in [false, true] {
        let (hold_after, control) = gate();
        let (published, receive) = bounded(1);
        let mut worker = CaptureFreshnessWorker::with_hooks(WorkerHooks {
            after_success: Some(hold_after),
            published: Some(published),
            ..Default::default()
        })
        .unwrap();
        worker
            .try_enqueue(Arc::clone(&products), lease.clone())
            .unwrap();
        control.wait();
        if after_publication {
            control.resume();
            receive.recv_timeout(WATCHDOG).unwrap();
        }
        worker.cancel();
        assert_eq!(
            worker.try_enqueue(Arc::clone(&products), lease.clone()),
            Err(EnqueueError::Busy)
        );
        if !after_publication {
            assert!(matches!(worker.try_poll(), FreshnessPoll::Pending));
            control.resume();
            receive.recv_timeout(WATCHDOG).unwrap();
        }
        assert!(matches!(
            ready(&mut worker),
            Err(CompositionError::Cancelled)
        ));
    }
}

#[test]
fn completed_source_errors_keep_their_provenance_after_cancellation_or_retirement() {
    for remove in [false, true] {
        let install = input::install();
        let products = capture(&install);
        let (mailbox, lease, _) = request(WATCHDOG);
        let (hold_before, control) = gate();
        let (published, receive) = bounded(1);
        let mut worker = CaptureFreshnessWorker::with_hooks(WorkerHooks {
            before_verify: Some(hold_before),
            published: Some(published),
            ..Default::default()
        })
        .unwrap();
        worker
            .try_enqueue(Arc::clone(&products), lease.clone())
            .unwrap();
        control.wait();
        if remove {
            drop(install);
        } else {
            install.write(&input::overlapping_entries());
        }
        // Poll cannot touch the now-changed source while verification is held.
        assert!(matches!(worker.try_poll(), FreshnessPoll::Pending));
        control.resume();
        receive.recv_timeout(WATCHDOG).unwrap();
        worker.cancel();
        drop(mailbox);
        assert_eq!(
            worker.try_enqueue(Arc::clone(&products), lease.clone()),
            Err(EnqueueError::Busy)
        );
        match (remove, ready(&mut worker)) {
            (true, Err(CompositionError::TerrainSource))
            | (
                false,
                Err(CompositionError::Collision(PrepareError::Source(MapBundleError::Source(
                    SourceError::Changed,
                )))),
            ) => (),
            (removed, Err(error)) => {
                panic!("unexpected source failure after removal={removed}: {error:?}")
            }
            (_, Ok(_)) => panic!("changed source unexpectedly produced an observation"),
        }
        assert!(matches!(worker.try_poll(), FreshnessPoll::Idle));
    }
}

#[test]
fn observations_reject_equal_byte_foreign_captures_and_replacement_work() {
    let install = input::install();
    let products = capture(&install);
    let foreign = capture(&install);
    assert!(!Arc::ptr_eq(&products, &foreign));
    assert_eq!(products.binding, foreign.binding);
    let (session, request) = captured_route_request();
    let (_, mailbox) = mailbox(session.clone()).unwrap();
    let (_, replacement) = super::super::super::super::super::core::mailbox(session).unwrap();
    let deadline = Instant::now() + WATCHDOG;
    let (lease, _) = mailbox
        .enqueue_service_test_request_until(17, request.clone(), deadline)
        .unwrap();
    let (other, _) = replacement
        .enqueue_service_test_request_until(17, request, deadline)
        .unwrap();
    assert!(!lease.same_job(&other));
    assert_eq!(lease.request(), other.request());
    assert_eq!(lease.ticket(), other.ticket());
    assert_eq!(lease.deadline(), other.deadline());

    let (_worker, observation) = observed(&products, &lease);
    assert!(matches!(
        observation.consume(&foreign, &lease),
        Err(CompositionError::Validation(Error::Identity))
    ));
    let (_worker, observation) = observed(&products, &lease);
    assert!(matches!(
        observation.consume(&products, &other),
        Err(CompositionError::Validation(Error::Identity))
    ));
    let (_worker, observation) = observed(&products, &lease);
    observation.consume(&products, &lease.clone()).unwrap();
}

#[test]
fn drained_observations_still_reject_retired_expired_and_stopped_owners() {
    let install = input::install();
    let products = capture(&install);
    for mode in 0..3 {
        let (mailbox, lease, _) = request(if mode == 1 {
            Duration::from_secs(5)
        } else {
            WATCHDOG
        });
        let (worker, observation) = observed(&products, &lease);
        match mode {
            0 => drop(mailbox),
            1 => thread::sleep(lease.deadline().saturating_duration_since(Instant::now())),
            2 => drop(worker),
            _ => unreachable!(),
        }
        assert!(matches!(
            observation.consume(&products, &lease),
            Err(CompositionError::Cancelled)
        ));
    }
}

#[test]
fn already_expired_original_lease_is_refused_without_occupying_the_worker() {
    let install = input::install();
    let products = capture(&install);
    let (_mailbox, lease, _) = request(Duration::ZERO);
    assert!(!lease.is_active());
    let mut worker = CaptureFreshnessWorker::new().unwrap();
    assert_eq!(
        worker.try_enqueue(products, lease),
        Err(EnqueueError::Inactive)
    );
    assert!(matches!(worker.try_poll(), FreshnessPoll::Idle));
}

#[test]
fn drained_observation_is_revoked_by_explicit_cancel_or_a_new_accepted_request() {
    let install = input::install();
    let products = capture(&install);
    let (_mailbox, lease, _) = request(WATCHDOG);
    let (_replacement, replacement, _) = request(WATCHDOG);
    for replace in [false, true] {
        let (mut worker, observation) = observed(&products, &lease);
        assert!(matches!(worker.try_poll(), FreshnessPoll::Idle));
        if replace {
            worker
                .try_enqueue(Arc::clone(&products), replacement.clone())
                .unwrap();
        } else {
            worker.cancel();
        }
        assert!(lease.is_active(), "original lease unexpectedly retired");
        assert!(matches!(
            observation.consume(&products, &lease),
            Err(CompositionError::Cancelled)
        ));
    }
}

#[test]
fn dropped_worker_never_joins_a_held_verification_and_releases_owned_captures() {
    for after_success in [false, true] {
        let install = input::install();
        let products = capture(&install);
        let weak = Arc::downgrade(&products);
        let (_mailbox, lease, _) = request(WATCHDOG);
        let (hold, control) = gate();
        let (exited, exit) = bounded(1);
        let mut hooks = WorkerHooks {
            exited: Some(exited),
            ..Default::default()
        };
        if after_success {
            hooks.after_success = Some(hold);
        } else {
            hooks.before_verify = Some(hold);
        }
        let mut worker = CaptureFreshnessWorker::with_hooks(hooks).unwrap();
        worker.try_enqueue(products, lease).unwrap();
        control.wait();
        let (dropped, done) = bounded(1);
        thread::spawn(move || {
            drop(worker);
            dropped.send(()).unwrap();
        });
        done.recv_timeout(WATCHDOG).unwrap();
        assert!(
            weak.upgrade().is_some(),
            "held worker lost its exact capture"
        );
        assert!(matches!(exit.try_recv(), Err(TryRecvError::Empty)));
        control.resume();
        exit.recv_timeout(WATCHDOG).unwrap();
        assert!(
            weak.upgrade().is_none(),
            "detached worker leaked its capture"
        );
    }
}

#[test]
fn disconnected_worker_stays_stopped_instead_of_spawning_a_replacement() {
    let install = input::install();
    let products = capture(&install);
    let (_mailbox, lease, _) = request(WATCHDOG);
    let (hold_before, control) = gate();
    let (exited, exit) = bounded(1);
    let mut worker = CaptureFreshnessWorker::with_hooks(WorkerHooks {
        before_verify: Some(hold_before),
        exited: Some(exited),
        ..Default::default()
    })
    .unwrap();
    worker
        .try_enqueue(Arc::clone(&products), lease.clone())
        .unwrap();
    control.wait();
    drop(control);
    exit.recv_timeout(WATCHDOG).unwrap();
    // The exit notification precedes channel destruction, so wait for disconnection
    // without receiving or inventing a source result.
    assert!(worker.completed.recv_timeout(WATCHDOG).is_err());
    for _ in 0..3 {
        assert!(matches!(worker.try_poll(), FreshnessPoll::Stopped));
        assert_eq!(
            worker.try_enqueue(Arc::clone(&products), lease.clone()),
            Err(EnqueueError::Stopped)
        );
    }
}

#[test]
fn resumed_observation_clamps_deadline_and_uses_original_request_on_caller_thread() {
    let install = input::install();
    let products = capture(&install);
    let (_mailbox, lease, _) = request(WATCHDOG);
    let caller = thread::current().id();
    for invocation in [
        lease.deadline() + WATCHDOG,
        Instant::now() + Duration::from_secs(5),
    ] {
        let (_worker, observation) = observed(&products, &lease);
        let result: Result<_, CompositionError> = observation.with_current(
            &products,
            &lease,
            invocation,
            |context, deadline, cancelled| {
                assert_eq!(thread::current().id(), caller);
                assert!(std::ptr::eq(context, lease.request()));
                assert_eq!(deadline, invocation.min(lease.deadline()));
                assert!(!cancelled());
                products.verify_sources(cancelled)?;
                Ok(17)
            },
        );
        assert_eq!(result.unwrap(), 17);
    }
    let (_worker, observation) = observed(&products, &lease);
    let result: Result<(), CompositionError> =
        observation.with_current(&products, &lease, Instant::now(), |_, _, _| {
            panic!("expired slice resumed")
        });
    assert!(matches!(result, Err(CompositionError::Cancelled)));
    assert!(
        lease.is_active(),
        "a slice expiry must not retire or renew the Work"
    );
}

#[test]
fn resumed_observation_rejects_foreign_capture_or_work_before_callback() {
    let install = input::install();
    let products = capture(&install);
    let foreign = capture(&install);
    let (_mailbox, lease, _) = request(WATCHDOG);
    let (_replacement, replacement, _) = request(WATCHDOG);
    for (capture, work) in [(&foreign, &lease), (&products, &replacement)] {
        let (_worker, observation) = observed(&products, &lease);
        let result: Result<(), CompositionError> =
            observation.with_current(capture, work, lease.deadline(), |_, _, _| {
                panic!("foreign identity resumed")
            });
        assert!(matches!(
            result,
            Err(CompositionError::Validation(Error::Identity))
        ));
    }
}

#[test]
fn resumed_observation_keeps_revocation_live_until_callback_returns() {
    let install = input::install();
    let products = capture(&install);
    // Cancellation, accepted replacement, attachment retirement, and worker drop
    // are tested both between Ready/consume and inside the synchronous attempt.
    for prepared in [false, true] {
        for before in [false, true] {
            for mode in 0..4 {
                let (mailbox, lease, _) = request(WATCHDOG);
                let (_replacement, replacement, _) = request(WATCHDOG);
                let (worker, observation) = observed(&products, &lease);
                let observation = if prepared {
                    let tokens = (
                        Arc::clone(&observation.cancelled),
                        Arc::clone(&observation.stop),
                    );
                    let (observation, ()) = observation
                        .prepare_current::<_, CompositionError>(
                            &products,
                            &lease,
                            lease.deadline(),
                            |_, _, _| Ok(()),
                        )
                        .unwrap();
                    assert!(Arc::ptr_eq(&tokens.0, &observation.cancelled));
                    assert!(Arc::ptr_eq(&tokens.1, &observation.stop));
                    observation
                } else {
                    observation
                };
                let mut worker = Some(worker);
                let mut mailbox = Some(mailbox);
                let mut revoke = || match mode {
                    0 => worker.as_ref().unwrap().cancel(),
                    1 => worker
                        .as_mut()
                        .unwrap()
                        .try_enqueue(Arc::clone(&products), replacement.clone())
                        .unwrap(),
                    2 => drop(mailbox.take()),
                    3 => drop(worker.take()),
                    _ => unreachable!(),
                };
                if before {
                    revoke();
                }
                let result: Result<(), CompositionError> = observation.with_current(
                    &products,
                    &lease,
                    lease.deadline(),
                    |_, _, cancelled| {
                        assert!(!before, "retired completion entered callback");
                        assert!(!cancelled());
                        revoke();
                        assert!(cancelled());
                        Ok(())
                    },
                );
                assert!(matches!(result, Err(CompositionError::Cancelled)));
            }
        }
    }
}

#[test]
fn resumed_observation_refuses_late_slice_expiry_and_changed_current_sources() {
    let install = input::install();
    let products = capture(&install);
    let (_mailbox, lease, _) = request(WATCHDOG);
    let (_worker, observation) = observed(&products, &lease);
    let deadline = Instant::now() + Duration::from_millis(50);
    let result: Result<(), CompositionError> =
        observation.with_current(&products, &lease, deadline, |_, deadline, cancelled| {
            thread::sleep(deadline.saturating_duration_since(Instant::now()));
            assert!(cancelled());
            Ok(())
        });
    assert!(matches!(result, Err(CompositionError::Cancelled)));
    let (_worker, observation) = observed(&products, &lease);
    install.write(&input::overlapping_entries());
    let result: Result<(), CompositionError> =
        observation.with_current(&products, &lease, lease.deadline(), |_, _, cancelled| {
            products.verify_sources(cancelled)
        });
    assert!(
        result.is_err(),
        "historical success substituted for current sources"
    );
    assert!(!matches!(result, Err(CompositionError::Cancelled)));
}

#[test]
fn resumed_observation_cannot_renew_original_expiry_after_ready() {
    let install = input::install();
    let products = capture(&install);
    let (_mailbox, lease, _) = request(Duration::from_secs(5));
    let (_worker, observation) = observed(&products, &lease);
    thread::sleep(lease.deadline().saturating_duration_since(Instant::now()));
    let result: Result<(), CompositionError> =
        observation.with_current(&products, &lease, Instant::now() + WATCHDOG, |_, _, _| {
            panic!("new slice renewed original expired Work")
        });
    assert!(matches!(result, Err(CompositionError::Cancelled)));
}

#[cfg(not(benilla_headless_geometry))]
mod f05 {
    use super::super::super::super::freshness_timings::{self, Phase};
    use super::*;

    fn held_guard(
        products: &Arc<OwnedSourceProducts>,
        lease: &JobLease,
        deadline: Instant,
    ) -> (CaptureFreshnessWorker, F05Guard, Control, Receiver<()>) {
        let (before, control) = gate();
        let (published, receive) = bounded(1);
        let mut worker = CaptureFreshnessWorker::with_hooks(WorkerHooks {
            before_verify: Some(before),
            published: Some(published),
            ..Default::default()
        })
        .unwrap();
        worker
            .try_enqueue(Arc::clone(products), lease.clone())
            .unwrap();
        control.wait();
        control.resume();
        receive.recv_timeout(WATCHDOG).unwrap();
        let guard = ready(&mut worker)
            .unwrap()
            .into_f05_guard(products, lease, deadline)
            .unwrap();
        (worker, guard, control, receive)
    }

    fn raw_ready(worker: &mut CaptureFreshnessWorker, slot: &F05Slot) -> F05Completion {
        match worker.try_poll_f05(slot) {
            F05Poll::Ready(result) => result,
            _ => panic!("published original F05 slot was not ready"),
        }
    }

    #[test]
    fn continuation_keeps_original_guard_and_occupied_slot_until_exact_raw_drain() {
        let install = input::install();
        let products = capture(&install);
        let (_mailbox, lease, _) = request(WATCHDOG);
        let deadline = Instant::now() + Duration::from_secs(10);
        let (mut worker, guard, control, published) = held_guard(&products, &lease, deadline);
        assert!(guard.matches(&products, &lease));
        assert!(std::ptr::eq(guard.request(), lease.request()));
        assert!(guard.lease().same_job(&lease));
        assert_eq!(guard.callback_deadline(), deadline.min(lease.deadline()));
        assert!(Arc::ptr_eq(&guard.observation.products, &products));
        assert!(Arc::ptr_eq(
            &guard.observation.cancelled,
            worker.latest.as_ref().unwrap()
        ));
        assert!(Arc::ptr_eq(&guard.observation.stop, &worker.stop));
        let original_cancel = Arc::clone(&guard.observation.cancelled);
        let slot = worker.try_continue_f05(&guard).unwrap();
        let source_thread = control.wait();
        assert_ne!(source_thread, thread::current().id());
        assert!(!guard.cancelled(), "continuation must not revoke original observation");
        assert!(Arc::ptr_eq(&original_cancel, worker.latest.as_ref().unwrap()));
        for _ in 0..3 {
            assert!(matches!(worker.try_poll_f05(&slot), F05Poll::Pending));
            assert!(matches!(worker.try_poll(), FreshnessPoll::Pending));
            assert!(matches!(
                worker.try_continue_f05(&guard),
                Err(F05EnqueueError::Busy)
            ));
            assert_eq!(
                worker.try_enqueue(Arc::clone(&products), lease.clone()),
                Err(EnqueueError::Busy)
            );
        }
        let wrong_slot = F05Slot {
            identity: Arc::new(()),
        };
        assert!(matches!(worker.try_poll_f05(&wrong_slot), F05Poll::Mismatch));
        control.resume();
        published.recv_timeout(WATCHDOG).unwrap();
        worker.completion_wake().recv_timeout(WATCHDOG).unwrap();
        assert!(worker.occupied, "completed-undrained work still occupies capacity");
        assert!(matches!(worker.try_poll_f05(&wrong_slot), F05Poll::Mismatch));
        let raw = raw_ready(&mut worker, &slot);
        assert!(raw.matches_guard(&guard));
        assert!(raw.result.is_ok());
        assert_eq!(raw.diagnostics.trace.len(), 1, "actual worker TLS source occurrence");
        assert!(raw.diagnostics.trace[0].0.ends_with("source_height/owned.rs"));
        assert!(raw.diagnostics().cancel_checks > 0);
        assert_eq!(raw.diagnostics().first_cancelled_check, None);
        assert_eq!(raw.diagnostics().counts.started(Phase::Total), 1);
        let F05Outcome::Success(success) = raw.into_outcome(&guard).unwrap() else {
            panic!("successful source must mint only the exact verified token");
        };
        assert!(success.matches_guard(&guard));
        assert_eq!(success.diagnostics().trace.len(), 1);
        let diagnostics = success.into_diagnostics();
        assert_eq!(diagnostics.trace.len(), 1);
        assert!(!worker.occupied);
        assert!(!guard.cancelled());
        guard.check_success().unwrap();
        assert!(matches!(worker.try_poll_f05(&slot), F05Poll::Mismatch));
        assert!(matches!(
            worker.try_continue_f05(&guard),
            Err(F05EnqueueError::AlreadySubmitted)
        ));
        // A genuinely new admitted observation retains its original revocation rule.
        worker
            .try_enqueue(Arc::clone(&products), lease.clone())
            .unwrap();
        control.wait();
        assert!(guard.cancelled());
        control.resume();
        published.recv_timeout(WATCHDOG).unwrap();
        ready(&mut worker).unwrap();
    }

    #[test]
    fn raw_success_and_source_error_survive_late_cancel_retirement_and_stop() {
        for source_error in [false, true] {
            let install = input::install();
            let products = capture(&install);
            let (mailbox, lease, _) = request(WATCHDOG);
            let (mut worker, guard, control, published) =
                held_guard(&products, &lease, lease.deadline());
            let slot = worker.try_continue_f05(&guard).unwrap();
            control.wait();
            if source_error {
                let mut entries = input::navigation_entries();
                entries
                    .iter_mut()
                    .find(|(name, _)| name == input::WDT)
                    .unwrap()
                    .1[20] ^= 1;
                install.write(&entries);
            }
            control.resume();
            published.recv_timeout(WATCHDOG).unwrap();
            worker.cancel();
            drop(mailbox);
            worker.stop.store(true, Ordering::Release);
            assert!(guard.cancelled());
            let raw = raw_ready(&mut worker, &slot);
            assert_eq!(raw.diagnostics.trace.len(), 1);
            assert!(raw.matches_guard(&guard), "inactive matching must preserve raw precedence");
            match raw.into_outcome(&guard).unwrap() {
                F05Outcome::SourceError(CompositionError::TerrainSource, diagnostics) => {
                    assert!(source_error);
                    assert_eq!(diagnostics.first_cancelled_check, None);
                }
                F05Outcome::Success(success) => {
                    assert!(!source_error);
                    assert!(success.matches_guard(&guard));
                    assert_eq!(success.into_diagnostics().first_cancelled_check, None);
                }
                _ => panic!("late retirement rewrote actual raw source outcome"),
            }
            assert!(matches!(guard.check_success(), Err(CompositionError::Cancelled)));
            // Actual canceled F59 source entry can still run from the ORIGINAL guard.
            let (exit, trace) = freshness_timings::trace_sources(|| {
                products.verify_sources(|| guard.cancelled())
            });
            assert!(matches!(exit, Err(CompositionError::Cancelled)));
            assert_eq!(trace.len(), 1);
        }
    }

    #[test]
    fn original_callback_expiry_cancel_and_stop_keep_running_continuation_tombstoned() {
        let install = input::install();
        let products = capture(&install);
        for mode in 0..4 {
            let (mailbox, lease, _) = request(WATCHDOG);
            let callback_deadline = if mode == 0 {
                Instant::now() + Duration::from_millis(500)
            } else {
                lease.deadline()
            };
            let (mut worker, guard, control, published) =
                held_guard(&products, &lease, callback_deadline);
            let slot = worker.try_continue_f05(&guard).unwrap();
            control.wait();
            let mut mailbox = Some(mailbox);
            match mode {
                0 => thread::sleep(callback_deadline.saturating_duration_since(Instant::now())),
                1 => guard.cancel(),
                2 => drop(mailbox.take()),
                3 => worker.stop.store(true, Ordering::Release),
                _ => unreachable!(),
            }
            assert!(guard.cancelled());
            assert!(matches!(worker.try_poll_f05(&slot), F05Poll::Pending));
            assert!(worker.occupied);
            assert!(matches!(
                worker.try_continue_f05(&guard),
                Err(F05EnqueueError::Busy | F05EnqueueError::Stopped)
            ));
            control.resume();
            published.recv_timeout(WATCHDOG).unwrap();
            let raw = raw_ready(&mut worker, &slot);
            assert!(matches!(raw.result, Err(CompositionError::Cancelled)));
            assert_eq!(raw.diagnostics.trace.len(), 1);
            assert_eq!(raw.diagnostics.cancel_checks, 1);
            assert_eq!(raw.diagnostics.first_cancelled_check, Some(1));
            assert_eq!(guard.callback_deadline(), callback_deadline.min(lease.deadline()));
            if mode != 2 {
                assert!(
                    lease.is_active(),
                    "callback expiry/cancel/worker stop never renews or retires Work"
                );
            }
        }
    }

    #[test]
    fn foreign_products_work_worker_and_revoked_guard_cannot_submit_continuation() {
        let install = input::install();
        let products = capture(&install);
        let foreign = capture(&install);
        let (_mailbox, lease, _) = request(WATCHDOG);
        let (_other_mailbox, other, _) = request(WATCHDOG);
        for (capture, work) in [(&foreign, &lease), (&products, &other)] {
            let (_worker, observation) = observed(&products, &lease);
            assert!(matches!(
                observation.into_f05_guard(capture, work, lease.deadline()),
                Err(CompositionError::Validation(Error::Identity))
            ));
        }
        let (mut worker, guard, control, published) =
            held_guard(&products, &lease, lease.deadline());
        assert!(!guard.matches(&foreign, &lease));
        assert!(!guard.matches(&products, &other));
        let mut other_worker = CaptureFreshnessWorker::new().unwrap();
        assert!(matches!(
            other_worker.try_continue_f05(&guard),
            Err(F05EnqueueError::Identity)
        ));
        assert!(!other_worker.occupied);
        worker.try_enqueue(Arc::clone(&products), other.clone()).unwrap();
        control.wait();
        assert!(guard.cancelled());
        assert!(matches!(
            worker.try_continue_f05(&guard),
            Err(F05EnqueueError::Busy)
        ));
        control.resume();
        published.recv_timeout(WATCHDOG).unwrap();
        ready(&mut worker).unwrap();
        assert!(matches!(
            worker.try_continue_f05(&guard),
            Err(F05EnqueueError::Identity)
        ));
        assert!(!worker.occupied);
    }

    #[test]
    fn control_wake_precedes_held_completion_and_disconnect_never_fabricates_raw_result() {
        let install = input::install();
        let products = capture(&install);
        let (_mailbox, lease, _) = request(WATCHDOG);
        let (mut worker, guard, control, _published) =
            held_guard(&products, &lease, lease.deadline());
        let slot = worker.try_continue_f05(&guard).unwrap();
        control.wait();
        let wake = worker.completion_wake();
        worker.cancel();
        wake.recv_timeout(WATCHDOG).unwrap();
        assert!(matches!(worker.try_poll_f05(&slot), F05Poll::Pending));
        assert!(worker.occupied);
        drop(control); // Real gate/channel disconnect abandons the still-incomplete source.
        assert!(worker.completed.recv_timeout(WATCHDOG).is_err());
        assert!(matches!(worker.try_poll_f05(&slot), F05Poll::Stopped));
        assert!(worker.occupied);
        assert!(matches!(
            worker.try_continue_f05(&guard),
            Err(F05EnqueueError::Stopped)
        ));
    }

    #[test]
    fn dropped_continuation_worker_does_not_join_and_pinned_guard_remains_cancelled() {
        let install = input::install();
        let products = capture(&install);
        let (_mailbox, lease, _) = request(WATCHDOG);
        let (mut worker, guard, control, _published) =
            held_guard(&products, &lease, lease.deadline());
        let _slot = worker.try_continue_f05(&guard).unwrap();
        control.wait();
        let (done, dropped) = bounded(1);
        thread::spawn(move || {
            drop(worker);
            done.send(()).unwrap();
        });
        dropped.recv_timeout(WATCHDOG).unwrap();
        assert!(guard.cancelled());
        assert!(guard.matches(&products, &lease), "inactive custody identity remains checkable");
        control.resume();
    }
    #[test]
    fn foreign_raw_result_identity_never_releases_original_slot_or_mints_completion() {
        let install = input::install();
        let products = capture(&install);
        let (_mailbox, lease, _) = request(WATCHDOG);
        let (mut worker, guard, control, _published) =
            held_guard(&products, &lease, lease.deadline());
        let slot = worker.try_continue_f05(&guard).unwrap();
        control.wait();
        let (send, receive) = bounded(1);
        let original_receiver = std::mem::replace(&mut worker.completed, receive);
        assert!(send
            .send(WorkerResult::F05(F05Result {
                identity: Arc::new(()),
                completion: F05Completion {
                    identity: F05ProofIdentity {
                        operation: Arc::new(()),
                        products: Arc::clone(&products),
                        lease: lease.clone(),
                        cancelled: Arc::clone(&guard.observation.cancelled),
                        stop: Arc::clone(&guard.observation.stop),
                        callback_deadline: guard.callback_deadline(),
                    },
                    result: Ok(()),
                    diagnostics: F05Diagnostics::default(),
                },
            }))
            .is_ok());
        assert!(matches!(worker.try_poll_f05(&slot), F05Poll::Mismatch));
        assert!(worker.occupied);
        assert!(
            guard.cancelled(),
            "protocol mismatch abandons the original operation"
        );
        assert!(matches!(
            worker.try_continue_f05(&guard),
            Err(F05EnqueueError::Stopped)
        ));
        // The injected empty trace was never returned as the original result.
        drop(worker);
        control.resume();
        drop(original_receiver);
    }

    #[test]
    fn completion_must_match_original_guard_before_success_token_is_minted() {
        let install = input::install();
        let products = capture(&install);
        let (_mailbox, lease, _) = request(WATCHDOG);
        let deadline = lease.deadline();
        let (mut first, first_guard, first_control, first_published) =
            held_guard(&products, &lease, deadline);
        let (mut second, second_guard, second_control, second_published) =
            held_guard(&products, &lease, deadline);
        let first_slot = first.try_continue_f05(&first_guard).unwrap();
        let second_slot = second.try_continue_f05(&second_guard).unwrap();
        first_control.wait();
        second_control.wait();
        first_control.resume();
        second_control.resume();
        first_published.recv_timeout(WATCHDOG).unwrap();
        second_published.recv_timeout(WATCHDOG).unwrap();
        let first_result = raw_ready(&mut first, &first_slot);
        let second_result = raw_ready(&mut second, &second_slot);
        assert!(first_result.matches_guard(&first_guard));
        assert!(!first_result.matches_guard(&second_guard));
        assert!(matches!(
            first_result.into_outcome(&second_guard),
            Err(CompositionError::Validation(Error::Identity))
        ));
        let F05Outcome::Success(proof) = second_result.into_outcome(&second_guard).unwrap() else {
            panic!("actual matched success");
        };
        assert!(proof.matches_guard(&second_guard));
        assert!(!proof.matches_guard(&first_guard));
        assert_eq!(proof.into_diagnostics().trace.len(), 1);
    }

    #[test]
    fn f05_only_gate_never_blocks_observation_and_rechecks_original_guard_after_release() {
        let install = input::install();
        let products = capture(&install);
        let (_mailbox, lease, _) = request(WATCHDOG);
        let (entered, arrived) = bounded(1);
        let (release, resume) = bounded(1);
        let (published, complete) = bounded(1);
        let mut worker = CaptureFreshnessWorker::with_hooks(WorkerHooks {
            f05_before_verify: Some(F05Gate {
                entered,
                release: resume,
            }),
            published: Some(published),
            ..Default::default()
        })
        .unwrap();
        worker
            .try_enqueue(Arc::clone(&products), lease.clone())
            .unwrap();
        complete.recv_timeout(WATCHDOG).unwrap();
        assert!(matches!(arrived.try_recv(), Err(TryRecvError::Empty)));
        let guard = ready(&mut worker)
            .unwrap()
            .into_f05_guard(&products, &lease, lease.deadline())
            .unwrap();
        let slot = worker.try_continue_f05(&guard).unwrap();
        arrived.recv_timeout(WATCHDOG).unwrap();
        assert!(matches!(complete.try_recv(), Err(TryRecvError::Empty)));
        worker.cancel();
        worker.completion_wake().recv_timeout(WATCHDOG).unwrap();
        assert!(matches!(worker.try_poll_f05(&slot), F05Poll::Pending));
        release.send(()).unwrap();
        complete.recv_timeout(WATCHDOG).unwrap();
        let F05Outcome::SourceError(CompositionError::Cancelled, diagnostics) =
            raw_ready(&mut worker, &slot).into_outcome(&guard).unwrap()
        else {
            panic!("released held source must use the original cancelled guard");
        };
        assert_eq!(diagnostics.trace.len(), 1);
        assert_eq!(diagnostics.cancel_checks, 1);
        assert_eq!(diagnostics.first_cancelled_check, Some(1));
    }
}
