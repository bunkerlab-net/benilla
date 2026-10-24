// SPDX-License-Identifier: GPL-3.0-or-later
//! Genuine post-ACK dispatch shape only. No retained Route is accepted here.
use crate::net::p2p::world::{resident_fixture::Driver, PauseKind};
use benilla_sim::creature_ai::Point;
use benilla_world_runtime::native::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(super) const REFUSAL: &str = "resident retained dispatch inspected without calculation";

pub(super) fn after_cold_ack(
    driver: &mut Driver,
    callbacks: &AtomicUsize,
    routes: &AtomicUsize,
    cold_tick: u64,
    cold_events: usize,
) -> (u64, usize, usize) {
    assert_eq!(driver.tick(), cold_tick);
    assert!(!driver.route_pending());
    assert!(driver.pending_inputs().is_none());
    let start = driver.state().position;
    // Authored ordinary target input: 0.125 units per unchanged 25 ms tick.
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
        if routes.load(Ordering::SeqCst) == 2 {
            assert_eq!(
                out.pause.as_ref().map(|p| &p.kind),
                Some(&PauseKind::Authority)
            );
            assert!(out.pause.as_ref().unwrap().message.contains(REFUSAL));
            assert_eq!(out.ticks, 0);
            assert!(out.events.is_empty());
            assert_eq!(driver.checkpoint(), before);
            assert_eq!(driver.records(), records);
            assert_eq!(driver.store_generation(), generation);
            assert_eq!(driver.tick(), tick);
            assert!(!driver.route_pending());
            assert_eq!(driver.queued_actions(), 0);
            let inputs = driver
                .pending_inputs()
                .expect("failed input remains pending")
                .to_vec();
            assert_eq!(inputs.len(), 1);
            assert_eq!(inputs[0].input.tick, tick + 1);
            assert_eq!(inputs[0].input.player, driver.state().player);
            assert_eq!(inputs[0].authenticated_owner, driver.state().player.owner);
            assert!(matches!(
                &inputs[0].input.command,
                Command::MovementPose { destination: actual, .. } if *actual == destination
            ));
            let count = callbacks.load(Ordering::SeqCst);
            let blocked = driver.pump_one();
            assert_eq!(blocked.ticks, 0);
            assert!(blocked.events.is_empty());
            assert_eq!(callbacks.load(Ordering::SeqCst), count);
            assert_eq!(driver.checkpoint(), before);
            assert_eq!(driver.records(), records);
            assert_eq!(driver.pending_inputs().unwrap(), inputs);
            eprintln!("resident retained dispatch rollback: cold-tick={cold_tick} refused-tick={} input-step={step} committed-routes=1", tick + 1);
            return (tick, cold_events, count);
        }
        assert_eq!(routes.load(Ordering::SeqCst), 1);
        assert!(
            out.pause.is_none(),
            "unexpected earlier callback: {:?}",
            out.pause
        );
        assert_eq!(out.ticks, 1);
        assert_eq!(driver.tick(), tick + 1);
        assert!(driver.pending_inputs().is_none());
        assert!(!driver.route_pending());
    }
    panic!("ordinary bounded player movement produced no retained Chase dispatch");
}

#[test]
fn resident_controller_retained_dispatch_refuses_without_disturbing_committed_owner() {
    super::source_route_resident_controller::retained_dispatch_probe();
}
