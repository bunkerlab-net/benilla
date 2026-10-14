// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_world_runtime::native::synthetic_fixture::{self as f, SyntheticAuthority};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "benilla-native-store-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
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
fn tick(w: &mut NativeWorld) {
    w.step(w.state().tick + 1, None, &mut SyntheticAuthority::default())
        .unwrap();
}
fn identity() -> Identity {
    Identity::of(&f::world().checkpoint().unwrap())
}
fn open(p: &Path) -> Result<(NativeStore, NativeWorld), Error> {
    NativeStore::open(p, identity(), |c| {
        NativeWorld::restore_synthetic_component_test(f::profile(), c)
    })
}
#[test]
fn native_roundtrip_replays_cast_rng_and_retains_prior_good() {
    let t = Temp::new();
    let mut w = f::world();
    let (mut s, r) = NativeStore::create(t.path(), &w).unwrap();
    r.acknowledge(&mut w).unwrap();
    let input = f::cast(&w, 60001);
    w.step(1, Some(input), &mut SyntheticAuthority::default())
        .unwrap();
    for _ in 0..15 {
        tick(&mut w);
    }
    let expected = w.checkpoint().unwrap();
    let receipt = s.checkpoint(&mut w).unwrap();
    assert_eq!(receipt.generation(), 2);
    assert!(w.records().is_empty());
    drop(s);
    let (s, restored) = open(&t.path()).unwrap();
    assert_eq!(restored.checkpoint().unwrap(), expected);
    assert_eq!(s.generation(), 2);
    assert!(t.path().join("world.previous").exists());
}
#[test]
fn journal_continues_across_more_than_record_cap_with_checkpoint_cadence() {
    let t = Temp::new();
    let mut w = f::world();
    let (mut s, _) = NativeStore::create(t.path(), &w).unwrap();
    for i in 1..=4100 {
        tick(&mut w);
        if i % 100 == 0 {
            s.checkpoint(&mut w).unwrap();
        }
    }
    let expected = w.checkpoint().unwrap();
    drop(s);
    let (_, r) = open(&t.path()).unwrap();
    assert_eq!(r.checkpoint().unwrap(), expected);
}
#[test]
fn stale_receipt_cannot_clear_new_ticks_and_unacknowledged_suffix_is_saved() {
    let t = Temp::new();
    let mut w = f::world();
    let (mut s, _) = NativeStore::create(t.path(), &w).unwrap();
    tick(&mut w);
    let p = s.prepare(&w).unwrap();
    let r = s.commit(p).unwrap();
    tick(&mut w);
    assert!(r.acknowledge(&mut w).is_err());
    assert_eq!(w.records().len(), 2);
    s.checkpoint(&mut w).unwrap();
    let expected = w.checkpoint().unwrap();
    drop(s);
    assert_eq!(open(&t.path()).unwrap().1.checkpoint().unwrap(), expected);
}
#[test]
fn stale_preparation_and_wrong_owner_are_rejected() {
    let t = Temp::new();
    let mut w = f::world();
    let (mut s, _) = NativeStore::create(t.path(), &w).unwrap();
    tick(&mut w);
    let first = s.prepare(&w).unwrap();
    let stale = s.prepare(&w).unwrap();
    s.commit(first).unwrap();
    assert!(matches!(
        s.commit(stale),
        Err(Error::Store(super::super::Error::StalePrepared))
    ));
    drop(s);
    let mut wrong = identity();
    wrong.player.owner = [9; 16];
    assert!(NativeStore::open(t.path(), wrong, |c| {
        NativeWorld::restore_synthetic_component_test(f::profile(), c)
    })
    .is_err());
}
#[test]
fn corrupt_current_never_falls_back_and_restore_cannot_substitute_world() {
    let t = Temp::new();
    let mut w = f::world();
    let (mut s, _) = NativeStore::create(t.path(), &w).unwrap();
    tick(&mut w);
    s.checkpoint(&mut w).unwrap();
    tick(&mut w);
    s.checkpoint(&mut w).unwrap();
    drop(s);
    assert!(NativeStore::open(t.path(), identity(), |_| Ok(f::world())).is_err());
    fs::write(t.path().join("world.checkpoint"), b"broken").unwrap();
    assert!(open(&t.path()).is_err());
    assert!(t.path().join("world.previous").exists());
}
#[test]
fn original_and_native_envelopes_are_not_interchangeable() {
    let t = Temp::new();
    let w = f::world();
    let (s, _) = NativeStore::create(t.path(), &w).unwrap();
    let bytes = fs::read(t.path().join("world.checkpoint")).unwrap();
    assert!(super::super::Envelope::decode(&bytes).is_err());
    drop(s);
    let data = benilla_world_data::load_json(
        include_bytes!("../../../benilla-world-data/tests/fixtures/original-camp.json"),
        benilla_world_data::Limits::default(),
    )
    .unwrap();
    let original = benilla_world_runtime::World::new(
        data,
        benilla_world_runtime::Rules::original_camp(),
        [8; 16],
        1234,
        vec![f::player()],
    )
    .unwrap();
    let t2 = Temp::new();
    let (s, _) = super::super::WorldStore::create(t2.path(), &original).unwrap();
    drop(s);
    assert!(open(&t2.path()).is_err());
}
#[test]
fn publication_failure_retains_journal_and_uncertainty_latches() {
    let t = Temp::new();
    let mut w = f::world();
    let (mut s, _) = NativeStore::create(t.path(), &w).unwrap();
    tick(&mut w);
    s.storage
        .inject_fault(super::super::storage::SaveFault::AfterRename);
    assert!(s.checkpoint(&mut w).is_err());
    assert!(!w.records().is_empty());
    assert!(s.requires_reopen());
    assert!(s.prepare(&w).is_err());
    drop(s);
    let (_, restored) = open(&t.path()).unwrap();
    assert_eq!(restored.checkpoint().unwrap(), w.checkpoint().unwrap());
}
#[test]
fn explicit_create_never_replaces_and_lock_is_exclusive() {
    let t = Temp::new();
    let w = f::world();
    let (s, _) = NativeStore::create(t.path(), &w).unwrap();
    assert!(open(&t.path()).is_err());
    drop(s);
    assert!(NativeStore::create(t.path(), &w).is_err());
}

#[test]
fn resigned_journal_tamper_and_wrong_restore_are_rejected() {
    let t = Temp::new();
    let mut w = f::world();
    let (mut s, _) = NativeStore::create(t.path(), &w).unwrap();
    tick(&mut w);
    s.checkpoint(&mut w).unwrap();
    drop(s);
    let path = t.path().join("world.checkpoint");
    let mut e = Envelope::decode(&fs::read(&path).unwrap()).unwrap();
    e.records[0].after = [99; 32];
    fs::write(&path, e.encode().unwrap()).unwrap();
    assert!(open(&t.path()).is_err());
}
#[test]
fn failed_prepublication_save_can_retry_without_clearing_records() {
    let t = Temp::new();
    let mut w = f::world();
    let (mut s, _) = NativeStore::create(t.path(), &w).unwrap();
    tick(&mut w);
    s.storage
        .inject_fault(super::super::storage::SaveFault::BeforeRename);
    assert!(s.checkpoint(&mut w).is_err());
    assert!(!s.requires_reopen());
    assert_eq!(w.records().len(), 1);
    s.storage.clear_fault();
    s.checkpoint(&mut w).unwrap();
    assert!(w.records().is_empty());
}

#[test]
fn resigned_checkpoint_cannot_fork_from_a_different_base_with_correct_parent_hash() {
    let t = Temp::new();
    let mut w = f::world();
    let (mut s, _) = NativeStore::create(t.path(), &w).unwrap();
    tick(&mut w);
    s.checkpoint(&mut w).unwrap();
    tick(&mut w);
    s.checkpoint(&mut w).unwrap();
    drop(s);
    let path = t.path().join("world.checkpoint");
    let mut e = Envelope::decode(&fs::read(&path).unwrap()).unwrap();
    // Preserve correct generation and parent-file hash but silently rewind the admitted state.
    e.base = f::world().checkpoint().unwrap();
    e.records.clear();
    fs::write(path, e.encode().unwrap()).unwrap();
    assert!(matches!(
        open(&t.path()),
        Err(Error::Store(super::super::Error::DivergentHistory))
    ));
}

#[test]
fn all_uncertain_native_publication_points_reopen_only_verified_history() {
    use super::super::storage::SaveFault;
    for fault in [
        SaveFault::AfterPreviousRename,
        SaveFault::AfterRename,
        SaveFault::BeforeVerification,
    ] {
        let t = Temp::new();
        let mut w = f::world();
        let old = w.checkpoint().unwrap();
        let (mut s, _) = NativeStore::create(t.path(), &w).unwrap();
        tick(&mut w);
        let new = w.checkpoint().unwrap();
        s.storage.inject_fault(fault);
        assert!(s.checkpoint(&mut w).is_err());
        assert!(s.requires_reopen());
        assert_eq!(w.records().len(), 1);
        drop(s);
        let (_, restored) = open(&t.path()).unwrap();
        let restored = restored.checkpoint().unwrap();
        if matches!(fault, SaveFault::AfterPreviousRename) {
            assert_eq!(restored, old);
        } else {
            assert_eq!(restored, new);
        }
    }
}
#[test]
fn backup_first_duplicate_checkpoint_is_valid_after_prepublication_failure() {
    let t = Temp::new();
    let mut w = f::world();
    let old = w.checkpoint().unwrap();
    let (mut s, _) = NativeStore::create(t.path(), &w).unwrap();
    tick(&mut w);
    s.storage
        .inject_fault(super::super::storage::SaveFault::BeforeRename);
    assert!(s.checkpoint(&mut w).is_err());
    drop(s);
    assert_eq!(
        fs::read(t.path().join("world.checkpoint")).unwrap(),
        fs::read(t.path().join("world.previous")).unwrap()
    );
    assert_eq!(open(&t.path()).unwrap().1.checkpoint().unwrap(), old);
}
#[test]
fn external_current_or_previous_edits_latch_without_erasing_runtime_history() {
    for file in ["world.checkpoint", "world.previous"] {
        let t = Temp::new();
        let mut w = f::world();
        let (mut s, _) = NativeStore::create(t.path(), &w).unwrap();
        tick(&mut w);
        s.checkpoint(&mut w).unwrap();
        tick(&mut w);
        fs::write(t.path().join(file), b"external edit").unwrap();
        assert!(s.checkpoint(&mut w).is_err());
        assert!(s.requires_reopen());
        assert_eq!(w.records().len(), 1);
    }
}
#[test]
fn same_state_different_history_cannot_use_receipt_or_prepare() {
    let t = Temp::new();
    let mut a = f::world();
    let mut b = a.clone();
    let (mut s, _) = NativeStore::create(t.path(), &a).unwrap();
    let ca = f::cast(&a, 60002);
    let cb = f::cast(&b, 99999);
    a.step(1, Some(ca), &mut SyntheticAuthority::default())
        .unwrap();
    b.step(1, Some(cb), &mut SyntheticAuthority::default())
        .unwrap();
    assert_eq!(a.state_digest(), b.state_digest());
    assert_ne!(a.checkpoint().unwrap(), b.checkpoint().unwrap());
    let receipt = s.checkpoint(&mut a).unwrap();
    assert!(receipt.acknowledge(&mut b).is_err());
    assert!(s.prepare(&b).is_err());
    assert_eq!(b.records().len(), 1);
}

#[test]
fn generation_one_requires_actual_genesis_even_with_valid_checksum() {
    let t = Temp::new();
    let mut w = f::world();
    let (mut s, _) = NativeStore::create(t.path(), &w).unwrap();
    tick(&mut w);
    s.checkpoint(&mut w).unwrap();
    drop(s);
    let path = t.path().join("world.checkpoint");
    let mut bytes = fs::read(&path).unwrap();
    bytes[12..20].copy_from_slice(&1u64.to_le_bytes());
    bytes[20..52].fill(0);
    let end = bytes.len() - 32;
    let digest = checksum(&bytes[..end]);
    bytes[end..].copy_from_slice(&digest);
    assert!(Envelope::decode(&bytes).is_err());
}

#[test]
fn prior_native_formats_current_or_previous_are_preserved_on_refusal() {
    for (version, policy) in [
        (12, "benilla-native-current-pose-facing-v12"),
        (13, "benilla-native-npc-events-v13"),
    ] {
        for incompatible in ["world.checkpoint", "world.previous"] {
            let t = Temp::new();
            let mut w = f::world();
            let (mut store, _) = NativeStore::create(t.path(), &w).unwrap();
            tick(&mut w);
            store.checkpoint(&mut w).unwrap();
            drop(store);
            let path = t.path().join(incompatible);
            let mut envelope = Envelope::decode(&fs::read(&path).unwrap()).unwrap();
            let mut old = serde_json::to_value(&envelope.base).unwrap();
            old["version"] = version.into();
            old["state"]["version"] = version.into();
            old["state"]["policy"] = policy.into();
            envelope.base = serde_json::from_value(old).unwrap();
            fs::write(&path, envelope.encode().unwrap()).unwrap();
            let current = fs::read(t.path().join("world.checkpoint")).unwrap();
            let previous = fs::read(t.path().join("world.previous")).unwrap();
            assert!(open(&t.path()).is_err());
            assert!(NativeStore::create(t.path(), &f::world()).is_err());
            assert_eq!(
                fs::read(t.path().join("world.checkpoint")).unwrap(),
                current
            );
            assert_eq!(fs::read(t.path().join("world.previous")).unwrap(), previous);
        }
    }
}

#[test]
fn native_npc_queue_pending_notify_is_durable_and_replays_exactly() {
    use benilla_world_runtime::native::synthetic_motion_fixture as motion;
    let t = Temp::new();
    let mut world = motion::world();
    let profile = world.profile().clone();
    let (mut store, _) = NativeStore::create(t.path(), &world).unwrap();
    let mut authority = motion::SyntheticMotionAuthority {
        visits: Some(false),
        ..Default::default()
    };
    world.step(1, None, &mut authority).unwrap();
    let expected = world.state().npcs[&2].events.clone();
    let identity = Identity::of(&world.checkpoint().unwrap());
    store.checkpoint(&mut world).unwrap();
    drop(store);
    let (_, mut restored) = NativeStore::open(t.path(), identity, |checkpoint| {
        NativeWorld::restore_synthetic_component_test(profile.clone(), checkpoint)
    })
    .unwrap();
    assert_eq!(restored.state().npcs[&2].events, expected);
    assert!(expected.notify_handle().is_some());
    restored.step(2, None, &mut authority).unwrap();
    world.step(2, None, &mut authority).unwrap();
    assert_eq!(restored.last_tick(), world.last_tick());
    assert_eq!(restored.state_digest(), world.state_digest());
    assert!(restored.state().npcs[&2].events.notify_handle().is_none());
}
