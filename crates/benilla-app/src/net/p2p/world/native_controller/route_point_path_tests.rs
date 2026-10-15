// SPDX-License-Identifier: GPL-3.0-or-later
//! SyntheticFixture initial calculation followed by a real loaded terminal point-path
//! dependency at the runtime-owned warm request. Neither is a source Route producer.
use super::*;
use crate::net::p2p::native_geometry::{
    authored_point_path_revision, authored_route_smoothing, PointPathConfiguration,
    PointPathRequest, SourceRoutePointPathProvider, SourceSmoothRequest, WarmCacheConfiguration,
};
use std::sync::mpsc;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Observation {
    context: n::AuthorityRequest,
    owner: [u32; 3],
    tickets: [u64; 4],
    from: [u32; 3],
    to: [u32; 3],
    after_storage: Vec<u64>,
    after_active: usize,
    trace: Vec<benilla_native_nav::SourceWarmTrace>,
    phase: benilla_native_nav::SourceWarmPolyPhase,
    polygons: [u64; 2],
    points: Vec<[u32; 3]>,
    smooth_points: Vec<[u32; 3]>,
    smooth_status: Option<u32>,
    smooth_stop: Option<benilla_native_nav::SourceSmoothStop>,
    actual_end: [u32; 3],
    path_type: u32,
    diagnostics: benilla_native_nav::SourcePointPathDiagnostics,
}
struct PointPathAuthority {
    send: mpsc::Sender<Option<n::AuthorityRequest>>,
    receive: mpsc::Receiver<Result<Option<motion::PathFinderCache>, Failure>>,
    service: Option<std::thread::JoinHandle<()>>,
    other: source::SyntheticMotionAuthority,
    queries: Arc<Mutex<Vec<n::AuthorityRequest>>>,
    failures: Arc<Mutex<Vec<Failure>>>,
}
impl n::Authority for PointPathAuthority {
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
            .map_err(|error| {
                self.failures.lock().unwrap().push(error);
                match error {
                    Failure::Binding => n::NativeError::AuthorityBinding,
                    Failure::Invalid => n::NativeError::InvalidInput,
                    _ => n::NativeError::AuthorityUnavailable(
                        "loaded terminal point-path dependency",
                    ),
                }
            })?;
        let Some(seed) = seed else {
            // The terminal smooth-calculation dependency still lacks source owner
            // dispatch and continuous segment evidence. No Complete or NoPath fact.
            return Err(n::NativeError::AuthorityUnavailable(
                "source owner route after terminal point path unavailable",
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
impl Drop for PointPathAuthority {
    fn drop(&mut self) {
        let _ = self.send.send(None);
        if let Some(service) = self.service.take() {
            service.join().unwrap();
        }
    }
}
fn point_path_setup(
    optimize_path: bool,
    crossing: bool,
) -> (
    n::Profile,
    PointPathAuthority,
    Arc<AtomicU64>,
    Arc<Mutex<Vec<Observation>>>,
) {
    let (mut data, base) = super::setup(true);
    let products = authored_route_smoothing();
    let liquid = products.load_dry_liquid().unwrap();
    let navigation = products.load_navigation().unwrap();
    let config = PointPathConfiguration {
        warm: WarmCacheConfiguration {
            budget: benilla_native_nav::SourceWarmCacheBudget::default(),
        },
        optimize_path,
        normalize_z: false,
        ignore_normalization: false,
        budget: benilla_native_nav::SourcePointPathBudget::default(),
    };
    let binding = liquid.binding();
    data.map_id = binding.map;
    data.content_digest = binding.content;
    data.geometry_revision =
        authored_point_path_revision(binding, navigation.binding(), config).unwrap();
    data.npcs[0].ai.map_id = binding.map;
    data.npcs[0].ai.content_revision = ai::movement::Revision(binding.content);
    let (owner_y, player_y) = if crossing {
        (-280., -266.7)
    } else {
        (-400., -390.)
    };
    data.npcs[0].ai.spawn = ai::Point::new(-400., owner_y, 0.).unwrap();
    data.player.start = ai::Point::new(-400., player_y, 0.).unwrap();
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
        let point_limit = lineage.cache().point_limit();
        let filter = lineage.cache().filter();
        assert_eq!(active, 2);
        assert_eq!(storage.len(), 74);
        let provider = SourceRoutePointPathProvider::bind(
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
                        point_limit: point_limit as u16,
                        path_type: 1,
                        requested_end: route.request.goal,
                        include_flags: filter.include_flags,
                        exclude_flags: filter.exclude_flags,
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
                let request = PointPathRequest {
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
                let warm = dependency.warm_dependency();
                let filter = warm.filter_dependency();
                assert_eq!(filter.request().context, context);
                let polygons = warm.polygon_dependency();
                let result = polygons.result();
                let terminal = dependency.point_dependency();
                let points = terminal.result();
                assert_eq!(terminal.request().warm, polygons.request());
                assert_eq!(points.warm(), result);
                assert_eq!(points.config(), terminal.request().configuration);
                assert_eq!(points.config().optimize_path, optimize_path);
                assert!(!points.config().normalize_z);
                assert!(!points.config().ignore_normalization);
                assert!(!points.config().force_destination);
                assert_eq!(
                    points.config().previous_path_type,
                    route.request.prior_cache.as_ref().unwrap().path_type
                );
                assert_eq!(result.from(), point_bits(route.request.origin));
                assert_eq!(result.to(), point_bits(route.request.goal));
                assert_eq!(result.after().active_len(), if crossing { 2 } else { 1 });
                assert_eq!(points.after(), result.after());
                assert_eq!(result.after().storage(), storage);
                seen.lock().unwrap().push(Observation {
                    context: context.clone(),
                    owner: filter.owner_position(),
                    tickets: [
                        filter.liquid_observations()[0].reply.request.ticket,
                        filter.liquid_observations()[1].reply.request.ticket,
                        polygons.request().ticket,
                        terminal.request().ticket,
                    ],
                    from: result.from(),
                    to: result.to(),
                    after_storage: result.after().storage().to_vec(),
                    after_active: result.after().active_len(),
                    trace: result.trace().to_vec(),
                    phase: result.phase(),
                    polygons: [result.start().polygon, result.end().polygon],
                    points: points.points().to_vec(),
                    smooth_points: points.smooth_points().to_vec(),
                    smooth_status: points.smooth_status(),
                    smooth_stop: points.smooth_stop(),
                    actual_end: points.actual_end(),
                    path_type: points.path_type(),
                    diagnostics: points.diagnostics().clone(),
                });
                Ok(None)
            })();
            answers.send(result).unwrap();
        }
    });
    (
        data,
        PointPathAuthority {
            send,
            receive,
            service: Some(service),
            other: source::SyntheticMotionAuthority::default(),
            queries: Arc::new(Mutex::new(vec![])),
            failures: Arc::new(Mutex::new(vec![])),
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
fn synthetic_seed_replays_then_loaded_terminal_crossing_dependency_rolls_back_exactly() {
    for optimize_path in [false, true] {
        exercise_terminal_dependency(optimize_path, true);
    }
}

#[test]
fn same_polygon_terminal_dependency_keeps_smoothing_absent_and_never_publishes() {
    exercise_terminal_dependency(true, false);
}

fn exercise_terminal_dependency(optimize_path: bool, crossing: bool) {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let h = Harness::new();
    let (data, authority, fault, observations) = point_path_setup(optimize_path, crossing);
    let requests = Arc::clone(&authority.queries);
    let failures = Arc::clone(&authority.failures);
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
    assert_eq!(calculation.source, motion::RouteSource::SyntheticFixture);
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
    assert_eq!(*failures.lock().unwrap(), [Failure::MissingGeometry]);
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
    assert_eq!(warm[0].binding.world, [8; 16]);
    assert_eq!(warm[0].binding.profile, data.digest());
    assert_eq!(warm[0].binding.geometry, data.geometry_revision);
    assert_eq!(warm[0].binding.npc, Some(npc));
    assert_eq!(
        route_query(warm[0]).unwrap().owner_position,
        route_query(warm[0]).unwrap().request.origin
    );
    let observed = observations.lock().unwrap();
    assert_eq!(observed.len(), 2);
    assert_eq!(observed[0], observed[1]);
    assert_eq!(observed[0].context, *warm[0]);
    assert_eq!(observed[0].tickets, [51, 52, 53, 54]);
    assert_eq!(
        observed[0].owner,
        point_bits(route_query(warm[0]).unwrap().owner_position)
    );
    assert_eq!(observed[0].after_active, if crossing { 2 } else { 1 });
    let terminal = &observed[0];
    assert_eq!(terminal.actual_end, terminal.to);
    assert_eq!(terminal.path_type, 1);
    assert_eq!(terminal.points.first(), Some(&terminal.from));
    assert_eq!(terminal.points.last(), Some(&terminal.to));
    if crossing {
        assert!(f32::from_bits(terminal.from[1]) < -266.666_66);
        assert!(f32::from_bits(terminal.to[1]) > -266.666_66);
        assert_eq!(terminal.phase, benilla_native_nav::SourceWarmPolyPhase::Cut);
        assert_ne!(terminal.polygons[0], terminal.polygons[1]);
        assert_eq!(terminal.smooth_status, Some(1 << 30));
        assert_eq!(
            terminal.smooth_stop,
            Some(benilla_native_nav::SourceSmoothStop::TargetReached)
        );
        assert!(terminal.diagnostics.smoothing.primitive_calls > 0);
        assert!(terminal.diagnostics.smoothing.iterations > 0);
        assert!(terminal.smooth_points.len() > 2);
        assert!(terminal.smooth_points[1..terminal.smooth_points.len() - 1]
            .iter()
            .any(|p| p[2] == 0.5f32.to_bits()));
        if optimize_path {
            assert!(terminal.points.len() < terminal.smooth_points.len());
            assert!(terminal.diagnostics.postprocess_primitives > terminal.smooth_points.len() + 1);
        } else {
            assert_eq!(terminal.points, terminal.smooth_points);
            assert_eq!(
                terminal.diagnostics.postprocess_primitives,
                terminal.smooth_points.len() + 1
            );
        }
    } else {
        assert_eq!(
            terminal.phase,
            benilla_native_nav::SourceWarmPolyPhase::SamePolygon
        );
        assert_eq!(terminal.polygons[0], terminal.polygons[1]);
        assert_eq!(terminal.smooth_status, None);
        assert_eq!(terminal.smooth_stop, None);
        assert!(terminal.smooth_points.is_empty());
        assert_eq!(terminal.points, [terminal.from, terminal.to]);
        assert_eq!(
            terminal.diagnostics.smoothing,
            benilla_native_nav::SourceSmoothingDiagnostics::default()
        );
        assert_eq!(terminal.diagnostics.postprocess_primitives, 1);
    }
    assert_eq!(observed[0].after_storage, cache.poly_refs);
    assert!(!observed[0].trace.is_empty());
    drop(observed);
    drop(requests);
    let digest = c.world.state_digest();
    c.shutdown().unwrap();
    drop(c);
    let (_, authority, _, _) = point_path_setup(optimize_path, crossing);
    let reopened = enter(&h, Entry::Open { world: [8; 16] }, data, authority);
    assert_eq!(reopened.world.state_digest(), digest);
    assert_eq!(retained(&reopened), cache);
    assert_eq!(reopened.tick(), 4);
    assert_eq!(reopened.world.state().next_sequence, 0);
}
#[test]
fn point_path_geometry_or_retained_ref_binding_failure_is_terminal_without_mutation() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    for mode in [2, 3] {
        let h = Harness::new();
        let (data, authority, fault, observations) = point_path_setup(true, true);
        let queries = Arc::clone(&authority.queries);
        let failures = Arc::clone(&authority.failures);
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
        assert_eq!(*failures.lock().unwrap(), [Failure::Binding]);
        assert!(matches!(c.retry_tick(), Err(Error::Paused)));
        let count = queries.lock().unwrap().len();
        c.pump(Duration::from_secs(60));
        assert_eq!(queries.lock().unwrap().len(), count);
        assert_eq!(c.pending.as_ref().unwrap().inputs, pending);
        assert_eq!(c.world.checkpoint().unwrap(), before);
        assert_eq!(durable_files(&h), files);
    }
}
