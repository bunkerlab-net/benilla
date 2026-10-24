// SPDX-License-Identifier: GPL-3.0-or-later
//! The strict straight policy still refuses the actual raised fifth callback.
//! A unique controller error carries that refusal, never fabricated Route Facts.
use super::{
    core::Failure,
    products::source_height::inventory::InventoryCompatibility,
    real_corridor_fixture::installed_source::InstalledFixture,
    source_route_filter::resident::{ResidentFilterError, ResidentFilterProvider},
    source_route_resident_local::ledger::ResidentLedger,
};
use crate::net::p2p::world::{resident_fixture::Driver, PauseKind, Pump};
use benilla_sim::creature_ai::Point;
use benilla_world::terrain_stream::{CoverageError, NativeTerrainCoverage};
use benilla_world_runtime::native::{
    AuthorityRequest, Checkpoint, Command, MotionQuery, NativeError, Query, TickRecord,
};
use bevy::ecs::system::SystemState;
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

const STRICT_POLICY_REFUSAL: &str = "actual fifth resident Route refused by strict straight policy";

pub(super) fn refuse_actual_fifth(
    ledger: &mut ResidentLedger,
    geometry: &mut InstalledFixture,
    compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
    provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
    request: &AuthorityRequest,
) -> NativeError {
    let Query::SourceMotion(MotionQuery::Route(route)) = &request.query else {
        panic!("strict-straight boundary must be an actual Route callback")
    };
    assert_eq!(request.binding.tick, 128);
    assert_eq!(route.request.binding.query_id, 142);
    assert_eq!(route.request.binding.now_ms, 3200);
    let cache = route.request.prior_cache.as_deref().unwrap();
    assert_eq!(cache.revision, 4);
    assert_eq!(cache.active_len, 2);
    assert_eq!(cache.poly_refs.len(), 76);
    assert_eq!(ledger.counts(), (1, 1));
    let before = ledger.diagnostic();
    let constructors = provider.retained_constructor_count();
    let deadline = Instant::now() + Duration::from_secs(30);
    geometry.advance_physics();
    let diagnostic = ledger
        .probe_straight_geometry_for_test(provider, request, 501, deadline, || false)
        .unwrap();
    assert_eq!(diagnostic.points.len(), 4);
    assert_eq!(diagnostic.points.first(), Some(&route.request.origin));
    assert_eq!(diagnostic.points.last(), Some(&route.request.goal));
    assert_eq!(diagnostic.points[0].z.bits(), 3259498496);
    assert_eq!(diagnostic.points[1].z.bits(), 3259428591);
    assert_eq!(diagnostic.points[2].z.bits(), 3259428591);
    assert_eq!(diagnostic.points[3].z.bits(), 3259498496);
    let mut state = SystemState::<NativeTerrainCoverage>::new(geometry.app.world_mut());
    let terrain = state.get(geometry.app.world());
    let from = point(diagnostic.points[0]);
    let to = point(diagnostic.points[1]);
    assert!(matches!(
        terrain.prove(
            compatibility.source_binding().0.map,
            from,
            to,
            diagnostic.body.radius.as_f32()
        ),
        Err(CoverageError::UnsupportedTerrain)
    ));
    for pair in diagnostic.points.windows(2) {
        let mut from = point(pair[0]);
        let mut to = point(pair[1]);
        from[2] = diagnostic.points[0].z.as_f32();
        to[2] = from[2];
        terrain
            .prove(
                compatibility.source_binding().0.map,
                from,
                to,
                diagnostic.body.radius.as_f32(),
            )
            .unwrap();
    }
    // The raw error is asserted separately: the unchanged strict certifier maps
    // it to MissingGeometry, not a newly invented outer error variant.
    assert!(matches!(
        ledger.certify_straight(
            geometry.app.world_mut(),
            compatibility,
            provider,
            request,
            501,
            deadline,
            || false,
        ),
        Err(ResidentFilterError::Context(Failure::MissingGeometry))
    ));
    assert!(Instant::now() < deadline, "the refusal is not expiry");
    assert_eq!(ledger.diagnostic(), before);
    assert_eq!(provider.retained_constructor_count(), constructors);
    ledger.check_committed_warm_for_test().unwrap();
    let owned = ledger.flat_ownership_for_test().unwrap();
    assert_eq!((owned.terminal_count, owned.staged_count), (2, 0));
    assert!(owned.storage_bytes <= owned.upper_bound_bytes);
    eprintln!("resident strict-straight actual fifth refusal: tick=128 query=142 now-ms=3200 prior-revision=4 storage=76 raw-coverage=UnsupportedTerrain context=MissingGeometry unchanged-ledger=true no-facts=true");
    NativeError::AuthorityUnavailable(STRICT_POLICY_REFUSAL)
}

fn point(point: Point) -> [f32; 3] {
    [point.x.as_f32(), point.y.as_f32(), point.z.as_f32()]
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
        assert_eq!(self.tick, 127);
        assert_eq!(self.now_ms, 3175);
        assert_eq!(routes.load(Ordering::SeqCst), 5);
        assert_eq!(
            out.pause.as_ref().map(|pause| &pause.kind),
            Some(&PauseKind::Authority)
        );
        assert!(out
            .pause
            .as_ref()
            .unwrap()
            .message
            .contains(STRICT_POLICY_REFUSAL));
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
            assert_eq!(routes.load(Ordering::SeqCst), 5);
            assert_eq!(driver.pending_inputs().unwrap(), inputs);
            self.assert_unchanged(driver);
        }
        eprintln!("resident strict-straight rollback: tick=127 now-ms=3175 committed-epochs=4 failed-route=5 pending-input=exact journal=unchanged store-generation=unchanged blocked-callback-replay=0");
    }
}

#[test]
fn strict_straight_policy_refuses_actual_fifth_raised_path_without_tick_or_store_commit() {
    super::source_route_resident_controller::run_finite_loop(
        super::source_route_resident_finite_loop::Mode::StrictStraightBoundary,
    );
}
