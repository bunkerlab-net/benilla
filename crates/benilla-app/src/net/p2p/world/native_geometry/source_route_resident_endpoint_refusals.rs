// SPDX-License-Identifier: GPL-3.0-or-later
//! Prior planar policy cannot infer support from absent original height calls.
use super::{
    core::Failure,
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
    cell::Cell,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
const ENDPOINT_SUPPORT_REFUSAL: &str =
    "actual sixth resident Route refused without original height attribution";

pub(super) fn refuse_actual_sixth(
    ledger: &mut ResidentLedger,
    geometry: &mut InstalledFixture,
    compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
    provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
    request: &AuthorityRequest,
) -> NativeError {
    let Query::SourceMotion(MotionQuery::Route(route)) = &request.query else {
        panic!("must be genuine sixth Route")
    };
    assert_eq!(request.binding.tick, 158);
    assert_eq!(route.request.binding.query_id, 177);
    assert_eq!(route.request.binding.now_ms, 3950);
    assert_eq!(route.request.prior_cache.as_deref().unwrap().revision, 5);
    assert!(provider.allows_planar_nonpenetration());
    assert!(!provider.allows_endpoint_support());
    assert!(matches!(
        provider.require_endpoint_support(),
        Err(ResidentFilterError::Context(Failure::UnsupportedQuery))
    ));
    let before = ledger.diagnostic();
    let constructors = provider.retained_constructor_count();
    let until = Instant::now() + Duration::from_secs(30);
    geometry.advance_physics();
    let diagnostic = ledger
        .probe_straight_geometry_for_test(provider, request, 601, until, || false)
        .unwrap();
    assert_eq!(diagnostic.points.len(), 2);
    assert_eq!(diagnostic.points.first(), Some(&route.request.origin));
    assert_eq!(diagnostic.points.last(), Some(&route.request.goal));
    let phases = Cell::new(0);
    let completed = Cell::new(0);
    assert!(matches!(
        ledger.certify_straight_segments_observed(
            geometry.app.world_mut(),
            compatibility,
            provider,
            request,
            601,
            until,
            || false,
            |_, _| phases.set(phases.get() + 1),
            || completed.set(completed.get() + 1),
        ),
        Err(ResidentFilterError::Context(Failure::UnsupportedNavigation))
    ));
    assert_eq!(phases.get(), 0);
    assert_eq!(completed.get(), 0);
    assert!(Instant::now() < until);
    assert_eq!(ledger.diagnostic(), before);
    assert_eq!(provider.retained_constructor_count(), constructors);
    ledger.check_committed_warm_for_test().unwrap();
    let owned = ledger.flat_ownership_for_test().unwrap();
    assert_eq!((owned.terminal_count, owned.staged_count), (2, 0));
    eprintln!(
        "resident prior-planar actual sixth refusal: tick=158 query=177 original-height-calls=0 no-supplemental-authority=true context=UnsupportedNavigation segments=0 no-facts=true"
    );
    NativeError::AuthorityUnavailable(ENDPOINT_SUPPORT_REFUSAL)
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
        assert_eq!(self.tick, 157);
        assert_eq!(self.now_ms, 3925);
        assert_eq!(routes.load(Ordering::SeqCst), 6);
        assert_eq!(
            out.pause.as_ref().map(|pause| &pause.kind),
            Some(&PauseKind::Authority)
        );
        assert!(out
            .pause
            .as_ref()
            .unwrap()
            .message
            .contains(ENDPOINT_SUPPORT_REFUSAL));
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
            assert_eq!(routes.load(Ordering::SeqCst), 6);
            assert_eq!(driver.pending_inputs().unwrap(), inputs);
            self.assert_unchanged(driver);
        }
        eprintln!(
            "resident prior-planar-support rollback: tick=157 now-ms=3925 committed-epochs=5 failed-route=6 pending-input=exact journal=unchanged store-generation=unchanged blocked-callback-replay=0"
        );
    }
}

#[test]
fn prior_planar_policy_refuses_actual_sixth_without_original_height_attribution() {
    super::source_route_resident_controller::run_finite_loop(
        super::source_route_resident_finite_loop::Mode::StrictEndpointSupportBoundary,
    );
}
