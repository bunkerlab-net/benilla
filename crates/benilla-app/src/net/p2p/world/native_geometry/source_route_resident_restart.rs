// SPDX-License-Identifier: GPL-3.0-or-later
//! Component-only cold + retained capture, complete owner destruction, live replay,
//! then the existing third epoch. Not production cached-world reopening.
use super::{
    core::{PlayerEnvironmentPolicy, Session},
    products::{
        source_height::ResidentInitializationConfiguration, vmap::installed_source_fixture as input,
    },
    real_corridor_fixture::installed_source::InstalledFixture,
    route_commit::{RouteCommitBoundary, RouteCommitNotice, RouteCommitSink},
    source_cache_reconstruction::{
        self as reconstruction, resident::ResidentReconstructionOwner, ReconstructionAuthority,
        ReconstructionRouteOwner,
    },
    source_creature_initialization::resident::ResidentCreatureInitializationProvider,
    source_pathfinder_query::resident_joined_tests as runtime,
    source_route_filter::resident::ResidentFilterProvider,
    source_route_resident_local::ledger::ResidentLedger,
};
use crate::net::p2p::world::resident_fixture::{Capture, Driver, Fixture};
use benilla_sim::creature_ai::Point;
use benilla_world_runtime::native::{
    self as n, Authority, AuthorityReply, AuthorityRequest, Facts, MotionQuery, NativeError, Query,
};
use benilla_world_store::native::{
    reconstruction::{Recipe, RecipeStore},
    Identity,
};
use crossbeam_channel::{bounded, Receiver, Sender};
use std::{
    cell::Cell,
    rc::Rc,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

enum Message {
    Resolve(
        AuthorityRequest,
        Sender<Result<AuthorityReply, NativeError>>,
    ),
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
        let (tx, rx) = bounded(1);
        self.0.send(make(tx)).map_err(|_| {
            NativeError::AuthorityUnavailable("resident restart source owner closed")
        })?;
        rx.recv_timeout(Duration::from_secs(120)).map_err(|_| {
            NativeError::AuthorityUnavailable("resident restart source owner timeout")
        })?
    }
}
impl Authority for Bridge {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        self.exchange(|tx| Message::Resolve(request.clone(), tx))
    }
}
impl RouteCommitSink for Bridge {
    fn initialize(&mut self, boundary: &RouteCommitBoundary) -> Result<(), NativeError> {
        self.exchange(|tx| Message::Initialize(boundary.clone(), tx))
    }
    fn deliver(&mut self, notice: &RouteCommitNotice) -> Result<(), NativeError> {
        self.exchange(|tx| Message::Notice(notice.clone(), tx))
    }
}
struct Consumer<O, F> {
    owner: O,
    resolve: F,
    session: Session,
    boundary: RouteCommitBoundary,
}
impl<O, F: FnMut(&mut O, &AuthorityRequest) -> Result<Facts, NativeError>> Authority
    for Consumer<O, F>
{
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        Ok(AuthorityReply {
            request: request.clone(),
            facts: (self.resolve)(&mut self.owner, request)?,
        })
    }
}
impl<
        O: ReconstructionRouteOwner,
        F: FnMut(&mut O, &AuthorityRequest) -> Result<Facts, NativeError>,
    > ReconstructionAuthority for Consumer<O, F>
{
    fn route_state(
        &mut self,
    ) -> (
        &Session,
        &mut dyn ReconstructionRouteOwner,
        &mut RouteCommitBoundary,
    ) {
        (&self.session, &mut self.owner, &mut self.boundary)
    }
}
fn serve(
    receiver: Receiver<Message>,
    owner: &mut impl ReconstructionAuthority,
    routes: &AtomicUsize,
) {
    while let Ok(message) = receiver.recv_timeout(Duration::from_secs(120)) {
        match message {
            Message::Resolve(request, reply) => {
                let result = owner.resolve(&request);
                if result.is_ok()
                    && matches!(request.query, Query::SourceMotion(MotionQuery::Route(_)))
                {
                    routes.fetch_add(1, Ordering::SeqCst);
                }
                reply.send(result).unwrap();
            }
            Message::Initialize(boundary, reply) => {
                let (session, state, current) = owner.route_state();
                let result = state
                    .initialize_reconstruction(session, &boundary)
                    .map_err(|_| NativeError::AuthorityBinding);
                if result.is_ok() {
                    *current = boundary;
                }
                reply.send(result).unwrap();
            }
            Message::Notice(notice, reply) => {
                let (session, state, current) = owner.route_state();
                let result = state
                    .acknowledge(session, &notice)
                    .map_err(|_| NativeError::AuthorityBinding);
                if result.is_ok() {
                    *current = notice.boundary().clone();
                }
                reply.send(result).unwrap();
            }
        }
    }
}
struct Initializer<F>(F);
impl<F: FnMut(&AuthorityRequest) -> Result<Facts, NativeError>> Authority for Initializer<F> {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if matches!(request.query, Query::CreatureInitialization(_)) {
            Ok(AuthorityReply {
                request: request.clone(),
                facts: (self.0)(request)?,
            })
        } else {
            assert!(!matches!(
                request.query,
                Query::SourceMotion(
                    MotionQuery::Route(_) | MotionQuery::NearPoint(_) | MotionQuery::Spatial(_)
                )
            ));
            n::synthetic_motion_fixture::SyntheticMotionAuthority.resolve(request)
        }
    }
}
fn advance_to(driver: &mut Driver, routes: &AtomicUsize, wanted: usize) {
    for _ in 0..128 {
        if routes.load(Ordering::SeqCst) == wanted {
            return;
        }
        if routes.load(Ordering::SeqCst) == 0 {
            driver
                .queue_profile_target_movement(driver.tick() + 1)
                .unwrap();
        } else {
            let p = driver.state().position;
            driver
                .queue_movement(
                    Point::new(p.x.as_f32() - 0.125, p.y.as_f32(), p.z.as_f32()).unwrap(),
                    0.5,
                )
                .unwrap();
        }
        let generation = driver.store_generation();
        let out = driver.pump_one();
        assert!(out.pause.is_none(), "{:?}", out.pause);
        assert_eq!(out.ticks, 1);
        assert_eq!(driver.store_generation(), generation + 1);
        assert!(!driver.route_pending());
        assert!(driver.records().is_empty());
    }
    panic!("existing bounded third epoch was not reached");
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fault {
    None,
    Cancelled,
    Expired,
    AfterColdPrefix,
    RecordMismatch,
    ChangedSource,
    TamperedEnvelope,
}
// Authored temporary source only. Restore exact bytes even on an assertion panic.
struct RestoreSource<'a> {
    install: &'a input::Install,
    original: Vec<(String, Vec<u8>)>,
    before: std::collections::BTreeMap<String, Vec<u8>>,
}
impl<'a> RestoreSource<'a> {
    fn changed(install: &'a input::Install) -> Self {
        let guard = Self {
            install,
            original: input::navigation_entries(),
            before: install.bytes(),
        };
        let mut changed = guard.original.clone();
        changed
            .iter_mut()
            .find(|(name, _)| name == input::WDT)
            .unwrap()
            .1[20] ^= 1;
        install.write(&changed);
        assert_ne!(install.bytes(), guard.before);
        guard
    }
}
impl Drop for RestoreSource<'_> {
    fn drop(&mut self) {
        self.install.write(&self.original);
        assert_eq!(self.install.bytes(), self.before);
    }
}
fn run(fault: Fault) {
    let mut previous: Option<Capture> = None;
    let mut baseline: Option<Recipe> = None;
    for resumed in [false, true] {
        let prior = previous.take();
        let (capture, witness) = runtime::with_selected_controller_fixture(
            runtime::ControllerOwners::Single,
            true,
            true,
            false,
            false,
            runtime::GeneratedSupport::Disabled,
            |install, height, compatibility, inventory, policy, profile, query, _other| {
                let fixture = (!resumed).then(|| Fixture::new(profile.clone(), [8; 16]));
                let player = prior
                    .as_ref()
                    .map(Capture::player)
                    .unwrap_or_else(|| fixture.as_ref().unwrap().player());
                let world_id = prior
                    .as_ref()
                    .map(Capture::world_id)
                    .unwrap_or_else(|| fixture.as_ref().unwrap().world_id());
                if let Some(prior) = &prior {
                    assert_eq!(prior.profile(), &profile);
                }
                let mut session = runtime::session(&profile);
                session.world = world_id;
                session.player_environment =
                    Some(PlayerEnvironmentPolicy::from_profile(&profile, player).unwrap());
                let provider = ResidentFilterProvider::bind_with_policy(
                    &session, &profile, height, query, policy,
                )
                .unwrap();
                provider.require_two_retained_routes().unwrap();
                let ledger = ResidentLedger::bind(
                    &session,
                    &profile,
                    player,
                    &provider,
                    compatibility,
                    runtime::until(),
                )
                .unwrap();
                let owner = ResidentReconstructionOwner::new(provider, ledger);
                assert_eq!(owner.constructor_count(), 0);
                let source = ResidentCreatureInitializationProvider::bind(
                    world_id,
                    &profile,
                    height,
                    Some(ResidentInitializationConfiguration::VmapHeightEnabled),
                    runtime::until(),
                    || false,
                )
                .unwrap();
                let mut initializer = Initializer(|request: &AuthorityRequest| {
                    source
                        .resolve(request, runtime::until(), || false)
                        .map_err(|_| NativeError::AuthorityBinding)
                });
                let world = n::NativeWorld::synthetic_component_test_with_initialization(
                    profile.clone(),
                    world_id,
                    player,
                    1234,
                    None,
                    &mut initializer,
                )
                .unwrap();
                drop(initializer);
                drop(source);
                let boundary = RouteCommitBoundary::capture_initial(&world).unwrap();
                let mut geometry = InstalledFixture::new(install.path(), inventory);
                geometry.advance_physics();
                let source_session = session.clone();
                let mut authored = n::synthetic_motion_fixture::SyntheticMotionAuthority;
                let route_calls = Rc::new(Cell::new(0_usize));
                let observed_routes = route_calls.clone();
                let active_fault = if resumed { fault } else { Fault::None };
                let consumer = Consumer {
                    owner,
                    session,
                    boundary,
                    resolve: move |owner: &mut ResidentReconstructionOwner<'_, '_, '_, '_>,
                                   request: &AuthorityRequest| {
                        let changed_source = if active_fault == Fault::ChangedSource
                            && observed_routes.get() == 1
                            && matches!(request.query, Query::SourceMotion(MotionQuery::Route(_)))
                        {
                            assert_eq!(owner.ledger_counts(), (1, 1));
                            Some(RestoreSource::changed(install))
                        } else {
                            None
                        };
                        if matches!(request.query, Query::SourceMotion(MotionQuery::Route(_))) {
                            if active_fault == Fault::AfterColdPrefix && observed_routes.get() == 1
                            {
                                assert_eq!(owner.ledger_counts(), (1, 1));
                                return Err(NativeError::AuthorityUnavailable(
                                    "restart deliberate later source failure",
                                ));
                            }
                            observed_routes.set(observed_routes.get() + 1);
                        }
                        let deadline = if active_fault == Fault::Expired {
                            Instant::now() - Duration::from_secs(1)
                        } else {
                            runtime::until()
                        };
                        if changed_source.is_some() {
                            assert!(deadline > Instant::now());
                        }
                        let mut facts = match owner.resolve_live(
                            geometry.app.world_mut(),
                            compatibility,
                            &source_session,
                            player,
                            request,
                            deadline,
                            || active_fault == Fault::Cancelled,
                        )? {
                            Some(facts) => facts,
                            None => authored.resolve(request)?.facts,
                        };
                        if active_fault == Fault::RecordMismatch {
                            if let Facts::SourceMotion(n::MotionFacts::Spatial(reply)) = &mut facts
                            {
                                if let benilla_sim::creature_motion::Fact::Bearing(angle) =
                                    &mut reply.fact
                                {
                                    *angle = benilla_sim::creature_ai::Real::from_bits(
                                        if angle.bits() == 0 {
                                            0x0080_0000
                                        } else {
                                            angle.bits() + 1
                                        },
                                    )
                                    .unwrap();
                                }
                            }
                        }
                        Ok(facts)
                    },
                };
                let (tx, rx) = bounded(1);
                let bridge = Bridge(tx);
                let routes = Arc::new(AtomicUsize::new(if resumed { 2 } else { 0 }));
                let worker_routes = routes.clone();
                let (mut consumer, worker) = if let Some(capture) = prior {
                    let recipe = capture.recipe().clone();
                    let before = std::fs::read(capture.path().join("world.checkpoint")).unwrap();
                    if fault == Fault::TamperedEnvelope {
                        let witness = consumer.owner.liveness();
                        let mut tampered = before.clone();
                        let last = tampered.len() - 1;
                        tampered[last] ^= 1;
                        std::fs::write(capture.path().join("world.checkpoint"), &tampered).unwrap();
                        assert!(RecipeStore::open(
                            capture.path(),
                            Identity::of(capture.expected_checkpoint()),
                            |anchor| n::NativeWorld::restore_synthetic_component_test(
                                capture.profile().clone(),
                                anchor
                            )
                        )
                        .is_err());
                        assert_eq!(
                            std::fs::read(capture.path().join("world.checkpoint")).unwrap(),
                            tampered
                        );
                        std::fs::write(capture.path().join("world.checkpoint"), &before).unwrap();
                        let (reopened, actual) = RecipeStore::open(
                            capture.path(),
                            Identity::of(capture.expected_checkpoint()),
                            |anchor| {
                                n::NativeWorld::restore_synthetic_component_test(
                                    capture.profile().clone(),
                                    anchor,
                                )
                            },
                        )
                        .unwrap();
                        assert_eq!(actual, recipe);
                        drop(reopened);
                        drop(consumer);
                        return (capture, witness);
                    }
                    let (store, checked_recipe) = RecipeStore::open(
                        capture.path(),
                        Identity::of(capture.expected_checkpoint()),
                        |anchor| {
                            n::NativeWorld::restore_synthetic_component_test(
                                capture.profile().clone(),
                                anchor,
                            )
                        },
                    )
                    .unwrap();
                    assert_eq!(checked_recipe, recipe);
                    let failed_witness = consumer.owner.liveness();
                    let result =
                        reconstruction::reconstruct_completed(store, &recipe, world, consumer);
                    if fault != Fault::None {
                        assert!(result.is_err(), "{fault:?} unexpectedly reconstructed");
                        if fault == Fault::RecordMismatch {
                            assert!(matches!(result, Err(NativeError::ReplayMismatch)));
                        }
                        if fault == Fault::AfterColdPrefix {
                            assert_eq!(route_calls.get(), 1);
                        }
                        if fault == Fault::ChangedSource {
                            assert_eq!(route_calls.get(), 2);
                        }
                        assert_eq!(
                            std::fs::read(capture.path().join("world.checkpoint")).unwrap(),
                            before
                        );
                        let (reopened, actual) = RecipeStore::open(
                            capture.path(),
                            Identity::of(capture.expected_checkpoint()),
                            |anchor| {
                                n::NativeWorld::restore_synthetic_component_test(
                                    capture.profile().clone(),
                                    anchor,
                                )
                            },
                        )
                        .unwrap();
                        assert_eq!(actual, recipe);
                        drop(reopened);
                        return (capture, failed_witness);
                    }
                    let complete = result.unwrap();
                    assert_eq!(
                        std::fs::read(capture.path().join("world.checkpoint")).unwrap(),
                        before
                    );
                    let (sealed, consumer) = complete.detach_authority();
                    assert_eq!(consumer.owner.constructor_count(), 1);
                    let worker = std::thread::spawn(move || {
                        let (mut driver, ()) =
                            capture.resume(sealed, bridge.clone(), bridge).unwrap();
                        assert!(!driver.initial_events().unwrap().is_empty());
                        assert!(driver.initial_events().is_err());
                        let generation = driver.store_generation();
                        let idle = driver.pump_zero();
                        assert_eq!(idle.ticks, 0);
                        assert!(idle.events.is_empty());
                        assert_eq!(driver.store_generation(), generation);
                        advance_to(&mut driver, &worker_routes, 3);
                        (driver.into_recipe_capture(), None)
                    });
                    (consumer, worker)
                } else {
                    let fixture = fixture.unwrap();
                    let worker = std::thread::spawn(move || {
                        let mut driver = fixture
                            .open_recipe_initialized(world, bridge.clone())
                            .unwrap();
                        driver.attach_route_commit_sink(bridge).unwrap();
                        assert!(!driver.initial_events().unwrap().is_empty());
                        advance_to(&mut driver, &worker_routes, 2);
                        let archive = driver.snapshot_recipe_for_restart();
                        advance_to(&mut driver, &worker_routes, 3);
                        (driver.into_recipe_capture(), Some(archive))
                    });
                    (consumer, worker)
                };
                serve(rx, &mut consumer, &routes);
                let (mut capture, archive) = worker.join().unwrap();
                if let Some(archive) = archive {
                    baseline = Some(capture.recipe().clone());
                    assert_eq!(archive.checkpoint(), archive.recipe().expected_checkpoint());
                    capture = capture.select_archive(archive);
                }
                let witness = consumer.owner.liveness();
                assert!(witness.has_retained_lineage());
                assert_eq!(consumer.owner.constructor_count(), 1);
                drop(consumer);
                (capture, witness)
            },
        );
        assert!(witness.released());
        if resumed && fault == Fault::None {
            assert_eq!(capture.recipe(), baseline.as_ref().unwrap());
        }
        previous = Some(capture);
    }
}

#[test]
fn resident_controller_fresh_recipe_replay_then_third_epoch_matches_uninterrupted() {
    run(Fault::None);
}
#[test]
fn resident_controller_recipe_cancelled_replay_releases_all_owners_without_publication() {
    run(Fault::Cancelled);
}
#[test]
fn resident_controller_recipe_expired_replay_releases_all_owners_without_publication() {
    run(Fault::Expired);
}
#[test]
fn resident_controller_recipe_late_failure_after_cold_prefix_preserves_committed_bytes() {
    run(Fault::AfterColdPrefix);
}
#[test]
fn resident_controller_recipe_full_record_mismatch_rejects_before_publication() {
    run(Fault::RecordMismatch);
}

#[test]
fn resident_controller_recipe_changed_consumed_source_after_cold_prefix_refuses_replay() {
    run(Fault::ChangedSource);
}
#[test]
fn resident_controller_recipe_tampered_envelope_refuses_before_publication() {
    run(Fault::TamperedEnvelope);
}
