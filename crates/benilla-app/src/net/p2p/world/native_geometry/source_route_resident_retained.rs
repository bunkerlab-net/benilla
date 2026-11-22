// SPDX-License-Identifier: GPL-3.0-or-later
//! Component-only cold ACK plus exactly one genuine retained Chase calculation.
//! The ordinary target-input schedule never edits NPC state or admits a third route.
use super::route_commit::RouteCommitNotice;
use crate::net::p2p::world::{resident_fixture::Driver, PauseKind, SavedBoundary};
use benilla_sim::{creature_ai::Point, creature_motion as motion};
use benilla_world_runtime::native::{
    AuthorityRequest, Command, Facts, MotionFacts, MotionQuery, Profile, Query,
};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Accept,
    Refusals,
    RollbackRetry,
    DropAfterApply,
    SourceChangeBeforeAck,
    SecondRetainedDispatch,
}

pub(super) fn assert_request(
    profile: &Profile,
    cold: &RouteCommitNotice,
    request: &AuthorityRequest,
) {
    assert_eq!(cold.routes().len(), 1);
    assert_eq!(cold.boundary().heads().len(), 1);
    let old = &cold.routes()[0].request;
    let Query::SourceMotion(MotionQuery::Route(route)) = &request.query else {
        panic!("retained calculation requires the actual Route callback")
    };
    let saved = old.require_pathfinder_dispatch(profile).unwrap();
    let live = request.require_pathfinder_dispatch(profile).unwrap();
    assert!(saved.first_construction());
    assert!(!live.first_construction());
    assert!(!saved.same_live_dispatch(live));
    assert_eq!(live.construction_binding(), saved.construction_binding());
    assert_eq!(
        live.constructor_kind(),
        motion::PathfinderConstructorKind::ChaseLazy
    );
    assert_eq!(route.request.kind, motion::RouteKind::ChaseReplan);
    let cache = route.request.prior_cache.as_deref().unwrap();
    assert_eq!(Some(cache), cold.boundary().heads()[0].cache());
    assert_eq!(cache.revision, 1);
    assert_eq!(cache.poly_refs.len(), 75);
    assert_eq!(cache.active_len, 1);
    assert_eq!(cache.point_limit, motion::PATHFINDER_CACHE_POINT_LIMIT);
    assert_eq!(cache.include_flags, 13);
    assert_eq!(cache.exclude_flags, 0);
    assert_eq!(cache.path_type, 1);
    assert!(request.binding.tick > old.binding.tick);
}

pub(super) fn assert_facts(request: &AuthorityRequest, facts: &Facts) {
    let Query::SourceMotion(MotionQuery::Route(route)) = &request.query else {
        unreachable!()
    };
    let Facts::SourceMotion(MotionFacts::Spatial(spatial)) = facts else {
        panic!("retained route must emit genuine paired spatial facts")
    };
    assert_eq!(spatial.request, motion::Query::Route(route.request.clone()));
    let motion::Fact::RouteCalculation(calculation) = &spatial.fact else {
        panic!("retained route must carry its complete calculation")
    };
    calculation.validate_for(&route.request).unwrap();
    assert_eq!(calculation.prior_cache, route.request.prior_cache);
    assert_eq!(calculation.source, motion::RouteSource::OwnerPathFinder);
    assert!(matches!(
        calculation.outcome,
        motion::RouteOutcome::Complete(_)
    ));
    assert!(calculation.wander_no_path.is_none());
    assert_eq!(calculation.after_cache.revision, 2);
    assert_eq!(calculation.after_cache.include_flags, 13);
    assert_eq!(calculation.after_cache.exclude_flags, 0);
    assert_eq!(calculation.after_cache.path_type, 1);
    assert_eq!(calculation.after_cache.active_len, 1);
    eprintln!(
        "resident actual retained calculation: tick={} query={} prior-revision=1 after-revision=2 storage={} filter=13 origin={:?} goal={:?}",
        request.binding.tick,
        route.request.binding.query_id,
        calculation.after_cache.poly_refs.len(),
        route.request.origin,
        route.request.goal
    );
}

/// These are cancellation-at-stage negatives, not fabricated wet/ignore/missing
/// tile responses. Every source-only result is dropped before the real certifier.
pub(super) fn assert_source_order(
    ledger: &super::source_route_resident_local::ledger::ResidentLedger,
    provider: &mut super::source_route_filter::resident::ResidentFilterProvider<'_, '_, '_, '_>,
    request: &AuthorityRequest,
) {
    use super::{
        products::source_height::CompositionError,
        source_route_filter::resident::{
            retained::ResidentRetainedPhase as Phase, ResidentFilterError,
        },
    };
    use std::{
        cell::Cell,
        time::{Duration, Instant},
    };
    const ORDER: [Phase; 12] = [
        Phase::Model,
        Phase::ChaseRay,
        Phase::ChaseRayComplete,
        Phase::WaterGate,
        Phase::OwnerWater,
        Phase::TargetWater,
        Phase::Selected,
        Phase::OwnerIgnore,
        Phase::Tiles,
        Phase::FilterAllLiquids,
        Phase::FilterWaterOrOcean,
        Phase::Terminal,
    ];
    let before = ledger.diagnostic();
    for stop in [
        Phase::ChaseRay,
        Phase::OwnerWater,
        Phase::OwnerIgnore,
        Phase::FilterAllLiquids,
    ] {
        let cancelled = Cell::new(false);
        let mut phases = vec![];
        let deadline = Instant::now() + Duration::from_secs(30);
        assert!(matches!(
            ledger.probe_retained_source_for_test(
                provider,
                request,
                201,
                deadline,
                || cancelled.get(),
                |phase| {
                    phases.push(phase);
                    if phase == stop {
                        cancelled.set(true);
                    }
                },
            ),
            Err(ResidentFilterError::Source(CompositionError::Cancelled))
        ));
        assert_eq!(
            phases,
            ORDER[..=ORDER.iter().position(|phase| *phase == stop).unwrap()]
        );
        assert!(
            Instant::now() < deadline,
            "cancellation, not expiry, refused this attempt"
        );
        assert_eq!(
            ledger.diagnostic(),
            before,
            "no candidate or Facts staged by source probe"
        );
    }
    let mut phases = vec![];
    ledger
        .probe_retained_source_for_test(
            provider,
            request,
            201,
            Instant::now() + Duration::from_secs(30),
            || false,
            |phase| phases.push(phase),
        )
        .unwrap();
    assert_eq!(phases, ORDER);
    assert_eq!(ledger.diagnostic(), before);
    eprintln!("resident retained source order: {phases:?}; cancellation prefixes=4; staged=0");
}

/// Immutable evidence from the actual persisted tick, after the store receipt
/// intentionally retired the runtime's in-memory journal and before ACK retry.
pub(super) struct CommittedTick {
    before: benilla_world_runtime::native::CheckpointIdentity,
    after: benilla_world_runtime::native::CheckpointIdentity,
    saved: SavedBoundary,
}
pub(super) fn assert_commit_proof(proof: &CommittedTick, notice: &RouteCommitNotice) {
    let boundary = notice.boundary();
    assert_eq!(proof.before.tick.checked_add(1), Some(proof.after.tick));
    assert_eq!(proof.saved.tick, proof.after.tick);
    assert_eq!(notice.previous_record(), proof.before.history_tip);
    assert_eq!(notice.before(), proof.before.state_digest);
    assert_eq!(boundary.record(), proof.after.history_tip);
    assert_eq!(boundary.state(), proof.after.state_digest);
    assert_eq!(boundary.tick(), proof.after.tick);
    assert_eq!(boundary.world(), proof.after.world);
    assert_eq!(boundary.player(), proof.after.player);
    assert_eq!(boundary.profile(), proof.after.profile);
    assert_eq!(notice.routes().len(), 1);
    assert_eq!(boundary.heads().len(), 1);
    let Facts::SourceMotion(MotionFacts::Spatial(spatial)) = &notice.routes()[0].facts else {
        panic!("accepted notice carries the genuine warm calculation")
    };
    let motion::Fact::RouteCalculation(calculation) = &spatial.fact else {
        unreachable!()
    };
    assert_eq!(boundary.heads()[0].cache(), Some(&calculation.after_cache));
    assert_eq!(calculation.after_cache.revision, 2);
    eprintln!(
        "resident retained durable proof: tick={} generation={} accepted-routes=1 live-journal-retired=true",
        proof.saved.tick, proof.saved.generation
    );
}

pub(super) fn after_cold_ack(
    driver: &mut Driver,
    callbacks: &AtomicUsize,
    routes: &AtomicUsize,
    cold_tick: u64,
    cold_events: usize,
    mode: Mode,
) -> (u64, usize, usize, CommittedTick) {
    assert_eq!(driver.tick(), cold_tick);
    assert_eq!(routes.load(Ordering::SeqCst), 1);
    assert!(!driver.route_pending());
    assert!(driver.pending_inputs().is_none());
    let start = driver.state().position;
    let mut events = cold_events;
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
        let mut out = driver.pump_one();
        if routes.load(Ordering::SeqCst) == 1 {
            assert!(
                out.pause.is_none(),
                "unexpected earlier callback: {:?}",
                out.pause
            );
            assert_eq!(out.ticks, 1);
            assert_eq!(driver.tick(), tick + 1);
            assert!(driver.pending_inputs().is_none());
            assert!(!driver.route_pending());
            events += out.events.len();
            continue;
        }
        assert_eq!(
            routes.load(Ordering::SeqCst),
            2,
            "only one retained dispatch per tick"
        );
        if mode == Mode::RollbackRetry {
            assert_eq!(
                out.pause.as_ref().map(|p| &p.kind),
                Some(&PauseKind::Authority)
            );
            assert!(out
                .pause
                .as_ref()
                .unwrap()
                .message
                .contains("deliberate later"));
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
                .expect("rolled-back authenticated input")
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
            let blocked = driver.pump_zero();
            assert_eq!(blocked.ticks, 0);
            assert!(blocked.events.is_empty());
            assert_eq!(callbacks.load(Ordering::SeqCst), count);
            assert_eq!(driver.checkpoint(), before);
            assert_eq!(driver.records(), records);
            assert_eq!(driver.store_generation(), generation);
            assert_eq!(driver.pending_inputs().unwrap(), inputs);
            driver.retry_tick().unwrap();
            assert_eq!(driver.pending_inputs().unwrap(), inputs);
            out = driver.pump_zero();
            assert_eq!(
                routes.load(Ordering::SeqCst),
                3,
                "identical input gets one fresh live attempt"
            );
        }
        assert_eq!(out.ticks, 1);
        assert_eq!(driver.tick(), tick + 1);
        // This controller persists pose/environment ticks before delivering ACK.
        // The successful store receipt retires the in-memory journal, not history.
        let saved = out
            .saved
            .clone()
            .expect("warm tick saved before ACK delivery");
        assert_eq!(saved.tick, tick + 1);
        assert_eq!(Some(saved.generation), generation.checked_add(1));
        assert_eq!(driver.store_generation(), saved.generation);
        assert!(
            driver.records().is_empty(),
            "exact durable receipt retired the live journal"
        );
        let committed = CommittedTick {
            before: before.identity(),
            after: driver.checkpoint().identity(),
            saved,
        };
        assert_ne!(committed.after.history_tip, committed.before.history_tip);
        assert!(
            driver.pending_inputs().is_none(),
            "input retired before ACK delivery"
        );
        assert_eq!(driver.queued_actions(), 0);
        let checkpoint = driver.checkpoint();
        let committed_records = driver.records().to_vec();
        let committed_generation = driver.store_generation();
        let count = callbacks.load(Ordering::SeqCst);
        if matches!(mode, Mode::DropAfterApply | Mode::SourceChangeBeforeAck) {
            assert_eq!(
                out.pause.as_ref().map(|p| &p.kind),
                Some(&PauseKind::CommittedAwaitingRouteAck)
            );
            assert!(driver.route_pending());
            let blocked = driver.pump_one();
            assert_eq!(blocked.ticks, 0);
            assert!(blocked.events.is_empty());
            assert_eq!(callbacks.load(Ordering::SeqCst), count);
            assert_eq!(driver.checkpoint(), checkpoint);
            assert_eq!(driver.records(), committed_records);
            assert_eq!(driver.store_generation(), committed_generation);
            assert!(driver.pending_inputs().is_none());
            assert_eq!(driver.queued_actions(), 0);
            driver.retry_route_commit().unwrap();
        } else {
            assert!(out.pause.is_none(), "{:?}", out.pause);
        }
        assert!(!driver.route_pending());
        assert!(driver.pause().is_none());
        assert!(driver.pending_inputs().is_none());
        assert_eq!(driver.checkpoint(), checkpoint);
        assert_eq!(driver.records(), committed_records);
        assert_eq!(driver.store_generation(), committed_generation);
        assert_eq!(
            callbacks.load(Ordering::SeqCst),
            count,
            "ACK delivery never replays callbacks"
        );
        let flushed = driver.pump_zero();
        assert_eq!(flushed.ticks, 0);
        assert!(
            flushed.events.is_empty(),
            "committed output is published only once"
        );
        assert_eq!(driver.tick(), tick + 1);
        events += out.events.len();
        eprintln!(
            "resident retained ACK measurement: mode={mode:?} cold-tick={cold_tick} warm-tick={} input-step={step}",
            tick + 1
        );
        return (tick + 1, events, count, committed);
    }
    panic!("ordinary bounded player movement produced no retained Chase dispatch");
}

#[test]
fn resident_controller_one_retained_commits_exact_ack_selected_predecessor() {
    super::source_route_resident_controller::one_retained_probe(Mode::Accept);
}

#[test]
fn resident_controller_one_retained_later_failure_rolls_back_and_selects_fresh_retry() {
    super::source_route_resident_controller::one_retained_probe(Mode::RollbackRetry);
}

#[test]
fn resident_controller_one_retained_lost_ack_retries_without_replay() {
    super::source_route_resident_controller::one_retained_probe(Mode::DropAfterApply);
}
