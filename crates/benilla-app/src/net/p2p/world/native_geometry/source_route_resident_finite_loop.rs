// SPDX-License-Identifier: GPL-3.0-or-later
//! Component-only finite cold plus seven retained Chase epochs. Ordinary
//! authenticated target movement drives every actual callback and saved tick.
use super::route_commit::RouteCommitNotice;
use crate::net::p2p::world::{resident_fixture::Driver, PauseKind, SavedBoundary};
use benilla_sim::{
    creature_ai::{
        movement::{Check, SegmentChecks},
        Point,
    },
    creature_motion as motion,
};
use benilla_world_runtime::native::{
    AuthorityRequest, CheckpointIdentity, Command, Facts, MotionFacts, MotionQuery, Profile, Query,
};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Recovery,
    RefusalsHistory,
    OldPolicyBoundary,
    StrictStraightBoundary,
    StrictGeneratedSupportBoundary,
    StrictEndpointSupportBoundary,
}

/// Injected once by the owner on a real non-Route callback after epoch-four Facts.
pub(super) const LATER_FAILURE: &str =
    "deliberate later callback after actual fourth resident route facts";

pub(super) fn assert_request(
    profile: &Profile,
    previous_notice: &RouteCommitNotice,
    request: &AuthorityRequest,
    expected_revision: u64,
) {
    assert!((2..=8).contains(&expected_revision));
    assert_eq!(profile.tick_ms, 25);
    assert_eq!(previous_notice.routes().len(), 1);
    assert_eq!(previous_notice.boundary().heads().len(), 1);
    let previous = &previous_notice.routes()[0];
    let Query::SourceMotion(MotionQuery::Route(old)) = &previous.request.query else {
        panic!("previous accepted notice must contain its actual Route callback")
    };
    let Query::SourceMotion(MotionQuery::Route(route)) = &request.query else {
        panic!("finite loop requires an actual retained Route callback")
    };
    let saved = previous
        .request
        .require_pathfinder_dispatch(profile)
        .unwrap();
    let live = request.require_pathfinder_dispatch(profile).unwrap();
    assert_eq!(saved.first_construction(), expected_revision == 2);
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
    assert!(route.collision_width.is_some());
    assert!(route.collision_height.is_some());
    assert_eq!(
        route.request.owner_collision_height,
        old.request.owner_collision_height
    );
    assert_eq!(
        route.target_player_height.as_ref().unwrap().witness(),
        old.target_player_height.as_ref().unwrap().witness()
    );
    assert_eq!(route.owner_ignore_pathfinding, old.owner_ignore_pathfinding);
    assert!(!route
        .owner_ignore_pathfinding
        .as_ref()
        .unwrap()
        .ignore_pathfinding());
    assert_eq!(route.request.kind, motion::RouteKind::ChaseReplan);
    assert!(route.request.wander.is_none());
    // These are the live callback's operands, not replayed poses or a new target.
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

    let boundary = previous_notice.boundary();
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
        "exact prior cache includes every dynamic backing slot and binding"
    );
    let Facts::SourceMotion(MotionFacts::Spatial(spatial)) = &previous.facts else {
        panic!("previous accepted notice must carry its actual spatial facts")
    };
    assert_eq!(spatial.request, motion::Query::Route(old.request.clone()));
    let motion::Fact::RouteCalculation(calculation) = &spatial.fact else {
        panic!("previous accepted notice must carry its complete calculation")
    };
    calculation.validate_for(&old.request).unwrap();
    assert_eq!(cache, &calculation.after_cache);
    assert_eq!(cache.revision, expected_revision - 1);
    assert_eq!(cache.source, motion::RouteSource::OwnerPathFinder);
    assert_eq!(cache.point_limit, motion::PATHFINDER_CACHE_POINT_LIMIT);
    assert_eq!(cache.include_flags, 13);
    assert_eq!(cache.exclude_flags, 0);
    assert_eq!(cache.path_type, 1);
    if expected_revision == 4 {
        assert_eq!(cache.revision, 3);
        eprintln!(
            "resident finite actual 3->4 request: previous-tick={} tick={} prior-revision=3 query={} now-ms={}",
            previous.request.binding.tick,
            request.binding.tick,
            route.request.binding.query_id,
            route.request.binding.now_ms
        );
    }
}

pub(super) fn assert_facts(request: &AuthorityRequest, facts: &Facts, expected_revision: u64) {
    assert!((2..=8).contains(&expected_revision));
    let Query::SourceMotion(MotionQuery::Route(route)) = &request.query else {
        panic!("finite-epoch facts must belong to an actual Route request")
    };
    let Facts::SourceMotion(MotionFacts::Spatial(spatial)) = facts else {
        panic!("finite epoch must emit genuine paired spatial facts")
    };
    assert_eq!(spatial.request, motion::Query::Route(route.request.clone()));
    let motion::Fact::RouteCalculation(calculation) = &spatial.fact else {
        panic!("finite epoch must carry its complete calculation")
    };
    calculation.validate_for(&route.request).unwrap();
    let before = route.request.prior_cache.as_deref().unwrap();
    let after = &calculation.after_cache;
    assert_eq!(calculation.prior_cache, route.request.prior_cache);
    assert_eq!(before.revision, expected_revision - 1);
    assert_eq!(calculation.source, motion::RouteSource::OwnerPathFinder);
    let motion::RouteOutcome::Complete(path) = &calculation.outcome else {
        panic!("actual finite calculation must be NORMAL, not authored NoPath")
    };
    assert_eq!(path.source, motion::RouteSource::OwnerPathFinder);
    assert_eq!(path.points.first(), Some(&route.request.launch_origin));
    assert_eq!(path.points.last(), Some(&after.requested_end));
    assert!(calculation.wander_no_path.is_none());
    assert_eq!(after.revision, expected_revision);
    assert_eq!(after.binding, before.binding);
    assert_eq!(after.source, before.source);
    assert_eq!(after.point_limit, motion::PATHFINDER_CACHE_POINT_LIMIT);
    assert_eq!(after.include_flags, 13);
    assert_eq!(after.exclude_flags, 0);
    assert_eq!(after.path_type, 1);
    // The owner verifies every dynamic point and cache slot against its sealed
    // native terminal, not a fixed two-point or 75-slot diagnostic shape.
    assert_eq!(path.segments.len() + 1, path.points.len());
    assert!(path.segments.iter().all(|segment| *segment
        == SegmentChecks {
            loaded_coverage: Check::Passed,
            line_of_sight: Check::Passed,
            body_sweep: Check::Passed,
            continuous_ground_support: Check::Passed,
            dry_ground: Check::Passed,
        }));
    eprintln!(
        "resident finite actual calculation: epoch={expected_revision} tick={} query={} now-ms={} prior-revision={} after-revision={} prior-storage={} storage={} active={} points={} filter=13 origin={:?} goal={:?}",
        request.binding.tick,
        route.request.binding.query_id,
        route.request.binding.now_ms,
        before.revision,
        after.revision,
        before.poly_refs.len(),
        after.poly_refs.len(),
        after.active_len,
        path.points.len(),
        route.request.origin,
        route.request.goal
    );
    if expected_revision == 4 {
        assert_eq!((before.revision, after.revision), (3, 4));
        assert!(
            after.active_len > 1,
            "actual fourth terminal crosses polygons"
        );
        assert!(
            path.points.len() > 2,
            "actual fourth terminal has native intermediate points"
        );
        eprintln!("resident finite actual 3->4 calculation: complete genuine native multi-polygon straight facts");
    }
}

/// Cancellation diagnostics do not stage Facts or fabricate a source branch.
/// The real attempt must certify separately after these source-only results drop.
pub(super) fn assert_source_order(
    ledger: &super::source_route_resident_local::ledger::ResidentLedger,
    provider: &mut super::source_route_filter::resident::ResidentFilterProvider<'_, '_, '_, '_>,
    request: &AuthorityRequest,
    ticket: u64,
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
    let Query::SourceMotion(MotionQuery::Route(route)) = &request.query else {
        panic!("source-order probe requires the actual fifth Route callback")
    };
    assert_eq!(route.request.prior_cache.as_deref().unwrap().revision, 4);
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
            ledger.probe_straight_source_for_test(
                provider,
                request,
                ticket,
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
            "cancellation, not expiry, refused this source-only attempt"
        );
        assert_eq!(
            ledger.diagnostic(),
            before,
            "fifth-epoch source probe stages neither a candidate nor Facts"
        );
    }
    let mut phases = vec![];
    ledger
        .probe_straight_source_for_test(
            provider,
            request,
            ticket,
            Instant::now() + Duration::from_secs(30),
            || false,
            |phase| phases.push(phase),
        )
        .unwrap();
    assert_eq!(phases, ORDER);
    assert_eq!(ledger.diagnostic(), before);
    eprintln!(
        "resident finite fifth source order: {phases:?}; cancellation prefixes=4; staged=0; fresh certification follows"
    );
}

/// Identities from the actual saved tick survive retirement of its live journal.
pub(super) struct CommittedTick {
    revision: u64,
    before: CheckpointIdentity,
    after: CheckpointIdentity,
    before_generation: u64,
    saved: SavedBoundary,
}

pub(super) fn assert_commit_proof(proof: &CommittedTick, notice: &RouteCommitNotice) {
    assert!((2..=8).contains(&proof.revision));
    let boundary = notice.boundary();
    assert_eq!(proof.before.tick.checked_add(1), Some(proof.after.tick));
    assert_eq!(proof.before.world, proof.after.world);
    assert_eq!(proof.before.player, proof.after.player);
    assert_eq!(proof.before.profile, proof.after.profile);
    assert_eq!(proof.before.domain, proof.after.domain);
    assert_ne!(proof.before.history_tip, proof.after.history_tip);
    assert_ne!(proof.before.state_digest, proof.after.state_digest);
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
    assert_facts(&reply.request, &reply.facts, proof.revision);
    let Facts::SourceMotion(MotionFacts::Spatial(spatial)) = &reply.facts else {
        unreachable!()
    };
    let motion::Fact::RouteCalculation(calculation) = &spatial.fact else {
        unreachable!()
    };
    assert_eq!(boundary.heads()[0].cache(), Some(&calculation.after_cache));
    assert_eq!(calculation.after_cache.revision, proof.revision);
    eprintln!(
        "resident finite durable proof: epoch={} tick={} generation={} accepted-routes=1 cache-revision={} live-journal-retired=true",
        proof.revision, proof.saved.tick, proof.saved.generation, proof.revision
    );
}

fn assert_ready(driver: &Driver) {
    assert!(driver.pause().is_none(), "{:?}", driver.pause());
    assert!(!driver.route_pending());
    assert!(driver.pending_inputs().is_none());
    assert_eq!(driver.queued_actions(), 0);
}

pub(super) fn after_cold_ack(
    driver: &mut Driver,
    callbacks: &AtomicUsize,
    routes: &AtomicUsize,
    cold_tick: u64,
    cold_events: usize,
    mode: Mode,
) -> (u64, usize, usize, Vec<CommittedTick>) {
    assert_eq!(driver.tick(), cold_tick);
    assert!((1..=24).contains(&cold_tick));
    assert_eq!(routes.load(Ordering::SeqCst), 1);
    assert_ready(driver);
    let original_position = driver.state().position;
    let mut events = cold_events;
    let mut committed = Vec::with_capacity(7);
    let mut rolled_back = false;
    let final_revision = match mode {
        Mode::OldPolicyBoundary => 4,
        Mode::StrictStraightBoundary | Mode::StrictGeneratedSupportBoundary => 5,
        Mode::StrictEndpointSupportBoundary => 6,
        Mode::Recovery | Mode::RefusalsHistory => 8,
    };
    for revision in 2..=final_revision {
        let start = driver.state().position;
        let attempts_before = routes.load(Ordering::SeqCst);
        assert_eq!(
            attempts_before,
            revision as usize - 1 + usize::from(rolled_back)
        );
        let mut accepted = false;
        // At most 24 + 7 * 64 successful 25 ms ticks, or 11.8 s, within the
        // unchanged 15 s pursuit. No NPC, timer, speed, target or cache is edited.
        for step in 1..=64 {
            let destination = Point::new(
                start.x.as_f32() - step as f32 * 0.125,
                original_position.y.as_f32(),
                original_position.z.as_f32(),
            )
            .unwrap();
            assert_ready(driver);
            driver.queue_movement(destination, 0.5).unwrap();
            assert_eq!(driver.queued_actions(), 1);
            let before = driver.checkpoint();
            let records = driver.records().to_vec();
            let generation = driver.store_generation();
            let tick = driver.tick();
            let now_ms = driver.state().now_ms;
            let sequence = driver.state().next_sequence;
            let old_boundary = (mode == Mode::OldPolicyBoundary && revision == 4).then(|| {
                super::source_route_resident_straight_refusals::RefusedTick::capture(
                    driver,
                    destination,
                )
            });
            let strict_boundary =
                (mode == Mode::StrictStraightBoundary && revision == 5).then(|| {
                    super::source_route_resident_supported_refusals::RefusedTick::capture(
                        driver,
                        destination,
                    )
                });
            let generated_boundary =
                (mode == Mode::StrictGeneratedSupportBoundary && revision == 5).then(|| {
                    super::source_route_resident_planar_refusals::RefusedTick::capture(
                        driver,
                        destination,
                    )
                });
            let endpoint_boundary = (mode == Mode::StrictEndpointSupportBoundary && revision == 6)
                .then(|| {
                    super::source_route_resident_endpoint_refusals::RefusedTick::capture(
                        driver,
                        destination,
                    )
                });
            let mut out = driver.pump_one();
            let route_attempted = routes.load(Ordering::SeqCst) != attempts_before;
            if route_attempted {
                assert_eq!(
                    routes.load(Ordering::SeqCst),
                    attempts_before + 1,
                    "exactly one genuine Route dispatch per finite-loop tick"
                );
                if mode == Mode::OldPolicyBoundary && revision == 4 {
                    old_boundary
                        .unwrap()
                        .assert_refused(driver, callbacks, routes, &out);
                    assert_eq!(committed.len(), 2);
                    assert!(!rolled_back);
                    assert_eq!(driver.tick(), 97);
                    assert_eq!(driver.state().now_ms, 2425);
                    return (
                        driver.tick(),
                        events,
                        callbacks.load(Ordering::SeqCst),
                        committed,
                    );
                }
                if mode == Mode::StrictStraightBoundary && revision == 5 {
                    strict_boundary
                        .unwrap()
                        .assert_refused(driver, callbacks, routes, &out);
                    assert_eq!(committed.len(), 3);
                    assert!(!rolled_back);
                    assert_eq!(driver.tick(), 127);
                    assert_eq!(driver.state().now_ms, 3175);
                    return (
                        driver.tick(),
                        events,
                        callbacks.load(Ordering::SeqCst),
                        committed,
                    );
                }
                if mode == Mode::StrictGeneratedSupportBoundary && revision == 5 {
                    generated_boundary
                        .unwrap()
                        .assert_refused(driver, callbacks, routes, &out);
                    assert_eq!(committed.len(), 3);
                    assert!(!rolled_back);
                    assert_eq!(driver.tick(), 127);
                    assert_eq!(driver.state().now_ms, 3175);
                    return (
                        driver.tick(),
                        events,
                        callbacks.load(Ordering::SeqCst),
                        committed,
                    );
                }
                if mode == Mode::StrictEndpointSupportBoundary && revision == 6 {
                    endpoint_boundary
                        .unwrap()
                        .assert_refused(driver, callbacks, routes, &out);
                    assert_eq!(committed.len(), 4);
                    assert!(!rolled_back);
                    assert_eq!(driver.tick(), 157);
                    assert_eq!(driver.state().now_ms, 3925);
                    return (
                        driver.tick(),
                        events,
                        callbacks.load(Ordering::SeqCst),
                        committed,
                    );
                }
                if mode == Mode::Recovery && revision == 4 {
                    assert!(!rolled_back);
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
                    assert_eq!(inputs[0].input.sequence, sequence);
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
                    assert_eq!(routes.load(Ordering::SeqCst), attempts_before + 1);
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
                    assert_eq!(routes.load(Ordering::SeqCst), attempts_before + 1);
                    out = driver.pump_zero();
                    assert_eq!(
                        routes.load(Ordering::SeqCst),
                        attempts_before + 2,
                        "identical authenticated input gets one fresh live Route attempt"
                    );
                    rolled_back = true;
                }
            }
            let expected_attempts =
                revision as usize - usize::from(!route_attempted) + usize::from(rolled_back);
            assert_eq!(routes.load(Ordering::SeqCst), expected_attempts);
            assert_eq!(out.ticks, 1, "finite epoch {revision}: {:?}", out.pause);
            assert_eq!(driver.tick(), tick + 1);
            assert_eq!(driver.state().now_ms, now_ms + 25);
            assert_eq!(driver.state().next_sequence, sequence + 1);
            assert_eq!(driver.state().position, destination);
            assert_eq!(driver.state().orientation.as_f32(), 0.5);
            let saved = out
                .saved
                .clone()
                .expect("each successful movement tick is durably saved separately");
            assert_eq!(saved.tick, tick + 1);
            assert_eq!(Some(saved.generation), generation.checked_add(1));
            assert_eq!(driver.store_generation(), saved.generation);
            assert!(
                driver.records().is_empty(),
                "each successful save retires the complete live journal"
            );
            let checkpoint = driver.checkpoint();
            assert_ne!(
                before.identity().history_tip,
                checkpoint.identity().history_tip
            );
            assert_ne!(
                before.identity().state_digest,
                checkpoint.identity().state_digest
            );
            assert!(driver.pending_inputs().is_none());
            assert_eq!(driver.queued_actions(), 0);
            events += out.events.len();
            if !route_attempted {
                assert!(
                    out.pause.is_none(),
                    "unsupported intervening callback is a failure, not an epoch: {:?}",
                    out.pause
                );
                assert_ready(driver);
                continue;
            }
            committed.push(CommittedTick {
                revision,
                before: before.identity(),
                after: checkpoint.identity(),
                before_generation: generation,
                saved,
            });
            let committed_records = driver.records().to_vec();
            let committed_generation = driver.store_generation();
            let count = callbacks.load(Ordering::SeqCst);
            let lost_ack = (mode == Mode::Recovery && revision == 6)
                || (mode == Mode::RefusalsHistory && revision == 8);
            if lost_ack {
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
            assert_ready(driver);
            assert_eq!(driver.tick(), tick + 1);
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
                "saved output publishes only once"
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
            assert_ready(driver);
            eprintln!(
                "resident finite ACK measurement: mode={mode:?} epoch={revision} cold-tick={cold_tick} tick={} successful-input-steps={step} attempts={expected_attempts} lost-ack={lost_ack}",
                tick + 1
            );
            accepted = true;
            break;
        }
        assert!(
            accepted,
            "ordinary movement produced no genuine Route epoch {revision} within 64 successful ticks"
        );
    }
    assert_eq!(committed.len(), 7);
    assert_eq!(rolled_back, mode == Mode::Recovery);
    assert_eq!(
        routes.load(Ordering::SeqCst),
        if mode == Mode::Recovery { 9 } else { 8 }
    );
    assert!(driver.tick() - cold_tick <= 7 * 64);
    assert!(driver.tick() <= 24 + 7 * 64);
    assert!(driver.state().now_ms <= 11_800);
    assert_ready(driver);
    let count = callbacks.load(Ordering::SeqCst);
    eprintln!(
        "resident finite exact stop: mode={mode:?} accepted-epochs=8 route-attempts={} tick={} now-ms={} callbacks={count} pending-input=none pause=none route-pending=false queued-actions=0",
        routes.load(Ordering::SeqCst), driver.tick(), driver.state().now_ms
    );
    (driver.tick(), events, count, committed)
}

#[test]
fn finite_eight_epoch_loop_recovers_rollback_expiry_and_lost_ack() {
    super::source_route_resident_controller::run_finite_loop(Mode::Recovery);
}

#[test]
fn finite_eight_epoch_loop_refuses_stale_scene_and_historically_acks_source_change() {
    super::source_route_resident_controller::run_finite_loop(Mode::RefusalsHistory);
}
