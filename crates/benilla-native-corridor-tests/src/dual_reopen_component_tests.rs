// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual loaded two-owner reopen and MotionMaster continuation. The authored
//! component scene does not admit imported content or ordinary native sessions.
use super::{
    products::{dry_route::tests as products_fixture, same_polygon_fixture},
    real_corridor_fixture::Fixture,
    source_creature_initialization::SourceCreatureInitializationProvider,
    source_near_point::SourceNearPointConfiguration,
    source_route_same_polygon::{
        chase_capture::ChaseCaptureView,
        dual_reconstruction::{DualReopenStage, SamePolygonReopenProducts},
        tests::config,
        SourceSamePolygonRouteProvider,
    },
    suspended_owner_component_tests::delayed_home_profile,
    wander_component_tests::{self as wander, Consumer},
    Session,
};
use benilla_sim::{creature_ai as ai, creature_ai::movement::Check, creature_motion as motion};
use benilla_world_runtime::{
    native::{synthetic_fixture as f, synthetic_two_owner_fixture as visitors, *},
    NpcId,
};
use benilla_world_store::native::{wander_reconstruction::WanderStore, Identity};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const REOPEN_BUDGET: Duration = Duration::from_secs(10);

// Keep independent live providers on the heap while exercising nested restore.
type LiveFixture = (Box<NativeWorld>, Box<Consumer>);
type StoredFixture = (Box<WanderStore>, Box<NativeWorld>, Box<Consumer>);
type AttemptFixture = (Option<Box<WanderStore>>, Box<NativeWorld>, Box<Consumer>);

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(std::env::temp_dir().join(format!(
            "benilla-dual-owner-reopen-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn profile(profile: &mut Profile) {
    delayed_home_profile(profile);
    // Leave enough source pursuit time for a genuine retained replan after reopen.
    profile.npcs[0].ai.pursuit_ms = 8000;
}
#[inline(never)]
fn setup() -> LiveFixture {
    let (world, consumer) = wander::setup_with_profile(Some(0), Some(800), profile);
    (Box::new(world), Box::new(consumer))
}
fn request(reply: &AuthorityReply) -> Option<&motion::RouteRequest> {
    let Query::SourceMotion(MotionQuery::Route(route)) = &reply.request.query else {
        return None;
    };
    Some(&route.request)
}
fn chase(snapshot: &motion::MotionSnapshot) -> &motion::ChaseSnapshot {
    let motion::Generator::Chase(chase) = &snapshot.generator else {
        panic!("actual active Chase");
    };
    chase
}
fn step(world: &mut NativeWorld, consumer: &mut Consumer, input: Option<OwnedInput>) -> TickRecord {
    let record = world
        .step(world.state().tick + 1, input, consumer)
        .unwrap()
        .clone();
    consumer.acknowledge(world);
    consumer.boundary.validate_world(world).unwrap();
    record
}
fn move_target(world: &NativeWorld, sequence: u32) -> OwnedInput {
    let phase = sequence % 28;
    let position = if phase <= 14 { phase } else { 28 - phase };
    OwnedInput {
        authenticated_owner: world.state().player.owner,
        input: Input {
            world: world.state().world,
            player: world.state().player,
            tick: world.state().tick + 1,
            sequence: world.state().next_sequence,
            command: Command::MovementPose {
                destination: ai::Point::new(
                    5. + position as f32 * 0.5,
                    12. - position as f32 * 0.5,
                    0.,
                )
                .unwrap(),
                orientation: world.state().orientation,
                movement_flags: motion::GroundMovementFlags::from_bits(1).unwrap(),
            },
        },
    }
}
fn clear_journal(world: &mut NativeWorld) {
    let checkpoint = world.checkpoint().unwrap();
    world.checkpoint_committed(&checkpoint).unwrap();
    assert!(world.records().is_empty());
}
fn paired_step(
    world: &mut NativeWorld,
    consumer: &mut Consumer,
    control: &mut NativeWorld,
    old: &mut Consumer,
    input: Option<OwnedInput>,
) -> TickRecord {
    let actual = step(world, consumer, input.clone());
    let expected = step(control, old, input);
    assert_eq!(actual, expected);
    assert_eq!(world.checkpoint().unwrap(), control.checkpoint().unwrap());
    assert_eq!(wander::snapshot(world), wander::snapshot(control));
    assert_eq!(world.state().rng_counter, control.state().rng_counter);
    actual
}

#[derive(Clone)]
struct Owner {
    npc: NpcId,
    construction: motion::QueryBinding,
    provider: usize,
    candidate: usize,
    cache: motion::PathFinderCache,
    first_event: PathfinderDispatchReceipt,
    wander_capture: Option<Vec<u8>>,
    chase_capture: Option<ChaseCaptureView>,
}
impl Owner {
    fn read(consumer: &Consumer, npc: NpcId, construction: motion::QueryBinding) -> Self {
        let (candidate, cache) = consumer
            .providers
            .route
            .committed_candidate_for_constructor(npc, construction)
            .unwrap();
        let (provider, first_event, selected) = consumer
            .providers
            .route
            .registered_constructor_for(npc, construction)
            .unwrap();
        assert!(selected);
        Self {
            npc,
            construction,
            provider,
            candidate,
            cache: cache.clone(),
            first_event,
            wander_capture: consumer
                .providers
                .route
                .reconstruction_capture_for(npc, construction),
            chase_capture: consumer
                .providers
                .route
                .chase_reconstruction_capture_for(npc, construction),
        }
    }
    fn assert_unchanged(&self, consumer: &Consumer) {
        let current = Self::read(consumer, self.npc, self.construction);
        assert_eq!(current.provider, self.provider);
        assert_eq!(current.candidate, self.candidate);
        assert_eq!(current.cache, self.cache);
        assert_eq!(current.first_event, self.first_event);
        assert_eq!(
            current.first_event.same_live_dispatch(&self.first_event),
            self.first_event.same_live_dispatch(&self.first_event)
        );
        assert_eq!(current.wander_capture, self.wander_capture);
        assert_eq!(current.chase_capture, self.chase_capture);
    }
    fn assert_retired(&self, consumer: &Consumer) {
        assert!(consumer
            .providers
            .route
            .committed_candidate_for_constructor(self.npc, self.construction)
            .is_none());
        assert!(consumer
            .providers
            .route
            .registered_constructor_for(self.npc, self.construction)
            .is_none());
        assert!(consumer
            .providers
            .route
            .reconstruction_capture_for(self.npc, self.construction)
            .is_none());
        assert!(consumer
            .providers
            .route
            .chase_reconstruction_capture_for(self.npc, self.construction)
            .is_none());
    }
}

fn owners(world: &NativeWorld, consumer: &Consumer) -> (Owner, Owner) {
    let state = wander::snapshot(world);
    let npc = world.state().npcs[&2].id;
    let suspended = state.suspended_wander.as_ref().unwrap();
    (
        Owner::read(consumer, npc, suspended.pathfinder_construction),
        Owner::read(
            consumer,
            npc,
            chase(&state).pathfinder_construction.unwrap(),
        ),
    )
}
fn assert_moving_dual(world: &NativeWorld, consumer: &Consumer) {
    let state = wander::snapshot(world);
    let active = chase(&state);
    let suspended = state.suspended_wander.as_ref().unwrap();
    let spline = state.spline.as_ref().unwrap();
    let head = suspended.normal_route.as_ref().unwrap();
    assert!(spline.spline.elapsed_ms > 0 && !spline.spline.finalized);
    assert_eq!(spline.route_request.kind, motion::RouteKind::ChaseReplan);
    assert_eq!(head.request.kind, motion::RouteKind::Wander);
    assert!(suspended.failed_route.is_none());
    assert_ne!(head.request, spline.route_request);
    assert_ne!(
        suspended.pathfinder_construction,
        active.pathfinder_construction.unwrap()
    );
    let (normal, chase) = owners(world, consumer);
    assert_eq!(normal.cache.path_type, 1);
    assert_eq!(normal.cache.active_len, 1);
    assert_eq!(suspended.pathfinder_cache.as_deref(), Some(&normal.cache));
    assert_eq!(active.pathfinder_cache.as_deref(), Some(&chase.cache));
    assert_eq!(
        spline.route_request.prior_cache.as_ref().unwrap().revision + 1,
        chase.cache.revision
    );
    let captured = chase.chase_capture.as_ref().unwrap();
    assert_eq!(captured.native.operation_count, chase.cache.revision);
    assert_eq!(captured.operations.len(), 2);
    assert_eq!(captured.operations[0].ordinal, 0);
    assert_eq!(
        captured.operations.last().unwrap().ordinal + 1,
        chase.cache.revision
    );
    let Query::SourceMotion(MotionQuery::Route(final_operation)) =
        &captured.operations.last().unwrap().calculation.query
    else {
        unreachable!()
    };
    assert_eq!(final_operation.request, spline.route_request);
    assert_eq!(normal.provider, chase.provider);
    assert_eq!(consumer.providers.route.registered_constructor_count(), 2);
    assert_eq!(consumer.providers.route.registered_generator_count(), 2);
    assert_eq!(consumer.providers.route.retained_candidate_count(), 2);
    consumer.boundary.validate_world(world).unwrap();
    wander::assert_dry_owner(world);
}

/// Genuine accepted inputs for sibling registry tests; no request or cache is forged.
#[inline(never)]
pub(super) fn moving_dual() -> (Box<NativeWorld>, Box<Consumer>, u32) {
    let (mut world, mut consumer) = setup();
    let first = step(&mut world, &mut consumer, None);
    assert!(wander::launched(&first));
    assert!(wander::wander(&wander::snapshot(&world))
        .normal_route
        .is_some());
    consumer.allow_chase = true;
    consumer.other.visibility = Some(Check::Passed);
    let mut entered = false;
    for _ in 0..128 {
        let before = wander::snapshot(&world);
        let record = step(&mut world, &mut consumer, None);
        if world.state().npcs[&2].ai.phase() == ai::Phase::Combat {
            assert!(record
                .replies
                .iter()
                .any(|reply| request(reply)
                    .is_some_and(|r| r.kind == motion::RouteKind::ChaseInitial)));
            assert_eq!(
                wander::snapshot(&world).suspended_wander.as_ref(),
                Some(wander::wander(&before))
            );
            entered = true;
            break;
        }
    }
    assert!(
        entered,
        "ordinary initial Unit notification must enter Chase"
    );
    let mut replanned = false;
    for sequence in 1..=128 {
        let input = move_target(&world, sequence);
        let record = step(&mut world, &mut consumer, Some(input));
        assert_eq!(world.state().npcs[&2].ai.phase(), ai::Phase::Combat);
        replanned |= record
            .replies
            .iter()
            .any(|reply| request(reply).is_some_and(|r| r.kind == motion::RouteKind::ChaseReplan));
        let state = wander::snapshot(&world);
        if replanned
            && state
                .spline
                .as_ref()
                .is_some_and(|s| s.spline.elapsed_ms > 0 && !s.spline.finalized)
        {
            let final_reply = consumer
                .route_replies
                .iter()
                .rev()
                .find(|reply| {
                    request(reply).is_some_and(|r| {
                        matches!(
                            r.kind,
                            motion::RouteKind::ChaseInitial | motion::RouteKind::ChaseReplan
                        )
                    })
                })
                .unwrap();
            let final_calculation = wander::calculation(final_reply);
            assert_eq!(
                chase(&state).pathfinder_cache.as_deref(),
                Some(&final_calculation.after_cache)
            );
            let motion::RouteOutcome::Complete(path) = &final_calculation.outcome else {
                panic!("latest accepted Chase calculation must launch");
            };
            assert_eq!(*path, state.spline.as_ref().unwrap().route);
            assert_moving_dual(&world, &consumer);
            return (world, consumer, sequence);
        }
    }
    panic!("loaded Chase must reach a moving latest-cache-matching retained replan");
}
fn recipe(world: &NativeWorld, consumer: &Consumer) -> Vec<u8> {
    consumer
        .providers
        .route
        .encode_wander_and_chase_reconstruction(&consumer.session, world, &consumer.boundary)
        .unwrap()
}

fn reopen_bytes(
    profile: &Profile,
    session: &Session,
    checkpoint: Checkpoint,
    bytes: &[u8],
    budget: Duration,
) -> Result<LiveFixture, NativeError> {
    reopen_observed(profile, session, checkpoint, bytes, budget, None, None)
}
#[inline(never)]
fn reopen_observed(
    profile: &Profile,
    session: &Session,
    checkpoint: Checkpoint,
    bytes: &[u8],
    budget: Duration,
    reject: Option<DualReopenStage>,
    foreign: Option<&SourceSamePolygonRouteProvider>,
) -> Result<LiveFixture, NativeError> {
    let (_, world, consumer) = reopen_attempt(
        profile,
        session,
        checkpoint,
        bytes,
        budget,
        ReopenAttempt::Observed { reject, foreign },
    )?;
    Ok((world, consumer))
}
enum ReopenAttempt<'a> {
    Observed {
        reject: Option<DualReopenStage>,
        foreign: Option<&'a SourceSamePolygonRouteProvider>,
    },
    Store(Box<WanderStore>),
}
#[inline(never)]
fn reopen_attempt(
    profile: &Profile,
    session: &Session,
    checkpoint: Checkpoint,
    bytes: &[u8],
    budget: Duration,
    attempt: ReopenAttempt<'_>,
) -> Result<AttemptFixture, NativeError> {
    let reject = match &attempt {
        ReopenAttempt::Observed { reject, .. } => *reject,
        ReopenAttempt::Store(_) => None,
    };
    let mut geometry = Fixture::new();
    let products = geometry.products();
    let raw_geometry = geometry.raw_geometry_revision();
    let environment = products_fixture::environment(Arc::clone(&products));
    let height_configuration = same_polygon_fixture::height_configuration(&products);
    let height = Arc::clone(&products)
        .load_dry_map_height_owned(height_configuration, session.participants.as_ref())
        .unwrap();
    let initial_height = Arc::clone(&products)
        .load_dry_map_height_owned(height_configuration, session.participants.as_ref())
        .unwrap();
    let pending = NativeWorld::prepare_synthetic_wander_and_chase_reopen(
        profile.clone(),
        checkpoint.clone(),
    )?;
    let rejected = std::cell::Cell::new(false);
    let restored = geometry.with_local_corridor(|corridor| {
        let products = SamePolygonReopenProducts {
            inventory: &products,
            environment,
            height,
            near_configuration: SourceNearPointConfiguration {
                height: height_configuration,
                detect_position_collision: true,
                vmap_line_of_sight_enabled: true,
            },
            raw_geometry,
            configuration: config(),
        };
        match attempt {
            ReopenAttempt::Store(store) => {
                SourceSamePolygonRouteProvider::reopen_wander_and_chase_store(
                    session,
                    pending,
                    *store,
                    profile,
                    f::player(),
                    products,
                    corridor,
                    Instant::now() + budget,
                )
                .map(|(store, world, providers, boundary)| {
                    (
                        Some(Box::new(store)),
                        Box::new(world),
                        Box::new(providers),
                        boundary,
                    )
                })
            }
            ReopenAttempt::Observed { foreign, .. } => {
                SourceSamePolygonRouteProvider::reopen_wander_and_chase_with_checks(
                    session,
                    pending,
                    bytes,
                    profile,
                    f::player(),
                    products,
                    corridor,
                    Instant::now() + budget,
                    |stage| {
                        if reject == Some(stage) {
                            rejected.set(true);
                            Err(NativeError::AuthorityUnavailable(
                                "authored dual publication failure",
                            ))
                        } else {
                            Ok(())
                        }
                    },
                    |queries, plan, wander, chase| {
                        if let Some(foreign) = foreign {
                            foreign.assert_reconstructed_batch_rejections(
                                queries, plan, wander, chase,
                            );
                        }
                        Ok(())
                    },
                )
                .map(|(world, providers, boundary)| {
                    (None, Box::new(world), Box::new(providers), boundary)
                })
            }
        }
    });
    if reject.is_some() {
        assert!(
            rejected.get(),
            "selected private publication boundary must be reached"
        );
    }
    let (store, world, providers, boundary) = restored?;
    assert_eq!(world.checkpoint()?, checkpoint);
    assert_eq!(world.checkpoint()?.encode()?, checkpoint.encode()?);
    assert!(world.records().is_empty());
    let initialization =
        SourceCreatureInitializationProvider::bind(session.world, profile, initial_height).unwrap();
    let mut other = visitors::CompleteAuthority {
        visibility: Some(Check::Passed),
        ..Default::default()
    };
    other.base.base.critical_chance = Some(0);
    other.base.base.resist_chance = Some(0);
    let consumer = Box::new(Consumer {
        geometry,
        session: session.clone(),
        providers: *providers,
        initialization,
        other,
        boundary,
        routes: vec![],
        route_replies: vec![],
        initializations: vec![],
        allow_chase: true,
        fail_after_movement: false,
        rejected: None,
    });
    assert_moving_dual(&world, &consumer);
    Ok((store, world, consumer))
}
#[inline(never)]
fn reopen_store(
    path: &Directory,
    profile: &Profile,
    session: &Session,
    checkpoint: &Checkpoint,
) -> StoredFixture {
    let (store, saved) = WanderStore::open(&path.0, Identity::of(checkpoint)).unwrap();
    assert_eq!(saved.checkpoint(), checkpoint);
    assert!(!store.requires_reopen());
    let (store, world, consumer) = reopen_attempt(
        profile,
        session,
        saved.checkpoint().clone(),
        saved.recipe_bytes(),
        REOPEN_BUDGET,
        ReopenAttempt::Store(Box::new(store)),
    )
    .unwrap();
    let store = store.unwrap();
    assert_eq!(store.recipe(), &saved);
    (store, world, consumer)
}

#[test]
fn fresh_dual_query_reopens_moving_chase_then_home_reset_and_single_wander() {
    let path = Directory::new();
    let (mut control, mut old, mut sequence) = moving_dual();
    let profile = control.profile().clone();
    let session = old.session.clone();
    let checkpoint = control.checkpoint().unwrap();
    let motion_before = wander::snapshot(&control);
    let (old_normal, old_chase) = owners(&control, &old);
    let bytes = recipe(&control, &old);
    let (store, _) = WanderStore::create(&path.0, &mut control, &bytes).unwrap();
    assert!(control.records().is_empty());
    assert!(control.validate_saved_session_reopen().is_err());
    wander::assert_loaded_checkpoint_rejected(&control, &checkpoint);
    drop(store);
    let (mut store, mut world, mut consumer) = reopen_store(&path, &profile, &session, &checkpoint);
    assert_eq!(wander::snapshot(&world), motion_before);
    assert_eq!(world.checkpoint().unwrap(), control.checkpoint().unwrap());
    assert_eq!(world.state().rng_counter, control.state().rng_counter);
    assert!(!consumer
        .providers
        .route
        .shares_native_query(&old.providers.route));
    let (normal, mut active) = owners(&world, &consumer);
    assert_ne!(normal.provider, old_normal.provider);
    assert_ne!(active.provider, old_chase.provider);
    assert_eq!(normal.cache, old_normal.cache);
    assert_eq!(active.cache, old_chase.cache);
    assert!(!normal.first_event.same_live_dispatch(&normal.first_event));
    assert!(!active.first_event.same_live_dispatch(&active.first_event));
    old_normal.assert_unchanged(&old);
    old_chase.assert_unchanged(&old);
    let suspended = motion_before.suspended_wander.as_ref().unwrap();
    let npc = world.state().npcs[&2].id;
    let initialized = world.state().npcs[&2]
        .initialized_creature()
        .unwrap()
        .clone();
    let combat_start = world.state().npcs[&2]
        .source_motion
        .as_ref()
        .unwrap()
        .combat_start
        .unwrap();
    let restored_revision = active.cache.revision;
    let mut replanned = false;
    let mut returning = false;
    for _ in 0..256 {
        sequence += 1;
        let input = move_target(&world, sequence);
        let record = paired_step(
            &mut world,
            &mut consumer,
            &mut control,
            &mut old,
            Some(input),
        );
        normal.assert_unchanged(&consumer);
        for reply in &record.replies {
            if world.state().npcs[&2].ai.phase() != ai::Phase::ReturningHome
                && request(reply).is_some_and(|r| r.kind == motion::RouteKind::ChaseReplan)
            {
                let next = Owner::read(&consumer, npc, active.construction);
                assert!(next.cache.revision > restored_revision);
                assert_eq!(
                    request(reply).unwrap().prior_cache.as_deref(),
                    Some(&active.cache)
                );
                active = next;
                replanned = true;
            }
        }
        if world.state().npcs[&2].ai.phase() == ai::Phase::ReturningHome {
            assert!(
                replanned,
                "a real retained Chase continuation must precede natural Home"
            );
            let home = record
                .replies
                .iter()
                .find(|reply| {
                    request(reply)
                        .is_some_and(|r| r.kind == motion::RouteKind::HomeForceDestination)
                })
                .unwrap();
            let home = request(home).unwrap();
            assert_eq!(home.goal, combat_start.position);
            assert!(home.prior_cache.is_none());
            assert_eq!(
                home.pathfinder_dispatch.unwrap().constructor_kind(),
                motion::PathfinderConstructorKind::HomeLocal
            );
            active.assert_retired(&consumer);
            assert_eq!(consumer.providers.route.registered_constructor_count(), 1);
            assert_eq!(consumer.providers.route.registered_generator_count(), 1);
            assert_eq!(
                wander::snapshot(&world).suspended_wander.as_ref(),
                Some(suspended)
            );
            returning = true;
            break;
        }
    }
    assert!(returning, "source pursuit expiry must begin Home");
    consumer.other.visibility = Some(Check::Failed);
    old.other.visibility = Some(Check::Failed);
    let mut reset = false;
    for _ in 0..256 {
        let record = paired_step(&mut world, &mut consumer, &mut control, &mut old, None);
        if record.events.iter().any(|event| {
            matches!(
                event,
                Event::NpcMotion {
                    event: motion::Event::HomeReached { .. },
                    ..
                }
            )
        }) {
            assert!(!wander::launched(&record));
            reset = true;
            break;
        }
        normal.assert_unchanged(&consumer);
        assert_eq!(consumer.providers.route.registered_constructor_count(), 1);
    }
    assert!(reset, "source HomeReached must reset the default Wander");
    let state = wander::snapshot(&world);
    let fresh = wander::wander(&state);
    assert_eq!(state.map_pose.position, combat_start.position);
    assert_eq!(fresh.next_move_count, suspended.next_move_count);
    assert_eq!(fresh.next_move_timer_ms, 0);
    assert_eq!(fresh.pathfinder_construction.now_ms, state.now_ms);
    assert_ne!(
        fresh.pathfinder_construction,
        suspended.pathfinder_construction
    );
    assert!(
        fresh.pathfinder_cache.is_none()
            && fresh.normal_route.is_none()
            && fresh.failed_route.is_none()
    );
    assert!(state.suspended_wander.is_none() && state.spline.is_none());
    normal.assert_retired(&consumer);
    active.assert_retired(&consumer);
    assert_eq!(consumer.providers.route.registered_constructor_count(), 0);
    assert_eq!(consumer.providers.route.retained_candidate_count(), 0);
    assert_eq!(world.state().npcs[&2].id, npc);
    assert_eq!(
        world.state().npcs[&2].initialized_creature(),
        Some(&initialized)
    );
    let construction = fresh.pathfinder_construction;
    let reset_tick = world.state().tick;
    let record = paired_step(&mut world, &mut consumer, &mut control, &mut old, None);
    assert!(wander::launched(&record));
    assert_eq!(world.state().tick, reset_tick + 1);
    let state = wander::snapshot(&world);
    let fresh = wander::wander(&state);
    assert_eq!(fresh.pathfinder_construction, construction);
    assert_eq!(fresh.pathfinder_cache.as_ref().unwrap().revision, 1);
    assert!(fresh.normal_route.is_some() && fresh.failed_route.is_none());
    assert_eq!(consumer.providers.route.registered_constructor_count(), 1);
    let bytes = consumer
        .providers
        .route
        .encode_wander_reconstruction(&consumer.session, &world, &consumer.boundary)
        .unwrap();
    store.checkpoint(&mut world, &bytes).unwrap();
    clear_journal(&mut control);
    let checkpoint = world.checkpoint().unwrap();
    drop((store, world, consumer));
    let (_store, mut world, mut consumer) = reopen_single_store(&path, &profile, &checkpoint);
    assert!(!consumer
        .providers
        .route
        .shares_native_query(&old.providers.route));
    assert_eq!(
        wander::wander(&wander::snapshot(&world))
            .pathfinder_cache
            .as_ref()
            .unwrap()
            .revision,
        1
    );
    paired_step(&mut world, &mut consumer, &mut control, &mut old, None);
}

#[inline(never)]
fn reopen_single_store(
    path: &Directory,
    expected_profile: &Profile,
    checkpoint: &Checkpoint,
) -> StoredFixture {
    let (store, saved) = WanderStore::open(&path.0, Identity::of(checkpoint)).unwrap();
    let (fresh, mut consumer) = wander::setup_with_profile(Some(0), Some(800), profile);
    assert_eq!(fresh.profile(), expected_profile);
    drop(fresh);
    let pending = NativeWorld::prepare_synthetic_wander_reopen(
        expected_profile.clone(),
        saved.checkpoint().clone(),
    )
    .unwrap();
    let route = consumer.providers.route;
    let (store, world, route, boundary) = consumer
        .geometry
        .with_local_corridor(|corridor| {
            route.reopen_wander_store(
                &consumer.session,
                pending,
                store,
                corridor,
                Instant::now() + REOPEN_BUDGET,
            )
        })
        .unwrap();
    consumer.providers.route = route;
    consumer.boundary = boundary;
    assert_eq!(world.checkpoint().unwrap(), *checkpoint);
    assert!(world.records().is_empty());
    (Box::new(store), Box::new(world), Box::new(consumer))
}

fn change_number(bytes: &[u8], marker: &str, change: impl FnOnce(u64) -> u64) -> Vec<u8> {
    let text = std::str::from_utf8(bytes).unwrap();
    let start = text.find(marker).expect("known canonical recipe field") + marker.len();
    let count = text[start..].bytes().take_while(u8::is_ascii_digit).count();
    assert!(count > 0);
    let old = text[start..start + count].parse::<u64>().unwrap();
    let mut result = text.to_owned();
    result.replace_range(start..start + count, &change(old).to_string());
    result.into_bytes()
}
fn change_chase_number(bytes: &[u8], marker: &str, change: impl FnOnce(u64) -> u64) -> Vec<u8> {
    let at = std::str::from_utf8(bytes)
        .unwrap()
        .find("\"chase\":")
        .unwrap();
    let mut result = bytes[..at].to_vec();
    result.extend(change_number(&bytes[at..], marker, change));
    result
}
fn replace_request(
    bytes: &[u8],
    expected: &AuthorityRequest,
    replacement: &AuthorityRequest,
) -> Vec<u8> {
    let text = std::str::from_utf8(bytes).unwrap();
    let expected = serde_json::to_string(expected).unwrap();
    let replacement = serde_json::to_string(replacement).unwrap();
    let start = text
        .rfind(&expected)
        .expect("captured request in canonical recipe");
    let mut result = text.to_owned();
    result.replace_range(start..start + expected.len(), &replacement);
    result.into_bytes()
}

#[test]
fn rejected_dual_recipes_preserve_disk_and_both_live_owners_for_fresh_retry() {
    let path = Directory::new();
    let (mut control, mut old, _) = moving_dual();
    let profile = control.profile().clone();
    let session = old.session.clone();
    let checkpoint = control.checkpoint().unwrap();
    let state = wander::snapshot(&control);
    let (normal, active) = owners(&control, &old);
    let bytes = recipe(&control, &old);
    let (store, _) = WanderStore::create(&path.0, &mut control, &bytes).unwrap();
    let disk = std::fs::read(path.0.join("world.checkpoint")).unwrap();
    drop(store);
    let captured = active.chase_capture.as_ref().unwrap();
    let last = &captured.operations.last().unwrap().calculation;
    let first = &captured.constructor;
    let wander_first = old
        .route_replies
        .iter()
        .find(|reply| request(reply).is_some_and(|r| r.kind == motion::RouteKind::WanderPrepare))
        .unwrap();
    let mut wrong_owner = last.clone();
    wrong_owner.binding.npc = Some(NpcId {
        generation: active.npc.generation + 1,
        ..active.npc
    });
    let mutations = [
        change_number(&bytes, "\"version\":", |n| n + 1),
        change_number(&bytes, "\"geometry\":[", |n| n ^ 1),
        change_number(&bytes, "\"checkpoint\":[", |n| n ^ 1),
        change_number(&bytes, "\"inventory\":[", |n| n ^ 1),
        change_number(&bytes, "\"first_counter\":", |n| n + 1),
        change_chase_number(&bytes, "\"operation_count\":", |n| n + 1),
        change_chase_number(&bytes, "\"query_id\":", |n| n + 1),
        replace_request(&bytes, last, &wrong_owner),
        replace_request(&bytes, last, first),
        replace_request(&bytes, first, &wander_first.request),
    ];
    assert!(reopen_bytes(
        &profile,
        &session,
        checkpoint.clone(),
        &bytes,
        Duration::ZERO
    )
    .is_err());
    for (index, changed) in mutations.iter().enumerate() {
        serde_json::from_slice::<serde_json::Value>(changed).unwrap();
        assert!(
            reopen_bytes(
                &profile,
                &session,
                checkpoint.clone(),
                changed,
                REOPEN_BUDGET
            )
            .is_err(),
            "dual recipe mutation {index} must not publish either owner"
        );
        assert_eq!(
            std::fs::read(path.0.join("world.checkpoint")).unwrap(),
            disk
        );
        assert_eq!(control.checkpoint().unwrap(), checkpoint);
        assert_eq!(wander::snapshot(&control), state);
        assert_eq!(recipe(&control, &old), bytes);
        normal.assert_unchanged(&old);
        active.assert_unchanged(&old);
    }
    for stage in [
        DualReopenStage::WanderReplayed,
        DualReopenStage::OwnersReplayed,
        DualReopenStage::ConstructorsInstalled,
        DualReopenStage::BeforePublication,
    ] {
        assert!(reopen_observed(
            &profile,
            &session,
            checkpoint.clone(),
            &bytes,
            REOPEN_BUDGET,
            Some(stage),
            None,
        )
        .is_err());
        assert_eq!(
            std::fs::read(path.0.join("world.checkpoint")).unwrap(),
            disk
        );
        assert_eq!(control.checkpoint().unwrap(), checkpoint);
        normal.assert_unchanged(&old);
        active.assert_unchanged(&old);
        // A rejected private attempt releases every fresh owner; the same store
        // lease and original capture can immediately take a new successful path.
        let (store, resumed, reopened) = reopen_store(&path, &profile, &session, &checkpoint);
        assert_eq!(resumed.checkpoint().unwrap(), checkpoint);
        assert!(!reopened
            .providers
            .route
            .shares_native_query(&old.providers.route));
        drop((store, resumed, reopened));
    }
    let (mut probe_world, mut probe) = reopen_observed(
        &profile,
        &session,
        checkpoint.clone(),
        &bytes,
        REOPEN_BUDGET,
        None,
        Some(&old.providers.route),
    )
    .unwrap();
    assert!(!probe
        .providers
        .route
        .shares_native_query(&old.providers.route));
    normal.assert_unchanged(&old);
    active.assert_unchanged(&old);
    assert_eq!(
        std::fs::read(path.0.join("world.checkpoint")).unwrap(),
        disk
    );
    // A real source update produces B; the disk lease still owns checkpoint A.
    step(&mut probe_world, &mut probe, None);
    assert_moving_dual(&probe_world, &probe);
    let successor = probe_world.checkpoint().unwrap();
    assert_ne!(successor, checkpoint);
    let (successor_normal, successor_chase) = owners(&probe_world, &probe);
    let (lease, saved) = WanderStore::open(&path.0, Identity::of(&checkpoint)).unwrap();
    assert_eq!(saved.checkpoint(), &checkpoint);
    assert!(matches!(
        reopen_attempt(
            &profile,
            &session,
            successor.clone(),
            saved.recipe_bytes(),
            REOPEN_BUDGET,
            ReopenAttempt::Store(Box::new(lease))
        ),
        Err(NativeError::InvalidCheckpoint)
    ));
    assert_eq!(probe_world.checkpoint().unwrap(), successor);
    successor_normal.assert_unchanged(&probe);
    successor_chase.assert_unchanged(&probe);
    assert_eq!(control.checkpoint().unwrap(), checkpoint);
    normal.assert_unchanged(&old);
    active.assert_unchanged(&old);
    assert_eq!(
        std::fs::read(path.0.join("world.checkpoint")).unwrap(),
        disk
    );
    drop((probe_world, probe));
    // The rejected wrapper dropped its owned lease, so A immediately reopens.
    let (_store, mut world, mut consumer) = reopen_store(&path, &profile, &session, &checkpoint);
    assert!(!consumer
        .providers
        .route
        .shares_native_query(&old.providers.route));
    let (fresh_normal, fresh_active) = owners(&world, &consumer);
    let boundary = consumer.boundary.clone();
    consumer.fail_after_movement = true;
    assert!(world
        .step(world.state().tick + 1, None, consumer.as_mut())
        .is_err());
    assert_eq!(world.checkpoint().unwrap(), checkpoint);
    assert!(world.records().is_empty());
    assert_eq!(consumer.boundary, boundary);
    fresh_normal.assert_unchanged(&consumer);
    fresh_active.assert_unchanged(&consumer);
    assert_eq!(
        std::fs::read(path.0.join("world.checkpoint")).unwrap(),
        disk
    );
    consumer.fail_after_movement = false;
    paired_step(&mut world, &mut consumer, &mut control, &mut old, None);
    assert_eq!(
        std::fs::read(path.0.join("world.checkpoint")).unwrap(),
        disk
    );
}
