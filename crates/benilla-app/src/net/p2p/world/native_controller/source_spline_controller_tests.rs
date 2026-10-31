// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual controller publication of source player StopMoving; no native admission.
use super::*;
use benilla_world_runtime::native::synthetic_source_spline_fixture as splines;

fn enter_splines(h: &Harness, open: bool) -> NativeController {
    NativeController::enter_component_test(
        &h.root,
        &h.profile,
        &h.character,
        if open {
            Entry::Open { world: [8; 16] }
        } else {
            Entry::Create {
                world: [8; 16],
                seed: 42,
            }
        },
        splines::profile(),
        &MeleeCatalog,
        MeleeAuthority::default(),
    )
    .unwrap()
}
fn arbitrary_ack() -> ClientCommand {
    ClientCommand::MoveSplineDone {
        flags: u32::MAX,
        pos: [f32::NAN, f32::INFINITY, f32::NEG_INFINITY],
        orientation: f32::NAN,
        spline_id: u32::MAX,
    }
}
fn assert_noop_ack(c: &mut NativeController) {
    let checkpoint = c.world.checkpoint().unwrap();
    let actions = c.actions.len();
    let received = c.receive(arbitrary_ack()).unwrap();
    assert!(received.events.is_empty());
    assert!(!received.logout);
    assert_eq!(c.actions.len(), actions);
    assert_eq!(c.world.checkpoint().unwrap(), checkpoint);
}
#[test]
fn actual_player_forced_stop_is_withheld_until_checkpoint_retry_and_ack_is_source_noop() {
    let h = Harness::new();
    let mut c = enter_splines(&h, false);
    assert!(c.world.state().source_splines.is_some());
    assert_noop_ack(&mut c);
    c.receive(ClientCommand::AttackSwing { guid: guid(&c, 2) })
        .unwrap();
    let mut stopped = None;
    for _ in 0..300 {
        fs::write(
            h.path().join(".world-checkpoint.pending"),
            b"authored recoverable fault",
        )
        .unwrap();
        let withheld = c.pump(c.tick_duration());
        assert_eq!(withheld.ticks, 1);
        assert!(withheld.events.is_empty());
        assert!(matches!(
            withheld.pause.unwrap().kind,
            PauseKind::Checkpoint {
                requires_reopen: false
            }
        ));
        let died = !c.world.state().player_alive();
        let receipt = c
            .world
            .state()
            .source_splines
            .as_ref()
            .unwrap()
            .player_stop()
            .cloned();
        fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
        c.retry_checkpoint().unwrap();
        let published = c.pump(Duration::ZERO);
        assert!(published.pause.is_none());
        if died {
            let receipt = receipt.expect("actual first-life death must capture StopMoving");
            assert_eq!(receipt.position, c.world.state().position);
            let stop_events: Vec<_> = published.events.iter().filter(|event| matches!(event,
                SessionEvent::MonsterMove { guid, stop: true, .. } if *guid == c.player_guid()
            )).collect();
            assert_eq!(stop_events.len(), 1);
            assert!(matches!(stop_events[0], SessionEvent::MonsterMove {
                spline_id, transport: None, path, duration_ms: 0, ..
            } if *spline_id == receipt.allocation.wire_id && path.is_empty()));
            assert!(c.pump(Duration::ZERO).events.is_empty());
            stopped = Some(receipt);
            break;
        }
        assert!(!published.events.iter().any(|event| matches!(event,
            SessionEvent::MonsterMove { guid, stop: true, .. } if *guid == c.player_guid()
        )));
    }
    let stopped = stopped.expect("actual NPC combat must kill fragile source player");
    assert!(!c.world.state().player_alive());
    assert_noop_ack(&mut c);
    let checkpoint = c.world.checkpoint().unwrap();
    drop(c);
    let mut reopened = enter_splines(&h, true);
    assert_eq!(reopened.world.checkpoint().unwrap(), checkpoint);
    assert_eq!(
        reopened
            .world
            .state()
            .source_splines
            .as_ref()
            .unwrap()
            .player_stop(),
        Some(&stopped)
    );
    assert_noop_ack(&mut reopened);
    let initial = reopened.initial_events().unwrap();
    assert!(
        !initial.iter().any(|event| matches!(event,
            SessionEvent::MonsterMove { guid, stop: true, .. } if *guid == reopened.player_guid()
        )),
        "reopen must not replay a consumed one-shot player stop"
    );
}
#[test]
fn spline_done_remains_unsupported_without_composed_source_owner() {
    let h = Harness::new();
    let mut c = h.start(fixture::profile());
    assert!(c.world.state().source_splines.is_none());
    let checkpoint = c.world.checkpoint().unwrap();
    let actions = c.actions.len();
    assert!(matches!(
        c.receive(arbitrary_ack()),
        Err(Error::UnsupportedCommand)
    ));
    assert_eq!(c.actions.len(), actions);
    assert_eq!(c.world.checkpoint().unwrap(), checkpoint);
}
