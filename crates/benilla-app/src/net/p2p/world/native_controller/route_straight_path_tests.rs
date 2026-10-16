// SPDX-License-Identifier: GPL-3.0-or-later
//! Runtime-owned model width and loaded straight/fallback dependencies over authored
//! products. Initial cache, successful replay and all other authorities are synthetic.
use super::*;
use crate::net::p2p::native_geometry::{
    authored_route_smoothing, authored_route_straight_ground, authored_straight_path_revision,
    SourceRouteStraightPathProvider, SourceSmoothRequest, SourceTileReply, SourceTileRequest,
    SourceWarmRequest, StraightPathConfiguration, StraightPathDependency, StraightPathRequest,
    WarmCacheConfiguration,
};
use n::synthetic_creature_model_fixture as model;
use std::sync::mpsc;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Observation {
    context: n::AuthorityRequest,
    width: n::CreatureCollisionWidthWitness,
    owner_ignore_pathfinding: n::OwnerIgnorePathfindingReceipt,
    owner: [u32; 3],
    tickets: Vec<u64>,
    tiles: TileObservation,
    from: [u32; 3],
    requested_end: [u32; 3],
    end: [u32; 3],
    actual_end: [u32; 3],
    after_storage: Vec<u64>,
    after_active: usize,
    trace: Vec<benilla_native_nav::SourceWarmTrace>,
    phase: benilla_native_nav::SourceWarmPolyPhase,
    points: Vec<[u32; 3]>,
    path_type: u32,
    ray: Option<benilla_native_nav::SourceStraightRay>,
    diagnostics: benilla_native_nav::SourceStraightPathDiagnostics,
    fallback: Option<FallbackObservation>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct FallbackObservation {
    owner_ignore_pathfinding: n::OwnerIgnorePathfindingReceipt,
    tiles: TileObservation,
    before_storage: Vec<u64>,
    before_active: usize,
    from: [u32; 3],
    to: [u32; 3],
    previous_type: u32,
    after_storage: Vec<u64>,
    after_active: usize,
    points: Vec<[u32; 3]>,
    actual_end: [u32; 3],
    path_type: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct TileObservation {
    request: SourceTileRequest,
    start: ([i32; 2], bool),
    end: Option<([i32; 2], bool)>,
    both_present: bool,
}
fn observe_tiles(reply: &SourceTileReply) -> TileObservation {
    let request = reply.request();
    let result = reply.result();
    assert_eq!(result.from(), request.from);
    assert_eq!(result.to(), request.to);
    let start = result.start();
    TileObservation {
        request,
        start: (start.coordinates(), start.present()),
        end: result.end().map(|end| (end.coordinates(), end.present())),
        both_present: result.both_present(),
    }
}
struct StraightPathAuthority {
    send: mpsc::Sender<Option<n::AuthorityRequest>>,
    receive: mpsc::Receiver<Result<Option<motion::PathFinderCache>, Failure>>,
    service: Option<std::thread::JoinHandle<()>>,
    other: source::SyntheticMotionAuthority,
    queries: Arc<Mutex<Vec<n::AuthorityRequest>>>,
    failures: Arc<Mutex<Vec<Failure>>>,
    synthetic_goal: Option<ai::Point>,
    seeded: bool,
}
impl n::Authority for StraightPathAuthority {
    fn resolve(
        &mut self,
        context: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        self.queries.lock().unwrap().push(context.clone());
        let Some(route) = route_query(context) else {
            let mut reply = self.other.resolve(context)?;
            if matches!(
                context.query,
                n::Query::SourceMotion(n::MotionQuery::NearPoint(_))
            ) {
                if let Some(goal) = self.synthetic_goal.filter(|_| self.seeded) {
                    // Explicit surrounding synthetic NearPoint fixture. The runtime,
                    // not the loaded provider, constructs and binds the resulting Route.
                    let n::Facts::SourceMotion(n::MotionFacts::Spatial(spatial)) = &mut reply.facts
                    else {
                        unreachable!()
                    };
                    spatial.fact = motion::Fact::NearPoint(goal);
                }
            }
            return Ok(reply);
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
                    _ => n::NativeError::AuthorityUnavailable("loaded straight-path dependency"),
                }
            })?;
        let Some(seed) = seed else {
            // Loaded calculations still lack complete segment evidence and production
            // admission. Never manufacture a Complete or NoPath authority reply.
            return Err(n::NativeError::AuthorityUnavailable(
                "source owner route after straight/fallback dependencies unavailable",
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
        self.seeded = true;
        Ok(reply)
    }
    fn observe_derived(&mut self, reply: &n::AuthorityReply) -> Result<(), n::NativeError> {
        self.other.observe_derived(reply)
    }
}
impl Drop for StraightPathAuthority {
    fn drop(&mut self) {
        let _ = self.send.send(None);
        if let Some(service) = self.service.take() {
            service.join().unwrap();
        }
    }
}
struct WidthCatalog;
impl ClientCatalog for WidthCatalog {
    fn build(&self) -> u32 {
        GroundCatalog.build()
    }
    fn manifest_digest(&self) -> runtime::Digest {
        GroundCatalog.manifest_digest()
    }
    fn contains_map(&self, id: u32) -> bool {
        GroundCatalog.contains_map(id)
    }
    fn contains_creature_display(&self, id: u32) -> bool {
        id == 34
    }
    fn contains_item_display(&self, id: u32) -> bool {
        GroundCatalog.contains_item_display(id)
    }
    fn contains_spell(&self, id: u32) -> bool {
        GroundCatalog.contains_spell(id)
    }
    fn player_power_type(&self, player: PlayerId) -> Option<u8> {
        GroundCatalog.player_power_type(player)
    }
    fn player_bytes_2(&self, player: PlayerId) -> Option<u32> {
        GroundCatalog.player_bytes_2(player)
    }
    fn item_metadata(&self, entry: u32) -> Option<benilla_protocol::messages::ItemInfo> {
        GroundCatalog.item_metadata(entry)
    }
    fn quest_metadata(&self, entry: u32) -> Option<benilla_world_client::QuestMetadata> {
        GroundCatalog.quest_metadata(entry)
    }
    fn quest_greeting(&self, entry: u32) -> Option<String> {
        GroundCatalog.quest_greeting(entry)
    }
}
fn enter(
    h: &Harness,
    entry: Entry,
    data: n::Profile,
    authority: StraightPathAuthority,
) -> NativeController {
    NativeController::enter_component_test(
        &h.root,
        &h.profile,
        &h.character,
        entry,
        data,
        &WidthCatalog,
        authority,
    )
    .unwrap()
}
fn create(h: &Harness, data: n::Profile, authority: StraightPathAuthority) -> NativeController {
    enter(
        h,
        Entry::Create {
            world: [8; 16],
            seed: 7,
        },
        data,
        authority,
    )
}
fn configuration(optimize_path: bool, budget: u8) -> StraightPathConfiguration {
    let mut config = StraightPathConfiguration {
        warm: WarmCacheConfiguration {
            budget: benilla_native_nav::SourceWarmCacheBudget::default(),
        },
        optimize_path,
        normalize_z: false,
        ignore_normalization: false,
        budget: benilla_native_nav::SourceStraightPathBudget::default(),
        fallback_budget: benilla_native_nav::SourcePointPathBudget::default(),
    };
    match budget {
        1 => config.budget.lookup_primitives = 0,
        2 => config.budget.native_primitives = 0,
        3 => config.budget.sampled_points = 0,
        4 => config.warm.budget.lookup_primitives = 0,
        5 => config.fallback_budget.smoothing_primitives = 0,
        _ => {}
    }
    config
}
fn straight_path_setup(
    optimize_path: bool,
    crossing: bool,
    budget: u8,
) -> (
    n::Profile,
    StraightPathAuthority,
    Arc<AtomicU64>,
    Arc<Mutex<Vec<Observation>>>,
) {
    setup_with_width(optimize_path, crossing, budget, None)
}
// Snake rows are original authored geometry from the pinned source oracle.
fn snake_from() -> ai::Point {
    ai::Point::new(
        f32::from_bits(0xc3a4aaaa),
        f32::from_bits(0xc3d82aaa),
        f32::from_bits(0x3d800000),
    )
    .unwrap()
}
fn snake_to() -> ai::Point {
    ai::Point::new(
        f32::from_bits(0xc39caaaa),
        f32::from_bits(0xc3cd2aaa),
        f32::from_bits(0x3fb80000),
    )
    .unwrap()
}
fn setup_with_width(
    optimize_path: bool,
    crossing: bool,
    budget: u8,
    raw_snake_width: Option<f32>,
) -> (
    n::Profile,
    StraightPathAuthority,
    Arc<AtomicU64>,
    Arc<Mutex<Vec<Observation>>>,
) {
    setup_with_goal(
        optimize_path,
        crossing,
        budget,
        raw_snake_width,
        raw_snake_width.map(|_| snake_to()),
    )
}
fn setup_with_goal(
    optimize_path: bool,
    crossing: bool,
    budget: u8,
    raw_snake_width: Option<f32>,
    synthetic_goal: Option<ai::Point>,
) -> (
    n::Profile,
    StraightPathAuthority,
    Arc<AtomicU64>,
    Arc<Mutex<Vec<Observation>>>,
) {
    setup_with_poses(
        optimize_path,
        crossing,
        budget,
        raw_snake_width,
        synthetic_goal,
        None,
    )
}
fn setup_with_poses(
    optimize_path: bool,
    crossing: bool,
    budget: u8,
    raw_snake_width: Option<f32>,
    synthetic_goal: Option<ai::Point>,
    poses: Option<(ai::Point, ai::Point)>,
) -> (
    n::Profile,
    StraightPathAuthority,
    Arc<AtomicU64>,
    Arc<Mutex<Vec<Observation>>>,
) {
    let (mut data, base) = super::setup(true);
    let products = if raw_snake_width.is_some() {
        authored_route_straight_ground()
    } else {
        authored_route_smoothing()
    };
    let liquid = products.load_dry_liquid().unwrap();
    let navigation = products.load_navigation().unwrap();
    let config = configuration(optimize_path, budget);
    let binding = liquid.binding();
    data.map_id = binding.map;
    data.content_digest = binding.content;
    data.geometry_revision =
        authored_straight_path_revision(binding, navigation.binding(), config).unwrap();
    data.npcs[0].ai.map_id = binding.map;
    data.npcs[0].ai.content_revision = ai::movement::Revision(binding.content);
    let (owner_y, player_y) = if crossing {
        (-280., -266.7)
    } else {
        (-400., -390.)
    };
    data.npcs[0].ai.spawn = ai::Point::new(-400., owner_y, 0.).unwrap();
    data.player.start = ai::Point::new(-400., player_y, 0.).unwrap();
    if raw_snake_width.is_some() {
        data.npcs[0].ai.spawn = snake_from();
        data.player.start = snake_to();
        // Aggro is a synthetic prerequisite, not a source world-row claim.
        data.npcs[0].ai.detection = fixture::real(100.);
        data.source_motion
            .as_mut()
            .unwrap()
            .creature_detection_range = 100;
    }
    if let Some((owner, player)) = poses {
        data.npcs[0].ai.spawn = owner;
        data.player.start = player;
    }
    data.initialize_creature_capabilities(source::creature_capability_sources(&data))
        .unwrap();
    data.npcs[0].display_id = 34;
    // This is Plan's opaque model-selection receipt and real authored MPQ/DBC
    // loading, not a freely supplied width or a render/body approximation.
    data.initialize_creature_collision_widths(n::CreatureCollisionWidthRows::Complete(vec![
        n::CreatureCollisionWidthSource::from_initial_model(
            2,
            model::selection(),
            &model::catalog_with_width(raw_snake_width.unwrap_or(1.375)),
        )
        .unwrap(),
    ]))
    .unwrap();
    // Original authored player/control rows permit the real runtime-owned bit
    // receipt without changing the movement fixture or authenticating its warm seed.
    let initialized = fixture::initialized_profile();
    data.npcs[0].combat = initialized.npcs[0].combat.clone();
    if let n::AbilityAdmission::Available(spec) = &mut data.abilities[0].admission {
        spec.spell_rank = 1;
    }
    data.initialize_player(
        &fixture::initialized_starter_input(),
        fixture::float(100.),
        1,
    )
    .unwrap();
    data.initialize_interactions(n::synthetic_combat_fixture::source_rows(&data))
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
        let products = if raw_snake_width.is_some() {
            authored_route_straight_ground()
        } else {
            authored_route_smoothing()
        };
        let navigation = products.load_navigation().unwrap();
        let nav_binding = navigation.binding();
        // An actual loaded bounded corridor mints this opaque handle on its mesh
        // owner thread. No serialized refs recreate a loaded handle, even on reopen.
        let lineage = navigation
            .seed_bounded_warm_lineage(
                SourceSmoothRequest {
                    snapshot: nav_binding,
                    ticket: 1,
                    from: if raw_snake_width.is_some() {
                        point_bits(snake_to())
                    } else {
                        [-400f32, -400., 0.].map(f32::to_bits)
                    },
                    to: if raw_snake_width.is_some() {
                        point_bits(snake_to())
                    } else {
                        [-400f32, -200., 0.].map(f32::to_bits)
                    },
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
        assert_eq!(active, if raw_snake_width.is_some() { 1 } else { 2 });
        assert_eq!(storage.len(), 74);
        let provider = SourceRouteStraightPathProvider::bind(
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
            if mode >= 3 {
                let n::Query::SourceMotion(n::MotionQuery::Route(route)) =
                    &mut actual_context.query
                else {
                    unreachable!()
                };
                match mode {
                    3 => route.request.prior_cache.as_mut().unwrap().poly_refs[0] ^= 1,
                    4 => {
                        route
                            .request
                            .prior_cache
                            .as_mut()
                            .unwrap()
                            .binding
                            .owner_generation += 1
                    }
                    5 => actual_session.content[0] ^= 1,
                    6..=14 => {
                        let width = route.collision_width.as_mut().unwrap();
                        let mut value = serde_json::to_value(&*width).unwrap();
                        match mode {
                            6 => value["npc"]["generation"] = 1.into(),
                            7 => {
                                value["content"][0] =
                                    (value["content"][0].as_u64().unwrap() ^ 1).into()
                            }
                            8 => {
                                value["source_binding"][0] =
                                    (value["source_binding"][0].as_u64().unwrap() ^ 1).into()
                            }
                            9 => value["native_display_id"] = 31.into(),
                            10 => value["model_id"] = 7.into(),
                            11 => value["object_scale"] = 7.0f32.to_bits().into(),
                            12 => value["raw_width"] = 11.0f32.to_bits().into(),
                            13 => value["width"] = profile.npcs[0].ai.bounding_radius.bits().into(),
                            _ => value["width"] = (1.25f32 * 1.375 * 1.75 * 2.5).to_bits().into(),
                        }
                        *width = serde_json::from_value(value).unwrap();
                    }
                    15 => route.collision_width = None,
                    20..=27 => edit_owner_ignore_pathfinding(route, (mode - 20) as u8),
                    _ => {}
                }
            }
            let result = (|| {
                let bound = provider.begin(&actual_session, &actual_context)?;
                let mut request = StraightPathRequest {
                    snapshot: provider.binding(),
                    context: actual_context,
                    first_ticket: 51,
                };
                match mode {
                    16 => request.snapshot.navigation.content[0] ^= 1,
                    17 => request.first_ticket = 0,
                    18 => request.first_ticket = u64::MAX - 5,
                    19 => request.context.binding.tick += 1,
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
                observe_dependency(&dependency, &profile, &context, &seen);
                Ok(None)
            })();
            answers.send(result).unwrap();
        }
    });
    (
        data,
        StraightPathAuthority {
            send,
            receive,
            service: Some(service),
            other: source::SyntheticMotionAuthority::default(),
            queries: Arc::new(Mutex::new(vec![])),
            failures: Arc::new(Mutex::new(vec![])),
            synthetic_goal,
            seeded: false,
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct StableBoundary {
    checkpoint: n::Checkpoint,
    state: n::State,
    records: Vec<n::TickRecord>,
    cache: motion::PathFinderCache,
    files: Vec<(PathBuf, Vec<u8>)>,
    generation: u64,
    position: [u32; 3],
    facing: u32,
    npc_guid: Option<u64>,
    projection_input: n::OwnedInput,
    next_ticket: u64,
    saved_tick: u64,
}
fn boundary(c: &NativeController, h: &Harness) -> StableBoundary {
    StableBoundary {
        checkpoint: c.world.checkpoint().unwrap(),
        state: c.world.state().clone(),
        records: c.world.records().to_vec(),
        cache: retained(c),
        files: durable_files(h),
        generation: c.store.generation(),
        position: c.position_xyz().map(f32::to_bits),
        facing: c.facing().to_bits(),
        npc_guid: c.projection.npc_guid(c.world.state().npcs[&2].id),
        // The projection's stale-state guard also checks its prior digest.
        projection_input: c
            .projection
            .movement_pose(
                &c.world,
                c.world.state().position,
                c.world.state().orientation,
                motion::GroundMovementFlags::STOPPED,
            )
            .unwrap(),
        next_ticket: c.next_ticket,
        saved_tick: c.saved_tick,
    }
}
fn stage_replan(c: &mut NativeController) {
    // Player movement remains an explicitly synthetic surrounding authority.
    let mut moved = c.position_xyz();
    moved[1] += 6.;
    c.receive(move_to(moved)).unwrap();
}
fn edit_owner_ignore_pathfinding(route: &mut n::CreatureRouteRequest, edit: u8) {
    if edit == 0 {
        route.owner_ignore_pathfinding = None;
        return;
    }
    let mut value = serde_json::to_value(route.owner_ignore_pathfinding.as_ref().unwrap()).unwrap();
    match edit {
        1 => value["npc"]["generation"] = (route.owner.npc().generation + 1).into(),
        2..=4 => {
            let field =
                ["profile_binding", "source_binding", "capability_binding"][usize::from(edit - 2)];
            value[field][0] = (value[field][0].as_u64().unwrap() ^ 1).into();
        }
        5 => value["policy"] = "forged-owner-policy".into(),
        6 => value["projected_unit_state"] = n::UNIT_STAT_IGNORE_PATHFINDING.into(),
        7 => value["live_abilities"][0]["id"] = 60009.into(),
        _ => unreachable!(),
    }
    route.owner_ignore_pathfinding = Some(serde_json::from_value(value).unwrap());
}
fn assert_owner_ignore_pathfinding(
    route: &n::CreatureRouteRequest,
    profile: &n::Profile,
) -> n::OwnerIgnorePathfindingReceipt {
    let receipt = route.require_owner_ignore_pathfinding(profile).unwrap();
    assert_eq!(receipt.npc(), route.owner.npc());
    assert_eq!(receipt.profile_binding(), profile.digest());
    assert_eq!(receipt.policy(), n::OWNER_IGNORE_PATHFINDING_POLICY);
    assert!(!receipt.ignore_pathfinding());
    assert_eq!(receipt.projected_unit_state(), 0);
    receipt.clone()
}
fn assert_width(
    route: &n::CreatureRouteRequest,
    data: &n::Profile,
) -> n::CreatureCollisionWidthWitness {
    let width = route.require_collision_width(data).unwrap();
    assert_eq!(width.npc(), route.owner.npc());
    assert_eq!(width.native_display_id(), 34);
    assert_eq!(width.model_id(), 9);
    assert_eq!(width.object_scale().bits(), 1.25f32.to_bits());
    let expected = data
        .creature_collision_widths
        .as_ref()
        .unwrap()
        .source(2)
        .unwrap()
        .initial_width();
    assert_eq!(width.raw_width(), expected.raw_width());
    assert_eq!(width.width(), expected.width());
    assert_eq!(
        width.width().bits(),
        (f32::from_bits(width.raw_width().bits()) * 1.25).to_bits()
    );
    assert_ne!(width.width().bits(), data.npcs[0].ai.bounding_radius.bits());
    assert_ne!(
        width.width().bits(),
        (data.npcs[0].ai.bounding_radius.as_f32() * 2.).to_bits()
    );
    assert_ne!(
        width.width().bits(),
        (1.25f32 * 1.375 * 1.75 * 2.5).to_bits()
    );
    width.clone()
}

#[test]
fn actual_width_straight_dependencies_retry_without_publishing_and_only_synthetic_seed_replays() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    for crossing in [false, true] {
        for optimize_path in [false, true] {
            let h = Harness::new();
            let (data, authority, fault, observations) =
                straight_path_setup(optimize_path, crossing, 0);
            let queries = Arc::clone(&authority.queries);
            let failures = Arc::clone(&authority.failures);
            let mut c = create(&h, data.clone(), authority);
            c.initial_events().unwrap();
            let genesis = c.world.clone();
            pump_to(&mut c, 4);
            let cache = retained(&c);
            assert_eq!(cache.source, motion::RouteSource::SyntheticFixture);
            assert_eq!(cache.revision, 1);
            assert_eq!(cache.active_len, 2);
            let records =
                n::decode_records(&n::encode_records(c.world.records()).unwrap()).unwrap();
            assert!(records
                .iter()
                .all(|record| record.version == n::NATIVE_FORMAT_VERSION));
            let first = route_reply(&records);
            let first_route = route_query(&first.request).unwrap();
            let width = assert_width(first_route, &data);
            let first_owner_bit = assert_owner_ignore_pathfinding(first_route, &data);
            assert_eq!(
                c.world.state().npcs[&2]
                    .creature_collision_width()
                    .unwrap()
                    .witness(),
                &width
            );
            let n::Facts::SourceMotion(n::MotionFacts::Spatial(reply)) = &first.facts else {
                panic!("spatial seed")
            };
            let motion::Fact::RouteCalculation(seed) = &reply.fact else {
                panic!("synthetic calculation")
            };
            assert_eq!(seed.source, motion::RouteSource::SyntheticFixture);
            assert_eq!(seed.after_cache, cache);
            let motion::RouteOutcome::Complete(path) = &seed.outcome else {
                panic!("synthetic complete")
            };
            assert_eq!(path.source, motion::RouteSource::SyntheticFixture);
            // Only the accepted synthetic seed route replays; the loaded calculation
            // never emits a Route receipt. Its owned-bit receipt has a narrower scope.
            let mut replay = genesis;
            replay.replay(&records).unwrap();
            assert_eq!(replay.checkpoint().unwrap(), c.world.checkpoint().unwrap());
            stage_replan(&mut c);
            let before = boundary(&c, &h);
            let mut pending = None;
            for (attempt, mode) in [0, 1, 15, 20, 0].into_iter().enumerate() {
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
                assert_eq!(boundary(&c, &h), before);
                assert!(c.outbox.is_none());
                assert!(c.actions.is_empty());
                let retained = c.pending.as_ref().unwrap();
                let current = (retained.inputs.clone(), retained.moving);
                if let Some(expected) = &pending {
                    assert_eq!(&current, expected);
                } else {
                    assert_eq!(current.0.len(), 1);
                    pending = Some(current);
                }
                let count = queries.lock().unwrap().len();
                assert_eq!(c.pump(Duration::from_secs(60)).ticks, 0);
                assert_eq!(queries.lock().unwrap().len(), count);
                assert_eq!(boundary(&c, &h), before);
            }
            assert_eq!(*failures.lock().unwrap(), [Failure::MissingGeometry; 3]);
            let queries = queries.lock().unwrap();
            let warm: Vec<_> = queries
                .iter()
                .filter(|q| route_query(q).is_some_and(|r| r.request.prior_cache.is_some()))
                .collect();
            assert_eq!(warm.len(), 5);
            assert!(warm.iter().all(|q| *q == warm[0]));
            let route = route_query(warm[0]).unwrap();
            assert_eq!(route.request.prior_cache.as_deref(), Some(&cache));
            assert_eq!(route.owner_position, route.request.origin);
            assert_eq!(warm[0].binding.tick, 5);
            assert_eq!(warm[0].binding.world, [8; 16]);
            assert_eq!(warm[0].binding.profile, data.digest());
            assert_eq!(warm[0].binding.geometry, data.geometry_revision);
            assert_eq!(warm[0].binding.npc, Some(width.npc()));
            assert_eq!(assert_width(route, &data), width);
            assert_eq!(
                assert_owner_ignore_pathfinding(route, &data),
                first_owner_bit
            );
            let seen = observations.lock().unwrap();
            assert_eq!(seen.len(), 2);
            assert_eq!(seen[0], seen[1]);
            let result = &seen[0];
            assert_eq!(result.context, *warm[0]);
            assert_eq!(result.width, width);
            assert_eq!(result.owner_ignore_pathfinding, first_owner_bit);
            assert_eq!(result.tickets, [51, 52, 53, 54]);
            assert_eq!(result.tiles.start, ([0, 0], true));
            assert_eq!(result.tiles.end, Some(([0, 0], true)));
            assert_eq!(result.owner, point_bits(route.owner_position));
            assert_eq!(result.from, point_bits(route.request.origin));
            assert_eq!(result.requested_end, point_bits(route.request.goal));
            assert_eq!(result.end, result.requested_end);
            assert_eq!(result.actual_end, result.requested_end);
            assert_eq!(result.path_type, 1);
            assert!(result.ray.is_none());
            assert!(result.fallback.is_none());
            assert_eq!(result.points.first(), Some(&result.from));
            assert_eq!(result.points.last(), Some(&result.requested_end));
            assert_eq!(result.after_active, if crossing { 2 } else { 1 });
            assert_eq!(result.after_storage, cache.poly_refs);
            assert_eq!(
                result.phase,
                if crossing {
                    benilla_native_nav::SourceWarmPolyPhase::Cut
                } else {
                    benilla_native_nav::SourceWarmPolyPhase::SamePolygon
                }
            );
            if crossing {
                assert!(result.points.len() > 2);
                assert!(result.diagnostics.sampled_points > 0);
            } else {
                assert_eq!(result.points, [result.from, result.requested_end]);
                assert_eq!(result.diagnostics.sampled_points, 0);
            }
            drop(seen);
            drop(queries);
            let digest = c.world.state_digest();
            c.shutdown().unwrap();
            drop(c);
            let (_, authority, _, _) = straight_path_setup(optimize_path, crossing, 0);
            let reopened = enter(&h, Entry::Open { world: [8; 16] }, data, authority);
            assert_eq!(reopened.world.state_digest(), digest);
            assert_eq!(retained(&reopened), cache);
            assert_eq!(reopened.tick(), 4);
            assert_eq!(reopened.world.state().next_sequence, 0);
        }
    }
}

#[test]
fn stale_generation_content_width_or_request_binding_never_mutates_controller() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    for mode in (2..=27).filter(|mode| !matches!(*mode, 15 | 20)) {
        let h = Harness::new();
        let (data, authority, fault, observations) = straight_path_setup(true, true, 0);
        let queries = Arc::clone(&authority.queries);
        let failures = Arc::clone(&authority.failures);
        let mut c = create(&h, data, authority);
        c.initial_events().unwrap();
        pump_to(&mut c, 4);
        stage_replan(&mut c);
        let before = boundary(&c, &h);
        fault.store(mode, Ordering::SeqCst);
        let out = c.pump(c.tick_duration());
        assert_eq!(out.ticks, 0, "fault {mode}");
        assert!(out.events.is_empty());
        assert!(
            matches!(out.pause.unwrap().kind, PauseKind::Simulation),
            "fault {mode}"
        );
        assert_eq!(boundary(&c, &h), before, "fault {mode}");
        assert!(c.outbox.is_none());
        assert!(observations.lock().unwrap().is_empty());
        assert_eq!(
            *failures.lock().unwrap(),
            [if mode == 18 {
                Failure::Invalid
            } else {
                Failure::Binding
            }],
            "fault {mode}"
        );
        let pending = c.pending.as_ref().unwrap();
        let expected = (pending.inputs.clone(), pending.moving);
        assert_eq!(expected.0.len(), 1);
        assert!(matches!(c.retry_tick(), Err(Error::Paused)));
        let count = queries.lock().unwrap().len();
        c.pump(Duration::from_secs(60));
        assert_eq!(queries.lock().unwrap().len(), count);
        let pending = c.pending.as_ref().unwrap();
        assert_eq!((pending.inputs.clone(), pending.moving), expected);
        assert_eq!(boundary(&c, &h), before);
    }
}

#[test]
fn straight_operational_budget_refusals_retry_exact_request_without_publishing() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    for budget in 1..=3 {
        let h = Harness::new();
        let (data, authority, _, observations) = straight_path_setup(true, true, budget);
        let queries = Arc::clone(&authority.queries);
        let failures = Arc::clone(&authority.failures);
        let mut c = create(&h, data, authority);
        c.initial_events().unwrap();
        pump_to(&mut c, 4);
        stage_replan(&mut c);
        let before = boundary(&c, &h);
        let mut pending = None;
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
            assert!(matches!(out.pause.unwrap().kind, PauseKind::Authority));
            assert_eq!(boundary(&c, &h), before);
            assert!(c.outbox.is_none());
            let inputs = c.pending.as_ref().unwrap().inputs.clone();
            if let Some(expected) = &pending {
                assert_eq!(&inputs, expected);
            } else {
                pending = Some(inputs);
            }
        }
        assert!(observations.lock().unwrap().is_empty());
        assert_eq!(*failures.lock().unwrap(), [Failure::MissingGeometry; 2]);
        let queries = queries.lock().unwrap();
        let warm: Vec<_> = queries
            .iter()
            .filter(|q| route_query(q).is_some_and(|r| r.request.prior_cache.is_some()))
            .collect();
        assert_eq!(warm.len(), 2);
        assert_eq!(warm[0], warm[1]);
    }
}

#[test]
fn source_invalid_runtime_route_coordinates_preserve_controller_boundary_and_pending_input() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    for (goal, poses, expected) in [
        (
            ai::Point::new(20_000., -390., 0.).unwrap(),
            None,
            benilla_native_nav::SourceMapCoordError::DestinationInvalid,
        ),
        (
            ai::Point::new(-400., -20_000., 0.).unwrap(),
            None,
            benilla_native_nav::SourceMapCoordError::DestinationInvalid,
        ),
        (
            ai::Point::new(-400., -390., 0.).unwrap(),
            Some((
                ai::Point::new(20_000., -400., 0.).unwrap(),
                ai::Point::new(20_000., -390., 0.).unwrap(),
            )),
            benilla_native_nav::SourceMapCoordError::StartInvalid,
        ),
    ] {
        let h = Harness::new();
        // The runtime owns these Route endpoints. Only the surrounding initial
        // pose, initial route and NearPoint goal are explicit synthetic fixtures.
        let (data, authority, _, observations) =
            setup_with_poses(true, false, 0, None, Some(goal), poses);
        let queries = Arc::clone(&authority.queries);
        let failures = Arc::clone(&authority.failures);
        let mut c = create(&h, data, authority);
        c.initial_events().unwrap();
        pump_to(&mut c, 4);
        stage_replan(&mut c);
        let before = boundary(&c, &h);
        let out = c.pump(c.tick_duration());
        assert_eq!(out.ticks, 0);
        assert!(out.events.is_empty());
        assert!(out.saved.is_none());
        assert!(matches!(out.pause.unwrap().kind, PauseKind::Simulation));
        assert_eq!(boundary(&c, &h), before);
        assert!(c.outbox.is_none());
        assert!(c.actions.is_empty());
        assert!(observations.lock().unwrap().is_empty());
        assert_eq!(*failures.lock().unwrap(), [Failure::Invalid]);
        let pending = c.pending.as_ref().unwrap();
        let retained_input = (pending.inputs.clone(), pending.moving);
        assert_eq!(retained_input.0.len(), 1);
        assert!(matches!(c.retry_tick(), Err(Error::Paused)));
        let count = queries.lock().unwrap().len();
        assert_eq!(c.pump(Duration::from_secs(60)).ticks, 0);
        assert_eq!(queries.lock().unwrap().len(), count);
        let pending = c.pending.as_ref().unwrap();
        assert_eq!((pending.inputs.clone(), pending.moving), retained_input);
        assert_eq!(boundary(&c, &h), before);
        let queries = queries.lock().unwrap();
        let warm: Vec<_> = queries
            .iter()
            .filter_map(route_query)
            .filter(|route| route.request.prior_cache.is_some())
            .collect();
        assert_eq!(warm.len(), 1);
        let route = warm[0];
        assert_eq!(route.request.goal, goal);
        assert_eq!(route.owner_position, route.request.origin);
        assert_eq!(route.request.prior_cache.as_deref(), Some(&before.cache));
        assert_eq!(
            benilla_native_nav::source_map_coord_pair(
                point_bits(route.request.origin),
                point_bits(route.request.goal),
            ),
            Err(expected)
        );
    }
}

fn nearby_polygon_does_not_replace_missing_tile(
    route: &n::CreatureRouteRequest,
    start_missing: bool,
) {
    // A separate loaded authored fixture diagnoses these exact runtime endpoints.
    // This is raw polygon lookup evidence, not an admitted calculation or a way
    // to recreate the controller's opaque native lineage from retained refs.
    let navigation = authored_route_smoothing().load_navigation().unwrap();
    let snapshot = navigation.binding();
    let deadline = || Instant::now() + Duration::from_secs(5);
    let request = SourceTileRequest {
        snapshot,
        ticket: 1,
        from: point_bits(route.request.origin),
        to: point_bits(route.request.goal),
    };
    let tiles = observe_tiles(
        &navigation
            .source_tile_presence(request, deadline())
            .unwrap(),
    );
    assert!(!tiles.both_present);
    assert_eq!(tiles.start.1, !start_missing);
    if start_missing {
        assert!(tiles.start.0.contains(&-1));
        assert!(
            tiles.end.is_none(),
            "source short-circuits after missing start tile"
        );
    } else {
        assert!(!tiles.end.unwrap().1);
    }
    let lineage = navigation
        .seed_bounded_warm_lineage(
            SourceSmoothRequest {
                snapshot,
                ticket: 2,
                from: [-400f32, -400., 0.].map(f32::to_bits),
                to: [-400f32, -200., 0.].map(f32::to_bits),
                filter: benilla_native_nav::SourceOwnerFilter {
                    include_flags: 1,
                    exclude_flags: 0,
                },
                point_limit: 74,
                budget: benilla_native_nav::SourceSmoothingBudget::default(),
            },
            deadline(),
        )
        .unwrap();
    let cache = lineage.cache().clone();
    let raw = navigation
        .source_warm(
            &lineage,
            SourceWarmRequest {
                snapshot,
                ticket: 3,
                from: request.from,
                to: request.to,
                budget: benilla_native_nav::SourceWarmCacheBudget::default(),
            },
            deadline(),
        )
        .unwrap();
    assert_eq!(raw.result().from(), request.from);
    assert_eq!(raw.result().to(), request.to);
    assert!(raw.result().after().active_len() > 0);
    assert_eq!(lineage.cache(), &cache);
    assert_eq!(
        observe_tiles(
            &navigation
                .source_tile_presence(request, deadline())
                .unwrap()
        ),
        tiles,
        "nearby cached polygons cannot manufacture an endpoint tile"
    );
}

#[test]
fn absent_endpoint_tile_near_cached_polygon_is_unavailable_and_retries_without_publication() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    for start_missing in [false, true] {
        let h = Harness::new();
        let (goal, poses) = if start_missing {
            (
                ai::Point::new(-530., -380., 0.).unwrap(),
                Some((
                    ai::Point::new(-534., -400., 0.).unwrap(),
                    ai::Point::new(-534., -390., 0.).unwrap(),
                )),
            )
        } else {
            (ai::Point::new(-400., -534., 0.).unwrap(), None)
        };
        let (data, authority, _, observations) =
            setup_with_poses(true, false, 0, None, Some(goal), poses);
        let queries = Arc::clone(&authority.queries);
        let failures = Arc::clone(&authority.failures);
        let mut c = create(&h, data, authority);
        c.initial_events().unwrap();
        pump_to(&mut c, 4);
        stage_replan(&mut c);
        let before = boundary(&c, &h);
        let mut pending = None;
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
            assert!(c.actions.is_empty());
            let current = c.pending.as_ref().unwrap();
            let current = (current.inputs.clone(), current.moving);
            if let Some(expected) = &pending {
                assert_eq!(&current, expected);
            } else {
                assert_eq!(current.0.len(), 1);
                pending = Some(current);
            }
            let count = queries.lock().unwrap().len();
            assert_eq!(c.pump(Duration::from_secs(60)).ticks, 0);
            assert_eq!(queries.lock().unwrap().len(), count);
            assert_eq!(boundary(&c, &h), before);
        }
        assert!(observations.lock().unwrap().is_empty());
        assert_eq!(*failures.lock().unwrap(), [Failure::MissingGeometry; 2]);
        let queries = queries.lock().unwrap();
        let warm: Vec<_> = queries
            .iter()
            .filter(|q| route_query(q).is_some_and(|r| r.request.prior_cache.is_some()))
            .collect();
        assert_eq!(warm.len(), 2);
        assert_eq!(warm[0], warm[1]);
        let route = route_query(warm[0]).unwrap();
        assert_eq!(route.request.goal, goal);
        assert_eq!(route.request.prior_cache.as_deref(), Some(&before.cache));
        assert_eq!(before.cache.source, motion::RouteSource::SyntheticFixture);
        nearby_polygon_does_not_replace_missing_tile(route, start_missing);
    }
}

fn observe_dependency(
    dependency: &StraightPathDependency,
    profile: &n::Profile,
    context: &n::AuthorityRequest,
    seen: &Mutex<Vec<Observation>>,
) {
    let route = route_query(context).unwrap();
    let width = assert_width(route, profile);
    let owner_ignore_pathfinding = assert_owner_ignore_pathfinding(route, profile);
    assert_eq!(
        dependency.owner_ignore_pathfinding_dependency(),
        &owner_ignore_pathfinding
    );
    let filter = dependency.filter_dependency();
    let tiles = observe_tiles(dependency.tile_dependency());
    assert_eq!(
        tiles.request,
        SourceTileRequest {
            snapshot: dependency.request().snapshot.navigation,
            ticket: dependency.request().first_ticket,
            from: point_bits(route.request.origin),
            to: point_bits(route.request.goal),
        }
    );
    assert!(tiles.both_present);
    assert!(tiles.start.1);
    assert!(tiles.end.unwrap().1);
    assert_eq!(filter.request().first_ticket, tiles.request.ticket + 1);
    assert_eq!(dependency.request().context, *context);
    assert_eq!(filter.request().context, *context);
    assert_eq!(filter.owner_position(), point_bits(route.owner_position));
    assert_eq!(filter.continued_from_ticket(), None);
    assert_eq!(
        std::array::from_fn::<_, 2, _>(|i| filter.liquid_observations()[i].mask),
        [
            route_filter::LiquidMask::AllLiquids,
            route_filter::LiquidMask::WaterOrOcean
        ]
    );
    let mut tickets = vec![tiles.request.ticket];
    tickets.extend(filter.liquid_observations().iter().map(|o| {
        assert_eq!(
            o.reply.request.snapshot,
            dependency.request().snapshot.liquid
        );
        assert_eq!(o.reply.request.position, point_bits(route.owner_position));
        o.reply.request.ticket
    }));
    let straight = dependency.straight_dependency();
    let query = straight.request();
    let result = straight.result();
    tickets.push(query.ticket);
    assert_eq!(query.ticket, tiles.request.ticket + 3);
    assert_eq!(query.snapshot, dependency.request().snapshot.navigation);
    assert_eq!(query.from, point_bits(route.request.origin));
    assert_eq!(query.to, point_bits(route.request.goal));
    assert_eq!(query.collision_width_bits, width.width().bits());
    assert_eq!(result.collision_width_bits(), width.width().bits());
    assert_eq!(result.from(), query.from);
    assert_eq!(result.requested_end(), query.to);
    assert_eq!(result.config(), query.configuration);
    assert_eq!(
        query.configuration.previous_path_type,
        route.request.prior_cache.as_ref().unwrap().path_type
    );
    assert_eq!(
        query.configuration.optimize_path,
        dependency.request().snapshot.configuration.optimize_path
    );
    assert!(!query.configuration.normalize_z);
    assert!(!query.configuration.ignore_normalization);
    assert!(!query.configuration.force_destination);
    assert_eq!(result.after().filter(), filter.filter());
    let fallback = dependency.fallback_dependency().map(|fallback| {
        let retry_tiles = observe_tiles(fallback.tile_dependency());
        assert_eq!(
            retry_tiles.request,
            SourceTileRequest {
                ticket: tiles.request.ticket + 4,
                ..tiles.request
            }
        );
        assert!(retry_tiles.both_present);
        assert_eq!(retry_tiles.start, tiles.start);
        assert_eq!(retry_tiles.end, tiles.end);
        tickets.push(retry_tiles.request.ticket);
        let update = fallback.filter_dependency();
        assert_eq!(
            update.request().first_ticket,
            retry_tiles.request.ticket + 1
        );
        assert_eq!(update.request().context, *context);
        assert_eq!(update.owner_position(), filter.owner_position());
        assert_eq!(
            update.continued_from_ticket(),
            Some(filter.request().first_ticket)
        );
        assert_eq!(update.filter(), result.after().filter());
        assert_eq!(
            std::array::from_fn::<_, 2, _>(|i| update.liquid_observations()[i].mask),
            [
                route_filter::LiquidMask::AllLiquids,
                route_filter::LiquidMask::WaterOrOcean
            ]
        );
        for o in update.liquid_observations() {
            assert_eq!(o.reply.request.position, point_bits(route.owner_position));
            assert_eq!(
                o.reply.request.snapshot,
                dependency.request().snapshot.liquid
            );
            tickets.push(o.reply.request.ticket);
        }
        // This is the actual typed continuation's input, not a copied expectation.
        assert_eq!(fallback.before_cache(), result.after());
        let polygons = fallback.polygon_dependency();
        let points = fallback.point_dependency();
        let warm = polygons.result();
        let terminal = points.result();
        tickets.push(polygons.request().ticket);
        tickets.push(points.request().ticket);
        assert_eq!(polygons.request().from, query.from);
        assert_eq!(polygons.request().to, query.to);
        assert_eq!(warm.from(), result.from());
        assert_eq!(warm.to(), result.requested_end());
        assert_eq!(points.request().warm, polygons.request());
        assert_eq!(
            points.request().configuration.previous_path_type,
            result.path_type()
        );
        assert_eq!(terminal.config(), points.request().configuration);
        assert_eq!(terminal.warm(), warm);
        assert_eq!(terminal.after(), warm.after());
        assert_eq!(
            fallback.owner_ignore_pathfinding_dependency(),
            &owner_ignore_pathfinding
        );
        FallbackObservation {
            owner_ignore_pathfinding: fallback.owner_ignore_pathfinding_dependency().clone(),
            tiles: retry_tiles,
            before_storage: fallback.before_cache().storage().to_vec(),
            before_active: fallback.before_cache().active_len(),
            from: warm.from(),
            to: warm.to(),
            previous_type: terminal.config().previous_path_type,
            after_storage: terminal.after().storage().to_vec(),
            after_active: terminal.after().active_len(),
            points: terminal.points().to_vec(),
            actual_end: terminal.actual_end(),
            path_type: terminal.path_type(),
        }
    });
    assert_eq!(fallback.is_some(), result.path_type() & (8 | 4) != 0);
    seen.lock().unwrap().push(Observation {
        context: context.clone(),
        width,
        owner_ignore_pathfinding,
        owner: filter.owner_position(),
        tickets,
        tiles,
        from: result.from(),
        requested_end: result.requested_end(),
        end: result.end(),
        actual_end: result.actual_end(),
        after_storage: result.after().storage().to_vec(),
        after_active: result.after().active_len(),
        trace: result.trace().to_vec(),
        phase: result.phase(),
        points: result.points().to_vec(),
        path_type: result.path_type(),
        ray: result.ray().cloned(),
        diagnostics: result.diagnostics().clone(),
        fallback,
    });
}

#[test]
fn real_straight_rejection_or_clipping_continues_from_terminal_cache_type_and_original_goal() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    for raw_width in [1.375, 0.025] {
        for optimize in [false, true] {
            let h = Harness::new();
            let (data, authority, fault, observations) =
                setup_with_width(optimize, false, 0, Some(raw_width));
            let queries = Arc::clone(&authority.queries);
            let failures = Arc::clone(&authority.failures);
            let mut c = create(&h, data.clone(), authority);
            c.initial_events().unwrap();
            pump_to(&mut c, 4);
            let seed = retained(&c);
            assert_eq!(seed.source, motion::RouteSource::SyntheticFixture);
            assert_eq!(seed.path_type, 1);
            assert_eq!(seed.active_len, 1);
            let n::Facts::SourceMotion(n::MotionFacts::Spatial(spatial)) =
                &route_reply(c.world.records()).facts
            else {
                panic!("synthetic seed")
            };
            let motion::Fact::RouteCalculation(calculation) = &spatial.fact else {
                panic!("synthetic cache")
            };
            assert!(
                matches!(&calculation.outcome, motion::RouteOutcome::Complete(path) if path.source == motion::RouteSource::SyntheticFixture)
            );
            // Only this accepted initial route is synthetic. The loaded dependency
            // below never becomes a runtime NoPath or Complete outcome.
            stage_replan(&mut c);
            let before = boundary(&c, &h);
            let mut pending = None;
            for (attempt, mode) in [0, 1, 0].into_iter().enumerate() {
                fault.store(mode, Ordering::SeqCst);
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
                assert!(matches!(out.pause.unwrap().kind, PauseKind::Authority));
                assert_eq!(boundary(&c, &h), before);
                assert!(c.outbox.is_none());
                let current = c.pending.as_ref().unwrap();
                let current = (current.inputs.clone(), current.moving);
                if let Some(expected) = &pending {
                    assert_eq!(&current, expected);
                } else {
                    assert_eq!(current.0.len(), 1);
                    pending = Some(current);
                }
            }
            assert_eq!(*failures.lock().unwrap(), [Failure::MissingGeometry]);
            let queries = queries.lock().unwrap();
            let warm: Vec<_> = queries
                .iter()
                .filter(|q| route_query(q).is_some_and(|r| r.request.prior_cache.is_some()))
                .collect();
            assert_eq!(warm.len(), 3);
            assert!(warm.iter().all(|q| *q == warm[0]));
            let seen = observations.lock().unwrap();
            assert_eq!(seen.len(), 2);
            assert_eq!(seen[0], seen[1]);
            let first = &seen[0];
            assert_eq!(first.context, *warm[0]);
            assert_eq!(
                first.from,
                point_bits(route_query(warm[0]).unwrap().request.origin)
            );
            assert_eq!(first.requested_end, point_bits(snake_to()));
            assert_eq!(first.tickets, [51, 52, 53, 54, 55, 56, 57, 58, 59]);
            assert_eq!(
                first.phase,
                benilla_native_nav::SourceWarmPolyPhase::Replacement
            );
            assert!(first.ray.is_some());
            assert_eq!(first.after_active, 1);
            assert_ne!(first.after_storage, seed.poly_refs);
            let ray = first.ray.as_ref().unwrap();
            assert!(f32::from_bits(ray.hit_bits) < 1.);
            // Pinned source subtracts the full collision width directly from hit,
            // without halving it or dividing it by the route length.
            assert_eq!(
                ray.adjusted_hit_bits,
                Some(
                    (f32::from_bits(ray.hit_bits) - f32::from_bits(first.width.width().bits()))
                        .to_bits()
                )
            );
            if raw_width == 1.375 {
                assert_eq!(first.width.width().bits(), 1.71875f32.to_bits());
                assert_eq!(first.path_type, 8);
                assert!(f32::from_bits(ray.adjusted_hit_bits.unwrap()) < 0.1);
                assert!(first.points.is_empty());
                assert_eq!(first.end, first.requested_end);
            } else {
                assert_eq!(first.width.width().bits(), 0.03125f32.to_bits());
                assert_eq!(first.path_type, 4);
                assert!(f32::from_bits(ray.adjusted_hit_bits.unwrap()) >= 0.1);
                assert!(!first.points.is_empty());
                assert_ne!(first.end, first.requested_end);
                assert_eq!(first.actual_end, first.end);
            }
            let fallback = first.fallback.as_ref().unwrap();
            assert_eq!(first.tiles.request.ticket, 51);
            assert_eq!(fallback.tiles.request.ticket, 55);
            assert_eq!(fallback.tiles.start, first.tiles.start);
            assert_eq!(fallback.tiles.end, first.tiles.end);
            assert_eq!(fallback.before_storage, first.after_storage);
            assert_eq!(fallback.before_active, first.after_active);
            assert_eq!(fallback.previous_type, first.path_type);
            assert_eq!(fallback.from, first.from);
            assert_eq!(fallback.to, first.requested_end);
            assert_eq!(fallback.actual_end, first.requested_end);
            assert_eq!(fallback.points.first(), Some(&first.from));
            assert_eq!(fallback.points.last(), Some(&first.requested_end));
            assert_eq!(fallback.path_type, if raw_width == 1.375 { 1 } else { 4 });
            assert!(fallback.after_active > first.after_active);
            assert_eq!(retained(&c), seed);
        }
    }
}

#[test]
fn continuation_budget_refusals_discard_first_straight_terminal_and_retry_unmodified_input() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    for budget in [4, 5] {
        let h = Harness::new();
        let (data, authority, _, observations) = setup_with_width(true, false, budget, Some(1.375));
        let queries = Arc::clone(&authority.queries);
        let failures = Arc::clone(&authority.failures);
        let mut c = create(&h, data, authority);
        c.initial_events().unwrap();
        pump_to(&mut c, 4);
        stage_replan(&mut c);
        let before = boundary(&c, &h);
        let mut pending = None;
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
            assert!(matches!(out.pause.unwrap().kind, PauseKind::Authority));
            assert_eq!(boundary(&c, &h), before);
            assert!(c.outbox.is_none());
            let inputs = c.pending.as_ref().unwrap().inputs.clone();
            if let Some(expected) = &pending {
                assert_eq!(&inputs, expected);
            } else {
                assert_eq!(inputs.len(), 1);
                pending = Some(inputs);
            }
        }
        assert!(observations.lock().unwrap().is_empty());
        assert_eq!(*failures.lock().unwrap(), [Failure::MissingGeometry; 2]);
        let queries = queries.lock().unwrap();
        let warm: Vec<_> = queries
            .iter()
            .filter(|q| route_query(q).is_some_and(|r| r.request.prior_cache.is_some()))
            .collect();
        assert_eq!(warm.len(), 2);
        assert_eq!(warm[0], warm[1]);
    }
}

#[test]
fn loaded_short_alone_keeps_retry_absent_and_does_not_publish_cleared_terminal_cache() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let h = Harness::new();
    // The initial accepted route and later NearPoint goal are explicitly synthetic.
    // The long goal is admitted only as an actual runtime-issued warm Route query.
    let goal = ai::Point::new(-400., -108., 0.).unwrap();
    let (data, authority, _, observations) = setup_with_goal(false, false, 0, None, Some(goal));
    let queries = Arc::clone(&authority.queries);
    let mut c = create(&h, data, authority);
    c.initial_events().unwrap();
    pump_to(&mut c, 4);
    stage_replan(&mut c);
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
        assert!(matches!(out.pause.unwrap().kind, PauseKind::Authority));
        assert_eq!(boundary(&c, &h), before);
        assert!(c.outbox.is_none());
        assert_eq!(c.pending.as_ref().unwrap().inputs.len(), 1);
    }
    let queries = queries.lock().unwrap();
    let warm: Vec<_> = queries
        .iter()
        .filter(|q| route_query(q).is_some_and(|r| r.request.prior_cache.is_some()))
        .collect();
    assert_eq!(warm.len(), 2);
    assert_eq!(warm[0], warm[1]);
    let seen = observations.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0], seen[1]);
    assert_eq!(seen[0].context, *warm[0]);
    assert_eq!(seen[0].path_type, 32);
    assert_eq!(seen[0].after_active, 0);
    assert_eq!(seen[0].requested_end, point_bits(goal));
    assert_eq!(seen[0].tickets, [51, 52, 53, 54]);
    assert!(seen[0].fallback.is_none());
    assert!(retained(&c).active_len > 0);
}

#[path = "route_chase_gate_tests.rs"]
mod route_chase_gate_tests;

#[test]
fn runtime_owned_bit_in_synthetic_seed_journal_roundtrips_but_edits_cannot_replay() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let h = Harness::new();
    let (data, authority, _, _) = straight_path_setup(true, false, 0);
    let mut c = create(&h, data.clone(), authority);
    c.initial_events().unwrap();
    let genesis = c.world.clone();
    pump_to(&mut c, 4);
    let records = n::decode_records(&n::encode_records(c.world.records()).unwrap()).unwrap();
    let index = records
        .iter()
        .position(|record| {
            record
                .replies
                .iter()
                .any(|reply| route_query(&reply.request).is_some())
        })
        .unwrap();
    let receipt = assert_owner_ignore_pathfinding(
        route_query(&route_reply(&records).request).unwrap(),
        &data,
    );
    let mut before_route = genesis.clone();
    before_route.replay(&records[..index]).unwrap();
    let before = before_route.checkpoint().unwrap();
    for edit in 0..8 {
        let mut record = records[index].clone();
        let reply = record
            .replies
            .iter_mut()
            .find(|reply| route_query(&reply.request).is_some())
            .unwrap();
        let n::Query::SourceMotion(n::MotionQuery::Route(route)) = &mut reply.request.query else {
            unreachable!()
        };
        assert_eq!(route.owner_ignore_pathfinding.as_ref(), Some(&receipt));
        edit_owner_ignore_pathfinding(route, edit);
        let changed = n::decode_records(&n::encode_records(&[record]).unwrap()).unwrap();
        let mut rejected = before_route.clone();
        assert!(rejected.replay(&changed).is_err(), "receipt edit {edit}");
        assert_eq!(
            rejected.checkpoint().unwrap(),
            before,
            "receipt edit {edit}"
        );
    }
    let mut replay = genesis;
    replay.replay(&records).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), c.world.checkpoint().unwrap());
    assert_eq!(retained(&c).source, motion::RouteSource::SyntheticFixture);
}
