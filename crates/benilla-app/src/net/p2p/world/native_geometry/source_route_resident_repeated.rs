// SPDX-License-Identifier: GPL-3.0-or-later
//! Component-only post-revision-two dispatch witness. The third Route is refused.
use crate::net::p2p::world::{resident_fixture::Driver, PauseKind};
use benilla_sim::creature_ai::Point;
use benilla_world_runtime::native::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(super) const REFUSAL: &str = "resident second retained dispatch inspected without calculation";

pub(super) fn after_warm_ack(
    driver: &mut Driver,
    callbacks: &AtomicUsize,
    routes: &AtomicUsize,
    warm_tick: u64,
    warm_events: usize,
) -> (u64, usize, usize) {
    assert_eq!(driver.tick(), warm_tick);
    assert_eq!(routes.load(Ordering::SeqCst), 2);
    assert!(driver.pause().is_none());
    assert!(!driver.route_pending());
    assert!(driver.pending_inputs().is_none());
    assert_eq!(driver.queued_actions(), 0);
    let start = driver.state().position;
    let mut events = warm_events;
    // Ordinary target input advances 0.125 units per unchanged 25 ms tick.
    // Never edit NPC state, timers, speed, target, terrain or native cache.
    for step in 1..=64 {
        let destination = Point::new(
            start.x.as_f32() - step as f32 * 0.125,
            start.y.as_f32(),
            start.z.as_f32(),
        )
        .unwrap();
        driver.queue_movement(destination, 0.5).unwrap();
        let before = driver.checkpoint();
        let records = driver.records().to_vec();
        let generation = driver.store_generation();
        let tick = driver.tick();
        let out = driver.pump_one();
        if routes.load(Ordering::SeqCst) == 3 {
            assert_eq!(
                out.pause.as_ref().map(|pause| &pause.kind),
                Some(&PauseKind::Authority)
            );
            assert!(out.pause.as_ref().unwrap().message.contains(REFUSAL));
            assert_eq!(driver.pause(), out.pause.as_ref());
            assert_eq!(out.ticks, 0);
            assert!(out.events.is_empty());
            assert!(out.saved.is_none());
            assert_eq!(driver.checkpoint(), before);
            assert_eq!(driver.records(), records);
            assert_eq!(driver.store_generation(), generation);
            assert_eq!(driver.tick(), tick);
            assert!(!driver.route_pending());
            assert_eq!(driver.queued_actions(), 0);
            let inputs = driver
                .pending_inputs()
                .expect("refused authenticated movement remains pending")
                .to_vec();
            assert_eq!(inputs.len(), 1);
            assert_eq!(inputs[0].input.world, driver.state().world);
            assert_eq!(inputs[0].input.tick, tick + 1);
            assert_eq!(inputs[0].input.player, driver.state().player);
            assert_eq!(inputs[0].input.sequence, driver.state().next_sequence);
            assert_eq!(inputs[0].authenticated_owner, driver.state().player.owner);
            assert!(matches!(
                &inputs[0].input.command,
                Command::MovementPose {
                    destination: actual,
                    orientation,
                    movement_flags,
                } if *actual == destination
                    && orientation.as_f32() == 0.5
                    && movement_flags.bits() == 0
            ));
            let count = callbacks.load(Ordering::SeqCst);
            let blocked = driver.pump_one();
            assert_eq!(blocked.pause, out.pause);
            assert_eq!(driver.pause(), blocked.pause.as_ref());
            assert_eq!(blocked.ticks, 0);
            assert!(blocked.events.is_empty());
            assert!(blocked.saved.is_none());
            assert_eq!(callbacks.load(Ordering::SeqCst), count);
            assert_eq!(routes.load(Ordering::SeqCst), 3);
            assert_eq!(driver.checkpoint(), before);
            assert_eq!(driver.records(), records);
            assert_eq!(driver.store_generation(), generation);
            assert_eq!(driver.tick(), tick);
            assert!(!driver.route_pending());
            assert_eq!(driver.queued_actions(), 0);
            assert_eq!(driver.pending_inputs().unwrap(), inputs);
            eprintln!(
                "resident second retained dispatch rollback: warm-tick={warm_tick} refused-tick={} input-step={step} committed-routes=2",
                tick + 1
            );
            return (tick, events, count);
        }
        assert_eq!(
            routes.load(Ordering::SeqCst),
            2,
            "only the first actual third Route may end this witness"
        );
        assert!(
            out.pause.is_none(),
            "unexpected earlier callback before second retained dispatch: {:?}",
            out.pause
        );
        assert!(driver.pause().is_none());
        assert_eq!(out.ticks, 1);
        assert_eq!(driver.tick(), tick + 1);
        let saved = out
            .saved
            .as_ref()
            .expect("intervening committed movement tick must be saved normally");
        assert_eq!(saved.tick, tick + 1);
        assert_eq!(Some(saved.generation), generation.checked_add(1));
        assert_eq!(driver.store_generation(), saved.generation);
        assert!(
            driver.records().is_empty(),
            "successful store receipt retires the live journal"
        );
        assert!(driver.pending_inputs().is_none());
        assert!(!driver.route_pending());
        assert_eq!(driver.queued_actions(), 0);
        events += out.events.len();
    }
    panic!("ordinary bounded player movement produced no second retained Chase dispatch");
}

#[test]
fn resident_controller_second_retained_dispatch_refuses_with_revision_two_owner_intact() {
    super::source_route_resident_controller::second_retained_dispatch_probe();
}
