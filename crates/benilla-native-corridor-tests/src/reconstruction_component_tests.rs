// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual fresh-query reconstruction; serialized Route facts are never resolved.
use super::{
    core::Session,
    route_commit::RouteCommitBoundary,
    source_cache_reconstruction::{self, ReconstructionAuthority},
    source_route_same_polygon::SourceSamePolygonRouteProvider,
    wander_component_tests::{self as wander, Consumer},
};
use benilla_world_runtime::native::{
    Authority, AuthorityReply, AuthorityRequest, MotionQuery, NativeError, NativeWorld, Profile,
    Query,
};
use benilla_world_store::native::{reconstruction::RecipeStore, Identity};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

impl ReconstructionAuthority for Consumer {
    fn route_state(
        &mut self,
    ) -> (
        &Session,
        &mut SourceSamePolygonRouteProvider,
        &mut RouteCommitBoundary,
    ) {
        (&self.session, &mut self.providers.route, &mut self.boundary)
    }
}

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(std::env::temp_dir().join(format!(
            "benilla-native-reconstruction-{}-{}",
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

fn reopen(path: &Directory, profile: &Profile) -> (RecipeStore, NativeWorld, Consumer) {
    let (fresh, authority) = wander::setup_with_schedule(Some(8), None);
    assert_eq!(fresh.profile(), profile);
    let identity = Identity::of(&fresh.checkpoint().unwrap());
    let (store, recipe) = RecipeStore::open(&path.0, identity, |anchor| {
        NativeWorld::restore_synthetic_component_test(profile.clone(), anchor)
    })
    .unwrap();
    source_cache_reconstruction::reconstruct(store, &recipe, fresh, authority).unwrap()
}

#[test]
fn fresh_mesh_reconstructs_no_path_then_mid_wander_arrival_next_leg_and_second_save() {
    let path = Directory::new();
    let (mut world, mut authority) = wander::setup_with_schedule(Some(8), None);
    let profile = world.profile().clone();
    let (mut store, receipt) = RecipeStore::create(&path.0, &world).unwrap();
    receipt.acknowledge(&mut world).unwrap();
    // This seed reaches the real minimum-distance NOPATH before a normal leg.
    let first = wander::step(&mut world, &mut authority);
    assert!(!wander::launched(&first));
    assert_eq!(
        wander::wander(&wander::snapshot(&world))
            .pathfinder_cache
            .as_ref()
            .unwrap()
            .path_type,
        8
    );
    store.checkpoint(&mut world).unwrap();
    let mut serialized_dispatches = 0;
    for reply in &store.recipe().records()[0].replies {
        let Query::SourceMotion(MotionQuery::Route(route)) = &reply.request.query else {
            continue;
        };
        let Some(dispatch) = &route.pathfinder_dispatch else {
            continue;
        };
        serialized_dispatches += 1;
        assert!(!dispatch.same_live_dispatch(dispatch));
        if dispatch.first_construction() {
            assert_eq!(
                dispatch.claim_constructor_query(),
                Err(NativeError::AuthorityUnavailable(
                    "live pathfinder dispatch"
                ))
            );
        }
    }
    assert!(serialized_dispatches > 0);
    for _ in 0..9 {
        wander::step(&mut world, &mut authority);
    }
    let moving = world.checkpoint().unwrap();
    assert!(
        wander::snapshot(&world)
            .spline
            .as_ref()
            .unwrap()
            .spline
            .elapsed_ms
            > 0
    );
    assert!(
        !wander::snapshot(&world)
            .spline
            .as_ref()
            .unwrap()
            .spline
            .finalized
    );
    store.checkpoint(&mut world).unwrap();
    assert_eq!(
        store.recipe().records().len(),
        10,
        "ordinary journal clearing retains the anchor recipe"
    );
    drop((world, authority, store));

    let (mut store, mut resumed, mut authority) = reopen(&path, &profile);
    assert_eq!(resumed.checkpoint().unwrap(), moving);
    wander::assert_loaded_checkpoint_rejected(&resumed, &moving);
    let (mut uninterrupted, mut baseline) = wander::setup_with_schedule(Some(8), None);
    for _ in 0..10 {
        wander::step(&mut uninterrupted, &mut baseline);
    }
    let mut arrived = false;
    let mut relaunched = false;
    for _ in 0..150 {
        let actual = wander::step(&mut resumed, &mut authority);
        let expected = wander::step(&mut uninterrupted, &mut baseline);
        assert_eq!(actual, expected);
        assert_eq!(
            resumed.checkpoint().unwrap(),
            uninterrupted.checkpoint().unwrap()
        );
        arrived |= wander::snapshot(&resumed)
            .spline
            .as_ref()
            .is_some_and(|s| s.spline.finalized);
        if wander::launched(&actual) {
            relaunched = true;
            break;
        }
    }
    assert!(arrived && relaunched);
    let next_leg = resumed.checkpoint().unwrap();
    store.checkpoint(&mut resumed).unwrap();
    assert!(store.recipe().records().len() > 10);
    drop((resumed, authority, store));
    let (_store, mut resumed, mut authority) = reopen(&path, &profile);
    assert_eq!(resumed.checkpoint().unwrap(), next_leg);
    for _ in 0..3 {
        assert_eq!(
            wander::step(&mut resumed, &mut authority),
            wander::step(&mut uninterrupted, &mut baseline)
        );
    }
    assert_eq!(
        resumed.checkpoint().unwrap(),
        uninterrupted.checkpoint().unwrap()
    );
}

struct ReceiptOnlyAuthority {
    inner: Consumer,
    replies: Vec<AuthorityReply>,
}

struct FailAfterPrefixAuthority {
    inner: Consumer,
    fail_tick: u64,
}
impl Authority for FailAfterPrefixAuthority {
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        if reply.request.binding.tick == self.fail_tick {
            return Err(NativeError::AuthorityUnavailable(
                "reconstruction prefix fault",
            ));
        }
        self.inner.observe_derived(reply)
    }
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        self.inner.resolve(request)
    }
}
impl ReconstructionAuthority for FailAfterPrefixAuthority {
    fn route_state(
        &mut self,
    ) -> (
        &Session,
        &mut SourceSamePolygonRouteProvider,
        &mut RouteCommitBoundary,
    ) {
        self.inner.route_state()
    }
}

#[test]
fn failure_after_reconstructed_no_path_and_normal_leg_discards_the_whole_fresh_session() {
    let path = Directory::new();
    let (mut original, mut authority) = wander::setup_with_schedule(Some(8), None);
    let profile = original.profile().clone();
    let (mut store, _) = RecipeStore::create(&path.0, &original).unwrap();
    for _ in 0..7 {
        wander::step(&mut original, &mut authority);
    }
    store.checkpoint(&mut original).unwrap();
    let expected = original.checkpoint().unwrap();
    drop((store, authority));
    let bytes = std::fs::read(path.0.join("world.checkpoint")).unwrap();
    let (store, recipe) = RecipeStore::open(&path.0, Identity::of(&expected), |anchor| {
        NativeWorld::restore_synthetic_component_test(profile.clone(), anchor)
    })
    .unwrap();
    let (fresh, authority) = wander::setup_with_schedule(Some(8), None);
    assert!(matches!(
        source_cache_reconstruction::reconstruct(
            store,
            &recipe,
            fresh,
            FailAfterPrefixAuthority {
                inner: authority,
                fail_tick: 7
            }
        ),
        Err(NativeError::AuthorityUnavailable(
            "reconstruction prefix fault"
        ))
    ));
    assert_eq!(
        std::fs::read(path.0.join("world.checkpoint")).unwrap(),
        bytes
    );
    assert_eq!(original.checkpoint().unwrap(), expected);
    let (_store, resumed, _authority) = reopen(&path, &profile);
    assert_eq!(resumed.checkpoint().unwrap(), expected);
}
impl Authority for ReceiptOnlyAuthority {
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        self.inner.observe_derived(reply)
    }
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if matches!(request.query, Query::SourceMotion(MotionQuery::Route(_))) {
            let expected = self
                .replies
                .iter()
                .find(|reply| reply.request == *request)
                .ok_or(NativeError::ReplayMismatch)?;
            // Even attaching stored facts to the actual fresh dispatch cannot
            // create a concrete provider proposal or its loaded cache lineage.
            Ok(AuthorityReply {
                request: request.clone(),
                facts: expected.facts.clone(),
            })
        } else {
            self.inner.resolve(request)
        }
    }
}
impl ReconstructionAuthority for ReceiptOnlyAuthority {
    fn route_state(
        &mut self,
    ) -> (
        &Session,
        &mut SourceSamePolygonRouteProvider,
        &mut RouteCommitBoundary,
    ) {
        self.inner.route_state()
    }
}

#[test]
fn receipt_only_replay_cannot_reconstruct_and_failure_keeps_saved_bytes_and_lock_reusable() {
    let path = Directory::new();
    let (mut original, mut authority) = wander::setup_with_schedule(Some(8), None);
    let profile = original.profile().clone();
    let (mut store, _) = RecipeStore::create(&path.0, &original).unwrap();
    wander::step(&mut original, &mut authority);
    store.checkpoint(&mut original).unwrap();
    let expected = original.checkpoint().unwrap();
    drop((store, authority));
    let bytes = std::fs::read(path.0.join("world.checkpoint")).unwrap();
    let (fresh, authority) = wander::setup_with_schedule(Some(8), None);
    let (store, recipe) = RecipeStore::open(&path.0, Identity::of(&expected), |anchor| {
        NativeWorld::restore_synthetic_component_test(profile.clone(), anchor)
    })
    .unwrap();
    let replies = recipe
        .records()
        .iter()
        .flat_map(|record| record.replies.clone())
        .collect();
    let result = source_cache_reconstruction::reconstruct(
        store,
        &recipe,
        fresh,
        ReceiptOnlyAuthority {
            inner: authority,
            replies,
        },
    );
    assert!(matches!(result, Err(NativeError::AuthorityBinding)));
    assert_eq!(
        std::fs::read(path.0.join("world.checkpoint")).unwrap(),
        bytes
    );
    assert_eq!(original.checkpoint().unwrap(), expected);
    let (_store, reopened, _authority) = reopen(&path, &profile);
    assert_eq!(reopened.checkpoint().unwrap(), expected);
}

#[test]
fn changed_genesis_geometry_binding_and_late_failure_publish_no_reconstructed_session() {
    let path = Directory::new();
    let (mut original, mut authority) = wander::setup_with_schedule(Some(8), None);
    let profile = original.profile().clone();
    let (mut store, _) = RecipeStore::create(&path.0, &original).unwrap();
    wander::step(&mut original, &mut authority);
    store.checkpoint(&mut original).unwrap();
    let expected = original.checkpoint().unwrap();
    drop((store, authority));
    let bytes = std::fs::read(path.0.join("world.checkpoint")).unwrap();
    let (store, recipe) = RecipeStore::open(&path.0, Identity::of(&expected), |anchor| {
        NativeWorld::restore_synthetic_component_test(profile.clone(), anchor)
    })
    .unwrap();
    let (wrong, authority) = wander::setup_with_schedule(Some(9), None);
    assert!(matches!(
        source_cache_reconstruction::reconstruct(store, &recipe, wrong, authority),
        Err(NativeError::InvalidCheckpoint)
    ));

    let (store, recipe) = RecipeStore::open(&path.0, Identity::of(&expected), |anchor| {
        NativeWorld::restore_synthetic_component_test(profile.clone(), anchor)
    })
    .unwrap();
    let (fresh, mut authority) = wander::setup_with_schedule(Some(8), None);
    authority.session.geometry[0] ^= 1;
    assert!(matches!(
        source_cache_reconstruction::reconstruct(store, &recipe, fresh, authority),
        Err(NativeError::AuthorityBinding)
    ));

    let (store, recipe) = RecipeStore::open(&path.0, Identity::of(&expected), |anchor| {
        NativeWorld::restore_synthetic_component_test(profile.clone(), anchor)
    })
    .unwrap();
    let (fresh, mut authority) = wander::setup_with_schedule(Some(8), None);
    authority.fail_after_movement = true;
    assert!(source_cache_reconstruction::reconstruct(store, &recipe, fresh, authority).is_err());
    assert_eq!(
        std::fs::read(path.0.join("world.checkpoint")).unwrap(),
        bytes
    );
    let (_store, reopened, _authority) = reopen(&path, &profile);
    assert_eq!(reopened.checkpoint().unwrap(), expected);
}

#[test]
fn rolled_back_first_calculation_cannot_supply_reconstruction_candidates() {
    let path = Directory::new();
    let (mut original, mut authority) = wander::setup_with_schedule(Some(8), None);
    let profile = original.profile().clone();
    let (mut store, _) = RecipeStore::create(&path.0, &original).unwrap();
    wander::step(&mut original, &mut authority);
    store.checkpoint(&mut original).unwrap();
    let expected = original.checkpoint().unwrap();
    drop((store, authority));
    let (store, recipe) = RecipeStore::open(&path.0, Identity::of(&expected), |anchor| {
        NativeWorld::restore_synthetic_component_test(profile.clone(), anchor)
    })
    .unwrap();
    let (mut fresh, mut authority) = wander::setup_with_schedule(Some(8), None);
    let anchor = fresh.checkpoint().unwrap();
    authority.fail_after_movement = true;
    assert!(fresh.step(1, None, &mut authority).is_err());
    assert_eq!(fresh.checkpoint().unwrap(), anchor);
    assert!(authority.providers.route.registered_constructor_count() > 0);
    authority.fail_after_movement = false;
    let replies = recipe
        .records()
        .iter()
        .flat_map(|record| record.replies.clone())
        .collect();
    assert!(matches!(
        source_cache_reconstruction::reconstruct(
            store,
            &recipe,
            fresh,
            ReceiptOnlyAuthority {
                inner: authority,
                replies
            }
        ),
        Err(NativeError::AuthorityBinding)
    ));
    let (_store, resumed, _authority) = reopen(&path, &profile);
    assert_eq!(resumed.checkpoint().unwrap(), expected);
}

#[test]
fn blocked_store_cannot_publish_a_session_from_its_old_recipe() {
    let path = Directory::new();
    let (mut original, mut authority) = wander::setup_with_schedule(Some(8), None);
    let (mut store, _) = RecipeStore::create(&path.0, &original).unwrap();
    let file = path.0.join("world.checkpoint");
    let bytes = std::fs::read(&file).unwrap();
    wander::step(&mut original, &mut authority);
    let prepared = store.prepare(&original).unwrap();
    let mut changed = bytes;
    changed[0] ^= 1;
    std::fs::write(&file, &changed).unwrap();
    assert!(store.commit(prepared).is_err());
    assert!(store.requires_reopen());
    let recipe = store.recipe().clone();
    let (fresh, authority) = wander::setup_with_schedule(Some(8), None);
    assert!(matches!(
        source_cache_reconstruction::reconstruct(store, &recipe, fresh, authority),
        Err(NativeError::InvalidCheckpoint)
    ));
    assert_eq!(std::fs::read(&file).unwrap(), changed);
}

fn millisecond_scene(profile: &mut Profile) {
    // Authored supported clock resolution only. Corpse/respawn source delays
    // remain unchanged; no periodic death/reset is injected by persistence.
    profile.tick_ms = 1;
}

fn reopen_reanchored(path: &Directory, profile: &Profile) -> (RecipeStore, NativeWorld, Consumer) {
    let identity_world = wander::setup_with_profile(Some(0), None, millisecond_scene).0;
    let identity = Identity::of(&identity_world.checkpoint().unwrap());
    let (store, recipe) = RecipeStore::open(&path.0, identity, |anchor| {
        NativeWorld::restore_synthetic_component_test(profile.clone(), anchor)
    })
    .unwrap();
    let (fresh, authority) = wander::setup_with_checkpoint(
        Some(0),
        None,
        millisecond_scene,
        Some(recipe.anchor().clone()),
    );
    source_cache_reconstruction::reconstruct(store, &recipe, fresh, authority).unwrap()
}

#[test]
fn natural_death_reanchors_then_fresh_respawn_crosses_the_old_total_tick_limit() {
    use benilla_world_runtime::native::synthetic_fixture as f;
    let path = Directory::new();
    let (mut world, mut authority) = wander::setup_with_profile(Some(0), None, millisecond_scene);
    let (mut baseline, mut baseline_authority) =
        wander::setup_with_profile(Some(0), None, millisecond_scene);
    let profile = world.profile().clone();
    let original_id = world.state().npcs[&2].id;
    let (mut store, receipt) = RecipeStore::create(&path.0, &world).unwrap();
    receipt.acknowledge(&mut world).unwrap();
    assert_eq!(
        wander::step(&mut world, &mut authority),
        wander::step(&mut baseline, &mut baseline_authority)
    );
    store.checkpoint(&mut world).unwrap();
    // A live source Wander has retained native state. Saving does not authorize
    // truncating its constructor history or resetting the actual generator.
    assert!(world.validate_saved_session_reopen().is_err());
    assert!(store.prepare_reanchor(&world).is_err());
    let command = f::cast(&world, 60001);
    assert_eq!(
        world
            .step(
                world.state().tick + 1,
                Some(command.clone()),
                &mut authority
            )
            .unwrap(),
        baseline
            .step(
                baseline.state().tick + 1,
                Some(command),
                &mut baseline_authority
            )
            .unwrap()
    );
    authority.acknowledge(&world);
    baseline_authority.acknowledge(&baseline);
    let mut reanchors = 0;
    let mut reopened = 0;
    let mut died = false;
    let mut respawned = false;
    for _ in 0..4000 {
        let actual = wander::step(&mut world, &mut authority);
        let expected = wander::step(&mut baseline, &mut baseline_authority);
        assert_eq!(actual, expected);
        assert_eq!(world.checkpoint().unwrap(), baseline.checkpoint().unwrap());
        let dead = world.state().npcs[&2].health == 0;
        let first_death = dead && !died;
        died |= dead;
        if first_death || world.state().tick.is_multiple_of(128) {
            store.checkpoint(&mut world).unwrap();
            let checkpoint = baseline.checkpoint().unwrap();
            baseline.checkpoint_committed(&checkpoint).unwrap();
            if world.validate_saved_session_reopen().is_ok() {
                let saved = world.checkpoint().unwrap();
                let prepared = store.prepare_reanchor(&world).unwrap();
                store
                    .commit(prepared)
                    .unwrap()
                    .acknowledge(&mut world)
                    .unwrap();
                assert_eq!(store.recipe().anchor(), &saved);
                assert!(store.recipe().records().is_empty());
                assert_eq!(world.checkpoint().unwrap(), saved);
                reanchors += 1;
                if first_death || world.state().tick.is_multiple_of(512) {
                    drop((world, authority, store));
                    (store, world, authority) = reopen_reanchored(&path, &profile);
                    assert_eq!(world.checkpoint().unwrap(), saved);
                    reopened += 1;
                }
            }
        }
        if world.state().npcs[&2].id.generation > original_id.generation {
            respawned = true;
            break;
        }
    }
    assert!(died && respawned);
    assert!(reanchors > 2 && reopened > 2);
    // Source respawn time uses whole-second clocks; the minimum can fall at
    // tick 2000 in this authored resolution. Continue actual movement past 2048.
    while world.state().tick <= 2100 {
        assert_eq!(
            wander::step(&mut world, &mut authority),
            wander::step(&mut baseline, &mut baseline_authority)
        );
    }
    assert!(world.state().tick > 2048);
    // The source respawn constructs a new real owner after the final cache-free
    // anchor. Save/reopen its loaded movement as well, then compare continuation.
    for _ in 0..3 {
        assert_eq!(
            wander::step(&mut world, &mut authority),
            wander::step(&mut baseline, &mut baseline_authority)
        );
    }
    assert!(world.validate_saved_session_reopen().is_err());
    store.checkpoint(&mut world).unwrap();
    let moving = world.checkpoint().unwrap();
    assert!(store.prepare_reanchor(&world).is_err());
    drop((world, authority, store));
    let (_store, mut world, mut authority) = reopen_reanchored(&path, &profile);
    assert_eq!(world.checkpoint().unwrap(), moving);
    for _ in 0..3 {
        assert_eq!(
            wander::step(&mut world, &mut authority),
            wander::step(&mut baseline, &mut baseline_authority)
        );
    }
    assert_eq!(world.checkpoint().unwrap(), baseline.checkpoint().unwrap());
}

fn home_reset_scene(profile: &mut Profile) {
    use benilla_sim::creature_ai as ai;
    use benilla_world_runtime::native::synthetic_fixture as f;
    profile.player.start = ai::Point::new(12., 12., 0.).unwrap();
    profile.npcs[0].ai.pursuit_ms = 1;
    profile.npcs[0].ai.leash_radius = f::real(0.01);
}

#[test]
fn natural_home_reset_reanchors_without_changing_generation_or_next_wander() {
    use benilla_sim::{creature_ai::movement::Check, creature_motion as motion};
    let path = Directory::new();
    let (mut world, mut authority) =
        wander::setup_with_profile(Some(0), Some(800), home_reset_scene);
    let profile = world.profile().clone();
    let original_id = world.state().npcs[&2].id;
    let identity = Identity::of(&world.checkpoint().unwrap());
    let (mut store, receipt) = RecipeStore::create(&path.0, &world).unwrap();
    receipt.acknowledge(&mut world).unwrap();
    for _ in 0..7 {
        wander::step(&mut world, &mut authority);
    }
    let old_construction = wander::wander(&wander::snapshot(&world)).pathfinder_construction;
    authority.allow_chase = true;
    authority.other.visibility = Some(Check::Passed);
    wander::step(&mut world, &mut authority);
    assert!(matches!(
        wander::snapshot(&world).generator,
        motion::Generator::Home { .. }
    ));
    store.checkpoint(&mut world).unwrap();
    assert!(
        store.prepare_reanchor(&world).is_err(),
        "suspended Wander is still owned during Home"
    );
    authority.other.visibility = Some(Check::Failed);
    let mut reset = false;
    for _ in 0..100 {
        wander::step(&mut world, &mut authority);
        let state = wander::snapshot(&world);
        if matches!(&state.generator, motion::Generator::Wander(w) if w.pathfinder_cache.is_none())
            && state.spline.is_none()
            && state.suspended_wander.is_none()
        {
            reset = true;
            break;
        }
    }
    assert!(reset);
    assert_eq!(world.state().npcs[&2].id, original_id);
    assert_ne!(
        wander::wander(&wander::snapshot(&world)).pathfinder_construction,
        old_construction
    );
    store.checkpoint(&mut world).unwrap();
    let saved = world.checkpoint().unwrap();
    let prepared = store.prepare_reanchor(&world).unwrap();
    store
        .commit(prepared)
        .unwrap()
        .acknowledge(&mut world)
        .unwrap();
    assert_eq!(store.recipe().anchor(), &saved);
    assert!(store.recipe().records().is_empty());
    // Preserve an uninterrupted owner for exact comparison; reopened authority
    // is a distinct actual mesh/query allocation with no imported cached refs.
    drop(store);
    let (store, recipe) = RecipeStore::open(&path.0, identity, |anchor| {
        NativeWorld::restore_synthetic_component_test(profile.clone(), anchor)
    })
    .unwrap();
    let (fresh, fresh_authority) = wander::setup_with_checkpoint(
        Some(0),
        Some(800),
        home_reset_scene,
        Some(recipe.anchor().clone()),
    );
    let (_store, mut resumed, mut fresh_authority) =
        source_cache_reconstruction::reconstruct(store, &recipe, fresh, fresh_authority).unwrap();
    for _ in 0..3 {
        assert_eq!(
            wander::step(&mut resumed, &mut fresh_authority),
            wander::step(&mut world, &mut authority)
        );
    }
    assert_eq!(resumed.checkpoint().unwrap(), world.checkpoint().unwrap());
    let cache = wander::wander(&wander::snapshot(&resumed))
        .pathfinder_cache
        .clone()
        .unwrap();
    assert_eq!(cache.revision, 1);
    assert_eq!(cache.binding.owner_generation, original_id.generation);
}
