// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual controller commit/projection/store boundaries with an authored control sink.
//! These tests do not claim loaded geometry, native query ownership or cache rehydration.
use super::*;

#[derive(Default)]
struct Observed {
    initialized: Vec<RouteCommitBoundary>,
    attempts: Vec<RouteCommitNotice>,
    applied: Vec<RouteCommitNotice>,
    reject_initialize: bool,
    reject_delivery: bool,
    lose_response: bool,
}
struct RecordingSink(Arc<Mutex<Observed>>);
impl RouteCommitSink for RecordingSink {
    fn initialize(&mut self, boundary: &RouteCommitBoundary) -> Result<(), n::NativeError> {
        let mut observed = self.0.lock().unwrap();
        observed.initialized.push(boundary.clone());
        if observed.reject_initialize {
            Err(n::NativeError::AuthorityUnavailable(
                "authored initialize response loss",
            ))
        } else {
            Ok(())
        }
    }
    fn deliver(&mut self, notice: &RouteCommitNotice) -> Result<(), n::NativeError> {
        let mut observed = self.0.lock().unwrap();
        observed.attempts.push(notice.clone());
        if observed.reject_delivery {
            return Err(n::NativeError::AuthorityUnavailable(
                "authored control queue unavailable",
            ));
        }
        if observed.applied.last() != Some(notice) {
            let previous = observed
                .applied
                .last()
                .map(RouteCommitNotice::boundary)
                .unwrap_or_else(|| observed.initialized.last().unwrap());
            assert_eq!(notice.boundary().session(), previous.session());
            assert_eq!(notice.boundary().tick(), previous.tick() + 1);
            assert_eq!(notice.previous_record(), previous.record());
            assert_eq!(notice.before(), previous.state());
            observed.applied.push(notice.clone());
        }
        if observed.lose_response {
            observed.lose_response = false;
            return Err(n::NativeError::AuthorityUnavailable(
                "authored applied acknowledgement loss",
            ));
        }
        Ok(())
    }
}
fn attach(c: &mut NativeController) -> Arc<Mutex<Observed>> {
    let observed = Arc::new(Mutex::new(Observed::default()));
    c.attach_route_commit_sink(RecordingSink(observed.clone()))
        .unwrap();
    observed
}
fn pending(c: &NativeController) -> RouteCommitNotice {
    c.route_commit
        .as_ref()
        .unwrap()
        .pending
        .as_ref()
        .unwrap()
        .as_ref()
        .unwrap()
        .clone()
}

#[test]
fn lost_ack_retries_exact_committed_notice_never_inputs_or_publication() {
    let h = Harness::new();
    let mut c = h.start(stationary());
    c.initial_events().unwrap();
    let observed = attach(&mut c);
    observed.lock().unwrap().lose_response = true;
    c.receive(move_to([0.25, 0.0, 0.0])).unwrap();
    let out = c.pump(c.tick_duration() * 5);
    assert_eq!(out.ticks, 1);
    assert_eq!(out.pending_ticks, 4);
    assert_eq!(
        out.pause.unwrap().kind,
        PauseKind::CommittedAwaitingRouteAck
    );
    assert!(!out.events.is_empty());
    assert!(out.saved.is_some());
    assert!(c.world.records().is_empty());
    assert!(c.world.last_tick().is_none());
    assert!(c.pending.is_none());
    assert!(c.outbox.is_none());
    assert_eq!(c.world.state().next_sequence, 1);
    let notice = pending(&c);
    let state = c.world.state_digest();
    assert_eq!(notice.boundary().tick(), 1);
    assert_eq!(notice.boundary().state(), state);
    assert_eq!(
        notice.boundary().record(),
        c.world.checkpoint().unwrap().identity().history_tip
    );
    assert!(notice.routes().is_empty());
    assert!(matches!(c.retry_tick(), Err(Error::Paused)));
    assert!(matches!(c.retry_checkpoint(), Err(Error::Paused)));
    assert!(matches!(
        c.receive(move_to([0.5, 0.0, 0.0])),
        Err(Error::Paused)
    ));
    let out = c.pump(Duration::from_secs(100));
    assert!(out.events.is_empty());
    assert_eq!(out.ticks, 0);
    assert_eq!(observed.lock().unwrap().attempts, vec![notice.clone()]);
    c.retry_route_commit().unwrap();
    assert_eq!(c.world.state_digest(), state);
    assert!(c.route_commit.as_ref().unwrap().pending.is_none());
    {
        let observed = observed.lock().unwrap();
        assert_eq!(observed.attempts, vec![notice.clone(), notice.clone()]);
        assert_eq!(observed.applied, vec![notice]);
    }
    let resumed = c.pump(Duration::ZERO);
    assert_eq!(resumed.ticks, 4);
    assert!(!resumed
        .events
        .iter()
        .any(|event| matches!(event, SessionEvent::UnitMove { .. })));
    assert_eq!(c.tick(), 5);
    assert_eq!(c.world.state().next_sequence, 1);
    assert_eq!(observed.lock().unwrap().applied.len(), 5);
}

#[test]
fn checkpoint_failure_retains_notice_and_retry_publishes_before_ack_failure() {
    let h = Harness::new();
    let mut c = h.start(stationary());
    c.initial_events().unwrap();
    let observed = attach(&mut c);
    c.receive(move_to([0.125, 0.0, 0.0])).unwrap();
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored fault",
    )
    .unwrap();
    let out = c.pump(c.tick_duration());
    assert_eq!(out.ticks, 1);
    assert!(out.events.is_empty());
    assert!(matches!(
        out.pause.unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: false
        }
    ));
    let notice = pending(&c);
    let state = c.world.state_digest();
    assert_eq!(
        notice.boundary().record(),
        c.world.last_tick().unwrap().digest()
    );
    assert!(observed.lock().unwrap().attempts.is_empty());
    assert!(c.pending.is_none());
    assert!(matches!(c.retry_route_commit(), Err(Error::Paused)));
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    assert!(c.world.last_tick().is_none());
    assert!(c.world.records().is_empty());
    assert_eq!(pending(&c), notice);
    assert_eq!(c.world.state_digest(), state);
    // Even a failed control response must not discard or defer this durable outbox.
    observed.lock().unwrap().reject_delivery = true;
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 0);
    assert!(!out.events.is_empty());
    assert_eq!(
        out.pause.unwrap().kind,
        PauseKind::CommittedAwaitingRouteAck
    );
    assert!(c.outbox.is_none());
    assert_eq!(pending(&c), notice);
    assert_eq!(c.position_xyz(), [0.125, 0.0, 0.0]);
    observed.lock().unwrap().reject_delivery = false;
    c.retry_route_commit().unwrap();
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(c.world.state_digest(), state);
    assert_eq!(c.world.state().next_sequence, 1);
    assert_eq!(
        observed.lock().unwrap().attempts,
        vec![notice.clone(), notice]
    );
}

#[test]
fn projection_failure_preserves_notice_and_committed_state_until_shutdown() {
    let h = Harness::new();
    let mut c = h.start(stationary());
    // A malformed projection gap is the existing actual projection-failure fixture.
    // Attach to the real tick-one boundary so the commit chain itself remains valid.
    c.world
        .step(1, None, &mut fixture::KernelAuthority::default())
        .unwrap();
    let observed = attach(&mut c);
    c.receive(move_to([0.125, 0.0, 0.0])).unwrap();
    let out = c.pump(c.tick_duration());
    assert_eq!(out.ticks, 1);
    assert_eq!(out.pause.unwrap().kind, PauseKind::Projection);
    let notice = pending(&c);
    assert_eq!(notice.boundary().tick(), 2);
    assert_eq!(notice.boundary().state(), c.world.state_digest());
    assert_eq!(
        notice.boundary().record(),
        c.world.last_tick().unwrap().digest()
    );
    assert!(observed.lock().unwrap().attempts.is_empty());
    assert!(c.pending.is_none());
    assert!(matches!(c.retry_tick(), Err(Error::Paused)));
    assert!(matches!(c.retry_route_commit(), Err(Error::Paused)));
    assert_eq!(c.pump(Duration::from_secs(100)).ticks, 0);
    assert_eq!(pending(&c), notice);
    let digest = c.world.state_digest();
    c.shutdown().unwrap();
    drop(c);
    let c = h.reopen(stationary());
    assert_eq!(c.world.state_digest(), digest);
    assert_eq!(c.world.state().next_sequence, 1);
    assert_eq!(c.tick(), 2);
}

#[test]
fn initialization_retry_keeps_exact_nonzero_restored_boundary_and_fresh_session() {
    let h = Harness::new();
    let mut c = h.start(stationary());
    pump_to(&mut c, 3);
    let previous = RouteCommitBoundary::capture_initial(&c.world).unwrap();
    c.shutdown().unwrap();
    drop(c);
    let mut c = h.reopen(stationary());
    let observed = Arc::new(Mutex::new(Observed {
        reject_initialize: true,
        ..Default::default()
    }));
    assert!(c
        .attach_route_commit_sink(RecordingSink(observed.clone()))
        .is_err());
    assert_eq!(
        c.pause().unwrap().kind,
        PauseKind::CommittedAwaitingRouteAck
    );
    assert_eq!(c.pump(Duration::from_secs(100)).ticks, 0);
    assert_eq!(c.tick(), 3);
    observed.lock().unwrap().reject_initialize = false;
    c.retry_route_commit().unwrap();
    {
        let observed = observed.lock().unwrap();
        assert_eq!(observed.initialized.len(), 2);
        assert_eq!(observed.initialized[0], observed.initialized[1]);
        assert_eq!(observed.initialized[0].tick(), 3);
        assert_eq!(observed.initialized[0].record(), previous.record());
        assert_eq!(observed.initialized[0].state(), previous.state());
        assert_ne!(observed.initialized[0].session(), previous.session());
    }
    c.receive(move_to([0.25, 0.0, 0.0])).unwrap();
    assert!(c.pump(c.tick_duration()).pause.is_none());
    assert!(c.world.last_tick().is_none());
    assert!(c.pump(c.tick_duration()).pause.is_none());
    let observed = observed.lock().unwrap();
    assert_eq!(observed.applied.len(), 2);
    let a = &observed.applied[0];
    let b = &observed.applied[1];
    assert!(a.routes().is_empty() && b.routes().is_empty());
    assert_eq!(a.boundary().tick(), 4);
    assert_eq!(b.boundary().tick(), 5);
    assert_eq!(b.previous_record(), a.boundary().record());
    assert_eq!(b.before(), a.boundary().state());
}

#[test]
fn unsuccessful_step_has_no_notice_and_authority_retry_still_uses_pending_inputs() {
    let h = Harness::new();
    let unavailable = Arc::new(AtomicBool::new(true));
    let queries = Arc::new(Mutex::new(vec![]));
    let mut c = h
        .enter(
            Entry::Create {
                world: [8; 16],
                seed: 7,
            },
            stationary(),
            ToggleAuthority {
                unavailable: unavailable.clone(),
                observed: queries.clone(),
                inner: Default::default(),
            },
        )
        .unwrap();
    let observed = attach(&mut c);
    c.receive(move_to([0.25, 0.0, 0.0])).unwrap();
    let digest = c.world.state_digest();
    let out = c.pump(c.tick_duration());
    assert_eq!(out.ticks, 0);
    assert_eq!(out.pause.unwrap().kind, PauseKind::Authority);
    assert_eq!(c.world.state_digest(), digest);
    assert!(c.route_commit.as_ref().unwrap().pending.is_none());
    assert!(observed.lock().unwrap().attempts.is_empty());
    assert!(matches!(c.retry_route_commit(), Err(Error::Paused)));
    unavailable.store(false, Ordering::SeqCst);
    c.retry_tick().unwrap();
    assert_eq!(c.pump(Duration::ZERO).ticks, 1);
    assert_eq!(c.world.state().next_sequence, 1);
    assert_eq!(observed.lock().unwrap().applied.len(), 1);
    let queries = queries.lock().unwrap();
    assert_eq!(queries[0], queries[1]);
}

#[test]
fn actual_driver_routes_ack_retry_without_replaying_the_committed_tick() {
    let h = Harness::new();
    let mut c = h.start(stationary());
    let observed = attach(&mut c);
    observed.lock().unwrap().reject_delivery = true;
    let (et, er) = crossbeam_channel::bounded(1000);
    let (ct, cr) = crossbeam_channel::bounded(10);
    let (rt, rr) = crossbeam_channel::bounded(1);
    ct.send(move_to([0.25, 0.0, 0.0])).unwrap();
    let worker = std::thread::spawn(move || run(&mut c, &et, &cr, &rr, &AtomicBool::new(false)));
    let mut moved = 0;
    loop {
        match er.recv_timeout(Duration::from_secs(5)).unwrap() {
            SessionEvent::UnitMove {
                position: [0.25, 0.0, 0.0],
                ..
            } => moved += 1,
            SessionEvent::Notification { text } if text.contains("World paused") => break,
            _ => {}
        }
    }
    assert_eq!(moved, 1);
    let notice = observed.lock().unwrap().attempts.last().unwrap().clone();
    observed.lock().unwrap().reject_delivery = false;
    rt.send(Control::AcknowledgeCommittedRoute).unwrap();
    loop {
        match er.recv_timeout(Duration::from_secs(5)).unwrap() {
            SessionEvent::UnitMove {
                position: [0.25, 0.0, 0.0],
                ..
            } => moved += 1,
            SessionEvent::Notification { text } if text.contains("World retry accepted") => break,
            _ => {}
        }
    }
    ct.send(ClientCommand::Logout).unwrap();
    let exit = worker.join().unwrap().unwrap();
    assert!(exit.saved.tick >= 1);
    assert_eq!(moved, 1);
    let c = h.reopen(stationary());
    assert_eq!(c.world.state().next_sequence, 1);
    assert_eq!(c.position_xyz(), [0.25, 0.0, 0.0]);
    let observed = observed.lock().unwrap();
    assert_eq!(
        observed
            .attempts
            .iter()
            .filter(|attempt| **attempt == notice)
            .count(),
        2
    );
    assert_eq!(
        observed
            .applied
            .iter()
            .filter(|applied| **applied == notice)
            .count(),
        1
    );
}

struct AuthoredCacheAuthority {
    inner: n::synthetic_motion_fixture::SyntheticMotionAuthority,
    fail_after_route: Arc<AtomicBool>,
    calculated: Arc<AtomicBool>,
    far_near_point: Arc<AtomicBool>,
}
impl n::Authority for AuthoredCacheAuthority {
    fn resolve(
        &mut self,
        request: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        use benilla_sim::creature_motion as m;
        if self.fail_after_route.load(Ordering::SeqCst) && self.calculated.load(Ordering::SeqCst) {
            return Err(n::NativeError::AuthorityUnavailable(
                "authored failure after route",
            ));
        }
        let mut reply = self.inner.resolve(request)?;
        if self.far_near_point.load(Ordering::SeqCst) {
            if let n::Query::SourceMotion(n::MotionQuery::NearPoint(near)) = &request.query {
                let n::Facts::SourceMotion(n::MotionFacts::Spatial(spatial)) = &mut reply.facts
                else {
                    panic!("expected near-point fact");
                };
                let target = near.request.target.pose.position;
                spatial.fact = m::Fact::NearPoint(
                    Point::new(
                        target.x.as_f32() + 10.0,
                        target.y.as_f32(),
                        target.z.as_f32(),
                    )
                    .unwrap(),
                );
            }
        }
        let n::Query::SourceMotion(n::MotionQuery::Route(route)) = &request.query else {
            return Ok(reply);
        };
        let n::Facts::SourceMotion(n::MotionFacts::Spatial(spatial)) = &mut reply.facts else {
            panic!("expected motion fact");
        };
        let m::Fact::Route(outcome) = &spatial.fact else {
            panic!("expected authored route");
        };
        let mut after = route
            .request
            .prior_cache
            .as_deref()
            .cloned()
            .unwrap_or_else(|| m::PathFinderCache {
                binding: m::PathFinderCacheBinding {
                    identity: route.request.binding.identity,
                    owner_generation: route.owner.npc().generation,
                    inventory: [3; 32],
                    source_epoch: [4; 32],
                    navigation_policy: [5; 32],
                },
                revision: 0,
                source: m::RouteSource::SyntheticFixture,
                poly_refs: vec![1; usize::from(m::PATHFINDER_CACHE_POINT_LIMIT)],
                active_len: 1,
                point_limit: m::PATHFINDER_CACHE_POINT_LIMIT,
                path_type: 1,
                requested_end: route.request.goal,
                include_flags: 1,
                exclude_flags: 0,
            });
        after.revision += 1;
        after.requested_end = route.request.goal;
        spatial.fact = m::Fact::RouteCalculation(Box::new(m::RouteCalculation {
            source: m::RouteSource::SyntheticFixture,
            outcome: outcome.clone(),
            prior_cache: route.request.prior_cache.clone(),
            after_cache: after,
        }));
        self.calculated.store(true, Ordering::SeqCst);
        Ok(reply)
    }
}

#[test]
fn later_frame_failure_emits_no_route_notice_then_retry_captures_exact_record_and_current_head() {
    use benilla_sim::creature_motion as m;
    use n::synthetic_motion_fixture as source;
    let h = Harness::new();
    let mut profile = source::profile();
    profile
        .initialize_creature_capabilities(source::creature_capability_sources(&profile))
        .unwrap();
    let fail = Arc::new(AtomicBool::new(true));
    let calculated = Arc::new(AtomicBool::new(false));
    let mut c = h
        .enter(
            Entry::Create {
                world: [8; 16],
                seed: 7,
            },
            profile,
            AuthoredCacheAuthority {
                inner: Default::default(),
                fail_after_route: fail.clone(),
                calculated: calculated.clone(),
                far_near_point: Arc::new(AtomicBool::new(false)),
            },
        )
        .unwrap();
    let observed = attach(&mut c);
    loop {
        assert!(c.tick() < 10);
        let before = c.world.checkpoint().unwrap();
        let attempts = observed.lock().unwrap().attempts.len();
        let out = c.pump(c.tick_duration());
        if calculated.load(Ordering::SeqCst) {
            assert_eq!(out.ticks, 0);
            assert_eq!(out.pause.unwrap().kind, PauseKind::Authority);
            assert_eq!(c.world.checkpoint().unwrap(), before);
            assert_eq!(observed.lock().unwrap().attempts.len(), attempts);
            assert!(c.route_commit.as_ref().unwrap().pending.is_none());
            break;
        }
        assert!(out.pause.is_none());
    }
    fail.store(false, Ordering::SeqCst);
    c.retry_tick().unwrap();
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 1);
    assert!(out.pause.is_none());
    let observed = observed.lock().unwrap();
    let notice = observed.applied.last().unwrap();
    let record = c.world.last_tick().unwrap();
    let routes: Vec<_> = record
        .replies
        .iter()
        .filter(|reply| {
            matches!(
                reply.request.query,
                n::Query::SourceMotion(n::MotionQuery::Route(_))
            )
        })
        .cloned()
        .collect();
    assert!(!routes.is_empty());
    assert_eq!(notice.routes(), routes);
    assert_eq!(notice.boundary().record(), record.digest());
    assert_eq!(notice.previous_record(), record.previous_record);
    assert_eq!(notice.before(), record.before);
    assert_eq!(notice.boundary().state(), record.after);
    let npc = &c.world.state().npcs[&2];
    let snapshot = npc.source_motion.as_ref().unwrap().motion.snapshot();
    let m::Generator::Chase(chase) = snapshot.generator else {
        panic!("expected chase");
    };
    let head = &notice.boundary().heads()[0];
    assert_eq!(head.npc(), npc.id);
    assert_eq!(head.construction(), chase.pathfinder_construction);
    assert_eq!(head.cache(), chase.pathfinder_cache.as_deref());
    assert_eq!(head.cache().unwrap().revision, 1);
    assert!(RouteCommitBoundary::capture_initial(&c.world).is_err());
}

#[test]
fn complete_unreachable_replan_notice_uses_new_chase_cache_and_keeps_old_spline() {
    use benilla_sim::creature_motion as m;
    use n::synthetic_motion_fixture as source;
    let h = Harness::new();
    let mut profile = source::profile();
    profile
        .initialize_creature_capabilities(source::creature_capability_sources(&profile))
        .unwrap();
    let calculated = Arc::new(AtomicBool::new(false));
    let far_near_point = Arc::new(AtomicBool::new(false));
    let mut c = h
        .enter(
            Entry::Create {
                world: [8; 16],
                seed: 7,
            },
            profile,
            AuthoredCacheAuthority {
                inner: Default::default(),
                fail_after_route: Arc::new(AtomicBool::new(false)),
                calculated: calculated.clone(),
                far_near_point: far_near_point.clone(),
            },
        )
        .unwrap();
    let observed = attach(&mut c);
    while !calculated.load(Ordering::SeqCst) {
        assert!(c.tick() < 10);
        assert!(c.pump(c.tick_duration()).pause.is_none());
    }
    let old = c.world.state().npcs[&2]
        .source_motion
        .as_ref()
        .unwrap()
        .motion
        .snapshot();
    let old_spline = old.spline.unwrap();
    assert!(old_spline.route_request.prior_cache.is_none());
    assert!(!old_spline.spline.finalized);
    far_near_point.store(true, Ordering::SeqCst);
    c.receive(move_to([-10.0, 0.0, 0.0])).unwrap();
    let first = c.tick();
    loop {
        assert!(c.tick() < first + 5);
        assert!(c.pump(c.tick_duration()).pause.is_none());
        let snapshot = c.world.state().npcs[&2]
            .source_motion
            .as_ref()
            .unwrap()
            .motion
            .snapshot();
        let m::Generator::Chase(chase) = snapshot.generator else {
            panic!("expected chase");
        };
        if chase.pathfinder_cache.as_ref().unwrap().revision == 1 {
            continue;
        }
        let retained = snapshot.spline.unwrap();
        assert_eq!(retained.spline_id, old_spline.spline_id);
        assert_eq!(retained.route_request, old_spline.route_request);
        assert_eq!(retained.route, old_spline.route);
        assert!(retained.route_request.prior_cache.is_none());
        assert!(!chase.reachable);
        let observed = observed.lock().unwrap();
        let notice = observed.applied.last().unwrap();
        assert_eq!(notice.routes().len(), 1);
        assert_eq!(
            notice.boundary().heads()[0].cache(),
            chase.pathfinder_cache.as_deref()
        );
        assert_eq!(notice.boundary().heads()[0].cache().unwrap().revision, 2);
        assert_eq!(notice.boundary().state(), c.world.state_digest());
        break;
    }
}
