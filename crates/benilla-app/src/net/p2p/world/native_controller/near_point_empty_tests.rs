// SPDX-License-Identifier: GPL-3.0-or-later
//! Separately selected empty-selector facts over actual loaded products and the
//! unchanged mailbox deadline. All other component authorities remain synthetic.
use super::*;
use crate::net::p2p::native_geometry::{
    authored_empty_near_point_revision, SourceEmptySelectorNearPointProvider,
};
use benilla_sim::near_point;

struct EmptyProduct {
    provider: SourceEmptySelectorNearPointProvider,
    session: Session,
    fault: Arc<AtomicU64>,
    observed: Arc<Mutex<Vec<n::AuthorityRequest>>>,
    failures: Arc<Mutex<Vec<Failure>>>,
    service_step: Arc<Mutex<()>>,
    other: source::SyntheticMotionAuthority,
}

fn empty_setup(z: f32, wall: bool) -> (n::Profile, EmptyProduct) {
    // Reuse the controller's source-profile/session fixture. Loading and binding
    // both authored snapshots happens before the mailbox service window starts.
    let (mut data, base) = super::setup(z);
    let (products, height) = authored_ground_height();
    let proof = base.session.participants.as_ref().unwrap();
    let snapshot = Arc::new(products)
        .load_map_height_owned(height, Some(proof))
        .unwrap();
    data.geometry_revision = authored_empty_near_point_revision(&snapshot, base.config).unwrap();
    assert_ne!(data.geometry_revision, base.session.geometry);
    if wall {
        data.npcs[0].ai.spawn = ai::Point::new(MID - 26., MID - 1., z).unwrap();
        data.player.start = ai::Point::new(MID - 42., MID - 1., z).unwrap();
    }
    data.initialize_creature_capabilities(source::creature_capability_sources(&data))
        .unwrap();
    let mut session = base.session;
    session.profile = data.digest();
    session.geometry = data.geometry_revision;
    session.primary_near_point = None;
    session.empty_selector_near_point =
        Some(core::EmptySelectorNearPointPolicy::from_profile(&data).unwrap());
    let provider =
        SourceEmptySelectorNearPointProvider::bind(&session, &data, snapshot, base.config).unwrap();
    (
        data,
        EmptyProduct {
            provider,
            session,
            fault: base.fault,
            observed: base.observed,
            failures: base.failures,
            service_step: Arc::new(Mutex::new(())),
            other: base.other,
        },
    )
}

fn empty_mailbox(product: EmptyProduct) -> MailboxAuthority {
    let EmptyProduct {
        provider,
        session,
        fault,
        observed,
        failures,
        service_step,
        other,
    } = product;
    let (local, mailbox) = core::mailbox(session).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let service_stop = Arc::clone(&stop);
    let controller_thread = std::thread::current().id();
    let service = std::thread::spawn(move || {
        assert_ne!(std::thread::current().id(), controller_thread);
        while !service_stop.load(Ordering::Acquire) {
            // Tests can drain a callback still running when the request timer expires.
            let step = service_step.lock().unwrap();
            let count = mailbox.serve_queries_until(1, |session, request, _, deadline| {
                observed.lock().unwrap().push(request.clone());
                let mode = fault.load(Ordering::SeqCst);
                let mut session = session.clone();
                if mode == 2 {
                    session.geometry[0] ^= 1;
                }
                if mode == 5 {
                    std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
                }
                let result = if mode == 1 {
                    Err(Failure::MissingGeometry)
                } else {
                    // No renewed, extended or replacement provider deadline.
                    provider.resolve(&session, request, deadline)
                };
                let mut facts = result.inspect_err(|&failure| {
                    failures.lock().unwrap().push(failure);
                })?;
                let n::Facts::SourceMotion(n::MotionFacts::Spatial(reply)) = &mut facts else {
                    unreachable!()
                };
                match mode {
                    3 => {
                        let motion::Query::NearPoint(query) = &mut reply.request else {
                            unreachable!()
                        };
                        query.binding.query_id += 1;
                    }
                    4 => reply.fact = motion::Fact::Missing,
                    _ => {}
                }
                Ok(facts)
            });
            assert!(count <= 1);
            drop(step);
            if count == 0 {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    });
    MailboxAuthority {
        local,
        submissions: Arc::new(AtomicU64::new(0)),
        other,
        stop,
        service: Some(service),
    }
}

fn assert_exact_spline(
    c: &NativeController,
    data: &n::Profile,
    events: &[SessionEvent],
    expected: ai::Point,
) {
    let npc = &c.world.state().npcs[&2];
    let actual = npc.source_motion.as_ref().unwrap().motion.snapshot();
    let active = actual.spline.as_ref().unwrap();
    assert_eq!(active.route.source, motion::RouteSource::SyntheticFixture);
    assert_eq!(active.route.points, vec![data.npcs[0].ai.spawn, expected]);
    let mut spline = motion::LinearSpline::launch(
        vec![data.npcs[0].ai.spawn, expected],
        data.npcs[0].ai.spawn,
        data.npc_motion(2).unwrap().npc_policy.run_speed,
    )
    .unwrap();
    spline.advance(data.tick_ms);
    assert_eq!(active.spline, spline.snapshot());
    assert_eq!(npc.ai.position(), spline.position().unwrap());
    assert_eq!(actual.map_pose.position, npc.ai.position());
    let guid = c.projection.npc_guid(npc.id).unwrap();
    let launches: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            SessionEvent::MonsterMove {
                guid: event_guid,
                stop: false,
                start,
                path,
                duration_ms,
                ..
            } if *event_guid == guid => Some((start, path, duration_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(launches.len(), 1);
    assert_eq!(
        launches[0].0.map(f32::to_bits),
        point_bits(data.npcs[0].ai.spawn)
    );
    assert_eq!(
        launches[0].1.last().unwrap().map(f32::to_bits),
        point_bits(expected)
    );
    assert_eq!(*launches[0].2, spline.duration_ms());
}

#[test]
fn empty_selector_vertical_exhaustion_commits_exact_final_point_replays_and_reopens() {
    let h = Harness::new();
    // The first and every free point have abs(7-11) >= 2.5+.5+.5.
    // Only the source's exhausted final return can produce this destination.
    let (data, product) = empty_setup(11., false);
    let observed = Arc::clone(&product.observed);
    let failures = Arc::clone(&product.failures);
    let mut c = create(&h, data.clone(), empty_mailbox(product));
    c.initial_events().unwrap();
    let genesis = c.world.clone();
    let events = pump_to(&mut c, 2);
    let records = n::decode_records(&n::encode_records(c.world.records()).unwrap()).unwrap();
    assert_eq!(records, c.world.records());
    let reply = near_reply(&records);
    let n::Query::SourceMotion(n::MotionQuery::NearPoint(query)) = &reply.request.query else {
        unreachable!()
    };
    assert_eq!(reply.request.binding.world, [8; 16]);
    assert_eq!(reply.request.binding.profile, data.digest());
    assert_eq!(reply.request.binding.geometry, data.geometry_revision);
    assert_eq!(reply.request.binding.tick, 2);
    assert_eq!(reply.request.binding.npc, Some(c.world.state().npcs[&2].id));
    assert_eq!(query.request.owner.position, data.npcs[0].ai.spawn);
    assert_eq!(query.request.target.pose.position, data.player.start);
    assert_eq!(query.request.binding.now_ms, 200);
    assert_eq!(query.request.binding.identity.map, data.map_id);
    assert_eq!(
        query.request.binding.identity.content.0,
        data.content_digest
    );
    assert_eq!(
        query.request.binding.identity.geometry.0,
        data.geometry_revision
    );
    assert_eq!(
        &query.searcher,
        c.world.state().npcs[&2]
            .creature_capabilities()
            .unwrap()
            .witness()
    );
    let n::Facts::SourceMotion(n::MotionFacts::Spatial(spatial)) = &reply.facts else {
        unreachable!()
    };
    assert_eq!(
        spatial.request,
        motion::Query::NearPoint(query.request.clone())
    );
    let expected = ai::Point::new(MID - 52.5, MID - 1., 7.).unwrap();
    assert_eq!(spatial.fact, motion::Fact::NearPoint(expected));
    assert!(!near_point::primary::vertical_acceptance_bits(
        query.request.target.pose.position.z.bits(),
        expected.z.bits(),
        query.request.distance.bits(),
        query.request.owner_bounding_radius.bits(),
        query.request.target.bounding_radius.bits(),
    )
    .unwrap());
    assert_eq!(*observed.lock().unwrap(), vec![reply.request.clone()]);
    assert!(failures.lock().unwrap().is_empty());
    assert_exact_spline(&c, &data, &events, expected);
    let mut replay = genesis.clone();
    replay.replay(&records).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), c.world.checkpoint().unwrap());

    for field in 0..3 {
        let mut changed = records.clone();
        let reply = changed
            .iter_mut()
            .flat_map(|record| &mut record.replies)
            .find(|reply| {
                matches!(
                    reply.request.query,
                    n::Query::SourceMotion(n::MotionQuery::NearPoint(_))
                )
            })
            .unwrap();
        match field {
            0 => reply.request.binding.geometry[0] ^= 1,
            1 => reply.request.binding.npc.as_mut().unwrap().generation += 1,
            _ => {
                let n::Facts::SourceMotion(n::MotionFacts::Spatial(spatial)) = &mut reply.facts
                else {
                    unreachable!()
                };
                spatial.fact = motion::Fact::NearPoint(data.player.start);
            }
        }
        let mut rejected = genesis.clone();
        assert!(rejected.replay(&changed).is_err(), "field {field}");
        assert_eq!(
            rejected.checkpoint().unwrap(),
            genesis.checkpoint().unwrap()
        );
    }
    let digest = c.world.state_digest();
    c.shutdown().unwrap();
    drop(c);
    let persisted = durable_files(&h);
    let mut changed_geometry = data.clone();
    changed_geometry.geometry_revision[0] ^= 1;
    let (_, product) = empty_setup(11., false);
    let rejected_requests = Arc::clone(&product.observed);
    assert!(NativeController::enter_component_test(
        &h.root,
        &h.profile,
        &h.character,
        Entry::Open { world: [8; 16] },
        changed_geometry,
        &GroundCatalog,
        empty_mailbox(product),
    )
    .is_err());
    assert_eq!(durable_files(&h), persisted);
    assert!(rejected_requests.lock().unwrap().is_empty());
    let (reopened_profile, product) = empty_setup(11., false);
    assert_eq!(reopened_profile, data);
    let reopened = enter(
        &h,
        Entry::Open { world: [8; 16] },
        data,
        empty_mailbox(product),
    );
    assert_eq!(reopened.world.state_digest(), digest);
}

#[test]
fn empty_selector_wall_rejection_returns_the_first_clear_free_candidate() {
    let h = Harness::new();
    let (data, product) = empty_setup(7., true);
    let mut c = create(&h, data.clone(), empty_mailbox(product));
    c.initial_events().unwrap();
    let events = pump_to(&mut c, 2);
    let reply = near_reply(c.world.records());
    let n::Query::SourceMotion(n::MotionQuery::NearPoint(query)) = &reply.request.query else {
        unreachable!()
    };
    let q = &query.request;
    // The wall at internal x40 blocks the primary and the first four offsets.
    // The third positive offset is the first point on the target's clear side.
    let offset =
        near_point::selector::EmptySelector::new(q.owner_bounding_radius.bits(), q.distance.bits())
            .unwrap()
            .nth(4)
            .unwrap();
    let angle = near_point::selector::candidate_angle_bits(0, offset).unwrap();
    let xy = near_point::fallback_candidate_bits(
        [data.player.start.x.bits(), data.player.start.y.bits()],
        q.distance.bits(),
        angle,
    )
    .unwrap();
    let expected = ai::Point::new(f32::from_bits(xy[0]), f32::from_bits(xy[1]), 7.).unwrap();
    let n::Facts::SourceMotion(n::MotionFacts::Spatial(spatial)) = &reply.facts else {
        unreachable!()
    };
    assert_eq!(spatial.fact, motion::Fact::NearPoint(expected));
    assert_ne!(expected, ai::Point::new(MID - 39.5, MID - 1., 7.).unwrap());
    assert_exact_spline(&c, &data, &events, expected);
}

#[test]
fn empty_selector_unavailable_retry_and_failed_save_publish_exact_staged_tick_once() {
    let h = Harness::new();
    let (data, product) = empty_setup(11., false);
    let fault = Arc::clone(&product.fault);
    let observed = Arc::clone(&product.observed);
    let failures = Arc::clone(&product.failures);
    let service_step = Arc::clone(&product.service_step);
    let mut c = create(&h, data.clone(), empty_mailbox(product));
    c.initial_events().unwrap();
    pump_to(&mut c, 1);
    let before = c.world.checkpoint().unwrap();
    let persisted = durable_files(&h);
    let generation = c.store.generation();
    let mut replay = c.world.clone();
    let moved = [MID - 49.75, MID - 1., 11.];
    c.receive(move_to(moved)).unwrap();
    let mut pending = None;
    for (attempt, mode) in [1, 5].into_iter().enumerate() {
        fault.store(mode, Ordering::SeqCst);
        if attempt > 0 {
            c.retry_tick().unwrap();
        }
        let out = c.pump(if attempt == 0 {
            c.tick_duration()
        } else {
            Duration::ZERO
        });
        assert_eq!(out.ticks, 0);
        assert!(out.events.is_empty());
        assert!(matches!(out.pause.unwrap().kind, PauseKind::Authority));
        assert_eq!(c.world.checkpoint().unwrap(), before);
        assert_eq!(c.store.generation(), generation);
        assert_eq!(durable_files(&h), persisted);
        let inputs = &c.pending.as_ref().unwrap().inputs;
        if let Some(pending) = &pending {
            assert_eq!(inputs, pending);
        } else {
            pending = Some(inputs.clone());
        }
        assert_eq!(c.position_xyz(), [MID - 50., MID - 1., 11.]);
    }
    let pending = pending.unwrap();
    // Absolute expiry can wake the controller before the final callback returns.
    let step = service_step.lock().unwrap();
    let failed_attempts = failures.lock().unwrap();
    assert_eq!(failed_attempts.first(), Some(&Failure::MissingGeometry));
    assert!(failed_attempts.len() >= 2);
    assert!(failed_attempts[1..]
        .iter()
        .all(|failure| *failure == Failure::Deadline));
    let failed_count = failed_attempts.len();
    drop(failed_attempts);
    assert_eq!(observed.lock().unwrap().len(), failed_count);
    drop(step);
    assert_eq!(c.pump(Duration::from_secs(60)).ticks, 0);
    let step = service_step.lock().unwrap();
    assert_eq!(observed.lock().unwrap().len(), failed_count);
    fault.store(0, Ordering::SeqCst);
    drop(step);
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored empty-selector save fault",
    )
    .unwrap();
    c.retry_tick().unwrap();
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 1);
    assert!(out.events.is_empty());
    assert!(matches!(
        out.pause.unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: false
        }
    ));
    assert_eq!(c.store.generation(), generation);
    assert_eq!(c.position_xyz(), [MID - 50., MID - 1., 11.]);
    assert!(c.pending.is_none());
    assert_eq!(c.world.state().next_sequence, 1);
    let record = c.world.last_tick().unwrap().clone();
    assert_eq!(
        record.inputs,
        pending
            .into_iter()
            .map(|owned| owned.input)
            .collect::<Vec<_>>()
    );
    let encoded =
        n::decode_records(&n::encode_records(std::slice::from_ref(&record)).unwrap()).unwrap();
    replay.replay(&encoded).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), c.world.checkpoint().unwrap());
    let step = service_step.lock().unwrap();
    let failed_attempts = failures.lock().unwrap();
    assert!(failed_attempts[failed_count..]
        .iter()
        .all(|failure| *failure == Failure::Deadline));
    let attempts = observed.lock().unwrap();
    // Any further slice-expiry attempts still culminate in exactly one success.
    let completed_count = attempts.len();
    assert_eq!(completed_count, failed_attempts.len() + 1);
    drop(failed_attempts);
    assert!(attempts.iter().all(|attempt| attempt == &attempts[0]));
    assert_eq!(
        near_reply(std::slice::from_ref(&record)).request,
        attempts[0]
    );
    drop(attempts);
    drop(step);
    let digest = c.world.state_digest();
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 0);
    assert_eq!(c.store.generation(), generation + 1);
    assert_eq!(c.world.state_digest(), digest);
    assert_eq!(c.position_xyz(), moved);
    assert_exact_spline(
        &c,
        &data,
        &out.events,
        ai::Point::new(MID - 52.25, MID - 1., 7.).unwrap(),
    );
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(observed.lock().unwrap().len(), completed_count);
    assert_eq!(c.world.state().next_sequence, 1);
    assert!(c.world.records().is_empty());
    c.shutdown().unwrap();
    drop(c);
    let (_, product) = empty_setup(11., false);
    let reopened = enter(
        &h,
        Entry::Open { world: [8; 16] },
        data,
        empty_mailbox(product),
    );
    assert_eq!(reopened.world.state_digest(), digest);
    assert_eq!(reopened.world.state().next_sequence, 1);
}

#[test]
fn empty_selector_binding_and_malformed_replies_never_commit_or_publish() {
    for mode in 2..=4 {
        let h = Harness::new();
        let (data, product) = empty_setup(11., false);
        let fault = Arc::clone(&product.fault);
        let observed = Arc::clone(&product.observed);
        let failures = Arc::clone(&product.failures);
        let mut c = create(&h, data, empty_mailbox(product));
        c.initial_events().unwrap();
        pump_to(&mut c, 1);
        let before = c.world.checkpoint().unwrap();
        let persisted = durable_files(&h);
        let generation = c.store.generation();
        c.receive(move_to([MID - 49.75, MID - 1., 11.])).unwrap();
        fault.store(mode, Ordering::SeqCst);
        let out = c.pump(c.tick_duration());
        assert_eq!(out.ticks, 0, "mode {mode}");
        assert!(out.events.is_empty());
        assert!(matches!(out.pause.unwrap().kind, PauseKind::Simulation));
        let pending = c.pending.as_ref().unwrap().inputs.clone();
        assert_eq!(c.world.checkpoint().unwrap(), before);
        assert_eq!(c.store.generation(), generation);
        assert_eq!(durable_files(&h), persisted);
        assert_eq!(c.position_xyz(), [MID - 50., MID - 1., 11.]);
        assert_eq!(observed.lock().unwrap().len(), 1);
        if mode == 2 {
            assert_eq!(*failures.lock().unwrap(), vec![Failure::Binding]);
        }
        assert!(c.retry_tick().is_err());
        let waiting = c.pump(Duration::from_secs(60));
        assert_eq!(waiting.ticks, 0);
        assert!(waiting.events.is_empty());
        assert_eq!(c.pending.as_ref().unwrap().inputs, pending);
        assert_eq!(c.world.checkpoint().unwrap(), before);
        assert_eq!(durable_files(&h), persisted);
        assert_eq!(observed.lock().unwrap().len(), 1);
    }
}
