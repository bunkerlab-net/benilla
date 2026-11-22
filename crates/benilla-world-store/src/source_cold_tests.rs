// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
#[cfg(unix)]
use crate::storage::LOCK;
use crate::storage::{SaveFault, CURRENT, PREVIOUS, TEMP};
use benilla_world_runtime::native::source_execution::source_rows::FloatWord;
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
            "benilla-source-cold-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn file(&self, name: &str) -> PathBuf {
        self.0.join(DIRECTORY).join(name)
    }

    fn create(&self) -> (SourceColdStore, Receipt) {
        SourceColdStore::create(&self.0, identity()).unwrap()
    }

    fn open(&self) -> Result<(SourceColdStore, Rows), Error> {
        SourceColdStore::open(&self.0, identity())
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn identity() -> Identity {
    Identity {
        world: [1; 16],
        player: PlayerId {
            owner: [2; 16],
            character: [3; 16],
        },
        account: 1,
        guid: 1,
        domain: Domain::HumanMageNorthshire,
        catalog: [4; 32],
        logic_policy: [5; 32],
        world_product: [6; 32],
    }
}

fn character() -> Box<CharacterRow> {
    Box::new(CharacterRow {
        guid: 1,
        account: 1,
        name: "Localmage".into(),
        race: 1,
        class: 8,
        gender: 0,
        level: 1,
        xp: 0,
        money: 0,
        player_bytes: 0,
        player_bytes2: 0,
        player_flags: 0,
        map: 0,
        xyz: [FloatWord::default(); 3],
        orientation: FloatWord::from_float(-0.0),
        taxi_mask: [2, 0, 0, 0, 0, 0, 0, 0],
        online: false,
        cinematic: 0,
        total_time: 0,
        level_time: 0,
        rest_bonus: FloatWord::default(),
        logout_time: 1000,
        is_logout_resting: false,
        reset_talents_cost: 0,
        reset_talents_time: 0,
        transport_pose: [FloatWord::default(); 4],
        transport_guid: 0,
        extra_flags: 4,
        stable_slots: 0,
        at_login: 32,
        zone: 0,
        death_expire_time: 0,
        taxi_path: vec![],
        honor_highest_rank: 0,
        honor_standing: 0,
        stored_honor_rating: FloatWord::default(),
        stored_dishonorable_kills: 0,
        stored_honorable_kills: 0,
        watched_faction: u32::MAX,
        drunk: 0,
        health: 51,
        powers: [165, 0, 0, 100, 0],
        explored_zones: [0; 64],
        equipment_cache: [(0, 0); 20],
        ammo_id: 0,
        action_bars: 0,
        fishing_steps: 0,
    })
}

fn image() -> Rows {
    Rows {
        character: Some(character()),
        ..Rows::default()
    }
}

#[test]
fn weekly_batch_refuses_before_publication_without_blocking_store() {
    use benilla_world_runtime::native::source_execution::source_rows::{RowDelete, RowInsert};

    let temp = Temp::new();
    let (mut store, _) = temp.create();
    let staged = store.prepare_rows(&image()).unwrap();
    let receipt = store.commit(staged).unwrap();
    let old = fs::read(temp.file(CURRENT)).unwrap();
    for operation in [
        RowOperation::Delete(RowDelete::WeeklyQuestsByGuid(1)),
        RowOperation::Insert(RowInsert::WeeklyQuest { guid: 1, quest: 42 }),
    ] {
        assert!(matches!(
            store.prepare([RowOperation::Delete(RowDelete::Character(1)), operation]),
            Err(Error::Invalid(
                "execution.player_save.Player_16180.weekly_quest_store_owner"
            ))
        ));
        assert_eq!(fs::read(temp.file(CURRENT)).unwrap(), old);
        assert_eq!(store.generation(), receipt.generation());
        assert!(!store.requires_reopen());
        assert_eq!(store.read_rows().unwrap().character(), image().character());
    }
}

#[test]
fn checked_publish_uses_source_subdirectory_and_reopens_exact_rows() {
    let temp = Temp::new();
    let (mut store, initial) = temp.create();
    assert_eq!(initial.generation(), 1);
    let original = fs::read(temp.file(CURRENT)).unwrap();
    assert_eq!(initial.digest(), hash(&original));
    assert_eq!(initial.durability(), durability());
    assert!(!temp.0.join(CURRENT).exists());
    assert!(store.read_rows().unwrap().character().is_none());

    let staged = store.prepare_rows(&image()).unwrap();
    assert_eq!(fs::read(temp.file(CURRENT)).unwrap(), original);
    let committed = store.commit(staged).unwrap();
    assert_eq!(committed.generation(), 2);
    assert_eq!(
        committed.digest(),
        hash(&fs::read(temp.file(CURRENT)).unwrap())
    );
    assert_eq!(fs::read(temp.file(PREVIOUS)).unwrap(), original);
    drop(store);

    let (reopened, rows) = temp.open().unwrap();
    assert_eq!(reopened.identity(), &identity());
    assert_eq!(reopened.generation(), 2);
    assert_eq!(rows.character(), Some(&*character()));
    assert_eq!(
        rows.character().unwrap().orientation.as_float().to_bits(),
        0x8000_0000
    );
}

#[test]
fn wrong_selected_identity_version_and_checksum_refuse_open() {
    let temp = Temp::new();
    drop(temp.create());
    let base = identity();
    for changed in [
        Identity {
            world: [9; 16],
            ..base
        },
        Identity {
            player: PlayerId {
                owner: [9; 16],
                ..base.player
            },
            ..base
        },
        Identity {
            player: PlayerId {
                character: [9; 16],
                ..base.player
            },
            ..base
        },
        Identity { account: 2, ..base },
        Identity { guid: 2, ..base },
        Identity {
            catalog: [9; 32],
            ..base
        },
        Identity {
            logic_policy: [9; 32],
            ..base
        },
        Identity {
            world_product: [9; 32],
            ..base
        },
    ] {
        assert!(matches!(
            SourceColdStore::open(&temp.0, changed),
            Err(Error::Incompatible)
        ));
    }

    let path = temp.file(CURRENT);
    let mut bytes = fs::read(&path).unwrap();
    let end = bytes.len() - CHECKSUM;
    bytes[8..12].copy_from_slice(&(VERSION + 1).to_le_bytes());
    let checksum = hash(&bytes[..end]);
    bytes[end..].copy_from_slice(&checksum);
    fs::write(&path, &bytes).unwrap();
    assert!(matches!(temp.open(), Err(Error::UnsupportedVersion)));
    bytes[8..12].copy_from_slice(&VERSION.to_le_bytes());
    let checksum = hash(&bytes[..end]);
    bytes[end..].copy_from_slice(&checksum);
    bytes[end] ^= 1;
    fs::write(&path, &bytes).unwrap();
    assert!(matches!(temp.open(), Err(Error::Corrupt)));
}

#[test]
fn creation_is_explicit_and_the_lock_is_exclusive() {
    let temp = Temp::new();
    let (store, _) = temp.create();
    let before = fs::read(temp.file(CURRENT)).unwrap();
    assert!(matches!(temp.open(), Err(Error::Locked)));
    assert!(matches!(
        SourceColdStore::create(&temp.0, identity()),
        Err(Error::Locked)
    ));
    drop(store);
    assert!(matches!(
        SourceColdStore::create(&temp.0, identity()),
        Err(Error::AlreadyExists)
    ));
    assert_eq!(fs::read(temp.file(CURRENT)).unwrap(), before);
}

#[test]
fn stale_preparation_refuses_without_blocking_or_writing() {
    let temp = Temp::new();
    let (mut store, _) = temp.create();
    let staged = store.prepare_rows(&image()).unwrap();
    let same_parent = store.prepare_rows(&Rows::default()).unwrap();
    store.commit(staged).unwrap();
    let before = fs::read(temp.file(CURRENT)).unwrap();
    let backup = fs::read(temp.file(PREVIOUS)).unwrap();
    assert!(matches!(
        store.commit(same_parent),
        Err(Error::StalePrepared)
    ));
    assert!(!store.requires_reopen());
    assert_eq!(fs::read(temp.file(CURRENT)).unwrap(), before);
    assert_eq!(fs::read(temp.file(PREVIOUS)).unwrap(), backup);
}

#[test]
fn wrong_previous_or_invalid_current_never_recovers_implicitly() {
    let temp = Temp::new();
    let (mut store, _) = temp.create();
    let staged = store.prepare_rows(&image()).unwrap();
    store.commit(staged).unwrap();
    drop(store);
    let previous = fs::read(temp.file(PREVIOUS)).unwrap();
    let different = Header {
        generation: 1,
        parent: [0; 32],
    }
    .encode(&identity(), &image())
    .unwrap();
    fs::write(temp.file(PREVIOUS), different).unwrap();
    assert!(matches!(temp.open(), Err(Error::DivergentHistory)));
    let moved = temp.0.join("backup");
    fs::rename(temp.file(PREVIOUS), &moved).unwrap();
    assert!(matches!(temp.open(), Err(Error::DivergentHistory)));
    fs::write(&moved, previous).unwrap();
    fs::rename(moved, temp.file(PREVIOUS)).unwrap();
    fs::write(temp.file(CURRENT), b"broken").unwrap();
    // A valid previous image is still insufficient when current is invalid.
    assert!(matches!(temp.open(), Err(Error::Corrupt)));
}

#[test]
fn external_write_to_current_or_previous_blocks_publication() {
    for name in [CURRENT, PREVIOUS] {
        let temp = Temp::new();
        let (mut store, _) = temp.create();
        let staged = store.prepare_rows(&image()).unwrap();
        store.commit(staged).unwrap();
        let staged = store.prepare_rows(&image()).unwrap();
        fs::write(temp.file(name), b"external writer").unwrap();
        let current = fs::read(temp.file(CURRENT)).unwrap();
        let previous = fs::read(temp.file(PREVIOUS)).unwrap();
        assert!(matches!(
            store.commit(staged),
            Err(Error::ConcurrentModification)
        ));
        assert!(store.requires_reopen());
        assert_eq!(fs::read(temp.file(CURRENT)).unwrap(), current);
        assert_eq!(fs::read(temp.file(PREVIOUS)).unwrap(), previous);
    }
}

#[test]
fn publication_faults_block_until_strict_reopen() {
    for fault in [
        SaveFault::BeforeRename,
        SaveFault::AfterRename,
        SaveFault::BeforeVerification,
    ] {
        let temp = Temp::new();
        let (mut store, _) = temp.create();
        let old = fs::read(temp.file(CURRENT)).unwrap();
        let staged = store.prepare_rows(&image()).unwrap();
        let retry = store.prepare_rows(&image()).unwrap();
        store.storage.inject_fault(fault);
        let result = store.commit(staged);
        let published = fault != SaveFault::BeforeRename;
        if published {
            assert!(matches!(result, Err(Error::CommitUncertain)));
            assert_ne!(fs::read(temp.file(CURRENT)).unwrap(), old);
        } else {
            assert!(matches!(result, Err(Error::Io(_))));
            assert_eq!(fs::read(temp.file(CURRENT)).unwrap(), old);
        }
        assert!(store.requires_reopen());
        assert_eq!(store.generation(), 1);
        assert!(matches!(store.commit(retry), Err(Error::CommitUncertain)));
        assert!(matches!(
            store.prepare_rows(&image()),
            Err(Error::CommitUncertain)
        ));
        assert!(matches!(store.read_rows(), Err(Error::CommitUncertain)));
        assert_eq!(fs::read(temp.file(PREVIOUS)).unwrap(), old);
        assert!(!temp.file(TEMP).exists());
        drop(store);
        let (reopened, rows) = temp.open().unwrap();
        assert_eq!(reopened.generation(), if published { 2 } else { 1 });
        assert_eq!(rows.character().is_some(), published);
        assert!(!reopened.requires_reopen());
    }
}

#[test]
fn invalid_path_never_creates_a_world() {
    let temp = Temp::new();
    // Joining onto a verbatim Windows base can normalize the parent component.
    let mut raw = temp.0.as_os_str().to_os_string();
    raw.push(std::path::MAIN_SEPARATOR_STR);
    raw.push("..");
    raw.push(std::path::MAIN_SEPARATOR_STR);
    raw.push("escape");
    let path = PathBuf::from(raw);
    assert!(path
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir)));
    assert!(matches!(
        SourceColdStore::create(path, identity()),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        SourceColdStore::create(temp.0.join("missing-parent"), identity()),
        Err(Error::Io(_))
    ));
    assert!(!temp.0.join(DIRECTORY).exists());
}

#[cfg(unix)]
#[test]
fn symlinks_and_hard_links_are_refused() {
    use std::os::unix::fs::symlink;
    let temp = Temp::new();
    symlink(&temp.0, temp.0.join(DIRECTORY)).unwrap();
    assert!(SourceColdStore::create(&temp.0, identity()).is_err());
    fs::remove_file(temp.0.join(DIRECTORY)).unwrap();
    drop(temp.create());
    fs::hard_link(temp.file(CURRENT), temp.0.join("link")).unwrap();
    assert!(matches!(temp.open(), Err(Error::Invalid(_))));
    fs::remove_file(temp.file(CURRENT)).unwrap();
    symlink(temp.0.join("link"), temp.file(CURRENT)).unwrap();
    assert!(temp.open().is_err());
    fs::remove_file(temp.file(LOCK)).unwrap();
    symlink(temp.0.join("link"), temp.file(LOCK)).unwrap();
    assert!(temp.open().is_err());
}

#[test]
fn format_91_fixture_and_source_files_are_independent() {
    use crate::native::{Identity as NativeIdentity, NativeStore};
    use benilla_world_runtime::native::{synthetic_fixture, NativeWorld, NATIVE_FORMAT_VERSION};

    assert_eq!(NATIVE_FORMAT_VERSION, 91);
    let temp = Temp::new();
    let parent = temp.0.join("world");
    let world = synthetic_fixture::world();
    let (native, _) = NativeStore::create(&parent, &world).unwrap();
    let before = fs::read(parent.join(CURRENT)).unwrap();
    drop(native);
    let (mut source, _) = SourceColdStore::create(&parent, identity()).unwrap();
    let staged = source.prepare_rows(&image()).unwrap();
    source.commit(staged).unwrap();
    assert_eq!(fs::read(parent.join(CURRENT)).unwrap(), before);
    drop(source);
    let selected = NativeIdentity::of(&world.checkpoint().unwrap());
    assert!(
        NativeStore::open(parent.join(DIRECTORY), selected, |checkpoint| {
            NativeWorld::restore_synthetic_component_test(synthetic_fixture::profile(), checkpoint)
        })
        .is_err()
    );
    fs::write(parent.join(DIRECTORY).join(CURRENT), &before).unwrap();
    assert!(matches!(
        SourceColdStore::open(&parent, identity()),
        Err(Error::Corrupt)
    ));
}

#[path = "source_cold/rows_tests.rs"]
mod batch;

#[path = "source_cold_character_tests.rs"]
mod character_create;
