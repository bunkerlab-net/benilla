// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact controller/client/store route with authored source-shaped inputs only.
use super::*;
use n::synthetic_motion_fixture as source;

#[test]
fn scheduled_cast_notify_save_failure_retry_and_reopen_publish_once() {
    let h = Harness::new();
    let data = source::profile();
    let authority = source::SyntheticMotionAuthority {
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
    c.receive(move_to([0.0, 0.0, 0.0])).unwrap();
    c.receive(cast(&c)).unwrap();
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored scheduler save fault",
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
    let digest = c.world.state_digest();
    let queue = c.world.state().player_events.queue_snapshot();
    assert_eq!(queue.clock_ms, 100);
    assert_eq!(queue.queued.len(), 1);
    assert_eq!(c.world.state().casts.len(), 1);
    let mut replay = genesis;
    replay.replay(c.world.records()).unwrap();
    assert_eq!(replay.state_digest(), digest);
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 0);
    assert_eq!(
        out.events
            .iter()
            .filter(|e| matches!(e, SessionEvent::SpellStart { .. }))
            .count(),
        1
    );
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(c.world.state().player_events.queue_snapshot(), queue);
    let flight = pump_to(&mut c, 4);
    assert_eq!(
        flight
            .iter()
            .filter(|e| matches!(e, SessionEvent::SpellGo { .. }))
            .count(),
        1
    );
    assert_eq!(c.world.state().casts[&1].execution.last_update_ms(), 300);
    let digest = c.world.state_digest();
    let queue = c.world.state().player_events.queue_snapshot();
    c.shutdown().unwrap();
    drop(c);
    let mut reopened = h
        .enter(
            Entry::Open { world: [8; 16] },
            data,
            source::SyntheticMotionAuthority {
                visits: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(reopened.world.state_digest(), digest);
    assert_eq!(reopened.world.state().player_events.queue_snapshot(), queue);
    let events = reopened.initial_events().unwrap();
    assert!(!events.iter().any(|e| matches!(
        e,
        SessionEvent::SpellStart { .. } | SessionEvent::SpellGo { .. }
    )));
    let future = pump_to(&mut reopened, 50);
    assert!(!future.iter().any(|e| matches!(
        e,
        SessionEvent::SpellStart { .. } | SessionEvent::SpellGo { .. }
    )));
    assert!(reopened
        .world
        .state()
        .player_events
        .queue_snapshot()
        .queued
        .is_empty());
    reopened.shutdown().unwrap();
}
