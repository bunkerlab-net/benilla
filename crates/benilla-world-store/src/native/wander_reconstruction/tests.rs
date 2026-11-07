// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_world_runtime::native::{
    synthetic_fixture::{self as fixture, SyntheticAuthority},
    MAX_RECORDS,
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
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "benilla-wander-snapshot-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
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
fn tick(world: &mut NativeWorld) {
    world
        .step(
            world.state().tick + 1,
            None,
            &mut SyntheticAuthority::default(),
        )
        .unwrap();
}
fn identity() -> Identity {
    Identity::of(&fixture::world().checkpoint().unwrap())
}
fn open(temp: &Temp) -> Result<(WanderStore, Recipe), Error> {
    WanderStore::open(temp.path(), identity())
}
fn read(temp: &Temp, name: &str) -> Vec<u8> {
    fs::read(temp.path().join(name)).unwrap()
}
fn envelope(temp: &Temp) -> Envelope {
    Envelope::decode(&read(temp, "world.checkpoint")).unwrap()
}

#[test]
fn nonzero_creation_captures_exact_current_world_and_acknowledges_after_publication() {
    let temp = Temp::new();
    let mut world = fixture::world();
    for _ in 0..3 {
        tick(&mut world);
    }
    let expected = world.checkpoint().unwrap();
    let (store, receipt) = WanderStore::create(temp.path(), &mut world, b"typed app data").unwrap();
    assert_eq!(receipt.generation(), 1);
    assert_eq!(receipt.boundary(), expected.identity());
    assert!(world.records().is_empty());
    assert_eq!(world.checkpoint().unwrap(), expected);
    assert_eq!(store.recipe().checkpoint(), &expected);
    assert_eq!(store.recipe().recipe_bytes(), b"typed app data");
    drop(store);
    let (store, recipe) = open(&temp).unwrap();
    assert_eq!(recipe.checkpoint(), &expected);
    assert_eq!(recipe.recipe_bytes(), b"typed app data");
    assert_eq!(store.identity(), &identity());
}

#[test]
fn bounded_current_snapshots_continue_past_full_history_limit_with_periodic_reopen() {
    let temp = Temp::new();
    let mut world = fixture::world();
    let (mut store, _) = WanderStore::create(temp.path(), &mut world, b"initial").unwrap();
    for i in 1..=MAX_RECORDS + 8 {
        tick(&mut world);
        if i % 64 == 0 || i == MAX_RECORDS + 8 {
            let payload = (i as u64).to_le_bytes();
            store.checkpoint(&mut world, &payload).unwrap();
            assert!(world.records().is_empty());
            let expected = world.checkpoint().unwrap();
            // One checkpoint and one bounded recipe, never a retained TickRecord array.
            assert_eq!(
                read(&temp, "world.checkpoint").len(),
                OVERHEAD + expected.encode().unwrap().len() + payload.len()
            );
            if i % 512 == 0 || i == MAX_RECORDS + 8 {
                let generation = store.generation();
                drop(store);
                let (opened, recipe) = open(&temp).unwrap();
                assert_eq!(opened.generation(), generation);
                assert_eq!(recipe.checkpoint(), &expected);
                assert_eq!(recipe.recipe_bytes(), payload);
                // Synthetic test-only restoration is explicit and outside the store.
                world = NativeWorld::restore_synthetic_component_test(
                    fixture::profile(),
                    recipe.checkpoint().clone(),
                )
                .unwrap();
                store = opened;
            }
        }
    }
    assert!(world.state().tick > MAX_RECORDS as u64);
}

#[test]
fn stale_preparation_and_receipt_cannot_discard_unsaved_suffix() {
    let temp = Temp::new();
    let mut world = fixture::world();
    let (mut store, _) = WanderStore::create(temp.path(), &mut world, b"a").unwrap();
    tick(&mut world);
    let first = store.prepare(&world, b"b").unwrap();
    let stale = store.prepare(&world, b"other").unwrap();
    let receipt = store.commit(first).unwrap();
    assert_eq!(world.records().len(), 1);
    assert!(matches!(
        store.commit(stale),
        Err(Error::Store(crate::Error::StalePrepared))
    ));
    tick(&mut world);
    let before = world.checkpoint().unwrap();
    assert!(receipt.acknowledge(&mut world).is_err());
    assert_eq!(world.records().len(), 2);
    assert_eq!(world.checkpoint().unwrap(), before);
    store.checkpoint(&mut world, b"c").unwrap();
    assert!(world.records().is_empty());
    drop(store);
    let (_, recipe) = open(&temp).unwrap();
    assert_eq!(recipe.checkpoint(), &before);
    assert_eq!(recipe.recipe_bytes(), b"c");
}

#[test]
fn empty_journal_requires_exact_saved_boundary_and_live_suffix_must_link() {
    let temp = Temp::new();
    let mut world = fixture::world();
    let (mut store, _) = WanderStore::create(temp.path(), &mut world, b"a").unwrap();
    let bytes = read(&temp, "world.checkpoint");
    let mut missing = world.clone();
    tick(&mut missing);
    missing
        .checkpoint_committed(&missing.checkpoint().unwrap())
        .unwrap();
    assert!(matches!(
        store.prepare(&missing, b"missing journal"),
        Err(Error::Store(crate::Error::DivergentHistory))
    ));
    let mut divergent =
        NativeWorld::synthetic_component_test(fixture::profile(), [8; 16], fixture::player(), 4321)
            .unwrap();
    assert!(matches!(
        store.prepare(&divergent, b"same tick, different state"),
        Err(Error::Store(crate::Error::DivergentHistory))
    ));
    tick(&mut divergent);
    assert!(matches!(
        store.prepare(&divergent, b"wrong history"),
        Err(Error::Store(crate::Error::DivergentHistory))
    ));
    assert_eq!(read(&temp, "world.checkpoint"), bytes);
    store
        .checkpoint(&mut world, b"same exact boundary")
        .unwrap();
    assert_eq!(store.generation(), 2);
    tick(&mut world);
    store.checkpoint(&mut world, b"advanced").unwrap();
    assert!(matches!(
        store.prepare(&fixture::world(), b"rewind"),
        Err(Error::Store(crate::Error::DivergentHistory))
    ));
}

#[test]
fn wrong_identity_never_changes_files_or_clears_the_world() {
    let temp = Temp::new();
    let mut world = fixture::world();
    let (mut store, _) = WanderStore::create(temp.path(), &mut world, b"a").unwrap();
    let before = read(&temp, "world.checkpoint");
    let mut wrong_world =
        NativeWorld::synthetic_component_test(fixture::profile(), [9; 16], fixture::player(), 1234)
            .unwrap();
    tick(&mut wrong_world);
    assert!(matches!(
        store.checkpoint(&mut wrong_world, b"wrong world"),
        Err(Error::Store(crate::Error::Incompatible))
    ));
    assert_eq!(wrong_world.records().len(), 1);
    assert_eq!(read(&temp, "world.checkpoint"), before);
    drop(store);
    for mutation in 0..4 {
        let mut wrong = identity();
        match mutation {
            0 => wrong.world = [9; 16],
            1 => wrong.player.owner = [9; 16],
            2 => wrong.player.character = [9; 16],
            3 => wrong.profile = [9; 32],
            _ => unreachable!(),
        }
        assert!(matches!(
            WanderStore::open(temp.path(), wrong),
            Err(Error::Store(crate::Error::Incompatible))
        ));
    }
    assert_eq!(read(&temp, "world.checkpoint"), before);
}

#[test]
fn combined_payload_bound_is_exact_and_overflow_never_reaches_publication() {
    let temp = Temp::new();
    let mut world = fixture::world();
    let size = world.checkpoint().unwrap().encode().unwrap().len();
    let payload = vec![0xa5; MAX_PAYLOAD_BYTES - size];
    let (mut store, _) = WanderStore::create(temp.path(), &mut world, &payload).unwrap();
    assert_eq!(
        read(&temp, "world.checkpoint").len(),
        MAX_PAYLOAD_BYTES + OVERHEAD
    );
    assert_eq!(store.recipe().recipe_bytes(), payload);
    let old = store.recipe().clone();
    let bytes = read(&temp, "world.checkpoint");
    assert!(matches!(
        store.checkpoint(&mut world, &vec![0xa5; payload.len() + 1]),
        Err(Error::Runtime(NativeError::TooLarge))
    ));
    assert_eq!(store.recipe(), &old);
    assert_eq!(read(&temp, "world.checkpoint"), bytes);
    assert!(!temp.path().join("world.previous").exists());
    drop(store);
    assert_eq!(open(&temp).unwrap().1, old);

    let other = Temp::new();
    tick(&mut world);
    assert!(matches!(
        WanderStore::create(other.path(), &mut world, &vec![0; MAX_PAYLOAD_BYTES + 1]),
        Err(Error::Runtime(NativeError::TooLarge))
    ));
    assert!(!other.path().exists());
    assert_eq!(world.records().len(), 1);
}

#[test]
fn locks_are_exclusive_and_explicit_create_never_replaces_saved_data() {
    let temp = Temp::new();
    let mut world = fixture::world();
    let (store, _) = WanderStore::create(temp.path(), &mut world, b"a").unwrap();
    assert!(matches!(
        open(&temp),
        Err(Error::Store(crate::Error::Locked))
    ));
    let bytes = read(&temp, "world.checkpoint");
    drop(store);
    tick(&mut world);
    assert!(matches!(
        WanderStore::create(temp.path(), &mut world, b"replacement"),
        Err(Error::Store(crate::Error::AlreadyExists))
    ));
    assert_eq!(world.records().len(), 1);
    assert_eq!(read(&temp, "world.checkpoint"), bytes);
}

#[test]
fn corruption_truncation_and_invalid_previous_never_fall_back_or_rewrite() {
    for target in ["world.checkpoint", "world.previous"] {
        let temp = Temp::new();
        let mut world = fixture::world();
        let (mut store, _) = WanderStore::create(temp.path(), &mut world, b"a").unwrap();
        tick(&mut world);
        store.checkpoint(&mut world, b"b").unwrap();
        drop(store);
        let original = read(&temp, target);
        for length in [0, 7, 8, 11, 59, 60, OVERHEAD, original.len() - 1] {
            let truncated = &original[..length];
            fs::write(temp.path().join(target), truncated).unwrap();
            assert!(open(&temp).is_err(), "{target} truncated to {length}");
            assert_eq!(read(&temp, target), truncated);
        }
        for index in [0, 12, HEADER, original.len() - 33, original.len() - 1] {
            let mut changed = original.clone();
            changed[index] ^= 1;
            fs::write(temp.path().join(target), &changed).unwrap();
            assert!(open(&temp).is_err(), "{target} mutation at {index}");
            assert_eq!(read(&temp, target), changed);
        }
    }
}

#[test]
fn generation_parent_reordering_missing_previous_and_checkpoint_rewind_are_rejected() {
    for mutation in 0..8 {
        let temp = Temp::new();
        let mut world = fixture::world();
        let (mut store, _) = WanderStore::create(temp.path(), &mut world, b"a").unwrap();
        let genesis = world.checkpoint().unwrap();
        tick(&mut world);
        store.checkpoint(&mut world, b"b").unwrap();
        tick(&mut world);
        store.checkpoint(&mut world, b"c").unwrap();
        drop(store);
        let mut current = envelope(&temp);
        let previous = read(&temp, "world.previous");
        match mutation {
            0 => current.generation += 1,
            1 => current.parent = [99; 32],
            2 => current.recipe.checkpoint = genesis,
            3 => {
                let mut alternate = NativeWorld::synthetic_component_test(
                    fixture::profile(),
                    [8; 16],
                    fixture::player(),
                    4321,
                )
                .unwrap();
                tick(&mut alternate);
                current.recipe.checkpoint = alternate.checkpoint().unwrap();
            }
            4 => {
                fs::write(
                    temp.path().join("world.previous"),
                    current.encode().unwrap(),
                )
                .unwrap();
                fs::write(temp.path().join("world.checkpoint"), &previous).unwrap();
            }
            5 => fs::remove_file(temp.path().join("world.previous")).unwrap(),
            6 => {
                let wrong = NativeWorld::synthetic_component_test(
                    fixture::profile(),
                    [9; 16],
                    fixture::player(),
                    1234,
                )
                .unwrap();
                current.recipe.checkpoint = wrong.checkpoint().unwrap();
            }
            7 => {
                let mut prior = Envelope::decode(&previous).unwrap();
                prior.generation += 1;
                let changed = prior.encode().unwrap();
                current.parent = hash(&changed);
                fs::write(temp.path().join("world.previous"), changed).unwrap();
            }
            _ => unreachable!(),
        }
        if mutation != 4 {
            fs::write(
                temp.path().join("world.checkpoint"),
                current.encode().unwrap(),
            )
            .unwrap();
        }
        let before = read(&temp, "world.checkpoint");
        assert!(open(&temp).is_err(), "mutation {mutation}");
        assert_eq!(read(&temp, "world.checkpoint"), before);
    }
}

#[test]
fn opaque_payload_is_data_only_and_formats_and_versions_are_not_interchangeable() {
    let temp = Temp::new();
    let mut world = fixture::world();
    let (store, _) = WanderStore::create(temp.path(), &mut world, b"untrusted bytes").unwrap();
    drop(store);
    assert!(
        super::super::NativeStore::open(temp.path(), identity(), |_| {
            panic!("Wander bytes cannot reach generic restoration")
        })
        .is_err()
    );
    assert!(
        super::super::reconstruction::RecipeStore::open(temp.path(), identity(), |_| {
            panic!("Wander bytes cannot reach historical reconstruction")
        })
        .is_err()
    );
    // A re-signed opaque payload is still inert and must be rejected by the app's typed
    // decoder/reconstruction if invalid. Storage checksums are not admission authority.
    let mut changed = envelope(&temp);
    changed.recipe.recipe_bytes = b"not a valid native capability".to_vec();
    fs::write(
        temp.path().join("world.checkpoint"),
        changed.encode().unwrap(),
    )
    .unwrap();
    let (store, recipe) = open(&temp).unwrap();
    assert_eq!(recipe.recipe_bytes(), b"not a valid native capability");
    drop(store);
    let mut bytes = read(&temp, "world.checkpoint");
    bytes[8..12].copy_from_slice(&2u32.to_le_bytes());
    let end = bytes.len() - 32;
    let digest = checksum(&bytes[..end]);
    bytes[end..].copy_from_slice(&digest);
    fs::write(temp.path().join("world.checkpoint"), bytes).unwrap();
    assert!(matches!(
        open(&temp),
        Err(Error::Store(crate::Error::UnsupportedVersion))
    ));

    let other = Temp::new();
    let (store, _) = super::super::NativeStore::create(other.path(), &fixture::world()).unwrap();
    drop(store);
    assert!(open(&other).is_err());
}

#[test]
fn resigned_invalid_origin_and_length_framing_never_decode() {
    let world = fixture::world();
    let envelope = Envelope {
        generation: 1,
        parent: [0; 32],
        recipe: Recipe {
            checkpoint: world.checkpoint().unwrap(),
            recipe_bytes: b"recipe".to_vec(),
        },
    };
    let original = envelope.encode().unwrap();
    for mutation in 0..7 {
        let mut bytes = original.clone();
        match mutation {
            0 => bytes[12..20].copy_from_slice(&0u64.to_le_bytes()),
            1 => bytes[20] = 1,
            2 => bytes[12..20].copy_from_slice(&2u64.to_le_bytes()),
            3 => bytes[52..56].copy_from_slice(&u32::MAX.to_le_bytes()),
            4 => bytes[56..60].copy_from_slice(&u32::MAX.to_le_bytes()),
            5 => bytes[52..56].copy_from_slice(&0u32.to_le_bytes()),
            6 => bytes.push(0),
            _ => unreachable!(),
        }
        let end = bytes.len() - 32;
        let digest = checksum(&bytes[..end]);
        bytes[end..].copy_from_slice(&digest);
        assert!(Envelope::decode(&bytes).is_err(), "mutation {mutation}");
    }
}

#[test]
fn all_publication_faults_preserve_live_journal_and_reopen_only_whole_old_or_new_pair() {
    use crate::storage::SaveFault;
    for fault in [
        SaveFault::PartialWrite,
        SaveFault::BeforeSync,
        SaveFault::BeforeRename,
        SaveFault::BeforePreviousRename,
        SaveFault::AfterPreviousRename,
        SaveFault::AfterRename,
        SaveFault::BeforeVerification,
    ] {
        let temp = Temp::new();
        let mut world = fixture::world();
        let (mut store, _) = WanderStore::create(temp.path(), &mut world, b"a").unwrap();
        tick(&mut world);
        store.checkpoint(&mut world, b"old pair").unwrap();
        let old = store.recipe().clone();
        let generation = store.generation();
        tick(&mut world);
        let expected = world.checkpoint().unwrap();
        let records = world.records().to_vec();
        store.storage.inject_fault(fault);
        assert!(store.checkpoint(&mut world, b"new pair").is_err());
        assert_eq!(store.recipe(), &old);
        assert_eq!(store.generation(), generation);
        assert_eq!(world.checkpoint().unwrap(), expected);
        assert_eq!(world.records(), records);
        let published = matches!(
            fault,
            SaveFault::AfterRename | SaveFault::BeforeVerification
        );
        let on_disk = envelope(&temp).recipe;
        if published {
            assert_eq!(on_disk.checkpoint(), &expected);
            assert_eq!(on_disk.recipe_bytes(), b"new pair");
        } else {
            assert_eq!(on_disk, old);
        }
        assert!(!temp.path().join(".world-checkpoint.pending").exists());
        let uncertain = published || fault == SaveFault::AfterPreviousRename;
        assert_eq!(store.requires_reopen(), uncertain);
        if uncertain {
            assert!(matches!(
                store.prepare(&world, b"retry"),
                Err(Error::Store(crate::Error::CommitUncertain))
            ));
        } else {
            store.storage.clear_fault();
            store.checkpoint(&mut world, b"new pair").unwrap();
            assert!(world.records().is_empty());
        }
        drop(store);
        let (mut store, recipe) = open(&temp).unwrap();
        if fault == SaveFault::AfterPreviousRename {
            assert_eq!(recipe, old);
            assert_eq!(store.generation(), generation);
            store.checkpoint(&mut world, b"new pair").unwrap();
        } else {
            assert_eq!(recipe.checkpoint(), &expected);
            assert_eq!(recipe.recipe_bytes(), b"new pair");
            assert_eq!(store.generation(), generation + 1);
        }
    }
}

#[test]
fn every_prepublication_fault_can_close_and_reopen_old_pair_before_retry() {
    use crate::storage::SaveFault;
    for fault in [
        SaveFault::PartialWrite,
        SaveFault::BeforeSync,
        SaveFault::BeforeRename,
        SaveFault::BeforePreviousRename,
        SaveFault::AfterPreviousRename,
    ] {
        let temp = Temp::new();
        let mut world = fixture::world();
        let (mut store, _) = WanderStore::create(temp.path(), &mut world, b"a").unwrap();
        tick(&mut world);
        store.checkpoint(&mut world, b"old").unwrap();
        let old = store.recipe().clone();
        tick(&mut world);
        let expected = world.checkpoint().unwrap();
        store.storage.inject_fault(fault);
        assert!(store.checkpoint(&mut world, b"new").is_err());
        drop(store);
        let (mut store, recipe) = open(&temp).unwrap();
        assert_eq!(recipe, old);
        assert_eq!(world.records().len(), 1);
        store.checkpoint(&mut world, b"new").unwrap();
        assert!(world.records().is_empty());
        drop(store);
        let (_, recipe) = open(&temp).unwrap();
        assert_eq!(recipe.checkpoint(), &expected);
        assert_eq!(recipe.recipe_bytes(), b"new");
    }
}

#[test]
fn external_file_modification_latches_without_acknowledgement() {
    for target in ["world.checkpoint", "world.previous"] {
        let temp = Temp::new();
        let mut world = fixture::world();
        let (mut store, _) = WanderStore::create(temp.path(), &mut world, b"a").unwrap();
        tick(&mut world);
        store.checkpoint(&mut world, b"b").unwrap();
        let old = store.recipe().clone();
        tick(&mut world);
        let records = world.records().to_vec();
        fs::write(temp.path().join(target), b"external mutation").unwrap();
        assert!(matches!(
            store.checkpoint(&mut world, b"c"),
            Err(Error::Store(crate::Error::ConcurrentModification))
        ));
        assert!(store.requires_reopen());
        assert_eq!(store.recipe(), &old);
        assert_eq!(world.records(), records);
    }
}

#[cfg(unix)]
#[test]
fn replaced_lock_latches_without_publication_or_journal_loss() {
    use std::os::unix::fs::PermissionsExt;

    let temp = Temp::new();
    let mut world = fixture::world();
    let (mut store, _) = WanderStore::create(temp.path(), &mut world, b"a").unwrap();
    let before = read(&temp, "world.checkpoint");
    tick(&mut world);
    fs::rename(temp.path().join("world.lock"), temp.path().join("old.lock")).unwrap();
    fs::write(temp.path().join("world.lock"), b"").unwrap();
    fs::set_permissions(
        temp.path().join("world.lock"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    assert!(matches!(
        store.checkpoint(&mut world, b"b"),
        Err(Error::Store(crate::Error::ConcurrentModification))
    ));
    assert!(store.requires_reopen());
    assert_eq!(world.records().len(), 1);
    assert_eq!(read(&temp, "world.checkpoint"), before);
    assert!(!temp.path().join("world.previous").exists());
}
