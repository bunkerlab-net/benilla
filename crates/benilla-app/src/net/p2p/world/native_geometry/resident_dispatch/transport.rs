// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicitly paired test transport; ordinary LocalAuthority remains closed to resident Routes.
use super::*;

pub(crate) struct ResidentTestAuthority {
    ordinary: LocalAuthority,
    selection: Arc<ResidentTransportSelection>,
    route_budget: RouteBudget,
}
// Correctness fixtures opt in explicitly; this is not a production latency budget.
#[derive(Clone, Copy)]
enum RouteBudget {
    Default,
    ControllerProof,
}
impl RouteBudget {
    fn wait(self) -> Duration {
        match self {
            Self::Default => WAIT,
            Self::ControllerProof => Duration::from_secs(60),
        }
    }
}
/// The ordinary paired transport keeps the same 500 ms Route deadline.
pub(crate) fn paired_test_mailbox(
    resident: &OwnedResident,
) -> Result<(ResidentTestAuthority, Mailbox), Failure> {
    paired_mailbox(resident, RouteBudget::Default)
}
/// Fixed-budget controller correctness proof only, not a configurable timeout.
/// Non-Route requests and the separate commit sink still use the ordinary WAIT.
pub(crate) fn paired_controller_proof_mailbox(
    resident: &OwnedResident,
) -> Result<(ResidentTestAuthority, Mailbox), Failure> {
    paired_mailbox(resident, RouteBudget::ControllerProof)
}
fn paired_mailbox(
    resident: &OwnedResident,
    route_budget: RouteBudget,
) -> Result<(ResidentTestAuthority, Mailbox), Failure> {
    let selection = resident.transport_selection();
    let (ordinary, mailbox) =
        mailbox_selected(selection.session().clone(), Some(Arc::clone(&selection)))?;
    Ok((
        ResidentTestAuthority {
            ordinary,
            selection,
            route_budget,
        },
        mailbox,
    ))
}
impl ResidentTestAuthority {
    pub(crate) fn take_route_commit_sink(&mut self) -> Option<MailboxRouteCommitSink> {
        self.ordinary.take_route_commit_sink()
    }
    fn finish_reply(
        &self,
        request: &AuthorityRequest,
        lease: &JobLease,
        deadline: Instant,
        reply: Reply,
    ) -> Result<AuthorityReply, NativeError> {
        self.ordinary.check_active()?;
        if deadline != lease.deadline()
            || lease.ticket() != self.ordinary.ticket
            || !Arc::ptr_eq(&lease.0.attachment, &self.ordinary.active)
        {
            return Err(NativeError::AuthorityBinding);
        }
        if Instant::now() >= deadline {
            return Err(Failure::Deadline.native());
        }
        if reply.ticket != self.ordinary.ticket || reply.request != *request {
            return Err(NativeError::AuthorityBinding);
        }
        let facts = reply.result.map_err(Failure::native)?;
        reply
            .resident
            .ok_or(NativeError::AuthorityBinding)?
            .validate_reply(&self.selection, lease, request, &facts)
            .map_err(Failure::native)?;
        self.ordinary.check_active()?;
        if Instant::now() >= deadline {
            return Err(Failure::Deadline.native());
        }
        Ok(AuthorityReply {
            request: request.clone(),
            facts,
        })
    }
}
impl Authority for ResidentTestAuthority {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if !matches!(request.query, Query::SourceMotion(MotionQuery::Route(_))) {
            return self.ordinary.resolve(request);
        }
        self.ordinary.check_active()?;
        if !self.ordinary.route_ready.load(Ordering::Acquire) {
            return Err(NativeError::AuthorityUnavailable(
                "resident commit session is not initialized",
            ));
        }
        self.selection.request(request).map_err(Failure::native)?;
        self.ordinary.ticket = self
            .ordinary
            .ticket
            .checked_add(1)
            .ok_or(NativeError::Limit)?;
        let (send, receive) = bounded(1);
        let started = Instant::now();
        // Assigned once: retries and service slices never renew this Work's expiry.
        let deadline = started + self.route_budget.wait();
        let work = Work::new(
            self.ordinary.ticket,
            request.clone(),
            deadline,
            send,
            self.ordinary.active.clone(),
        );
        let lease = work.lease();
        self.ordinary
            .tx
            .try_send(work)
            .map_err(|error| match error {
                TrySendError::Full(_) => {
                    NativeError::AuthorityUnavailable("local geometry queue full")
                }
                TrySendError::Disconnected(_) => {
                    NativeError::AuthorityUnavailable("local geometry service closed")
                }
            })?;
        let reply = receive_reply_until(&receive, &self.ordinary.cancelled, deadline);
        eprintln!(
            "resident Route receiver: tick={} ticket={} elapsed_ms={} budget_ms={} received={}",
            request.binding.tick,
            lease.ticket(),
            started.elapsed().as_millis(),
            self.route_budget.wait().as_millis(),
            reply.is_ok(),
        );
        // Diagnostics precede the final active/identity/expiry checks, never follow them.
        let reply = reply?;
        self.finish_reply(request, &lease, deadline, reply)
    }
}

/// Exercise the real default request and its exact requeued Work without certifying Facts.
fn assert_default_route_expiry(owner: &OwnedResident, request: &AuthorityRequest) {
    let (mut authority, mailbox) = paired_test_mailbox(owner).unwrap();
    assert_eq!(authority.route_budget.wait(), Duration::from_millis(500));
    let (proof, _) = paired_controller_proof_mailbox(owner).unwrap();
    assert_eq!(proof.route_budget.wait(), Duration::from_secs(60));
    // Isolate the request deadline from initialization in this refusal-only probe.
    // No successful result or commit is supplied and no ledger operation is performed.
    authority
        .ordinary
        .route_ready
        .store(true, Ordering::Release);
    std::thread::scope(|scope| {
        let started = Instant::now();
        let receiver = scope.spawn(|| authority.resolve(request));
        let work = mailbox.rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let lease = work.lease();
        let original_deadline = work.deadline;
        assert!(matches!(
            mailbox.defer_route_query(work),
            QueryDisposition::Deferred
        ));
        assert_eq!(lease.ticket(), 1);
        assert_eq!(lease.request(), request);
        assert_eq!(lease.deadline(), original_deadline);
        assert!(original_deadline >= started + WAIT);
        let requeued = mailbox.rx.try_recv().unwrap();
        assert!(lease.same_job(&requeued.lease()));
        assert_eq!(requeued.request, *request);
        assert_eq!(requeued.ticket, lease.ticket());
        assert_eq!(requeued.deadline, original_deadline);
        // Keep the original sender alive, but supply no reply until the default expires.
        let refused = receiver.join().unwrap();
        assert!(matches!(
            refused,
            Err(NativeError::AuthorityUnavailable(
                "local geometry request deadline exceeded"
            ))
        ));
        assert!(!lease.is_active());
        assert_eq!(lease.deadline(), original_deadline);
        assert!(matches!(
            mailbox.defer_route_query(requeued),
            QueryDisposition::Retired
        ));
        assert!(mailbox.rx.is_empty());
        assert!(
            mailbox.route_rx.is_empty(),
            "expiry cannot enqueue a commit/ACK"
        );
    });
}

/// Genuine component setup supplies the live request; no seed Facts enter this probe.
#[cfg(test)]
pub(in super::super::super) fn exercise_result_transport(
    owner: &mut OwnedResident,
    geometry: &mut super::super::super::real_corridor_fixture::installed_source::InstalledFixture,
    request: &AuthorityRequest,
    drop_reply: bool,
) -> Facts {
    use super::super::super::source_route_filter::resident::ResidentFilterError;
    use super::fifo::ResidentFifoSelection;
    use super::super::super::source_route_filter::resident::owned::ResidentPhaseResult;
    use benilla_world::terrain_stream::NativeStaticMapIndex;
    fn send<T: Send>() {}
    send::<Work>();
    send::<Reply>();
    send::<ResidentTestAuthority>();
    if !drop_reply {
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
                Instant::now() + Duration::from_secs(120),
                || false,
                |_, _, ledger| Ok(ledger.diagnostic()),
            )
            .unwrap();
        assert_default_route_expiry(owner, request);
        owner
            .inspect(
                &installed,
                Instant::now() + Duration::from_secs(120),
                || false,
                |_, _, ledger| {
                    assert_eq!(
                        ledger.diagnostic(),
                        before,
                        "default expiry changes no resident state"
                    );
                    Ok(())
                },
            )
            .unwrap();
    }
    let (mut authority, mailbox) = paired_test_mailbox(owner).unwrap();
    authority.ordinary.ticket = 901;
    let mut selector = ResidentFifoSelection::new(&mailbox).unwrap();
    let deadline = Instant::now() + Duration::from_secs(120);
    let (lease, receive) = mailbox
        .enqueue_resident_test_request(owner, 901, request.clone(), deadline)
        .unwrap();
    let mut receive = Some(receive);
    if drop_reply {
        drop(receive.take());
    }
    let installed = geometry
        .app
        .world()
        .resource::<NativeStaticMapIndex>()
        .source_provenance()
        .unwrap()
        .clone();
    let retained = matches!(&request.query, Query::SourceMotion(MotionQuery::Route(route)) if route.request.prior_cache.is_some());
    let mut callbacks = 0;
    let mut facts = None;
    // A rejected post-stage attempt and then a successful late attempt each requeue
    // the same Work, without leaving a stale live-dispatch candidate behind.
    while callbacks < 2 {
        assert!(Instant::now() < deadline);
        let slice = Instant::now() + Duration::from_secs(2);
        selector.serve_phased_results(
            &mailbox,
            owner,
            8,
            slice,
            |owner, job, phase| {
                assert!(job.lease().same_job(&lease));
                geometry.advance_physics();
                if callbacks == 0 {
                    let mut after_called = false;
                    let result = owner.certify_phase_result_after(
                        phase,
                        job,
                        geometry.app.world_mut(),
                        &installed,
                        || {
                            after_called = true;
                            let remaining = slice.saturating_duration_since(Instant::now());
                            std::thread::sleep(remaining + Duration::from_millis(1));
                            Err(Failure::Deadline)
                        },
                    );
                    if let Ok(ResidentPhaseResult::Prepared(prepared)) = result {
                        return Ok(ResidentPhaseResult::Prepared(prepared));
                    }
                    assert!(
                        after_called,
                        "injected failure follows genuine certification"
                    );
                    assert!(matches!(
                        result,
                        Err(ResidentFilterError::Context(Failure::Deadline))
                    ));
                    callbacks += 1;
                    return Err(Failure::Deadline);
                }
                let result = owner
                    .certify_phase_result(
                        phase,
                        job,
                        geometry.app.world_mut(),
                        &installed,
                    )
                    .map_err(|error| match error {
                        ResidentFilterError::Context(error) => error,
                        _ => Failure::Binding,
                    })?;
                let ResidentPhaseResult::Complete(result) = result else {
                    return Ok(result);
                };
                callbacks += 1;
                facts = Some(result.facts().clone());
                let remaining = slice.saturating_duration_since(Instant::now());
                std::thread::sleep(remaining + Duration::from_millis(1));
                Ok(ResidentPhaseResult::Complete(result))
            },
            |_, _| panic!("resident must use selected continuation"),
        );
        std::thread::yield_now();
    }
    assert!(lease.is_active(), "late result requeues the original Work");
    if let Some(receive) = &receive {
        assert!(matches!(
            receive.0.try_recv(),
            Err(crossbeam_channel::TryRecvError::Empty)
        ));
    }
    owner
        .inspect(
            &installed,
            deadline,
            || false,
            |_, _, ledger| {
                assert_eq!(ledger.counts(), (usize::from(retained), usize::from(retained)));
                assert_eq!(ledger.next_transport_candidate(), if retained { 3 } else { 2 });
                Ok(())
            },
        )
        .unwrap();
    // A second observation and concrete certification, never a saved result, supplies success.
    while lease.is_active() {
        assert!(Instant::now() < deadline);
        selector.serve_phased_results(
            &mailbox,
            owner,
            8,
            deadline,
            |owner, job, phase| {
                assert!(job.lease().same_job(&lease));
                geometry.advance_physics();
                let result = owner
                    .certify_phase_result(
                        phase,
                        job,
                        geometry.app.world_mut(),
                        &installed,
                    )
                    .map_err(|_| Failure::Binding)?;
                let ResidentPhaseResult::Complete(result) = result else {
                    return Ok(result);
                };
                owner.assert_result_refusals_for_test(&result);
                callbacks += 1;
                facts = Some(result.facts().clone());
                Ok(ResidentPhaseResult::Complete(result))
            },
            |_, _| panic!("resident must use selected continuation"),
        );
        std::thread::yield_now();
    }
    assert_eq!(callbacks, 3);
    if drop_reply {
        owner
            .inspect(
                &installed,
                deadline,
                || false,
                |_, _, ledger| {
                    assert_eq!(ledger.counts(), (usize::from(retained), usize::from(retained)));
                    Ok(())
                },
            )
            .unwrap();
        return facts.unwrap();
    }
    let reply = receive.unwrap().0.try_recv().unwrap();
    let copy = || Reply {
        ticket: reply.ticket,
        request: reply.request.clone(),
        result: reply.result.clone(),
        resident: reply.resident.clone(),
    };
    assert!(
        authority
            .finish_reply(request, &lease, deadline, copy())
            .is_ok(),
        "Work retirement cannot invalidate already-delivered selected evidence"
    );
    let mut wrong = copy();
    wrong.ticket += 1;
    assert!(authority
        .finish_reply(request, &lease, deadline, wrong)
        .is_err());
    let mut wrong = copy();
    wrong.request.binding.tick += 1;
    assert!(authority
        .finish_reply(request, &lease, deadline, wrong)
        .is_err());
    let mut wrong = copy();
    wrong.resident = None;
    assert!(authority
        .finish_reply(request, &lease, deadline, wrong)
        .is_err());
    let mut wrong = copy();
    let Facts::SourceMotion(MotionFacts::Spatial(spatial)) = wrong.result.as_mut().unwrap() else {
        unreachable!()
    };
    let motion::Fact::RouteCalculation(calculation) = &mut spatial.fact else {
        unreachable!()
    };
    calculation.after_cache.revision += 1;
    assert!(authority
        .finish_reply(request, &lease, deadline, wrong)
        .is_err());
    let (foreign, _) = mailbox
        .enqueue_resident_test_request(owner, 902, request.clone(), deadline)
        .unwrap();
    assert!(authority
        .finish_reply(request, &foreign, deadline, copy())
        .is_err());
    assert!(authority
        .finish_reply(request, &lease, Instant::now(), copy())
        .is_err());
    authority.ordinary.active.store(false, Ordering::Release);
    assert!(authority
        .finish_reply(request, &lease, deadline, copy())
        .is_err());
    facts.unwrap()
}

#[cfg(test)]
pub(in super::super::super) fn exercise_result_refusal(
    owner: &mut OwnedResident,
    geometry: &mut super::super::super::real_corridor_fixture::installed_source::InstalledFixture,
    request: &AuthorityRequest,
    mut foreign: Option<benilla_world::terrain_stream::NativeStaticMapIndex>,
) -> Result<Facts, super::super::super::source_route_filter::resident::ResidentFilterError> {
    use super::fifo::ResidentFifoSelection;
    use benilla_world::terrain_stream::NativeStaticMapIndex;
    let (_, mailbox) = paired_test_mailbox(owner).unwrap();
    let mut selector = ResidentFifoSelection::new(&mailbox).unwrap();
    let deadline = Instant::now() + Duration::from_secs(120);
    let (lease, receive) = mailbox
        .enqueue_resident_test_request(owner, 903, request.clone(), deadline)
        .unwrap();
    let installed = geometry
        .app
        .world()
        .resource::<NativeStaticMapIndex>()
        .source_provenance()
        .unwrap()
        .clone();
    let mut error = None;
    while error.is_none() {
        assert!(Instant::now() < deadline);
        selector.serve_results(
            &mailbox,
            owner,
            8,
            deadline,
            |owner, job, observation| {
                let observation = observation.unwrap();
                assert!(job.lease().same_job(&lease));
                // Mutate only after the successful source worker observation is consumed.
                let original = foreign.take().map(|foreign| {
                    assert!(!installed.same_owner(foreign.source_provenance().unwrap()));
                    let original = geometry
                        .app
                        .world_mut()
                        .remove_resource::<NativeStaticMapIndex>()
                        .unwrap();
                    geometry.app.insert_resource(foreign);
                    original
                });
                let obstacle = if original.is_none() {
                    let Query::SourceMotion(MotionQuery::Route(route)) = &request.query else {
                        unreachable!()
                    };
                    let a = route.request.origin;
                    let b = route.request.goal;
                    let middle = [
                        (a.x.as_f32() + b.x.as_f32()) * 0.5,
                        (a.y.as_f32() + b.y.as_f32()) * 0.5,
                        a.z.as_f32() + 1.,
                    ];
                    Some(
                        geometry
                            .app
                            .world_mut()
                            .spawn((
                                avian3d::prelude::Collider::cuboid(2., 3., 0.25),
                                avian3d::prelude::RigidBody::Static,
                                avian3d::prelude::CollisionLayers::DEFAULT,
                                bevy::prelude::Transform::from_translation(
                                    benilla_assets::coords::wow_to_bevy(middle),
                                ),
                            ))
                            .id(),
                    )
                } else {
                    None
                };
                geometry.advance_physics();
                let result = owner.certify_observed_result(
                    observation,
                    job,
                    geometry.app.world_mut(),
                    &installed,
                );
                if let Some(original) = original {
                    geometry.app.insert_resource(original);
                }
                if let Some(obstacle) = obstacle {
                    geometry.app.world_mut().despawn(obstacle);
                }
                error = Some(match result {
                    Ok(_) => panic!("changed current geometry cannot mint a selected result"),
                    Err(error) => error,
                });
                Err(Failure::Binding)
            },
            |_, _| panic!("resident cannot enter ordinary provider"),
        );
        std::thread::yield_now();
    }
    assert!(matches!(receive.try_result(), Ok(Err(Failure::Binding))));
    assert!(!lease.is_active());
    Err(error.unwrap())
}

#[cfg(test)]
pub(in super::super::super) fn exercise_result_terminal_retirement(
    owner: &mut OwnedResident,
    geometry: &mut super::super::super::real_corridor_fixture::installed_source::InstalledFixture,
    request: &AuthorityRequest,
    expire: bool,
) -> Facts {
    use super::fifo::ResidentFifoSelection;
    use super::super::super::source_route_filter::resident::owned::ResidentPhaseResult;
    use benilla_world::terrain_stream::NativeStaticMapIndex;
    let (_, mailbox) = paired_test_mailbox(owner).unwrap();
    let mut selector = ResidentFifoSelection::new(&mailbox).unwrap();
    let watchdog = Instant::now() + Duration::from_secs(120);
    let original_deadline = if expire {
        Instant::now() + Duration::from_secs(5)
    } else {
        watchdog
    };
    let (lease, receive) = mailbox
        .enqueue_resident_test_request(owner, 904, request.clone(), original_deadline)
        .unwrap();
    let installed = geometry
        .app
        .world()
        .resource::<NativeStaticMapIndex>()
        .source_provenance()
        .unwrap()
        .clone();
    let retained = matches!(&request.query, Query::SourceMotion(MotionQuery::Route(route)) if route.request.prior_cache.is_some());
    let mut tail = Vec::new();
    let mut facts = None;
    while facts.is_none() {
        assert!(Instant::now() < watchdog);
        let slice = if expire {
            watchdog
        } else {
            Instant::now() + Duration::from_secs(2)
        };
        selector.serve_phased_results(
            &mailbox,
            owner,
            8,
            slice,
            |owner, job, phase| {
                assert!(job.lease().same_job(&lease));
                geometry.advance_physics();
                let result = owner
                    .certify_phase_result(
                        phase,
                        job,
                        geometry.app.world_mut(),
                        &installed,
                    )
                    .unwrap();
                let ResidentPhaseResult::Complete(result) = result else {
                    return Ok(result);
                };
                facts = Some(result.facts().clone());
                if !expire {
                    for ticket in 0..QUEUE_CAPACITY {
                        tail.push(
                            mailbox
                                .enqueue_resident_test_request(
                                    owner,
                                    1000 + ticket as u64,
                                    request.clone(),
                                    watchdog,
                                )
                                .unwrap(),
                        );
                    }
                    assert_eq!(mailbox.rx.len(), QUEUE_CAPACITY);
                    assert!(matches!(
                        mailbox.enqueue_resident_test_request(
                            owner,
                            9999,
                            request.clone(),
                            watchdog
                        ),
                        Err(Failure::QueueCapacity)
                    ));
                }
                let until = if expire { original_deadline } else { slice };
                std::thread::sleep(
                    until.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
                );
                Ok(ResidentPhaseResult::Complete(result))
            },
            |_, _| panic!("selected resident Work only"),
        );
        std::thread::yield_now();
    }
    assert!(!lease.is_active());
    if expire {
        assert!(matches!(
            receive.try_result(),
            Err(crossbeam_channel::TryRecvError::Disconnected)
        ));
        assert!(mailbox.rx.is_empty());
    } else {
        assert!(matches!(
            receive.try_result(),
            Ok(Err(Failure::QueueCapacity))
        ));
        assert_eq!(mailbox.rx.len(), QUEUE_CAPACITY);
        assert!(tail.iter().all(|(lease, _)| lease.is_active()));
    }
    owner
        .inspect(
            &installed,
            watchdog,
            || false,
            |_, _, ledger| {
                assert_eq!(ledger.counts(), (usize::from(retained), usize::from(retained)));
                assert_eq!(ledger.next_transport_candidate(), if retained { 2 } else { 1 });
                Ok(())
            },
        )
        .unwrap();
    facts.unwrap()
}

#[test]
fn default_route_budget_matches_ordinary_wait_not_controller_proof() {
    assert_eq!(RouteBudget::Default.wait(), WAIT);
    assert_eq!(RouteBudget::Default.wait(), Duration::from_millis(500));
    assert_eq!(RouteBudget::ControllerProof.wait(), Duration::from_secs(60));
}
