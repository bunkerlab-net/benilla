// SPDX-License-Identifier: GPL-3.0-or-later
//! Component-only fixed cold plus two retained Chase epochs. The third actual
//! calculation uses ordinary authenticated target movement, never NPC mutation.
use super::route_commit::RouteCommitNotice;
use crate::net::p2p::world::{resident_fixture::Driver, PauseKind, SavedBoundary};
use benilla_sim::{creature_ai::Point, creature_motion as motion};
use benilla_world_runtime::native::{
    AuthorityRequest, CheckpointIdentity, Command, Facts, MotionFacts, MotionQuery, Profile, Query,
};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Accept,
    Refusals,
    RollbackRetry,
    DropAfterApply,
}

/// The owner thread injects this once, only on a real callback after third Facts.
pub(super) const LATER_FAILURE: &str =
    "deliberate later callback after actual third resident route facts";

pub(super) fn assert_request(
    profile: &Profile,
    warm1: &RouteCommitNotice,
    request: &AuthorityRequest,
) {
    assert_eq!(profile.tick_ms, 25);
    assert_eq!(warm1.routes().len(), 1);
    assert_eq!(warm1.boundary().heads().len(), 1);
    let previous = &warm1.routes()[0];
    let Query::SourceMotion(MotionQuery::Route(old)) = &previous.request.query else {
        panic!("first retained ACK must contain its actual Route callback")
    };
    let Query::SourceMotion(MotionQuery::Route(route)) = &request.query else {
        panic!("third epoch requires the actual second retained Route callback")
    };
    let saved = previous
        .request
        .require_pathfinder_dispatch(profile)
        .unwrap();
    let live = request.require_pathfinder_dispatch(profile).unwrap();
    assert!(!saved.first_construction());
    assert!(!live.first_construction());
    assert!(!saved.same_live_dispatch(live));
    assert_eq!(live.construction_binding(), saved.construction_binding());
    assert_eq!(live.configuration_binding(), saved.configuration_binding());
    assert_eq!(live.profile_binding(), saved.profile_binding());
    assert_eq!(
        live.constructor_kind(),
        motion::PathfinderConstructorKind::ChaseLazy
    );
    assert_eq!(saved.constructor_kind(), live.constructor_kind());
    assert_eq!(live.dispatch_binding(), route.request.binding);
    assert_eq!(live.dispatch_tick(), request.binding.tick);
    assert_eq!(live.world(), request.binding.world);
    assert_eq!(live.npc(), route.owner.npc());

    assert_eq!(request.binding.world, previous.request.binding.world);
    assert_eq!(request.binding.profile, previous.request.binding.profile);
    assert_eq!(request.binding.geometry, previous.request.binding.geometry);
    assert_eq!(
        request.binding.authority_policy,
        previous.request.binding.authority_policy
    );
    assert_eq!(request.binding.npc, previous.request.binding.npc);
    assert_eq!(request.binding.npc, Some(route.owner.npc()));
    assert!(request.binding.tick > previous.request.binding.tick);
    assert!(route.request.binding.query_id > old.request.binding.query_id);
    assert!(route.request.binding.now_ms > old.request.binding.now_ms);
    assert_eq!(route.request.binding.identity, old.request.binding.identity);
    assert_eq!(route.owner, old.owner);
    assert_eq!(route.collision_width, old.collision_width);
    assert_eq!(route.collision_height, old.collision_height);
    assert_eq!(
        route.request.owner_collision_height,
        old.request.owner_collision_height
    );
    assert_eq!(
        route.target_player_height.as_ref().unwrap().witness(),
        old.target_player_height.as_ref().unwrap().witness()
    );
    assert_eq!(route.request.kind, motion::RouteKind::ChaseReplan);
    assert!(route.request.wander.is_none());
    // The moving origin and destination are actual callback operands, not the
    // previous tick's poses or a replacement target synthesized by this fixture.
    assert_eq!(route.owner_position, route.request.origin);
    assert_eq!(route.request.launch_origin, route.request.origin);
    assert_eq!(
        route.target_player_height.as_ref().unwrap().target(),
        route.request.target.as_ref().unwrap()
    );
    assert_eq!(
        route.request.target.as_ref().unwrap().id,
        old.request.target.as_ref().unwrap().id
    );
    assert_eq!(
        route.request.point_limit,
        motion::PATHFINDER_CACHE_POINT_LIMIT
    );

    let boundary = warm1.boundary();
    assert_eq!(boundary.world(), previous.request.binding.world);
    assert_eq!(boundary.profile(), previous.request.binding.profile);
    assert_eq!(boundary.geometry(), previous.request.binding.geometry);
    assert_eq!(
        boundary.authority(),
        previous.request.binding.authority_policy
    );
    assert_eq!(boundary.tick(), previous.request.binding.tick);
    let head = &boundary.heads()[0];
    assert_eq!(head.npc(), route.owner.npc());
    assert_eq!(head.constructor_kind(), Some(live.constructor_kind()));
    assert_eq!(head.construction(), Some(live.construction_binding()));
    assert!(head.suspended_wander().is_none());
    let cache = route.request.prior_cache.as_deref().unwrap();
    assert_eq!(
        Some(cache),
        head.cache(),
        "compare all dynamic backing slots and cache bindings"
    );
    let Facts::SourceMotion(MotionFacts::Spatial(spatial)) = &previous.facts else {
        panic!("first retained ACK carries its actual spatial facts")
    };
    assert_eq!(spatial.request, motion::Query::Route(old.request.clone()));
    let motion::Fact::RouteCalculation(calculation) = &spatial.fact else {
        panic!("first retained ACK carries its complete calculation")
    };
    calculation.validate_for(&old.request).unwrap();
    assert_eq!(cache, &calculation.after_cache);
    assert_eq!(cache.revision, 2);
    assert_eq!(cache.source, motion::RouteSource::OwnerPathFinder);
    assert_eq!(cache.active_len, 1);
    assert_eq!(cache.point_limit, motion::PATHFINDER_CACHE_POINT_LIMIT);
    assert_eq!(cache.include_flags, 13);
    assert_eq!(cache.exclude_flags, 0);
    assert_eq!(cache.path_type, 1);
}

pub(super) fn assert_facts(request: &AuthorityRequest, facts: &Facts) {
    let Query::SourceMotion(MotionQuery::Route(route)) = &request.query else {
        panic!("third-epoch facts must belong to an actual Route request")
    };
    let Facts::SourceMotion(MotionFacts::Spatial(spatial)) = facts else {
        panic!("third epoch must emit genuine paired spatial facts")
    };
    assert_eq!(spatial.request, motion::Query::Route(route.request.clone()));
    let motion::Fact::RouteCalculation(calculation) = &spatial.fact else {
        panic!("third epoch must carry its complete calculation")
    };
    calculation.validate_for(&route.request).unwrap();
    let before = route.request.prior_cache.as_deref().unwrap();
    let after = &calculation.after_cache;
    assert_eq!(calculation.prior_cache, route.request.prior_cache);
    assert_eq!(before.revision, 2);
    assert_eq!(calculation.source, motion::RouteSource::OwnerPathFinder);
    let motion::RouteOutcome::Complete(path) = &calculation.outcome else {
        panic!("actual third calculation must be NORMAL, not authored NoPath")
    };
    assert_eq!(path.source, motion::RouteSource::OwnerPathFinder);
    assert_eq!(path.points.first(), Some(&route.request.launch_origin));
    assert_eq!(path.points.last(), Some(&after.requested_end));
    assert!(calculation.wander_no_path.is_none());
    assert_eq!(after.revision, 3);
    assert_eq!(after.binding, before.binding);
    assert_eq!(after.source, before.source);
    assert_eq!(after.point_limit, motion::PATHFINDER_CACHE_POINT_LIMIT);
    assert_eq!(after.active_len, 1);
    assert_eq!(after.include_flags, 13);
    assert_eq!(after.exclude_flags, 0);
    assert_eq!(after.path_type, 1);
    // Full native backing equality is checked by the certifier, not fabricated
    // here from a fixed 75-slot first calculation or its active prefix.
    assert!(after.poly_refs.len() >= before.poly_refs.len());
    eprintln!(
        "resident actual third calculation: tick={} query={} now-ms={} prior-revision=2 after-revision=3 prior-storage={} storage={} active=1 filter=13 origin={:?} goal={:?}",
        request.binding.tick,
        route.request.binding.query_id,
        route.request.binding.now_ms,
        before.poly_refs.len(),
        after.poly_refs.len(),
        route.request.origin,
        route.request.goal
    );
}

/// Cancellation-at-stage diagnostics only: these do not fabricate wet, ignored
/// or missing-tile outcomes. Source-only results are dropped before certification.
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
            ledger.probe_second_retained_source_for_test(
                provider,
                request,
                401,
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
            "no candidate or Facts staged by third-epoch source probe"
        );
    }
    let mut phases = vec![];
    ledger
        .probe_second_retained_source_for_test(
            provider,
            request,
            401,
            Instant::now() + Duration::from_secs(30),
            || false,
            |phase| phases.push(phase),
        )
        .unwrap();
    assert_eq!(phases, ORDER);
    assert_eq!(ledger.diagnostic(), before);
    eprintln!("resident third source order: {phases:?}; cancellation prefixes=4; staged=0");
}

/// Immutable checkpoint identities are retained after the actual saved tick has
/// retired its live journal, for pairing with the owner's accepted notice.
pub(super) struct CommittedTick {
    before: CheckpointIdentity,
    after: CheckpointIdentity,
    before_generation: u64,
    saved: SavedBoundary,
}

pub(super) fn assert_commit_proof(proof: &CommittedTick, notice: &RouteCommitNotice) {
    let boundary = notice.boundary();
    assert_eq!(proof.before.tick.checked_add(1), Some(proof.after.tick));
    assert_eq!(proof.before.world, proof.after.world);
    assert_eq!(proof.before.player, proof.after.player);
    assert_eq!(proof.before.profile, proof.after.profile);
    assert_eq!(proof.before.domain, proof.after.domain);
    assert_ne!(proof.before.history_tip, proof.after.history_tip);
    assert_eq!(proof.saved.tick, proof.after.tick);
    assert_eq!(
        proof.before_generation.checked_add(1),
        Some(proof.saved.generation)
    );
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
    let reply = &notice.routes()[0];
    assert_eq!(reply.request.binding.tick, proof.after.tick);
    assert_eq!(reply.request.binding.world, proof.after.world);
    assert_eq!(reply.request.binding.profile, proof.after.profile);
    assert_facts(&reply.request, &reply.facts);
    let Facts::SourceMotion(MotionFacts::Spatial(spatial)) = &reply.facts else {
        unreachable!()
    };
    let motion::Fact::RouteCalculation(calculation) = &spatial.fact else {
        unreachable!()
    };
    assert_eq!(boundary.heads()[0].cache(), Some(&calculation.after_cache));
    assert_eq!(calculation.after_cache.revision, 3);
    eprintln!(
        "resident third durable proof: tick={} generation={} accepted-routes=1 cache-revision=3 live-journal-retired=true",
        proof.saved.tick, proof.saved.generation
    );
}

pub(super) fn after_warm_ack(
    driver: &mut Driver,
    callbacks: &AtomicUsize,
    routes: &AtomicUsize,
    warm_tick: u64,
    warm_events: usize,
    mode: Mode,
) -> (u64, usize, usize, CommittedTick) {
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
        let mut out = driver.pump_one();
        if routes.load(Ordering::SeqCst) == 2 {
            assert!(
                out.pause.is_none(),
                "unexpected earlier callback: {:?}",
                out.pause
            );
            assert!(driver.pause().is_none());
            assert_eq!(out.ticks, 1);
            assert_eq!(driver.tick(), tick + 1);
            let saved = out
                .saved
                .as_ref()
                .expect("intervening movement tick saved normally");
            assert_eq!(saved.tick, tick + 1);
            assert_eq!(Some(saved.generation), generation.checked_add(1));
            assert_eq!(driver.store_generation(), saved.generation);
            assert!(
                driver.records().is_empty(),
                "successful save retires the live journal"
            );
            assert!(driver.pending_inputs().is_none());
            assert!(!driver.route_pending());
            assert_eq!(driver.queued_actions(), 0);
            events += out.events.len();
            continue;
        }
        assert_eq!(
            routes.load(Ordering::SeqCst),
            3,
            "one actual third dispatch per tick"
        );
        if mode == Mode::RollbackRetry {
            assert_eq!(
                out.pause.as_ref().map(|pause| &pause.kind),
                Some(&PauseKind::Authority)
            );
            assert!(out.pause.as_ref().unwrap().message.contains(LATER_FAILURE));
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
                .expect("rolled-back authenticated movement remains pending")
                .to_vec();
            assert_eq!(inputs.len(), 1);
            assert_eq!(inputs[0].input.world, driver.state().world);
            assert_eq!(inputs[0].input.tick, tick + 1);
            assert_eq!(inputs[0].input.player, driver.state().player);
            assert_eq!(inputs[0].input.sequence, driver.state().next_sequence);
            assert_eq!(inputs[0].authenticated_owner, driver.state().player.owner);
            assert!(matches!(
                &inputs[0].input.command,
                Command::MovementPose { destination: actual, orientation, movement_flags }
                    if *actual == destination && orientation.as_f32() == 0.5 && movement_flags.bits() == 0
            ));
            let count = callbacks.load(Ordering::SeqCst);
            let blocked = driver.pump_zero();
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
            driver.retry_tick().unwrap();
            assert!(driver.pause().is_none());
            assert_eq!(driver.pending_inputs().unwrap(), inputs);
            assert_eq!(driver.checkpoint(), before);
            assert_eq!(driver.records(), records);
            assert_eq!(driver.store_generation(), generation);
            assert_eq!(callbacks.load(Ordering::SeqCst), count);
            assert_eq!(routes.load(Ordering::SeqCst), 3);
            out = driver.pump_zero();
            assert_eq!(
                routes.load(Ordering::SeqCst),
                4,
                "same authenticated input gets one fresh live attempt"
            );
        }
        let expected_attempts = if mode == Mode::RollbackRetry { 4 } else { 3 };
        assert_eq!(routes.load(Ordering::SeqCst), expected_attempts);
        assert_eq!(out.ticks, 1);
        assert_eq!(driver.tick(), tick + 1);
        let saved = out
            .saved
            .clone()
            .expect("actual third tick saved before ACK delivery");
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
            before_generation: generation,
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
        if mode == Mode::DropAfterApply {
            assert_eq!(
                out.pause.as_ref().map(|pause| &pause.kind),
                Some(&PauseKind::CommittedAwaitingRouteAck)
            );
            assert_eq!(driver.pause(), out.pause.as_ref());
            assert!(driver.route_pending());
            let blocked = driver.pump_one();
            assert_eq!(blocked.pause, out.pause);
            assert_eq!(driver.pause(), blocked.pause.as_ref());
            assert_eq!(blocked.ticks, 0);
            assert!(blocked.events.is_empty());
            assert!(blocked.saved.is_none());
            assert_eq!(callbacks.load(Ordering::SeqCst), count);
            assert_eq!(routes.load(Ordering::SeqCst), expected_attempts);
            assert_eq!(driver.tick(), tick + 1);
            assert_eq!(driver.checkpoint(), checkpoint);
            assert_eq!(driver.records(), committed_records);
            assert_eq!(driver.store_generation(), committed_generation);
            assert!(driver.route_pending());
            assert!(driver.pending_inputs().is_none());
            assert_eq!(driver.queued_actions(), 0);
            driver.retry_route_commit().unwrap();
        } else {
            assert!(out.pause.is_none(), "{:?}", out.pause);
        }
        assert!(!driver.route_pending());
        assert!(driver.pause().is_none());
        assert!(driver.pending_inputs().is_none());
        assert_eq!(driver.queued_actions(), 0);
        assert_eq!(driver.checkpoint(), checkpoint);
        assert_eq!(driver.records(), committed_records);
        assert_eq!(driver.store_generation(), committed_generation);
        assert_eq!(
            callbacks.load(Ordering::SeqCst),
            count,
            "ACK delivery never replays callbacks"
        );
        assert_eq!(routes.load(Ordering::SeqCst), expected_attempts);
        let flushed = driver.pump_zero();
        assert!(flushed.pause.is_none());
        assert_eq!(flushed.ticks, 0);
        assert!(
            flushed.events.is_empty(),
            "committed output is published only once"
        );
        assert!(
            flushed.saved.is_none(),
            "ACK retry never repeats persistence"
        );
        assert_eq!(driver.tick(), tick + 1);
        assert_eq!(driver.checkpoint(), checkpoint);
        assert_eq!(driver.records(), committed_records);
        assert_eq!(driver.store_generation(), committed_generation);
        assert_eq!(callbacks.load(Ordering::SeqCst), count);
        assert_eq!(routes.load(Ordering::SeqCst), expected_attempts);
        assert!(!driver.route_pending());
        assert!(driver.pending_inputs().is_none());
        assert_eq!(driver.queued_actions(), 0);
        events += out.events.len();
        eprintln!(
            "resident three-epoch ACK measurement: mode={mode:?} warm1-tick={warm_tick} warm2-tick={} input-step={step} accepted-epochs=3",
            tick + 1
        );
        return (tick + 1, events, count, committed);
    }
    panic!("ordinary bounded player movement produced no actual third Chase dispatch");
}

#[test]
fn resident_controller_three_epoch_commits_exact_ack_selected_predecessor() {
    super::source_route_resident_controller::three_epoch_probe(Mode::Accept);
}

#[test]
fn resident_controller_three_epoch_later_failure_rolls_back_and_selects_fresh_retry() {
    super::source_route_resident_controller::three_epoch_probe(Mode::RollbackRetry);
}

#[test]
fn resident_controller_three_epoch_lost_ack_retries_without_replay() {
    super::source_route_resident_controller::three_epoch_probe(Mode::DropAfterApply);
}

#[test]
fn resident_controller_three_epoch_refusals_preserve_predecessor_before_fresh_success() {
    super::source_route_resident_controller::three_epoch_probe(Mode::Refusals);
}
