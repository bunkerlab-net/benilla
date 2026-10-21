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
    RollbackRetry,
    TwoOwners,
}
enum Message {
    Resolve(
        AuthorityRequest,
        Sender<Result<AuthorityReply, NativeError>>,
    ),
    Derived(AuthorityReply, Sender<Result<(), NativeError>>),
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
        self.exchange(|answer| Message::Derived(reply.clone(), answer))
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
    runtime::with_controller_fixture(
        mode == Mode::TwoOwners,
        |install, height, compatibility, inventory, policy, p, query, other| {
            let fixture = resident_fixture::Fixture::new(p.clone(), [8; 16]);
            let player = fixture.player();
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
                        if matches!(mode, Mode::RejectBeforeApply | Mode::DropAfterApply) {
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
                        return (tick, out.events.len(), count);
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
                                    let (constructor, water) = runtime::prerequisites(
                                        &mut provider,
                                        &request,
                                        101,
                                        deadline,
                                    );
                                    if attempt == 0 {
                                        let before = ledger.diagnostic();
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
                                    let (constructor, water) = runtime::prerequisites(
                                        &mut provider,
                                        &request,
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
                                        let (constructor, water) = runtime::prerequisites(
                                            &mut provider,
                                            &request,
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
                                assert_eq!(routes.load(Ordering::SeqCst), 0);
                                binding_error(ledger.acknowledge(&notice)).unwrap();
                                reply.send(Ok(())).unwrap();
                                continue;
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
                            if mode == Mode::DropAfterApply && !fault_delivered {
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
                let (tick, events, count) = worker.join().expect("bounded controller worker");
                assert_eq!(ledger.counts(), (owner_count, owner_count));
                assert_eq!(
                    deliveries,
                    if matches!(mode, Mode::DropAfterApply | Mode::RejectBeforeApply) {
                        2
                    } else {
                        1
                    }
                );
                assert_eq!(
                    routes.load(Ordering::SeqCst),
                    if mode == Mode::RollbackRetry {
                        2
                    } else {
                        owner_count
                    }
                );
                assert_eq!(late_failures, usize::from(mode == Mode::RollbackRetry));
                assert!(captured.is_some());
                assert_eq!(provider.retained_constructor_count(), owner_count);
                // Direct negative source work occurs only after all controller
                // retry measurements and worker shutdown, never during ACK retry.
                let context = &attempts.last().unwrap().0;
                let deadline = runtime::until();
                let (constructor, water) =
                    runtime::prerequisites(&mut provider, context, 101, deadline);
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

                eprintln!("resident controller actual ACK: mode={mode:?} tick={tick} owners={owner_count} candidates={owner_count} committed={owner_count} deliveries={deliveries} callbacks={count} published-events={events}");
            });
        },
    );
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
