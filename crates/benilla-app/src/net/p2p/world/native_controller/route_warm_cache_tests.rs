// SPDX-License-Identifier: GPL-3.0-or-later
//! SyntheticFixture initial calculation followed by actual loaded warm polygon
//! dependencies. Neither the seed nor dependency admits a source Route producer.
use super::*;
use crate::net::p2p::native_geometry::{
    authored_route_smoothing, authored_warm_cache_revision, SourceRouteWarmCacheProvider,
    SourceSmoothRequest, WarmCacheConfiguration, WarmCacheRequest,
};
use std::sync::mpsc;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Observation {
    context: n::AuthorityRequest,
    owner: [u32; 3],
    tickets: [u64; 3],
    from: [u32; 3],
    to: [u32; 3],
    after_storage: Vec<u64>,
    after_active: usize,
    trace: Vec<benilla_native_nav::SourceWarmTrace>,
}
struct WarmAuthority {
    send: mpsc::Sender<Option<n::AuthorityRequest>>,
    receive: mpsc::Receiver<Result<Option<motion::PathFinderCache>, Failure>>,
    service: Option<std::thread::JoinHandle<()>>,
    other: source::SyntheticMotionAuthority,
    queries: Arc<Mutex<Vec<n::AuthorityRequest>>>,
}
impl n::Authority for WarmAuthority {
    fn resolve(
        &mut self,
        context: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        self.queries.lock().unwrap().push(context.clone());
        let Some(route) = route_query(context) else {
            return self.other.resolve(context);
        };
        self.send.send(Some(context.clone())).unwrap();
        let seed = self
            .receive
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .map_err(|error| match error {
                Failure::Binding => n::NativeError::AuthorityBinding,
                Failure::Invalid => n::NativeError::InvalidInput,
                _ => n::NativeError::AuthorityUnavailable("loaded warm cache prerequisite"),
            })?;
        let Some(seed) = seed else {
            // Polygon proposals lack terminal path type, point normalization and
            // segment evidence. Do not turn them into Complete or NoPath facts.
            return Err(n::NativeError::AuthorityUnavailable(
                "source owner route after warm polygons unavailable",
            ));
        };
        assert!(route.request.prior_cache.is_none());
        assert_eq!(seed.source, motion::RouteSource::SyntheticFixture);
        let mut reply = self.other.resolve(context)?;
        let n::Facts::SourceMotion(n::MotionFacts::Spatial(inner)) = &mut reply.facts else {
            panic!("fixture spatial reply")
        };
        let motion::Fact::Route(outcome) = &inner.fact else {
            panic!("fixture route")
        };
        inner.fact = motion::Fact::RouteCalculation(Box::new(motion::RouteCalculation {
            wander_no_path: None,
            source: motion::RouteSource::SyntheticFixture,
            outcome: outcome.clone(),
            prior_cache: None,
            after_cache: seed,
        }));
        Ok(reply)
    }
    fn observe_derived(&mut self, reply: &n::AuthorityReply) -> Result<(), n::NativeError> {
        self.other.observe_derived(reply)
    }
}
impl Drop for WarmAuthority {
    fn drop(&mut self) {
        let _ = self.send.send(None);
        if let Some(service) = self.service.take() {
            service.join().unwrap();
        }
    }
}
fn warm_setup() -> (
    n::Profile,
    WarmAuthority,
    Arc<AtomicU64>,
    Arc<Mutex<Vec<Observation>>>,
) {
    let (mut data, base) = super::setup(true);
    let products = authored_route_smoothing();
    let liquid = products.load_dry_liquid().unwrap();
    let navigation = products.load_navigation().unwrap();
    let config = WarmCacheConfiguration {
        budget: benilla_native_nav::SourceWarmCacheBudget::default(),
    };
    let binding = liquid.binding();
    data.map_id = binding.map;
    data.content_digest = binding.content;
    data.geometry_revision =
        authored_warm_cache_revision(binding, navigation.binding(), config).unwrap();
    data.npcs[0].ai.map_id = binding.map;
    data.npcs[0].ai.content_revision = ai::movement::Revision(binding.content);
    data.npcs[0].ai.spawn = ai::Point::new(-400., -400., 0.).unwrap();
    data.player.start = ai::Point::new(-400., -390., 0.).unwrap();
    data.initialize_creature_capabilities(source::creature_capability_sources(&data))
        .unwrap();
    let mut session = base.session;
    session.profile = data.digest();
    session.content = data.content_digest;
    session.geometry = data.geometry_revision;
    session.map = data.map_id;
    session.participants = Some(NonRenderParticipants::authored_fixture(
        binding.content,
        binding.map,
    ));
    drop(navigation);
    let profile = data.clone();
    let fault = Arc::new(AtomicU64::new(0));
    let mode = Arc::clone(&fault);
    let observations = Arc::new(Mutex::new(vec![]));
    let seen = Arc::clone(&observations);
    let (send, requests) = mpsc::channel::<Option<n::AuthorityRequest>>();
    let (answers, receive) = mpsc::channel();
    let parent_thread = std::thread::current().id();
    let service = std::thread::spawn(move || {
        assert_ne!(std::thread::current().id(), parent_thread);
        let products = authored_route_smoothing();
        let navigation = products.load_navigation().unwrap();
        let nav_binding = navigation.binding();
        // An actual loaded bounded corridor mints this opaque handle on its mesh
        // owner thread. No serialized refs recreate a loaded handle, even on reopen.
        let lineage = navigation
            .seed_bounded_warm_lineage(
                SourceSmoothRequest {
                    snapshot: nav_binding,
                    ticket: 1,
                    from: [-400f32, -400., 0.].map(f32::to_bits),
                    to: [-400f32, -200., 0.].map(f32::to_bits),
                    filter: benilla_native_nav::SourceOwnerFilter {
                        include_flags: 1,
                        exclude_flags: 0,
                    },
                    point_limit: 74,
                    budget: benilla_native_nav::SourceSmoothingBudget::default(),
                },
                Instant::now() + Duration::from_secs(5),
            )
            .unwrap();
        let storage = lineage.cache().storage().to_vec();
        let active = lineage.cache().active_len();
        assert_eq!(active, 2);
        assert_eq!(storage.len(), 74);
        let provider = SourceRouteWarmCacheProvider::bind(
            &session,
            &profile,
            products.load_dry_liquid().unwrap(),
            navigation,
            lineage,
            config,
        )
        .unwrap();
        while let Some(context) = requests.recv().unwrap() {
            let route = route_query(&context).unwrap();
            if route.request.prior_cache.is_none() {
                // Only the initial fixture calculation is accepted. Actual refs
                // do not make this synthetic point path or its evidence authentic.
                answers
                    .send(Ok(Some(motion::PathFinderCache {
                        binding: motion::PathFinderCacheBinding {
                            identity: route.request.binding.identity,
                            owner_generation: route.owner.npc().generation,
                            inventory: nav_binding.inventory,
                            source_epoch: nav_binding.source_epoch,
                            navigation_policy: nav_binding.policy,
                        },
                        revision: 1,
                        source: motion::RouteSource::SyntheticFixture,
                        poly_refs: storage.clone(),
                        active_len: active as u16,
                        point_limit: 74,
                        path_type: 1,
                        requested_end: route.request.goal,
                        include_flags: 1,
                        exclude_flags: 0,
                    })))
                    .unwrap();
                continue;
            }
            let mode = mode.load(Ordering::SeqCst);
            let mut actual_session = session.clone();
            let mut actual_context = context.clone();
            if mode == 2 {
                actual_session.geometry[0] ^= 1;
            }
            if mode == 3 {
                let n::Query::SourceMotion(n::MotionQuery::Route(route)) =
                    &mut actual_context.query
                else {
                    unreachable!()
                };
                route.request.prior_cache.as_mut().unwrap().poly_refs[0] ^= 1;
            }
            let result = (|| {
                let bound = provider.begin(&actual_session, &actual_context)?;
                let request = WarmCacheRequest {
                    snapshot: provider.binding(),
                    context: actual_context,
                    first_ticket: 51,
                };
                let deadline = Instant::now()
                    + if mode == 1 {
                        Duration::ZERO
                    } else {
                        Duration::from_secs(5)
                    };
                let dependency = bound.query(request.clone(), deadline)?;
                assert_eq!(dependency.request(), &request);
                let filter = dependency.filter_dependency();
                assert_eq!(filter.request().context, context);
                let polygons = dependency.polygon_dependency();
                let result = polygons.result();
                assert_eq!(result.from(), point_bits(route.request.origin));
                assert_eq!(result.to(), point_bits(route.request.goal));
                assert_eq!(result.after().active_len(), 1);
                assert_eq!(result.after().storage(), storage);
                seen.lock().unwrap().push(Observation {
                    context: context.clone(),
                    owner: filter.owner_position(),
                    tickets: [
                        filter.liquid_observations()[0].reply.request.ticket,
                        filter.liquid_observations()[1].reply.request.ticket,
                        polygons.request().ticket,
                    ],
                    from: result.from(),
                    to: result.to(),
                    after_storage: result.after().storage().to_vec(),
                    after_active: result.after().active_len(),
                    trace: result.trace().to_vec(),
                });
                Ok(None)
            })();
            answers.send(result).unwrap();
        }
    });
    (
        data,
        WarmAuthority {
            send,
            receive,
            service: Some(service),
            other: source::SyntheticMotionAuthority::default(),
            queries: Arc::new(Mutex::new(vec![])),
        },
        fault,
        observations,
    )
}
fn retained(c: &NativeController) -> motion::PathFinderCache {
    let motion::Generator::Chase(chase) = c.world.state().npcs[&2]
        .source_motion
        .as_ref()
        .unwrap()
        .motion
        .generator()
    else {
        panic!("Chase")
    };
    chase.pathfinder_cache.as_deref().unwrap().clone()
}
#[test]
fn synthetic_seed_persists_then_runtime_owned_loaded_warm_dependency_rolls_back_exactly() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let h = Harness::new();
    let (data, authority, fault, observations) = warm_setup();
    let requests = Arc::clone(&authority.queries);
    let mut c = create(&h, data.clone(), authority);
    c.initial_events().unwrap();
    let genesis = c.world.clone();
    pump_to(&mut c, 4);
    let cache = retained(&c);
    assert_eq!(cache.source, motion::RouteSource::SyntheticFixture);
    assert_eq!(cache.revision, 1);
    assert_eq!(cache.active_len, 2);
    assert_eq!(cache.poly_refs.len(), 74);
    let before = c.world.checkpoint().unwrap();
    let records = n::decode_records(&n::encode_records(c.world.records()).unwrap()).unwrap();
    let first = route_reply(&records);
    let n::Facts::SourceMotion(n::MotionFacts::Spatial(reply)) = &first.facts else {
        panic!("spatial")
    };
    let motion::Fact::RouteCalculation(calculation) = &reply.fact else {
        panic!("tagged synthetic calculation")
    };
    assert!(calculation.prior_cache.is_none());
    assert_eq!(calculation.after_cache, cache);
    let motion::RouteOutcome::Complete(path) = &calculation.outcome else {
        panic!("synthetic complete")
    };
    assert_eq!(path.source, motion::RouteSource::SyntheticFixture);
    let mut replay = genesis;
    replay.replay(&records).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), before);
    let files = durable_files(&h);
    let generation = c.store.generation();
    let position = c.position_xyz();
    let facing = c.facing();
    let npc = c.world.state().npcs[&2].id;
    let guid = c.projection.npc_guid(npc);
    // Player movement is explicitly synthetic in this prerequisite test.
    let mut moved = position;
    moved[1] += 6.;
    c.receive(move_to(moved)).unwrap();
    let mut pending = None;
    for (attempt, mode) in [0, 1, 0].into_iter().enumerate() {
        fault.store(mode, Ordering::SeqCst);
        if attempt != 0 {
            c.retry_tick().unwrap();
        }
        let out = c.pump(if attempt == 0 {
            c.tick_duration()
        } else {
            Duration::ZERO
        });
        assert_eq!(out.ticks, 0);
        assert!(out.events.is_empty());
        assert!(matches!(out.pause.unwrap().kind, PauseKind::Authority));
        assert_eq!(c.world.checkpoint().unwrap(), before);
        assert_eq!(c.world.records(), records);
        assert_eq!(retained(&c), cache);
        assert_eq!(durable_files(&h), files);
        assert_eq!(c.store.generation(), generation);
        assert_eq!(c.position_xyz(), position);
        assert_eq!(c.facing(), facing);
        assert_eq!(c.projection.npc_guid(npc), guid);
        let inputs = c.pending.as_ref().unwrap().inputs.clone();
        if let Some(expected) = &pending {
            assert_eq!(&inputs, expected);
        } else {
            assert_eq!(inputs.len(), 1);
            pending = Some(inputs);
        }
        let count = requests.lock().unwrap().len();
        assert_eq!(c.pump(Duration::from_secs(60)).ticks, 0);
        assert_eq!(requests.lock().unwrap().len(), count);
    }
    let requests = requests.lock().unwrap();
    let warm: Vec<_> = requests
        .iter()
        .filter(|q| route_query(q).is_some_and(|r| r.request.prior_cache.is_some()))
        .collect();
    assert_eq!(warm.len(), 3);
    assert!(warm.iter().all(|q| *q == warm[0]));
    let prior = route_query(warm[0])
        .unwrap()
        .request
        .prior_cache
        .as_deref()
        .unwrap();
    assert_eq!(prior, &cache);
    assert_eq!(warm[0].binding.tick, 5);
    let observed = observations.lock().unwrap();
    assert_eq!(observed.len(), 2);
    assert_eq!(observed[0], observed[1]);
    assert_eq!(observed[0].context, *warm[0]);
    assert_eq!(observed[0].tickets, [51, 52, 53]);
    assert_eq!(observed[0].after_active, 1);
    assert_eq!(observed[0].after_storage, cache.poly_refs);
    assert!(!observed[0].trace.is_empty());
    drop(observed);
    drop(requests);
    let digest = c.world.state_digest();
    c.shutdown().unwrap();
    drop(c);
    let (_, authority, _, _) = warm_setup();
    let reopened = enter(&h, Entry::Open { world: [8; 16] }, data, authority);
    assert_eq!(reopened.world.state_digest(), digest);
    assert_eq!(retained(&reopened), cache);
    assert_eq!(reopened.tick(), 4);
    assert_eq!(reopened.world.state().next_sequence, 0);
}
#[test]
fn warm_geometry_or_retained_ref_binding_failure_is_terminal_without_mutation() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    for mode in [2, 3] {
        let h = Harness::new();
        let (data, authority, fault, observations) = warm_setup();
        let queries = Arc::clone(&authority.queries);
        let mut c = create(&h, data, authority);
        c.initial_events().unwrap();
        pump_to(&mut c, 4);
        let before = c.world.checkpoint().unwrap();
        let records = c.world.records().to_vec();
        let files = durable_files(&h);
        let generation = c.store.generation();
        let cache = retained(&c);
        let position = c.position_xyz();
        let facing = c.facing();
        let npc = c.world.state().npcs[&2].id;
        let guid = c.projection.npc_guid(npc);
        fault.store(mode, Ordering::SeqCst);
        let mut moved = position;
        moved[1] += 6.;
        c.receive(move_to(moved)).unwrap();
        let out = c.pump(c.tick_duration());
        assert_eq!(out.ticks, 0);
        assert!(out.events.is_empty());
        assert!(matches!(out.pause.unwrap().kind, PauseKind::Simulation));
        let pending = c.pending.as_ref().unwrap().inputs.clone();
        assert_eq!(pending.len(), 1);
        assert_eq!(c.world.checkpoint().unwrap(), before);
        assert_eq!(c.world.records(), records);
        assert_eq!(retained(&c), cache);
        assert_eq!(durable_files(&h), files);
        assert_eq!(c.store.generation(), generation);
        assert_eq!(c.position_xyz(), position);
        assert_eq!(c.facing(), facing);
        assert_eq!(c.projection.npc_guid(npc), guid);
        assert!(observations.lock().unwrap().is_empty());
        assert!(matches!(c.retry_tick(), Err(Error::Paused)));
        let count = queries.lock().unwrap().len();
        c.pump(Duration::from_secs(60));
        assert_eq!(queries.lock().unwrap().len(), count);
        assert_eq!(c.pending.as_ref().unwrap().inputs, pending);
        assert_eq!(c.world.checkpoint().unwrap(), before);
        assert_eq!(durable_files(&h), files);
    }
}
