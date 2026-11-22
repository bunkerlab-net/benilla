// SPDX-License-Identifier: GPL-3.0-or-later
//! Transport-only controls captured from genuine no-Route world transitions. The
//! callback below acknowledges delivery, never claims native candidate promotion.
use super::super::{
    products::{dry_route::tests as products_fixture, same_polygon_fixture},
    source_player_environment::SourcePlayerEnvironmentProvider,
    source_route_same_polygon::tests::composed_inputs,
};
use super::*;
use benilla_world_runtime::native::{
    synthetic_fixture as f, synthetic_motion_fixture::SyntheticMotionAuthority, Binding,
    NativeWorld,
};

fn fixture() -> (Session, RouteCommitBoundary, RouteCommitNotice) {
    let raw = same_polygon_fixture::RawMap::new(same_polygon_fixture::TerrainKind::Flat);
    let (spec, files) = same_polygon_fixture::fixture(&raw);
    let products = products_fixture::load(spec, &files);
    let inventory = raw.inventory();
    let tiles: Vec<_> = (30..=32)
        .flat_map(|y| (30..=32).map(move |x| (x, y)))
        .map(|tile| (tile, inventory.tile_digest(tile).unwrap()))
        .collect();
    let revision = geometry_revision(
        0,
        inventory.wdt_digest(),
        &tiles.try_into().unwrap(),
        &[],
        inventory.source_digest(),
    );
    let (profile, session, products) = composed_inputs(products, revision, &raw.maps());
    let providers = products.bind(&session, &profile, f::player()).unwrap();
    let mut world =
        NativeWorld::synthetic_component_test(profile, session.world, f::player(), 1234).unwrap();
    let initial = RouteCommitBoundary::capture_initial(&world).unwrap();
    struct NoRouteAuthority<'a> {
        session: &'a Session,
        environment: SourcePlayerEnvironmentProvider,
        other: SyntheticMotionAuthority,
    }
    impl Authority for NoRouteAuthority<'_> {
        fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            assert!(!matches!(
                request.query,
                Query::SourceMotion(MotionQuery::Route(_))
            ));
            if matches!(request.query, Query::PlayerEnvironment(_)) {
                let facts = self
                    .environment
                    .resolve(
                        self.session,
                        request,
                        Instant::now() + Duration::from_secs(10),
                    )
                    .map_err(Failure::native)?;
                Ok(AuthorityReply {
                    request: request.clone(),
                    facts,
                })
            } else {
                self.other.resolve(request)
            }
        }
    }
    let mut authority = NoRouteAuthority {
        session: &session,
        environment: providers.environment,
        other: SyntheticMotionAuthority {
            visits: Some(false),
            ..Default::default()
        },
    };
    world.step(1, None, &mut authority).unwrap();
    let notice = RouteCommitNotice::capture(&world, &initial).unwrap();
    assert!(notice.routes().is_empty());
    assert!(notice
        .boundary()
        .heads()
        .iter()
        .all(|head| head.cache().is_none()));
    (session, initial, notice)
}

fn player_request(session: &Session) -> AuthorityRequest {
    let position = ai::Point::new(5., 13., 0.).unwrap();
    AuthorityRequest {
        binding: Binding {
            world: session.world,
            profile: session.profile,
            authority_policy: session.policy,
            geometry: session.geometry,
            tick: 1,
            npc: None,
        },
        query: Query::PlayerMovement {
            frame_origin: position,
            accepted_path: vec![],
            destination: position,
            elapsed_ms: 100,
            speed: None,
        },
    }
}

fn enqueue(
    sink: &MailboxRouteCommitSink,
    ticket: u64,
    command: RouteCommitCommand,
    deadline: Instant,
) -> Receiver<RouteCommitReply> {
    let (reply, rx) = bounded(1);
    sink.tx
        .try_send(RouteCommitWork {
            ticket,
            command,
            deadline,
            reply,
        })
        .unwrap();
    rx
}

fn wait_for_control(service: &Mailbox) {
    let deadline = Instant::now() + WAIT;
    while service.route_rx.is_empty() {
        assert!(Instant::now() < deadline, "control was not enqueued");
        std::thread::yield_now();
    }
}

fn initialize(
    authority: &mut LocalAuthority,
    service: &Mailbox,
    boundary: &RouteCommitBoundary,
) -> MailboxRouteCommitSink {
    let mut sink = authority.take_route_commit_sink().unwrap();
    let boundary = boundary.clone();
    let handle = std::thread::spawn(move || {
        sink.initialize(&boundary).unwrap();
        sink
    });
    wait_for_control(service);
    assert_eq!(service.serve_route_commits(1, |_, _| Ok(())), 1);
    handle.join().unwrap()
}

#[test]
fn selected_route_requires_its_single_paired_sink_and_accepted_initial_reply() {
    let (session, boundary, _) = fixture();
    let request = player_request(&session);
    let (mut authority, service) = mailbox(session.clone()).unwrap();
    assert!(matches!(
        authority.resolve(&request),
        Err(NativeError::AuthorityUnavailable(_))
    ));
    assert!(service.rx.is_empty());
    let mut sink = authority.take_route_commit_sink().unwrap();
    assert!(authority.take_route_commit_sink().is_none());
    let handle = std::thread::spawn(move || {
        let result = sink.initialize(&boundary);
        (sink, result)
    });
    wait_for_control(&service);
    assert_eq!(
        service.serve_route_commits(1, |_, _| {
            assert!(!authority.route_ready.load(Ordering::Acquire));
            assert!(matches!(
                authority.resolve(&request),
                Err(NativeError::AuthorityUnavailable(_))
            ));
            Ok(())
        }),
        1
    );
    let (_, result) = handle.join().unwrap();
    assert_eq!(result, Ok(()));
    assert!(authority.route_ready.load(Ordering::Acquire));
    let mut unselected = session;
    unselected.same_polygon_route = None;
    let (mut ordinary, _) = mailbox(unselected).unwrap();
    assert!(ordinary.take_route_commit_sink().is_none());
    assert!(ordinary.route_ready.load(Ordering::Acquire));
}

#[test]
fn route_control_queue_is_one_slot_and_separate_from_query_capacity() {
    let (session, boundary, _) = fixture();
    let (mut authority, service) = mailbox(session.clone()).unwrap();
    let mut sink = authority.take_route_commit_sink().unwrap();
    let first = enqueue(
        &sink,
        1,
        RouteCommitCommand::Initialize(boundary.clone()),
        Instant::now() + WAIT,
    );
    assert_eq!(service.route_rx.len(), ROUTE_COMMIT_CAPACITY);
    assert!(matches!(
        sink.initialize(&boundary),
        Err(NativeError::AuthorityUnavailable(
            "local route commit queue full"
        ))
    ));
    assert_eq!(service.route_rx.len(), 1);
    let request = player_request(&session);
    for ticket in 0..QUEUE_CAPACITY {
        let (reply, _) = bounded(1);
        authority
            .tx
            .try_send(Work::new(
                ticket as u64,
                request.clone(),
                Instant::now() + WAIT,
                reply,
                service.active.clone(),
            ))
            .unwrap();
    }
    assert_eq!(service.rx.len(), QUEUE_CAPACITY);
    assert_eq!(
        service.serve_route_commits(0, |_, _| panic!("zero budget")),
        0
    );
    assert_eq!(service.serve_route_commits(1, |_, _| Ok(())), 1);
    assert_eq!(first.recv().unwrap().result, Ok(()));
    assert_eq!(service.rx.len(), QUEUE_CAPACITY);
    assert!(!authority.route_ready.load(Ordering::Acquire));
}

#[test]
fn route_control_deadline_is_absolute_on_service_and_receiver() {
    let (session, boundary, _) = fixture();
    let (mut authority, service) = mailbox(session).unwrap();
    let sink = authority.take_route_commit_sink().unwrap();
    let command = RouteCommitCommand::Initialize(boundary);
    let expired = enqueue(&sink, 1, command.clone(), Instant::now() - WAIT);
    assert_eq!(
        service.serve_route_commits(1, |_, _| panic!("expired control applied")),
        1
    );
    assert!(expired.recv().is_err());
    let deadline = Instant::now() + Duration::from_millis(20);
    let response = enqueue(&sink, 2, command.clone(), deadline);
    assert_eq!(
        service.serve_route_commits(1, |_, _| {
            std::thread::sleep(
                deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
            );
            Ok(())
        }),
        1
    );
    assert!(response.recv().is_err());
    assert!(!authority.route_ready.load(Ordering::Acquire));
    let (tx, rx) = bounded(1);
    tx.try_send(RouteCommitReply {
        ticket: 2,
        command,
        result: Ok(()),
    })
    .unwrap();
    assert!(receive_reply_until(&rx, &crossbeam_channel::never(), Instant::now() - WAIT).is_err());
}

#[test]
fn initial_response_loss_retries_exact_boundary_and_rejects_new_attachment() {
    let (session, boundary, _) = fixture();
    let (_, different, _) = fixture();
    let (mut authority, service) = mailbox(session).unwrap();
    let mut sink = authority.take_route_commit_sink().unwrap();
    let resend = sink.tx.clone();
    let initial = boundary.clone();
    let handle = std::thread::spawn(move || {
        let result = sink.initialize(&initial);
        (sink, result)
    });
    let work = service.route_rx.recv_timeout(WAIT).unwrap();
    let exact = work.command.clone();
    let (dropped_reply, dropped_receiver) = bounded(1);
    drop(dropped_receiver);
    resend
        .try_send(RouteCommitWork {
            ticket: work.ticket,
            command: work.command.clone(),
            deadline: work.deadline,
            reply: dropped_reply,
        })
        .unwrap();
    assert_eq!(
        service.serve_route_commits(1, |_, command| {
            assert_eq!(command, &exact);
            Ok(())
        }),
        1
    );
    assert_eq!(service.route_initial.borrow().as_ref(), Some(&boundary));
    drop(work.reply);
    let (mut sink, result) = handle.join().unwrap();
    assert!(matches!(result, Err(NativeError::AuthorityUnavailable(_))));
    assert!(!authority.route_ready.load(Ordering::Acquire));
    assert_eq!(
        sink.initialize(&different),
        Err(NativeError::AuthorityBinding)
    );
    assert!(service.route_rx.is_empty());
    let handle = std::thread::spawn(move || {
        let result = sink.initialize(&boundary);
        (sink, result)
    });
    wait_for_control(&service);
    assert_eq!(
        service.serve_route_commits(1, |_, command| {
            assert_eq!(command, &exact);
            Ok(())
        }),
        1
    );
    assert_eq!(handle.join().unwrap().1, Ok(()));
    assert!(authority.route_ready.load(Ordering::Acquire));
}

#[test]
fn dropped_control_receiver_allows_exact_duplicate_without_delivery_claim() {
    let (session, boundary, notice) = fixture();
    let (mut authority, service) = mailbox(session).unwrap();
    let mut sink = initialize(&mut authority, &service, &boundary);
    let command = RouteCommitCommand::Commit(notice.clone());
    let response = enqueue(&sink, 7, command.clone(), Instant::now() + WAIT);
    drop(response);
    let mut observed = None;
    let mut deliveries = 0;
    let mut accept_exact = |_: &Session, received: &RouteCommitCommand| {
        if let Some(previous) = &observed {
            assert_eq!(received, previous);
        } else {
            observed = Some(received.clone());
        }
        deliveries += 1;
        Ok(())
    };
    assert_eq!(service.serve_route_commits(1, &mut accept_exact), 1);
    let handle = std::thread::spawn(move || {
        let result = sink.deliver(&notice);
        (sink, result)
    });
    wait_for_control(&service);
    assert_eq!(service.serve_route_commits(1, &mut accept_exact), 1);
    assert_eq!(handle.join().unwrap().1, Ok(()));
    assert_eq!(deliveries, 2);
    assert_eq!(observed, Some(command));
}

#[test]
fn route_reply_ticket_command_and_replaced_session_are_checked() {
    let (session, boundary, notice) = fixture();
    let (mut authority, service) = mailbox(session).unwrap();
    let mut sink = initialize(&mut authority, &service, &boundary);
    let command = RouteCommitCommand::Commit(notice);
    sink.ticket = 8;
    let reply = || RouteCommitReply {
        ticket: 8,
        command: command.clone(),
        result: Ok(()),
    };
    let mut wrong_ticket = reply();
    wrong_ticket.ticket += 1;
    assert_eq!(
        sink.finish_reply(&command, wrong_ticket),
        Err(NativeError::AuthorityBinding)
    );
    let mut wrong_command = reply();
    wrong_command.command = RouteCommitCommand::Initialize(boundary);
    assert_eq!(
        sink.finish_reply(&command, wrong_command),
        Err(NativeError::AuthorityBinding)
    );
    assert_eq!(sink.finish_reply(&command, reply()), Ok(()));
    drop(service);
    assert!(matches!(
        sink.finish_reply(&command, reply()),
        Err(NativeError::AuthorityUnavailable(_))
    ));
}

#[test]
fn control_session_binding_and_uninitialized_notices_reject_before_callback() {
    let (session, boundary, notice) = fixture();
    let (_, other_boundary, other_notice) = fixture();
    let (mut authority, service) = mailbox(session).unwrap();
    let mut sink = authority.take_route_commit_sink().unwrap();
    assert!(sink.deliver(&notice).is_err());
    let early = enqueue(
        &sink,
        1,
        RouteCommitCommand::Commit(notice),
        Instant::now() + WAIT,
    );
    assert_eq!(
        service.serve_route_commits(1, |_, _| panic!("uninitialized commit")),
        1
    );
    assert_eq!(early.recv().unwrap().result, Err(Failure::Binding));
    authority.route_commit = Some(sink);
    let mut sink = initialize(&mut authority, &service, &boundary);
    assert_eq!(
        sink.deliver(&other_notice),
        Err(NativeError::AuthorityBinding)
    );
    assert_eq!(
        sink.initialize(&other_boundary),
        Err(NativeError::AuthorityBinding)
    );
    for command in [
        RouteCommitCommand::Initialize(other_boundary),
        RouteCommitCommand::Commit(other_notice),
    ] {
        let rejected = enqueue(&sink, 2, command, Instant::now() + WAIT);
        assert_eq!(
            service.serve_route_commits(1, |_, _| panic!("foreign attachment applied")),
            1
        );
        assert_eq!(rejected.recv().unwrap().result, Err(Failure::Binding));
    }
}

#[test]
fn route_control_binding_mismatch_never_enters_the_queue() {
    let (session, boundary, notice) = fixture();
    let (mut authority, service) = mailbox(session.clone()).unwrap();
    let mut sink = initialize(&mut authority, &service, &boundary);
    for field in 0..5 {
        sink.session = session.clone();
        match field {
            0 => sink.session.world[0] ^= 1,
            1 => sink.session.profile[0] ^= 1,
            2 => sink.session.geometry[0] ^= 1,
            3 => sink.session.policy[0] ^= 1,
            _ => {
                sink.session
                    .same_polygon_route
                    .as_mut()
                    .unwrap()
                    .environment
                    .player
                    .owner[0] ^= 1;
            }
        }
        assert_eq!(sink.deliver(&notice), Err(NativeError::AuthorityBinding));
        assert_eq!(
            sink.initialize(&boundary),
            Err(NativeError::AuthorityBinding)
        );
        assert!(service.route_rx.is_empty());
    }
}

#[test]
fn cancellation_drains_both_queues_and_invalidates_equal_byte_replacement() {
    let (session, boundary, notice) = fixture();
    let (mut authority, service) = mailbox(session.clone()).unwrap();
    let mut sink = initialize(&mut authority, &service, &boundary);
    let pending = enqueue(
        &sink,
        3,
        RouteCommitCommand::Commit(notice.clone()),
        Instant::now() + WAIT,
    );
    let (reply, geometry_reply) = bounded(1);
    authority
        .tx
        .try_send(Work::new(
            1,
            player_request(&session),
            Instant::now() + WAIT,
            reply,
            service.active.clone(),
        ))
        .unwrap();
    drop(service);
    assert!(pending.recv().is_err());
    assert!(geometry_reply.recv().is_err());
    assert!(sink.deliver(&notice).is_err());
    assert!(authority.resolve(&player_request(&session)).is_err());
    let (mut replacement, _) = mailbox(session).unwrap();
    let replacement_sink = replacement.take_route_commit_sink().unwrap();
    assert!(!Arc::ptr_eq(&sink.active, &replacement_sink.active));
    assert!(!replacement.route_ready.load(Ordering::Acquire));
}

#[test]
fn cancellation_wins_over_buffered_control_success() {
    let (session, boundary, _) = fixture();
    let (mut authority, service) = mailbox(session).unwrap();
    let sink = authority.take_route_commit_sink().unwrap();
    let response = enqueue(
        &sink,
        1,
        RouteCommitCommand::Initialize(boundary),
        Instant::now() + WAIT,
    );
    assert_eq!(service.serve_route_commits(1, |_, _| Ok(())), 1);
    assert_eq!(response.len(), 1);
    drop(service);
    assert!(receive_reply_until(&response, &sink.cancelled, Instant::now() + WAIT).is_err());
}
