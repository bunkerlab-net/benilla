// SPDX-License-Identifier: GPL-3.0-or-later
//! The exact app ingress preserves source ground flags before the runtime tick.
use super::*;
use benilla_sim::creature_motion::GroundMovementFlags;
use n::synthetic_motion_fixture as motion_fixture;

fn source_profile() -> n::Profile {
    let mut profile = fixture::profile();
    let mut genesis = motion_fixture::genesis(&profile);
    genesis.npc_spawn_orientation = n::Orientation::new(1.25).unwrap();
    profile.source_motion = Some(genesis);
    profile
}
fn movement(flags: u32, x: f32, orientation: f32) -> ClientCommand {
    ClientCommand::Move {
        kind: MoveKind::Heartbeat,
        flags,
        pos: [x, 0.0, 0.0],
        orientation,
        pitch: 0.0,
        fall_time: 0,
        jump: None,
        transport: None,
    }
}

#[test]
fn source_move_flags_and_cast_keep_exact_input_order_and_durable_movement_boundary() {
    let h = Harness::new();
    let mut c = h.start(source_profile());
    let before = c.world.state_digest();
    c.receive(movement(0x1, 0.25, 0.5)).unwrap();
    c.receive(movement(0x101, 0.5, 1.0)).unwrap();
    c.receive(cast(&c)).unwrap();
    c.receive(movement(0x10, 0.5, 1.5)).unwrap();
    c.receive(movement(0, 0.5, 2.0)).unwrap();
    assert_eq!(c.world.state_digest(), before);
    assert_eq!(
        c.world.state().player_movement_flags,
        Some(GroundMovementFlags::STOPPED)
    );
    let pending = c.next_tick().unwrap();
    assert!(
        pending.moving,
        "flags-only and turn inputs must save before publication"
    );
    assert_eq!(pending.inputs.len(), 5);
    let expected = [
        (0, 0x1, 0.25, 0.5),
        (1, 0x101, 0.5, 1.0),
        (3, 0x10, 0.5, 1.5),
        (4, 0, 0.5, 2.0),
    ];
    for (sequence, flags, x, facing) in expected {
        let input = &pending.inputs[sequence];
        assert_eq!(input.input.sequence, sequence as u64);
        assert!(matches!(&input.input.command, n::Command::MovementPose {
            destination, orientation, movement_flags,
        } if destination.x.as_f32() == x && orientation.as_f32() == facing && movement_flags.bits() == flags));
    }
    assert!(matches!(
        pending.inputs[2].input.command,
        n::Command::Cast { ability: 60001, .. }
    ));
}

#[test]
fn source_movement_rejects_root_flight_jump_transport_and_unknown_flags_before_queueing() {
    let h = Harness::new();
    let mut c = h.start(source_profile());
    for flags in [0x1000, 0x2000, 0x200, 0x2000000, 0x4000000, 0x80, u32::MAX] {
        assert!(
            matches!(
                c.receive(movement(flags, 1.0, 2.0)),
                Err(Error::InvalidMovement)
            ),
            "flags={flags:#x}"
        );
    }
    assert!(c.actions.is_empty());
    assert_eq!(c.world.state().next_sequence, 0);
    assert_eq!(
        c.world.state().player_movement_flags,
        Some(GroundMovementFlags::STOPPED)
    );
}

#[test]
fn legacy_profiles_still_queue_pose_without_source_movement_state() {
    let h = Harness::new();
    let mut c = h.start(stationary());
    c.receive(movement(0x101, 0.5, 1.25)).unwrap();
    assert_eq!(c.world.state().player_movement_flags, None);
    let pending = c.next_tick().unwrap();
    assert!(pending.moving);
    assert!(matches!(
        pending.inputs[0].input.command,
        n::Command::Pose { .. }
    ));
}

#[test]
fn stationary_source_flags_save_before_publication_and_survive_retry_and_reopen() {
    let h = Harness::new();
    let data = source_profile();
    let authority = motion_fixture::SyntheticMotionAuthority {
        visits: Some(false),
        ..Default::default()
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
    let genesis = c.world.clone();
    let generation = c.store.generation();
    c.receive(movement(0x101, 0.0, 1.5)).unwrap();
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored source flags save fault",
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
    assert_eq!(c.store.generation(), generation);
    assert_eq!(c.facing(), 0.0);
    let flags = GroundMovementFlags::from_bits(0x101).unwrap();
    assert_eq!(c.world.state().player_movement_flags, Some(flags));
    assert_eq!(c.world.state().next_sequence, 1);
    assert!(c
        .world
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|event| matches!(event,
        n::Event::PlayerMovementState { movement_flags } if *movement_flags == flags)));
    let digest = c.world.state_digest();
    let mut replay = genesis;
    replay.replay(c.world.records()).unwrap();
    assert_eq!(replay.state_digest(), digest);
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 0);
    assert!(!out.events.is_empty());
    assert_eq!(c.facing(), 1.5);
    assert!(c.pump(Duration::ZERO).events.is_empty());
    drop(c);
    let reopened = h.reopen(data);
    assert_eq!(reopened.world.state_digest(), digest);
    assert_eq!(reopened.world.state().player_movement_flags, Some(flags));
    assert_eq!(reopened.facing(), 1.5);
}
