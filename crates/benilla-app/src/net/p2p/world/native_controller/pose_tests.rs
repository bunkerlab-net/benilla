// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored native pose transactions compiled against the actual app controller.
use super::*;

fn pose_command(pos: [f32; 3], orientation: f32) -> ClientCommand {
    ClientCommand::Move {
        kind: MoveKind::Heartbeat,
        flags: 0,
        pos,
        orientation,
        pitch: 0.0,
        fall_time: 0,
        jump: None,
        transport: None,
    }
}
fn projected_pose(
    events: &[SessionEvent],
    c: &NativeController,
    pos: [f32; 3],
    facing: f32,
) -> bool {
    events.iter().any(|event| {
        matches!(event, SessionEvent::UnitMove { guid, position, orientation, .. }
        if *guid == c.player_guid() && *position == pos && *orientation == facing)
    })
}
#[test]
fn queued_pose_and_failed_save_never_change_delivered_pose_retry_publishes_once() {
    let h = Harness::new();
    let mut data = stationary();
    data.player.orientation = n::Orientation::new(0.25).unwrap();
    let mut c = h.start(data.clone());
    c.initial_events().unwrap();
    let genesis = c.world.clone();
    let generation = c.store.generation();
    c.receive(pose_command([1.0, 0.0, 0.0], -1.0)).unwrap();
    assert_eq!(c.facing(), 0.25);
    assert_eq!(c.position_xyz(), [0.0; 3]);
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored save fault",
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
    assert_eq!(c.facing(), 0.25);
    assert_eq!(c.position_xyz(), [0.0; 3]);
    assert!(projected_pose(&[c.correction()], &c, [0.0; 3], 0.25));
    let state = c.world.state_digest();
    let records = c.world.records().to_vec();
    let mut replay = genesis;
    replay.replay(&records).unwrap();
    assert_eq!(replay.state_digest(), state);
    assert!(c.retry_tick().is_err());
    assert!(c.receive(pose_command([2.0, 0.0, 0.0], 3.0)).is_err());
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    assert_eq!(c.world.state_digest(), state);
    // Receipt alone does not deliver the saved outbox.
    assert_eq!(c.facing(), 0.25);
    assert_eq!(c.position_xyz(), [0.0; 3]);
    let events = c.pump(Duration::ZERO).events;
    let facing = n::Orientation::new(-1.0).unwrap().as_f32();
    assert!(projected_pose(&events, &c, [1.0, 0.0, 0.0], facing));
    assert_eq!(c.facing(), facing);
    assert_eq!(c.position_xyz(), [1.0, 0.0, 0.0]);
    assert_eq!(c.world.state().next_sequence, 1);
    assert!(c.pump(Duration::ZERO).events.is_empty());
    drop(c);
    let mut reopened = h.reopen(data);
    assert_eq!(reopened.world.state_digest(), state);
    assert_eq!(reopened.facing(), facing);
    assert_eq!(reopened.position_xyz(), [1.0, 0.0, 0.0]);
    assert!(reopened.initial_events().unwrap().iter().any(
        |e| matches!(e, SessionEvent::ObjectMove { position, orientation, .. }
        if *position == [1.0, 0.0, 0.0] && *orientation == facing)
    ));
}

struct BlockMovement;
impl n::Authority for BlockMovement {
    fn resolve(
        &mut self,
        request: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        assert!(matches!(request.query, n::Query::PlayerMovement { .. }));
        Ok(n::AuthorityReply {
            request: request.clone(),
            facts: n::Facts::PlayerMovement { accepted: false },
        })
    }
}
#[test]
fn blocked_movement_does_not_commit_its_facing_and_turn_needs_no_geometry() {
    let h = Harness::new();
    let mut c = h
        .enter(
            Entry::Create {
                world: [8; 16],
                seed: 7,
            },
            stationary(),
            BlockMovement,
        )
        .unwrap();
    c.initial_events().unwrap();
    c.receive(pose_command([1.0, 0.0, 0.0], 2.0)).unwrap();
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none());
    assert_eq!(c.facing(), 0.0);
    assert_eq!(c.position_xyz(), [0.0; 3]);
    assert!(projected_pose(&out.events, &c, [0.0; 3], 0.0));
    c.receive(pose_command([0.0; 3], 1.0)).unwrap();
    assert_eq!(c.facing(), 0.0);
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none());
    assert!(out.saved.is_some());
    assert_eq!(c.facing(), 1.0);
    assert!(projected_pose(&out.events, &c, [0.0; 3], 1.0));
}

#[test]
fn turn_then_cast_preserves_source_order_and_exact_orientation_witness() {
    let h = Harness::new();
    let observed = Arc::new(Mutex::new(vec![]));
    let mut c = h
        .enter(
            Entry::Create {
                world: [8; 16],
                seed: 7,
            },
            fixture::profile(),
            ToggleAuthority {
                unavailable: Arc::new(AtomicBool::new(false)),
                observed: observed.clone(),
                inner: Default::default(),
            },
        )
        .unwrap();
    c.initial_events().unwrap();
    c.receive(pose_command([0.0; 3], 1.0)).unwrap();
    c.receive(cast(&c)).unwrap();
    c.receive(pose_command([0.0; 3], 2.0)).unwrap();
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none());
    assert!(out.saved.is_some());
    assert_eq!(c.facing(), 2.0);
    assert_eq!(c.world.state().next_sequence, 3);
    let ordered: Vec<_> = out
        .events
        .iter()
        .filter_map(|event| match event {
            SessionEvent::ObjectMove {
                guid, orientation, ..
            } if *guid == c.player_guid() => Some(*orientation),
            SessionEvent::SpellStart { .. } => Some(-1.0),
            _ => None,
        })
        .collect();
    assert_eq!(ordered, vec![1.0, -1.0, 2.0]);
    assert!(!c.world.state().casts.is_empty());
    assert!(observed
        .lock()
        .unwrap()
        .iter()
        .any(|request| matches!(request.query,
        n::Query::Spell { phase: n::CastPhase::Prepare, player_orientation, .. }
            if player_orientation == n::Orientation::new(1.0).unwrap())));
}

#[test]
fn invalid_packets_and_dead_or_rooted_movement_never_turn_the_player() {
    let h = Harness::new();
    let mut c = h.start(stationary());
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(matches!(
            c.receive(pose_command([0.0; 3], value)),
            Err(Error::InvalidMovement)
        ));
    }
    assert!(c.actions.is_empty());
    let before = c.world.state_digest();
    c.orders.pending = Some(RootOrder {
        guid: c.player_guid(),
        counter: 0,
    });
    let out = c.receive(pose_command([1.0, 0.0, 0.0], 2.0)).unwrap();
    assert_eq!(c.world.state_digest(), before);
    assert_eq!(c.facing(), 0.0);
    assert!(c.actions.is_empty());
    assert!(projected_pose(&out.events, &c, [0.0; 3], 0.0));

    let h = Harness::new();
    let mut c = h.start(death_profile());
    c.receive(cast(&c)).unwrap();
    while c.world.state().health > 0 && c.tick() < 200 {
        assert!(c.pump(c.tick_duration()).pause.is_none());
    }
    assert_eq!(c.world.state().health, 0);
    let facing = c.facing();
    let pos = c.position_xyz();
    let before = c.world.state_digest();
    let out = c.receive(pose_command([20.0, 0.0, 0.0], 2.0)).unwrap();
    assert_eq!(c.facing(), facing);
    assert_eq!(c.position_xyz(), pos);
    assert_eq!(c.world.state_digest(), before);
    assert!(projected_pose(&out.events, &c, pos, facing));
}

#[test]
fn late_initial_delivery_cannot_publish_a_failed_save_pose() {
    let h = Harness::new();
    let mut c = h.start(stationary());
    c.receive(pose_command([1.0, 0.0, 0.0], 2.0)).unwrap();
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored save fault",
    )
    .unwrap();
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_some());
    assert!(out.events.is_empty());
    c.initial_events().unwrap();
    assert_eq!(c.position_xyz(), [0.0; 3]);
    assert_eq!(c.facing(), 0.0);
    assert!(c.outbox.is_some());
}

#[test]
fn negative_whole_turn_projection_and_reopen_preserve_tau_bits() {
    let h = Harness::new();
    let mut c = h.start(stationary());
    c.initial_events().unwrap();
    c.receive(pose_command([0.0; 3], -std::f32::consts::TAU))
        .unwrap();
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none());
    assert_eq!(c.world.state().orientation.bits(), 0x40c9_0fdb);
    assert_eq!(c.facing().to_bits(), 0x40c9_0fdb);
    assert!(projected_pose(
        &out.events,
        &c,
        [0.0; 3],
        std::f32::consts::TAU
    ));
    drop(c);
    let mut c = h.reopen(stationary());
    c.initial_events().unwrap();
    assert_eq!(c.facing().to_bits(), 0x40c9_0fdb);
}
