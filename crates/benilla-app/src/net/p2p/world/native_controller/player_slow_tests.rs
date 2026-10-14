// SPDX-License-Identifier: GPL-3.0-or-later
//! Real controller/store/projection composition with explicitly authored slow effects.
use super::*;
use n::synthetic_slow_fixture::{self as slow, SyntheticSlowEffect, SyntheticSlowPhase};

fn start(h: &Harness, data: n::Profile, open: bool) -> NativeController {
    h.enter(
        if open {
            Entry::Open { world: [8; 16] }
        } else {
            Entry::Create {
                world: [8; 16],
                seed: 7,
            }
        },
        data,
        n::synthetic_motion_fixture::SyntheticMotionAuthority {
            visits: Some(false),
            ..Default::default()
        },
    )
    .unwrap()
}
fn speed_orders(events: &[SessionEvent]) -> Vec<SpeedOrder> {
    events
        .iter()
        .filter_map(|event| match event {
            SessionEvent::ForceSpeedChange {
                guid,
                kind,
                counter,
                speed,
            } => Some(SpeedOrder {
                guid: *guid,
                kind: *kind,
                counter: *counter,
                speed_bits: speed.to_bits(),
            }),
            _ => None,
        })
        .collect()
}
fn ack(order: SpeedOrder) -> ClientCommand {
    ClientCommand::ForceSpeedAck {
        guid: order.guid,
        kind: order.kind,
        counter: order.counter,
        speed: f32::from_bits(order.speed_bits),
        flags: move_flags::FORWARD,
        pos: [90.0, 0.0, 0.0],
        orientation: 2.0,
        pitch: 0.0,
        fall_time: 0,
        jump: None,
        transport: None,
    }
}
#[test]
fn exact_speed_ack_guid_counter_kind_bits_duplicates_and_out_of_order_never_mutate_pose() {
    let h = Harness::new();
    let effect = SyntheticSlowEffect {
        tick: 1,
        spell_id: 61004,
        caster: runtime::NpcId {
            spawn: 2,
            generation: 0,
        },
        phase: SyntheticSlowPhase::BeforePlayerUpdate,
    };
    let mut c = start(&h, slow::with_effects(vec![effect, effect]), false);
    let initial = speed_orders(&c.initial_events().unwrap());
    assert_eq!(
        initial.iter().map(|o| o.counter).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    let first = initial[0];
    for wrong in [
        SpeedOrder {
            guid: first.guid + 1,
            ..first
        },
        SpeedOrder {
            counter: 99,
            ..first
        },
        SpeedOrder {
            kind: SpeedKind::Swim,
            ..first
        },
        SpeedOrder {
            speed_bits: first.speed_bits + 1,
            ..first
        },
    ] {
        assert!(matches!(
            c.receive(ack(wrong)),
            Err(Error::InvalidMovementAck)
        ));
    }
    let before = c.world.state_digest();
    c.receive(ack(first)).unwrap();
    c.receive(ack(first)).unwrap();
    assert_eq!(c.world.state_digest(), before);
    assert_eq!(c.position_xyz(), [0.0; 3]);
    assert_eq!(c.facing(), 0.0);
    assert!(c.actions.is_empty());
    let batch = c.pump(c.tick_duration());
    assert!(batch.pause.is_none(), "{:?}", batch.pause);
    let orders = speed_orders(&batch.events);
    assert_eq!(orders.len(), 9);
    let run: Vec<_> = orders
        .iter()
        .copied()
        .filter(|o| o.kind == SpeedKind::Run)
        .collect();
    assert_eq!(
        run.iter().map(|o| o.counter).collect::<Vec<_>>(),
        vec![3, 6, 9]
    );
    // An earlier acknowledged duplicate never consumes the newer outstanding order.
    c.receive(ack(first)).unwrap();
    assert!(c.orders.pending_speeds.contains_key(&run[0].counter));
    let before = c.world.state_digest();
    c.receive(ack(run[2])).unwrap();
    c.receive(ack(run[0])).unwrap();
    c.receive(ack(run[1])).unwrap();
    c.receive(ack(run[2])).unwrap();
    assert!(matches!(
        c.receive(ack(run[0])),
        Err(Error::InvalidMovementAck)
    ));
    assert!(matches!(
        c.receive(ack(first)),
        Err(Error::InvalidMovementAck)
    ));
    assert_eq!(c.world.state_digest(), before);
    assert!(c.actions.is_empty());
    assert_eq!(c.position_xyz(), [0.0; 3]);
}

#[test]
fn duration_only_and_expiry_checkpoints_retry_publish_once_and_reopen_with_fresh_speeds() {
    let h = Harness::new();
    let data = slow::profile();
    let mut c = start(&h, data.clone(), false);
    c.initial_events().unwrap();
    let generation = c.store.generation();
    let apply = c.pump(c.tick_duration());
    assert!(apply.pause.is_none(), "{:?}", apply.pause);
    assert_eq!(speed_orders(&apply.events).len(), 3);
    assert_eq!(c.store.generation(), generation + 1);
    let counter = c.orders.counter.next_counter();
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored duration fault",
    )
    .unwrap();
    let duration = c.pump(c.tick_duration());
    assert!(matches!(
        duration.pause.unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: false
        }
    ));
    assert!(duration.events.is_empty());
    assert_eq!(
        c.world
            .state()
            .player_slow
            .as_ref()
            .unwrap()
            .holders()
            .next()
            .unwrap()
            .remaining_ms,
        300
    );
    assert_eq!(c.orders.counter.next_counter(), counter);
    let state = c.world.state_digest();
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    assert_eq!(c.world.state_digest(), state);
    let delivered = c.pump(Duration::ZERO);
    assert!(speed_orders(&delivered.events).is_empty());
    assert_eq!(
        delivered
            .events
            .iter()
            .filter(|event| matches!(
                event,
                SessionEvent::AuraDuration {
                    slot: 32,
                    remaining_ms: 300
                }
            ))
            .count(),
        1
    );
    assert!(!delivered.events.is_empty());
    assert!(c.pump(Duration::ZERO).events.is_empty());
    drop(c);
    let mut c = start(&h, data.clone(), true);
    assert_eq!(c.world.state_digest(), state);
    let initial = c.initial_events().unwrap();
    assert_eq!(
        speed_orders(&initial)
            .iter()
            .map(|o| o.counter)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert!(initial.iter().any(|e| matches!(
        e,
        SessionEvent::AuraDuration {
            slot: 32,
            remaining_ms: 300
        }
    )));
    assert!(c.orders.acknowledged_speeds.iter().all(Option::is_none));
    for _ in 0..2 {
        assert!(c.pump(c.tick_duration()).pause.is_none());
    }
    let generation = c.store.generation();
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored expiry fault",
    )
    .unwrap();
    let expired = c.pump(c.tick_duration());
    assert!(expired.events.is_empty());
    assert!(matches!(
        expired.pause.unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: false
        }
    ));
    assert_eq!(c.store.generation(), generation);
    assert!(c
        .world
        .state()
        .player_slow
        .as_ref()
        .unwrap()
        .holders()
        .next()
        .is_none());
    let state = c.world.state_digest();
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    assert_eq!(c.world.state_digest(), state);
    let published = c.pump(Duration::ZERO).events;
    let restored = speed_orders(&published);
    assert_eq!(restored.len(), 3);
    assert_eq!(restored[0].speed_bits, 7f32.to_bits());
    assert!(c.pump(Duration::ZERO).events.is_empty());
    drop(c);
    let mut reopened = start(&h, data, true);
    assert_eq!(reopened.world.state_digest(), state);
    let initial = reopened.initial_events().unwrap();
    assert_eq!(speed_orders(&initial)[0].speed_bits, 7f32.to_bits());
    assert!(!initial
        .iter()
        .any(|e| matches!(e, SessionEvent::AuraDuration { .. })));
}

#[test]
fn stationary_enabled_ticks_save_even_without_any_holder_or_speed_change() {
    let h = Harness::new();
    let mut c = start(&h, slow::with_effects(vec![]), false);
    c.initial_events().unwrap();
    let generation = c.store.generation();
    for _ in 0..3 {
        let out = c.pump(c.tick_duration());
        assert!(out.pause.is_none());
        assert!(out.saved.is_some());
        assert!(speed_orders(&out.events).is_empty());
    }
    assert_eq!(c.store.generation(), generation + 3);
    assert_eq!(c.world.state().next_sequence, 0);
}

#[test]
fn slow_profile_still_rejects_swimming_and_ack_pose_does_not_bypass_ground_admission() {
    let h = Harness::new();
    let mut c = start(&h, slow::profile(), false);
    let initial = speed_orders(&c.initial_events().unwrap());
    let mut command = ack(initial[2]);
    if let ClientCommand::ForceSpeedAck { flags, .. } = &mut command {
        *flags = 0x200000;
    }
    assert!(matches!(c.receive(command), Err(Error::InvalidMovementAck)));
    let mut command = move_to([1.0, 0.0, 0.0]);
    if let ClientCommand::Move { flags, .. } = &mut command {
        *flags = 0x200000;
    }
    assert!(matches!(c.receive(command), Err(Error::InvalidMovement)));
    assert!(c.actions.is_empty());
}

#[test]
fn death_root_and_slow_speeds_share_one_session_counter_and_independent_typed_acks() {
    #[derive(Default)]
    struct MeleeAuthority {
        melee: fixture::KernelAuthority,
        motion: n::synthetic_motion_fixture::SyntheticMotionAuthority,
    }
    impl n::Authority for MeleeAuthority {
        fn resolve(
            &mut self,
            request: &n::AuthorityRequest,
        ) -> Result<n::AuthorityReply, n::NativeError> {
            if matches!(request.query, n::Query::NativeMelee { .. }) {
                self.melee.resolve(request)
            } else {
                self.motion.resolve(request)
            }
        }
    }
    let h = Harness::new();
    let mut data = death_profile();
    data.player_slow = slow::profile().player_slow;
    data.source_motion = Some(n::synthetic_motion_fixture::genesis(&data));
    let mut c = h
        .enter(
            Entry::Create {
                world: [8; 16],
                seed: 7,
            },
            data,
            MeleeAuthority::default(),
        )
        .unwrap();
    let mut events = c.initial_events().unwrap();
    c.receive(cast(&c)).unwrap();
    while c.orders.pending.is_none() && c.tick() < 200 {
        let out = c.pump(c.tick_duration());
        assert!(out.pause.is_none(), "{:?}", out.pause);
        events.extend(out.events);
    }
    let root = c.orders.pending.expect("death root");
    let speeds = speed_orders(&events);
    assert!(!speeds.is_empty());
    assert_eq!(root.counter, speeds.len() as u32);
    assert_eq!(c.orders.counter.next_counter(), root.counter + 1);
    let spoof = SpeedOrder {
        counter: root.counter,
        ..speeds[0]
    };
    assert!(matches!(
        c.receive(ack(spoof)),
        Err(Error::InvalidMovementAck)
    ));
    assert!(matches!(
        c.receive(root_ack(&c, speeds[0].counter, move_flags::ROOT)),
        Err(Error::InvalidMovementAck)
    ));
    let state = c.world.state_digest();
    c.receive(ack(speeds[0])).unwrap();
    c.receive(root_ack(&c, root.counter, move_flags::ROOT))
        .unwrap();
    assert_eq!(c.world.state_digest(), state);
}

// Reuse the actual app solver. The standalone harness supplies this same module path.
use crate::net::p2p::native_geometry::core as geometry;
struct ClearAuthoredCorridor;
impl geometry::Corridor for ClearAuthoredCorridor {
    fn certify(
        &mut self,
        _: [f32; 3],
        _: [f32; 3],
        _: geometry::Body,
    ) -> Result<(), geometry::Failure> {
        Ok(())
    }
}
struct GeometryAuthority {
    motion: n::synthetic_motion_fixture::SyntheticMotionAuthority,
    observed: Arc<Mutex<Vec<n::AuthorityRequest>>>,
}
impl n::Authority for GeometryAuthority {
    fn resolve(
        &mut self,
        request: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        if matches!(request.query, n::Query::PlayerMovement { .. }) {
            self.observed.lock().unwrap().push(request.clone());
            let player = geometry::PlayerMotion {
                body: geometry::Body {
                    radius: fixture::real(0.5),
                    height: fixture::real(2.0),
                },
                speed: fixture::real(7.0),
            };
            return Ok(n::AuthorityReply {
                request: request.clone(),
                facts: geometry::solve_player(&request.query, player, &mut ClearAuthoredCorridor)
                    .map_err(|_| {
                    n::NativeError::AuthorityUnavailable("authored corridor failure")
                })?,
            });
        }
        self.motion.resolve(request)
    }
}
fn moving_to(flags: u32, x: f32) -> ClientCommand {
    let mut command = move_to([x, 0.0, 0.0]);
    if let ClientCommand::Move {
        flags: movement_flags,
        ..
    } = &mut command
    {
        *movement_flags = flags;
    }
    command
}
#[test]
fn actual_controller_runtime_geometry_and_store_use_live_run_back_and_walk_speed() {
    let h = Harness::new();
    let observed = Arc::new(Mutex::new(Vec::new()));
    let data = slow::profile();
    let authority = GeometryAuthority {
        motion: n::synthetic_motion_fixture::SyntheticMotionAuthority {
            visits: Some(false),
            ..Default::default()
        },
        observed: observed.clone(),
    };
    let mut c = h
        .enter(
            Entry::Create {
                world: [8; 16],
                seed: 7,
            },
            data.clone(),
            authority,
        )
        .unwrap();
    c.initial_events().unwrap();
    assert!(c.pump(c.tick_duration()).pause.is_none());
    // Normal 7 yd/s would admit .6; the authoritative slow permits only about .42.
    // Rejection spends no path, and accepted backtracking still consumes the shared budget.
    c.receive(moving_to(move_flags::FORWARD, 0.6)).unwrap();
    c.receive(moving_to(move_flags::FORWARD, 0.4)).unwrap();
    c.receive(moving_to(move_flags::FORWARD, 0.0)).unwrap();
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none(), "{:?}", out.pause);
    assert!(out.saved.is_some());
    assert_eq!(c.position_xyz(), [0.4, 0.0, 0.0]);
    assert_eq!(c.world.state().next_sequence, 3);
    c.receive(moving_to(move_flags::BACKWARD, 0.7)).unwrap();
    c.receive(moving_to(move_flags::FORWARD | move_flags::WALK_MODE, 0.65))
        .unwrap();
    assert!(c.pump(c.tick_duration()).pause.is_none());
    assert_eq!(c.position_xyz(), [0.65, 0.0, 0.0]);
    let observed = observed.lock().unwrap();
    assert_eq!(observed.len(), 5);
    let speeds: Vec<_> = observed
        .iter()
        .map(|request| match &request.query {
            n::Query::PlayerMovement {
                speed: Some(speed), ..
            } => speed.bits(),
            _ => panic!("enabled profiles bind live speed"),
        })
        .collect();
    let rates = c.world.state().player_slow.as_ref().unwrap().speeds();
    use benilla_sim::player_slow::MoveType;
    assert_eq!(
        speeds,
        vec![rates.speed(MoveType::Run).unwrap().bits(); 3]
            .into_iter()
            .chain([
                rates.speed(MoveType::RunBack).unwrap().bits(),
                rates.speed(MoveType::Walk).unwrap().bits(),
            ])
            .collect::<Vec<_>>()
    );
    assert!(
        matches!(&observed[2].query,n::Query::PlayerMovement {accepted_path,..} if accepted_path.len()==1)
    );
    drop(observed);
    let state = c.world.state_digest();
    drop(c);
    let reopened = start(&h, data, true);
    assert_eq!(reopened.world.state_digest(), state);
    assert_eq!(reopened.position_xyz(), [0.65, 0.0, 0.0]);
}

#[test]
fn first_apply_and_replacement_save_failure_retry_never_repeat_effects_or_publish_early() {
    for replacement in [false, true] {
        let h = Harness::new();
        let effect = SyntheticSlowEffect {
            tick: 1,
            spell_id: 61004,
            caster: runtime::NpcId {
                spawn: 2,
                generation: 0,
            },
            phase: SyntheticSlowPhase::BeforePlayerUpdate,
        };
        let data = slow::with_effects(if replacement {
            vec![effect, SyntheticSlowEffect { tick: 2, ..effect }]
        } else {
            vec![effect]
        });
        let mut c = start(&h, data.clone(), false);
        c.initial_events().unwrap();
        if replacement {
            assert!(c.pump(c.tick_duration()).pause.is_none());
        }
        let previous_holder = c
            .world
            .state()
            .player_slow
            .as_ref()
            .unwrap()
            .holders()
            .next()
            .map(|holder| holder.id);
        let counter = c.orders.counter.next_counter();
        let generation = c.store.generation();
        fs::write(
            h.path().join(".world-checkpoint.pending"),
            b"authored effect fault",
        )
        .unwrap();
        let paused = c.pump(c.tick_duration());
        assert!(matches!(
            paused.pause.unwrap().kind,
            PauseKind::Checkpoint {
                requires_reopen: false
            }
        ));
        assert!(paused.events.is_empty());
        assert_eq!(c.store.generation(), generation);
        let holder = c
            .world
            .state()
            .player_slow
            .as_ref()
            .unwrap()
            .holders()
            .next()
            .unwrap()
            .clone();
        assert_ne!(Some(holder.id), previous_holder);
        assert_eq!(holder.remaining_ms, 400);
        let expected = if replacement { 6 } else { 3 };
        let retained = speed_orders(&c.outbox.as_ref().unwrap().events);
        assert_eq!(retained.len(), expected);
        assert_eq!(retained[0].counter, counter);
        assert!(matches!(
            c.receive(ack(retained[0])),
            Err(Error::InvalidMovementAck)
        ));
        let state = c.world.state_digest();
        fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
        c.retry_checkpoint().unwrap();
        assert_eq!(c.world.state_digest(), state);
        let published = c.pump(Duration::ZERO).events;
        assert_eq!(speed_orders(&published), retained);
        assert_eq!(
            published
                .iter()
                .filter(|event| matches!(
                    event,
                    SessionEvent::AuraDuration {
                        slot: 32,
                        remaining_ms: 400
                    }
                ))
                .count(),
            1
        );
        assert!(c.pump(Duration::ZERO).events.is_empty());
        assert_eq!(
            c.world
                .state()
                .player_slow
                .as_ref()
                .unwrap()
                .holders()
                .next()
                .unwrap()
                .id,
            holder.id
        );
        drop(c);
        let mut reopened = start(&h, data, true);
        assert_eq!(reopened.world.state_digest(), state);
        assert_eq!(speed_orders(&reopened.initial_events().unwrap()).len(), 3);
        assert!(speed_orders(&reopened.pump(reopened.tick_duration()).events).is_empty());
        assert_eq!(
            reopened
                .world
                .state()
                .player_slow
                .as_ref()
                .unwrap()
                .holders()
                .next()
                .unwrap()
                .id,
            holder.id
        );
    }
}
