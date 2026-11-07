// SPDX-License-Identifier: GPL-3.0-or-later
//! Test-only accepted quest restart through fresh loaded source work. The controller
//! uses stock commands; spatial callbacks outside Environment/NearPoint/Route remain
//! the existing explicit quest fixture. Production cached-world admission stays shut.
use super::super::super::{
    route_commit::{RouteCommitBoundary, RouteCommitNotice, RouteCommitSink},
    source_cache_reconstruction::{
        self as reconstruction, ReconstructionAuthority, ReconstructionRouteOwner,
    },
    source_route_same_polygon::{SamePolygonProducts, SamePolygonProviders},
    source_route_same_polygon_local,
};
use super::{core, loaded, quest_inputs, Geometry};
use benilla_world_runtime::{
    native::{
        self as n, Authority, AuthorityReply, AuthorityRequest, MotionQuery, NativeError, Query,
    },
    PlayerId,
};
use benilla_world_store::native::{reconstruction::RecipeStore, Identity};
use crossbeam_channel::{bounded, Receiver, Sender};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
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
    Inspect(
        RouteCommitBoundary,
        Vec<AuthorityReply>,
        bool,
        Sender<Result<loaded::Heads, NativeError>>,
    ),
}
#[derive(Clone)]
struct Bridge {
    send: Sender<Message>,
    observed: loaded::Observed,
}
impl Bridge {
    fn exchange<T>(
        &self,
        make: impl FnOnce(Sender<Result<T, NativeError>>) -> Message,
    ) -> Result<T, NativeError> {
        let (tx, rx) = bounded(1);
        self.send
            .send(make(tx))
            .map_err(|_| NativeError::AuthorityUnavailable("quest restart source owner closed"))?;
        rx.recv_timeout(Duration::from_secs(120))
            .map_err(|_| NativeError::AuthorityUnavailable("quest restart source owner timeout"))?
    }
    fn inspect(
        &self,
        boundary: &RouteCommitBoundary,
        routes: &[AuthorityReply],
        acknowledged: bool,
    ) -> loaded::Heads {
        self.exchange(|tx| Message::Inspect(boundary.clone(), routes.to_vec(), acknowledged, tx))
            .unwrap()
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
        self.observed.attempted.lock().unwrap().push(notice.clone());
        self.exchange(|tx| Message::Notice(notice.clone(), tx))?;
        if self.observed.lose_ack.swap(false, Ordering::SeqCst) {
            return Err(NativeError::AuthorityUnavailable(
                "authored quest restart lost ACK",
            ));
        }
        Ok(())
    }
}

struct Consumer {
    geometry: Geometry,
    session: core::Session,
    providers: SamePolygonProviders,
    boundary: Option<RouteCommitBoundary>,
    other: n::synthetic_distinct_quest_fixture::CompleteAuthority,
    last_notice: Option<RouteCommitNotice>,
    fail_after_prefix: bool,
    fail_at_prefix: Option<u64>,
    replayed_prefix: Arc<AtomicU64>,
}
impl Consumer {
    fn new(
        geometry: Geometry,
        profile: &n::Profile,
        mut session: core::Session,
        products: SamePolygonProducts,
        player: PlayerId,
    ) -> Self {
        session.player_environment =
            Some(core::PlayerEnvironmentPolicy::from_profile(profile, player).unwrap());
        session.primary_near_point =
            Some(core::PrimaryNearPointPolicy::from_profile(profile).unwrap());
        session.same_polygon_route = Some(
            core::SamePolygonRoutePolicy::from_profile(
                profile,
                player,
                products.binding().unwrap(),
            )
            .unwrap(),
        );
        let providers = products.bind(&session, profile, player).unwrap();
        assert_eq!(providers.route.registered_constructor_count(), 0);
        Self {
            geometry,
            session,
            providers,
            boundary: None,
            other: Default::default(),
            last_notice: None,
            fail_after_prefix: false,
            fail_at_prefix: None,
            replayed_prefix: Arc::new(AtomicU64::new(0)),
        }
    }
    fn inspect(
        &self,
        boundary: &RouteCommitBoundary,
        routes: &[AuthorityReply],
        acknowledged: bool,
    ) -> loaded::Heads {
        let provider = &self.providers.route;
        let mut committed = BTreeMap::new();
        for head in boundary.heads() {
            let actual = provider.committed_candidate_for(head.npc());
            assert_eq!(actual.map(|(_, cache)| cache), head.cache());
            if let Some((pointer, cache)) = actual {
                committed.insert(head.npc(), (pointer, cache.clone()));
            }
        }
        let candidates = routes
            .iter()
            .map(|route| {
                provider
                    .candidate_identity(route)
                    .expect("fresh live native proposal")
            })
            .collect();
        if acknowledged {
            assert_eq!(provider.retained_candidate_count(), committed.len());
            assert_eq!(provider.certified_candidate_count(), committed.len());
        }
        loaded::Heads {
            committed,
            candidates,
        }
    }
    fn acknowledge(
        &mut self,
        notice: &RouteCommitNotice,
        observed: &loaded::Observed,
    ) -> Result<(), NativeError> {
        if self.last_notice.as_ref() == Some(notice) {
            self.providers
                .route
                .acknowledge(&self.session, notice)
                .map_err(|_| NativeError::AuthorityBinding)?;
            assert_eq!(self.boundary.as_ref(), Some(notice.boundary()));
            return Ok(());
        }
        let previous = self.boundary.as_ref().unwrap();
        assert_eq!(notice.boundary().tick(), previous.tick() + 1);
        assert_eq!(notice.previous_record(), previous.record());
        assert_eq!(notice.before(), previous.state());
        let before = self.inspect(previous, notice.routes(), false);
        for route in notice.routes() {
            core::validate_facts(&self.session, &route.request, &route.facts).unwrap();
            let npc = route.request.binding.npc.unwrap();
            let calculation = loaded::calculation(route);
            assert_eq!(calculation.after_cache.binding.identity.creature, npc.spawn);
            assert_eq!(
                calculation.after_cache.binding.owner_generation,
                npc.generation
            );
            if let Some((_, cache)) = before.committed.get(&npc) {
                assert_eq!(calculation.prior_cache.as_deref(), Some(cache));
                assert_eq!(calculation.after_cache.revision, cache.revision + 1);
            } else {
                assert!(calculation.prior_cache.is_none());
                assert_eq!(calculation.after_cache.revision, 1);
            }
        }
        self.providers
            .route
            .acknowledge(&self.session, notice)
            .map_err(|_| NativeError::AuthorityBinding)?;
        let after = self.inspect(notice.boundary(), notice.routes(), true);
        for (route, pointer) in notice.routes().iter().zip(before.candidates) {
            if let Some((committed, cache)) =
                after.committed.get(&route.request.binding.npc.unwrap())
            {
                assert_eq!(
                    *committed, pointer,
                    "only the exact live proposal is promoted"
                );
                assert_eq!(*cache, loaded::calculation(route).after_cache);
            }
        }
        self.boundary = Some(notice.boundary().clone());
        self.last_notice = Some(notice.clone());
        observed.committed.lock().unwrap().push(notice.clone());
        Ok(())
    }
}
impl Authority for Consumer {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        let deadline = Instant::now() + Duration::from_secs(20);
        let facts = match &request.query {
            Query::PlayerEnvironment(_) => self
                .providers
                .environment
                .resolve(&self.session, request, deadline)
                .map_err(|_| NativeError::AuthorityBinding)?,
            Query::SourceMotion(MotionQuery::NearPoint(_)) => self
                .providers
                .near_point
                .resolve(&self.session, request, deadline)
                .map_err(|_| NativeError::AuthorityBinding)?,
            Query::SourceMotion(MotionQuery::Route(_)) => self
                .geometry
                .with_local_corridor(|corridor| {
                    source_route_same_polygon_local::resolve(
                        &mut self.providers.route,
                        &self.session,
                        request,
                        corridor,
                        1,
                        deadline,
                    )
                })
                .map_err(|_| NativeError::AuthorityBinding)?,
            _ => return self.other.resolve(request),
        };
        core::validate_facts(&self.session, request, &facts).unwrap();
        Ok(AuthorityReply {
            request: request.clone(),
            facts,
        })
    }
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        self.other.observe_derived(reply)?;
        // Fail during a later live tick, not before replay begins. The preceding
        // cold tick, or exact completed-loot boundary, must already have been
        // compared and acknowledged by replay.
        if self.boundary.as_ref().is_some_and(|boundary| {
            (self.fail_after_prefix
                && boundary.tick() > 0
                && boundary.heads().iter().any(|head| head.cache().is_some()))
                || self.fail_at_prefix == Some(boundary.tick())
        }) && matches!(&reply.request.query, Query::SourceMotion(MotionQuery::Combat(n::CombatQuery::NpcSemantics(request))) if request.phase == n::CombatPhase::UnitPrelude)
            && reply.request.binding.npc.is_some_and(|npc| npc.spawn == 3)
        {
            self.replayed_prefix
                .store(self.boundary.as_ref().unwrap().tick(), Ordering::SeqCst);
            return Err(NativeError::AuthorityUnavailable(
                "authored quest replay late giver failure",
            ));
        }
        Ok(())
    }
}
impl ReconstructionAuthority for Consumer {
    fn route_state(
        &mut self,
    ) -> (
        &core::Session,
        &mut dyn ReconstructionRouteOwner,
        &mut RouteCommitBoundary,
    ) {
        (
            &self.session,
            &mut self.providers.route,
            self.boundary
                .as_mut()
                .expect("cache-free reconstruction anchor"),
        )
    }
}
fn serve(receiver: Receiver<Message>, consumer: &mut Consumer, observed: &loaded::Observed) {
    while let Ok(message) = receiver.recv_timeout(Duration::from_secs(120)) {
        consumer.geometry.advance_physics();
        match message {
            Message::Resolve(request, reply) => {
                reply.send(consumer.resolve(&request)).unwrap();
            }
            Message::Initialize(boundary, reply) => {
                let result = consumer
                    .providers
                    .route
                    .initialize_reconstruction(&consumer.session, &boundary)
                    .map_err(|_| NativeError::AuthorityBinding);
                if result.is_ok() {
                    consumer.boundary = Some(boundary);
                }
                reply.send(result).unwrap();
            }
            Message::Notice(notice, reply) => {
                reply.send(consumer.acknowledge(&notice, observed)).unwrap();
            }
            Message::Inspect(boundary, routes, acknowledged, reply) => {
                reply
                    .send(Ok(consumer.inspect(&boundary, &routes, acknowledged)))
                    .unwrap();
            }
        }
    }
}
fn scene() -> (Geometry, n::Profile, core::Session, SamePolygonProducts) {
    let geometry = Geometry::new();
    let (profile, session, products) = quest_inputs(
        geometry.products(),
        geometry.raw_geometry_revision(),
        &geometry
            .app
            .world()
            .resource::<benilla_assets::MapCatalogRes>()
            .0,
        2,
        3,
        [2, 3],
    );
    (geometry, profile, session, products)
}
fn released(witness: &(std::sync::Weak<()>, std::rc::Weak<()>)) {
    assert!(
        witness.0.upgrade().is_none(),
        "old source provider still lives"
    );
    assert!(
        witness.1.upgrade().is_none(),
        "old native query owner still lives"
    );
}
fn assert_retired(consumer: &Consumer) {
    let boundary = consumer.boundary.as_ref().unwrap();
    assert!(boundary
        .owner_heads()
        .all(|(_, head)| head.cache().is_none()));
    assert_eq!(consumer.providers.route.retained_candidate_count(), 0);
    assert_eq!(consumer.providers.route.certified_candidate_count(), 0);
    assert!(consumer.inspect(boundary, &[], true).committed.is_empty());
}
fn run(late_failure: bool, retry_faults: bool) {
    run_stage(late_failure, retry_faults, false);
}
fn run_stage(late_failure: bool, retry_faults: bool, post_reward: bool) {
    let (capture, witness) = {
        let (geometry, profile, session, products) = scene();
        let fixture = loaded::Fixture::new(profile.clone());
        let mut consumer = Consumer::new(geometry, &profile, session, products, fixture.player());
        let witness = consumer.providers.route.reconstruction_liveness();
        let observed = loaded::Observed::default();
        let (send, receive) = bounded(1);
        let bridge = Bridge {
            send,
            observed: observed.clone(),
        };
        let worker_observed = observed.clone();
        let worker = std::thread::spawn(move || {
            let authority = loaded::Authority::new(bridge.clone(), worker_observed.clone());
            if post_reward {
                fixture.capture_rewarded_recipe(
                    authority,
                    bridge.clone(),
                    worker_observed,
                    |boundary, routes, acknowledged| bridge.inspect(boundary, routes, acknowledged),
                )
            } else {
                fixture.capture_recipe(
                    authority,
                    bridge.clone(),
                    worker_observed,
                    |boundary, routes, acknowledged| bridge.inspect(boundary, routes, acknowledged),
                )
            }
        });
        serve(receive, &mut consumer, &observed);
        let capture = worker.join().unwrap();
        if post_reward {
            assert_retired(&consumer);
        }
        (capture, witness)
    };
    released(&witness);
    assert_eq!(capture.is_post_reward(), post_reward);
    let (geometry, profile, session, products) = scene();
    assert_eq!(&profile, capture.profile());
    let mut consumer = Consumer::new(geometry, &profile, session, products, capture.player());
    let fresh_witness = consumer.providers.route.reconstruction_liveness();
    let before = std::fs::read(capture.path().join("world.checkpoint")).unwrap();
    let previous = std::fs::read(capture.path().join("world.previous")).ok();
    let (store, recipe) = RecipeStore::open(
        capture.path(),
        Identity::of(capture.checkpoint()),
        |anchor| n::NativeWorld::restore_synthetic_component_test(profile.clone(), anchor),
    )
    .unwrap();
    assert_eq!(&recipe, capture.recipe());
    let world =
        n::NativeWorld::restore_synthetic_component_test(profile.clone(), recipe.anchor().clone())
            .unwrap();
    consumer.boundary = Some(RouteCommitBoundary::capture_initial(&world).unwrap());
    consumer.fail_after_prefix = late_failure && !post_reward;
    if late_failure && post_reward {
        assert!(capture.loot_prefix_tick() > recipe.anchor().identity().tick);
        assert!(capture.loot_prefix_tick() < recipe.expected_checkpoint().identity().tick);
        consumer.fail_at_prefix = Some(capture.loot_prefix_tick());
    }
    let prefix = Arc::clone(&consumer.replayed_prefix);
    let result = reconstruction::reconstruct_completed(store, &recipe, world, consumer);
    assert_eq!(
        std::fs::read(capture.path().join("world.checkpoint")).unwrap(),
        before
    );
    assert_eq!(
        std::fs::read(capture.path().join("world.previous")).ok(),
        previous
    );
    if late_failure {
        assert!(matches!(
            result,
            Err(NativeError::AuthorityUnavailable(
                "authored quest replay late giver failure"
            ))
        ));
        if post_reward {
            assert_eq!(prefix.load(Ordering::SeqCst), capture.loot_prefix_tick());
        } else {
            assert!(prefix.load(Ordering::SeqCst) > recipe.anchor().identity().tick);
        }
        released(&fresh_witness);
        let (store, unchanged) = RecipeStore::open(
            capture.path(),
            Identity::of(capture.checkpoint()),
            |anchor| n::NativeWorld::restore_synthetic_component_test(profile.clone(), anchor),
        )
        .unwrap();
        assert_eq!(unchanged, recipe);
        drop(store);
        return;
    }
    let completed = result.unwrap();
    let (completed, mut consumer) = completed.detach_authority();
    assert_eq!(
        consumer.boundary.as_ref().unwrap().tick(),
        capture.checkpoint().identity().tick
    );
    consumer.inspect(consumer.boundary.as_ref().unwrap(), &[], true);
    if post_reward {
        assert_retired(&consumer);
    }
    let observed = loaded::Observed::default();
    let (send, receive) = bounded(1);
    let bridge = Bridge {
        send,
        observed: observed.clone(),
    };
    let worker_observed = observed.clone();
    let worker = std::thread::spawn(move || {
        let authority = loaded::Authority::new(bridge.clone(), worker_observed.clone());
        capture.resume(completed, authority, bridge, worker_observed, retry_faults);
    });
    serve(receive, &mut consumer, &observed);
    worker.join().unwrap();
    if post_reward {
        assert_retired(&consumer);
    }
    drop(consumer);
    drop(observed);
    released(&fresh_witness);
}
#[test]
fn accepted_loaded_quest_fresh_replay_finishes_combat_loot_and_reward_exactly() {
    run(false, false);
}
#[test]
fn accepted_loaded_quest_late_replay_failure_releases_owners_and_preserves_bytes() {
    run(true, false);
}
#[test]
fn accepted_loaded_quest_resumed_save_and_lost_ack_retry_publish_once() {
    run(false, true);
}

#[test]
fn rewarded_loaded_quest_fresh_replay_rejects_duplicate_rewards_and_exhausted_loot() {
    run_stage(false, false, true);
}
#[test]
fn rewarded_loaded_quest_late_replay_after_loot_preserves_bytes_and_releases_owners() {
    run_stage(true, false, true);
}
