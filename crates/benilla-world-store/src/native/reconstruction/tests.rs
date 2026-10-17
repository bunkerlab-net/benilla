// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_world_runtime::native::{
    synthetic_fixture::{self as fixture, SyntheticAuthority},
    Event, Facts, OwnedInput, Rejection,
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
            "benilla-native-recipe-{}-{}",
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
fn open(path: &Path) -> Result<(RecipeStore, Recipe), Error> {
    RecipeStore::open(path, identity(), |anchor| {
        NativeWorld::restore_synthetic_component_test(fixture::profile(), anchor)
    })
}
fn reexecute(recipe: &Recipe) -> NativeWorld {
    let mut world = fixture::world();
    assert_eq!(world.checkpoint().unwrap(), *recipe.anchor());
    let mut authority = SyntheticAuthority::default();
    for record in recipe.records() {
        let inputs = record
            .inputs
            .iter()
            .cloned()
            .map(|input| OwnedInput {
                authenticated_owner: fixture::player().owner,
                input,
            })
            .collect();
        assert_eq!(
            world
                .step_batch(record.tick, inputs, &mut authority)
                .unwrap(),
            record
        );
    }
    assert_eq!(world.checkpoint().unwrap(), *recipe.expected_checkpoint());
    world
}
fn read_envelope(temp: &Temp) -> Envelope {
    Envelope::decode(&fs::read(temp.path().join("world.checkpoint")).unwrap()).unwrap()
}
fn write_envelope(temp: &Temp, envelope: &Envelope) {
    fs::write(
        temp.path().join("world.checkpoint"),
        envelope.encode().unwrap(),
    )
    .unwrap();
}

#[test]
fn two_commits_keep_genesis_and_full_records_then_continue_after_fresh_execution() {
    let temp = Temp::new();
    let mut world = fixture::world();
    let anchor = world.checkpoint().unwrap();
    let (mut store, receipt) = RecipeStore::create(temp.path(), &world).unwrap();
    receipt.acknowledge(&mut world).unwrap();
    assert_eq!(store.recipe().anchor(), &anchor);
    let input = fixture::cast(&world, 60001);
    world
        .step(1, Some(input), &mut SyntheticAuthority::default())
        .unwrap();
    let first = world.records()[0].clone();
    store.checkpoint(&mut world).unwrap();
    for _ in 0..4 {
        tick(&mut world);
    }
    let last = world.records().to_vec();
    let expected = world.checkpoint().unwrap();
    store.checkpoint(&mut world).unwrap();
    assert!(world.records().is_empty());
    assert_eq!(store.recipe().anchor(), &anchor);
    assert_eq!(store.recipe().records(), [vec![first], last].concat());
    let saved_recipe = store.recipe().clone();
    drop(store);

    let mut restores = 0;
    let (mut store, recipe) = RecipeStore::open(temp.path(), identity(), |checkpoint| {
        restores += 1;
        assert_eq!(checkpoint, anchor);
        NativeWorld::restore_synthetic_component_test(fixture::profile(), checkpoint)
    })
    .unwrap();
    assert_eq!(restores, 2);
    assert_eq!(recipe, saved_recipe);
    assert_eq!(recipe.expected_checkpoint(), &expected);
    assert_eq!(store.recipe(), &recipe);
    let mut world = reexecute(&recipe);
    world
        .checkpoint_committed(recipe.expected_checkpoint())
        .unwrap();
    tick(&mut world);
    store.checkpoint(&mut world).unwrap();
    assert_eq!(store.recipe().records().len(), 6);
    let expected = store.recipe().clone();
    drop(store);
    let (_, recipe) = open(&temp.path()).unwrap();
    assert_eq!(recipe, expected);
    reexecute(&recipe);
}

#[test]
fn stale_preparation_and_receipt_cannot_discard_unacknowledged_suffix() {
    let temp = Temp::new();
    let mut world = fixture::world();
    let (mut store, _) = RecipeStore::create(temp.path(), &world).unwrap();
    tick(&mut world);
    let prepared = store.prepare(&world).unwrap();
    let stale = store.prepare(&world).unwrap();
    let receipt = store.commit(prepared).unwrap();
    assert!(matches!(
        store.commit(stale),
        Err(Error::Store(crate::Error::StalePrepared))
    ));
    tick(&mut world);
    assert!(receipt.acknowledge(&mut world).is_err());
    assert_eq!(world.records().len(), 2);
    store.checkpoint(&mut world).unwrap();
    assert_eq!(store.recipe().records().len(), 2);
    let expected = world.checkpoint().unwrap();
    drop(store);
    assert_eq!(
        open(&temp.path()).unwrap().1.expected_checkpoint(),
        &expected
    );
}

#[test]
fn wrong_identity_and_wrong_anchor_constructor_fail_without_replacing_files() {
    let temp = Temp::new();
    let (store, _) = RecipeStore::create(temp.path(), &fixture::world()).unwrap();
    drop(store);
    let before = fs::read(temp.path().join("world.checkpoint")).unwrap();
    let mut wrong = identity();
    wrong.player.owner = [9; 16];
    assert!(matches!(
        RecipeStore::open(temp.path(), wrong, |_| panic!(
            "identity checked before restore"
        )),
        Err(Error::Store(crate::Error::Incompatible))
    ));
    let mut wrong_world = fixture::world();
    tick(&mut wrong_world);
    assert!(matches!(
        RecipeStore::open(temp.path(), identity(), |_| Ok(wrong_world.clone())),
        Err(Error::Store(crate::Error::Incompatible))
    ));
    assert_eq!(
        fs::read(temp.path().join("world.checkpoint")).unwrap(),
        before
    );
}

#[test]
fn creation_is_genesis_only_nonreplacing_and_lock_is_exclusive() {
    let temp = Temp::new();
    let mut world = fixture::world();
    tick(&mut world);
    assert!(RecipeStore::create(temp.path(), &world).is_err());
    assert!(!temp.path().exists());
    let (store, _) = RecipeStore::create(temp.path(), &fixture::world()).unwrap();
    assert!(matches!(
        open(&temp.path()),
        Err(Error::Store(crate::Error::Locked))
    ));
    drop(store);
    assert!(matches!(
        RecipeStore::create(temp.path(), &fixture::world()),
        Err(Error::Store(crate::Error::AlreadyExists))
    ));
}

#[test]
fn resigned_omitted_reordered_and_changed_records_fail_exact_replay() {
    for mutation in 0..6 {
        let temp = Temp::new();
        let mut world = fixture::world();
        let (mut store, _) = RecipeStore::create(temp.path(), &world).unwrap();
        let input = fixture::cast(&world, 60001);
        world
            .step(1, Some(input), &mut SyntheticAuthority::default())
            .unwrap();
        tick(&mut world);
        tick(&mut world);
        store.checkpoint(&mut world).unwrap();
        drop(store);
        let mut envelope = read_envelope(&temp);
        match mutation {
            0 => {
                envelope.records.remove(0);
            }
            1 => envelope.records.swap(0, 1),
            2 => envelope.records[0].after = [99; 32],
            3 => envelope.records[0].replies[0].request.binding.geometry = [99; 32],
            4 => envelope.records[0].replies[0].facts = Facts::PlayerMovement { accepted: false },
            5 => envelope.records[0].inputs[0].sequence += 1,
            _ => unreachable!(),
        }
        write_envelope(&temp, &envelope);
        assert!(open(&temp.path()).is_err(), "mutation {mutation}");
    }
}

#[test]
fn valid_current_history_cannot_rewind_or_replace_previous_prefix_or_anchor() {
    for mutation in 0..3 {
        let temp = Temp::new();
        let mut world = fixture::world();
        let (mut store, _) = RecipeStore::create(temp.path(), &world).unwrap();
        let input = fixture::cast(&world, 60001);
        world
            .step(1, Some(input), &mut SyntheticAuthority::default())
            .unwrap();
        store.checkpoint(&mut world).unwrap();
        tick(&mut world);
        store.checkpoint(&mut world).unwrap();
        drop(store);
        let mut envelope = read_envelope(&temp);
        match mutation {
            0 => envelope.records.clear(),
            1 => {
                let mut alternate = fixture::world();
                tick(&mut alternate);
                tick(&mut alternate);
                envelope.records = alternate.records().to_vec();
            }
            2 => {
                let mut alternate = NativeWorld::synthetic_component_test(
                    fixture::profile(),
                    [8; 16],
                    fixture::player(),
                    4321,
                )
                .unwrap();
                envelope.anchor = alternate.checkpoint().unwrap();
                tick(&mut alternate);
                tick(&mut alternate);
                envelope.records = alternate.records().to_vec();
            }
            _ => unreachable!(),
        }
        write_envelope(&temp, &envelope);
        assert!(matches!(
            open(&temp.path()),
            Err(Error::Store(crate::Error::DivergentHistory))
        ));
    }
}

#[test]
fn current_corruption_never_falls_back_and_invalid_previous_never_opens() {
    for target in ["world.checkpoint", "world.previous"] {
        let temp = Temp::new();
        let mut world = fixture::world();
        let (mut store, _) = RecipeStore::create(temp.path(), &world).unwrap();
        tick(&mut world);
        store.checkpoint(&mut world).unwrap();
        drop(store);
        fs::write(temp.path().join(target), b"broken").unwrap();
        assert!(open(&temp.path()).is_err());
    }
}

#[test]
fn retained_record_cap_does_not_reset_at_commit_and_overflow_preserves_disk_and_journal() {
    let temp = Temp::new();
    let mut world = fixture::world();
    let (mut store, _) = RecipeStore::create(temp.path(), &world).unwrap();
    for _ in 0..2 {
        for _ in 0..MAX_RECORDS / 2 {
            tick(&mut world);
        }
        store.checkpoint(&mut world).unwrap();
    }
    assert_eq!(store.recipe().records().len(), MAX_RECORDS);
    let current = fs::read(temp.path().join("world.checkpoint")).unwrap();
    let previous = fs::read(temp.path().join("world.previous")).unwrap();
    let recipe = store.recipe().clone();
    tick(&mut world);
    assert!(matches!(
        store.checkpoint(&mut world),
        Err(Error::Runtime(NativeError::TooLarge))
    ));
    assert_eq!(store.recipe(), &recipe);
    assert_eq!(world.records().len(), 1);
    assert_eq!(
        fs::read(temp.path().join("world.checkpoint")).unwrap(),
        current
    );
    assert_eq!(
        fs::read(temp.path().join("world.previous")).unwrap(),
        previous
    );
    assert!(!store.requires_reopen());
    drop(store);
    assert_eq!(open(&temp.path()).unwrap().1, recipe);
}

// Build malicious framing without the encoder's count/byte validation.
fn unchecked_bytes(envelope: &Envelope) -> Vec<u8> {
    let snapshot = envelope.anchor.encode().unwrap();
    let journal = serde_json::to_vec(&envelope.records).unwrap();
    let mut bytes = Vec::new();
    bytes.extend(MAGIC);
    bytes.extend(1u32.to_le_bytes());
    bytes.extend(envelope.generation.to_le_bytes());
    bytes.extend(envelope.parent);
    bytes.extend((snapshot.len() as u32).to_le_bytes());
    bytes.extend((journal.len() as u32).to_le_bytes());
    bytes.extend(snapshot);
    bytes.extend(journal);
    bytes.extend(checksum(&bytes));
    bytes
}

#[test]
fn typed_and_resigned_wire_recipes_reject_record_count_and_byte_overflow() {
    let mut world = fixture::world();
    let anchor = world.checkpoint().unwrap();
    tick(&mut world);
    let record = world.records()[0].clone();
    let mut envelope = Envelope {
        generation: 2,
        parent: [1; 32],
        anchor,
        records: vec![record.clone(); MAX_RECORDS + 1],
    };
    assert!(matches!(
        envelope.encode(),
        Err(Error::Runtime(NativeError::TooLarge))
    ));
    assert!(Envelope::decode(&unchecked_bytes(&envelope)).is_err());
    envelope.records = vec![record];
    envelope.records[0].events = vec![Event::Rejected(Rejection::UnavailableAbility {
        ability: 60002,
        reason: "x".repeat(MAX_BYTES),
    })];
    assert!(matches!(
        envelope.encode(),
        Err(Error::Runtime(NativeError::TooLarge))
    ));
    assert!(Envelope::decode(&unchecked_bytes(&envelope)).is_err());
}

#[test]
fn failed_publication_retains_full_recipe_and_reopens_only_verified_old_or_new_history() {
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
        let (mut store, _) = RecipeStore::create(temp.path(), &world).unwrap();
        tick(&mut world);
        store.checkpoint(&mut world).unwrap();
        let old = store.recipe().clone();
        tick(&mut world);
        let expected = world.checkpoint().unwrap();
        store.storage.inject_fault(fault);
        assert!(store.checkpoint(&mut world).is_err());
        assert_eq!(store.recipe(), &old);
        assert_eq!(world.records().len(), 1);
        let uncertain = matches!(
            fault,
            SaveFault::AfterPreviousRename | SaveFault::AfterRename | SaveFault::BeforeVerification
        );
        assert_eq!(store.requires_reopen(), uncertain);
        if uncertain {
            assert!(store.prepare(&world).is_err());
        } else {
            store.storage.clear_fault();
            store.checkpoint(&mut world).unwrap();
        }
        drop(store);
        let (_, recipe) = open(&temp.path()).unwrap();
        if matches!(fault, SaveFault::AfterPreviousRename) {
            assert_eq!(recipe, old);
        } else {
            assert_eq!(recipe.expected_checkpoint(), &expected);
            assert_eq!(recipe.records().len(), 2);
        }
    }
}

#[test]
fn concurrent_modification_latches_without_acknowledging_live_journal() {
    let temp = Temp::new();
    let mut world = fixture::world();
    let (mut store, _) = RecipeStore::create(temp.path(), &world).unwrap();
    tick(&mut world);
    let old = store.recipe().clone();
    fs::write(temp.path().join("world.checkpoint"), b"external edit").unwrap();
    assert!(matches!(
        store.checkpoint(&mut world),
        Err(Error::Store(crate::Error::ConcurrentModification))
    ));
    assert!(store.requires_reopen());
    assert_eq!(store.recipe(), &old);
    assert_eq!(world.records().len(), 1);
}

#[test]
fn reconstruction_and_compacting_formats_are_not_interchangeable() {
    let temp = Temp::new();
    let (store, _) = RecipeStore::create(temp.path(), &fixture::world()).unwrap();
    drop(store);
    assert!(
        super::super::NativeStore::open(temp.path(), identity(), |checkpoint| {
            NativeWorld::restore_synthetic_component_test(fixture::profile(), checkpoint)
        })
        .is_err()
    );
    let other = Temp::new();
    let (store, _) = super::super::NativeStore::create(other.path(), &fixture::world()).unwrap();
    drop(store);
    assert!(open(&other.path()).is_err());
}
