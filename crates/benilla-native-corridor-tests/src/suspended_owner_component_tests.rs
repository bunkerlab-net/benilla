// SPDX-License-Identifier: GPL-3.0-or-later
//! Live source MotionMaster ownership across sustained Chase, local Home and
//! default Reset. Authored visibility/inputs use the actual loaded route provider.
//! Cached checkpoint reopen remains closed for the Chase/suspended shape.
use super::{
    route_commit::{authored_conflicts, RouteCommitBoundary, RouteCommitNotice},
    wander_component_tests::{self as wander, Consumer},
};
use benilla_sim::{creature_ai as ai, creature_ai::movement::Check, creature_motion as motion};
use benilla_world_runtime::{
    native::{synthetic_fixture as f, *},
    NpcId,
};

#[derive(Clone)]
struct LiveOwner {
    npc: NpcId,
    construction: motion::QueryBinding,
    candidate: usize,
    cache: motion::PathFinderCache,
    provider: usize,
    first_event: PathfinderDispatchReceipt,
    selected: bool,
    capture: Option<Vec<u8>>,
}
impl LiveOwner {
    fn read(consumer: &Consumer, npc: NpcId, construction: motion::QueryBinding) -> Self {
        let (candidate, cache) = consumer
            .providers
            .route
            .committed_candidate_for_constructor(npc, construction)
            .expect("actual constructor-keyed committed candidate");
        // This helper checks RetainedSourceQuery against the original loaded
        // native allocation before returning its provider identity and seal.
        let (provider, first_event, selected) = consumer
            .providers
            .route
            .registered_constructor_for(npc, construction)
            .expect("original retained native constructor allocation");
        Self {
            npc,
            construction,
            candidate,
            cache: cache.clone(),
            provider,
            first_event,
            selected,
            capture: consumer
                .providers
                .route
                .reconstruction_capture_for(npc, construction),
        }
    }
    fn assert_constructor(&self, consumer: &Consumer) {
        let (provider, first_event, selected) = consumer
            .providers
            .route
            .registered_constructor_for(self.npc, self.construction)
            .expect("live constructor must not be pruned or replaced");
        assert_eq!(provider, self.provider);
        assert!(first_event.same_live_dispatch(&self.first_event));
        assert_eq!(selected, self.selected);
        assert!(selected);
    }
    fn assert_unchanged(&self, consumer: &Consumer) {
        self.assert_constructor(consumer);
        assert_eq!(
            consumer
                .providers
                .route
                .committed_candidate_for_constructor(self.npc, self.construction),
            Some((self.candidate, &self.cache)),
        );
        assert_eq!(
            consumer
                .providers
                .route
                .reconstruction_capture_for(self.npc, self.construction),
            self.capture,
        );
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
    }
}

fn committed_owners(consumer: &Consumer) -> Vec<LiveOwner> {
    consumer
        .boundary
        .heads()
        .iter()
        .flat_map(|head| {
            head.owner_heads()
                .filter(|owner| owner.cache().is_some())
                .map(move |owner| {
                    LiveOwner::read(consumer, head.npc(), owner.construction().unwrap())
                })
        })
        .collect()
}

fn counts(consumer: &Consumer) -> (usize, usize, usize, usize) {
    let provider = &consumer.providers.route;
    (
        provider.registered_generator_count(),
        provider.registered_constructor_count(),
        provider.retained_candidate_count(),
        provider.certified_candidate_count(),
    )
}

fn acknowledge_exact(world: &NativeWorld, consumer: &mut Consumer) {
    let notice = RouteCommitNotice::capture(world, &consumer.boundary).unwrap();
    let before = committed_owners(consumer);
    let counts_before = counts(consumer);
    if notice
        .boundary()
        .heads()
        .iter()
        .any(|head| head.suspended_wander().is_some())
    {
        let mut invalid = vec![
            authored_conflicts::wrong_suspended_cache(&notice),
            authored_conflicts::wrong_suspended_construction(&notice),
        ];
        if notice
            .boundary()
            .heads()
            .iter()
            .any(|head| head.cache().is_some())
        {
            invalid.push(authored_conflicts::crossed_owner_roles(&notice));
        }
        for invalid in invalid {
            assert!(consumer
                .providers
                .route
                .acknowledge(&consumer.session, &invalid)
                .is_err());
            assert_eq!(counts(consumer), counts_before);
            for owner in &before {
                owner.assert_unchanged(consumer);
            }
        }
    }
    consumer
        .providers
        .route
        .acknowledge(&consumer.session, &notice)
        .unwrap();
    consumer.boundary = notice.boundary().clone();
    let committed = committed_owners(consumer);
    let counts_after = counts(consumer);
    consumer
        .providers
        .route
        .acknowledge(&consumer.session, &notice)
        .unwrap();
    assert_eq!(
        counts(consumer),
        counts_after,
        "duplicate ACK cannot retain history"
    );
    for owner in &committed {
        owner.assert_unchanged(consumer);
    }
    consumer.boundary.validate_world(world).unwrap();
}

fn step_input(
    world: &mut NativeWorld,
    consumer: &mut Consumer,
    input: Option<OwnedInput>,
) -> TickRecord {
    let record = world
        .step(world.state().tick + 1, input, consumer)
        .unwrap()
        .clone();
    acknowledge_exact(world, consumer);
    record
}

fn reject_and_retry(
    world: &mut NativeWorld,
    consumer: &mut Consumer,
    input: Option<OwnedInput>,
) -> TickRecord {
    let checkpoint = world.checkpoint().unwrap();
    let boundary = consumer.boundary.clone();
    let records = world.records().to_vec();
    let owners = committed_owners(consumer);
    let route_floor = consumer.route_replies.len();
    consumer.fail_after_movement = true;
    consumer.rejected = None;
    let tick = world.state().tick + 1;
    assert_eq!(
        world.step(tick, input.clone(), consumer),
        Err(NativeError::AuthorityUnavailable(
            "authored Wander post-movement failure"
        ))
    );
    let rejected = consumer
        .rejected
        .take()
        .expect("actual post-movement rejection");
    assert_eq!(world.checkpoint().unwrap(), checkpoint);
    assert_eq!(world.records(), records);
    assert_eq!(consumer.boundary, boundary);
    for owner in &owners {
        owner.assert_unchanged(consumer);
    }
    // Provisional constructors may stay alive for exact retry. Only the accepted
    // commit boundary owns heads; rollback never promotes the staged proposal.
    let staged: Vec<_> = consumer.route_replies[route_floor..]
        .iter()
        .map(|reply| {
            (
                reply.clone(),
                consumer.providers.route.candidate_identity(reply),
            )
        })
        .collect();
    consumer.fail_after_movement = false;
    let record = step_input(world, consumer, input);
    assert!(record.replies.contains(&rejected));
    for (reply, identity) in staged {
        assert!(record.replies.contains(&reply));
        let Some(dispatch) = route(&reply).pathfinder_dispatch else {
            continue;
        };
        if consumer
            .providers
            .route
            .registered_constructor_for(
                reply.request.binding.npc.unwrap(),
                dispatch.construction_binding(),
            )
            .is_some()
            && route(&reply).kind != motion::RouteKind::WanderPrepare
        {
            assert_eq!(
                consumer.providers.route.candidate_identity(&reply),
                identity
            );
        }
    }
    record
}

fn route(reply: &AuthorityReply) -> &motion::RouteRequest {
    let Query::SourceMotion(MotionQuery::Route(route)) = &reply.request.query else {
        panic!("actual source route reply")
    };
    &route.request
}

fn delayed_home_profile(profile: &mut Profile) {
    profile.player.start = ai::Point::new(5., 12., 0.).unwrap();
    profile.npcs[0].ai.pursuit_ms = 2500;
    profile.npcs[0].ai.leash_radius = f::real(0.01);
    profile
        .source_motion
        .as_mut()
        .unwrap()
        .npcs
        .get_mut(&2)
        .unwrap()
        .chase_walking = true;
}

fn enter_chase(
    world: &mut NativeWorld,
    consumer: &mut Consumer,
) -> (motion::WanderSnapshot, LiveOwner, LiveOwner) {
    assert!(wander::launched(&step_input(world, consumer, None)));
    while world.state().tick < 7 {
        step_input(world, consumer, None);
    }
    let default = wander::wander(&wander::snapshot(world)).clone();
    let npc = world.state().npcs[&2].id;
    let suspended = LiveOwner::read(consumer, npc, default.pathfinder_construction);
    assert!(
        suspended.capture.is_some(),
        "accepted live Wander reconstruction capture"
    );
    consumer.allow_chase = true;
    consumer.other.visibility = Some(Check::Passed);
    let record = reject_and_retry(world, consumer, None);
    assert_eq!(world.state().npcs[&2].ai.phase(), ai::Phase::Combat);
    let state = wander::snapshot(world);
    assert_eq!(state.suspended_wander.as_ref(), Some(&default));
    let motion::Generator::Chase(chase) = &state.generator else {
        panic!("sustained Chase")
    };
    let chase = LiveOwner::read(consumer, npc, chase.pathfinder_construction.unwrap());
    assert_ne!(suspended.construction, chase.construction);
    assert_ne!(suspended.candidate, chase.candidate);
    assert!(
        chase.capture.is_none(),
        "Wander capture does not certify Chase"
    );
    assert!(record.replies.iter().any(|reply| matches!(
        &reply.request.query, Query::SourceMotion(MotionQuery::Route(route))
            if route.request.kind == motion::RouteKind::ChaseInitial
    )));
    suspended.assert_unchanged(consumer);
    assert_shape(consumer, Some(&chase), Some(&suspended));
    (default, suspended, chase)
}

fn assert_shape(consumer: &Consumer, current: Option<&LiveOwner>, suspended: Option<&LiveOwner>) {
    let [head] = consumer.boundary.heads() else {
        panic!("one ordered NPC record")
    };
    assert_eq!(head.construction(), current.map(|owner| owner.construction));
    assert_eq!(head.cache(), current.map(|owner| &owner.cache));
    assert_eq!(
        head.suspended_wander()
            .and_then(|owner| owner.construction()),
        suspended.map(|owner| owner.construction)
    );
    assert_eq!(
        head.suspended_wander().and_then(|owner| owner.cache()),
        suspended.map(|owner| &owner.cache)
    );
    let total = usize::from(current.is_some()) + usize::from(suspended.is_some());
    assert_eq!(counts(consumer), (total, total, total, total));
}

fn assert_cached_reopen_closed(world: &NativeWorld, consumer: &Consumer) {
    let checkpoint = world.checkpoint().unwrap();
    let boundary = consumer.boundary.clone();
    let owners = committed_owners(consumer);
    wander::assert_loaded_checkpoint_rejected(world, &checkpoint);
    assert!(world.validate_saved_session_reopen().is_err());
    assert!(RouteCommitBoundary::capture_initial(world).is_err());
    assert!(NativeWorld::prepare_synthetic_wander_reopen(
        world.profile().clone(),
        checkpoint.clone()
    )
    .is_err());
    assert!(consumer
        .providers
        .route
        .encode_wander_reconstruction(&consumer.session, world, &consumer.boundary)
        .is_err());
    assert_eq!(world.checkpoint().unwrap(), checkpoint);
    assert_eq!(consumer.boundary, boundary);
    for owner in &owners {
        owner.assert_unchanged(consumer);
    }
}

fn move_target(world: &NativeWorld, index: u32) -> OwnedInput {
    OwnedInput {
        authenticated_owner: world.state().player.owner,
        input: Input {
            world: world.state().world,
            player: world.state().player,
            tick: world.state().tick + 1,
            sequence: world.state().next_sequence,
            command: Command::MovementPose {
                destination: ai::Point::new(5. + index as f32 * 0.5, 12. - index as f32 * 0.5, 0.)
                    .unwrap(),
                orientation: world.state().orientation,
                movement_flags: motion::GroundMovementFlags::from_bits(1).unwrap(),
            },
        },
    }
}

#[test]
fn live_suspended_wander_survives_chase_replan_and_home_until_default_reset() {
    let (mut world, mut consumer) =
        wander::setup_with_profile(Some(0), Some(800), delayed_home_profile);
    let (default, suspended, initial_chase) = enter_chase(&mut world, &mut consumer);
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
    assert_cached_reopen_closed(&world, &consumer);
    let initial_reply = consumer
        .route_replies
        .iter()
        .find(|reply| route(reply).kind == motion::RouteKind::ChaseInitial)
        .unwrap()
        .clone();
    let mut chase = initial_chase.clone();
    let mut replans = 0;
    // These sequenced player moves make the real 250ms Chase check request a
    // retained replan. Every owner endpoint stays on the authored first polygon.
    for index in 1..=14 {
        let input = move_target(&world, index);
        let record = reject_and_retry(&mut world, &mut consumer, Some(input));
        assert_eq!(
            world.state().position,
            ai::Point::new(5. + index as f32 * 0.5, 12. - index as f32 * 0.5, 0.).unwrap()
        );
        assert_eq!(world.state().npcs[&2].ai.phase(), ai::Phase::Combat);
        assert_eq!(
            wander::snapshot(&world).suspended_wander.as_ref(),
            Some(&default)
        );
        for reply in record.replies.iter().filter(|reply| matches!(
            &reply.request.query, Query::SourceMotion(MotionQuery::Route(route)) if route.request.kind == motion::RouteKind::ChaseReplan
        )) {
            let request = route(reply);
            let dispatch = request.pathfinder_dispatch.unwrap();
            assert!(!dispatch.first_construction());
            assert_eq!(dispatch.construction_binding(), chase.construction);
            assert_eq!(request.prior_cache.as_deref(), Some(&chase.cache));
            assert_eq!(wander::calculation(reply).after_cache.revision, chase.cache.revision + 1);
            initial_chase.assert_constructor(&consumer);
            let next = LiveOwner::read(&consumer, npc, chase.construction);
            assert_ne!(next.candidate, chase.candidate);
            chase = next;
            replans += 1;
        }
        suspended.assert_unchanged(&consumer);
        assert_shape(&consumer, Some(&chase), Some(&suspended));
        wander::assert_dry_owner(&world);
    }
    assert!(
        replans > 0,
        "actual movement must reach a retained Chase replan"
    );
    assert!(
        consumer
            .providers
            .route
            .candidate_identity(&initial_reply)
            .is_none(),
        "old Chase candidate cannot be promoted from spline/cache history"
    );
    assert_cached_reopen_closed(&world, &consumer);
    let mut home_reply = None;
    for _ in 0..30 {
        let record = reject_and_retry(&mut world, &mut consumer, None);
        suspended.assert_unchanged(&consumer);
        if let Some(reply) = record.replies.iter().find(|reply| matches!(
            &reply.request.query, Query::SourceMotion(MotionQuery::Route(route)) if route.request.kind == motion::RouteKind::HomeForceDestination
        )) {
            home_reply = Some(reply.clone());
            break;
        }
        assert_eq!(world.state().npcs[&2].ai.phase(), ai::Phase::Combat);
        assert_shape(&consumer, Some(&chase), Some(&suspended));
    }
    let home_reply = home_reply.expect("actual CombatManager pursuit expiry starts Home");
    let home = route(&home_reply);
    assert_eq!(world.state().npcs[&2].ai.phase(), ai::Phase::ReturningHome);
    assert_eq!(home.goal, combat_start.position);
    assert_eq!(
        home.pathfinder_dispatch.unwrap().constructor_kind(),
        motion::PathfinderConstructorKind::HomeLocal
    );
    assert!(home.prior_cache.is_none());
    assert!(consumer
        .providers
        .route
        .registered_constructor_for(
            npc,
            home.pathfinder_dispatch.unwrap().construction_binding()
        )
        .is_none());
    assert!(consumer
        .providers
        .route
        .candidate_identity(&home_reply)
        .is_none());
    chase.assert_retired(&consumer);
    suspended.assert_unchanged(&consumer);
    assert_shape(&consumer, None, Some(&suspended));
    assert_cached_reopen_closed(&world, &consumer);
    consumer.other.visibility = Some(Check::Failed);
    let mut reset = false;
    for _ in 0..100 {
        let record = step_input(&mut world, &mut consumer, None);
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
        suspended.assert_unchanged(&consumer);
        assert_shape(&consumer, None, Some(&suspended));
    }
    assert!(reset, "Home arrival resets the default generator");
    let state = wander::snapshot(&world);
    let resumed = wander::wander(&state);
    assert_eq!(state.map_pose.position, combat_start.position);
    assert_eq!(resumed.next_move_count, default.next_move_count);
    assert_eq!(resumed.next_move_timer_ms, 0);
    assert_ne!(
        resumed.pathfinder_construction,
        default.pathfinder_construction
    );
    assert!(resumed.pathfinder_cache.is_none());
    assert!(state.suspended_wander.is_none() && state.spline.is_none());
    suspended.assert_retired(&consumer);
    chase.assert_retired(&consumer);
    assert_eq!(counts(&consumer), (0, 0, 0, 0));
    assert_eq!(
        consumer.boundary.heads()[0].construction(),
        Some(resumed.pathfinder_construction)
    );
    assert_eq!(world.state().npcs[&2].id, npc);
    assert_eq!(
        world.state().npcs[&2].initialized_creature(),
        Some(&initialized)
    );
    let new_construction = resumed.pathfinder_construction;
    let record = reject_and_retry(&mut world, &mut consumer, None);
    assert!(wander::launched(&record));
    let new_owner = LiveOwner::read(&consumer, npc, new_construction);
    assert_eq!(new_owner.cache.revision, 1);
    assert!(new_owner.capture.is_some());
    assert!(!new_owner
        .first_event
        .same_live_dispatch(&suspended.first_event));
    assert_shape(&consumer, Some(&new_owner), None);
    suspended.assert_retired(&consumer);
    chase.assert_retired(&consumer);
}

#[test]
fn actual_lethal_spell_clears_both_live_constructors_without_promoting_history() {
    let (mut world, mut consumer) = wander::setup_with_profile(Some(0), Some(800), |profile| {
        delayed_home_profile(profile);
        profile.npcs[0].ai.pursuit_ms = 15000;
    });
    let (_, suspended, chase) = enter_chase(&mut world, &mut consumer);
    let npc = world.state().npcs[&2].id;
    let cast = f::cast(&world, 60001);
    let record = step_input(&mut world, &mut consumer, Some(cast));
    assert!(record
        .events
        .iter()
        .any(|event| matches!(event, Event::InputResult { accepted: true, .. })));
    consumer.other.visibility = Some(Check::Failed);
    for _ in 0..30 {
        if world.state().npcs[&2].health == 0 {
            break;
        }
        step_input(&mut world, &mut consumer, None);
    }
    assert_eq!(
        world.state().npcs[&2].health,
        0,
        "actual source projectile kills the initialized owner"
    );
    assert_eq!(world.state().npcs[&2].id, npc);
    let state = wander::snapshot(&world);
    assert!(matches!(state.generator, motion::Generator::Idle));
    assert!(state.suspended_wander.is_none() && state.spline.is_none());
    suspended.assert_retired(&consumer);
    chase.assert_retired(&consumer);
    assert_shape(&consumer, None, None);
    assert!(consumer.route_replies.iter().all(|reply| consumer
        .providers
        .route
        .candidate_identity(reply)
        .is_none()));
    step_input(&mut world, &mut consumer, None);
    assert_eq!(counts(&consumer), (0, 0, 0, 0));
}

/// Reconstruction reexecutes the fixture's time-dependent visibility policy;
/// historical route replies are never supplied as fresh authority answers.
struct HistoryAuthority {
    consumer: Consumer,
    reject_tick: Option<u64>,
}
impl Authority for HistoryAuthority {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        self.consumer.other.visibility = Some(if request.binding.tick < 8 {
            Check::Failed
        } else {
            Check::Passed
        });
        self.consumer.resolve(request)
    }
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        if self.reject_tick == Some(reply.request.binding.tick)
            && matches!(
                reply.request.query,
                Query::SourceMotion(MotionQuery::Combat(CombatQuery::NpcSemantics(ref query)))
                    if query.phase == CombatPhase::AfterMovement
            )
        {
            return Err(NativeError::AuthorityUnavailable(
                "authored dual-owner reconstruction fault",
            ));
        }
        self.consumer.observe_derived(reply)
    }
}
impl super::source_cache_reconstruction::ReconstructionAuthority for HistoryAuthority {
    fn route_state(
        &mut self,
    ) -> (
        &super::Session,
        &mut super::source_route_same_polygon::SourceSamePolygonRouteProvider,
        &mut RouteCommitBoundary,
    ) {
        (
            &self.consumer.session,
            &mut self.consumer.providers.route,
            &mut self.consumer.boundary,
        )
    }
}

struct Directory(std::path::PathBuf);
impl Directory {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(std::env::temp_dir().join(format!(
            "benilla-suspended-owner-history-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        )))
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn complete_cache_free_anchor_history_reexecutes_both_owners_on_fresh_providers() {
    use benilla_world_store::native::{reconstruction::RecipeStore, Identity};
    let directory = Directory::new();
    let (mut world, mut consumer) =
        wander::setup_with_profile(Some(0), Some(800), delayed_home_profile);
    let profile = world.profile().clone();
    let identity = Identity::of(&world.checkpoint().unwrap());
    let (mut store, receipt) = RecipeStore::create(&directory.0, &world).unwrap();
    receipt.acknowledge(&mut world).unwrap();
    let (_, suspended, initial_chase) = enter_chase(&mut world, &mut consumer);
    for index in 1..=14 {
        let input = move_target(&world, index);
        step_input(&mut world, &mut consumer, Some(input));
    }
    let npc = world.state().npcs[&2].id;
    let chase = LiveOwner::read(&consumer, npc, initial_chase.construction);
    assert!(chase.cache.revision > initial_chase.cache.revision);
    suspended.assert_unchanged(&consumer);
    assert_shape(&consumer, Some(&chase), Some(&suspended));
    assert_cached_reopen_closed(&world, &consumer);
    store.checkpoint(&mut world).unwrap();
    let checkpoint = world.checkpoint().unwrap();
    let disk = std::fs::read(directory.0.join("world.checkpoint")).unwrap();
    drop(store);

    // The accepted initial Chase ACK precedes the injected failure. The failed
    // reconstruction owns and drops its fresh world, both providers and store
    // lock together, leaving the existing live pair and saved bytes untouched.
    for reject_tick in [Some(9), None] {
        let (store, recipe) = RecipeStore::open(&directory.0, identity.clone(), |anchor| {
            NativeWorld::restore_synthetic_component_test(profile.clone(), anchor)
        })
        .unwrap();
        assert_eq!(recipe.anchor().identity().tick, 0);
        let (fresh, mut authority) =
            wander::setup_with_profile(Some(0), Some(800), delayed_home_profile);
        authority.allow_chase = true;
        let result = super::source_cache_reconstruction::reconstruct(
            store,
            &recipe,
            fresh,
            HistoryAuthority {
                consumer: authority,
                reject_tick,
            },
        );
        if reject_tick.is_some() {
            assert!(matches!(
                result,
                Err(NativeError::AuthorityUnavailable(
                    "authored dual-owner reconstruction fault"
                ))
            ));
            assert_eq!(world.checkpoint().unwrap(), checkpoint);
            suspended.assert_unchanged(&consumer);
            chase.assert_unchanged(&consumer);
            assert_eq!(
                std::fs::read(directory.0.join("world.checkpoint")).unwrap(),
                disk
            );
            continue;
        }
        let (_store, mut reopened, mut authority) = result.unwrap();
        assert_eq!(reopened.checkpoint().unwrap(), checkpoint);
        assert_eq!(wander::snapshot(&reopened), wander::snapshot(&world));
        let fresh_suspended = LiveOwner::read(&authority.consumer, npc, suspended.construction);
        let fresh_chase = LiveOwner::read(&authority.consumer, npc, chase.construction);
        for (old, fresh) in [(&suspended, &fresh_suspended), (&chase, &fresh_chase)] {
            assert_eq!(fresh.cache, old.cache);
            assert_eq!(fresh.capture, old.capture);
            assert_ne!(fresh.provider, old.provider);
            assert_ne!(fresh.candidate, old.candidate);
            assert!(!fresh.first_event.same_live_dispatch(&old.first_event));
            fresh.assert_unchanged(&authority.consumer);
        }
        assert_shape(
            &authority.consumer,
            Some(&fresh_chase),
            Some(&fresh_suspended),
        );
        // This is complete source execution from a cache-free anchor, not direct
        // cached checkpoint reopen or a Chase native reconstruction certificate.
        assert_cached_reopen_closed(&reopened, &authority.consumer);
        assert_eq!(
            step_input(&mut reopened, &mut authority.consumer, None),
            step_input(&mut world, &mut consumer, None),
        );
        assert_eq!(reopened.checkpoint().unwrap(), world.checkpoint().unwrap());
        suspended.assert_unchanged(&consumer);
        fresh_suspended.assert_unchanged(&authority.consumer);
        assert_eq!(
            std::fs::read(directory.0.join("world.checkpoint")).unwrap(),
            disk
        );
    }
}
