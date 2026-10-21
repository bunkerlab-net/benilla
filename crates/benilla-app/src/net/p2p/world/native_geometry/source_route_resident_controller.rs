// SPDX-License-Identifier: GPL-3.0-or-later
//! App-only actual-controller bridge. Source/native/ECS owners never cross threads.
//! This opens an initialized cache-free component world, not production Create,
//! persisted native-owner reopening, content admission or sustained movement.
use super::{
    core::{Failure, PlayerEnvironmentPolicy},
    products::source_height::ResidentInitializationConfiguration,
    real_corridor_fixture::installed_source::InstalledFixture,
    route_commit::{
        authored_conflicts as wrong, RouteCommitBoundary, RouteCommitNotice, RouteCommitSink,
    },
    source_creature_initialization::resident::ResidentCreatureInitializationProvider,
    source_pathfinder_query::resident_joined_tests as runtime,
    source_route_filter::resident::{ResidentFilterError, ResidentFilterProvider},
    source_route_resident_local::ledger::ResidentLedger,
    source_route_resident_retained as retained,
};
use crate::net::p2p::world::{resident_fixture, PauseKind};
use benilla_sim::{creature_ai as ai, creature_motion as motion};
use benilla_world_runtime::native::{
    self as n, Authority, AuthorityReply, AuthorityRequest, Facts, MotionFacts, MotionQuery,
    NativeError, Query,
};
use crossbeam_channel::{bounded, Receiver, Sender};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(120);
const CONFIG: Option<ResidentInitializationConfiguration> =
    Some(ResidentInitializationConfiguration::VmapHeightEnabled);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Accept,
    RejectBeforeApply,
    DropAfterApply,
    ExpireBeforeAck,
    SourceChangeBeforeAck,
    RollbackRetry,
    TwoOwners,
    RetainedDispatch,
    OneRetained(retained::Mode),
}
impl Mode {
    fn retained(self) -> Option<retained::Mode> {
        match self {
            Self::OneRetained(mode) => Some(mode),
            _ => None,
        }
    }
}
enum Message {
    Resolve(
        AuthorityRequest,
        Sender<Result<AuthorityReply, NativeError>>,
    ),
    Derived(Box<AuthorityReply>, Sender<Result<(), NativeError>>),
    Initialize(RouteCommitBoundary, Sender<Result<(), NativeError>>),
    Notice(RouteCommitNotice, Sender<Result<(), NativeError>>),
}
#[derive(Clone)]
struct Bridge(Sender<Message>);
impl Bridge {
    fn exchange<T>(
        &self,
        make: impl FnOnce(Sender<Result<T, NativeError>>) -> Message,
    ) -> Result<T, NativeError> {
        let (send, receive) = bounded(1);
        self.0
            .send_timeout(make(send), WAIT)
            .map_err(|_| NativeError::AuthorityUnavailable("resident owner pump disconnected"))?;
        receive
            .recv_timeout(WAIT)
            .map_err(|_| NativeError::AuthorityUnavailable("resident owner pump timed out"))?
    }
}
impl Authority for Bridge {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        self.exchange(|reply| Message::Resolve(request.clone(), reply))
    }
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        self.exchange(|answer| Message::Derived(Box::new(reply.clone()), answer))
    }
}
impl RouteCommitSink for Bridge {
    fn initialize(&mut self, boundary: &RouteCommitBoundary) -> Result<(), NativeError> {
        self.exchange(|reply| Message::Initialize(boundary.clone(), reply))
    }
    fn deliver(&mut self, notice: &RouteCommitNotice) -> Result<(), NativeError> {
        self.exchange(|reply| Message::Notice(notice.clone(), reply))
    }
}
struct Initializer<F> {
    source: F,
    authored: n::synthetic_motion_fixture::SyntheticMotionAuthority,
}
impl<F: FnMut(&AuthorityRequest) -> Result<Facts, NativeError>> Authority for Initializer<F> {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if matches!(request.query, Query::CreatureInitialization(_)) {
            return Ok(AuthorityReply {
                request: request.clone(),
                facts: (self.source)(request)?,
            });
        }
        assert!(
            !matches!(
                request.query,
                Query::SourceMotion(
                    MotionQuery::Route(_) | MotionQuery::NearPoint(_) | MotionQuery::Spatial(_)
                )
            ),
            "genesis cannot use authored spatial replies"
        );
        self.authored.resolve(request)
    }
}
fn binding_error<T>(result: Result<T, Failure>) -> Result<T, NativeError> {
    result.map_err(|_| NativeError::AuthorityBinding)
}
fn refuse_unchanged(ledger: &mut ResidentLedger, notice: &RouteCommitNotice) {
    let before = ledger.diagnostic();
    assert_eq!(ledger.acknowledge(notice), Err(Failure::Binding));
    assert_eq!(
        ledger.diagnostic(),
        before,
        "all owners/boundary/staged history remain atomic"
    );
}

fn run(mode: Mode) {
    runtime::with_selected_controller_fixture(
        mode == Mode::TwoOwners,
        mode.retained().is_some(),
        |install, height, compatibility, inventory, policy, p, query, other| {
            let fixture = resident_fixture::Fixture::new(p.clone(), [8; 16]);
            let player = fixture.player();
            assert_ne!(player, n::synthetic_fixture::player());
            let mut session = runtime::session(&p);
            session.world = fixture.world_id();
            session.player_environment =
                Some(PlayerEnvironmentPolicy::from_profile(&p, player).unwrap());
            let mut provider = ResidentFilterProvider::bind_with_policy(
                &session,
                &p,
                height,
                query,
                policy.clone(),
            )
            .unwrap();
            let mut other_provider =
                ResidentFilterProvider::bind_with_policy(&session, &p, height, other, policy)
                    .unwrap();
            let mut wrong_session = session.clone();
            wrong_session.player_environment = None;
            assert!(matches!(
                ResidentLedger::bind(
                    &wrong_session,
                    &p,
                    player,
                    &provider,
                    compatibility,
                    runtime::until()
                ),
                Err(ResidentFilterError::Context(Failure::Binding))
            ));
            let source = ResidentCreatureInitializationProvider::bind(
                session.world,
                &p,
                height,
                CONFIG,
                runtime::until(),
                || false,
            )
            .unwrap();
            let mut initializations = 0;
            let mut initializer = Initializer {
                source: |request: &AuthorityRequest| {
                    initializations += 1;
                    source
                        .resolve(request, runtime::until(), || false)
                        .map_err(|_| NativeError::AuthorityBinding)
                },
                authored: Default::default(),
            };
            let world = n::NativeWorld::synthetic_component_test_with_initialization(
                p.clone(),
                session.world,
                player,
                1234,
                None,
                &mut initializer,
            )
            .unwrap();
            drop(initializer);
            assert_eq!(initializations, p.npcs.len());
            for npc in world.state().npcs.values() {
                let capabilities = npc.creature_capabilities().unwrap().witness();
                assert_eq!(capabilities.inhabit_type(), 3);
                capabilities
                    .initialization()
                    .unwrap()
                    .request
                    .validate_for(&p)
                    .unwrap();
            }
            // A genuine different attachment of the same cache-free state is negative evidence only.
            let foreign = RouteCommitBoundary::capture_initial(&world).unwrap();
            let environment = provider
                .player_environment(player, runtime::until(), || false)
                .unwrap();
            let mut ledger = ResidentLedger::bind(
                &session,
                &p,
                player,
                &provider,
                compatibility,
                runtime::until(),
            )
            .unwrap();
            let mut geometry = InstalledFixture::new(install.path(), inventory);
            let mut visitors = runtime::TwoOwnerVisitors::default();
            let owner_count = p.npcs.len();
            if mode == Mode::RetainedDispatch || mode.retained().is_some() {
                assert_eq!(p.tick_ms, 25);
                assert_eq!(owner_count, 1);
            }
            let chase_destination =
                (mode == Mode::TwoOwners).then(|| runtime::two_owner_chase_destination(&p));
            let mut authored = n::synthetic_motion_fixture::SyntheticMotionAuthority::default();
            let callbacks = Arc::new(AtomicUsize::new(0));
            let routes = Arc::new(AtomicUsize::new(0));
            let (send, receive) = bounded(1);
            let mut captured: Option<RouteCommitNotice> = None;
            let mut attempts: Vec<(AuthorityRequest, Instant)> = vec![];
            let mut deliveries = 0;
            let mut fault_delivered = false;
            let mut initialized = false;
            let mut late_failure_armed = false;
            let mut late_failures = 0;
            let mut registry = None;
            let mut retained_witness = None;
            let mut source_change = None;
            let mut retained_notice: Option<RouteCommitNotice> = None;
            let mut retained_accepted = false;
            let mut retained_root = None;
            let worker_callbacks = callbacks.clone();
            let worker_routes = routes.clone();
            std::thread::scope(|scope| {
                let worker = scope.spawn(move || {
                    let bridge = Bridge(send);
                    let mut driver = fixture.open_initialized(world, bridge.clone()).unwrap();
                    let initial = driver.initial_events().unwrap();
                    assert!(!initial.is_empty());
                    driver.attach_route_commit_sink(bridge).unwrap();
                    for tick in 1..=24 {
                        if let Some(destination) = chase_destination {
                            if tick == runtime::TWO_OWNER_MOVE_TICK {
                                assert_eq!(worker_routes.load(Ordering::SeqCst), 0);
                                assert!(driver.state().npcs.values().all(|npc| npc.events.can_aggro()), "both original NPC notifications must actually complete before movement");
                                driver.queue_movement(destination, 0.5).unwrap();
                            }
                        } else {
                            driver.queue_profile_target_movement(tick).unwrap();
                        }
                        let before = driver.checkpoint();
                        let before_records = driver.records().to_vec();
                        let mut out = driver.pump_one();
                        if mode == Mode::RollbackRetry
                            && out
                                .pause
                                .as_ref()
                                .is_some_and(|p| p.kind == PauseKind::Authority)
                        {
                            assert_eq!(worker_routes.load(Ordering::SeqCst), 1);
                            assert_eq!(out.ticks, 0);
                            assert!(out.events.is_empty());
                            assert_eq!(driver.checkpoint(), before);
                            assert_eq!(driver.records(), before_records);
                            assert!(!driver.route_pending());
                            let inputs = driver.pending_inputs().unwrap().to_vec();
                            driver.retry_tick().unwrap();
                            assert_eq!(driver.pending_inputs().unwrap(), inputs);
                            out = driver.pump_zero();
                        }
                        assert_eq!(
                            driver.tick(),
                            tick,
                            "opened genuine component tick must commit"
                        );
                        assert_eq!(out.ticks, 1);
                        if worker_routes.load(Ordering::SeqCst) == 0 {
                            assert!(out.pause.is_none(), "{:?}", out.pause);
                            continue;
                        }
                        assert_eq!(
                            worker_routes.load(Ordering::SeqCst),
                            if mode == Mode::RollbackRetry {
                                2
                            } else {
                                owner_count
                            }
                        );
                        let checkpoint = driver.checkpoint();
                        let records = driver.records().to_vec();
                        let generation = driver.store_generation();
                        let count = worker_callbacks.load(Ordering::SeqCst);
                        assert!(
                            driver.pending_inputs().is_none(),
                            "committed inputs retired BEFORE delivery"
                        );
                        assert_eq!(driver.queued_actions(), 0);
                        if matches!(mode, Mode::RejectBeforeApply | Mode::DropAfterApply | Mode::SourceChangeBeforeAck) {
                            assert_eq!(
                                out.pause.as_ref().map(|p| &p.kind),
                                Some(&PauseKind::CommittedAwaitingRouteAck)
                            );
                            assert!(driver.route_pending());
                            // Even elapsed time cannot run the next tick while the ACK is pending.
                            let blocked = driver.pump_one();
                            assert_eq!(blocked.ticks, 0);
                            assert!(blocked.events.is_empty());
                            assert_eq!(driver.checkpoint(), checkpoint);
                            assert_eq!(driver.records(), records);
                            assert_eq!(worker_callbacks.load(Ordering::SeqCst), count);
                            driver.retry_route_commit().unwrap();
                        } else {
                            assert!(out.pause.is_none(), "{:?}", out.pause);
                        }
                        assert!(!driver.route_pending());
                        assert!(driver.pause().is_none());
                        assert!(driver.pending_inputs().is_none());
                        assert_eq!(driver.checkpoint(), checkpoint);
                        assert_eq!(driver.records(), records);
                        assert_eq!(driver.store_generation(), generation);
                        assert_eq!(
                            worker_callbacks.load(Ordering::SeqCst),
                            count,
                            "ACK retry never resolves/replays input"
                        );
                        let flushed = driver.pump_zero();
                        assert_eq!(flushed.ticks, 0);
                        assert!(
                            flushed.events.is_empty(),
                            "already published output is never duplicated"
                        );
                        assert_eq!(driver.tick(), tick);
                        // Projection/checkpoint may have published once BEFORE the delivery fault.
                        // Do not claim a dropped ACK suppresses committed movement events.
                        if mode == Mode::RetainedDispatch {
                            return (super::source_route_resident_continuation::after_cold_ack(
                                &mut driver,
                                &worker_callbacks,
                                &worker_routes,
                                tick,
                                out.events.len(),
                            ), None);
                        }
                        if let Some(retained_mode) = mode.retained() {
                            let (tick, events, count, committed) = retained::after_cold_ack(
                                &mut driver,
                                &worker_callbacks,
                                &worker_routes,
                                tick,
                                out.events.len(),
                                retained_mode,
                            );
                            if retained_mode == retained::Mode::SecondRetainedDispatch {
                                return (super::source_route_resident_repeated::after_warm_ack(
                                    &mut driver,
                                    &worker_callbacks,
                                    &worker_routes,
                                    tick,
                                    events,
                                ), Some(committed));
                            }
                            return ((tick, events, count), Some(committed));
                        }
                        return ((tick, out.events.len(), count), None);
                    }
                    panic!("actual controller never requested its genuine first cold Route");
                });
                // Dropping this receiver on any panic disconnects all bounded worker sends;
                // per-message replies also drop on unwind. Scoped join cannot wait forever.
                let receive: Receiver<Message> = receive;
                while !worker.is_finished() {
                    let message = match receive.recv_timeout(Duration::from_millis(100)) {
                        Ok(message) => message,
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                    };
                    match message {
                        Message::Initialize(boundary, reply) => {
                            assert!(!initialized);
                            assert_ne!(boundary.session(), foreign.session());
                            binding_error(ledger.initialize(&boundary)).unwrap();
                            initialized = true;
                            reply.send(Ok(())).unwrap();
                        }
                        Message::Resolve(request, reply) => {
                            callbacks.fetch_add(1, Ordering::SeqCst);
                            if late_failure_armed {
                                assert!(
                                    !matches!(
                                        request.query,
                                        Query::SourceMotion(MotionQuery::Route(_))
                                    ),
                                    "second Route cannot masquerade as later failure"
                                );
                                assert_eq!(
                                    request.binding.tick,
                                    attempts.last().unwrap().0.binding.tick
                                );
                                late_failure_armed = false;
                                late_failures += 1;
                                reply
                                    .send(Err(NativeError::AuthorityUnavailable(
                                        "resident controller deliberate later callback failure",
                                    )))
                                    .unwrap();
                                continue;
                            }
                            if mode.retained() == Some(retained::Mode::SecondRetainedDispatch)
                                && matches!(
                                    request.query,
                                    Query::SourceMotion(MotionQuery::Route(_))
                                )
                                && routes.load(Ordering::SeqCst) == 2
                            {
                                assert!(retained_accepted);
                                assert!(retained_witness.is_none());
                                let before = ledger.diagnostic();
                                let installed = geometry.app.world()
                                    .resource::<benilla_world::terrain_stream::NativeStaticMapIndex>();
                                let installed_owner = installed.source_provenance().unwrap();
                                ledger
                                    .check_second_retained_dispatch_for_test(
                                        &provider,
                                        installed_owner,
                                        &request,
                                    )
                                    .unwrap();
                                provider
                                    .verify_installed_owner(
                                        compatibility,
                                        installed_owner,
                                        runtime::until(),
                                        || false,
                                    )
                                    .unwrap();
                                assert_eq!(
                                    provider.retained_constructor_for_test(&request).unwrap(),
                                    registry.unwrap(),
                                    "third callback still owns the original native constructor"
                                );
                                assert!(matches!(
                                    ledger.certify_retained(
                                        geometry.app.world_mut(),
                                        compatibility,
                                        &mut provider,
                                        &request,
                                        401,
                                        runtime::until(),
                                        || false,
                                    ),
                                    Err(ResidentFilterError::Context(Failure::UnsupportedQuery))
                                ));
                                assert_eq!(ledger.diagnostic(), before);
                                let Query::SourceMotion(MotionQuery::Route(route)) = &request.query
                                else {
                                    unreachable!()
                                };
                                let cache = route.request.prior_cache.as_ref().unwrap();
                                assert_eq!(cache.revision, 2);
                                assert_eq!(cache.point_limit, motion::PATHFINDER_CACHE_POINT_LIMIT);
                                assert_eq!(cache.active_len, 1);
                                assert_eq!(cache.include_flags, 13);
                                assert_eq!(cache.exclude_flags, 0);
                                assert_eq!(cache.path_type, 1);
                                assert_eq!(
                                    Some(cache.as_ref()),
                                    retained_notice.as_ref().unwrap().boundary().heads()[0].cache()
                                );
                                eprintln!("resident actual second retained dispatch: tick={} query={} prior-revision={} storage={} active={} filter={} origin={:?} goal={:?}", request.binding.tick, route.request.binding.query_id, cache.revision, cache.poly_refs.len(), cache.active_len, cache.include_flags, route.request.origin, route.request.goal);
                                retained_witness = Some((request.clone(), before));
                                assert_eq!(routes.fetch_add(1, Ordering::SeqCst), 2);
                                reply
                                    .send(Err(NativeError::AuthorityUnavailable(
                                        super::source_route_resident_repeated::REFUSAL,
                                    )))
                                    .unwrap();
                                continue;
                            }
                            if mode == Mode::RetainedDispatch
                                && matches!(
                                    request.query,
                                    Query::SourceMotion(MotionQuery::Route(_))
                                )
                                && routes.load(Ordering::SeqCst) == 1
                            {
                                assert!(retained_witness.is_none());
                                let before = ledger.diagnostic();
                                ledger
                                    .check_retained_dispatch_for_test(&provider, &request)
                                    .unwrap();
                                provider
                                    .verify_installed_owner(
                                        compatibility,
                                        compatibility.owner(),
                                        runtime::until(),
                                        || false,
                                    )
                                    .unwrap();
                                assert_eq!(
                                    provider.retained_constructor_for_test(&request).unwrap(),
                                    registry.unwrap(),
                                    "retained callback must re-enter the original native allocation"
                                );
                                assert!(matches!(
                                    provider.observe(&request, 401, runtime::until(), || false),
                                    Err(ResidentFilterError::Context(Failure::Binding))
                                ));
                                assert!(matches!(
                                    provider.require_one_retained_route(),
                                    Err(ResidentFilterError::Context(Failure::UnsupportedQuery))
                                ));
                                assert_eq!(
                                    ledger.require_open_epoch(),
                                    Err(Failure::UnsupportedQuery)
                                );
                                assert_eq!(ledger.diagnostic(), before);
                                let Query::SourceMotion(MotionQuery::Route(route)) = &request.query
                                else {
                                    unreachable!()
                                };
                                let cache = route.request.prior_cache.as_ref().unwrap();
                                assert_eq!(cache.revision, 1);
                                assert_eq!(cache.point_limit, motion::PATHFINDER_CACHE_POINT_LIMIT);
                                assert_eq!(cache.poly_refs.len(), 75);
                                assert_eq!(cache.active_len, 1);
                                assert_eq!(cache.include_flags, 13);
                                assert_eq!(cache.exclude_flags, 0);
                                assert_eq!(cache.path_type, 1);
                                eprintln!("resident actual retained dispatch: tick={} query={} prior-revision={} storage={} active={} filter={} origin={:?} goal={:?}", request.binding.tick, route.request.binding.query_id, cache.revision, cache.poly_refs.len(), cache.active_len, cache.include_flags, route.request.origin, route.request.goal);
                                retained_witness = Some((request.clone(), before));
                                assert_eq!(routes.fetch_add(1, Ordering::SeqCst), 1);
                                reply
                                    .send(Err(NativeError::AuthorityUnavailable(
                                        super::source_route_resident_continuation::REFUSAL,
                                    )))
                                    .unwrap();
                                continue;
                            }
                            if let Some(retained_mode) = mode.retained() {
                                if matches!(
                                    request.query,
                                    Query::SourceMotion(MotionQuery::Route(_))
                                ) && routes.load(Ordering::SeqCst) >= 1
                                {
                                    let attempt = routes.fetch_add(1, Ordering::SeqCst);
                                    assert!(
                                        attempt == 1
                                            || retained_mode == retained::Mode::RollbackRetry && attempt == 2,
                                        "only one retained calculation, or its identical-input retry, is supported"
                                    );
                                    assert_eq!(
                                        ledger.require_open_epoch(),
                                        Err(Failure::UnsupportedQuery)
                                    );
                                    retained::assert_request(
                                        &p,
                                        captured.as_ref().unwrap(),
                                        &request,
                                    );
                                    assert_eq!(
                                        provider.retained_constructor_for_test(&request).unwrap(),
                                        registry.unwrap(),
                                        "warm calculation retains the exact cold native allocation"
                                    );
                                    if attempt == 1 {
                                        assert_eq!(ledger.counts(), (1, 1));
                                        retained_root = Some(ledger.diagnostic());
                                    } else {
                                        let (failed, failed_deadline) = &attempts[1];
                                        assert_eq!(
                                            serde_json::to_vec(failed).unwrap(),
                                            serde_json::to_vec(&request).unwrap()
                                        );
                                        assert!(!failed
                                            .require_pathfinder_dispatch(&p)
                                            .unwrap()
                                            .same_live_dispatch(
                                                request.require_pathfinder_dispatch(&p).unwrap()
                                            ));
                                        assert_eq!(late_failures, 1);
                                        assert_eq!(ledger.counts(), (2, 1));
                                        assert!(ledger
                                            .diagnostic()
                                            .same_committed_root(retained_root.as_ref().unwrap()));
                                        ledger
                                            .check_fresh_candidate(
                                                &provider,
                                                failed,
                                                *failed_deadline,
                                            )
                                            .unwrap();
                                        assert!(matches!(
                                            ledger.check_fresh_candidate(&provider, &request, *failed_deadline),
                                            Err(ResidentFilterError::Context(Failure::Binding))
                                        ), "old warm dependency cannot answer the equal-byte new live dispatch");
                                    }
                                    if retained_mode == retained::Mode::Accept && attempt == 1 {
                                        retained::assert_source_order(
                                            &ledger,
                                            &mut provider,
                                            &request,
                                        );
                                    }
                                    if retained_mode == retained::Mode::Refusals && attempt == 1 {
                                        super::source_route_resident_retained_refusals::probe(
                                            &mut ledger,
                                            &mut geometry,
                                            install,
                                            height,
                                            compatibility,
                                            &mut provider,
                                            &request,
                                        );
                                        assert!(Instant::now() >= attempts[0].1);
                                        assert!(matches!(
                                            ledger.check_fresh_candidate(
                                                &provider, &attempts[0].0, runtime::until(),
                                            ),
                                            Err(ResidentFilterError::Context(Failure::Deadline))
                                        ), "expired cold point/LOS evidence stays expired; only native/filter ancestry survives");
                                    }
                                    let deadline = Instant::now() + Duration::from_secs(30);
                                    // New physics observation and source sequence for every attempt;
                                    // neither the cold clearance nor the failed warm reply is reused.
                                    geometry.advance_physics();
                                    let facts = ledger
                                        .certify_retained(
                                            geometry.app.world_mut(),
                                            compatibility,
                                            &mut provider,
                                            &request,
                                            201,
                                            deadline,
                                            || false,
                                        )
                                        .unwrap();
                                    retained::assert_facts(&request, &facts);
                                    let staged = ledger.diagnostic();
                                    assert!(
                                        matches!(
                                            ledger.certify_retained(
                                                geometry.app.world_mut(),
                                                compatibility,
                                                &mut provider,
                                                &request,
                                                201,
                                                deadline,
                                                || false,
                                            ),
                                            Err(ResidentFilterError::Context(Failure::Binding))
                                        ),
                                        "the same live warm dispatch cannot stage twice"
                                    );
                                    assert_eq!(ledger.diagnostic(), staged);
                                    assert_eq!(ledger.counts(), (attempt + 1, 1));
                                    assert!(ledger
                                        .diagnostic()
                                        .same_committed_root(retained_root.as_ref().unwrap()));
                                    ledger
                                        .check_fresh_candidate(&provider, &request, deadline)
                                        .unwrap();
                                    assert_eq!(
                                        provider.retained_constructor_for_test(&request).unwrap(),
                                        registry.unwrap()
                                    );
                                    attempts.push((request.clone(), deadline));
                                    if retained_mode == retained::Mode::RollbackRetry
                                        && attempt == 1
                                    {
                                        late_failure_armed = true;
                                    }
                                    reply.send(Ok(AuthorityReply { request, facts })).unwrap();
                                    continue;
                                }
                            }
                            let facts = match &request.query {
                                Query::PlayerEnvironment(_) => environment
                                    .resolve(&session, &request, runtime::until(), || false)
                                    .unwrap(),
                                Query::SourceMotion(MotionQuery::NearPoint(_)) => provider
                                    .resolve_dry_primary_near_point(
                                        &session,
                                        &request,
                                        player,
                                        compatibility,
                                        CONFIG,
                                        runtime::until(),
                                        || false,
                                    )
                                    .unwrap(),
                                Query::SourceMotion(MotionQuery::Route(route)) => {
                                    assert!(initialized);
                                    let attempt = routes.fetch_add(1, Ordering::SeqCst);
                                    assert!(
                                        attempt < owner_count
                                            || mode == Mode::RollbackRetry && attempt == 1,
                                        "later dispatch remains closed"
                                    );
                                    if mode == Mode::RollbackRetry && attempt == 1 {
                                        let (old, deadline) = &attempts[0];
                                        assert_eq!(
                                            serde_json::to_vec(old).unwrap(),
                                            serde_json::to_vec(&request).unwrap()
                                        );
                                        assert!(!old
                                            .require_pathfinder_dispatch(&p)
                                            .unwrap()
                                            .same_live_dispatch(
                                                request.require_pathfinder_dispatch(&p).unwrap()
                                            ));
                                        ledger
                                            .check_fresh_candidate(&provider, old, *deadline)
                                            .unwrap();
                                        assert_eq!(ledger.counts(), (1, 0));
                                        assert_eq!(
                                            provider
                                                .retained_constructor_for_test(&request)
                                                .unwrap(),
                                            registry.unwrap()
                                        );
                                    }
                                    assert!(request
                                        .require_pathfinder_dispatch(&p)
                                        .unwrap()
                                        .first_construction());
                                    assert!(route.request.prior_cache.is_none());
                                    ledger.require_open_epoch().unwrap();
                                    let deadline = Instant::now() + Duration::from_secs(30);
                                    let (constructor, water) = runtime::prerequisites_for_player(
                                        &mut provider,
                                        &request,
                                        player,
                                        101,
                                        deadline,
                                    );
                                    if attempt == 0 {
                                        let before = ledger.diagnostic();
                                        // Fixed fixture identity is refusal-only evidence; the
                                        // positive chain above uses the real profile selection.
                                        assert!(matches!(
                                            provider.observe_chase_los(
                                                &constructor,
                                                &request,
                                                n::synthetic_fixture::player(),
                                                103,
                                                deadline,
                                                || false,
                                            ),
                                            Err(ResidentFilterError::Context(Failure::Binding))
                                        ));
                                        assert_eq!(ledger.diagnostic(), before);
                                        assert!(matches!(
                                            ledger.certify(
                                                geometry.app.world_mut(),
                                                compatibility,
                                                &mut other_provider,
                                                &request,
                                                constructor,
                                                water,
                                                105,
                                                deadline,
                                                || false
                                            ),
                                            Err(ResidentFilterError::Context(Failure::Binding))
                                        ));
                                        assert_eq!(ledger.diagnostic(), before);
                                    } else {
                                        drop((constructor, water));
                                    }
                                    let (constructor, water) = runtime::prerequisites_for_player(
                                        &mut provider,
                                        &request,
                                        player,
                                        101,
                                        deadline,
                                    );
                                    geometry.advance_physics();
                                    let facts = ledger
                                        .certify(
                                            geometry.app.world_mut(),
                                            compatibility,
                                            &mut provider,
                                            &request,
                                            constructor,
                                            water,
                                            105,
                                            deadline,
                                            || false,
                                        )
                                        .unwrap();
                                    assert_eq!(
                                        ledger.counts(),
                                        (attempt + 1, 0),
                                        "resolve only stages, never commits"
                                    );
                                    ledger
                                        .check_fresh_candidate(&provider, &request, deadline)
                                        .unwrap();
                                    // Refusal-only metadata probes. Neither a repeated live
                                    // dispatch nor a fabricated changed request can emit again.
                                    for changed in [false, true] {
                                        let (constructor, water) =
                                            runtime::prerequisites_for_player(
                                                &mut provider,
                                                &request,
                                                player,
                                                101,
                                                deadline,
                                            );
                                        let mut unsupported = request.clone();
                                        if changed {
                                            let Query::SourceMotion(MotionQuery::Route(route)) =
                                                &mut unsupported.query
                                            else {
                                                unreachable!()
                                            };
                                            route.request.binding.query_id += 1;
                                        }
                                        let before = ledger.diagnostic();
                                        let result = ledger.certify(
                                            geometry.app.world_mut(),
                                            compatibility,
                                            &mut provider,
                                            &unsupported,
                                            constructor,
                                            water,
                                            105,
                                            deadline,
                                            || false,
                                        );
                                        let expected = if changed {
                                            Failure::UnsupportedQuery
                                        } else {
                                            Failure::Binding
                                        };
                                        assert!(
                                            matches!(result, Err(ResidentFilterError::Context(error)) if error == expected)
                                        );
                                        assert_eq!(
                                            ledger.diagnostic(),
                                            before,
                                            "no candidate/Facts emitted by refused dispatch"
                                        );
                                    }
                                    registry = Some(
                                        provider.retained_constructor_for_test(&request).unwrap(),
                                    );
                                    attempts.push((request.clone(), deadline));
                                    if mode == Mode::RollbackRetry && attempt == 0 {
                                        late_failure_armed = true;
                                    }
                                    facts
                                }
                                Query::SourceMotion(MotionQuery::Spatial(
                                    motion::Query::Bearing(bearing),
                                )) => {
                                    let angle = benilla_sim::facing::bearing_bits(
                                        [bearing.origin.x.bits(), bearing.origin.y.bits()],
                                        [bearing.toward.x.bits(), bearing.toward.y.bits()],
                                    )
                                    .unwrap();
                                    Facts::SourceMotion(MotionFacts::Spatial(Box::new(
                                        motion::Reply {
                                            request: motion::Query::Bearing(bearing.clone()),
                                            fact: motion::Fact::Bearing(
                                                ai::Real::from_bits(angle).unwrap(),
                                            ),
                                        },
                                    )))
                                }
                                Query::SourceMotion(MotionQuery::Spatial(_)) => {
                                    panic!("unsupported resident spatial callback")
                                }
                                Query::CreatureInitialization(_) => panic!(
                                    "Open must restore genuine initialization, not reinitialize"
                                ),
                                _ => {
                                    if mode == Mode::TwoOwners {
                                        visitors.resolve(&request).unwrap().unwrap_or_else(|| {
                                            authored.resolve(&request).unwrap().facts
                                        })
                                    } else {
                                        authored.resolve(&request).unwrap().facts
                                    }
                                }
                            };
                            reply.send(Ok(AuthorityReply { request, facts })).unwrap();
                        }
                        Message::Derived(derived, reply) => {
                            callbacks.fetch_add(1, Ordering::SeqCst);
                            if late_failure_armed {
                                assert!(!matches!(
                                    derived.request.query,
                                    Query::SourceMotion(MotionQuery::Route(_))
                                ));
                                assert_eq!(
                                    derived.request.binding.tick,
                                    attempts.last().unwrap().0.binding.tick
                                );
                                late_failure_armed = false;
                                late_failures += 1;
                                reply.send(Err(NativeError::AuthorityUnavailable("resident controller deliberate later derived callback failure"))).unwrap();
                                continue;
                            }
                            authored.observe_derived(&derived).unwrap();
                            reply.send(Ok(())).unwrap();
                        }
                        Message::Notice(notice, reply) => {
                            if notice.routes().is_empty() {
                                assert!(
                                    routes.load(Ordering::SeqCst) == 0
                                        || (mode == Mode::RetainedDispatch
                                            || mode.retained().is_some())
                                            && routes.load(Ordering::SeqCst) == 1
                                            && ledger.counts() == (1, 1)
                                        || mode.retained()
                                            == Some(retained::Mode::SecondRetainedDispatch)
                                            && routes.load(Ordering::SeqCst) == 2
                                            && ledger.counts() == (1, 1)
                                );
                                binding_error(ledger.acknowledge(&notice)).unwrap();
                                reply.send(Ok(())).unwrap();
                                continue;
                            }
                            if let Some(retained_mode) = mode.retained() {
                                if matches!(
                                    &notice.routes()[0].request.query,
                                    Query::SourceMotion(MotionQuery::Route(route)) if route.request.prior_cache.is_some()
                                ) {
                                    deliveries += 1;
                                    assert_eq!(notice.routes().len(), 1);
                                    assert_eq!(notice.boundary().heads().len(), 1);
                                    assert_eq!(
                                        notice.boundary().heads()[0].cache().unwrap().revision,
                                        2
                                    );
                                    let actual = &notice.routes()[0].request;
                                    let latest = &attempts.last().unwrap().0;
                                    assert_eq!(actual, latest);
                                    assert!(actual
                                        .require_pathfinder_dispatch(&p)
                                        .unwrap()
                                        .same_live_dispatch(
                                            latest.require_pathfinder_dispatch(&p).unwrap()
                                        ));
                                    retained::assert_request(
                                        &p,
                                        captured.as_ref().unwrap(),
                                        actual,
                                    );
                                    retained::assert_facts(actual, &notice.routes()[0].facts);
                                    if let Some(old) = &retained_notice {
                                        assert_eq!(&notice, old);
                                        assert!(actual
                                            .require_pathfinder_dispatch(&p)
                                            .unwrap()
                                            .same_live_dispatch(
                                                old.routes()[0]
                                                    .request
                                                    .require_pathfinder_dispatch(&p)
                                                    .unwrap()
                                            ));
                                    } else {
                                        retained_notice = Some(notice.clone());
                                    }
                                    let prior = ledger.diagnostic();
                                    if !retained_accepted {
                                        assert_eq!(
                                            ledger.counts(),
                                            (
                                                if retained_mode == retained::Mode::RollbackRetry {
                                                    3
                                                } else {
                                                    2
                                                },
                                                1
                                            )
                                        );
                                        assert!(prior
                                            .same_committed_root(retained_root.as_ref().unwrap()));
                                    }
                                    for wrong in [
                                        wrong::foreign_session(&notice, &foreign),
                                        wrong::skipped_tick(&notice),
                                        wrong::wrong_current_cache(&notice),
                                        wrong::without_routes(&notice),
                                        wrong::duplicate_route(&notice),
                                        wrong::stale_generation(&notice),
                                        wrong::omitted_head(&notice),
                                        wrong::extra_head(&notice),
                                        wrong::decoded_dispatch(&notice),
                                        wrong::uncertified_reply(&notice),
                                    ] {
                                        refuse_unchanged(&mut ledger, &wrong);
                                    }
                                    let cold_notice = captured.as_ref().unwrap();
                                    if !retained_accepted
                                        && cold_notice.boundary().tick().checked_add(1)
                                            == Some(actual.binding.tick)
                                    {
                                        // An immediately following replan leaves the exact
                                        // cold notice current, so its duplicate is still valid.
                                        let before = ledger.diagnostic();
                                        ledger.acknowledge(cold_notice).unwrap();
                                        assert_eq!(ledger.diagnostic(), before);
                                    } else {
                                        refuse_unchanged(&mut ledger, cold_notice);
                                    }
                                    if retained_mode == retained::Mode::RollbackRetry {
                                        assert_eq!(late_failures, 1);
                                        refuse_unchanged(
                                            &mut ledger,
                                            &wrong::substitute_dispatch(&notice, &attempts[1].0),
                                        );
                                    }
                                    if retained_mode == retained::Mode::SourceChangeBeforeAck
                                        && source_change.is_none()
                                    {
                                        assert!(!retained_accepted);
                                        let (context, deadline) = attempts.last().unwrap();
                                        source_change = Some(
                                            super::source_route_resident_history::SourceChange::begin(
                                                install, &ledger, &provider, context, *deadline,
                                            ),
                                        );
                                        assert_eq!(ledger.diagnostic(), prior);
                                    }
                                    ledger.acknowledge(&notice).unwrap();
                                    assert_eq!(ledger.counts(), (1, 1));
                                    if retained_accepted {
                                        assert_eq!(
                                            ledger.diagnostic(),
                                            prior,
                                            "exact warm ACK retry is historical and idempotent"
                                        );
                                    }
                                    let accepted = ledger.diagnostic();
                                    ledger.acknowledge(&notice).unwrap();
                                    assert_eq!(ledger.diagnostic(), accepted);
                                    refuse_unchanged(
                                        &mut ledger,
                                        &wrong::decoded_dispatch(&notice),
                                    );
                                    refuse_unchanged(&mut ledger, captured.as_ref().unwrap());
                                    if retained_mode == retained::Mode::RollbackRetry {
                                        refuse_unchanged(
                                            &mut ledger,
                                            &wrong::substitute_dispatch(&notice, &attempts[1].0),
                                        );
                                    }
                                    assert_eq!(
                                        ledger.require_open_epoch(),
                                        Err(Failure::UnsupportedQuery)
                                    );
                                    retained_accepted = true;
                                    if matches!(
                                        retained_mode,
                                        retained::Mode::DropAfterApply
                                            | retained::Mode::SourceChangeBeforeAck
                                    ) && !fault_delivered
                                    {
                                        fault_delivered = true;
                                        reply.send(Err(NativeError::AuthorityUnavailable("deliberately lost warm ACK after resident ledger application"))).unwrap();
                                    } else {
                                        reply.send(Ok(())).unwrap();
                                    }
                                    continue;
                                }
                            }
                            deliveries += 1;
                            assert_eq!(notice.routes().len(), owner_count);
                            assert_eq!(
                                notice
                                    .boundary()
                                    .heads()
                                    .iter()
                                    .filter(|head| head.cache().is_some())
                                    .count(),
                                owner_count
                            );
                            if let Some(old) = &captured {
                                assert_eq!(&notice, old);
                                assert!(notice.routes()[0]
                                    .request
                                    .require_pathfinder_dispatch(&p)
                                    .unwrap()
                                    .same_live_dispatch(
                                        old.routes()[0]
                                            .request
                                            .require_pathfinder_dispatch(&p)
                                            .unwrap()
                                    ));
                            } else {
                                captured = Some(notice.clone());
                            }
                            let prior = ledger.diagnostic();
                            if mode == Mode::TwoOwners {
                                assert_ne!(
                                    notice.routes()[0].request.binding.npc,
                                    notice.routes()[1].request.binding.npc
                                );
                                refuse_unchanged(&mut ledger, &wrong::reordered_routes(&notice));
                                refuse_unchanged(&mut ledger, &wrong::wrong_last_cache(&notice));
                                refuse_unchanged(&mut ledger, &wrong::crossed_owner_heads(&notice));
                            }
                            if mode == Mode::RollbackRetry {
                                assert_eq!(late_failures, 1);
                                assert_eq!(ledger.counts(), (2, 0));
                                refuse_unchanged(
                                    &mut ledger,
                                    &wrong::substitute_dispatch(&notice, &attempts[0].0),
                                );
                            }
                            for wrong in [
                                wrong::foreign_session(&notice, &foreign),
                                wrong::skipped_tick(&notice),
                                wrong::wrong_current_cache(&notice),
                                wrong::without_routes(&notice),
                                wrong::duplicate_route(&notice),
                                wrong::stale_generation(&notice),
                                wrong::omitted_head(&notice),
                                wrong::extra_head(&notice),
                                wrong::decoded_dispatch(&notice),
                                wrong::uncertified_reply(&notice),
                            ] {
                                refuse_unchanged(&mut ledger, &wrong);
                            }
                            if mode == Mode::RejectBeforeApply && !fault_delivered {
                                assert_eq!(ledger.counts(), (1, 0));
                                assert_eq!(ledger.diagnostic(), prior);
                                fault_delivered = true;
                                reply.send(Err(NativeError::AuthorityBinding)).unwrap();
                                continue;
                            }
                            if mode == Mode::ExpireBeforeAck {
                                let (context, deadline) = &attempts[0];
                                ledger
                                    .check_fresh_candidate(&provider, context, *deadline)
                                    .expect("original source lease still live before expiry wait");
                                std::thread::sleep(
                                    deadline.saturating_duration_since(Instant::now())
                                        + Duration::from_millis(1),
                                );
                                assert!(matches!(
                                    ledger.check_fresh_candidate(&provider, context, *deadline),
                                    Err(ResidentFilterError::Context(Failure::Deadline))
                                ));
                            }
                            if mode == Mode::SourceChangeBeforeAck && source_change.is_none() {
                                let (context, deadline) = &attempts[0];
                                source_change = Some(
                                    super::source_route_resident_history::SourceChange::begin(
                                        install, &ledger, &provider, context, *deadline,
                                    ),
                                );
                                assert_eq!(ledger.diagnostic(), prior);
                            }
                            let already = ledger.counts().1 == owner_count;
                            ledger.acknowledge(&notice).unwrap();
                            assert_eq!(ledger.counts(), (owner_count, owner_count));
                            if already {
                                assert_eq!(
                                    ledger.diagnostic(),
                                    prior,
                                    "lost successful ACK retries are idempotent"
                                );
                            }
                            let accepted = ledger.diagnostic();
                            ledger.acknowledge(&notice).unwrap();
                            assert_eq!(ledger.diagnostic(), accepted);
                            refuse_unchanged(&mut ledger, &wrong::decoded_dispatch(&notice));
                            assert_eq!(ledger.require_open_epoch(), Err(Failure::UnsupportedQuery));
                            if mode == Mode::RollbackRetry {
                                refuse_unchanged(
                                    &mut ledger,
                                    &wrong::substitute_dispatch(&notice, &attempts[0].0),
                                );
                            }
                            if matches!(mode, Mode::DropAfterApply | Mode::SourceChangeBeforeAck)
                                && !fault_delivered
                            {
                                fault_delivered = true;
                                reply
                                    .send(Err(NativeError::AuthorityUnavailable(
                                        "deliberately lost ACK after resident ledger application",
                                    )))
                                    .unwrap();
                            } else {
                                reply.send(Ok(())).unwrap();
                            }
                        }
                    }
                }
                let ((tick, events, count), committed_warm) =
                    worker.join().expect("bounded controller worker");
                if mode.retained().is_some() {
                    retained::assert_commit_proof(
                        committed_warm.as_ref().expect("actual durable warm tick"),
                        retained_notice
                            .as_ref()
                            .expect("actual accepted warm notice"),
                    );
                } else {
                    assert!(committed_warm.is_none());
                }
                assert_eq!(ledger.counts(), (owner_count, owner_count));
                assert_eq!(
                    deliveries,
                    if matches!(
                        mode,
                        Mode::DropAfterApply
                            | Mode::RejectBeforeApply
                            | Mode::SourceChangeBeforeAck
                    ) {
                        2
                    } else if let Some(retained_mode) = mode.retained() {
                        if matches!(
                            retained_mode,
                            retained::Mode::DropAfterApply | retained::Mode::SourceChangeBeforeAck
                        ) {
                            3
                        } else {
                            2
                        }
                    } else {
                        1
                    }
                );
                assert_eq!(
                    routes.load(Ordering::SeqCst),
                    if matches!(mode, Mode::RollbackRetry | Mode::RetainedDispatch) {
                        2
                    } else if let Some(retained_mode) = mode.retained() {
                        if matches!(
                            retained_mode,
                            retained::Mode::RollbackRetry | retained::Mode::SecondRetainedDispatch
                        ) {
                            3
                        } else {
                            2
                        }
                    } else {
                        owner_count
                    }
                );
                assert_eq!(
                    late_failures,
                    usize::from(
                        mode == Mode::RollbackRetry
                            || mode.retained() == Some(retained::Mode::RollbackRetry)
                    )
                );
                assert_eq!(retained_accepted, mode.retained().is_some());
                assert_eq!(retained_notice.is_some(), mode.retained().is_some());
                assert!(captured.is_some());
                assert_eq!(provider.retained_constructor_count(), owner_count);
                if mode == Mode::RetainedDispatch
                    || mode.retained() == Some(retained::Mode::SecondRetainedDispatch)
                {
                    let (context, before) =
                        retained_witness.as_ref().expect("actual retained callback");
                    assert_eq!(context.binding.tick, tick + 1);
                    assert_eq!(ledger.diagnostic(), *before);
                } else {
                    assert!(retained_witness.is_none());
                }
                // Direct negative source work occurs only after all controller
                // retry measurements and worker shutdown, never during ACK retry.
                let context = &attempts.last().unwrap().0;
                if mode.retained().is_some() {
                    let before = ledger.diagnostic();
                    assert!(matches!(
                        ledger.certify_retained(
                            geometry.app.world_mut(),
                            compatibility,
                            &mut provider,
                            context,
                            301,
                            runtime::until(),
                            || false,
                        ),
                        Err(ResidentFilterError::Context(Failure::UnsupportedQuery))
                    ));
                    assert_eq!(
                        ledger.diagnostic(),
                        before,
                        "cache revision two closes retained certification before source work"
                    );
                    if mode.retained() == Some(retained::Mode::SourceChangeBeforeAck) {
                        source_change
                            .as_ref()
                            .expect("authored source remains changed through warm ACK retry")
                            .refuse_fresh_retained_point_source(
                                &ledger, &provider, height, context,
                            );
                    } else {
                        assert!(source_change.is_none());
                    }
                } else if let Some(change) = &source_change {
                    assert_eq!(mode, Mode::SourceChangeBeforeAck);
                    change.refuse_fresh_source(&ledger, &mut provider, context);
                } else {
                    assert_ne!(mode, Mode::SourceChangeBeforeAck);
                    let deadline = runtime::until();
                    let (constructor, water) = runtime::prerequisites_for_player(
                        &mut provider,
                        context,
                        player,
                        101,
                        deadline,
                    );
                    let before = ledger.diagnostic();
                    assert!(matches!(
                        ledger.certify(
                            geometry.app.world_mut(),
                            compatibility,
                            &mut provider,
                            context,
                            constructor,
                            water,
                            105,
                            deadline,
                            || false
                        ),
                        Err(ResidentFilterError::Context(Failure::UnsupportedQuery))
                    ));
                    assert_eq!(
                        ledger.diagnostic(),
                        before,
                        "post-ACK certification cannot emit another Route"
                    );
                }

                eprintln!("resident controller actual ACK: mode={mode:?} tick={tick} owners={owner_count} candidates={owner_count} committed={owner_count} deliveries={deliveries} callbacks={count} published-events={events}");
            });
        },
    );
}
pub(super) fn one_retained_probe(mode: retained::Mode) {
    run(Mode::OneRetained(mode));
}
pub(super) fn changed_source_ack_probe() {
    run(Mode::SourceChangeBeforeAck);
}
pub(super) fn retained_dispatch_probe() {
    run(Mode::RetainedDispatch);
}
#[test]
fn resident_controller_commits_exact_first_cold_candidate() {
    run(Mode::Accept);
}
#[test]
fn resident_controller_rejected_notice_recovers_without_replay() {
    run(Mode::RejectBeforeApply);
}
#[test]
fn resident_controller_lost_ack_retries_historical_notice_without_replay() {
    run(Mode::DropAfterApply);
}
#[test]
fn resident_controller_historical_ack_survives_original_source_expiry() {
    run(Mode::ExpireBeforeAck);
}

#[test]
fn resident_controller_retry_ack_selects_fresh_live_candidate_not_failed_ancestor() {
    run(Mode::RollbackRetry);
}

#[test]
fn resident_controller_two_owner_notice_is_atomic_before_publication() {
    run(Mode::TwoOwners);
}

pub(super) fn one_retained_refusals_probe() {
    run(Mode::OneRetained(retained::Mode::Refusals));
}

pub(super) fn second_retained_dispatch_probe() {
    run(Mode::OneRetained(retained::Mode::SecondRetainedDispatch));
}
