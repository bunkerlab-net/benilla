// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_world_data::{load_json, Limits};
use benilla_world_runtime::{Command, Input, OwnedInput, PlayerId, Snapshot};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use storage::{SaveFault, CURRENT, LOCK, PREVIOUS, TEMP};
static NEXT: AtomicU64 = AtomicU64::new(0);
const FIXTURE: &[u8] = include_bytes!("../../benilla-world-data/tests/fixtures/original-camp.json");
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        // macOS /tmp can itself be a symlink; only synthetic temp fixtures are canonicalized.
        let base = std::env::temp_dir().canonicalize().unwrap();
        let dir = base.join(format!(
            "benilla-world-store-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        Self(dir)
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
fn data() -> WorldData {
    load_json(FIXTURE, Limits::default()).unwrap()
}
fn player() -> PlayerId {
    PlayerId {
        owner: [1; 16],
        character: [2; 16],
    }
}
fn world() -> World {
    World::new(data(), Rules::original_camp(), [7; 16], 42, vec![player()]).unwrap()
}
fn open(path: &Path) -> Result<(WorldStore, World), Error> {
    WorldStore::open(
        path,
        WorldIdentity::of(&world()),
        data(),
        Rules::original_camp(),
    )
}
fn tick(w: &mut World) {
    w.step(w.state().tick + 1, vec![]).unwrap();
}
fn command(w: &mut World, command: Command) {
    w.step(
        w.state().tick + 1,
        vec![OwnedInput {
            authenticated_owner: player().owner,
            input: Input {
                world: w.state().world,
                tick: w.state().tick + 1,
                player: player(),
                sequence: w.player(player()).unwrap().next_sequence,
                command,
            },
        }],
    )
    .unwrap();
}
fn bytes(w: &World) -> Vec<u8> {
    w.snapshot_bytes().unwrap()
}
fn rewrite(path: &Path, value: &[u8]) {
    fs::write(path, value).unwrap();
}

#[test]
fn explicit_creation_and_exact_identity_binding() {
    let temp = Temp::new();
    assert!(open(&temp.path()).is_err());
    assert!(!temp.path().exists());
    let mut world = world();
    let (store, receipt) = WorldStore::create(temp.path(), &world).unwrap();
    receipt.acknowledge(&mut world).unwrap();
    assert_eq!(receipt.generation(), 1);
    assert_eq!(receipt.boundary(), &world.join_boundary());
    assert_eq!(receipt.durability(), store.durability());
    assert!(temp.path().join(CURRENT).exists());
    assert!(!temp.path().join(PREVIOUS).exists());
    drop(store);
    for part in 0..3 {
        let mut id = WorldIdentity::of(&world);
        match part {
            0 => id.world = [8; 16],
            1 => id.content_digest = [8; 32],
            _ => id.rules_digest = [8; 32],
        }
        assert!(matches!(
            WorldStore::open(temp.path(), id, data(), Rules::original_camp()),
            Err(Error::Incompatible)
        ));
    }
    assert!(matches!(
        WorldStore::create(temp.path(), &world),
        Err(Error::AlreadyExists)
    ));
    tick(&mut world);
    assert!(matches!(
        WorldStore::create(temp.0.join("other"), &world),
        Err(Error::Invalid(_))
    ));
    assert!(!temp.0.join("other").exists());
}

#[test]
fn generations_reopen_and_real_runtime_continue_past_ten_thousand_ticks() {
    let temp = Temp::new();
    let mut w = world();
    let (mut store, receipt) = WorldStore::create(temp.path(), &w).unwrap();
    receipt.acknowledge(&mut w).unwrap();
    for round in 0..12 {
        for _ in 0..1001 {
            tick(&mut w);
        }
        let expected = bytes(&w);
        let before = fs::read(temp.path().join(CURRENT)).unwrap();
        let receipt = store.checkpoint(&mut w).unwrap();
        assert_eq!(receipt.generation(), round + 2);
        assert!(w.last_tick().is_none());
        assert!(!w.checkpoint_due());
        assert_eq!(fs::read(temp.path().join(PREVIOUS)).unwrap(), before);
        drop(store);
        (store, w) = open(&temp.path()).unwrap();
        assert_eq!(bytes(&w), expected);
        assert_eq!(store.generation(), round + 2);
    }
    assert_eq!(w.state().tick, 12_012);
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 3);
    assert!(fs::metadata(temp.path().join(CURRENT)).unwrap().len() <= MAX_CHECKPOINT_BYTES as u64);
    tick(&mut w);
    store.checkpoint(&mut w).unwrap();
    assert_eq!(w.state().tick, 12_013);
}

#[test]
fn stale_async_receipt_preserves_newer_history_and_can_save_again() {
    let temp = Temp::new();
    let mut w = world();
    let (mut store, _) = WorldStore::create(temp.path(), &w).unwrap();
    tick(&mut w);
    let prepared = store.prepare(&w).unwrap();
    tick(&mut w);
    let journal = w.journal_bytes().unwrap();
    let receipt = store.commit(prepared).unwrap();
    assert!(matches!(
        receipt.acknowledge(&mut w),
        Err(Error::Runtime(
            benilla_world_runtime::Error::StaleCheckpoint
        ))
    ));
    assert_eq!(w.journal_bytes().unwrap(), journal);
    store.checkpoint(&mut w).unwrap();
    let expected = bytes(&w);
    drop(store);
    let (_, restored) = open(&temp.path()).unwrap();
    assert_eq!(bytes(&restored), expected);
}

#[test]
fn receipts_bind_chain_not_only_equal_final_state() {
    let temp = Temp::new();
    let mut a = world();
    let mut b = world();
    let (mut store, _) = WorldStore::create(temp.path(), &a).unwrap();
    command(&mut a, Command::Move { delta: [0, 0, 0] });
    command(
        &mut b,
        Command::Move {
            delta: [100_000, 0, 0],
        },
    );
    assert_eq!(a.join_boundary(), b.join_boundary());
    let receipt = store.commit(store.prepare(&a).unwrap()).unwrap();
    let b_journal = b.journal_bytes().unwrap();
    assert!(receipt.acknowledge(&mut b).is_err());
    assert_eq!(b.journal_bytes().unwrap(), b_journal);
    assert!(matches!(store.prepare(&b), Err(Error::DivergentHistory)));
}

#[test]
fn stale_preparations_and_divergent_worlds_are_not_merged() {
    let temp = Temp::new();
    let mut w = world();
    let (mut store, _) = WorldStore::create(temp.path(), &w).unwrap();
    tick(&mut w);
    let stale = store.prepare(&w).unwrap();
    store.checkpoint(&mut w).unwrap();
    let before = fs::read(temp.path().join(CURRENT)).unwrap();
    assert!(matches!(store.commit(stale), Err(Error::StalePrepared)));
    let mut fork = world();
    command(&mut fork, Command::Move { delta: [10, 0, 0] });
    assert!(matches!(store.prepare(&fork), Err(Error::DivergentHistory)));
    assert_eq!(fs::read(temp.path().join(CURRENT)).unwrap(), before);
}

#[test]
fn deterministic_faults_preserve_journal_and_latch_uncertain_publication() {
    for fault in [
        SaveFault::PartialWrite,
        SaveFault::BeforeSync,
        SaveFault::BeforeRename,
        SaveFault::AfterRename,
        SaveFault::BeforePreviousRename,
        SaveFault::AfterPreviousRename,
        SaveFault::BeforeVerification,
    ] {
        let temp = Temp::new();
        let mut w = world();
        let (mut store, _) = WorldStore::create(temp.path(), &w).unwrap();
        let original = fs::read(temp.path().join(CURRENT)).unwrap();
        tick(&mut w);
        let journal = w.journal_bytes().unwrap();
        store.storage.inject_fault(fault);
        let result = store.checkpoint(&mut w);
        assert!(result.is_err(), "{fault:?}");
        let uncertain = matches!(
            fault,
            SaveFault::AfterRename | SaveFault::AfterPreviousRename | SaveFault::BeforeVerification
        );
        assert_eq!(
            matches!(result, Err(Error::CommitUncertain)),
            uncertain,
            "{fault:?}"
        );
        assert_eq!(store.requires_reopen(), uncertain, "{fault:?}");
        assert_eq!(store.generation(), 1);
        assert_eq!(w.journal_bytes().unwrap(), journal);
        if uncertain {
            assert!(matches!(store.prepare(&w), Err(Error::CommitUncertain)));
        }
        let published = matches!(
            fault,
            SaveFault::AfterRename | SaveFault::BeforeVerification
        );
        if !published {
            assert_eq!(fs::read(temp.path().join(CURRENT)).unwrap(), original);
        }
        if !matches!(fault, SaveFault::BeforePreviousRename) {
            assert_eq!(fs::read(temp.path().join(PREVIOUS)).unwrap(), original);
        }
        assert!(!temp.path().join(TEMP).exists());
        drop(store);
        let (_, restored) = open(&temp.path()).unwrap();
        assert_eq!(restored.state().tick, u64::from(published), "{fault:?}");
    }
}

#[test]
fn failed_new_world_publication_never_implicitly_recreates_identity() {
    let temp = Temp::new();
    let storage = Storage::acquire(&temp.path(), true).unwrap();
    rewrite(&temp.path().join(TEMP), b"interrupted initial checkpoint");
    drop(storage);
    assert!(WorldStore::create(temp.path(), &world()).is_err());
    assert_eq!(
        fs::read(temp.path().join(TEMP)).unwrap(),
        b"interrupted initial checkpoint"
    );
    assert!(open(&temp.path()).is_err());
    assert!(!temp.path().join(CURRENT).exists());
}

#[test]
fn corrupt_missing_or_incompatible_current_never_falls_back_to_previous() {
    for kind in 0..6 {
        let temp = Temp::new();
        let mut w = world();
        let (mut store, _) = WorldStore::create(temp.path(), &w).unwrap();
        tick(&mut w);
        store.checkpoint(&mut w).unwrap();
        drop(store);
        let backup = fs::read(temp.path().join(PREVIOUS)).unwrap();
        let path = temp.path().join(CURRENT);
        let mut current = fs::read(&path).unwrap();
        match kind {
            0 => current.truncate(current.len() / 2),
            1 => current[70] ^= 1,
            2 => current[8..12].copy_from_slice(&99u32.to_le_bytes()),
            3 => {
                fs::remove_file(&path).unwrap();
            }
            4 => {
                let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
                file.set_len(MAX_CHECKPOINT_BYTES as u64 + 1).unwrap();
            }
            _ => current.push(0),
        }
        if !matches!(kind, 3 | 4) {
            rewrite(&path, &current);
        }
        assert!(open(&temp.path()).is_err());
        assert!(WorldStore::create(temp.path(), &world()).is_err());
        assert_eq!(fs::read(temp.path().join(PREVIOUS)).unwrap(), backup);
    }
}

#[test]
fn bounded_framing_and_recomputed_checksums_do_not_bypass_runtime_validation() {
    let temp = Temp::new();
    let mut w = world();
    let (mut store, _) = WorldStore::create(temp.path(), &w).unwrap();
    tick(&mut w);
    store.checkpoint(&mut w).unwrap();
    drop(store);
    let original = fs::read(temp.path().join(CURRENT)).unwrap();
    for kind in 0..6 {
        let mut env = Envelope::decode(&original).unwrap();
        match kind {
            0 => {
                let mut j: Journal = serde_json::from_slice(&env.journal).unwrap();
                j.records[0].after_digest = [5; 32];
                env.journal = serde_json::to_vec(&j).unwrap();
            }
            1 => {
                let mut j: Journal = serde_json::from_slice(&env.journal).unwrap();
                j.final_boundary.tick += 1;
                env.journal = serde_json::to_vec(&j).unwrap();
            }
            2 => {
                let mut j: Journal = serde_json::from_slice(&env.journal).unwrap();
                j.base.world = [8; 16];
                env.journal = serde_json::to_vec(&j).unwrap();
            }
            3 => {
                let mut s: Snapshot = serde_json::from_slice(&env.snapshot).unwrap();
                s.state.tick += 1;
                env.snapshot = serde_json::to_vec(&s).unwrap();
            }
            4 => {
                env.journal.insert(0, b' ');
            }
            _ => {
                env.generation = 99;
            }
        }
        rewrite(&temp.path().join(CURRENT), &env.encode().unwrap());
        assert!(open(&temp.path()).is_err(), "case {kind}");
    }
    let mut huge = original;
    huge[52..56].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(Envelope::decode(&huge).is_err());
}

#[test]
fn missing_corrupt_or_unrelated_previous_fails_closed() {
    for case in 0..3 {
        let temp = Temp::new();
        let mut w = world();
        let (mut store, _) = WorldStore::create(temp.path(), &w).unwrap();
        tick(&mut w);
        store.checkpoint(&mut w).unwrap();
        drop(store);
        match case {
            0 => fs::remove_file(temp.path().join(PREVIOUS)).unwrap(),
            1 => rewrite(&temp.path().join(PREVIOUS), b"broken"),
            _ => {
                let mut env =
                    Envelope::decode(&fs::read(temp.path().join(PREVIOUS)).unwrap()).unwrap();
                env.generation = 3;
                env.parent_digest = [3; 32];
                rewrite(&temp.path().join(PREVIOUS), &env.encode().unwrap());
            }
        }
        assert!(open(&temp.path()).is_err());
    }
}

#[test]
fn duplicate_writer_and_external_changes_are_rejected() {
    let temp = Temp::new();
    let mut w = world();
    let (mut store, _) = WorldStore::create(temp.path(), &w).unwrap();
    assert!(matches!(open(&temp.path()), Err(Error::Locked)));
    assert!(matches!(
        WorldStore::create(temp.path(), &w),
        Err(Error::Locked)
    ));
    tick(&mut w);
    let prepared = store.prepare(&w).unwrap();
    rewrite(&temp.path().join(CURRENT), b"unexpected external edit");
    assert!(matches!(
        store.commit(prepared),
        Err(Error::ConcurrentModification)
    ));
    assert!(store.requires_reopen());
    assert_eq!(
        fs::read(temp.path().join(CURRENT)).unwrap(),
        b"unexpected external edit"
    );
    assert!(w.last_tick().is_some());
}

#[test]
fn separate_process_cannot_take_writer_lock() {
    let temp = Temp::new();
    let (_store, _) = WorldStore::create(temp.path(), &world()).unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "tests::lock_child_probe", "--ignored"])
        .env("BENILLA_SYNTHETIC_WORLD_LOCK_TEST", temp.path())
        .status()
        .unwrap();
    assert!(result.success());
}
#[test]
#[ignore = "subprocess probe invoked by separate_process_cannot_take_writer_lock"]
fn lock_child_probe() {
    let Some(path) = std::env::var_os("BENILLA_SYNTHETIC_WORLD_LOCK_TEST") else {
        return;
    };
    assert!(matches!(open(Path::new(&path)), Err(Error::Locked)));
}

#[test]
fn root_and_parent_traversal_are_rejected() {
    let temp = Temp::new();
    // canonicalize() produces a verbatim Windows path. PathBuf::join would
    // normalize its parent component before the store ever sees the input.
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
        WorldStore::create(path, &world()),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        WorldStore::create("../escape", &world()),
        Err(Error::Invalid(_))
    ));
    #[cfg(unix)]
    assert!(WorldStore::create("/", &world()).is_err());
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
    #[test]
    fn private_modes_and_link_rejection() {
        let temp = Temp::new();
        let (store, _) = WorldStore::create(temp.path(), &world()).unwrap();
        assert_eq!(fs::metadata(temp.path()).unwrap().mode() & 0o777, 0o700);
        for name in [CURRENT, LOCK] {
            assert_eq!(
                fs::metadata(temp.path().join(name)).unwrap().mode() & 0o777,
                0o600
            );
        }
        drop(store);
        let linked = temp.0.join("alias");
        symlink(temp.path(), &linked).unwrap();
        assert!(open(&linked).is_err());
        let saved = temp.path().join("held");
        fs::rename(temp.path().join(CURRENT), &saved).unwrap();
        symlink(&saved, temp.path().join(CURRENT)).unwrap();
        assert!(open(&temp.path()).is_err());
        fs::remove_file(temp.path().join(CURRENT)).unwrap();
        fs::hard_link(&saved, temp.path().join(CURRENT)).unwrap();
        assert!(open(&temp.path()).is_err());
        fs::remove_file(temp.path().join(CURRENT)).unwrap();
        fs::rename(&saved, temp.path().join(CURRENT)).unwrap();
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(open(&temp.path()).is_err());
    }
    #[test]
    fn replaced_lock_and_fifo_do_not_redirect_or_hang() {
        let temp = Temp::new();
        let mut w = world();
        let (mut store, _) = WorldStore::create(temp.path(), &w).unwrap();
        fs::rename(temp.path().join(LOCK), temp.path().join("held-lock")).unwrap();
        fs::write(temp.path().join(LOCK), b"").unwrap();
        fs::set_permissions(temp.path().join(LOCK), fs::Permissions::from_mode(0o600)).unwrap();
        tick(&mut w);
        assert!(matches!(
            store.checkpoint(&mut w),
            Err(Error::ConcurrentModification)
        ));
        assert!(store.requires_reopen());
        drop(store);
        fs::remove_file(temp.path().join(CURRENT)).unwrap();
        use std::os::unix::ffi::OsStrExt;
        let name =
            std::ffi::CString::new(temp.path().join(CURRENT).as_os_str().as_bytes()).unwrap();
        // SAFETY: NUL-terminated synthetic test path; mkfifo retains no pointers.
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        assert!(open(&temp.path()).is_err());
    }
    #[test]
    fn renamed_parent_cannot_redirect_held_store() {
        let temp = Temp::new();
        let mut w = world();
        let (mut store, _) = WorldStore::create(temp.path(), &w).unwrap();
        let held = temp.0.join("held-world");
        fs::rename(temp.path(), &held).unwrap();
        fs::create_dir(temp.path()).unwrap();
        fs::write(temp.path().join(CURRENT), b"unrelated").unwrap();
        tick(&mut w);
        store.checkpoint(&mut w).unwrap();
        assert_eq!(fs::read(temp.path().join(CURRENT)).unwrap(), b"unrelated");
        assert!(held.join(PREVIOUS).exists());
        drop(store);
        let (_, restored) = open(&held).unwrap();
        assert_eq!(restored.state().tick, 1);
    }
}

#[test]
fn backup_external_edits_and_pending_files_are_preserved() {
    let temp = Temp::new();
    let mut w = world();
    let (mut store, _) = WorldStore::create(temp.path(), &w).unwrap();
    tick(&mut w);
    store.checkpoint(&mut w).unwrap();
    let before = fs::read(temp.path().join(CURRENT)).unwrap();
    rewrite(&temp.path().join(PREVIOUS), b"external backup edit");
    tick(&mut w);
    assert!(matches!(
        store.checkpoint(&mut w),
        Err(Error::ConcurrentModification)
    ));
    assert!(store.requires_reopen());
    assert_eq!(fs::read(temp.path().join(CURRENT)).unwrap(), before);
    assert_eq!(
        fs::read(temp.path().join(PREVIOUS)).unwrap(),
        b"external backup edit"
    );
    drop(store);

    let other = Temp::new();
    let (store, _) = WorldStore::create(other.path(), &world()).unwrap();
    drop(store);
    rewrite(&other.path().join(TEMP), b"interrupted staging bytes");
    let (mut store, mut w) = open(&other.path()).unwrap();
    tick(&mut w);
    assert!(store.checkpoint(&mut w).is_err());
    assert_eq!(
        fs::read(other.path().join(TEMP)).unwrap(),
        b"interrupted staging bytes"
    );
    assert_eq!(w.state().tick, 1);
    assert!(w.last_tick().is_some());
}

#[test]
fn profile_directory_is_never_used_for_world_files() {
    let temp = Temp::new();
    let path = temp.0.join("p2p-profile");
    assert!(matches!(
        WorldStore::create(&path, &world()),
        Err(Error::Invalid(_))
    ));
    assert!(!path.exists());
}

#[test]
fn prepublication_failure_can_be_retried_without_rewriting_other_history() {
    let temp = Temp::new();
    let mut w = world();
    let (mut store, _) = WorldStore::create(temp.path(), &w).unwrap();
    tick(&mut w);
    store.checkpoint(&mut w).unwrap();
    tick(&mut w);
    store.storage.inject_fault(SaveFault::BeforeRename);
    assert!(matches!(store.checkpoint(&mut w), Err(Error::Io(_))));
    assert!(!store.requires_reopen());
    store.storage.clear_fault();
    store.checkpoint(&mut w).unwrap();
    drop(store);
    let (_, restored) = open(&temp.path()).unwrap();
    assert_eq!(restored.state().tick, 2);
}

#[test]
fn active_cast_quest_and_npc_state_restore_and_continue_identically() {
    let temp = Temp::new();
    let mut w = world();
    let (mut store, _) = WorldStore::create(temp.path(), &w).unwrap();
    let giver = w.npc(1).unwrap().id;
    command(
        &mut w,
        Command::AcceptQuest {
            quest: 910001,
            giver,
        },
    );
    command(
        &mut w,
        Command::Move {
            delta: [1000, 0, 0],
        },
    );
    command(
        &mut w,
        Command::Move {
            delta: [1000, 0, 0],
        },
    );
    let target = w.npc(2).unwrap().id;
    command(&mut w, Command::Melee { target });
    command(
        &mut w,
        Command::Cast {
            ability: 930001,
            target: benilla_world_runtime::Target::Npc { id: target },
        },
    );
    assert!(w.player(player()).unwrap().casting.is_some());
    assert!(!w.player(player()).unwrap().quests.is_empty());
    let expected = bytes(&w);
    let mut reference = World::restore(data(), Rules::original_camp(), &expected).unwrap();
    store.checkpoint(&mut w).unwrap();
    drop(store);
    let (mut store, mut w) = open(&temp.path()).unwrap();
    assert_eq!(bytes(&w), expected);
    for _ in 0..40 {
        tick(&mut w);
        tick(&mut reference);
    }
    assert!(w.player(player()).unwrap().casting.is_none());
    assert_eq!(bytes(&w), bytes(&reference));
    store.checkpoint(&mut w).unwrap();
}

#[cfg(windows)]
#[test]
fn windows_junction_path_is_rejected_without_touching_target() {
    let temp = Temp::new();
    let (store, _) = WorldStore::create(temp.path(), &world()).unwrap();
    drop(store);
    let before = fs::read(temp.path().join(CURRENT)).unwrap();
    let junction = temp.0.join("world-junction");
    let output = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&junction)
        .arg(temp.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "junction creation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(open(&junction).is_err());
    assert_eq!(fs::read(temp.path().join(CURRENT)).unwrap(), before);
    fs::remove_dir(junction).unwrap();
}

#[test]
fn closing_store_unlocks_even_with_an_inherited_file_description() {
    let temp = Temp::new();
    let (store, _) = WorldStore::create(temp.path(), &world()).unwrap();
    let inherited = store.storage.clone_lock();
    drop(store);
    let (_reopened, _) = open(&temp.path()).unwrap();
    drop(inherited);
}

#[test]
fn genuine_runtime_v1_checkpoint_is_rejected_without_migration_or_replacement() {
    let legacy = include_bytes!("../tests/fixtures/runtime-v1.checkpoint");
    let temp = Temp::new();
    let storage = Storage::acquire(&temp.path(), true).unwrap();
    storage.write(legacy, None).unwrap();
    drop(storage);
    assert!(matches!(open(&temp.path()), Err(Error::Runtime(_))));
    assert_eq!(fs::read(temp.path().join(CURRENT)).unwrap(), legacy);
    assert!(WorldStore::create(temp.path(), &world()).is_err());
    assert_eq!(fs::read(temp.path().join(CURRENT)).unwrap(), legacy);
    assert!(!temp.path().join(PREVIOUS).exists());
}

#[test]
fn v2_partial_loot_and_item_identity_survive_save_reopen_and_continuation() {
    let temp = Temp::new();
    let mut rules = Rules::original_camp();
    rules.melee_damage = 30;
    let mut w = World::new(data(), rules.clone(), [7; 16], 42, vec![player()]).unwrap();
    let identity = WorldIdentity::of(&w);
    let (mut store, _) = WorldStore::create(temp.path(), &w).unwrap();
    command(
        &mut w,
        Command::Move {
            delta: [1000, 0, 0],
        },
    );
    command(
        &mut w,
        Command::Move {
            delta: [1000, 0, 0],
        },
    );
    let target = w.npc(2).unwrap().id;
    command(&mut w, Command::Melee { target });
    assert_eq!(w.npc(2).unwrap().health, 0);
    command(&mut w, Command::OpenLoot { target });
    command(&mut w, Command::TakeLootMoney { target });
    assert_eq!(w.npc(2).unwrap().loot_money, 0);
    assert!(w.npc(2).unwrap().loot_items[0].is_some());
    assert_eq!(w.player(player()).unwrap().active_loot, Some(target));
    assert_eq!(w.player(player()).unwrap().money, 1);
    let partial = bytes(&w);
    store.checkpoint(&mut w).unwrap();
    drop(store);
    let (mut store, mut w) =
        WorldStore::open(temp.path(), identity.clone(), data(), rules.clone()).unwrap();
    assert_eq!(bytes(&w), partial);
    command(&mut w, Command::TakeLootItem { target, slot: 0 });
    let item = w.player(player()).unwrap().inventory[0].clone();
    let counter = w.player(player()).unwrap().next_item_instance;
    assert_eq!(item.instance, 1);
    assert_eq!(item.slot, 0);
    assert_eq!(item.count, 1);
    assert!(w.npc(2).unwrap().loot_items[0].is_none());
    command(&mut w, Command::TakeLootItem { target, slot: 0 });
    assert_eq!(w.player(player()).unwrap().inventory, vec![item.clone()]);
    store.checkpoint(&mut w).unwrap();
    drop(store);
    let (mut store, mut w) = WorldStore::open(temp.path(), identity, data(), rules).unwrap();
    assert_eq!(w.player(player()).unwrap().inventory, vec![item]);
    assert_eq!(w.player(player()).unwrap().next_item_instance, counter);
    assert_eq!(w.player(player()).unwrap().money, 1);
    let respawn = w.npc(2).unwrap().respawn_at.unwrap();
    while w.state().tick < respawn {
        tick(&mut w);
    }
    assert_eq!(w.npc(2).unwrap().id.generation, target.generation + 1);
    assert_eq!(w.player(player()).unwrap().next_item_instance, counter);
    store.checkpoint(&mut w).unwrap();
}
