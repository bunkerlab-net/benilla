use super::*;
use benilla_world_runtime::{native::Binding, NpcId};

fn body() -> Body {
    Body {
        radius: ai::Real::new(0.5).unwrap(),
        height: ai::Real::new(2.0).unwrap(),
    }
}
fn movement() -> MovementRequest {
    MovementRequest {
        request_id: 1,
        command_id: 2,
        creature_id: 3,
        map_id: 0,
        origin: ai::Point::new(0.0, 0.0, 5.0).unwrap(),
        goal: ai::Point::new(4.0, 0.0, 5.0).unwrap(),
        speed: ai::Real::new(7.0).unwrap(),
        elapsed_ms: 50,
        content_revision: Revision([3; 32]),
        geometry_revision: Revision([4; 32]),
        purpose: MovementPurpose::Chase { target: 1 },
    }
}
fn request() -> AuthorityRequest {
    AuthorityRequest {
        binding: Binding {
            world: [1; 16],
            tick: 1,
            profile: [1; 32],
            authority_policy: [2; 32],
            geometry: [4; 32],
            npc: Some(NpcId {
                spawn: 3,
                generation: 0,
            }),
        },
        query: Query::Movement(movement()),
    }
}
fn session() -> Session {
    Session {
        world: [1; 16],
        profile: [1; 32],
        policy: [2; 32],
        content: [3; 32],
        geometry: [4; 32],
        map: 0,
        bodies: BTreeMap::from([(3, body())]),
        player: Some(PlayerMotion {
            body: body(),
            speed: ai::Real::new(7.0).unwrap(),
        }),
        participants: Some(NonRenderParticipants::authored_fixture([3; 32], 0)),
        static_los: None,
        primary_near_point: None,
    }
}
struct Proof(Result<(), Failure>);
impl Corridor for Proof {
    fn certify(&mut self, _: [f32; 3], _: [f32; 3], _: Body) -> Result<(), Failure> {
        self.0
    }
}

#[test]
fn advances_at_bounded_speed_and_arrives_exactly() {
    let mut m = movement();
    let out = solve(&m, body(), &mut Proof(Ok(()))).unwrap();
    let MovementResult::Progress {
        position,
        travelled,
        arrived,
        ..
    } = out.result
    else {
        panic!()
    };
    assert!(position.x.as_f32() > 0.0 && position.x.as_f32() <= 0.35);
    assert!(travelled.as_f32() <= 0.35);
    assert!(!arrived);
    m.goal = ai::Point::new(0.1, 0.0, 5.0).unwrap();
    let out = solve(&m, body(), &mut Proof(Ok(()))).unwrap();
    assert!(
        matches!(out.result,MovementResult::Progress {position,arrived:true,..} if position==m.goal)
    );
}
#[test]
fn unknown_or_obstructed_geometry_never_produces_progress() {
    for failure in [
        Failure::MissingGeometry,
        Failure::UnsupportedGround,
        Failure::Water,
    ] {
        assert_eq!(
            solve(&movement(), body(), &mut Proof(Err(failure))),
            Err(failure)
        );
    }
}
#[test]
fn diagonal_native_coordinate_rounding_obeys_software_budget() {
    for x in [0.0, 83.9, -8949.95, 32700.0] {
        for y in [0.0, 1234.5, -132.49] {
            let mut m = movement();
            m.origin = ai::Point::new(x, y, 5.0).unwrap();
            m.goal = ai::Point::new(x + 4.0, y + 3.0, 5.0).unwrap();
            let outcome = solve(&m, body(), &mut Proof(Ok(()))).unwrap();
            validate_outcome(&m, &outcome).unwrap();
        }
    }
}
#[test]
fn rejects_nonplanar_excessive_time_and_unresolved_dimensions() {
    let mut m = movement();
    m.goal.z = ai::Real::new(6.0).unwrap();
    assert_eq!(
        solve(&m, body(), &mut Proof(Ok(()))),
        Err(Failure::UnsupportedGround)
    );
    m = movement();
    m.elapsed_ms = 1001;
    assert_eq!(solve(&m, body(), &mut Proof(Ok(()))), Err(Failure::Invalid));
    let mut b = body();
    b.radius = ai::Real::ZERO;
    assert_eq!(
        solve(&movement(), b, &mut Proof(Ok(()))),
        Err(Failure::Invalid)
    );
}
#[test]
fn exact_request_and_outcome_bindings_are_checked() {
    let mut s = session();
    let r = request();
    assert!(s.body(&r).is_ok());
    s.map = 1;
    assert_eq!(s.body(&r), Err(Failure::UnprovenParticipants));
    for field in 0..6 {
        let mut r = request();
        match field {
            0 => r.binding.world = [9; 16],
            1 => r.binding.profile = [9; 32],
            2 => r.binding.authority_policy = [9; 32],
            3 => r.binding.geometry = [9; 32],
            4 => r.binding.npc.as_mut().unwrap().spawn = 99,
            _ => r.binding.tick = 0,
        }
        assert_eq!(session().body(&r), Err(Failure::Binding));
    }
    let m = movement();
    let mut out = solve(&m, body(), &mut Proof(Ok(()))).unwrap();
    out.request_id += 1;
    assert_eq!(validate_outcome(&m, &out), Err(Failure::Binding));
}
#[test]
fn worker_roundtrip_and_service_frame_limit() {
    let (mut worker, service) = mailbox(session()).unwrap();
    let handle = std::thread::spawn(move || worker.resolve(&request()));
    let deadline = Instant::now() + Duration::from_secs(2);
    while service.rx.is_empty() {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert_eq!(
        service.serve(PER_FRAME, |m, b| solve(m, b, &mut Proof(Ok(())))),
        1
    );
    assert!(matches!(
        handle.join().unwrap().unwrap().facts,
        Facts::Movement(_)
    ));
    let (worker, service) = mailbox(session()).unwrap();
    let (tx, _rx) = bounded(1);
    for ticket in 0..QUEUE_CAPACITY {
        worker
            .tx
            .try_send(Work {
                ticket: ticket as u64,
                request: request(),
                deadline: Instant::now() + WAIT,
                reply: tx.clone(),
            })
            .unwrap();
    }
    assert_eq!(
        service.serve(usize::MAX, |m, b| solve(m, b, &mut Proof(Ok(())))),
        PER_FRAME
    );
    assert_eq!(service.rx.len(), QUEUE_CAPACITY - PER_FRAME);
}
#[test]
fn overflow_closed_and_expired_jobs_are_fail_closed() {
    let (mut worker, service) = mailbox(session()).unwrap();
    let (tx, _rx) = bounded(1);
    for ticket in 0..QUEUE_CAPACITY {
        worker
            .tx
            .try_send(Work {
                ticket: ticket as u64,
                request: request(),
                deadline: Instant::now() - WAIT,
                reply: tx.clone(),
            })
            .unwrap();
    }
    assert!(worker.resolve(&request()).is_err());
    assert_eq!(
        service.serve(PER_FRAME, |_, _| panic!("expired query must not execute")),
        PER_FRAME
    );
    drop(service);
    assert!(worker.resolve(&request()).is_err());
}
#[test]
fn forged_service_outcome_fails_before_delivery() {
    let (mut worker, service) = mailbox(session()).unwrap();
    let handle = std::thread::spawn(move || worker.resolve(&request()));
    while service.rx.is_empty() {
        std::thread::yield_now();
    }
    service.serve(1, |m, b| {
        let mut out = solve(m, b, &mut Proof(Ok(())))?;
        out.map_id += 1;
        Ok(out)
    });
    assert_eq!(handle.join().unwrap(), Err(NativeError::AuthorityBinding));
}
#[test]
fn unsupported_queries_are_never_delegated_or_guessed() {
    let (mut worker, service) = mailbox(session()).unwrap();
    let mut r = request();
    r.query = Query::Melee {
        player_health: 100,
        npc_health: 100,
    };
    assert!(matches!(
        worker.resolve(&r),
        Err(NativeError::AuthorityUnavailable(_))
    ));
    assert!(service.rx.is_empty());
}
#[test]
fn fixed_ticks_share_one_render_frame_budget() {
    let mut budget = FrameBudget::default();
    assert_eq!(budget.remaining(1), PER_FRAME);
    budget.spend(3);
    assert_eq!(budget.remaining(1), PER_FRAME - 3);
    budget.spend(5);
    assert_eq!(budget.remaining(1), 0);
    assert_eq!(budget.remaining(1), 0);
    assert_eq!(budget.remaining(2), PER_FRAME);
}

#[test]
fn known_obstacle_is_a_bound_blocked_outcome() {
    let m = movement();
    let out = solve(&m, body(), &mut Proof(Err(Failure::Obstacle))).unwrap();
    assert_eq!(out.result, MovementResult::Blocked(BlockReason::Obstacle));
    validate_outcome(&m, &out).unwrap();
}

#[test]
fn geometry_identity_binds_actual_tile_bytes_map_and_coordinates() {
    let tiles = std::array::from_fn(|i| (((i % 3) as i32, (i / 3) as i32), [i as u8; 32]));
    let wdt = [7; 32];
    let digest = geometry_revision(0, wdt, &tiles, &[], [9; 32]);
    assert_ne!(digest, geometry_revision(1, wdt, &tiles, &[], [9; 32]));
    assert_ne!(digest, geometry_revision(0, [8; 32], &tiles, &[], [9; 32]));
    for i in 0..9 {
        let mut replacement = tiles;
        replacement[i].1[0] ^= 1;
        assert_ne!(
            digest,
            geometry_revision(0, wdt, &replacement, &[], [9; 32])
        );
        replacement = tiles;
        replacement[i].0 .0 += 1;
        assert_ne!(
            digest,
            geometry_revision(0, wdt, &replacement, &[], [9; 32])
        );
    }
}

#[test]
fn render_absence_does_not_certify_world_collision_participants() {
    let mut s = session();
    s.participants = None;
    assert!(matches!(
        mailbox(s.clone()),
        Err(Failure::UnprovenParticipants)
    ));
    assert_eq!(s.body(&request()), Err(Failure::UnprovenParticipants));
    s = session();
    s.participants = Some(NonRenderParticipants::authored_fixture([9; 32], 0));
    assert!(matches!(mailbox(s), Err(Failure::UnprovenParticipants)));
    s = session();
    s.participants = Some(NonRenderParticipants::authored_fixture([3; 32], 1));
    assert!(matches!(mailbox(s), Err(Failure::UnprovenParticipants)));
}

#[test]
fn timeout_cancels_the_reply_and_late_work_never_executes() {
    let (mut worker, service) = mailbox(session()).unwrap();
    let start = Instant::now();
    assert!(matches!(
        worker.resolve(&request()),
        Err(NativeError::AuthorityUnavailable(_))
    ));
    assert!(start.elapsed() >= WAIT);
    assert_eq!(service.serve(1, |_, _| panic!("timed-out job executed")), 1);
}

#[test]
fn buffered_reply_cannot_extend_the_absolute_request_deadline() {
    let (tx, rx) = bounded(1);
    tx.try_send(Reply {
        ticket: 1,
        request: request(),
        result: solve(&movement(), body(), &mut Proof(Ok(()))).map(Facts::Movement),
    })
    .unwrap();
    // Deterministic past deadline: no sleep or scheduler timing makes the test pass.
    assert!(matches!(
        receive_reply_until(&rx, &crossbeam_channel::never(), Instant::now() - WAIT),
        Err(NativeError::AuthorityUnavailable(_))
    ));
}

#[test]
fn forged_reply_ticket_or_npc_generation_is_rejected() {
    for alter_generation in [false, true] {
        let (mut worker, service) = mailbox(session()).unwrap();
        let handle = std::thread::spawn(move || worker.resolve(&request()));
        let work = service.rx.recv_timeout(WAIT).unwrap();
        let mut reply = Reply {
            ticket: work.ticket,
            request: work.request.clone(),
            result: solve(&movement(), body(), &mut Proof(Ok(()))).map(Facts::Movement),
        };
        if alter_generation {
            reply.request.binding.npc.as_mut().unwrap().generation += 1;
        } else {
            reply.ticket += 1;
        }
        work.reply.try_send(reply).unwrap();
        assert_eq!(handle.join().unwrap(), Err(NativeError::AuthorityBinding));
    }
}

fn player_query(path: &[[f32; 3]], destination: [f32; 3]) -> Query {
    Query::PlayerMovement {
        frame_origin: ai::Point::new(0.0, 0.0, 5.0).unwrap(),
        accepted_path: path
            .iter()
            .map(|p| ai::Point::new(p[0], p[1], p[2]).unwrap())
            .collect(),
        destination: ai::Point::new(destination[0], destination[1], destination[2]).unwrap(),
        elapsed_ms: 50,
        speed: None,
    }
}
#[test]
fn player_path_uses_one_cumulative_budget_including_return_trips() {
    let player = session().player.unwrap();
    assert_eq!(
        solve_player(
            &player_query(&[], [0.25, 0.0, 5.0]),
            player,
            &mut Proof(Ok(()))
        ),
        Ok(Facts::PlayerMovement { accepted: true })
    );
    // Each 0.25 hop fits 7yd/s x 50ms, but the 0.5 round trip does not.
    assert_eq!(
        solve_player(
            &player_query(&[[0.25, 0.0, 5.0]], [0.0, 0.0, 5.0]),
            player,
            &mut Proof(Ok(()))
        ),
        Ok(Facts::PlayerMovement { accepted: false })
    );
    // Net displacement 0.125 is irrelevant: path spends 0.375.
    assert_eq!(
        solve_player(
            &player_query(&[[0.25, 0.0, 5.0]], [0.125, 0.0, 5.0]),
            player,
            &mut Proof(Ok(()))
        ),
        Ok(Facts::PlayerMovement { accepted: false })
    );
    assert_eq!(
        solve_player(
            &player_query(&[[0.125, 0.0, 5.0]], [0.25, 0.0, 5.0]),
            player,
            &mut Proof(Ok(()))
        ),
        Ok(Facts::PlayerMovement { accepted: true })
    );
}
#[test]
fn player_path_certifies_every_segment_and_distinguishes_unknown_from_obstacle() {
    struct Trace(Vec<([f32; 3], [f32; 3])>);
    impl Corridor for Trace {
        fn certify(&mut self, a: [f32; 3], b: [f32; 3], _: Body) -> Result<(), Failure> {
            self.0.push((a, b));
            Ok(())
        }
    }
    let player = session().player.unwrap();
    let q = player_query(&[[0.1, 0.0, 5.0]], [0.1, 0.1, 5.0]);
    let mut trace = Trace(Vec::new());
    solve_player(&q, player, &mut trace).unwrap();
    assert_eq!(
        trace.0,
        [
            ([0.0, 0.0, 5.0], [0.1, 0.0, 5.0]),
            ([0.1, 0.0, 5.0], [0.1, 0.1, 5.0])
        ]
    );
    assert_eq!(
        solve_player(&q, player, &mut Proof(Err(Failure::MissingGeometry))),
        Err(Failure::MissingGeometry)
    );
    assert_eq!(
        solve_player(&q, player, &mut Proof(Err(Failure::Obstacle))),
        Ok(Facts::PlayerMovement { accepted: false })
    );
    // Even a stationary request has to certify its body volume.
    let mut trace = Trace(Vec::new());
    solve_player(&player_query(&[], [0.0, 0.0, 5.0]), player, &mut trace).unwrap();
    assert_eq!(trace.0.len(), 1);
}
#[test]
fn player_path_mailbox_rejects_forged_acceptance_and_unresolved_body() {
    let mut s = session();
    let mut r = request();
    r.binding.npc = None;
    r.query = player_query(&[[0.25, 0.0, 5.0]], [0.0, 0.0, 5.0]);
    assert_eq!(
        validate_facts(&s, &r, &Facts::PlayerMovement { accepted: true }),
        Err(Failure::Binding)
    );
    r.binding.npc = Some(NpcId {
        spawn: 3,
        generation: 0,
    });
    assert_eq!(s.body(&r), Err(Failure::Binding));
    r.binding.npc = None;
    s.player = None;
    assert_eq!(s.body(&r), Err(Failure::UnresolvedBody));
}
#[test]
fn player_queries_use_bounded_mailbox_and_echo_exact_path() {
    let (mut worker, service) = mailbox(session()).unwrap();
    let mut r = request();
    r.binding.npc = None;
    r.query = player_query(&[[0.1, 0.0, 5.0]], [0.2, 0.0, 5.0]);
    let expected = r.clone();
    let handle = std::thread::spawn(move || worker.resolve(&r));
    while service.rx.is_empty() {
        std::thread::yield_now();
    }
    assert_eq!(
        service.serve_queries(1, |s, r, _| solve_player(
            &r.query,
            s.player.unwrap(),
            &mut Proof(Ok(()))
        )),
        1
    );
    let reply = handle.join().unwrap().unwrap();
    assert_eq!(reply.request, expected);
    assert_eq!(reply.facts, Facts::PlayerMovement { accepted: true });
}
#[test]
fn player_path_caps_count_time_height_and_diagonal_distance() {
    let player = session().player.unwrap();
    let mut q = player_query(&vec![[0.0, 0.0, 5.0]; 64], [0.0, 0.0, 5.0]);
    assert_eq!(
        solve_player(&q, player, &mut Proof(Ok(()))),
        Err(Failure::Invalid)
    );
    q = player_query(&[], [0.0, 0.0, 6.0]);
    assert_eq!(
        solve_player(&q, player, &mut Proof(Ok(()))),
        Err(Failure::UnsupportedGround)
    );
    q = player_query(&[], [0.25, 0.25, 5.0]);
    assert_eq!(
        solve_player(&q, player, &mut Proof(Ok(()))),
        Ok(Facts::PlayerMovement { accepted: false })
    );
    if let Query::PlayerMovement { elapsed_ms, .. } = &mut q {
        *elapsed_ms = 0;
    }
    assert_eq!(
        solve_player(&q, player, &mut Proof(Ok(()))),
        Err(Failure::Invalid)
    );
}
#[test]
fn geometry_revision_binds_loaded_m2_sources_and_membership() {
    let tiles = [((1, 1), [2; 32]); 9];
    let base = geometry_revision(0, [1; 32], &tiles, &[(42, [3; 32])], [9; 32]);
    assert_ne!(
        base,
        geometry_revision(0, [1; 32], &tiles, &[(42, [4; 32])], [9; 32])
    );
    assert_ne!(
        base,
        geometry_revision(0, [1; 32], &tiles, &[(43, [3; 32])], [9; 32])
    );
    assert_ne!(base, geometry_revision(0, [1; 32], &tiles, &[], [9; 32]));
    assert_ne!(
        base,
        geometry_revision(0, [1; 32], &tiles, &[(42, [3; 32])], [8; 32])
    );
}

#[test]
fn player_budget_never_rounds_up_fractional_millisecond_allowance() {
    let player = PlayerMotion {
        body: body(),
        speed: ai::Real::new(1.0).unwrap(),
    };
    let a = f32::from_bits(0x3a03126e);
    let b = 3.445_893_2e-11_f32;
    let c = 3.035_766e-18_f32;
    let mut q = player_query(
        &[
            [a, 0.0, 5.0],
            [0.0, 0.0, 5.0],
            [b, 0.0, 5.0],
            [0.0, 0.0, 5.0],
            [c, 0.0, 5.0],
        ],
        [0.0, 0.0, 5.0],
    );
    if let Query::PlayerMovement { elapsed_ms, .. } = &mut q {
        *elapsed_ms = 1;
    }
    assert_eq!(
        solve_player(&q, player, &mut Proof(Ok(()))),
        Ok(Facts::PlayerMovement { accepted: false })
    );
}

#[test]
fn oversized_player_path_is_rejected_before_transport_enqueue() {
    let (mut worker, service) = mailbox(session()).unwrap();
    let mut r = request();
    r.binding.npc = None;
    r.query = player_query(&vec![[0.0, 0.0, 5.0]; 64], [0.0, 0.0, 5.0]);
    assert_eq!(worker.resolve(&r), Err(NativeError::InvalidInput));
    assert!(service.rx.is_empty());
}

#[test]
fn loaded_vmap_is_an_additional_bound_corridor_veto_never_a_support_proof() {
    use super::super::{
        products::vmap::tests::snapshot,
        source_vmap::{geometry_revision, SourceVmapProvider},
    };
    let snapshot = snapshot();
    let mut s = session();
    let raw = s.geometry;
    s.content = snapshot.binding().content;
    s.participants = Some(NonRenderParticipants::authored_fixture(s.content, s.map));
    s.geometry = geometry_revision(raw, snapshot.binding());
    let mut request = request();
    request.binding.geometry = s.geometry;
    if let Query::Movement(m) = &mut request.query {
        m.content_revision = Revision(s.content);
        m.geometry_revision = Revision(s.geometry);
    }
    let provider = SourceVmapProvider::bind(&s, snapshot, raw).unwrap();
    assert_eq!(provider.raw_geometry(), raw);
    let deadline = Instant::now() + Duration::from_secs(10);
    let world = |p: [f32; 3]| [17066.666f32 - p[0], 17066.666f32 - p[1], p[2]];
    // Authored VMap has a vertical wall at internal x=40, z=0..8.
    let a = world([35., 0., 2.]);
    let b = world([45., 0., 2.]);
    let mut corridor = provider
        .corridor(&s, &request, Proof(Ok(())), deadline)
        .unwrap();
    assert_eq!(corridor.certify(a, b, body()), Err(Failure::Obstacle));
    let mut corridor = provider
        .corridor(
            &s,
            &request,
            Proof(Err(Failure::UnsupportedGround)),
            deadline,
        )
        .unwrap();
    assert_eq!(
        corridor.certify(a, b, body()),
        Err(Failure::UnsupportedGround)
    );
    let mut corridor = provider
        .corridor(&s, &request, Proof(Ok(())), deadline)
        .unwrap();
    assert_eq!(
        corridor.certify(world([35., 0., 12.]), world([45., 0., 12.]), body()),
        Ok(())
    );
    let mut no_participants = s.clone();
    no_participants.participants = None;
    assert!(matches!(
        provider.corridor(&no_participants, &request, Proof(Ok(())), deadline),
        Err(Failure::UnprovenParticipants)
    ));
    let mut changed = request.clone();
    changed.binding.tick = 0;
    assert!(matches!(
        provider.corridor(&s, &changed, Proof(Ok(())), deadline),
        Err(Failure::Binding)
    ));
    let mut changed = s.clone();
    changed.geometry = raw;
    assert!(matches!(
        provider.corridor(&changed, &request, Proof(Ok(())), deadline),
        Err(Failure::Binding)
    ));
    let mut corridor = provider
        .corridor(&s, &request, Proof(Ok(())), Instant::now())
        .unwrap();
    assert_eq!(
        corridor.certify(a, b, body()),
        Err(Failure::MissingGeometry)
    );
}

#[test]
fn loaded_vmap_cannot_reuse_an_unbound_session_or_changed_epoch_revision() {
    use super::super::{
        products::vmap::tests::snapshot,
        source_vmap::{geometry_revision, SourceVmapProvider},
    };
    let mut s = session();
    let snapshot = snapshot();
    s.content = snapshot.binding().content;
    let raw = s.geometry;
    let b = snapshot.binding();
    assert!(matches!(
        SourceVmapProvider::bind(&s, snapshot, raw),
        Err(Failure::Binding)
    ));
    let original = geometry_revision(raw, b);
    for field in 0..6 {
        let mut changed = b;
        match field {
            0 => changed.map += 1,
            1 => changed.content = [99; 32],
            2 => changed.source_epoch = [99; 32],
            3 => changed.inventory = [99; 32],
            4 => changed.policy = [99; 32],
            _ => {
                assert_ne!(original, geometry_revision([99; 32], b));
                continue;
            }
        }
        assert_ne!(original, geometry_revision(raw, changed));
    }
}

#[test]
fn replacing_service_invalidates_even_an_already_buffered_old_snapshot_reply() {
    let (mut authority, mailbox) = mailbox(session()).unwrap();
    authority.ticket = 1;
    let request = request();
    let result = solve(&movement(), body(), &mut Proof(Ok(()))).map(Facts::Movement);
    let reply = Reply {
        ticket: 1,
        request: request.clone(),
        result,
    };
    drop(mailbox); // Same drop as uninstall or World resource replacement.
    assert!(matches!(
        authority.finish_reply(&request, reply),
        Err(NativeError::AuthorityUnavailable(_))
    ));
    assert!(matches!(
        authority.resolve(&request),
        Err(NativeError::AuthorityUnavailable(_))
    ));
}

#[test]
fn replacing_service_immediately_cancels_a_queued_live_reply_channel() {
    let (authority, service) = mailbox(session()).unwrap();
    let (reply, receive) = bounded(1);
    authority
        .tx
        .try_send(Work {
            ticket: 1,
            request: request(),
            deadline: Instant::now() + WAIT,
            reply,
        })
        .unwrap();
    assert!(!service.rx.is_empty());
    assert!(matches!(
        receive.try_recv(),
        Err(crossbeam_channel::TryRecvError::Empty)
    ));
    drop(service);
    assert!(matches!(
        receive.try_recv(),
        Err(crossbeam_channel::TryRecvError::Disconnected)
    ));
    assert!(matches!(
        authority.check_active(),
        Err(NativeError::AuthorityUnavailable(_))
    ));
}

#[test]
fn service_cancellation_wins_even_if_an_old_reply_is_buffered() {
    let (reply, receive) = bounded(1);
    reply
        .try_send(Reply {
            ticket: 1,
            request: request(),
            result: solve(&movement(), body(), &mut Proof(Ok(()))).map(Facts::Movement),
        })
        .unwrap();
    let (cancel, cancelled) = bounded::<()>(0);
    drop(cancel);
    assert!(matches!(
        receive_reply_until(&receive, &cancelled, Instant::now() + WAIT),
        Err(NativeError::AuthorityUnavailable(_))
    ));
}

#[test]
fn loaded_navigation_composes_all_previous_gates_and_cannot_grant_support() {
    use super::super::{
        products::navigation::tests::products,
        source_navigation::{geometry_revision, SourceNavigationProvider},
    };
    let p = products();
    if !benilla_native_nav::SUPPORTED_ABI {
        assert!(matches!(
            p.load_navigation(),
            Err(super::super::products::Error::Unsupported)
        ));
        return;
    }
    let nav = p.load_navigation().unwrap();
    let vmap = p.load_static_vmap().unwrap();
    let mut s = session();
    let raw = s.geometry;
    s.content = nav.binding().content;
    s.participants = Some(NonRenderParticipants::authored_fixture(s.content, s.map));
    s.geometry = geometry_revision(raw, vmap.binding(), nav.binding());
    let mut request = request();
    request.binding.geometry = s.geometry;
    if let Query::Movement(m) = &mut request.query {
        m.content_revision = Revision(s.content);
        m.geometry_revision = Revision(s.geometry);
    }
    let provider = SourceNavigationProvider::bind(&s, vmap, nav, raw).unwrap();
    assert_eq!(provider.raw_geometry(), raw);
    let from = [-400., -400., 0.];
    let to = [-400., -395., 0.];
    let deadline = Instant::now() + Duration::from_secs(2);
    assert_eq!(
        provider
            .corridor(&s, &request, Proof(Ok(())), deadline)
            .unwrap()
            .certify(from, to, body()),
        Ok(())
    );
    for failure in [
        Failure::UnsupportedGround,
        Failure::Water,
        Failure::UnprovenParticipants,
        Failure::Obstacle,
    ] {
        assert_eq!(
            provider
                .corridor(&s, &request, Proof(Err(failure)), deadline)
                .unwrap()
                .certify(from, to, body()),
            Err(failure)
        );
    }
    // A route query can project to a nav surface; this direct-segment provider cannot.
    assert_eq!(
        provider
            .corridor(&s, &request, Proof(Ok(())), deadline)
            .unwrap()
            .certify([-400., -400., 1.], [-400., -395., 1.], body()),
        Err(Failure::UnsupportedNavigation)
    );
    assert_eq!(
        provider
            .corridor(&s, &request, Proof(Ok(())), Instant::now())
            .unwrap()
            .certify(from, to, body()),
        Err(Failure::MissingGeometry)
    );
    let mut changed = s.clone();
    changed.participants = None;
    assert!(matches!(
        provider.corridor(&changed, &request, Proof(Ok(())), deadline),
        Err(Failure::UnprovenParticipants)
    ));
    let mut changed = request.clone();
    changed.binding.tick = 0;
    assert!(matches!(
        provider.corridor(&s, &changed, Proof(Ok(())), deadline),
        Err(Failure::Binding)
    ));
    let mut changed = s.clone();
    changed.geometry = raw;
    assert!(matches!(
        provider.corridor(&changed, &request, Proof(Ok(())), deadline),
        Err(Failure::Binding)
    ));
    let mut enlarged = s.clone();
    enlarged.bodies.get_mut(&3).unwrap().radius = ai::Real::new(0.6).unwrap();
    assert!(matches!(
        provider.corridor(&enlarged, &request, Proof(Ok(())), deadline),
        Err(Failure::UnresolvedBody)
    ));
}

#[test]
fn navigation_revision_binds_every_snapshot_field_and_raw_support() {
    use super::super::{
        products::navigation::tests::products,
        source_navigation::{geometry_revision, SourceNavigationProvider},
    };
    let p = products();
    if !benilla_native_nav::SUPPORTED_ABI {
        assert!(matches!(
            p.load_navigation(),
            Err(super::super::products::Error::Unsupported)
        ));
        return;
    }
    let nav = p.load_navigation().unwrap();
    let vmap = p.load_static_vmap().unwrap();
    let mut s = session();
    s.content = nav.binding().content;
    let raw = s.geometry;
    let a = vmap.binding();
    let b = nav.binding();
    assert!(matches!(
        SourceNavigationProvider::bind(&s, vmap, nav, raw),
        Err(Failure::Binding)
    ));
    let original = geometry_revision(raw, a, b);
    for i in 0..5 {
        let mut changed = b;
        match i {
            0 => changed.map += 1,
            1 => changed.content = [99; 32],
            2 => changed.source_epoch = [99; 32],
            3 => changed.inventory = [99; 32],
            _ => changed.policy = [99; 32],
        };
        assert_ne!(geometry_revision(raw, a, changed), original);
    }
    assert_ne!(geometry_revision([99; 32], a, b), original);
    // A VMap snapshot from another inventory cannot be paired with these navigation bytes.
    let unrelated = super::super::products::vmap::tests::snapshot();
    assert!(matches!(
        SourceNavigationProvider::bind(&s, unrelated, p.load_navigation().unwrap(), raw),
        Err(Failure::Binding)
    ));
}

#[test]
fn source_live_speed_replaces_fixed_session_speed_without_renewing_cumulative_budget() {
    let player = session().player.unwrap();
    let mut query = player_query(&[], [0.25, 0.0, 5.0]);
    assert_eq!(
        solve_player(&query, player, &mut Proof(Ok(()))),
        Ok(Facts::PlayerMovement { accepted: true })
    );
    if let Query::PlayerMovement { speed, .. } = &mut query {
        *speed = Some(ai::Real::new(4.2).unwrap());
    }
    assert_eq!(
        solve_player(&query, player, &mut Proof(Ok(()))),
        Ok(Facts::PlayerMovement { accepted: false })
    );
    if let Query::PlayerMovement { destination, .. } = &mut query {
        *destination = ai::Point::new(0.2, 0.0, 5.0).unwrap();
    }
    assert_eq!(
        solve_player(&query, player, &mut Proof(Ok(()))),
        Ok(Facts::PlayerMovement { accepted: true })
    );
    if let Query::PlayerMovement {
        accepted_path,
        destination,
        ..
    } = &mut query
    {
        accepted_path.push(*destination);
        *destination = ai::Point::new(0.0, 0.0, 5.0).unwrap();
    }
    assert_eq!(
        solve_player(&query, player, &mut Proof(Ok(()))),
        Ok(Facts::PlayerMovement { accepted: false })
    );
    if let Query::PlayerMovement {
        accepted_path,
        speed,
        ..
    } = &mut query
    {
        accepted_path.clear();
        *speed = Some(ai::Real::new(0.0).unwrap());
    }
    assert_eq!(
        solve_player(&query, player, &mut Proof(Ok(()))),
        Ok(Facts::PlayerMovement { accepted: true })
    );
    if let Query::PlayerMovement { destination, .. } = &mut query {
        *destination = ai::Point::new(0.001, 0.0, 5.0).unwrap();
    }
    assert_eq!(
        solve_player(&query, player, &mut Proof(Ok(()))),
        Ok(Facts::PlayerMovement { accepted: false })
    );
}

fn los_request() -> AuthorityRequest {
    use benilla_world_runtime::native::{
        InteractionMoment, InteractionWitness, Orientation, SourceSpellPhase, SourceSpellRequest,
    };
    let mut r = request();
    r.query = Query::SourceSpell(SourceSpellQuery::LineOfSight(SourceSpellRequest {
        witness: InteractionWitness {
            moment: InteractionMoment::Frame { action_ordinal: 0 },
            draft: [6; 32],
        },
        scheduler: None,
        history: [7; 32],
        cast_id: 1,
        ability: 133,
        phase: SourceSpellPhase::Prepare,
        player_orientation: Orientation::new(0.0).unwrap(),
        player_position: ai::Point::new(17066.666 - 35., 17066.666, 2.).unwrap(),
        target_position: ai::Point::new(17066.666 - 45., 17066.666, 2.).unwrap(),
        player_level: 1,
        target_level: 1,
    }));
    r
}
fn los_session() -> Session {
    let mut s = session();
    s.player.as_mut().unwrap().body.height = ai::Real::new(12.).unwrap();
    s.static_los = Some(StaticLosPolicy {
        profile: s.profile,
        map: s.map,
        content: s.content,
        geometry: s.geometry,
        authority: s.policy,
        player_height: s.player.unwrap().body.height,
        npc: 3,
        npc_height: body().height,
    });
    s
}
#[test]
fn source_los_uses_distinct_endpoint_heights_and_ignores_m2_without_movement_support() {
    use super::super::{
        products::vmap::{tests::snapshot, RayRequest, StaticRay},
        source_vmap::{geometry_revision, SourceVmapProvider},
    };
    use benilla_sim::creature_motion::Pose;
    use benilla_world_runtime::native::{CombatPhase, CombatRequest, CombatWitness};
    let snap = super::super::products::authored_vmap(0);
    let binding = snap.binding();
    let mut s = los_session();
    let raw = s.geometry;
    s.content = binding.content;
    s.participants = Some(NonRenderParticipants::authored_fixture(s.content, s.map));
    s.geometry = geometry_revision(raw, binding);
    s.static_los.as_mut().unwrap().geometry = s.geometry;
    s.static_los.as_mut().unwrap().content = s.content;
    let mut r = los_request();
    r.binding.geometry = s.geometry;
    let provider = SourceVmapProvider::bind(&s, snapshot(), raw).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    for (from, to) in [
        ([35., 0., 2.], [45., 0., 2.]),
        ([35., 0., -4.], [45., 0., -4.]),
        ([100., -15., 3.], [100., -15., -7.]),
    ] {
        let world = |p: [f32; 3]| ai::Point::new(17066.666 - p[0], 17066.666 - p[1], p[2]).unwrap();
        let p = world(from);
        let n = world(to);
        let eye =
            |p: ai::Point, h: f32| [p.x.as_f32(), p.y.as_f32(), p.z.as_f32() + h].map(f32::to_bits);
        for reverse in [false, true] {
            let Query::SourceSpell(SourceSpellQuery::LineOfSight(mut q)) = los_request().query
            else {
                unreachable!()
            };
            q.player_position = p;
            q.target_position = n;
            r.query = if reverse {
                Query::SourceMotion(MotionQuery::Combat(CombatQuery::NpcLineOfSight(
                    CombatRequest {
                        phase: CombatPhase::Sight,
                        witness: CombatWitness {
                            frame: q.witness,
                            working_npc: [8; 32],
                        },
                        action: None,
                        player_pose: Pose {
                            position: p,
                            orientation: ai::Real::new(0.).unwrap(),
                        },
                        creature_pose: Pose {
                            position: n,
                            orientation: ai::Real::new(0.).unwrap(),
                        },
                        player_health: 100,
                        creature_health: 100,
                        creature_phase: ai::Phase::Idle,
                        creature_ai_target: None,
                    },
                )))
            } else {
                Query::SourceSpell(SourceSpellQuery::LineOfSight(q))
            };
            let (p, n) = (eye(p, 12.), eye(n, 2.));
            let ray = RayRequest {
                snapshot: binding,
                ticket: 1,
                from: if reverse { n } else { p },
                to: if reverse { p } else { n },
                ignore_m2: true,
            };
            if from[0] == 100. {
                let mut with_m2 = ray;
                with_m2.ignore_m2 = false;
                assert!(matches!(
                    snap.query(with_m2, deadline).unwrap().result,
                    StaticRay::Hit { .. }
                ));
                assert_eq!(snap.query(ray, deadline).unwrap().result, StaticRay::Miss);
            }
            let expected = match snap.query(ray, deadline).unwrap().result {
                StaticRay::Miss => Check::Passed,
                StaticRay::Hit { .. } => Check::Failed,
            };
            let facts = provider.line_of_sight(&s, &r, deadline).unwrap();
            assert_eq!(
                facts,
                if reverse {
                    Facts::SourceMotion(MotionFacts::Combat(CombatFacts::NpcLineOfSight(expected)))
                } else {
                    Facts::SourceSpell(SourceSpellFacts::LineOfSight(expected))
                }
            );
            validate_facts(&s, &r, &facts).unwrap();
            if reverse {
                let mut wrong_phase = r.clone();
                let Query::SourceMotion(MotionQuery::Combat(CombatQuery::NpcLineOfSight(q))) =
                    &mut wrong_phase.query
                else {
                    unreachable!()
                };
                q.phase = CombatPhase::AfterMovement;
                let q = q.clone();
                assert_eq!(
                    provider.line_of_sight(&s, &wrong_phase, deadline),
                    Err(Failure::UnsupportedQuery)
                );
                wrong_phase.query =
                    Query::SourceMotion(MotionQuery::Combat(CombatQuery::NpcAccessibility(q)));
                assert_eq!(s.body(&wrong_phase), Err(Failure::UnsupportedQuery));
            }
        }
    }
    r = los_request();
    r.binding.geometry = s.geometry;
    assert_eq!(
        provider.line_of_sight(&s, &r, Instant::now()),
        Err(Failure::MissingGeometry)
    );
    let mut changed = s.clone();
    changed.player.as_mut().unwrap().body.height = body().height;
    assert_eq!(
        provider.line_of_sight(&changed, &r, deadline),
        Err(Failure::Binding)
    );
    let mut changed = s.clone();
    changed.participants = None;
    assert_eq!(
        provider.line_of_sight(&changed, &r, deadline),
        Err(Failure::UnprovenParticipants)
    );
    let mut changed = s.clone();
    changed.static_los = None;
    assert_eq!(changed.body(&r), Err(Failure::UnsupportedQuery));
    let mut changed = s.clone();
    changed.player = None;
    assert_eq!(changed.body(&r), Err(Failure::UnresolvedBody));
    let mut changed = s.clone();
    changed.bodies.clear();
    assert_eq!(changed.body(&r), Err(Failure::UnresolvedBody));
    for field in 0..4 {
        let mut changed = s.clone();
        match field {
            0 => changed.map += 1,
            1 => changed.content = [99; 32],
            2 => changed.policy = [99; 32],
            _ => changed.geometry = raw,
        }
        assert_eq!(s.static_los.unwrap().check(&changed), Err(Failure::Binding));
    }
    let Query::SourceSpell(SourceSpellQuery::LineOfSight(q)) = &mut r.query else {
        unreachable!()
    };
    q.phase = benilla_world_runtime::native::SourceSpellPhase::Impact;
    assert_eq!(
        provider.line_of_sight(&s, &r, deadline),
        Err(Failure::UnsupportedQuery)
    );
    let Query::SourceSpell(SourceSpellQuery::LineOfSight(q)) = r.query.clone() else {
        unreachable!()
    };
    r.query = Query::SourceSpell(SourceSpellQuery::ImpactVisibility(q));
    assert_eq!(s.body(&r), Err(Failure::UnsupportedQuery));
    r = los_request();
    r.binding.geometry = raw;
    assert_eq!(
        provider.line_of_sight(&s, &r, deadline),
        Err(Failure::Binding)
    );
}

#[test]
fn source_los_mailbox_rejects_wrong_kind_changed_request_and_revoked_reply() {
    let request = los_request();
    let (mut authority, service) = mailbox(los_session()).unwrap();
    authority.ticket = 1;
    let reply = || Reply {
        ticket: 1,
        request: request.clone(),
        result: Ok(Facts::SourceSpell(SourceSpellFacts::LineOfSight(
            Check::Passed,
        ))),
    };
    let mut wrong = reply();
    wrong.result = Ok(Facts::PlayerMovement { accepted: true });
    assert_eq!(
        authority.finish_reply(&request, wrong),
        Err(NativeError::AuthorityBinding)
    );
    for field in 0..3 {
        let mut changed = reply();
        match field {
            0 => changed.request.binding.npc.as_mut().unwrap().generation += 1,
            1 => changed.request.binding.tick += 1,
            _ => {
                let Query::SourceSpell(SourceSpellQuery::LineOfSight(q)) =
                    &mut changed.request.query
                else {
                    unreachable!()
                };
                q.target_position.x = ai::Real::new(10.).unwrap();
            }
        }
        assert_eq!(
            authority.finish_reply(&request, changed),
            Err(NativeError::AuthorityBinding)
        );
    }
    assert!(authority.finish_reply(&request, reply()).is_ok());
    drop(service);
    assert!(matches!(
        authority.finish_reply(&request, reply()),
        Err(NativeError::AuthorityUnavailable(_))
    ));
    let mut missing = request;
    missing.binding.npc = None;
    assert_eq!(los_session().body(&missing), Err(Failure::Binding));
}
