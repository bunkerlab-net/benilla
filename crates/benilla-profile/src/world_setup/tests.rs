use super::*;
use crate::{Appearance, NewCharacter};
struct Temp(std::path::PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn directory() -> (Temp, std::path::PathBuf) {
    let nonce = new_request_session().unwrap();
    let suffix: String = nonce.iter().map(|b| format!("{b:02x}")).collect();
    let path = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("benilla-world-setup-fixture-{suffix}"));
    fs::create_dir(&path).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    (Temp(path.clone()), path.join("profile"))
}
fn input(name: &str) -> NewCharacter {
    NewCharacter {
        name: name.into(),
        race: 1,
        class: 1,
        gender: 0,
        appearance: Appearance::default(),
    }
}
use std::fs;
fn request(sequence: u64, revision: u64, operation: SetupOperation) -> SetupRequest {
    SetupRequest {
        id: SetupRequestId {
            session: [41; 16],
            sequence,
        },
        expected_revision: revision,
        operation,
    }
}
fn create(sequence: u64, revision: u64) -> SetupRequest {
    request(
        sequence,
        revision,
        SetupOperation::Create {
            label: "Fixture world".into(),
            allowlist: [[19; 16]].into(),
        },
    )
}
#[test]
fn separate_sidecar_preserves_profile_bytes_and_reopens_signed_selection() {
    let (_temp, path) = directory();
    let mut p = ProfileStore::create(&path).unwrap();
    p.create_character(input("Alice")).unwrap();
    let before = fs::read(path.join("profile.json")).unwrap();
    let previous = fs::read(path.join("profile.previous.json")).unwrap();
    assert_eq!(p.world_setups().unwrap(), SetupSnapshot::default());
    assert!(!path.join("local-worlds.json").exists());
    let a = p.apply_world_setup(create(1, 0)).unwrap();
    let first = p.world_setups().unwrap().worlds[0].clone();
    first.verify().unwrap();
    assert_eq!(first.policy.world_owner, p.owner_id());
    assert_eq!(first.policy.revision, 1);
    let b = p.apply_world_setup(create(2, 1)).unwrap();
    assert_ne!(a, b);
    p.apply_world_setup(request(3, 2, SetupOperation::Select { world: a }))
        .unwrap();
    assert_eq!(
        p.world_setups().unwrap().worlds[0].digest().unwrap(),
        first.digest().unwrap()
    );
    assert_eq!(fs::read(path.join("profile.json")).unwrap(), before);
    assert_eq!(
        fs::read(path.join("profile.previous.json")).unwrap(),
        previous
    );
    drop(p);
    let mut p = ProfileStore::open(&path).unwrap();
    let saved = p.world_setups().unwrap();
    assert_eq!(saved.selected, Some(a));
    assert_eq!(saved.revision, 3);
    assert_eq!(saved.worlds.len(), 2);
    assert_eq!(saved.worlds[0], first);
}
#[test]
fn exact_replay_returns_same_world_and_conflicts_stale_or_foreign_requests_fail() {
    let (_temp, path) = directory();
    let mut p = ProfileStore::create(&path).unwrap();
    let req = create(1, 0);
    let world = p.apply_world_setup(req.clone()).unwrap();
    let bytes = fs::read(path.join("local-worlds.json")).unwrap();
    assert_eq!(p.apply_world_setup(req).unwrap(), world);
    assert!(p
        .apply_world_setup(request(1, 0, SetupOperation::Select { world }))
        .is_err());
    assert!(matches!(
        p.apply_world_setup(create(2, 0)),
        Err(Error::ConcurrentModification)
    ));
    assert!(p
        .apply_world_setup(request(2, 1, SetupOperation::Select { world: [99; 16] }))
        .is_err());
    assert_eq!(fs::read(path.join("local-worlds.json")).unwrap(), bytes);
}
#[test]
fn declaration_tampering_wrong_owner_and_transport_domain_fail() {
    let (_temp, path) = directory();
    let mut p = ProfileStore::create(&path).unwrap();
    p.apply_world_setup(create(1, 0)).unwrap();
    let world = p.world_setups().unwrap().worlds[0].clone();
    let mut tampered = world.clone();
    tampered.label = "Changed".into();
    assert!(tampered.verify().is_err());
    let mut tampered = world.clone();
    tampered.policy.world = [88; 16];
    assert!(tampered.verify().is_err());
    let mut tampered = world.clone();
    tampered.policy.allowlist.insert([7; 16]);
    assert!(tampered.verify().is_err());
    let mut tampered = world.clone();
    tampered.policy.world_owner = [7; 16];
    assert!(tampered.verify().is_err());
    let mut tampered = world.clone();
    tampered.policy.revision = 2;
    assert!(tampered.verify().is_err());
    let mut tampered = world.clone();
    tampered.signature = p
        .identity()
        .sign_transport_challenge(&world.digest().unwrap())
        .to_vec();
    assert!(tampered.verify().is_err());
    let sig: [u8; 64] = world.signature.try_into().unwrap();
    assert!(!crate::verify_transport_challenge(
        &p.public_key(),
        &[0; 32],
        &sig
    ));
}
#[test]
fn sidecar_cannot_be_copied_to_another_profile_or_silently_recovered() {
    let (_temp, path) = directory();
    let mut p = ProfileStore::create(&path).unwrap();
    p.apply_world_setup(create(1, 0)).unwrap();
    let (_other, path2) = directory();
    let mut other = ProfileStore::create(&path2).unwrap();
    fs::copy(
        path.join("local-worlds.json"),
        path2.join("local-worlds.json"),
    )
    .unwrap();
    assert!(matches!(other.world_setups(), Err(Error::Corrupt)));
    p.apply_world_setup(create(2, 1)).unwrap();
    fs::remove_file(path.join("local-worlds.json")).unwrap();
    assert!(matches!(p.world_setups(), Err(Error::Corrupt)));
    assert!(p.apply_world_setup(create(3, 0)).is_err());
    assert!(path.join("local-worlds.previous.json").exists());
}
#[test]
fn invalid_and_oversize_inputs_do_not_create_setup_state() {
    let (_temp, path) = directory();
    let mut p = ProfileStore::create(&path).unwrap();
    for (label, allowlist) in [
        ("".into(), BTreeSet::new()),
        ("x".repeat(41), BTreeSet::new()),
        ("bad/name".into(), BTreeSet::new()),
        ("World".into(), [[0; 16]].into()),
        ("World".into(), [p.owner_id()].into()),
        ("World".into(), (1..=65).map(|x| [x; 16]).collect()),
    ] {
        assert!(p
            .apply_world_setup(request(1, 0, SetupOperation::Create { label, allowlist }))
            .is_err());
    }
    let mut req = create(1, 0);
    req.id.session = [0; 16];
    assert!(p.apply_world_setup(req).is_err());
    assert!(!path.join("local-worlds.json").exists());
}
#[test]
fn world_limit_and_selection_validation_preserve_prior_declarations() {
    let (_temp, path) = directory();
    let mut p = ProfileStore::create(&path).unwrap();
    for i in 0..MAX_WORLDS as u64 {
        p.apply_world_setup(create(i + 1, i)).unwrap();
    }
    let before = p.world_setups().unwrap();
    assert!(p.apply_world_setup(create(17, 16)).is_err());
    assert_eq!(p.world_setups().unwrap(), before);
    p.apply_world_setup(request(
        18,
        16,
        SetupOperation::Select {
            world: before.worlds[0].policy.world,
        },
    ))
    .unwrap();
    assert_eq!(p.world_setups().unwrap().worlds, before.worlds);
}
#[test]
fn uncertain_commit_latches_both_setup_and_profile_until_reopen() {
    let (_temp, path) = directory();
    let mut p = ProfileStore::create(&path).unwrap();
    let character = p.create_character(input("Alice")).unwrap();
    p.storage
        .inject_fault(crate::storage::SaveFault::AfterRename);
    let req = create(1, 0);
    assert!(matches!(
        p.apply_world_setup(req.clone()),
        Err(Error::CommitUncertain)
    ));
    assert!(matches!(p.world_setups(), Err(Error::CommitUncertain)));
    assert!(matches!(
        p.create_character(input("Beryl")),
        Err(Error::CommitUncertain)
    ));
    assert_eq!(p.character(character).unwrap().name, "Alice");
    drop(p);
    let mut p = ProfileStore::open(&path).unwrap();
    let world = p.world_setups().unwrap().selected.unwrap();
    assert_eq!(p.apply_world_setup(req).unwrap(), world);
    assert_eq!(p.world_setups().unwrap().worlds.len(), 1);
}
#[test]
fn failed_precommit_preserves_previous_selection_and_cas_rejects_changed_sidecar() {
    let (_temp, path) = directory();
    let mut p = ProfileStore::create(&path).unwrap();
    p.apply_world_setup(create(1, 0)).unwrap();
    let before = p.world_setups().unwrap();
    p.storage
        .inject_fault(crate::storage::SaveFault::BeforeRename);
    assert!(p.apply_world_setup(create(2, 1)).is_err());
    assert_eq!(p.world_setups().unwrap(), before);
    let bytes = fs::read(path.join("local-worlds.json")).unwrap();
    assert!(matches!(
        p.storage.write_world_setups(&bytes, Some([9; 32])),
        Err(Error::ConcurrentModification)
    ));
}
#[test]
fn changed_profile_identity_prevents_setup_signing() {
    let (_temp, path) = directory();
    let mut p = ProfileStore::create(&path).unwrap();
    fs::write(path.join("profile.json"), b"changed").unwrap();
    assert!(matches!(
        p.apply_world_setup(create(1, 0)),
        Err(Error::ConcurrentModification)
    ));
    assert!(!path.join("local-worlds.json").exists());
}
#[cfg(unix)]
#[test]
fn symlink_hardlink_and_permissions_are_rejected_without_touching_targets() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let (_temp, path) = directory();
    let mut p = ProfileStore::create(&path).unwrap();
    let target = path.join("target");
    fs::write(&target, b"untouched").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    let current = path.join("local-worlds.json");
    symlink(&target, &current).unwrap();
    assert!(p.apply_world_setup(create(1, 0)).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"untouched");
    fs::remove_file(&current).unwrap();
    fs::hard_link(&target, &current).unwrap();
    assert!(p.world_setups().is_err());
    fs::remove_file(&current).unwrap();
    p.apply_world_setup(create(1, 0)).unwrap();
    fs::set_permissions(&current, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(p.world_setups().is_err());
}

#[test]
fn tampered_signed_selection_and_oversize_sidecar_are_preserved_and_rejected() {
    let (_temp, path) = directory();
    let mut p = ProfileStore::create(&path).unwrap();
    p.apply_world_setup(create(1, 0)).unwrap();
    let current = path.join("local-worlds.json");
    let bytes = fs::read(&current).unwrap();
    let mut envelope: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    envelope["state"]["snapshot"]["selected"] = serde_json::Value::Null;
    fs::write(&current, serde_json::to_vec(&envelope).unwrap()).unwrap();
    assert!(matches!(p.world_setups(), Err(Error::Corrupt)));
    assert!(p.apply_world_setup(create(2, 1)).is_err());
    fs::write(&current, vec![b' '; MAX_SETUP_BYTES as usize + 1]).unwrap();
    assert!(matches!(p.world_setups(), Err(Error::Corrupt)));
    assert_eq!(fs::metadata(current).unwrap().len(), MAX_SETUP_BYTES + 1);
}

#[test]
fn full_replay_history_is_never_evicted_and_exact_replay_still_succeeds() {
    let (_temp, path) = directory();
    let mut p = ProfileStore::create(&path).unwrap();
    let first = create(1, 0);
    let world = p.apply_world_setup(first.clone()).unwrap();
    let current = path.join("local-worlds.json");
    let mut envelope: Envelope = serde_json::from_slice(&fs::read(&current).unwrap()).unwrap();
    for i in 1..MAX_SETUP_RECEIPTS as u64 {
        envelope.state.receipts.push(Receipt {
            request: request(i + 1, i, SetupOperation::Select { world }),
            world,
        });
    }
    envelope.state.snapshot.revision = MAX_SETUP_RECEIPTS as u64;
    envelope.signature = p
        .identity
        .key
        .sign(&envelope.message().unwrap())
        .to_bytes()
        .to_vec();
    let full = serde_json::to_vec(&envelope).unwrap();
    fs::write(&current, &full).unwrap();
    assert_eq!(
        p.world_setups().unwrap().revision,
        MAX_SETUP_RECEIPTS as u64
    );
    assert_eq!(p.apply_world_setup(first).unwrap(), world);
    assert!(p.apply_world_setup(create(257, 256)).is_err());
    assert_eq!(fs::read(current).unwrap(), full);
}
