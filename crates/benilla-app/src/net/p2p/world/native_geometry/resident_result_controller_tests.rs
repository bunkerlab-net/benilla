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
use benilla_native_vmap_source::profiling as collision_profiling;
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
                        "resident controller Route authority returned: cold={} tick={} elapsed_us={} success={}",
                        matches!(&request.query, Query::SourceMotion(MotionQuery::Route(route)) if route.request.prior_cache.is_none()),
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
    ColdPhased,
    ColdPhasedDefaultBudget,
    ColdPhasedFrameCadenced,
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
            Self::FrameCadenced
                | Self::MissedFrame
                | Self::PhasedFrameCadenced
                | Self::ColdPhasedFrameCadenced
        )
    }
    fn default_budget(self) -> bool {
        matches!(
            self,
            Self::DefaultBudget | Self::PhasedDefaultBudget | Self::ColdPhasedDefaultBudget
        ) || self.frame_cadenced()
    }
    fn phased(self) -> bool {
        matches!(
            self,
            Self::Phased
                | Self::PhasedDefaultBudget
                | Self::PhasedFrameCadenced
                | Self::ColdPhased
                | Self::ColdPhasedDefaultBudget
                | Self::ColdPhasedFrameCadenced
        )
    }
    fn cold_phased(self) -> bool {
        matches!(
            self,
            Self::ColdPhased | Self::ColdPhasedDefaultBudget | Self::ColdPhasedFrameCadenced
        )
    }
    fn loses_ack(self) -> bool {
        matches!(self, Self::LostAck | Self::ChangedSourceAck)
    }
}
/// Four actual route owner turns only; all inner timing categories are inclusive.
#[derive(Debug)]
pub(super) struct ProfilePhaseEvidence {
    pub(super) cold: bool,
    pub(super) finishing: bool,
    pub(super) succeeded: bool,
    pub(super) callback: freshness_timings::Counts,
    pub(super) collision: collision_profiling::Counts,
    pub(super) outer: freshness_timings::Counts,
    pub(super) service_wall: Duration,
}

pub(super) struct RunEvidence {
    pub(super) cold_trace: freshness_timings::SourceTrace,
    pub(super) max_cold_owner_service: Duration,
    pub(super) cold_latency: Duration,
    pub(super) retained_trace: freshness_timings::SourceTrace,
    retained_final_trace: freshness_timings::SourceTrace,
    pub(super) max_owner_service: Duration,
    pub(super) profile_phases: Vec<ProfilePhaseEvidence>,
}
pub(super) fn run(mode: Mode) -> RunEvidence {
    run_inner(mode, None, &mut Vec::new(), false)
}
pub(super) fn run_profiled(
    profile: freshness_timings::Mode,
    phases_on_failure: &mut Vec<ProfilePhaseEvidence>,
) -> RunEvidence {
    run_inner(
        Mode::ColdPhasedFrameCadenced,
        Some(profile),
        phases_on_failure,
        false,
    )
}
fn run_inner(
    mode: Mode,
    profiling: Option<freshness_timings::Mode>,
    profile_phases: &mut Vec<ProfilePhaseEvidence>,
    capture_final_trace: bool,
) -> RunEvidence {
    let mut retained_trace = Vec::new();
    let mut retained_final_trace = Vec::new();
    let mut cold_trace = Vec::new();
    let mut max_cold_owner_service = Duration::ZERO;
    let mut cold_started = None;
    let mut cold_latency = Duration::ZERO;
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
                    let mut cold_service = false;
                    let mut service_phase = "none";
                    let mut service_tick = None;
                    let mut service_frame = None;
                    let mut profile_phase = None;
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
                    let (queries, outer_profile) = measure_optional(profiling, || {
                        selection.serve_phased_results(
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
                                let is_cold = matches!(
                                    &request.query,
                                    Query::SourceMotion(MotionQuery::Route(route))
                                        if route.request.prior_cache.is_none()
                                );
                                cold_service |= is_cold;
                                service_tick = Some(request.binding.tick);
                                if is_cold && cold_started.is_none() {
                                    cold_started = Some(Instant::now());
                                }
                                if mode.phased() {
                                    let finishing = matches!(&phase, ResidentPhase::Prepared(_));
                                    service_phase = if finishing {
                                        "final"
                                    } else if is_cold && !mode.cold_phased() {
                                        "sync"
                                    } else {
                                        "prepare"
                                    };
                                    if !mode.frame_cadenced() {
                                        geometry.advance_physics();
                                    }
                                    let frame = geometry.app.world().resource::<FrameCount>().0;
                                    service_frame = Some(frame);
                                    if finishing {
                                        assert_ne!(
                                            prepared_frame,
                                            Some(frame),
                                            "final phase requires an intervening actual app update",
                                        );
                                    }
                                    let before = owner.diagnostic();
                                    let started = Instant::now();
                                    let (((result, freshness), collision), trace) =
                                        freshness_timings::trace_sources(|| {
                                            measure_collision_optional(profiling, || {
                                                measure_callback(profiling, || {
                                                    if mode.cold_phased() {
                                                        owner.certify_phase_result(
                                                            phase,
                                                            job,
                                                            geometry.app.world_mut(),
                                                            &installed,
                                                        )
                                                    } else {
                                                        owner.certify_phase_result_retained_only(
                                                            phase,
                                                            job,
                                                            geometry.app.world_mut(),
                                                            &installed,
                                                        )
                                                    }
                                                })
                                            })
                                        });
                                    if let Some(collision) = collision {
                                        profile_phase = Some((
                                            is_cold,
                                            finishing,
                                            result.is_ok(),
                                            freshness,
                                            collision,
                                        ));
                                    }
                                    if result.is_ok() && matches!(&request.query, Query::SourceMotion(MotionQuery::Route(route)) if route.request.prior_cache.is_some()) {
                                        assert_eq!(trace.len(), if finishing { 59 } else { 34 });
                                        retained_trace.extend(trace.iter().copied());
                                        if capture_final_trace && finishing {
                                            assert!(retained_final_trace.is_empty());
                                            retained_final_trace.extend(trace.iter().copied());
                                        }
                                    }
                                    if result.is_ok() && is_cold {
                                        cold_trace.extend(trace.iter().copied());
                                    }
                                    if profiling.is_some() {
                                        // All observer modes defer Counts formatting until after
                                        // Work teardown, so log size does not depend on activation.
                                        eprintln!(
                                            "resident phased certification: cold={is_cold} tick={} phase={service_phase} wall_us={} success={} checks={} freshness=deferred",
                                            request.binding.tick,
                                            started.elapsed().as_micros(),
                                            result.is_ok(),
                                            trace.len(),
                                        );
                                    } else {
                                        eprintln!(
                                            "resident phased certification: cold={is_cold} tick={} phase={service_phase} wall_us={} success={} checks={} freshness={freshness:?}",
                                            request.binding.tick,
                                            started.elapsed().as_micros(),
                                            result.is_ok(),
                                            trace.len(),
                                            freshness = freshness.legacy_only(),
                                        );
                                    }
                                    let result = result.map_err(|_| Failure::Binding)?;
                                    match &result {
                                        ResidentPhaseResult::Prepared(_) => {
                                            if is_cold {
                                                assert!(
                                                    owner.diagnostic().same_semantic_state(&before),
                                                    "cold preparation cannot stage or promote",
                                                );
                                            } else {
                                                assert_eq!(
                                                    owner.diagnostic(), before,
                                                    "retained preparation cannot alter ledger state",
                                                );
                                            }
                                            prepared_frame = Some(frame);
                                            prepared_count += 1;
                                        }
                                        ResidentPhaseResult::Complete(_) => {
                                            if is_cold {
                                                cold_latency = cold_started.unwrap().elapsed();
                                            }
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
                        )
                    });
                    let service_elapsed = service_started.elapsed();
                    if let Some((cold, finishing, succeeded, callback, collision)) = profile_phase {
                        let outer = outer_profile.expect("profiled callback requires outer scope");
                        assert!(
                            profile_phases.len() < 4,
                            "one cold and one retained pair only"
                        );
                        profile_phases.push(ProfilePhaseEvidence {
                            cold,
                            finishing,
                            succeeded,
                            callback,
                            collision,
                            outer,
                            service_wall: service_elapsed,
                        });
                    }
                    max_owner_service = max_owner_service.max(service_elapsed);
                    if cold_service {
                        max_cold_owner_service = max_cold_owner_service.max(service_elapsed);
                    }
                    if queries > 0 {
                        eprintln!(
                            "resident phase owner service: cold={cold_service} phase={service_phase} tick={service_tick:?} frame={service_frame:?} wall_us={}",
                            service_elapsed.as_micros()
                        );
                    }
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
                    "resident owner-service maximum: phased={} cold_split={} frame_cadenced={} wall_us={}",
                    mode.phased(),
                    mode.cold_phased(),
                    mode.frame_cadenced(),
                    max_owner_service.as_micros(),
                );
                measured_max_owner_service = max_owner_service;
                if mode.phased() {
                    let count = if mode.cold_phased() { 2 } else { 1 };
                    assert_eq!((prepared_count, finished_count), (count, count));
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
        cold_trace,
        max_cold_owner_service,
        cold_latency,
        retained_trace,
        retained_final_trace,
        max_owner_service: measured_max_owner_service,
        profile_phases: std::mem::take(profile_phases),
    }
}

fn measure_callback<T>(
    mode: Option<freshness_timings::Mode>,
    run: impl FnOnce() -> T,
) -> (T, freshness_timings::Counts) {
    if let Some(mode) = mode {
        freshness_timings::measure_mode(mode, run)
    } else {
        freshness_timings::measure(run)
    }
}

fn measure_optional<T>(
    mode: Option<freshness_timings::Mode>,
    run: impl FnOnce() -> T,
) -> (T, Option<freshness_timings::Counts>) {
    if let Some(mode) = mode {
        let (result, counts) = freshness_timings::measure_mode(mode, run);
        (result, Some(counts))
    } else {
        (run(), None)
    }
}

fn measure_collision_optional<T>(
    mode: Option<freshness_timings::Mode>,
    run: impl FnOnce() -> T,
) -> (T, Option<collision_profiling::Counts>) {
    if let Some(mode) = mode {
        let mode = match mode {
            freshness_timings::Mode::Off => collision_profiling::Mode::Off,
            freshness_timings::Mode::CountOnly => collision_profiling::Mode::CountOnly,
            freshness_timings::Mode::Clocked => collision_profiling::Mode::Clocked,
        };
        let (result, counts) = collision_profiling::measure(mode, run);
        (result, Some(counts))
    } else {
        (run(), None)
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

#[derive(bevy::prelude::Resource, Default)]
struct F05ControlTurns(usize);

#[derive(bevy::ecs::schedule::ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
struct F05Control;

fn advance_f05_control(mut turns: bevy::prelude::ResMut<F05ControlTurns>) {
    turns.0 += 1;
}

/// Separate control World: no scene, resident, native, ledger or dispatch resource.
/// Completion wakes never run the scene App. This is not a rendered-frame proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum F05Fault {
    Clear,
    CancelHeld,
    CancelAfterStaging,
    CancelReady,
    SourceErrorThenCancelReady,
    ExpireHeld,
    QuitHeld,
    ColliderBirth,
    ForeignIndex,
    SourceChange,
}

fn run_f05_controller(fault: F05Fault) -> freshness_timings::SourceTrace {
    use super::source_route_filter::resident::owned::f05::ResidentF05Driver;

    let mut final_trace = Vec::new();
    fixture::with_selected_controller_fixture(
        fixture::ControllerOwners::Single,
        true,
        false,
        false,
        false,
        fixture::GeneratedSupport::Disabled,
        |install, height, compatibility, inventory, policy, profile, query, other| {
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
            let owner = OwnedResident::bind(
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
            // Pair against the genuine owner before transferring exclusive custody.
            let (mut authority, mailbox) = paired_test_mailbox(&owner).unwrap();
            let sink = authority.take_route_commit_sink().unwrap();
            let authority = ControllerAuthority {
                retained_deadline_refusals: Arc::new(AtomicUsize::new(0)),
                expect_retained_deadline: false,
                resident: authority,
                authored: Default::default(),
            };
            // Prepare foreign provenance before any original Work deadline starts.
            let mut foreign_index = (fault == F05Fault::ForeignIndex).then(|| {
                let mut foreign = InstalledFixture::new(install.path(), input::raw_inventory(install));
                foreign
                    .app
                    .world_mut()
                    .remove_resource::<NativeStaticMapIndex>()
                    .unwrap()
            });
            let (driver, gate) = if fault == F05Fault::Clear {
                (ResidentF05Driver::new(owner, &mailbox).unwrap(), None)
            } else {
                let (driver, entered, release) = if fault == F05Fault::CancelAfterStaging {
                    ResidentF05Driver::new_with_f05_gate_cancel_after_staging(owner, &mailbox).unwrap()
                } else {
                    ResidentF05Driver::new_with_f05_gate(owner, &mailbox).unwrap()
                };
                (driver, Some((entered, release)))
            };
            let wake = driver.completion_wake();
            let mut driver = Some(driver);
            let routes = Arc::new(AtomicUsize::new(0));
            let acks = Arc::new(AtomicUsize::new(0));
            let mut notices: Vec<RouteCommitNotice> = Vec::new();
            let mut acknowledged_ledger = None;
            let mut control = bevy::prelude::World::new();
            control.insert_resource(F05ControlTurns::default());
            let mut schedule = bevy::ecs::schedule::Schedule::new(F05Control);
            schedule.add_systems(advance_f05_control);
            control.add_schedule(schedule);
            let mut source_change = None;
            std::thread::scope(|scope| {
                let worker_routes = Arc::clone(&routes);
                let worker_acks = Arc::clone(&acks);
                let worker = scope.spawn(move || {
                    let mut controller = controller_fixture
                        .open_recipe_initialized(world, authority)
                        .unwrap();
                    assert!(!controller.initial_events().unwrap().is_empty());
                    controller.attach_route_commit_sink(sink).unwrap();
                    let mut proofs = Vec::new();
                    let mut cold_position = None;
                    let mut warm_step = 0;
                    for _ in 1..=88 {
                        if worker_routes.load(Ordering::SeqCst) == 0 {
                            controller
                                .queue_profile_target_movement(controller.tick() + 1)
                                .unwrap();
                        } else {
                            let start = *cold_position.get_or_insert(controller.state().position);
                            warm_step += 1;
                            controller
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
                        let before = controller.checkpoint().identity();
                        let generation = controller.store_generation();
                        let ack_before = worker_acks.load(Ordering::SeqCst);
                        let routes_before = worker_routes.load(Ordering::SeqCst);
                        let started = Instant::now();
                        let out = controller.pump_one();
                        eprintln!(
                            "F05 controller original-500ms pump: tick={} routes_before={} routes_after={} wall_us={} pause={:?}",
                            controller.tick(),
                            routes_before,
                            worker_routes.load(Ordering::SeqCst),
                            started.elapsed().as_micros(),
                            out.pause,
                        );
                        if out.pause.is_some() || out.ticks != 1 {
                            assert_eq!(controller.checkpoint().identity(), before);
                            assert_eq!(controller.store_generation(), generation);
                            assert_eq!(worker_acks.load(Ordering::SeqCst), ack_before);
                            assert!(out.saved.is_none());
                            assert!(out.events.is_empty());
                            controller.assert_result_transport_refusal_readback();
                            return Err(format!("original budget refusal: {:?}", out.pause));
                        }
                        let saved = out.saved.expect("environment requires durable publication");
                        assert_eq!(saved.tick, controller.tick());
                        assert_eq!(saved.generation, generation + 1);
                        assert!(!controller.route_pending());
                        assert!(controller.records().is_empty());
                        proofs.push((before, controller.checkpoint().identity(), saved));
                        let flushed = controller.pump_zero();
                        assert_eq!(flushed.ticks, 0);
                        assert!(flushed.events.is_empty());
                        if worker_routes.load(Ordering::SeqCst) == 2 {
                            controller.assert_recipe_readback();
                            return Ok(proofs);
                        }
                    }
                    Err("two actual route saves/ACKs were not reached".to_owned())
                });
                let outer_deadline = Instant::now() + Duration::from_secs(120);
                let mut begin_trace = None;
                let mut resume_trace = None;
                let mut begin_owner_wall = Duration::ZERO;
                let mut resume_owner_wall = Duration::ZERO;
                let mut pending_turns = 0;
                let mut fault_applied = false;
                let mut ledger_before_fault = None;
                let mut abandoned_counts = None;
                let mut completion_wakes = 0;
                let mut max_owner_service = Duration::ZERO;
                while !worker.is_finished() {
                    assert!(Instant::now() < outer_deadline, "F05 controller stalled");
                    control.run_schedule(F05Control);
                    let started = Instant::now();
                    if driver.is_some() {
                        mailbox.serve_route_commits_test_drop_reply(
                            32,
                            |_, command| match command {
                                RouteCommitCommand::Initialize(boundary) => {
                                    driver.as_mut().unwrap().initialize(boundary)
                                }
                                RouteCommitCommand::Commit(notice) => {
                                    driver.as_mut().unwrap().acknowledge(notice)?;
                                    acknowledged_ledger = Some((
                                        driver.as_ref().unwrap().diagnostic(),
                                        driver.as_ref().unwrap().ledger_counts(),
                                    ));
                                    if !notice.routes().is_empty() {
                                        assert_eq!(notice.routes().len(), 1);
                                        if let Some(cold) =
                                            notices.iter().find(|notice| !notice.routes().is_empty())
                                        {
                                            source_route_resident_retained::assert_request(
                                                &profile,
                                                cold,
                                                &notice.routes()[0].request,
                                            );
                                            source_route_resident_retained::assert_facts(
                                                &notice.routes()[0].request,
                                                &notice.routes()[0].facts,
                                            );
                                        }
                                        routes.fetch_add(1, Ordering::SeqCst);
                                    }
                                    notices.push(notice.clone());
                                    acks.fetch_add(1, Ordering::SeqCst);
                                    Ok(())
                                }
                            },
                            |_| false,
                        );
                        let before_counts = driver.as_ref().unwrap().f05_counts();
                        let (_, trace) = freshness_timings::trace_sources(|| {
                            driver.as_mut().unwrap().serve(
                                &mailbox,
                                &mut geometry,
                                1,
                                fixture::until(),
                                |job, deadline| {
                                    let request = match &job {
                                        ServiceQuery::Immediate(job) => job.request(),
                                        ServiceQuery::NearPoint(job) => job.request(),
                                        ServiceQuery::Route(_) => {
                                            panic!("F05 route bypassed private driver")
                                        }
                                    };
                                    let facts = match &request.query {
                                        Query::PlayerEnvironment(_) => ordinary
                                            .player_environment(player, deadline, || false)
                                            .map_err(|_| Failure::Binding)?
                                            .resolve(&session, request, deadline, || false)
                                            .map_err(|_| Failure::Binding)?,
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
                            )
                        });
                        let after_counts = driver.as_ref().unwrap().f05_counts();
                        if after_counts.0 != before_counts.0 {
                            assert_eq!((before_counts, after_counts), ((0, 0), (1, 0)));
                            begin_owner_wall = started.elapsed();
                            assert!(begin_trace.replace(trace.clone()).is_none());
                        }
                        if after_counts.1 != before_counts.1 {
                            assert_eq!((before_counts, after_counts), ((1, 0), (1, 1)));
                            resume_owner_wall = started.elapsed();
                            assert!(resume_trace.replace(trace).is_none());
                        }
                    }
                    max_owner_service = max_owner_service.max(started.elapsed());
                    if !fault_applied
                        && gate.as_ref().is_some_and(|(entered, _)| entered.try_recv().is_ok())
                    {
                        let active = driver.as_mut().unwrap();
                        assert!(active.pending());
                        assert_eq!(active.f05_reentry_jobs(), 0);
                        assert_eq!(routes.load(Ordering::SeqCst), 1);
                        let before = active.diagnostic();
                        let (acknowledged, acknowledged_counts) =
                            acknowledged_ledger.as_ref().unwrap();
                        assert!(before.same_semantic_state(acknowledged));
                        assert_eq!(active.ledger_counts(), *acknowledged_counts);
                        assert_eq!(active.ledger_counts().0, (1, 1));
                        ledger_before_fault = Some((before.clone(), active.ledger_counts()));
                        let ack_before = acks.load(Ordering::SeqCst);
                        let frame = geometry.app.world().resource::<FrameCount>().0;
                        let controls_before = control.resource::<F05ControlTurns>().0;
                        // Real control schedules run while the actual source task is held.
                        for _ in 0..3 {
                            control.run_schedule(F05Control);
                        }
                        assert_eq!(control.resource::<F05ControlTurns>().0, controls_before + 3);
                        assert_eq!(geometry.app.world().resource::<FrameCount>().0, frame);
                        let committed = notices.last().unwrap();
                        assert!(matches!(
                            active.initialize(committed.boundary()),
                            Err(Failure::QueueCapacity)
                        ));
                        assert!(matches!(
                            active.acknowledge(committed),
                            Err(Failure::QueueCapacity)
                        ));
                        match fault {
                            F05Fault::CancelHeld => active.cancel(),
                            F05Fault::CancelReady | F05Fault::CancelAfterStaging => {}
                            F05Fault::ExpireHeld => {
                                let original_deadline = active.pending_deadline().unwrap();
                                assert!(Instant::now() < original_deadline);
                                while Instant::now() < original_deadline {
                                    control.run_schedule(F05Control);
                                    let _ = wake.recv_timeout(
                                        original_deadline
                                            .saturating_duration_since(Instant::now())
                                            .min(Duration::from_millis(1)),
                                    );
                                }
                                assert!(active.pending());
                                assert_eq!(active.pending_deadline(), Some(original_deadline));
                            }
                            F05Fault::QuitHeld => {
                                abandoned_counts = Some(active.f05_counts());
                            }
                            F05Fault::ColliderBirth => {
                                let request = active.pending_request().unwrap();
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
                                geometry.app.world_mut().spawn((
                                    avian3d::prelude::Collider::cuboid(2., 3., 0.25),
                                    avian3d::prelude::RigidBody::Static,
                                    avian3d::prelude::CollisionLayers::DEFAULT,
                                    bevy::prelude::Transform::from_translation(
                                        benilla_assets::coords::wow_to_bevy(middle),
                                    ),
                                ));
                                // No sync here: the private final island must sync/reprove it.
                            }
                            F05Fault::ForeignIndex => {
                                geometry.app.insert_resource(foreign_index.take().unwrap());
                            }
                            F05Fault::SourceChange | F05Fault::SourceErrorThenCancelReady => {
                                source_change = Some(AuthoredSourceChange::begin(install));
                            }
                            F05Fault::Clear => unreachable!(),
                        }
                        assert!(active.diagnostic().same_semantic_state(&before));
                        assert_eq!(acks.load(Ordering::SeqCst), ack_before);
                        if matches!(fault, F05Fault::CancelReady | F05Fault::SourceErrorThenCancelReady) {
                            // No source result is drained or interpreted here. The worker sends
                            // this coalesced notification only after publishing its actual result.
                            while wake.try_recv().is_ok() {}
                            let original_deadline = active.pending_deadline().unwrap();
                            gate.as_ref().unwrap().1.try_send(()).unwrap();
                            loop {
                                assert!(
                                    Instant::now() < original_deadline,
                                    "ready-before-cancel missed original deadline"
                                );
                                control.run_schedule(F05Control);
                                if wake.recv_timeout(
                                    original_deadline
                                        .saturating_duration_since(Instant::now())
                                        .min(Duration::from_millis(1)),
                                ).is_ok() {
                                    break;
                                }
                            }
                            assert!(Instant::now() < original_deadline);
                            active.cancel();
                        } else {
                            if fault == F05Fault::QuitHeld {
                                drop(driver.take());
                            }
                            // No thread join or wait for source completion on the control path.
                            gate.as_ref().unwrap().1.try_send(()).unwrap();
                        }
                        fault_applied = true;
                    }
                    if driver.as_ref().is_some_and(|driver| driver.pending()) {
                        pending_turns += 1;
                    }
                    // Bounded control timer also admits queued mailbox/control work.
                    // Wake is notification only; serve drains and validates actual results.
                    if wake.recv_timeout(Duration::from_millis(1)).is_ok() {
                        completion_wakes += 1;
                    }
                }
                eprintln!(
                    "F05 nonframe control evidence: turns={} pending_turns={pending_turns} completion_wakes={completion_wakes} max_owner_service_us={} routes={} ACKs={}",
                    control.resource::<F05ControlTurns>().0,
                    max_owner_service.as_micros(),
                    routes.load(Ordering::SeqCst),
                    acks.load(Ordering::SeqCst),
                );
                eprintln!(
                    "F05 contiguous owner slices: begin_us={} resume_final_island_us={} publication_to_drain_latency=unmeasured",
                    begin_owner_wall.as_micros(),
                    resume_owner_wall.as_micros(),
                );
                // An already-retired receiver does not erase owed diagnostic unwind.
                while driver.as_ref().is_some_and(|driver| driver.pending()) {
                    assert!(Instant::now() < outer_deadline, "F05 tombstone did not drain");
                    control.run_schedule(F05Control);
                    driver.as_mut().unwrap().serve(
                        &mailbox,
                        &mut geometry,
                        1,
                        fixture::until(),
                        |_, _| Err(Failure::UnsupportedQuery),
                    );
                    let _ = wake.recv_timeout(Duration::from_millis(1));
                }
                let counts = driver
                    .as_ref()
                    .map(|driver| driver.f05_counts())
                    .or(abandoned_counts);
                eprintln!(
                    "F05 actual slot evidence: fault={fault:?} parks/drains={counts:?} worker_source_trace={:?} operation_outcome={:?}",
                    driver.as_ref().and_then(|driver| driver.f05_trace()),
                    driver.as_ref().map(|driver| driver.f05_outcome()),
                );
                let outcome = worker.join().unwrap();
                if fault != F05Fault::Clear {
                    assert!(
                        fault_applied,
                        "failed before reaching the intended held F05 fault"
                    );
                    assert!(outcome.is_err(), "actual fault must refuse without save/ACK");
                    let expected = match fault {
                        F05Fault::CancelHeld
                        | F05Fault::ExpireHeld
                        | F05Fault::CancelReady
                        | F05Fault::SourceErrorThenCancelReady => {
                            Some((false, Some("Source(Cancelled)")))
                        }
                        F05Fault::CancelAfterStaging => Some((true, Some("Source(Cancelled)"))),
                        F05Fault::ColliderBirth => Some((true, Some("Context(Obstacle)"))),
                        F05Fault::ForeignIndex => Some((true, Some("Context(Binding)"))),
                        F05Fault::SourceChange => Some((false, Some("Source(TerrainSource)"))),
                        F05Fault::QuitHeld => None,
                        F05Fault::Clear => unreachable!(),
                    };
                    assert_eq!(driver.as_ref().map(|driver| driver.f05_outcome()), expected);
                    let expected_raw = match fault {
                        F05Fault::CancelHeld | F05Fault::ExpireHeld => Some("Cancelled"),
                        F05Fault::SourceChange | F05Fault::SourceErrorThenCancelReady => {
                            Some("TerrainSource")
                        }
                        F05Fault::QuitHeld => None,
                        F05Fault::Clear
                        | F05Fault::CancelReady
                        | F05Fault::CancelAfterStaging
                        | F05Fault::ColliderBirth
                        | F05Fault::ForeignIndex => Some("Ok(())"),
                    };
                    assert_eq!(
                        driver.as_ref().and_then(|driver| driver.f05_raw_outcome()),
                        expected_raw
                    );
                    if let Some(driver) = &driver {
                        assert_eq!(
                            driver.f05_reentry_jobs(),
                            usize::from(matches!(
                                fault,
                                F05Fault::CancelAfterStaging
                                    | F05Fault::ColliderBirth
                                    | F05Fault::ForeignIndex
                            )),
                            "raw source errors and inactive success never construct a reentry job"
                        );
                        let (before, (counts_before, next_id_before)) =
                            ledger_before_fault.as_ref().unwrap();
                        assert!(driver.diagnostic().same_committed_root(before));
                        let (counts_after, next_id_after) = driver.ledger_counts();
                        assert_eq!(counts_after, *counts_before);
                        assert_eq!(
                            counts_after,
                            (1, 1),
                            "only original acknowledged cold remains"
                        );
                        assert_eq!(
                            next_id_after,
                            *next_id_before + u64::from(fault == F05Fault::CancelAfterStaging),
                            "only actual poststage cancellation consumes a fresh staging ID"
                        );
                    }
                    assert_eq!(routes.load(Ordering::SeqCst), 1);
                    assert_eq!(
                        counts,
                        Some((1, usize::from(fault != F05Fault::QuitHeld)))
                    );
                    return;
                }
                let proofs = outcome.unwrap();
                assert_eq!(driver.as_ref().unwrap().f05_raw_outcome(), Some("Ok(())"));
                assert_eq!(driver.as_ref().unwrap().f05_reentry_jobs(), 1);
                assert_eq!(counts, Some((1, 1)), "only first retained F05 parks");
                assert_eq!(
                    driver.as_ref().unwrap().f05_trace().unwrap().len(),
                    1,
                    "one actual worker source effect"
                );
                let begin = begin_trace.unwrap();
                let resume = resume_trace.unwrap();
                assert_eq!(begin.len(), 4, "original F01-F04 on BEGIN owner turn");
                assert_eq!(
                    resume.len(),
                    55,
                    "original F06-F59 plus labeled extra owner check"
                );
                final_trace.extend(begin);
                final_trace.extend(driver.as_ref().unwrap().f05_trace().unwrap().iter().copied());
                final_trace.extend(resume);
                assert_eq!(routes.load(Ordering::SeqCst), 2);
                assert!(pending_turns > 0);
                assert!(completion_wakes > 0);
                assert_eq!(proofs.len(), notices.len());
                for ((before, after, saved), notice) in proofs.iter().zip(&notices) {
                    assert_eq!(notice.previous_record(), before.history_tip);
                    assert_eq!(notice.before(), before.state_digest);
                    assert_eq!(notice.boundary().record(), after.history_tip);
                    assert_eq!(notice.boundary().state(), after.state_digest);
                    assert_eq!(notice.boundary().tick(), saved.tick);
                }
            });
            assert_eq!(
                source_change.is_some(),
                matches!(fault, F05Fault::SourceChange | F05Fault::SourceErrorThenCancelReady)
            );
            drop(source_change);
        },
    );
    final_trace
}

#[test]
#[ignore = "runner-sensitive original500ms paired F05 prototype; explicit release diagnostic"]
fn resident_f05_default_budget_actual_controller_comparison() {
    // One declared pair. Report both outcomes even if the first arm fails; no retry.
    // Additional trace storage is enabled only for this declared paired diagnostic.
    let baseline = std::panic::catch_unwind(|| {
        run_inner(Mode::ColdPhasedDefaultBudget, None, &mut Vec::new(), true)
    });
    let parked = std::panic::catch_unwind(|| run_f05_controller(F05Fault::Clear));
    eprintln!(
        "F05 original500ms paired outcome: baseline_cold_retained_split={} one_f05_slot={}",
        baseline.is_ok(),
        parked.is_ok(),
    );
    assert!(baseline.is_ok(), "baseline failed at the unchanged budget");
    assert!(parked.is_ok(), "F05 prototype failed at the unchanged budget");
    assert_f05_logical_trace(&baseline.unwrap().retained_final_trace, parked.unwrap());
}

#[test]
#[ignore = "mandatory release F05 fault gate; preserves the original500ms Route deadline"]
fn resident_f05_held_source_control_cancel_and_quit_never_ack() {
    for fault in [
        F05Fault::CancelHeld,
        F05Fault::ExpireHeld,
        F05Fault::CancelReady,
        F05Fault::SourceErrorThenCancelReady,
        F05Fault::QuitHeld,
    ] {
        run_f05_controller(fault);
    }
}

#[test]
#[ignore = "mandatory release F05 fault gate; preserves the original500ms Route deadline"]
fn resident_f05_current_scene_and_source_changes_refuse_before_save() {
    for fault in [
        F05Fault::CancelAfterStaging,
        F05Fault::ColliderBirth,
        F05Fault::ForeignIndex,
        F05Fault::SourceChange,
    ] {
        run_f05_controller(fault);
    }
}

fn assert_f05_logical_trace(
    baseline: &freshness_timings::SourceTrace,
    mut parked: freshness_timings::SourceTrace,
) {
    fn anchored(
        actual: &(&'static str, u32, u32),
        path: &str,
        source: &str,
        needle: &str,
        occurrence: usize,
    ) {
        let line = source
            .lines()
            .enumerate()
            .filter(|(_, line)| line.contains(needle))
            .nth(occurrence)
            .expect("audited source-call anchor must exist")
            .0
            + 1;
        assert!(actual.0.ends_with(path), "unexpected callsite: {actual:?}");
        assert_eq!(actual.1 as usize, line, "unexpected source-call anchor");
        assert!(actual.2 > 0);
    }
    // Exact authored environment-present branch only, not a universal G1 count.
    assert_eq!(baseline.len(), 59);
    assert_eq!(parked.len(), 60);
    const OWNED: &str = include_str!("products/source_height/owned.rs");
    const SCOPE: &str = include_str!("products/source_height/owned/f05_scope.rs");
    const ROOT: &str = include_str!("source_route_filter/retained.rs");
    anchored(
        &baseline[0],
        "products/source_height/owned.rs",
        OWNED,
        "height.verify_unchanged(&cancelled)?;",
        0,
    );
    anchored(
        &parked[0],
        "products/source_height/owned/f05_scope.rs",
        SCOPE,
        "height.verify_unchanged(||",
        0,
    );
    anchored(
        &baseline[4],
        "source_route_filter/retained.rs",
        ROOT,
        "let source = provider.height.verify_unchanged(cancelled);",
        0,
    );
    anchored(
        &parked[4],
        "products/source_height/owned.rs",
        OWNED,
        "self.view().verify_unchanged(cancelled)",
        0,
    );
    anchored(
        &baseline[58],
        "products/source_height/owned.rs",
        OWNED,
        "height.verify_unchanged(&cancelled)?;",
        1,
    );
    anchored(
        &parked[59],
        "products/source_height/owned.rs",
        OWNED,
        "self.view().verify_unchanged(cancelled)",
        0,
    );
    assert_eq!(parked[4], parked[59]);
    // The sole added current-owner source observation is after original F11.
    // It must be the exact inventory::fresh callsite already used by original F04.
    assert_eq!(parked[11], baseline[3]);
    eprintln!(
        "F05 explicitly added current-owner source occurrence after F11: {:?}",
        parked[11]
    );
    parked.remove(11);
    for slot in [0, 4, 58] {
        parked[slot] = baseline[slot];
    }
    assert_eq!(
        &parked,
        baseline,
        "every other original source occurrence stays ordered"
    );
}
