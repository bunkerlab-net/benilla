// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual dry Chase selection, cached player environment and loaded straight/retry
//! dependencies cross the real controller. Initial routes and other authorities are synthetic.
use super::*;
use crate::net::p2p::native_geometry::{
    authored_chase_gate_revision, authored_dry_chase_environment,
    authored_dry_chase_snake_products, core::PlayerEnvironmentPolicy, ChaseGateConfiguration,
    ChaseGateRejection, ChaseGateRequest, ChaseGateSelection, SourcePlayerEnvironmentProvider,
    SourceRouteChaseGateProvider,
};
use n::synthetic_player_model_fixture as player;

#[derive(Clone, Debug, PartialEq, Eq)]
struct GateObservation {
    context: n::AuthorityRequest,
    selection: ChaseGateSelection,
    // Actual dependency tickets, in source execution order. A missing branch has no ticket.
    tickets: Vec<u64>,
    target_water: Option<n::PlayerEnvironmentTarget>,
    straight: Option<Observation>,
}

enum Answer {
    Environment(n::Facts),
    SyntheticSeed(motion::PathFinderCache),
    RouteUnavailable,
}
struct GateAuthority {
    send: mpsc::Sender<Option<n::AuthorityRequest>>,
    receive: mpsc::Receiver<Result<Answer, Failure>>,
    service: Option<std::thread::JoinHandle<()>>,
    other: source::SyntheticMotionAuthority,
    queries: Arc<Mutex<Vec<n::AuthorityRequest>>>,
    failures: Arc<Mutex<Vec<Failure>>>,
    goal: ai::Point,
    seeded: bool,
    visits: Arc<AtomicBool>,
}
impl n::Authority for GateAuthority {
    fn resolve(
        &mut self,
        context: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        self.queries.lock().unwrap().push(context.clone());
        self.other.visits = Some(self.visits.load(Ordering::SeqCst));
        if route_query(context).is_none()
            && !matches!(context.query, n::Query::PlayerEnvironment(_))
        {
            let mut reply = self.other.resolve(context)?;
            if self.seeded
                && matches!(
                    context.query,
                    n::Query::SourceMotion(n::MotionQuery::NearPoint(_))
                )
            {
                let n::Facts::SourceMotion(n::MotionFacts::Spatial(spatial)) = &mut reply.facts
                else {
                    unreachable!()
                };
                // The surrounding NearPoint answer is an explicit synthetic fixture;
                // the runtime constructs the resulting Route and all of its witnesses.
                spatial.fact = motion::Fact::NearPoint(self.goal);
            }
            return Ok(reply);
        }
        self.send.send(Some(context.clone())).unwrap();
        let answer = self
            .receive
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .map_err(|failure| {
                self.failures.lock().unwrap().push(failure);
                match failure {
                    Failure::Binding => n::NativeError::AuthorityBinding,
                    Failure::Invalid => n::NativeError::InvalidInput,
                    _ => n::NativeError::AuthorityUnavailable("loaded Chase gate dependency"),
                }
            })?;
        match answer {
            Answer::Environment(facts) => Ok(n::AuthorityReply {
                request: context.clone(),
                facts,
            }),
            Answer::RouteUnavailable => Err(n::NativeError::AuthorityUnavailable(
                "source owner route remains unavailable after Chase gate/straight dependencies",
            )),
            Answer::SyntheticSeed(seed) => {
                assert!(route_query(context).unwrap().request.prior_cache.is_none());
                assert_eq!(seed.source, motion::RouteSource::SyntheticFixture);
                let mut reply = self.other.resolve(context)?;
                let n::Facts::SourceMotion(n::MotionFacts::Spatial(spatial)) = &mut reply.facts
                else {
                    unreachable!()
                };
                let motion::Fact::Route(outcome) = &spatial.fact else {
                    unreachable!()
                };
                spatial.fact = motion::Fact::RouteCalculation(Box::new(motion::RouteCalculation {
                    wander_no_path: None,
                    source: motion::RouteSource::SyntheticFixture,
                    outcome: outcome.clone(),
                    prior_cache: None,
                    after_cache: seed,
                }));
                self.seeded = true;
                Ok(reply)
            }
        }
    }
    fn observe_derived(&mut self, reply: &n::AuthorityReply) -> Result<(), n::NativeError> {
        self.other.observe_derived(reply)
    }
}
impl Drop for GateAuthority {
    fn drop(&mut self) {
        let _ = self.send.send(None);
        if let Some(service) = self.service.take() {
            let result = service.join();
            if !std::thread::panicking() {
                result.unwrap();
            }
        }
    }
}
struct Controls {
    fault: Arc<AtomicU64>,
    queries: Arc<Mutex<Vec<n::AuthorityRequest>>>,
    failures: Arc<Mutex<Vec<Failure>>>,
    observations: Arc<Mutex<Vec<GateObservation>>>,
    environment: Arc<Mutex<Vec<n::AuthorityReply>>>,
    visits: Arc<AtomicBool>,
}
fn setup(
    h: &Harness,
    budget: u8,
    goal: ai::Point,
    tick_ms: u32,
) -> (n::Profile, GateAuthority, Controls) {
    let (_, base) = super::super::setup(true);
    let mut profile = player::unowned_profile();
    let products = authored_dry_chase_snake_products();
    let projection = products.load_dry_route_projection().unwrap();
    let binding = projection.liquid().binding();
    let config = ChaseGateConfiguration {
        straight: configuration(true, budget),
        line_of_sight_enabled: true,
    };
    profile.map_id = binding.map;
    profile.content_digest = binding.content;
    profile.geometry_revision = authored_chase_gate_revision(
        binding,
        projection.navigation().binding(),
        projection.static_vmap().binding(),
        config,
    )
    .unwrap();
    profile.tick_ms = tick_ms;
    if tick_ms == 25 {
        profile
            .source_motion
            .as_mut()
            .unwrap()
            .relocation_aggro_delay_ms = 125;
    }
    profile.npcs[0].ai.map_id = binding.map;
    profile.npcs[0].ai.content_revision = ai::movement::Revision(binding.content);
    profile.npcs[0].ai.spawn = snake_from();
    profile.player.start = snake_to();
    // Aggro, movement and initial warm route are synthetic prerequisites, not physics.
    profile.npcs[0].ai.detection = fixture::real(100.);
    profile
        .source_motion
        .as_mut()
        .unwrap()
        .npcs
        .get_mut(&2)
        .unwrap()
        .creature_detection_range = 100;
    profile
        .initialize_creature_capabilities(source::creature_capability_sources(&profile))
        .unwrap();
    profile.npcs[0].display_id = 34;
    let models = model::catalog();
    profile
        .initialize_creature_collision_widths(n::CreatureCollisionWidthRows::Complete(vec![
            n::CreatureCollisionWidthSource::from_initial_model(2, model::selection(), &models)
                .unwrap(),
        ]))
        .unwrap();
    profile.initialize_creature_collision_heights().unwrap();
    profile
        .initialize_player_collision_height(&player::catalog(), &models)
        .unwrap();
    let environment = authored_dry_chase_environment(Arc::clone(&products));
    assert_eq!(
        environment.environment_binding().inventory,
        binding.inventory
    );
    profile
        .initialize_player_environment(
            environment.environment_binding(),
            environment.environment_catalog().clone(),
        )
        .unwrap();
    drop(environment);
    // Native navigation handles are never transferred between threads or reconstructed
    // from serialized references. The worker loads and owns its actual projection below.
    drop(projection);
    let player_id = Selection::from_profile(&h.profile, &h.character)
        .unwrap()
        .player;
    profile
        .initialize_interactions(n::synthetic_combat_fixture::source_rows(&profile))
        .unwrap();
    let mut session = base.session;
    session.profile = profile.digest();
    session.content = profile.content_digest;
    session.geometry = profile.geometry_revision;
    session.map = profile.map_id;
    let motion = profile.source_motion.as_ref().unwrap();
    session.bodies.get_mut(&2).unwrap().height =
        motion.npc(2).unwrap().npc_policy.settings.collision_height;
    session.player.as_mut().unwrap().body.height = motion.player_collision_height;
    session.participants = Some(NonRenderParticipants::authored_fixture(
        binding.content,
        binding.map,
    ));
    session.player_environment =
        Some(PlayerEnvironmentPolicy::from_profile(&profile, player_id).unwrap());
    let fault = Arc::new(AtomicU64::new(0));
    let queries = Arc::new(Mutex::new(vec![]));
    let failures = Arc::new(Mutex::new(vec![]));
    let observations = Arc::new(Mutex::new(vec![]));
    let environment = Arc::new(Mutex::new(vec![]));
    let visits = Arc::new(AtomicBool::new(true));
    let mode = Arc::clone(&fault);
    let seen = Arc::clone(&observations);
    let env_seen = Arc::clone(&environment);
    let worker_profile = profile.clone();
    let (send, requests) = mpsc::channel::<Option<n::AuthorityRequest>>();
    let (answers, receive) = mpsc::channel();
    let parent_thread = std::thread::current().id();
    let service = std::thread::spawn(move || {
        assert_ne!(std::thread::current().id(), parent_thread);
        let projection = products.load_dry_route_projection().unwrap();
        let nav_binding = projection.navigation().binding();
        let lineage = projection
            .navigation()
            .seed_bounded_warm_lineage(
                SourceSmoothRequest {
                    snapshot: nav_binding,
                    ticket: 1,
                    from: point_bits(snake_to()),
                    to: point_bits(snake_to()),
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
        assert_eq!(active, 1);
        assert_eq!(storage.len(), 74);
        let environment = SourcePlayerEnvironmentProvider::bind(
            &session,
            &worker_profile,
            player_id,
            authored_dry_chase_environment(products),
        )
        .unwrap();
        let gate = SourceRouteChaseGateProvider::bind(
            &session,
            &worker_profile,
            player_id,
            projection,
            lineage,
            config,
        )
        .unwrap();
        while let Some(context) = requests.recv().unwrap() {
            if matches!(context.query, n::Query::PlayerEnvironment(_)) {
                let answer = environment
                    .resolve(&session, &context, Instant::now() + Duration::from_secs(5))
                    .map(|facts| {
                        env_seen.lock().unwrap().push(n::AuthorityReply {
                            request: context,
                            facts: facts.clone(),
                        });
                        Answer::Environment(facts)
                    });
                answers.send(answer).unwrap();
                continue;
            }
            let route = route_query(&context).unwrap();
            if route.request.prior_cache.is_none() {
                // Real polygon identity is not a complete route proof. Only this
                // explicitly synthetic fixture seed may be accepted by the runtime.
                answers
                    .send(Ok(Answer::SyntheticSeed(motion::PathFinderCache {
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
            match mode {
                2 => actual_session.geometry[0] ^= 1,
                3 => actual_session.player_environment = None,
                4..=13 | 17..=24 => {
                    let n::Query::SourceMotion(n::MotionQuery::Route(route)) =
                        &mut actual_context.query
                    else {
                        unreachable!()
                    };
                    match mode {
                        4 => route.collision_width = None,
                        5 => route.collision_height = None,
                        6 => route.target_player_height = None,
                        7 => route.target_player_water = None,
                        8 => {
                            route
                                .request
                                .prior_cache
                                .as_mut()
                                .unwrap()
                                .binding
                                .owner_generation += 1
                        }
                        9 => {
                            let water = route.target_player_water.as_mut().unwrap();
                            let mut value = serde_json::to_value(&*water).unwrap();
                            value["tick"] = 999.into();
                            *water = serde_json::from_value(value).unwrap();
                        }
                        10 => {
                            let water = route.target_player_water.as_mut().unwrap();
                            let mut value = serde_json::to_value(&*water).unwrap();
                            value["in_water"] = true.into();
                            *water = serde_json::from_value(value).unwrap();
                        }
                        11 => {
                            let height = route.collision_height.as_mut().unwrap();
                            let mut value = serde_json::to_value(&*height).unwrap();
                            value["npc"]["generation"] = 1.into();
                            *height = serde_json::from_value(value).unwrap();
                        }
                        12 => route.request.binding.now_ms += u64::from(worker_profile.tick_ms),
                        13 => route.request.prior_cache.as_mut().unwrap().poly_refs[0] ^= 1,
                        17..=24 => edit_owner_ignore_pathfinding(route, (mode - 17) as u8),
                        _ => unreachable!(),
                    }
                }
                _ => {}
            }
            let result = (|| {
                let bound = gate.begin(&actual_session, &actual_context)?;
                let mut request = ChaseGateRequest {
                    snapshot: gate.binding(),
                    context: actual_context,
                    first_ticket: 51,
                };
                match mode {
                    14 => request.snapshot.vmap.inventory[0] ^= 1,
                    15 => request.first_ticket = u64::MAX - 7,
                    16 => request.context.binding.tick += 1,
                    _ => {}
                }
                let deadline = Instant::now()
                    + if mode == 1 {
                        Duration::ZERO
                    } else {
                        Duration::from_secs(5)
                    };
                let dependency = bound.query(request.clone(), deadline)?;
                assert_eq!(dependency.request(), &request);
                assert_eq!(dependency.model().selection, model::selection());
                assert_eq!(
                    dependency.model().selection.combat_reach().bits(),
                    worker_profile.npcs[0].ai.combat_reach.bits()
                );
                assert_eq!(
                    dependency.model().width,
                    assert_width(route, &worker_profile)
                );
                assert_eq!(
                    &dependency.model().owner_height,
                    route.require_collision_height(&worker_profile).unwrap()
                );
                assert_eq!(
                    &dependency.model().target_height,
                    route
                        .require_target_collision_height(&worker_profile, session.world, player_id)
                        .unwrap()
                );
                let mut tickets = vec![];
                if let Some(los) = dependency.line_of_sight() {
                    assert_eq!(los.request.snapshot, request.snapshot.vmap);
                    assert!(!los.request.ignore_m2);
                    let owner = point_bits(route.owner_position);
                    let goal = point_bits(route.request.goal);
                    assert_eq!(
                        los.request.from,
                        [
                            owner[0],
                            owner[1],
                            (f32::from_bits(owner[2])
                                + f32::from_bits(dependency.model().owner_height.height().bits()))
                            .to_bits()
                        ]
                    );
                    assert_eq!(
                        los.request.to,
                        [
                            goal[0],
                            goal[1],
                            (f32::from_bits(goal[2])
                                + f32::from_bits(dependency.model().target_height.height().bits()))
                            .to_bits()
                        ]
                    );
                    tickets.push(los.request.ticket);
                }
                if let Some(water) = dependency.owner_water() {
                    assert_eq!(
                        water.reply.request.position,
                        point_bits(route.owner_position)
                    );
                    assert_eq!(
                        water.reply.request.snapshot,
                        request.snapshot.straight.liquid
                    );
                    assert_eq!(water.required_type, 15);
                    assert_eq!(water.collision_height_bits, 2.03128_f32.to_bits());
                    tickets.push(water.reply.request.ticket);
                }
                if let Some(target) = dependency.target_water() {
                    assert_eq!(
                        target,
                        route
                            .require_target_water(&worker_profile, session.world, player_id)
                            .unwrap()
                    );
                }
                let straight = dependency.straight_dependency().map(|straight| {
                    let records = Mutex::new(vec![]);
                    observe_dependency(straight, &worker_profile, &context, &records);
                    let observation = records.into_inner().unwrap().pop().unwrap();
                    tickets.extend_from_slice(&observation.tickets);
                    observation
                });
                seen.lock().unwrap().push(GateObservation {
                    context: context.clone(),
                    selection: dependency.selection(),
                    tickets,
                    target_water: dependency.target_water().cloned(),
                    straight,
                });
                Ok(Answer::RouteUnavailable)
            })();
            answers.send(result).unwrap();
        }
    });
    let authority = GateAuthority {
        send,
        receive,
        service: Some(service),
        other: source::SyntheticMotionAuthority::default(),
        queries: Arc::clone(&queries),
        failures: Arc::clone(&failures),
        goal,
        seeded: false,
        visits: Arc::clone(&visits),
    };
    (
        profile,
        authority,
        Controls {
            fault,
            queries,
            failures,
            observations,
            environment,
            visits,
        },
    )
}
fn create(h: &Harness, budget: u8, goal: ai::Point) -> (NativeController, n::Profile, Controls) {
    let (profile, authority, controls) = setup(h, budget, goal, 100);
    let controller = NativeController::enter_component_test(
        &h.root,
        &h.profile,
        &h.character,
        Entry::Create {
            world: [8; 16],
            seed: 7,
        },
        profile.clone(),
        &WidthCatalog,
        authority,
    )
    .unwrap();
    (controller, profile, controls)
}
fn stage_cached_replan(c: &mut NativeController) {
    let mut pose = c.position_xyz();
    pose[1] += 6.;
    c.receive(move_to(pose)).unwrap();
    pose[1] += 1.;
    c.receive(move_to(pose)).unwrap();
}
fn warm_queries(controls: &Controls) -> Vec<n::AuthorityRequest> {
    controls
        .queries
        .lock()
        .unwrap()
        .iter()
        .filter(|q| route_query(q).is_some_and(|route| route.request.prior_cache.is_some()))
        .cloned()
        .collect()
}

#[test]
fn cached_player_gate_and_actual_straight_retry_leave_unavailable_route_transaction_unchanged() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let h = Harness::new();
    let (mut c, profile, controls) = create(&h, 0, snake_to());
    c.initial_events().unwrap();
    pump_to(&mut c, 4);
    assert_eq!(retained(&c).source, motion::RouteSource::SyntheticFixture);
    stage_cached_replan(&mut c);
    let before = boundary(&c, &h);
    let mut pending = None;
    for (attempt, mode) in [0, 1, 7, 0].into_iter().enumerate() {
        controls.fault.store(mode, Ordering::SeqCst);
        if attempt > 0 {
            c.retry_tick().unwrap();
        }
        let out = c.pump(if attempt == 0 {
            c.tick_duration()
        } else {
            Duration::ZERO
        });
        assert_eq!(out.ticks, 0);
        assert!(out.events.is_empty());
        assert!(out.saved.is_none());
        assert!(matches!(out.pause.unwrap().kind, PauseKind::Authority));
        assert_eq!(boundary(&c, &h), before);
        assert!(c.outbox.is_none());
        assert!(c.actions.is_empty());
        let current = c.pending.as_ref().unwrap();
        let current = (current.inputs.clone(), current.moving);
        if let Some(expected) = &pending {
            assert_eq!(&current, expected);
        } else {
            assert_eq!(current.0.len(), 2);
            pending = Some(current);
        }
        let count = controls.queries.lock().unwrap().len();
        assert_eq!(c.pump(Duration::from_secs(60)).ticks, 0);
        assert_eq!(controls.queries.lock().unwrap().len(), count);
        assert_eq!(boundary(&c, &h), before);
    }
    assert_eq!(
        *controls.failures.lock().unwrap(),
        [Failure::Deadline, Failure::MissingGeometry]
    );
    let queries = warm_queries(&controls);
    assert_eq!(queries.len(), 4);
    assert!(queries.iter().all(|q| q == &queries[0]));
    let route = route_query(&queries[0]).unwrap();
    let owner_bit = assert_owner_ignore_pathfinding(route, &profile);
    assert_eq!(route.request.prior_cache.as_deref(), Some(&before.cache));
    let target = route
        .require_target_water(&profile, [8; 16], c.world.state().player)
        .unwrap();
    assert_eq!(target.moment(), n::PlayerEnvironmentMoment::AfterCountdown);
    assert!(!target.in_water());
    let cached = serde_json::to_value(target).unwrap();
    let cached_pose = &cached["snapshot"]["state"]["cached_query"]["position"];
    let current_pose = point_bits(target.target().pose.position);
    assert_ne!(*cached_pose, serde_json::json!(current_pose));
    assert_eq!(
        cached_pose[1],
        serde_json::json!((f32::from_bits(current_pose[1]) - 1.).to_bits())
    );
    assert_ne!(
        route.request.goal,
        target.target().pose.position,
        "LOS must use the goal, not the current target pose"
    );
    let environment = controls.environment.lock().unwrap();
    assert_eq!(
        environment.len(),
        4,
        "exactly one position sample per retry, never a gate target resample"
    );
    assert!(environment.iter().all(|reply| reply == &environment[0]));
    assert!(environment
        .iter()
        .all(|reply| matches!(reply.facts, n::Facts::PlayerEnvironment(_))));
    let observations = controls.observations.lock().unwrap();
    assert_eq!(observations.len(), 2);
    assert_eq!(observations[0], observations[1]);
    let observed = &observations[0];
    assert_eq!(observed.selection, ChaseGateSelection::Straight);
    assert_eq!(observed.context, queries[0]);
    assert_eq!(observed.target_water.as_ref(), Some(target));
    assert_eq!(
        observed.tickets,
        [51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61]
    );
    let straight = observed.straight.as_ref().unwrap();
    assert_eq!(straight.owner_ignore_pathfinding, owner_bit);
    assert_eq!(
        straight.fallback.as_ref().unwrap().owner_ignore_pathfinding,
        owner_bit
    );
    assert_eq!(straight.tiles.request.ticket, 53);
    assert_eq!(
        straight.tiles.request.from,
        point_bits(route.request.origin)
    );
    assert_eq!(straight.tiles.request.to, point_bits(route.request.goal));
    assert!(straight.path_type & (8 | 4) != 0);
    assert!(straight.fallback.is_some());
    assert_eq!(straight.fallback.as_ref().unwrap().tiles.request.ticket, 57);
    assert_ne!(straight.after_storage, before.cache.poly_refs);
    assert_eq!(
        boundary(&c, &h),
        before,
        "uncommitted straight terminal cache never replaces retained cache"
    );
}

#[test]
fn numeric_gate_rejection_is_unavailable_without_liquid_straight_or_fake_nopath() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    for (goal, reason) in [
        (
            ai::Point::new(
                snake_from().x.as_f32() + 250.,
                snake_from().y.as_f32(),
                snake_from().z.as_f32(),
            )
            .unwrap(),
            ChaseGateRejection::Distance,
        ),
        (
            ai::Point::new(
                snake_to().x.as_f32(),
                snake_to().y.as_f32(),
                snake_from().z.as_f32() + 10.,
            )
            .unwrap(),
            ChaseGateRejection::Vertical,
        ),
    ] {
        let h = Harness::new();
        let (mut c, _, controls) = create(&h, 0, goal);
        c.initial_events().unwrap();
        pump_to(&mut c, 4);
        stage_cached_replan(&mut c);
        let before = boundary(&c, &h);
        for attempt in 0..2 {
            if attempt > 0 {
                c.retry_tick().unwrap();
            }
            let out = c.pump(if attempt == 0 {
                c.tick_duration()
            } else {
                Duration::ZERO
            });
            assert_eq!(out.ticks, 0);
            assert!(out.events.is_empty());
            assert!(out.saved.is_none());
            assert!(matches!(out.pause.unwrap().kind, PauseKind::Authority));
            assert_eq!(boundary(&c, &h), before);
            assert!(c.outbox.is_none());
        }
        assert!(controls.failures.lock().unwrap().is_empty());
        let observations = controls.observations.lock().unwrap();
        assert_eq!(observations.len(), 2);
        assert_eq!(observations[0], observations[1]);
        assert_eq!(
            observations[0].selection,
            ChaseGateSelection::NormalRouteUnavailable(reason)
        );
        assert!(observations[0].tickets.is_empty());
        assert!(observations[0].target_water.is_none());
        assert!(observations[0].straight.is_none());
        let queries = warm_queries(&controls);
        assert_eq!(queries.len(), 2);
        assert_eq!(queries[0], queries[1]);
    }
}

#[test]
fn missing_and_stale_gate_witnesses_sessions_and_generation_never_publish() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    for mode in 2..=24 {
        let h = Harness::new();
        let (mut c, _, controls) = create(&h, 0, snake_to());
        c.initial_events().unwrap();
        pump_to(&mut c, 4);
        stage_cached_replan(&mut c);
        let before = boundary(&c, &h);
        controls.fault.store(mode, Ordering::SeqCst);
        let out = c.pump(c.tick_duration());
        assert_eq!(out.ticks, 0, "mode {mode}");
        assert!(out.events.is_empty());
        assert!(out.saved.is_none());
        let recoverable = matches!(mode, 4..=7 | 17);
        assert!(
            matches!(out.pause.unwrap().kind, PauseKind::Authority) == recoverable,
            "mode {mode}"
        );
        assert_eq!(boundary(&c, &h), before, "mode {mode}");
        assert!(c.outbox.is_none());
        assert!(controls.observations.lock().unwrap().is_empty());
        assert_eq!(
            *controls.failures.lock().unwrap(),
            [if recoverable {
                Failure::MissingGeometry
            } else if mode == 15 {
                Failure::Invalid
            } else {
                Failure::Binding
            }],
            "mode {mode}"
        );
        if recoverable {
            controls.fault.store(0, Ordering::SeqCst);
            c.retry_tick().unwrap();
            let retried = c.pump(Duration::ZERO);
            assert_eq!(retried.ticks, 0);
            assert!(retried.events.is_empty());
            assert!(matches!(retried.pause.unwrap().kind, PauseKind::Authority));
            assert_eq!(controls.observations.lock().unwrap().len(), 1);
            assert_eq!(warm_queries(&controls)[0], warm_queries(&controls)[1]);
        } else {
            assert!(matches!(c.retry_tick(), Err(Error::Paused)));
        }
        assert_eq!(boundary(&c, &h), before);
    }
}

#[test]
fn loaded_fallback_budget_failure_retries_without_committing_gate_or_terminal_cache() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let h = Harness::new();
    let (mut c, _, controls) = create(&h, 5, snake_to());
    c.initial_events().unwrap();
    pump_to(&mut c, 4);
    stage_cached_replan(&mut c);
    let before = boundary(&c, &h);
    for attempt in 0..2 {
        if attempt > 0 {
            c.retry_tick().unwrap();
        }
        let out = c.pump(if attempt == 0 {
            c.tick_duration()
        } else {
            Duration::ZERO
        });
        assert_eq!(out.ticks, 0);
        assert!(out.events.is_empty());
        assert!(out.saved.is_none());
        assert!(matches!(out.pause.unwrap().kind, PauseKind::Authority));
        assert_eq!(boundary(&c, &h), before);
        assert!(c.outbox.is_none());
    }
    assert_eq!(
        *controls.failures.lock().unwrap(),
        [Failure::MissingGeometry; 2]
    );
    assert!(controls.observations.lock().unwrap().is_empty());
    let queries = warm_queries(&controls);
    assert_eq!(queries.len(), 2);
    assert_eq!(queries[0], queries[1]);
}

#[test]
fn synchronous_player_notify_uses_loaded_pre_countdown_cache_before_synthetic_seed() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let h = Harness::new();
    let (profile, authority, controls) = setup(&h, 0, snake_to(), 25);
    let mut c = NativeController::enter_component_test(
        &h.root,
        &h.profile,
        &h.character,
        Entry::Create {
            world: [8; 16],
            seed: 7,
        },
        profile.clone(),
        &WidthCatalog,
        authority,
    )
    .unwrap();
    c.initial_events().unwrap();
    controls.visits.store(false, Ordering::SeqCst);
    pump_to(&mut c, 8);
    let mut pose = c.position_xyz();
    pose[1] += 1.;
    c.receive(move_to(pose)).unwrap();
    pump_to(&mut c, 12);
    assert_eq!(
        c.world
            .state()
            .player_environment()
            .unwrap()
            .environment()
            .timer_ms(),
        0
    );
    controls.visits.store(true, Ordering::SeqCst);
    c.receive(move_to(pose)).unwrap();
    pump_to(&mut c, 13);
    let queries = controls.queries.lock().unwrap();
    let route = queries.iter().find_map(route_query).unwrap();
    assert!(route.request.prior_cache.is_none());
    let target = route
        .require_target_water(&profile, [8; 16], c.world.state().player)
        .unwrap();
    assert_eq!(target.tick(), 13);
    assert_eq!(target.moment(), n::PlayerEnvironmentMoment::BeforeCountdown);
    let snapshot = serde_json::to_value(target).unwrap();
    assert_eq!(snapshot["snapshot"]["updated_tick"], 12);
    assert_eq!(snapshot["snapshot"]["state"]["timer_ms"], 100);
    assert_eq!(
        snapshot["snapshot"]["state"]["cached_query"]["position"],
        serde_json::json!(pose.map(f32::to_bits))
    );
    assert_eq!(controls.environment.lock().unwrap().len(), 2);
    assert_eq!(retained(&c).source, motion::RouteSource::SyntheticFixture);
    assert!(
        controls.observations.lock().unwrap().is_empty(),
        "cold seed is not a loaded gate proof"
    );
    drop(queries);
    stage_cached_replan(&mut c);
    let motion::Generator::Chase(chase) = c.world.state().npcs[&2]
        .source_motion
        .as_ref()
        .unwrap()
        .motion
        .generator()
    else {
        panic!("Chase")
    };
    let latest = c.tick() + u64::from(chase.recheck_ms.div_ceil(profile.tick_ms)) + 1;
    loop {
        assert!(
            c.tick() < latest,
            "the staged target change must reach the warm gate after its source countdown"
        );
        let before = boundary(&c, &h);
        let out = c.pump(c.tick_duration());
        if let Some(pause) = out.pause {
            assert_eq!(out.ticks, 0);
            assert!(out.events.is_empty());
            assert!(matches!(pause.kind, PauseKind::Authority));
            assert_eq!(boundary(&c, &h), before);
            break;
        }
        assert_eq!(out.ticks, 1);
    }
    assert_eq!(controls.observations.lock().unwrap().len(), 1);
}
