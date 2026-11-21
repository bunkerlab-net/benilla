// SPDX-License-Identifier: GPL-3.0-or-later
//! The old finite same-polygon policy still refuses the actual fourth callback.
//! A unique controller error carries that refusal, never fabricated Route Facts.
use super::{
    products::source_height::inventory::InventoryCompatibility,
    real_corridor_fixture::installed_source::InstalledFixture,
    source_route_filter::resident::{ResidentFilterError, ResidentFilterProvider},
    source_route_resident_local::ledger::ResidentLedger,
};
use crate::net::p2p::world::{resident_fixture::Driver, PauseKind, Pump};
use benilla_sim::creature_ai::Point;
use benilla_world_runtime::native::{
    AuthorityRequest, Checkpoint, Command, MotionQuery, NativeError, Query, TickRecord,
};
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

const OLD_POLICY_REFUSAL: &str = "actual fourth resident Route refused by old same-polygon policy";

pub(super) fn refuse_actual_fourth(
    ledger: &mut ResidentLedger,
    geometry: &mut InstalledFixture,
    compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
    provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
    request: &AuthorityRequest,
) -> NativeError {
    let Query::SourceMotion(MotionQuery::Route(route)) = &request.query else {
        panic!("old-policy boundary must be an actual Route callback")
    };
    assert_eq!(request.binding.tick, 98);
    assert_eq!(route.request.binding.query_id, 107);
    assert_eq!(route.request.binding.now_ms, 2450);
    let cache = route.request.prior_cache.as_deref().unwrap();
    assert_eq!(cache.revision, 3);
    assert_eq!(cache.active_len, 1);
    assert_eq!(cache.poly_refs.len(), 75);
    assert_eq!(ledger.counts(), (1, 1));
    let before = ledger.diagnostic();
    let constructors = provider.retained_constructor_count();
    let deadline = Instant::now() + Duration::from_secs(30);
    geometry.advance_physics();
    assert!(matches!(
        ledger.certify_repeated(
            geometry.app.world_mut(),
            compatibility,
            provider,
            request,
            401,
            deadline,
            || false,
        ),
        Err(ResidentFilterError::Terminal(
            benilla_native_nav::SourceSamePolygonError::NonSamePolygonUnsupported
        ))
    ));
    assert!(Instant::now() < deadline, "the refusal is not expiry");
    assert_eq!(ledger.diagnostic(), before);
    assert_eq!(provider.retained_constructor_count(), constructors);
    ledger.check_committed_warm_for_test().unwrap();
    let owned = ledger.flat_ownership_for_test().unwrap();
    assert_eq!((owned.terminal_count, owned.staged_count), (2, 0));
    assert!(owned.storage_bytes <= owned.upper_bound_bytes);
    eprintln!(
        "resident old-policy actual fourth refusal: tick=98 query=107 now-ms=2450 prior-revision=3 storage=75 exact-terminal=NonSamePolygonUnsupported unchanged-ledger=true no-facts=true"
    );
    NativeError::AuthorityUnavailable(OLD_POLICY_REFUSAL)
}

pub(super) struct RefusedTick {
    checkpoint: Checkpoint,
    records: Vec<TickRecord>,
    generation: u64,
    tick: u64,
    now_ms: u64,
    sequence: u64,
    destination: Point,
}
impl RefusedTick {
    pub(super) fn capture(driver: &Driver, destination: Point) -> Self {
        Self {
            checkpoint: driver.checkpoint(),
            records: driver.records().to_vec(),
            generation: driver.store_generation(),
            tick: driver.tick(),
            now_ms: driver.state().now_ms,
            sequence: driver.state().next_sequence,
            destination,
        }
    }
    fn assert_unchanged(&self, driver: &Driver) {
        assert_eq!(driver.checkpoint(), self.checkpoint);
        assert_eq!(driver.records(), self.records);
        assert_eq!(driver.store_generation(), self.generation);
        assert_eq!(driver.tick(), self.tick);
        assert_eq!(driver.state().now_ms, self.now_ms);
        assert_eq!(driver.state().next_sequence, self.sequence);
        assert_eq!(driver.queued_actions(), 0);
        assert!(!driver.route_pending());
    }
    pub(super) fn assert_refused(
        self,
        driver: &mut Driver,
        callbacks: &AtomicUsize,
        routes: &AtomicUsize,
        out: &Pump,
    ) {
        assert_eq!(self.tick, 97);
        assert_eq!(self.now_ms, 2425);
        assert_eq!(routes.load(Ordering::SeqCst), 4);
        assert_eq!(
            out.pause.as_ref().map(|pause| &pause.kind),
            Some(&PauseKind::Authority)
        );
        assert!(out
            .pause
            .as_ref()
            .unwrap()
            .message
            .contains(OLD_POLICY_REFUSAL));
        assert_eq!(driver.pause(), out.pause.as_ref());
        assert_eq!(out.ticks, 0);
        assert!(out.events.is_empty());
        assert!(out.saved.is_none());
        self.assert_unchanged(driver);
        let inputs = driver.pending_inputs().unwrap().to_vec();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].input.world, driver.state().world);
        assert_eq!(inputs[0].input.tick, self.tick + 1);
        assert_eq!(inputs[0].input.player, driver.state().player);
        assert_eq!(inputs[0].input.sequence, self.sequence);
        assert_eq!(inputs[0].authenticated_owner, driver.state().player.owner);
        assert!(matches!(
            &inputs[0].input.command,
            Command::MovementPose { destination, orientation, movement_flags }
                if *destination == self.destination && orientation.as_f32() == 0.5 && movement_flags.bits() == 0
        ));
        let count = callbacks.load(Ordering::SeqCst);
        for elapsed in [false, true] {
            let blocked = if elapsed {
                driver.pump_one()
            } else {
                driver.pump_zero()
            };
            assert_eq!(blocked.pause, out.pause);
            assert_eq!(driver.pause(), blocked.pause.as_ref());
            assert_eq!(blocked.ticks, 0);
            assert!(blocked.events.is_empty());
            assert!(blocked.saved.is_none());
            assert_eq!(callbacks.load(Ordering::SeqCst), count);
            assert_eq!(routes.load(Ordering::SeqCst), 4);
            assert_eq!(driver.pending_inputs().unwrap(), inputs);
            self.assert_unchanged(driver);
        }
        eprintln!(
            "resident old-policy rollback: tick=97 now-ms=2425 committed-epochs=3 failed-route=4 pending-input=exact journal=unchanged store-generation=unchanged blocked-callback-replay=0"
        );
    }
}

#[test]
fn finite_old_policy_refuses_actual_fourth_non_same_polygon_without_tick_or_store_commit() {
    super::source_route_resident_controller::run_finite_loop(
        super::source_route_resident_finite_loop::Mode::OldPolicyBoundary,
    );
}
