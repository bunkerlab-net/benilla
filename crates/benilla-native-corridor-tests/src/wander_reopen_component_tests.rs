// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded current-Wander reopen on fresh loaded products and the real corridor.
//! Authored component ownership only; ordinary saved-session restore stays closed.
use super::wander_component_tests::{self as wander, Consumer};
use benilla_world_runtime::native::{Checkpoint, NativeError, NativeWorld, Profile, TickRecord};
use benilla_world_store::native::{wander_reconstruction::WanderStore, Identity};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(std::env::temp_dir().join(format!(
            "benilla-bounded-wander-reopen-{}-{}",
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

const REOPEN_SLICE: Duration = Duration::from_millis(500);

fn recipe(world: &NativeWorld, consumer: &Consumer) -> Vec<u8> {
    consumer
        .providers
        .route
        .encode_wander_reconstruction(&consumer.session, world, &consumer.boundary)
        .unwrap()
}

fn normal_head(world: &NativeWorld) -> bool {
    let snapshot = wander::snapshot(world);
    snapshot.spline.is_some()
        && wander::wander(&snapshot)
            .pathfinder_cache
            .as_ref()
            .is_some_and(|cache| cache.path_type == 1 && cache.active_len == 1)
}

fn mid_leg(world: &NativeWorld) -> bool {
    normal_head(world)
        && wander::snapshot(world)
            .spline
            .as_ref()
            .is_some_and(|spline| spline.spline.elapsed_ms > 0 && !spline.spline.finalized)
}

fn revision(world: &NativeWorld) -> u64 {
    wander::wander(&wander::snapshot(world))
        .pathfinder_cache
        .as_ref()
        .unwrap()
        .revision
}

fn clear_journal(world: &mut NativeWorld) {
    let checkpoint = world.checkpoint().unwrap();
    world.checkpoint_committed(&checkpoint).unwrap();
    assert!(world.records().is_empty());
}

fn paired_step(
    world: &mut NativeWorld,
    consumer: &mut Consumer,
    baseline: &mut NativeWorld,
    uninterrupted: &mut Consumer,
) -> TickRecord {
    let actual = wander::step(world, consumer);
    let expected = wander::step(baseline, uninterrupted);
    assert_eq!(actual, expected);
    assert_eq!(world.checkpoint().unwrap(), baseline.checkpoint().unwrap());
    assert_eq!(world.state().rng_counter, baseline.state().rng_counter);
    assert_eq!(wander::snapshot(world), wander::snapshot(baseline));
    actual
}

fn first_mid_leg(
    world: &mut NativeWorld,
    consumer: &mut Consumer,
    baseline: &mut NativeWorld,
    uninterrupted: &mut Consumer,
) {
    for _ in 0..128 {
        paired_step(world, consumer, baseline, uninterrupted);
        if mid_leg(world) {
            return;
        }
    }
    panic!("seed 8 must reach a live NORMAL mid-leg after its initial NOPATH");
}

fn reopen_bytes(
    profile: &Profile,
    checkpoint: Checkpoint,
    bytes: &[u8],
    budget: Duration,
) -> Result<(NativeWorld, Consumer), NativeError> {
    let (fresh, mut consumer) = wander::setup_with_schedule(Some(8), None);
    assert_eq!(fresh.profile(), profile);
    assert_eq!(fresh.state().tick, 0);
    assert_eq!(consumer.providers.route.registered_constructor_count(), 0);
    assert_eq!(consumer.providers.route.retained_candidate_count(), 0);
    drop(fresh);
    let pending =
        NativeWorld::prepare_synthetic_wander_reopen(profile.clone(), checkpoint.clone())?;
    let route = consumer.providers.route;
    let (world, route, boundary) = consumer.geometry.with_local_corridor(|corridor| {
        route.reopen_wander(
            &consumer.session,
            pending,
            bytes,
            corridor,
            Instant::now() + budget,
        )
    })?;
    consumer.providers.route = route;
    consumer.boundary = boundary;
    assert_eq!(world.checkpoint()?, checkpoint);
    assert!(world.records().is_empty());
    assert_eq!(consumer.providers.route.registered_constructor_count(), 1);
    assert_eq!(consumer.providers.route.retained_candidate_count(), 1);
    consumer.boundary.validate_world(&world)?;
    Ok((world, consumer))
}

fn reopen_store(
    path: &Directory,
    profile: &Profile,
    checkpoint: &Checkpoint,
) -> (WanderStore, NativeWorld, Consumer) {
    let (store, saved) = WanderStore::open(&path.0, Identity::of(checkpoint)).unwrap();
    assert_eq!(saved.checkpoint(), checkpoint);
    let (fresh, mut consumer) = wander::setup_with_schedule(Some(8), None);
    assert_eq!(fresh.profile(), profile);
    assert_eq!(fresh.state().tick, 0);
    assert_eq!(consumer.providers.route.registered_constructor_count(), 0);
    assert_eq!(consumer.providers.route.retained_candidate_count(), 0);
    drop(fresh);
    let pending =
        NativeWorld::prepare_synthetic_wander_reopen(profile.clone(), saved.checkpoint().clone())
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
                Instant::now() + REOPEN_SLICE,
            )
        })
        .unwrap();
    consumer.providers.route = route;
    consumer.boundary = boundary;
    assert_eq!(world.checkpoint().unwrap(), *checkpoint);
    assert_eq!(store.recipe(), &saved);
    assert!(world.records().is_empty());
    assert_eq!(consumer.providers.route.registered_constructor_count(), 1);
    assert_eq!(consumer.providers.route.retained_candidate_count(), 1);
    consumer.boundary.validate_world(&world).unwrap();
    (store, world, consumer)
}

#[test]
fn alive_wander_crosses_2048_ticks_with_repeated_bounded_mid_leg_reopens() {
    let path = Directory::new();
    let (mut world, mut consumer) = wander::setup_with_schedule(Some(8), None);
    let (mut baseline, mut uninterrupted) = wander::setup_with_schedule(Some(8), None);
    let profile = world.profile().clone();
    let original_owner = world.state().npcs[&2].id;
    first_mid_leg(&mut world, &mut consumer, &mut baseline, &mut uninterrupted);
    let bytes = recipe(&world, &consumer);
    let (mut store, _) = WanderStore::create(&path.0, &mut world, &bytes).unwrap();
    clear_journal(&mut baseline);
    let mut last_save = world.state().tick;
    let mut reopened = 0;
    let mut arrived = false;
    let mut waiting_for_arrival = false;
    let mut completed_continuations = 0;
    let thresholds = [10, 700, 1400, 2050];
    let mut saved_revisions = Vec::new();
    let mut max_recipe_operations = 0;
    for _ in 0..2500 {
        let record = paired_step(&mut world, &mut consumer, &mut baseline, &mut uninterrupted);
        assert_eq!(world.state().npcs[&2].id, original_owner);
        assert!(world.state().npcs[&2].health > 0);
        wander::assert_dry_owner(&world);
        if waiting_for_arrival {
            arrived |= wander::snapshot(&world)
                .spline
                .as_ref()
                .is_some_and(|spline| spline.spline.finalized);
            if arrived && wander::launched(&record) {
                completed_continuations += 1;
                waiting_for_arrival = false;
            }
        }
        let reopen_now = reopened < thresholds.len()
            && world.state().tick >= thresholds[reopened]
            && mid_leg(&world)
            && !waiting_for_arrival;
        if normal_head(&world) && (world.state().tick - last_save >= 128 || reopen_now) {
            let bytes = recipe(&world, &consumer);
            let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let count = value["operations"].as_array().unwrap().len();
            max_recipe_operations = max_recipe_operations.max(count);
            assert!(count <= benilla_native_nav::SOURCE_WANDER_MAX_RECONSTRUCTION_OPERATIONS);
            assert_eq!(value["operation_count"].as_u64(), Some(revision(&world)));
            assert!(bytes.len() < 8 * 1024 * 1024);
            store.checkpoint(&mut world, &bytes).unwrap();
            clear_journal(&mut baseline);
            assert!(world.records().is_empty());
            last_save = world.state().tick;
            if reopen_now {
                let checkpoint = world.checkpoint().unwrap();
                let before_motion = wander::snapshot(&world);
                let before_rng = world.state().rng_counter;
                let before_revision = revision(&world);
                saved_revisions.push(before_revision);
                assert!(world.validate_saved_session_reopen().is_err());
                drop((store, world, consumer));
                (store, world, consumer) = reopen_store(&path, &profile, &checkpoint);
                assert_eq!(world.checkpoint().unwrap(), baseline.checkpoint().unwrap());
                assert_eq!(wander::snapshot(&world), before_motion);
                assert_eq!(world.state().rng_counter, before_rng);
                assert_eq!(revision(&world), before_revision);
                assert!(mid_leg(&world));
                wander::assert_loaded_checkpoint_rejected(&world, &checkpoint);
                reopened += 1;
                waiting_for_arrival = true;
                arrived = false;
            }
        }
        if world.state().tick > 2200 && completed_continuations == thresholds.len() {
            break;
        }
    }
    assert!(world.state().tick > 2048);
    assert_eq!(reopened, thresholds.len());
    assert_eq!(completed_continuations, thresholds.len());
    assert!(saved_revisions.windows(2).all(|pair| pair[1] > pair[0]));
    assert_eq!(world.state().npcs[&2].id, original_owner);
    assert!(world.state().npcs[&2].health > 0);
    assert_eq!(world.checkpoint().unwrap(), baseline.checkpoint().unwrap());
    eprintln!(
        "bounded Wander reopen: ticks={} reopens={reopened} revisions={saved_revisions:?} max_retained_operations={max_recipe_operations}",
        world.state().tick
    );
}

/// Preserve the surrounding canonical struct field order, so mutation failures
/// exercise actual identity/RNG checks rather than JSON reordering rejection.
fn change_number(bytes: &[u8], marker: &str, change: impl FnOnce(u64) -> u64) -> Vec<u8> {
    let text = std::str::from_utf8(bytes).unwrap();
    let start = text.find(marker).expect("known recipe field") + marker.len();
    let count = text[start..].bytes().take_while(u8::is_ascii_digit).count();
    assert!(count > 0);
    let old = text[start..start + count].parse::<u64>().unwrap();
    let mut result = text.to_owned();
    result.replace_range(start..start + count, &change(old).to_string());
    result.into_bytes()
}

#[test]
fn failed_bounded_reopen_keeps_disk_and_live_owner_then_fresh_retry_matches() {
    let path = Directory::new();
    let (mut world, mut consumer) = wander::setup_with_schedule(Some(8), None);
    let (mut baseline, mut uninterrupted) = wander::setup_with_schedule(Some(8), None);
    first_mid_leg(&mut world, &mut consumer, &mut baseline, &mut uninterrupted);
    let profile = world.profile().clone();
    let checkpoint = world.checkpoint().unwrap();
    let bytes = recipe(&world, &consumer);
    let (store, _) = WanderStore::create(&path.0, &mut world, &bytes).unwrap();
    clear_journal(&mut baseline);
    let committed = consumer
        .providers
        .route
        .committed_candidate()
        .unwrap()
        .1
        .clone();
    let disk = std::fs::read(path.0.join("world.checkpoint")).unwrap();
    drop(store);
    assert!(reopen_bytes(&profile, checkpoint.clone(), &bytes, Duration::ZERO).is_err());
    let draw_marker = "\"draws\":[";
    let first_draws = std::str::from_utf8(&bytes)
        .unwrap()
        .rfind(draw_marker)
        .unwrap();
    let mut changed_draws = bytes[..first_draws].to_vec();
    changed_draws.extend(change_number(
        &bytes[first_draws..],
        "\"first_counter\":",
        |n| n + 1,
    ));
    let mutations = [
        change_number(&bytes, "\"version\":", |n| n + 1),
        change_number(&bytes, "\"operation_count\":", |n| n + 1),
        change_number(&bytes, "\"inventory\":[", |n| n ^ 1),
        change_number(&bytes, "\"configuration\":[", |n| n ^ 1),
        change_number(&bytes, "\"checkpoint\":[", |n| n ^ 1),
        change_number(&bytes, "\"capacity\":", |_| 512),
        changed_draws,
    ];
    for (index, changed) in mutations.iter().enumerate() {
        serde_json::from_slice::<serde_json::Value>(changed).unwrap();
        assert!(
            reopen_bytes(&profile, checkpoint.clone(), changed, REOPEN_SLICE).is_err(),
            "mutation {index} must not publish a restored owner"
        );
        assert_eq!(
            std::fs::read(path.0.join("world.checkpoint")).unwrap(),
            disk
        );
        assert_eq!(world.checkpoint().unwrap(), checkpoint);
        assert_eq!(
            consumer.providers.route.committed_candidate().unwrap().1,
            &committed
        );
    }
    let (_store, mut resumed, mut reopened) = reopen_store(&path, &profile, &checkpoint);
    assert_eq!(resumed.checkpoint().unwrap(), world.checkpoint().unwrap());
    let actual = wander::reject_late_and_retry(&mut resumed, &mut reopened);
    let expected = wander::step(&mut baseline, &mut uninterrupted);
    assert_eq!(actual, expected);
    assert_eq!(
        resumed.checkpoint().unwrap(),
        baseline.checkpoint().unwrap()
    );
    let retry_recipe = recipe(&resumed, &reopened);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&retry_recipe).unwrap()["operation_count"]
            .as_u64(),
        Some(revision(&resumed))
    );
    let mut arrived = false;
    let mut relaunched = false;
    for _ in 0..150 {
        let record = paired_step(
            &mut resumed,
            &mut reopened,
            &mut baseline,
            &mut uninterrupted,
        );
        arrived |= wander::snapshot(&resumed)
            .spline
            .as_ref()
            .is_some_and(|spline| spline.spline.finalized);
        if arrived && wander::launched(&record) {
            relaunched = true;
            break;
        }
    }
    assert!(arrived && relaunched);
    let next_launch_recipe = recipe(&resumed, &reopened);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&next_launch_recipe).unwrap()
            ["operation_count"]
            .as_u64(),
        Some(revision(&resumed))
    );
    assert_eq!(
        std::fs::read(path.0.join("world.checkpoint")).unwrap(),
        disk
    );
}

#[test]
fn current_nopath_wander_head_cannot_prepare_or_encode_reopen() {
    let (mut world, mut consumer) = wander::setup_with_schedule(Some(8), None);
    let record = wander::step(&mut world, &mut consumer);
    assert_eq!(world.state().tick, 1);
    assert!(!wander::launched(&record));
    let snapshot = wander::snapshot(&world);
    assert!(snapshot.spline.is_none());
    let cache = wander::wander(&snapshot).pathfinder_cache.as_ref().unwrap();
    assert_eq!(cache.path_type, 8);
    assert_eq!(cache.active_len, 0);
    let checkpoint = world.checkpoint().unwrap();
    assert!(NativeWorld::prepare_synthetic_wander_reopen(
        world.profile().clone(),
        checkpoint.clone()
    )
    .is_err());
    assert!(consumer
        .providers
        .route
        .encode_wander_reconstruction(&consumer.session, &world, &consumer.boundary)
        .is_err());
    assert_eq!(world.checkpoint().unwrap(), checkpoint);
}

#[test]
fn changed_loaded_corridor_revision_rejects_then_fresh_reopen_matches() {
    let (mut world, mut consumer) = wander::setup_with_schedule(Some(8), None);
    let (mut baseline, mut uninterrupted) = wander::setup_with_schedule(Some(8), None);
    first_mid_leg(&mut world, &mut consumer, &mut baseline, &mut uninterrupted);
    let profile = world.profile().clone();
    let checkpoint = world.checkpoint().unwrap();
    let bytes = recipe(&world, &consumer);
    let (fresh, mut changed) = wander::setup_with_schedule(Some(8), None);
    assert_eq!(fresh.profile(), &profile);
    drop(fresh);
    let pending =
        NativeWorld::prepare_synthetic_wander_reopen(profile.clone(), checkpoint.clone()).unwrap();
    let route = changed.providers.route;
    let result = changed.geometry.with_local_corridor(|corridor| {
        corridor.revision[0] ^= 1;
        route.reopen_wander(
            &changed.session,
            pending,
            &bytes,
            corridor,
            Instant::now() + REOPEN_SLICE,
        )
    });
    assert!(matches!(result, Err(NativeError::AuthorityBinding)));
    assert_eq!(world.checkpoint().unwrap(), checkpoint);
    assert_eq!(recipe(&world, &consumer), bytes);
    clear_journal(&mut baseline);
    let (mut resumed, mut reopened) =
        reopen_bytes(&profile, checkpoint, &bytes, REOPEN_SLICE).unwrap();
    paired_step(
        &mut resumed,
        &mut reopened,
        &mut baseline,
        &mut uninterrupted,
    );
}

#[test]
fn mismatched_store_checkpoint_drops_lease_then_exact_pair_reopens() {
    let path = Directory::new();
    let (mut world, mut consumer) = wander::setup_with_schedule(Some(8), None);
    let (mut baseline, mut uninterrupted) = wander::setup_with_schedule(Some(8), None);
    first_mid_leg(&mut world, &mut consumer, &mut baseline, &mut uninterrupted);
    let profile = world.profile().clone();
    let checkpoint = world.checkpoint().unwrap();
    let bytes = recipe(&world, &consumer);
    let (store, _) = WanderStore::create(&path.0, &mut world, &bytes).unwrap();
    let disk = std::fs::read(path.0.join("world.checkpoint")).unwrap();
    clear_journal(&mut baseline);
    let expected = paired_step(&mut world, &mut consumer, &mut baseline, &mut uninterrupted);
    let next_checkpoint = world.checkpoint().unwrap();
    assert_ne!(next_checkpoint, checkpoint);
    let pending =
        NativeWorld::prepare_synthetic_wander_reopen(profile.clone(), next_checkpoint.clone())
            .unwrap();
    let (fresh, mut changed) = wander::setup_with_schedule(Some(8), None);
    assert_eq!(fresh.profile(), &profile);
    drop(fresh);
    let route = changed.providers.route;
    let result = changed.geometry.with_local_corridor(|corridor| {
        route.reopen_wander_store(
            &changed.session,
            pending,
            store,
            corridor,
            Instant::now() + REOPEN_SLICE,
        )
    });
    assert!(matches!(result, Err(NativeError::InvalidCheckpoint)));
    assert_eq!(world.checkpoint().unwrap(), next_checkpoint);
    assert_eq!(
        std::fs::read(path.0.join("world.checkpoint")).unwrap(),
        disk
    );
    // Successful acquisition proves the failed consuming bridge released its lease.
    let (_store, mut resumed, mut reopened) = reopen_store(&path, &profile, &checkpoint);
    assert_eq!(wander::step(&mut resumed, &mut reopened), expected);
    assert_eq!(resumed.checkpoint().unwrap(), next_checkpoint);
}
