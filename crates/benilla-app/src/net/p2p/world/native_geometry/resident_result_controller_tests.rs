// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual controller saves and commit-channel ACKs for the component-only resident transport.
use super::{
    core::{
        resident_dispatch::{
            fifo::ResidentFifoSelection,
            transport::{
                paired_controller_proof_mailbox, paired_test_mailbox, ResidentTestAuthority,
            },
        },
        Failure, PlayerEnvironmentPolicy, RouteCommitCommand, ServiceQuery, ServiceStep,
    },
    frame_cadenced_controller_tests as cadence,
    products::{
        source_height::{
            freshness_timings, inventory::OwnedInventoryCompatibility, owned::OwnedSourceProducts,
            ResidentInitializationConfiguration,
        },
        vmap::installed_source_fixture as input,
    },
    real_corridor_fixture::installed_source::InstalledFixture,
    route_commit::RouteCommitNotice,
    source_creature_initialization::resident::ResidentCreatureInitializationProvider,
    source_pathfinder_query::resident_joined_tests as fixture,
    source_player_environment::resident::ResidentEnvironmentError,
    source_route_filter::resident::{
        owned::{OwnedResident, ResidentPhase, ResidentPhaseResult},
        ResidentFilterError, ResidentFilterProvider,
    },
    source_route_resident_retained,
};
use crate::net::p2p::world::{resident_fixture, PauseKind};
use benilla_sim::{creature_ai as ai, creature_motion as motion};
use benilla_world::terrain_stream::NativeStaticMapIndex;
use benilla_world_runtime::native::{
    self as n, Authority, AuthorityReply, AuthorityRequest, Facts, MotionFacts, MotionQuery,
    NativeError, Query,
};
use bevy::diagnostic::FrameCount;
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const CONFIG: Option<ResidentInitializationConfiguration> =
    Some(ResidentInitializationConfiguration::VmapHeightEnabled);

struct Initializer<F>(F, n::synthetic_motion_fixture::SyntheticMotionAuthority);
impl<F: FnMut(&AuthorityRequest) -> Result<Facts, NativeError>> Authority for Initializer<F> {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if matches!(request.query, Query::CreatureInitialization(_)) {
            return Ok(AuthorityReply {
                request: request.clone(),
                facts: (self.0)(request)?,
            });
        }
        assert!(!matches!(
            request.query,
            Query::SourceMotion(
                MotionQuery::Route(_) | MotionQuery::NearPoint(_) | MotionQuery::Spatial(_)
            )
        ));
        self.1.resolve(request)
    }
}

/// Only nonspatial fixture facts bypass the real mailbox; every Route and
/// source-backed environment/near-point observation uses the paired transport.
struct ControllerAuthority {
    retained_deadline_refusals: Arc<AtomicUsize>,
    expect_retained_deadline: bool,
    resident: ResidentTestAuthority,
    authored: n::synthetic_motion_fixture::SyntheticMotionAuthority,
}
impl Authority for ControllerAuthority {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        match &request.query {
            Query::PlayerEnvironment(_)
            | Query::SourceMotion(MotionQuery::Route(_) | MotionQuery::NearPoint(_)) => {
                let started = Instant::now();
                let result = self.resident.resolve(request);
                if self.expect_retained_deadline
                    && matches!(&request.query, Query::SourceMotion(MotionQuery::Route(route)) if route.request.prior_cache.is_some())
                {
                    assert!(matches!(
                        result,
                        Err(NativeError::AuthorityUnavailable(
                            "local geometry request deadline exceeded"
                        ))
                    ));
                    self.retained_deadline_refusals
                        .fetch_add(1, Ordering::SeqCst);
                }
                if matches!(request.query, Query::SourceMotion(MotionQuery::Route(_))) {
                    // This includes finish_reply's active, identity and expiry checks.
                    // It excludes the controller's subsequent durable save and ACK.
                    eprintln!(
                        "resident controller Route authority returned: tick={} elapsed_us={} success={}",
                        request.binding.tick,
                        started.elapsed().as_micros(),
                        result.is_ok(),
                    );
                }
                if let Err(error) = &result {
                    let kind = match &request.query {
                        Query::PlayerEnvironment(_) => "PlayerEnvironment",
                        Query::SourceMotion(MotionQuery::NearPoint(_)) => "NearPoint",
                        Query::SourceMotion(MotionQuery::Route(_)) => "Route",
                        _ => unreachable!(),
                    };
                    eprintln!(
                        "resident controller authority refused: tick={} kind={kind} error={error:?}",
                        request.binding.tick,
                    );
                }
                result
            }
            Query::SourceMotion(MotionQuery::Spatial(motion::Query::Bearing(bearing))) => {
                let angle = benilla_sim::facing::bearing_bits(
                    [bearing.origin.x.bits(), bearing.origin.y.bits()],
                    [bearing.toward.x.bits(), bearing.toward.y.bits()],
                )
                .map_err(|_| NativeError::AuthorityBinding)?;
                Ok(AuthorityReply {
                    request: request.clone(),
                    facts: Facts::SourceMotion(MotionFacts::Spatial(Box::new(motion::Reply {
                        request: motion::Query::Bearing(bearing.clone()),
                        fact: motion::Fact::Bearing(
                            ai::Real::from_bits(angle)
                                .map_err(|_| NativeError::AuthorityBinding)?,
                        ),
                    }))),
                })
            }
            Query::SourceMotion(MotionQuery::Spatial(_)) => Err(NativeError::AuthorityUnavailable(
                "unsupported resident fixture spatial query",
            )),
            _ => self.authored.resolve(request),
        }
    }
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        self.authored.observe_derived(reply)
    }
}

/// Only this authored temporary fixture is changed; restoration also runs on panic.
struct AuthoredSourceChange<'a> {
    install: &'a input::Install,
    original: Vec<(String, Vec<u8>)>,
}
impl<'a> AuthoredSourceChange<'a> {
    fn begin(install: &'a input::Install) -> Self {
        let original = input::navigation_entries();
        let before = install.bytes();
        let mut changed = original.clone();
        changed
            .iter_mut()
            .find(|(name, _)| name == input::WDT)
            .unwrap()
            .1[20] ^= 1;
        let guard = Self { install, original };
        install.write(&changed);
        assert_ne!(install.bytes(), before);
        guard
    }
}
impl Drop for AuthoredSourceChange<'_> {
    fn drop(&mut self) {
        self.install.write(&self.original);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Accept,
    DefaultBudget,
    FrameCadenced,
    Phased,
    PhasedDefaultBudget,
    PhasedFrameCadenced,
    MissedFrame,
    CheckpointFailure,
    LostAck,
    ChangedSourceAck,
    PhysicsChangedWhenReady,
}
impl Mode {
    fn frame_cadenced(self) -> bool {
        matches!(
            self,
            Self::FrameCadenced | Self::MissedFrame | Self::PhasedFrameCadenced
        )
    }
    fn default_budget(self) -> bool {
        matches!(self, Self::DefaultBudget | Self::PhasedDefaultBudget) || self.frame_cadenced()
    }
    fn phased(self) -> bool {
        matches!(
            self,
            Self::Phased | Self::PhasedDefaultBudget | Self::PhasedFrameCadenced
        )
    }
    fn loses_ack(self) -> bool {
        matches!(self, Self::LostAck | Self::ChangedSourceAck)
    }
}
pub(super) struct RunEvidence {
    pub(super) retained_trace: freshness_timings::SourceTrace,
    pub(super) max_owner_service: Duration,
}
pub(super) fn run(mode: Mode) -> RunEvidence {
    let mut retained_trace = Vec::new();
    let mut measured_max_owner_service = Duration::ZERO;
    let checkpoint_failure = mode == Mode::CheckpointFailure;
    fixture::with_selected_controller_fixture(
        fixture::ControllerOwners::Single,
        true,
        false,
        false,
        false,
        fixture::GeneratedSupport::Disabled,
        |install, height, compatibility, inventory, policy, profile, query, other| {
            assert!(
                profile.player_environment.is_some(),
                "the real controller must save every tick"
            );
            let controller_fixture = resident_fixture::Fixture::new(profile.clone(), [8; 16]);
            let player = controller_fixture.player();
            let mut session = fixture::session(&profile);
            session.world = controller_fixture.world_id();
            session.player_environment =
                Some(PlayerEnvironmentPolicy::from_profile(&profile, player).unwrap());
            let ordinary = ResidentFilterProvider::bind_with_policy(
                &session,
                &profile,
                height,
                other,
                policy.clone(),
            )
            .unwrap();
            let initialization = ResidentCreatureInitializationProvider::bind(
                session.world,
                &profile,
                height,
                CONFIG,
                fixture::until(),
                || false,
            )
            .unwrap();
            let mut initializer = Initializer(
                |request: &AuthorityRequest| {
                    initialization
                        .resolve(request, fixture::until(), || false)
                        .map_err(|_| NativeError::AuthorityBinding)
                },
                Default::default(),
            );
            let world = n::NativeWorld::synthetic_component_test_with_initialization(
                profile.clone(),
                session.world,
                player,
                1234,
                None,
                &mut initializer,
            )
            .unwrap();
            let mut geometry = InstalledFixture::new(install.path(), inventory);
            let installed = geometry
                .app
                .world()
                .resource::<NativeStaticMapIndex>()
                .source_provenance()
                .unwrap()
                .clone();
            let terrain =
                benilla_native_terrain::prepare(install.path(), 0, &input::guard_source_tiles())
                    .unwrap();
            let collision = benilla_native_vmap_source::prepare(install.path(), 0, || false)
                .unwrap()
                .into_map_bundle(input::profile(), || false)
                .unwrap()
                .unwrap();
            let products = OwnedSourceProducts::new(terrain, collision, || false).unwrap();
            let provenance =
                OwnedInventoryCompatibility::capture(compatibility, fixture::until(), || false)
                    .unwrap();
            let mut owner = OwnedResident::bind(
                products,
                provenance,
                &session,
                &profile,
                player,
                query,
                policy,
                &installed,
                fixture::until(),
                || false,
            )
            .unwrap();
            let (mut authority, mailbox) = if mode.default_budget() {
                paired_test_mailbox(&owner)
            } else {
                paired_controller_proof_mailbox(&owner)
            }
            .unwrap();
            let sink = authority.take_route_commit_sink().unwrap();
            let retained_deadline_refusals = Arc::new(AtomicUsize::new(0));
            let authority = ControllerAuthority {
                retained_deadline_refusals: retained_deadline_refusals.clone(),
                expect_retained_deadline: mode == Mode::MissedFrame,
                resident: authority,
                authored: Default::default(),
            };
            let mut selection = ResidentFifoSelection::new(&mailbox).unwrap();
            let routes = Arc::new(AtomicUsize::new(0));
            let calls = Arc::new(AtomicUsize::new(0));
            let environment_calls = Arc::new(AtomicUsize::new(0));
            let acks = Arc::new(AtomicUsize::new(0));
            let mut notices: Vec<RouteCommitNotice> = Vec::new();
            let mut cold: Option<RouteCommitNotice> = None;
            let mut constructor = None;
            let mut assert_constructor = |owner: &mut OwnedResident, request: &AuthorityRequest| {
                let diagnostic_started = Instant::now();
                let identity = owner
                    .constructor_identity(&installed, request, fixture::until())
                    .unwrap();
                eprintln!(
                    "resident controller constructor diagnostic: tick={} elapsed_ms={}",
                    request.binding.tick,
                    diagnostic_started.elapsed().as_millis(),
                );
                assert_eq!(identity.2, 1);
                if let Some(prior) = constructor {
                    assert_eq!(identity, prior);
                } else {
                    constructor = Some(identity);
                }
            };
            let mut source_change = None;
            let mut dropped = false;
            let mut retried = false;
            let mut physics_refused = false;
            if mode.frame_cadenced() {
                cadence::install(&mut geometry.app);
            }
            std::thread::scope(|scope| {
                let worker_refusals = retained_deadline_refusals.clone();
                let worker_routes = routes.clone();
                let worker_calls = calls.clone();
                let worker_environment_calls = environment_calls.clone();
                let worker_acks = acks.clone();
                let worker = scope.spawn(move || {
                    let mut driver = controller_fixture
                        .open_recipe_initialized(world, authority)
                        .unwrap();
                    assert!(!driver.initial_events().unwrap().is_empty());
                    driver.attach_route_commit_sink(sink).unwrap();
                    let mut proofs = Vec::new();
                    let mut failures = 0;
                    let mut ack_losses = 0;
                    let mut physics_retries = 0;
                    let mut cold_position = None;
                    let mut warm_step = 0;
                    for _ in 1..=88 {
                        if worker_routes.load(Ordering::SeqCst) == 0 {
                            driver
                                .queue_profile_target_movement(driver.tick() + 1)
                                .unwrap();
                        } else {
                            let start = *cold_position.get_or_insert(driver.state().position);
                            warm_step += 1;
                            driver
                                .queue_movement(
                                    ai::Point::new(
                                        start.x.as_f32() - (warm_step as f32) * 0.125,
                                        start.y.as_f32(),
                                        start.z.as_f32(),
                                    )
                                    .unwrap(),
                                    0.5,
                                )
                                .unwrap();
                        }
                        let before = driver.checkpoint().identity();
                        let generation = driver.store_generation();
                        let ack_before = worker_acks.load(Ordering::SeqCst);
                        // Exercise the genuine store barrier, including the cold route tick.
                        let blocked =
                            checkpoint_failure && worker_routes.load(Ordering::SeqCst) == 0;
                        if blocked {
                            driver.block_checkpoint_for_result_transport(true);
                        }
                        let routes_before = worker_routes.load(Ordering::SeqCst);
                        let tick_started = Instant::now();
                        let mut out = driver.pump_one();
                        let tick_elapsed = tick_started.elapsed();
                        if let (Mode::MissedFrame, Some(pause)) = (mode, out.pause.as_ref()) {
                            assert_eq!(worker_refusals.load(Ordering::SeqCst), 1);
                            assert_eq!(pause.kind, PauseKind::Authority);
                            assert_eq!(out.ticks, 0);
                            assert!(out.events.is_empty());
                            assert!(out.saved.is_none());
                            assert_eq!(driver.checkpoint().identity(), before);
                            assert_eq!(driver.store_generation(), generation);
                            assert_eq!(worker_acks.load(Ordering::SeqCst), ack_before);
                            assert_eq!(worker_routes.load(Ordering::SeqCst), 1);
                            assert!(!driver.route_pending());
                            eprintln!(
                                "resident frame-cadenced missed-frame pump refused: tick={} wall_us={}",
                                driver.tick(),
                                tick_elapsed.as_micros(),
                            );
                            driver.assert_result_transport_refusal_readback();
                            return proofs;
                        }
                        if mode == Mode::PhysicsChangedWhenReady
                            && out
                                .pause
                                .as_ref()
                                .is_some_and(|pause| pause.kind == PauseKind::Authority)
                        {
                            physics_retries += 1;
                            assert_eq!(out.ticks, 0);
                            assert!(out.events.is_empty());
                            assert!(out.saved.is_none());
                            assert_eq!(driver.checkpoint().identity(), before);
                            assert_eq!(driver.store_generation(), generation);
                            assert_eq!(worker_acks.load(Ordering::SeqCst), ack_before);
                            assert!(!driver.route_pending());
                            let pending = driver.pending_inputs().unwrap().to_vec();
                            driver.retry_tick().unwrap();
                            assert_eq!(driver.pending_inputs().unwrap(), pending);
                            out = driver.pump_zero();
                        }
                        assert_eq!(
                            out.ticks,
                            1,
                            "controller tick={} pause={:?} pending_ticks={} routes={} callbacks={} ACKs={}",
                            driver.tick(),
                            out.pause,
                            out.pending_ticks,
                            worker_routes.load(Ordering::SeqCst),
                            worker_calls.load(Ordering::SeqCst),
                            worker_acks.load(Ordering::SeqCst),
                        );
                        let saved = if blocked {
                            failures += 1;
                            assert!(matches!(
                                out.pause.as_ref().map(|p| &p.kind),
                                Some(PauseKind::Checkpoint {
                                    requires_reopen: false
                                })
                            ));
                            assert!(out.events.is_empty());
                            assert!(driver.pending_inputs().is_none());
                            assert!(driver.route_pending());
                            assert_eq!(
                                worker_acks.load(Ordering::SeqCst),
                                ack_before,
                                "no notice before required durable save"
                            );
                            assert_eq!(driver.store_generation(), generation);
                            let committed = driver.checkpoint();
                            let callbacks = worker_calls.load(Ordering::SeqCst);
                            driver.block_checkpoint_for_result_transport(false);
                            let saved = driver.retry_checkpoint_for_result_transport().unwrap();
                            assert_eq!(driver.checkpoint(), committed);
                            out = driver.pump_zero();
                            assert_eq!(out.ticks, 0);
                            assert_eq!(
                                worker_calls.load(Ordering::SeqCst),
                                callbacks,
                                "checkpoint recovery never reruns authority"
                            );
                            saved
                        } else {
                            out.saved
                                .clone()
                                .expect("source environment requires durable publication")
                        };
                        if mode.loses_ack()
                            && out.pause.as_ref().is_some_and(|pause| {
                                pause.kind == PauseKind::CommittedAwaitingRouteAck
                            })
                        {
                            ack_losses += 1;
                            assert_eq!(
                                worker_routes.load(Ordering::SeqCst),
                                if mode == Mode::ChangedSourceAck { 1 } else { 2 }
                            );
                            assert!(driver.route_pending());
                            assert!(driver.pending_inputs().is_none());
                            assert!(
                                !out.events.is_empty(),
                                "durably published movement survives ACK loss"
                            );
                            let checkpoint = driver.checkpoint();
                            let callbacks = worker_calls.load(Ordering::SeqCst);
                            let committed_generation = driver.store_generation();
                            driver.retry_route_commit().unwrap();
                            assert_eq!(driver.checkpoint(), checkpoint);
                            assert_eq!(driver.store_generation(), committed_generation);
                            assert_eq!(
                                worker_calls.load(Ordering::SeqCst),
                                callbacks,
                                "ACK retry performs no new geometry/source observation"
                            );
                            assert!(driver.pause().is_none());
                        } else {
                            assert!(out.pause.is_none(), "{:?}", out.pause);
                        }
                        assert!(driver.records().is_empty());
                        assert!(!driver.route_pending());
                        if mode.default_budget()
                            && worker_routes.load(Ordering::SeqCst) > routes_before
                        {
                            assert_eq!(worker_acks.load(Ordering::SeqCst), ack_before + 1);
                            // Full pump includes checkpoint and commit ACK. Those have
                            // their own budgets: this is not a 500 ms whole-tick SLA.
                            eprintln!(
                                "resident default-budget controller tick saved and acknowledged: tick={} route={} elapsed_us={}",
                                driver.tick(),
                                if routes_before == 0 { "cold" } else { "retained" },
                                tick_elapsed.as_micros(),
                            );
                        }
                        assert_eq!(saved.tick, driver.tick());
                        assert_eq!(saved.generation, generation + 1);
                        proofs.push((before, driver.checkpoint().identity(), saved));
                        let flushed = driver.pump_zero();
                        assert_eq!(flushed.ticks, 0);
                        assert!(flushed.events.is_empty());
                        if mode == Mode::ChangedSourceAck
                            && worker_routes.load(Ordering::SeqCst) == 1
                        {
                            // Update expiry alone never queries terrain. A real accepted
                            // SetPosition heartbeat refreshes the now-due environment,
                            // including at the identical pose (no invented displacement).
                            assert_eq!(
                                driver
                                    .state()
                                    .player_environment()
                                    .unwrap()
                                    .environment()
                                    .timer_ms(),
                                0,
                            );
                            driver
                                .queue_movement(
                                    driver.state().position,
                                    driver.state().orientation.as_f32(),
                                )
                                .unwrap();
                            let checkpoint = driver.checkpoint();
                            let generation = driver.store_generation();
                            let ack_count = worker_acks.load(Ordering::SeqCst);
                            let callbacks = worker_calls.load(Ordering::SeqCst);
                            let environment_callbacks =
                                worker_environment_calls.load(Ordering::SeqCst);
                            let refused = driver.pump_one();
                            assert_eq!(refused.ticks, 0);
                            assert!(refused.events.is_empty());
                            assert!(refused.saved.is_none());
                            assert!(refused.pause.is_some());
                            assert!(driver.pending_inputs().is_some());
                            assert!(!driver.route_pending());
                            assert_eq!(driver.checkpoint(), checkpoint);
                            assert_eq!(driver.store_generation(), generation);
                            assert_eq!(worker_acks.load(Ordering::SeqCst), ack_count);
                            assert!(
                                worker_calls.load(Ordering::SeqCst) > callbacks,
                                "the next actual controller geometry Work was serviced and refused"
                            );
                            assert_eq!(
                                worker_environment_calls.load(Ordering::SeqCst),
                                environment_callbacks + 1,
                                "the same-pose input must request fresh PlayerEnvironment geometry"
                            );
                            break;
                        }
                        if worker_routes.load(Ordering::SeqCst) == 2 {
                            break;
                        }
                    }
                    assert_eq!(
                        worker_routes.load(Ordering::SeqCst),
                        if mode == Mode::ChangedSourceAck { 1 } else { 2 }
                    );
                    assert_eq!(checkpoint_failure, failures > 0);
                    assert_eq!(
                        physics_retries,
                        usize::from(mode == Mode::PhysicsChangedWhenReady)
                    );
                    assert_eq!(ack_losses, usize::from(mode.loses_ack()));
                    if mode == Mode::ChangedSourceAck {
                        driver.assert_result_transport_refusal_readback();
                    } else {
                        driver.assert_recipe_readback();
                    }
                    proofs
                });
                let deadline = Instant::now() + Duration::from_secs(120);
                let mut missed_frame = false;
                let mut previous_frame = None;
                let mut previous_fixed_steps = 0;
                let mut prepared_frame = None;
                let mut prepared_count = 0;
                let mut finished_count = 0;
                let mut max_owner_service = Duration::ZERO;
                while !worker.is_finished() {
                    assert!(
                        Instant::now() < deadline,
                        "actual controller transport stalled"
                    );
                    let allowance = if mode.frame_cadenced() {
                        if mode == Mode::MissedFrame
                            && !missed_frame
                            && selection.selected_retained_deadline_for_test().is_some()
                        {
                            // Witness the original queued lease, not a pre-enqueue signal.
                            let lease_deadline =
                                selection.selected_retained_deadline_for_test().unwrap();
                            assert!(Instant::now() < lease_deadline);
                            // No App updates or service while the original 500 ms lease expires.
                            let frame = geometry.app.world().resource::<FrameCount>().0;
                            assert!(cadence::take_permit(&mut geometry.app).is_none());
                            std::thread::sleep(
                                (lease_deadline + Duration::from_millis(100))
                                    .saturating_duration_since(Instant::now()),
                            );
                            // Keep the queued reply sender alive until the receiver has
                            // observed timeout: otherwise a delayed receiver may see a
                            // disconnected reply channel before its ready timeout.
                            while retained_deadline_refusals.load(Ordering::SeqCst) == 0 {
                                assert!(
                                    !worker.is_finished()
                                        || retained_deadline_refusals.load(Ordering::SeqCst) == 1,
                                    "retained receiver did not observe timeout"
                                );
                                assert!(Instant::now() < deadline, "retained receiver stalled");
                                std::thread::sleep(Duration::from_millis(1));
                            }
                            assert_eq!(geometry.app.world().resource::<FrameCount>().0, frame);
                            assert!(cadence::take_permit(&mut geometry.app).is_none());
                            missed_frame = true;
                        }
                        std::thread::sleep(Duration::from_millis(16));
                        let update_started = Instant::now();
                        geometry.app.update();
                        let update_elapsed = update_started.elapsed();
                        let Some((frame, allowance, fixed_steps)) =
                            cadence::take_permit(&mut geometry.app)
                        else {
                            continue;
                        };
                        assert_ne!(
                            previous_frame,
                            Some(frame),
                            "one service pass per render frame"
                        );
                        previous_frame = Some(frame);
                        assert!(fixed_steps > previous_fixed_steps);
                        previous_fixed_steps = fixed_steps;
                        assert!(cadence::take_permit(&mut geometry.app).is_none());
                        eprintln!(
                            "resident frame-cadenced app update: frame={frame} fixed_steps={fixed_steps} wall_us={}",
                            update_elapsed.as_micros(),
                        );
                        allowance
                    } else {
                        32
                    };
                    let service_started = Instant::now();
                    let controls = mailbox.serve_route_commits_test_drop_reply(allowance, |_, command| match command {
                        RouteCommitCommand::Initialize(boundary) => owner.initialize(boundary),
                        RouteCommitCommand::Commit(notice) => {
                            let duplicate = notices.last().is_some_and(|prior| prior == notice);
                            if mode.default_budget() && !duplicate {
                                // The exact committed request has passed authority finish and save.
                                // Keep test-only source/identity diagnostics outside its Route lease;
                                // the full controller pump and commit ACK still include this work.
                                for reply in notice.routes() {
                                    assert_constructor(&mut owner, &reply.request);
                                }
                            }
                            if mode == Mode::ChangedSourceAck && !notice.routes().is_empty() && !duplicate {
                                owner.inspect(&installed, fixture::until(), || false, |provider, _, ledger| {
                                    ledger.check_fresh_candidate(provider, &notice.routes()[0].request, fixture::until())
                                }).unwrap();
                                source_change = Some(AuthoredSourceChange::begin(install));
                            }
                            owner.acknowledge(notice)?;
                            if !notice.routes().is_empty() { owner.assert_committed_transport_refusal_for_test(); }
                            if duplicate {
                                assert!(mode.loses_ack());
                                assert!(!retried);
                                let prior = notices.last().unwrap();
                                assert_eq!(prior, notice, "retry preserves the exact committed notice");
                                assert!(prior.routes()[0].request.require_pathfinder_dispatch(&profile).unwrap().same_live_dispatch(notice.routes()[0].request.require_pathfinder_dispatch(&profile).unwrap()));
                                retried = true;
                                return Ok(());
                            }
                            if !notice.routes().is_empty() {
                                if let Some(cold_notice) = cold.as_ref() {
                                    assert_eq!(notice.routes().len(), 1);
                                    source_route_resident_retained::assert_request(
                                        &profile,
                                        cold_notice,
                                        &notice.routes()[0].request,
                                    );
                                    source_route_resident_retained::assert_facts(
                                        &notice.routes()[0].request,
                                        &notice.routes()[0].facts,
                                    );
                                } else {
                                    cold = Some(notice.clone());
                                }
                            }
                            notices.push(notice.clone());
                            acks.fetch_add(1, Ordering::SeqCst);
                            Ok(())
                        }
                    }, |command| {
                        let warm = matches!(command, RouteCommitCommand::Commit(notice) if notice.routes().first().is_some_and(|reply| matches!(&reply.request.query, Query::SourceMotion(MotionQuery::Route(route)) if route.request.prior_cache.is_some())));
                        let cold = matches!(command, RouteCommitCommand::Commit(notice) if !notice.routes().is_empty());
                        if !dropped && ((mode == Mode::LostAck && warm) || (mode == Mode::ChangedSourceAck && cold)) { dropped = true; true } else { false }
                    });
                    let queries = selection.serve_phased_results(
                        &mailbox,
                        &mut owner,
                        if mode.frame_cadenced() {
                            allowance - controls
                        } else {
                            32
                        },
                        fixture::until(),
                        |owner, job, phase| {
                            calls.fetch_add(1, Ordering::SeqCst);
                            let request = job.request().clone();
                            if mode.phased() {
                                let finishing = matches!(&phase, ResidentPhase::Prepared(_));
                                if !mode.frame_cadenced() {
                                    geometry.advance_physics();
                                }
                                let frame = geometry.app.world().resource::<FrameCount>().0;
                                if finishing {
                                    assert_ne!(
                                        prepared_frame,
                                        Some(frame),
                                        "final phase requires an intervening actual app update",
                                    );
                                }
                                let before = owner.diagnostic();
                                let started = Instant::now();
                                let ((result, freshness), trace) = freshness_timings::trace_sources(|| {
                                    freshness_timings::measure(|| {
                                        owner.certify_phase_result(
                                            phase, job, geometry.app.world_mut(), &installed,
                                        )
                                    })
                                });
                                if result.is_ok() && matches!(&request.query, Query::SourceMotion(MotionQuery::Route(route)) if route.request.prior_cache.is_some()) {
                                    assert_eq!(trace.len(), if finishing { 59 } else { 34 });
                                    retained_trace.extend(trace);
                                }
                                eprintln!(
                                    "resident phased certification: tick={} phase={} wall_us={} success={} freshness={freshness:?}",
                                    request.binding.tick,
                                    if finishing { "final" } else { "begin" },
                                    started.elapsed().as_micros(),
                                    result.is_ok(),
                                );
                                let result = result.map_err(|_| Failure::Binding)?;
                                match &result {
                                    ResidentPhaseResult::Prepared(_) => {
                                        assert_eq!(
                                            owner.diagnostic(), before,
                                            "preparation cannot stage or promote",
                                        );
                                        prepared_frame = Some(frame);
                                        prepared_count += 1;
                                    }
                                    ResidentPhaseResult::Complete(_) => {
                                        if finishing {
                                            finished_count += 1;
                                        }
                                        if !mode.default_budget() {
                                            assert_constructor(owner, &request);
                                        }
                                        routes.fetch_add(1, Ordering::SeqCst);
                                    }
                                }
                                return Ok(result);
                            }
                            let ResidentPhase::Observation(observation) = phase else {
                                panic!("baseline cannot prepare");
                            };
                            if mode == Mode::PhysicsChangedWhenReady && !physics_refused {
                                let observation = observation.map_err(|_| Failure::Binding)?;
                                let Query::SourceMotion(MotionQuery::Route(route)) = &request.query
                                else {
                                    unreachable!()
                                };
                                let a = route.request.origin;
                                let b = route.request.goal;
                                let middle = [
                                    (a.x.as_f32() + b.x.as_f32()) * 0.5,
                                    (a.y.as_f32() + b.y.as_f32()) * 0.5,
                                    a.z.as_f32() + 1.,
                                ];
                                // The worker is already Ready: mutate physics only now, before the live scene borrow.
                                let obstacle = geometry
                                    .app
                                    .world_mut()
                                    .spawn((
                                        avian3d::prelude::Collider::cuboid(2., 3., 0.25),
                                        avian3d::prelude::RigidBody::Static,
                                        avian3d::prelude::CollisionLayers::DEFAULT,
                                        bevy::prelude::Transform::from_translation(
                                            benilla_assets::coords::wow_to_bevy(middle),
                                        ),
                                    ))
                                    .id();
                                geometry.advance_physics();
                                let certification_started = Instant::now();
                                let (refused, freshness) = freshness_timings::measure(|| {
                                    owner.certify_observed_result(
                                        observation,
                                        job,
                                        geometry.app.world_mut(),
                                        &installed,
                                    )
                                });
                                eprintln!(
                                    "resident controller changed-physics certification: tick={} elapsed_ms={} success={} freshness={freshness:?}",
                                    request.binding.tick,
                                    certification_started.elapsed().as_millis(),
                                    refused.is_ok(),
                                );
                                geometry.app.world_mut().despawn(obstacle);
                                geometry.advance_physics();
                                assert!(matches!(
                                    refused,
                                    Err(ResidentFilterError::Context(
                                        Failure::UnsupportedNavigation
                                    ))
                                ));
                                physics_refused = true;
                                return Err(Failure::UnsupportedNavigation);
                            }
                            if !mode.frame_cadenced() {
                                geometry.advance_physics();
                            }
                            let certification_started = Instant::now();
                            let observation = observation.map_err(|_| Failure::Binding)?;
                            let ((result, freshness), trace) = freshness_timings::trace_sources(|| {
                                freshness_timings::measure(|| {
                                    owner.certify_observed_result(
                                        observation,
                                        job,
                                        geometry.app.world_mut(),
                                        &installed,
                                    )
                                })
                            });
                            if result.is_ok() && matches!(&request.query, Query::SourceMotion(MotionQuery::Route(route)) if route.request.prior_cache.is_some()) {
                                assert_eq!(trace.len(), 87);
                                retained_trace.extend(trace);
                            }
                            eprintln!(
                                "resident controller certification: tick={} elapsed_ms={} success={} freshness={freshness:?}",
                                request.binding.tick,
                                certification_started.elapsed().as_millis(),
                                result.is_ok(),
                            );
                            let result = result.map_err(|_| Failure::Binding)?;
                            if !mode.default_budget() {
                                assert_constructor(owner, &request);
                            }
                            routes.fetch_add(1, Ordering::SeqCst);
                            Ok(ResidentPhaseResult::Complete(Box::new(result)))
                        },
                        |job, deadline| {
                            calls.fetch_add(1, Ordering::SeqCst);
                            let request = match &job {
                                ServiceQuery::Immediate(job) => job.request(),
                                ServiceQuery::NearPoint(job) => job.request(),
                                ServiceQuery::Route(_) => {
                                    panic!("resident route bypassed selected result transport")
                                }
                            };
                            let facts = match &request.query {
                                Query::PlayerEnvironment(_) => {
                                    environment_calls.fetch_add(1, Ordering::SeqCst);
                                    ordinary
                                        .player_environment(player, deadline, || false)
                                        .map_err(|_| Failure::Binding)?
                                        .resolve(&session, request, deadline, || false)
                                        .map_err(|_| Failure::Binding)?
                                }
                                Query::SourceMotion(MotionQuery::NearPoint(_)) => ordinary
                                    .resolve_dry_primary_near_point(
                                        &session,
                                        request,
                                        player,
                                        compatibility,
                                        CONFIG,
                                        deadline,
                                        || false,
                                    )
                                    .map_err(|_| Failure::Binding)?,
                                _ => return Err(Failure::UnsupportedQuery),
                            };
                            Ok(ServiceStep::Complete(Box::new(facts)))
                        },
                    );
                    max_owner_service = max_owner_service.max(service_started.elapsed());
                    if mode.frame_cadenced() {
                        assert!(controls + queries <= allowance);
                        eprintln!(
                            "resident frame-cadenced owner service: frame={previous_frame:?} controls={controls} queries={queries} wall_us={}",
                            service_started.elapsed().as_micros(),
                        );
                    } else {
                        std::thread::yield_now();
                    }
                }
                eprintln!(
                    "resident owner-service maximum: phased={} frame_cadenced={} wall_us={}",
                    mode.phased(), mode.frame_cadenced(), max_owner_service.as_micros(),
                );
                measured_max_owner_service = max_owner_service;
                if mode.phased() {
                    assert_eq!((prepared_count, finished_count), (1, 1));
                }
                let proofs = worker.join().unwrap();
                assert_eq!(missed_frame, mode == Mode::MissedFrame);
                assert_eq!(
                    retained_deadline_refusals.load(Ordering::SeqCst),
                    usize::from(mode == Mode::MissedFrame)
                );
                assert_eq!(dropped, mode.loses_ack());
                assert_eq!(retried, mode.loses_ack());
                assert_eq!(physics_refused, mode == Mode::PhysicsChangedWhenReady);
                if mode == Mode::ChangedSourceAck {
                    assert!(source_change.is_some());
                    let entered = std::cell::Cell::new(false);
                    let fresh = owner.inspect(
                        &installed,
                        fixture::until(),
                        || false,
                        |provider, _, _| {
                            entered.set(true);
                            provider
                                .player_environment(player, fixture::until(), || false)
                                .map(|_| ())
                                .map_err(|error| match error {
                                    ResidentEnvironmentError::Context(error) => {
                                        ResidentFilterError::Context(error)
                                    }
                                    ResidentEnvironmentError::Source(error) => {
                                        ResidentFilterError::Source(error)
                                    }
                                })
                        },
                    );
                    assert!(
                        fresh.is_err(),
                        "changed source forbids a subsequent fresh geometry observation"
                    );
                    assert!(
                        !entered.get(),
                        "source guard fails before fresh provider use"
                    );
                }
                assert_eq!(proofs.len(), notices.len());
                for ((before, after, saved), notice) in proofs.iter().zip(&notices) {
                    assert_eq!(notice.previous_record(), before.history_tip);
                    assert_eq!(notice.before(), before.state_digest);
                    assert_eq!(notice.boundary().record(), after.history_tip);
                    assert_eq!(notice.boundary().state(), after.state_digest);
                    assert_eq!(notice.boundary().tick(), saved.tick);
                }
                assert_eq!(
                    notices
                        .iter()
                        .filter(|notice| !notice.routes().is_empty())
                        .count(),
                    if matches!(mode, Mode::MissedFrame | Mode::ChangedSourceAck) {
                        1
                    } else {
                        2
                    }
                );
            });
        },
    );
    RunEvidence {
        retained_trace,
        max_owner_service: measured_max_owner_service,
    }
}

#[test]
fn resident_result_transport_actual_controller_saves_cold_and_first_retained() {
    run(Mode::Accept);
}

#[test]
fn resident_result_transport_checkpoint_failure_precedes_ack_and_recovery_never_reruns() {
    run(Mode::CheckpointFailure);
}

#[test]
fn resident_result_transport_lost_ack_retries_exact_notice_without_reexecution() {
    run(Mode::LostAck);
}
#[test]
fn resident_result_transport_changed_source_allows_historical_ack_not_fresh_geometry() {
    run(Mode::ChangedSourceAck);
}

#[test]
fn resident_result_transport_worker_ready_rechecks_current_physics_before_reply() {
    run(Mode::PhysicsChangedWhenReady);
}

/// Opt-in latency evidence on authored component inputs, not real gameplay.
/// Refusal is a failure; no retry or proof-budget fallback is allowed here.
#[test]
#[ignore = "runner-sensitive default Route budget; run explicitly in release profile"]
fn resident_default_budget_controller_saves_cold_and_first_retained() {
    run(Mode::DefaultBudget);
}
