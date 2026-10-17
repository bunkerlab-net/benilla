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
    let mut world =
        NativeWorld::restore_synthetic_component_test(fixture::profile(), recipe.anchor().clone())
            .unwrap();
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
    assert!(store.prepare_reanchor(&world).is_err());
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
    bytes.extend(VERSION.to_le_bytes());
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

#[test]
fn explicit_cache_free_reanchors_continue_past_the_retained_history_cap() {
    let temp = Temp::new();
    let mut world = fixture::world();
    let (mut store, receipt) = RecipeStore::create(temp.path(), &world).unwrap();
    receipt.acknowledge(&mut world).unwrap();
    for round in 0..5 {
        // Two ordinary saves prove exact same-anchor prefixes at nonzero anchors too.
        for _ in 0..2 {
            for _ in 0..MAX_RECORDS / 8 {
                tick(&mut world);
            }
            store.checkpoint(&mut world).unwrap();
        }
        assert_eq!(store.recipe().records().len(), MAX_RECORDS / 4);
        let expected = world.checkpoint().unwrap();
        let retained = store.recipe().clone();
        let generation = store.generation();
        let parent = store.digest;
        drop(store);
        let (opened, recipe) = open(&temp.path()).unwrap();
        assert_eq!(recipe, retained);
        assert_eq!(reexecute(&recipe).checkpoint().unwrap(), expected);
        store = opened;

        world.validate_saved_session_reopen().unwrap();
        let receipt = store.checkpoint_reanchor(&mut world).unwrap();
        assert_eq!(receipt.generation(), generation + 1);
        assert_eq!(receipt.boundary(), expected.identity());
        assert_eq!(store.recipe().anchor(), &expected);
        assert_eq!(store.recipe().expected_checkpoint(), &expected);
        assert!(store.recipe().records().is_empty());
        assert!(world.records().is_empty());
        let envelope = read_envelope(&temp);
        assert_eq!(envelope.parent, parent);
        assert_eq!(
            envelope.anchor.identity().tick,
            ((round + 1) * MAX_RECORDS / 4) as u64
        );
        assert_ne!(envelope.anchor.identity().history_tip, [0; 32]);
        let saved = store.recipe().clone();
        drop(store);
        let (opened, recipe) = open(&temp.path()).unwrap();
        assert_eq!(recipe, saved);
        world = reexecute(&recipe);
        store = opened;
    }
    assert!(world.state().tick > MAX_RECORDS as u64);
    tick(&mut world);
    store.checkpoint(&mut world).unwrap();
    let expected = world.checkpoint().unwrap();
    drop(store);
    let (_, recipe) = open(&temp.path()).unwrap();
    assert_eq!(recipe.records().len(), 1);
    assert_eq!(reexecute(&recipe).checkpoint().unwrap(), expected);
}

#[test]
fn reanchor_requires_the_exact_already_saved_and_acknowledged_boundary() {
    let temp = Temp::new();
    let mut world = fixture::world();
    let (mut store, _) = RecipeStore::create(temp.path(), &world).unwrap();
    tick(&mut world);
    let unsaved = world.clone();
    let before = fs::read(temp.path().join("world.checkpoint")).unwrap();
    assert!(matches!(
        store.checkpoint_reanchor(&mut world),
        Err(Error::Store(crate::Error::DivergentHistory))
    ));
    assert_eq!(world.records(), unsaved.records());
    assert_eq!(world.checkpoint().unwrap(), unsaved.checkpoint().unwrap());
    assert_eq!(
        fs::read(temp.path().join("world.checkpoint")).unwrap(),
        before
    );

    // Clearing a journal without a durable receipt cannot turn an unsaved tick into an anchor.
    let mut falsely_acknowledged = world.clone();
    falsely_acknowledged
        .checkpoint_committed(&world.checkpoint().unwrap())
        .unwrap();
    assert!(matches!(
        store.prepare_reanchor(&falsely_acknowledged),
        Err(Error::Store(crate::Error::DivergentHistory))
    ));
    let receipt = store.commit(store.prepare(&world).unwrap()).unwrap();
    assert!(matches!(
        store.prepare_reanchor(&world),
        Err(Error::Store(crate::Error::DivergentHistory))
    ));
    receipt.acknowledge(&mut world).unwrap();

    let mut divergent =
        NativeWorld::synthetic_component_test(fixture::profile(), [8; 16], fixture::player(), 4321)
            .unwrap();
    tick(&mut divergent);
    divergent
        .checkpoint_committed(&divergent.checkpoint().unwrap())
        .unwrap();
    assert!(matches!(
        store.prepare_reanchor(&divergent),
        Err(Error::Store(crate::Error::DivergentHistory))
    ));
    assert!(matches!(
        store.prepare_reanchor(&fixture::world()),
        Err(Error::Store(crate::Error::DivergentHistory))
    ));
    let wrong_identity =
        NativeWorld::synthetic_component_test(fixture::profile(), [9; 16], fixture::player(), 1234)
            .unwrap();
    assert!(matches!(
        store.prepare_reanchor(&wrong_identity),
        Err(Error::Store(crate::Error::Incompatible))
    ));
    let prepared = store.prepare_reanchor(&world).unwrap();
    let receipt = store.commit(prepared).unwrap();
    receipt.acknowledge(&mut world).unwrap();
    assert_eq!(store.recipe().anchor(), &world.checkpoint().unwrap());
    assert!(store.recipe().records().is_empty());
}

#[test]
fn reanchor_preparations_and_receipts_keep_normal_stale_guards() {
    let temp = Temp::new();
    let mut world = fixture::world();
    let (mut store, _) = RecipeStore::create(temp.path(), &world).unwrap();
    tick(&mut world);
    store.checkpoint(&mut world).unwrap();
    let stale_reanchor = store.prepare_reanchor(&world).unwrap();
    store.checkpoint(&mut world).unwrap();
    assert!(matches!(
        store.commit(stale_reanchor),
        Err(Error::Store(crate::Error::StalePrepared))
    ));
    let stale_append = store.prepare(&world).unwrap();
    let receipt = store.checkpoint_reanchor(&mut world).unwrap();
    assert!(matches!(
        store.commit(stale_append),
        Err(Error::Store(crate::Error::StalePrepared))
    ));
    tick(&mut world);
    assert!(receipt.acknowledge(&mut world).is_err());
    assert_eq!(world.records().len(), 1);
    store.checkpoint(&mut world).unwrap();
    assert_eq!(store.recipe().anchor().identity().tick, 1);
    assert_eq!(store.recipe().records().len(), 1);
}

#[test]
fn reanchor_cannot_substitute_nonzero_anchor_parent_identity_or_new_suffix() {
    for mutation in 0..6 {
        let temp = Temp::new();
        let mut world = fixture::world();
        let (mut store, _) = RecipeStore::create(temp.path(), &world).unwrap();
        tick(&mut world);
        let older = world.checkpoint().unwrap();
        tick(&mut world);
        store.checkpoint(&mut world).unwrap();
        store.checkpoint_reanchor(&mut world).unwrap();
        drop(store);
        let mut envelope = read_envelope(&temp);
        match mutation {
            0 => envelope.anchor = older,
            1 | 2 => {
                let mut alternate = NativeWorld::synthetic_component_test(
                    fixture::profile(),
                    if mutation == 1 { [8; 16] } else { [9; 16] },
                    fixture::player(),
                    4321,
                )
                .unwrap();
                tick(&mut alternate);
                tick(&mut alternate);
                envelope.anchor = alternate.checkpoint().unwrap();
            }
            3 => envelope.parent = [99; 32],
            4 => envelope.generation += 1,
            5 => {
                tick(&mut world);
                envelope.records = world.records().to_vec();
            }
            _ => unreachable!(),
        }
        write_envelope(&temp, &envelope);
        let current = fs::read(temp.path().join("world.checkpoint")).unwrap();
        let previous = fs::read(temp.path().join("world.previous")).unwrap();
        if mutation == 2 {
            assert!(matches!(
                open(&temp.path()),
                Err(Error::Store(crate::Error::Incompatible))
            ));
        } else {
            assert!(
                matches!(
                    open(&temp.path()),
                    Err(Error::Store(crate::Error::DivergentHistory))
                ),
                "mutation {mutation}"
            );
        }
        assert_eq!(
            fs::read(temp.path().join("world.checkpoint")).unwrap(),
            current
        );
        assert_eq!(
            fs::read(temp.path().join("world.previous")).unwrap(),
            previous
        );
    }
}

#[test]
fn reanchor_still_privately_validates_previous_records_with_a_matching_parent_digest() {
    let temp = Temp::new();
    let mut world = fixture::world();
    let (mut store, _) = RecipeStore::create(temp.path(), &world).unwrap();
    tick(&mut world);
    store.checkpoint(&mut world).unwrap();
    store.checkpoint_reanchor(&mut world).unwrap();
    drop(store);
    let mut current = read_envelope(&temp);
    let previous_path = temp.path().join("world.previous");
    let mut previous = Envelope::decode(&fs::read(&previous_path).unwrap()).unwrap();
    previous.records[0].after = [99; 32];
    let bytes = previous.encode().unwrap();
    current.parent = hash(&bytes);
    fs::write(previous_path, bytes).unwrap();
    write_envelope(&temp, &current);
    assert!(open(&temp.path()).is_err());
}

#[test]
fn failed_reanchor_publication_preserves_boundary_and_reopens_old_or_new_recipe() {
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
        let boundary = world.checkpoint().unwrap();
        let old_generation = store.generation();
        store.storage.inject_fault(fault);
        assert!(store.checkpoint_reanchor(&mut world).is_err());
        assert_eq!(store.recipe(), &old);
        assert_eq!(store.generation(), old_generation);
        assert_eq!(world.checkpoint().unwrap(), boundary);
        assert!(world.records().is_empty());
        let uncertain = matches!(
            fault,
            SaveFault::AfterPreviousRename | SaveFault::AfterRename | SaveFault::BeforeVerification
        );
        assert_eq!(store.requires_reopen(), uncertain);
        if uncertain {
            assert!(matches!(
                store.prepare_reanchor(&world),
                Err(Error::Store(crate::Error::CommitUncertain))
            ));
            assert!(store.prepare(&world).is_err());
        } else {
            store.storage.clear_fault();
            store.checkpoint_reanchor(&mut world).unwrap();
        }
        drop(store);
        let (store, recipe) = open(&temp.path()).unwrap();
        assert_eq!(recipe.expected_checkpoint(), &boundary);
        if matches!(fault, SaveFault::AfterPreviousRename) {
            assert_eq!(recipe, old);
            assert_eq!(store.generation(), old_generation);
        } else {
            assert_eq!(recipe.anchor(), &boundary);
            assert!(recipe.records().is_empty());
            assert_eq!(store.generation(), old_generation + 1);
        }
        assert_eq!(reexecute(&recipe).checkpoint().unwrap(), boundary);
    }
}

#[test]
fn reanchor_external_modification_latches_without_changing_the_world() {
    for target in ["world.checkpoint", "world.previous"] {
        let temp = Temp::new();
        let mut world = fixture::world();
        let (mut store, _) = RecipeStore::create(temp.path(), &world).unwrap();
        tick(&mut world);
        store.checkpoint(&mut world).unwrap();
        let old = store.recipe().clone();
        let boundary = world.checkpoint().unwrap();
        fs::write(temp.path().join(target), b"external edit").unwrap();
        assert!(matches!(
            store.checkpoint_reanchor(&mut world),
            Err(Error::Store(crate::Error::ConcurrentModification))
        ));
        assert!(store.requires_reopen());
        assert_eq!(store.recipe(), &old);
        assert_eq!(world.checkpoint().unwrap(), boundary);
        assert!(world.records().is_empty());
        assert!(store.prepare_reanchor(&world).is_err());
    }
}

#[test]
fn recipe_v1_is_explicitly_rejected_without_migration_and_v2_genesis_stays_tick_zero() {
    for target in ["world.checkpoint", "world.previous"] {
        let temp = Temp::new();
        let mut world = fixture::world();
        let (mut store, _) = RecipeStore::create(temp.path(), &world).unwrap();
        tick(&mut world);
        store.checkpoint(&mut world).unwrap();
        drop(store);
        let path = temp.path().join(target);
        let mut legacy = fs::read(&path).unwrap();
        legacy[..8].copy_from_slice(LEGACY_MAGIC);
        legacy[8..12].copy_from_slice(&1u32.to_le_bytes());
        let end = legacy.len() - 32;
        let mut hash = Sha256::new();
        hash.update(b"benilla-native-reconstruction-store-v1\0");
        hash.update(&legacy[..end]);
        legacy[end..].copy_from_slice(&hash.finalize());
        fs::write(&path, &legacy).unwrap();
        assert!(matches!(
            open(&temp.path()),
            Err(Error::Store(crate::Error::UnsupportedVersion))
        ));
        assert_eq!(fs::read(path).unwrap(), legacy);
    }
    let mut world = fixture::world();
    tick(&mut world);
    let envelope = Envelope {
        generation: 1,
        parent: [0; 32],
        anchor: world.checkpoint().unwrap(),
        records: vec![],
    };
    assert!(matches!(
        envelope.encode(),
        Err(Error::Store(crate::Error::Corrupt))
    ));
    assert!(matches!(
        Envelope::decode(&unchecked_bytes(&envelope)),
        Err(Error::Store(crate::Error::Corrupt))
    ));
}
