// SPDX-License-Identifier: GPL-3.0-or-later
//! Real exclusive-store publication with authored source-progression rows through sixty.
use super::*;
use benilla_world_runtime::native::{
    self as n, synthetic_fixture as fixture, synthetic_quest_fixture as quests,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "benilla-level-sixty-store-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn path(&self) -> PathBuf {
        self.0.join("world")
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn profile(long_cast: bool, rate: u32) -> n::Profile {
    let (mut profile, input, mut data) = quests::setup_with_levels(60, 60, true, rate);
    if long_cast {
        profile.npcs[0].max_health = 100_000;
        for ability in [&mut profile.abilities[0], &mut data.abilities[0].ability] {
            let n::AbilityAdmission::Available(spec) = &mut ability.admission else {
                unreachable!()
            };
            spec.cast_ms = 20_000;
        }
        let time = data.abilities[0].cast_time.as_mut().unwrap();
        time.base_ms = 20_000;
        time.minimum_ms = 20_000;
        profile
            .initialize_player(&input, fixture::float(100.0), 1)
            .unwrap();
    }
    profile.initialize_quest_progression(&input, data).unwrap();
    profile.admin = Some(n::AdminPolicy {
        world: [1; 16],
        world_owner: fixture::player().owner,
        allowlist: Default::default(),
        revision: 1,
    });
    profile
}

fn apply(world: &mut NativeWorld, operation: n::AdminOperation) -> n::AdminRequest {
    let mut key = [0; 16];
    key[..8].copy_from_slice(&(world.state().next_sequence + 1).to_le_bytes());
    let request = n::AdminRequest {
        key,
        world: world.state().world,
        player: world.state().player,
        policy_revision: 1,
        operation,
    };
    let input = quests::command(world, n::Command::Admin(request.clone()));
    world
        .step(
            world.state().tick + 1,
            Some(input),
            &mut quests::QuestAuthority::default(),
        )
        .unwrap();
    request
}

fn to_level(world: &mut NativeWorld, level: u8) {
    while world.player_level() < level {
        let before = world.player_level();
        apply(world, n::AdminOperation::GainOneLevel);
        assert_eq!(world.player_level(), before + 1);
    }
}

fn reopened(path: &Path, identity: Identity, profile: n::Profile) -> (NativeStore, NativeWorld) {
    NativeStore::open(path, identity, |checkpoint| {
        NativeWorld::restore_synthetic_component_test(profile.clone(), checkpoint)
    })
    .unwrap()
}

#[test]
fn level_nine_ten_and_fifty_nine_sixty_publish_once_and_reopen_exactly() {
    let temp = Temp::new();
    let profile = profile(false, 1.0f32.to_bits());
    let mut world =
        NativeWorld::synthetic_component_test(profile.clone(), [1; 16], fixture::player(), 42)
            .unwrap();
    let identity = Identity::of(&world.checkpoint().unwrap());
    let (mut store, receipt) = NativeStore::create(temp.path(), &world).unwrap();
    receipt.acknowledge(&mut world).unwrap();
    quests::kill_target(&mut world, &mut quests::QuestAuthority::default());
    assert_eq!(
        world
            .state()
            .quest_progression
            .as_ref()
            .unwrap()
            .player
            .view()
            .xp,
        10
    );
    apply(&mut world, n::AdminOperation::SetXpGain { enabled: false });

    for (before, after, points) in [(9, 10, 1), (59, 60, 51)] {
        to_level(&mut world, before);
        store.checkpoint(&mut world).unwrap();
        let before_generation = store.generation();
        let request = apply(&mut world, n::AdminOperation::GainOneLevel);
        let checkpoint = world.checkpoint().unwrap();
        let computed = world.admin_receipt(&request.key).unwrap().clone();
        assert_eq!(world.player_level(), after);
        assert!(store.committed_admin_receipt(&request).is_none());
        store
            .storage
            .inject_fault(super::super::storage::SaveFault::BeforeRename);
        assert!(store.checkpoint(&mut world).is_err());
        assert!(!store.requires_reopen());
        assert_eq!(store.generation(), before_generation);
        assert!(store.committed_admin_receipt(&request).is_none());
        assert_eq!(
            store
                .committed_admin_snapshot(fixture::player().owner)
                .unwrap()
                .level,
            u32::from(before)
        );
        assert_eq!(world.records().len(), 1);
        store.storage.clear_fault();
        store.checkpoint(&mut world).unwrap();
        assert_eq!(store.generation(), before_generation + 1);
        assert!(world.records().is_empty());
        assert_eq!(
            store.committed_admin_receipt(&request).unwrap().receipt(),
            &computed
        );
        drop(store);
        (store, world) = reopened(&temp.path(), identity.clone(), profile.clone());
        assert_eq!(world.checkpoint().unwrap(), checkpoint);
        let progression = world.state().quest_progression.as_ref().unwrap();
        assert_eq!(progression.player.view().free_talent_points, points);
        assert_eq!(progression.player.view().xp, 10);
        assert!(!progression.xp_enabled);
        let calls = progression.derived_calls.clone();
        let input = quests::command(&world, n::Command::Admin(request.clone()));
        world
            .step(
                world.state().tick + 1,
                Some(input),
                &mut quests::QuestAuthority::default(),
            )
            .unwrap();
        assert_eq!(world.player_level(), after);
        assert_eq!(
            world
                .state()
                .quest_progression
                .as_ref()
                .unwrap()
                .derived_calls,
            calls
        );
        assert!(!world
            .last_tick()
            .unwrap()
            .events
            .iter()
            .any(|event| matches!(event, n::Event::QuestProgression(_))));
        store.checkpoint(&mut world).unwrap();
        assert_eq!(
            store.committed_admin_receipt(&request).unwrap().receipt(),
            &computed
        );
    }
    drop(store);
    let mut wrong_identity = identity.clone();
    wrong_identity.player.character[0] ^= 1;
    assert!(
        NativeStore::open(temp.path(), wrong_identity, |checkpoint| {
            NativeWorld::restore_synthetic_component_test(profile.clone(), checkpoint)
        })
        .is_err()
    );
    let wrong_profile = self::profile(false, 1.5f32.to_bits());
    assert!(NativeStore::open(temp.path(), identity, |checkpoint| {
        NativeWorld::restore_synthetic_component_test(wrong_profile.clone(), checkpoint)
    })
    .is_err());
}

/// Recompute only the public envelope hashes, preserving typed state field ordering.
/// A valid outer checksum must not authenticate a forged historical prefix.
fn resign(mut value: serde_json::Value) -> Checkpoint {
    let typed: Checkpoint = serde_json::from_value(value.clone()).unwrap();
    let encoded = String::from_utf8(typed.encode().unwrap()).unwrap();
    let start = encoded.find("\"state\":").unwrap() + "\"state\":".len();
    let end = encoded.rfind(",\"digest\":").unwrap();
    let state = &encoded[start..end];
    let hash = |domain: String, bytes: &[u8]| -> Digest {
        let mut hasher = Sha256::new();
        hasher.update(domain.as_bytes());
        hasher.update(bytes);
        hasher.finalize().into()
    };
    let digest = hash(
        format!("benilla-native-state-v{}\0", n::NATIVE_FORMAT_VERSION),
        state.as_bytes(),
    );
    value["digest"] = serde_json::to_value(digest).unwrap();
    let body = format!("[{state},{},{}]", value["digest"], value["history_tip"]);
    value["checksum"] = serde_json::to_value(hash(
        format!("benilla-native-checkpoint-v{}\0", n::NATIVE_FORMAT_VERSION),
        body.as_bytes(),
    ))
    .unwrap();
    let typed: Checkpoint = serde_json::from_value(value).unwrap();
    Checkpoint::decode(&typed.encode().unwrap()).expect("outer checksum is valid")
}

#[test]
fn preparing_cast_survives_sixty_level_owner_history_and_rejects_resigned_prefix_tampering() {
    let temp = Temp::new();
    let profile = profile(true, 1.0f32.to_bits());
    let mut world =
        NativeWorld::synthetic_component_test(profile.clone(), [1; 16], fixture::player(), 42)
            .unwrap();
    let identity = Identity::of(&world.checkpoint().unwrap());
    let (mut store, _) = NativeStore::create(temp.path(), &world).unwrap();
    let input = quests::command(
        &world,
        n::Command::Cast {
            ability: 60001,
            target: world.state().npcs[&2].id,
        },
    );
    world
        .step(1, Some(input), &mut quests::QuestAuthority::default())
        .unwrap();
    let old_binding = world.state().casts[&1].binding.clone();
    assert_eq!(old_binding.level, 1);
    to_level(&mut world, 60);
    assert_eq!(world.state().casts[&1].binding, old_binding);
    assert!(world.state().casts[&1].release_binding.is_none());
    store.checkpoint(&mut world).unwrap();
    let checkpoint = world.checkpoint().unwrap();
    drop(store);
    let (_, restored) = reopened(&temp.path(), identity, profile.clone());
    assert_eq!(restored.checkpoint().unwrap(), checkpoint);
    assert_eq!(restored.state().casts[&1].binding, old_binding);
    let value = serde_json::to_value(checkpoint).unwrap();
    for prefix in ["training_prefix", "derived_prefix"] {
        for field in ["count", "digest"] {
            let mut forged = value.clone();
            let prefix = &mut forged["state"]["casts"]["1"]["binding"]["progression"][prefix];
            if field == "count" {
                prefix["count"] = serde_json::json!(prefix["count"].as_u64().unwrap() + 1);
            } else {
                prefix["digest"][0] = serde_json::json!(prefix["digest"][0].as_u64().unwrap() ^ 1);
            }
            let forged = resign(forged);
            assert!(
                NativeWorld::restore_synthetic_component_test(profile.clone(), forged).is_err()
            );
        }
    }
}
