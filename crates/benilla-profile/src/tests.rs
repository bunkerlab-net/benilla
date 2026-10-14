use super::*;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) struct TempDir(std::path::PathBuf);
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) fn input(name: &str) -> NewCharacter {
    NewCharacter {
        name: name.into(),
        race: 1,
        class: 1,
        gender: 0,
        appearance: Appearance::default(),
    }
}
pub(crate) fn directory() -> (TempDir, std::path::PathBuf) {
    let base = std::env::temp_dir().canonicalize().unwrap();
    let path = base.join(format!(
        "benilla-profile-fixture-{}-{}",
        std::process::id(),
        FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    #[allow(unused_mut)] // Unix configures the private creation mode below.
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&path).unwrap();
    let temp = TempDir(path.clone());
    let path = path.join("profile");
    (temp, path)
}
#[test]
fn create_select_save_reopen_preserves_identity_and_characters() {
    let (_temp, path) = directory();
    let mut profile = ProfileStore::create(&path).unwrap();
    let key = profile.public_key();
    let owner = profile.owner_id();
    assert!(profile.characters().is_empty());
    let first = profile.create_character(input("aLICE")).unwrap();
    let second = profile
        .create_character(NewCharacter {
            name: "Beryl".into(),
            race: 6,
            class: 11,
            gender: 1,
            appearance: Appearance::default(),
        })
        .unwrap();
    assert_ne!(first, second);
    assert_eq!(profile.selected_character().unwrap().id, second);
    profile.select_character(first).unwrap();
    let mut position = profile.character(first).unwrap().position;
    position.position[0] += 3.5;
    profile.save_position(first, position).unwrap();
    let character = profile.character(first).unwrap().clone();
    assert_eq!(character.name, "Alice");
    assert_eq!(character.owner_id, owner);
    assert_eq!(character.level, 1);
    drop(profile);
    let profile = ProfileStore::open(&path).unwrap();
    assert_eq!(profile.public_key(), key);
    assert_eq!(profile.owner_id(), owner);
    assert_eq!(profile.selected_character(), Some(&character));
    assert_eq!(profile.characters().len(), 2);
    assert_eq!(
        profile.character(second).unwrap().spawn_policy,
        SpawnPolicy::NorthshireMeetingV1
    );
}
#[cfg(windows)]
#[test]
fn windows_verbatim_drive_creation_reopens_through_standard_path() {
    use std::path::{Component, PathBuf, Prefix};

    // canonicalize returns extended-length syntax; retain it to exercise the prefix walk.
    let (_temp, path) = directory();
    let mut parts = path.components();
    let Some(Component::Prefix(prefix)) = parts.next() else {
        panic!("the Windows fixture must have a path prefix");
    };
    let Prefix::VerbatimDisk(drive) = prefix.kind() else {
        panic!("the Windows fixture must use a local drive");
    };
    let mut standard = PathBuf::from(format!("{}:", char::from(drive)));
    for part in parts {
        standard.push(part.as_os_str());
    }
    let mut profile = ProfileStore::create(&path).unwrap();
    let key = profile.public_key();
    let id = profile.create_character(input("Alice")).unwrap();
    assert!(path.join("profile.previous.json").is_file());
    assert!(matches!(ProfileStore::open(&standard), Err(Error::Locked)));
    drop(profile);
    let profile = ProfileStore::open(&standard).unwrap();
    assert_eq!(profile.public_key(), key);
    assert_eq!(profile.selected_character().unwrap().id, id);
}
#[test]
fn signatures_bind_owner_domain_and_challenge() {
    let (_temp, path) = directory();
    let profile = ProfileStore::create(path).unwrap();
    let identity = profile.identity();
    let challenge = [23; 32];
    let signature = identity.sign_transport_challenge(&challenge);
    assert!(verify_transport_challenge(
        &identity.public_key(),
        &challenge,
        &signature
    ));
    assert!(!verify_transport_challenge(
        &identity.public_key(),
        &[24; 32],
        &signature
    ));
    assert!(!verify_transport_challenge(
        &[0; 32], &challenge, &signature
    ));
    let verifier = VerifyingKey::from_bytes(&identity.public_key()).unwrap();
    assert!(verifier
        .verify_strict(&challenge, &Signature::from_bytes(&signature))
        .is_err());
    // Shared transport/profile parity vector; fictional fixture only, never persisted.
    let fixture = ProfileIdentity {
        key: SigningKey::from_bytes(&[1; 32]),
    };
    assert_eq!(
        fixture.owner_id(),
        [
            0xa2, 0x43, 0x02, 0xea, 0xb0, 0xac, 0x44, 0xa2, 0xb8, 0x8e, 0x29, 0xbb, 0xac, 0xa5,
            0x5e, 0x83
        ]
    );
    drop(profile);
    assert!(verify_transport_challenge(
        &identity.public_key(),
        &challenge,
        &identity.sign_transport_challenge(&challenge)
    ));
}
#[test]
fn invalid_creation_and_position_do_not_change_valid_save() {
    let (_temp, path) = directory();
    let mut profile = ProfileStore::create(&path).unwrap();
    let id = profile.create_character(input("Alice")).unwrap();
    let before = fs::read(path.join("profile.json")).unwrap();
    for name in [
        "a",
        "abcdefghijklmn",
        "A/lice",
        "AL Ice",
        "éclair",
        "\nAlice",
    ] {
        assert!(profile.create_character(input(name)).is_err());
    }
    assert!(profile.create_character(input("ALICE")).is_err());
    for (race, class) in [(0, 1), (10, 2), (1, 7), (4, 8), (7, 5), (6, 4)] {
        let mut candidate = input("Beryl");
        candidate.race = race;
        candidate.class = class;
        assert!(profile.create_character(candidate).is_err());
    }
    let mut candidate = input("Beryl");
    candidate.gender = 2;
    assert!(profile.create_character(candidate).is_err());
    let mut candidate = input("Beryl");
    candidate.appearance.skin = 16;
    assert!(profile.create_character(candidate).is_err());
    let original = profile.character(id).unwrap().position;
    for invalid in [f32::NAN, f32::INFINITY, -20_001.0] {
        let mut position = original;
        position.position[1] = invalid;
        assert!(profile.save_position(id, position).is_err());
    }
    let mut position = original;
    position.map_id = 2;
    assert!(profile.save_position(id, position).is_err());
    let mut position = original;
    position.orientation = 101.0;
    assert!(profile.save_position(id, position).is_err());
    let mut position = original;
    position.zone_id = 10_001;
    assert!(profile.save_position(id, position).is_err());
    assert!(profile.select_character([0; 16]).is_err());
    assert!(profile.save_position([0; 16], original).is_err());
    assert_eq!(fs::read(path.join("profile.json")).unwrap(), before);
}
#[test]
fn classic_race_class_matrix_and_limits() {
    assert_eq!(
        (1..=8).map(|race| valid_classes(race).len()).sum::<usize>(),
        40
    );
    for race in 1..=8 {
        for class in valid_classes(race) {
            validate_race_class(race, *class).unwrap();
        }
    }
    let (_temp, path) = directory();
    let mut profile = ProfileStore::create(path).unwrap();
    for name in ["Aa", "Ab", "Ac", "Ad", "Ae", "Af", "Ag", "Ah", "Ai", "Aj"] {
        profile.create_character(input(name)).unwrap();
    }
    assert!(profile.create_character(input("Ak")).is_err());
    assert_eq!(profile.characters().len(), MAX_CHARACTERS);
}
#[test]
fn opening_missing_never_creates_and_create_never_replaces() {
    let (_temp, path) = directory();
    assert!(ProfileStore::open(&path).is_err());
    assert!(!path.exists());
    let profile = ProfileStore::create(&path).unwrap();
    let key = profile.public_key();
    drop(profile);
    assert!(matches!(
        ProfileStore::create(&path),
        Err(Error::AlreadyExists)
    ));
    assert_eq!(ProfileStore::open(&path).unwrap().public_key(), key);
}
#[test]
fn exclusive_session_lock_released_on_drop() {
    let (_temp, path) = directory();
    let first = ProfileStore::create(&path).unwrap();
    assert!(matches!(ProfileStore::open(&path), Err(Error::Locked)));
    assert!(matches!(ProfileStore::create(&path), Err(Error::Locked)));
    drop(first);
    assert!(ProfileStore::open(&path).is_ok());
}
#[test]
fn corruption_fails_closed_retains_previous_and_never_regenerates() {
    let (_temp, path) = directory();
    let mut profile = ProfileStore::create(&path).unwrap();
    profile.create_character(input("Alice")).unwrap();
    let previous = fs::read(path.join("profile.previous.json")).unwrap();
    drop(profile);
    fs::write(path.join("profile.json"), b"{broken").unwrap();
    assert!(matches!(ProfileStore::open(&path), Err(Error::Corrupt)));
    assert_eq!(fs::read(path.join("profile.json")).unwrap(), b"{broken");
    assert_eq!(
        fs::read(path.join("profile.previous.json")).unwrap(),
        previous
    );
    assert!(ProfileStore::create(&path).is_err());
}
#[test]
fn schema_size_checksum_and_owner_checks_fail_closed() {
    let (_temp, path) = directory();
    let mut profile = ProfileStore::create(&path).unwrap();
    profile.create_character(input("Alice")).unwrap();
    let original = fs::read(path.join("profile.json")).unwrap();
    drop(profile);
    for mutation in [
        "unknown",
        "version",
        "checksum",
        "owner",
        "level",
        "duplicate",
        "selection",
    ] {
        let mut value: serde_json::Value = serde_json::from_slice(&original).unwrap();
        match mutation {
            "unknown" => value["unexpected"] = true.into(),
            "version" => value["format_version"] = 99.into(),
            "checksum" => value["checksum"][0] = 256.into(),
            "owner" => value["payload"]["owner_id"][0] = 256.into(),
            "level" => value["payload"]["state"]["characters"][0]["level"] = 60.into(),
            "duplicate" => {
                let cloned = value["payload"]["state"]["characters"][0].clone();
                value["payload"]["state"]["characters"]
                    .as_array_mut()
                    .unwrap()
                    .push(cloned);
            }
            "selection" => {
                value["payload"]["state"]["selected"] = serde_json::to_value([0; 16]).unwrap()
            }
            _ => unreachable!(),
        }
        fs::write(
            path.join("profile.json"),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
        assert!(ProfileStore::open(&path).is_err(), "{mutation}");
    }
    fs::write(
        path.join("profile.json"),
        vec![b' '; MAX_FILE_BYTES as usize + 1],
    )
    .unwrap();
    assert!(matches!(ProfileStore::open(&path), Err(Error::Corrupt)));
}
#[test]
fn externally_modified_file_is_not_overwritten() {
    let (_temp, path) = directory();
    let mut profile = ProfileStore::create(&path).unwrap();
    profile.create_character(input("Alice")).unwrap();
    fs::write(path.join("profile.json"), b"external change").unwrap();
    assert!(matches!(
        profile.create_character(input("Beryl")),
        Err(Error::ConcurrentModification)
    ));
    assert_eq!(profile.characters().len(), 1);
    assert_eq!(
        fs::read(path.join("profile.json")).unwrap(),
        b"external change"
    );
}
#[test]
fn failed_save_retains_memory_and_current() {
    let (_temp, path) = directory();
    let mut profile = ProfileStore::create(&path).unwrap();
    profile.create_character(input("Alice")).unwrap();
    let current = fs::read(path.join("profile.json")).unwrap();
    fs::remove_file(path.join("profile.previous.json")).unwrap();
    fs::create_dir(path.join("profile.previous.json")).unwrap();
    assert!(profile.create_character(input("Beryl")).is_err());
    assert_eq!(profile.characters().len(), 1);
    assert_eq!(fs::read(path.join("profile.json")).unwrap(), current);
    assert_eq!(
        fs::read_dir(&path)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count(),
        0
    );
}
#[test]
fn public_canonical_character_contains_no_secret_and_roundtrips() {
    let (_temp, path) = directory();
    let mut profile = ProfileStore::create(path).unwrap();
    profile.create_character(input("Alice")).unwrap();
    let character = profile.selected_character().unwrap();
    let bytes = character.canonical_bytes().unwrap();
    assert_eq!(bytes, character.canonical_bytes().unwrap());
    assert!(!String::from_utf8(bytes.clone()).unwrap().contains("secret"));
    assert_eq!(
        &serde_json::from_slice::<Character>(&bytes).unwrap(),
        character
    );
}
#[cfg(unix)]
#[test]
fn unix_private_modes_symlinks_and_hardlinks() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let (_temp, path) = directory();
    let mut profile = ProfileStore::create(&path).unwrap();
    profile.create_character(input("Alice")).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o700
    );
    for name in ["profile.json", "profile.previous.json", "profile.lock"] {
        assert_eq!(
            fs::metadata(path.join(name)).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    drop(profile);
    let alias = path.with_extension("link");
    symlink(&path, &alias).unwrap();
    assert!(ProfileStore::open(&alias).is_err());
    let real = path.join("real.json");
    fs::rename(path.join("profile.json"), &real).unwrap();
    symlink(&real, path.join("profile.json")).unwrap();
    assert!(ProfileStore::open(&path).is_err());
    fs::remove_file(path.join("profile.json")).unwrap();
    fs::hard_link(&real, path.join("profile.json")).unwrap();
    assert!(ProfileStore::open(&path).is_err());
    fs::remove_file(path.join("profile.json")).unwrap();
    fs::rename(real, path.join("profile.json")).unwrap();
    fs::set_permissions(path.join("profile.json"), fs::Permissions::from_mode(0o644)).unwrap();
    assert!(ProfileStore::open(&path).is_err());
}
#[cfg(unix)]
#[test]
fn replaced_lock_and_symlink_backup_fail_without_overwrite() {
    use std::os::unix::fs::symlink;
    let (_temp, path) = directory();
    let mut profile = ProfileStore::create(&path).unwrap();
    profile.create_character(input("Alice")).unwrap();
    let current = fs::read(path.join("profile.json")).unwrap();
    fs::remove_file(path.join("profile.previous.json")).unwrap();
    symlink(
        path.join("profile.json"),
        path.join("profile.previous.json"),
    )
    .unwrap();
    assert!(profile.create_character(input("Beryl")).is_err());
    assert_eq!(fs::read(path.join("profile.json")).unwrap(), current);
    fs::remove_file(path.join("profile.lock")).unwrap();
    assert!(profile.create_character(input("Beryl")).is_err());
    assert_eq!(profile.characters().len(), 1);
}

#[test]
fn pre_commit_io_failures_preserve_current_backup_and_memory() {
    for fault in [
        storage::SaveFault::BeforeSync,
        storage::SaveFault::BeforeRename,
    ] {
        let (_temp, path) = directory();
        let mut profile = ProfileStore::create(&path).unwrap();
        profile.create_character(input("Alice")).unwrap();
        let current = fs::read(path.join("profile.json")).unwrap();
        profile.storage.inject_fault(fault);
        assert!(matches!(
            profile.create_character(input("Beryl")),
            Err(Error::Io(_))
        ));
        assert_eq!(profile.characters().len(), 1);
        assert_eq!(fs::read(path.join("profile.json")).unwrap(), current);
        assert_eq!(
            fs::read(path.join("profile.previous.json")).unwrap(),
            current
        );
        assert_eq!(
            fs::read_dir(&path)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
                .count(),
            0
        );
        drop(profile);
        assert_eq!(ProfileStore::open(path).unwrap().characters().len(), 1);
    }
}
#[test]
fn post_rename_failure_preserves_backup_and_requires_reopen() {
    let (_temp, path) = directory();
    let mut profile = ProfileStore::create(&path).unwrap();
    profile.create_character(input("Alice")).unwrap();
    let current = fs::read(path.join("profile.json")).unwrap();
    profile
        .storage
        .inject_fault(storage::SaveFault::AfterRename);
    assert!(matches!(
        profile.create_character(input("Beryl")),
        Err(Error::CommitUncertain)
    ));
    assert_eq!(profile.characters().len(), 1);
    assert_eq!(
        fs::read(path.join("profile.previous.json")).unwrap(),
        current
    );
    assert!(matches!(
        profile.create_character(input("Clover")),
        Err(Error::CommitUncertain)
    ));
    drop(profile);
    // A rename already took effect; reopening validates actual disk state rather than retrying.
    let profile = ProfileStore::open(path).unwrap();
    assert_eq!(profile.characters().len(), 2);
    assert_eq!(profile.selected_character().unwrap().name, "Beryl");
}

#[test]
fn closing_profile_unlocks_even_with_an_inherited_file_description() {
    let (_temp, path) = directory();
    let profile = ProfileStore::create(&path).unwrap();
    let inherited = profile.storage.clone_lock();
    drop(profile);
    let _reopened = ProfileStore::open(&path).unwrap();
    drop(inherited);
}

#[test]
fn archive_and_restore_after_restart_preserve_identity_other_characters_and_world_bytes() {
    let (_temp, path) = directory();
    let mut profile = ProfileStore::create(&path).unwrap();
    let first = profile.create_character(input("Alice")).unwrap();
    let second = profile.create_character(input("Beryl")).unwrap();
    let key = profile.public_key();
    let owner = profile.owner_id();
    let mut position = profile.character(first).unwrap().position;
    position.position[0] += 50.0;
    profile.save_position(first, position).unwrap();
    let original = profile.character(first).unwrap().clone();
    let other = profile.character(second).unwrap().clone();
    // A synthetic native-world checkpoint keyed by the original character. No save is parsed,
    // renamed or removed by this profile-only operation, even if it is adjacent to the profile.
    let world = path.parent().unwrap().join("world-saves");
    fs::create_dir(&world).unwrap();
    fs::write(
        world.join("synthetic-world.json"),
        b"synthetic native-world checkpoint",
    )
    .unwrap();
    profile.archive_character(owner, first).unwrap();
    assert_eq!(profile.characters(), std::slice::from_ref(&other));
    assert_eq!(profile.selected_character(), Some(&other));
    assert_eq!(
        profile.archived_characters(),
        std::slice::from_ref(&original)
    );
    assert!(profile.select_character(first).is_err());
    assert!(profile.save_position(first, position).is_err());
    drop(profile);
    let mut profile = ProfileStore::open(&path).unwrap();
    assert_eq!(profile.public_key(), key);
    assert_eq!(profile.owner_id(), owner);
    assert_eq!(profile.character(first), None);
    profile.restore_character(owner, first).unwrap();
    assert_eq!(profile.selected_character(), Some(&other));
    drop(profile);
    let mut profile = ProfileStore::open(&path).unwrap();
    assert_eq!(profile.character(first), Some(&original));
    assert_eq!(profile.character(second), Some(&other));
    assert!(profile.archived_characters().is_empty());
    profile.select_character(first).unwrap();
    profile.archive_character(owner, first).unwrap();
    assert!(profile.selected_character().is_none());
    drop(profile);
    let profile = ProfileStore::open(&path).unwrap();
    assert!(profile.selected_character().is_none());
    assert_eq!(
        fs::read(world.join("synthetic-world.json")).unwrap(),
        b"synthetic native-world checkpoint"
    );
    let challenge = [45; 32];
    assert!(verify_transport_challenge(
        &key,
        &challenge,
        &profile.identity().sign_transport_challenge(&challenge)
    ));
}

#[test]
fn wrong_owner_stale_duplicate_and_conflicting_restore_never_mutate() {
    let (_temp, path) = directory();
    let mut profile = ProfileStore::create(&path).unwrap();
    let id = profile.create_character(input("Alice")).unwrap();
    let owner = profile.owner_id();
    let mut foreign = owner;
    foreign[0] ^= 1;
    let before = fs::read(path.join("profile.json")).unwrap();
    assert!(profile.archive_character(foreign, id).is_err());
    assert!(profile.archive_character(owner, [0; 16]).is_err());
    assert_eq!(fs::read(path.join("profile.json")).unwrap(), before);
    profile.archive_character(owner, id).unwrap();
    let before = fs::read(path.join("profile.json")).unwrap();
    assert!(profile.archive_character(owner, id).is_err());
    assert!(profile.restore_character(foreign, id).is_err());
    assert_eq!(fs::read(path.join("profile.json")).unwrap(), before);
    let new = profile.create_character(input("ALICE")).unwrap();
    let before = fs::read(path.join("profile.json")).unwrap();
    assert!(profile.restore_character(owner, id).is_err());
    assert_eq!(fs::read(path.join("profile.json")).unwrap(), before);
    assert_eq!(profile.characters()[0].id, new);
    assert_eq!(profile.archived_characters()[0].id, id);
    profile.archive_character(owner, new).unwrap();
    profile.restore_character(owner, id).unwrap();
    assert!(profile.restore_character(owner, id).is_err());
    assert_eq!(profile.characters()[0].id, id);
    assert_eq!(profile.archived_characters()[0].id, new);
}

#[test]
fn archive_and_active_limits_never_evict_records() {
    let (_temp, path) = directory();
    let mut profile = ProfileStore::create(&path).unwrap();
    let owner = profile.owner_id();
    for index in 0..MAX_RETAINED_CHARACTERS {
        let name = format!(
            "A{}{}",
            char::from(b'a' + (index / 26) as u8),
            char::from(b'a' + (index % 26) as u8)
        );
        let id = profile.create_character(input(&name)).unwrap();
        profile.archive_character(owner, id).unwrap();
    }
    let archive = profile.archived_characters().to_vec();
    let before = fs::read(path.join("profile.json")).unwrap();
    assert!(profile.create_character(input("Beryl")).is_err());
    assert_eq!(fs::read(path.join("profile.json")).unwrap(), before);
    assert_eq!(profile.archived_characters(), archive);
    // Full retention still permits restore then archive; no capacity deadlock or eviction.
    profile.restore_character(owner, archive[0].id).unwrap();
    profile.archive_character(owner, archive[0].id).unwrap();
    assert_eq!(profile.archived_characters().len(), MAX_ARCHIVED_CHARACTERS);
    for character in archive.iter().take(MAX_CHARACTERS) {
        profile.restore_character(owner, character.id).unwrap();
    }
    let before = fs::read(path.join("profile.json")).unwrap();
    assert!(profile
        .restore_character(owner, archive[MAX_CHARACTERS].id)
        .is_err());
    assert!(profile.create_character(input("Beryl")).is_err());
    assert_eq!(fs::read(path.join("profile.json")).unwrap(), before);
    assert_eq!(
        profile.characters().len() + profile.archived_characters().len(),
        MAX_RETAINED_CHARACTERS
    );
    profile.archive_character(owner, archive[0].id).unwrap();
    profile
        .restore_character(owner, archive[MAX_CHARACTERS].id)
        .unwrap();
}

#[test]
fn archive_and_restore_precommit_failures_preserve_current_and_memory() {
    for restoring in [false, true] {
        for fault in [
            storage::SaveFault::BeforeSync,
            storage::SaveFault::BeforeRename,
        ] {
            let (_temp, path) = directory();
            let mut profile = ProfileStore::create(&path).unwrap();
            let id = profile.create_character(input("Alice")).unwrap();
            let owner = profile.owner_id();
            if restoring {
                profile.archive_character(owner, id).unwrap();
            }
            let before = fs::read(path.join("profile.json")).unwrap();
            let active = profile.characters().to_vec();
            let archived = profile.archived_characters().to_vec();
            let selected = profile.selected_character().cloned();
            profile.storage.inject_fault(fault);
            let result = if restoring {
                profile.restore_character(owner, id)
            } else {
                profile.archive_character(owner, id)
            };
            assert!(matches!(result, Err(Error::Io(_))));
            assert_eq!(profile.characters(), active);
            assert_eq!(profile.archived_characters(), archived);
            assert_eq!(profile.selected_character(), selected.as_ref());
            assert_eq!(fs::read(path.join("profile.json")).unwrap(), before);
            drop(profile);
            let profile = ProfileStore::open(&path).unwrap();
            assert_eq!(profile.characters(), active);
            assert_eq!(profile.archived_characters(), archived);
        }
    }
}

#[test]
fn archive_and_restore_uncertain_commit_require_reopen_and_never_duplicate() {
    for restoring in [false, true] {
        let (_temp, path) = directory();
        let mut profile = ProfileStore::create(&path).unwrap();
        let id = profile.create_character(input("Alice")).unwrap();
        let owner = profile.owner_id();
        if restoring {
            profile.archive_character(owner, id).unwrap();
        }
        let before = fs::read(path.join("profile.json")).unwrap();
        profile
            .storage
            .inject_fault(storage::SaveFault::AfterRename);
        let result = if restoring {
            profile.restore_character(owner, id)
        } else {
            profile.archive_character(owner, id)
        };
        assert!(matches!(result, Err(Error::CommitUncertain)));
        assert_eq!(profile.character(id).is_some(), !restoring);
        assert_eq!(
            fs::read(path.join("profile.previous.json")).unwrap(),
            before
        );
        assert!(matches!(
            profile.create_character(input("Beryl")),
            Err(Error::CommitUncertain)
        ));
        drop(profile);
        let mut profile = ProfileStore::open(&path).unwrap();
        assert_eq!(profile.character(id).is_some(), restoring);
        assert_eq!(
            profile.characters().len() + profile.archived_characters().len(),
            1
        );
        let repeat = if restoring {
            profile.restore_character(owner, id)
        } else {
            profile.archive_character(owner, id)
        };
        assert!(repeat.is_err());
    }
}

// Independent frozen v1 layout: adding fields to current State cannot change these bytes or
// the original checksum's field order. Only generated synthetic fixture secrets are used.
fn version_one_bytes(profile: &ProfileStore) -> Vec<u8> {
    #[derive(Serialize)]
    struct OldState<'a> {
        revision: u64,
        selected: Option<CharacterId>,
        characters: &'a [Character],
    }
    #[derive(Serialize)]
    struct OldPayload<'a> {
        secret: [u8; 32],
        public_key: [u8; 32],
        owner_id: OwnerId,
        state: OldState<'a>,
    }
    #[derive(Serialize)]
    struct OldEnvelope<'a> {
        format_version: u32,
        payload: OldPayload<'a>,
        checksum: [u8; 32],
    }
    let payload = OldPayload {
        secret: profile.identity.key.to_bytes(),
        public_key: profile.public_key(),
        owner_id: profile.owner_id(),
        state: OldState {
            revision: profile.state.revision,
            selected: profile.state.selected,
            characters: profile.characters(),
        },
    };
    let bytes = serde_json::to_vec(&payload).unwrap();
    let mut hash = Sha256::new();
    hash.update(b"benilla-profile-v1\0");
    hash.update(bytes);
    serde_json::to_vec(&OldEnvelope {
        format_version: 1,
        payload,
        checksum: hash.finalize().into(),
    })
    .unwrap()
}

#[test]
fn version_one_checksum_verified_before_lossless_read_only_open_and_explicit_mutation() {
    let (_temp, path) = directory();
    let mut profile = ProfileStore::create(&path).unwrap();
    let one = profile.create_character(input("Alice")).unwrap();
    let two = profile.create_character(input("Beryl")).unwrap();
    let records = profile.characters().to_vec();
    let owner = profile.owner_id();
    let key = profile.public_key();
    let old = version_one_bytes(&profile);
    drop(profile);
    fs::write(path.join("profile.json"), &old).unwrap();
    let mut profile = ProfileStore::open(&path).unwrap();
    assert_eq!(fs::read(path.join("profile.json")).unwrap(), old);
    assert_eq!(profile.characters(), records);
    assert!(profile.archived_characters().is_empty());
    assert_eq!(profile.selected_character().unwrap().id, two);
    assert_eq!(profile.public_key(), key);
    profile.archive_character(owner, one).unwrap();
    assert_eq!(fs::read(path.join("profile.previous.json")).unwrap(), old);
    drop(profile);
    let profile = ProfileStore::open(&path).unwrap();
    assert_eq!(profile.character(two), Some(&records[1]));
    assert_eq!(
        profile.archived_characters(),
        std::slice::from_ref(&records[0])
    );
    assert_eq!(profile.public_key(), key);
    drop(profile);
    let mut corrupted: serde_json::Value = serde_json::from_slice(&old).unwrap();
    corrupted["payload"]["state"]["characters"][0]["name"] = "Tampered".into();
    let corrupted = serde_json::to_vec(&corrupted).unwrap();
    fs::write(path.join("profile.json"), &corrupted).unwrap();
    assert!(matches!(ProfileStore::open(&path), Err(Error::Corrupt)));
    assert_eq!(fs::read(path.join("profile.json")).unwrap(), corrupted);
    let mut unexpected: serde_json::Value = serde_json::from_slice(&old).unwrap();
    unexpected["payload"]["state"]["archived"] = serde_json::json!([]);
    let unexpected = serde_json::to_vec(&unexpected).unwrap();
    fs::write(path.join("profile.json"), &unexpected).unwrap();
    assert!(matches!(ProfileStore::open(&path), Err(Error::Corrupt)));
    assert_eq!(fs::read(path.join("profile.json")).unwrap(), unexpected);
}

#[test]
fn failed_version_one_migration_keeps_original_bytes_and_uncertain_migration_can_reopen() {
    for fault in [
        storage::SaveFault::BeforeSync,
        storage::SaveFault::BeforeRename,
        storage::SaveFault::AfterRename,
    ] {
        let (_temp, path) = directory();
        let mut profile = ProfileStore::create(&path).unwrap();
        let id = profile.create_character(input("Alice")).unwrap();
        let owner = profile.owner_id();
        let key = profile.public_key();
        let old = version_one_bytes(&profile);
        drop(profile);
        fs::write(path.join("profile.json"), &old).unwrap();
        let mut profile = ProfileStore::open(&path).unwrap();
        profile.storage.inject_fault(fault);
        assert!(profile.archive_character(owner, id).is_err());
        assert_eq!(fs::read(path.join("profile.previous.json")).unwrap(), old);
        if fault != storage::SaveFault::AfterRename {
            assert_eq!(fs::read(path.join("profile.json")).unwrap(), old);
        }
        assert!(profile.character(id).is_some());
        drop(profile);
        let profile = ProfileStore::open(&path).unwrap();
        assert_eq!(profile.public_key(), key);
        assert_eq!(
            profile.character(id).is_some(),
            fault != storage::SaveFault::AfterRename
        );
        assert_eq!(
            profile.characters().len() + profile.archived_characters().len(),
            1
        );
    }
}

#[test]
fn archive_validation_covers_owner_id_name_and_selected_character() {
    let (_temp, path) = directory();
    let mut profile = ProfileStore::create(&path).unwrap();
    let first = profile.create_character(input("Alice")).unwrap();
    profile.create_character(input("Beryl")).unwrap();
    profile
        .archive_character(profile.owner_id(), first)
        .unwrap();
    let owner = profile.owner_id();
    for mutation in [
        "owner",
        "duplicate",
        "duplicate_archive",
        "name",
        "position",
        "selection",
        "bound",
    ] {
        let mut state = profile.state.clone();
        match mutation {
            "owner" => state.archived[0].owner_id[0] ^= 1,
            "duplicate" => state.archived[0].id = state.characters[0].id,
            "duplicate_archive" => state.archived.push(state.archived[0].clone()),
            "position" => state.archived[0].position.map_id = 999,
            "name" => state.archived[0].name = "../../invalid".into(),
            "selection" => state.selected = Some(first),
            "bound" => {
                state.archived = vec![state.archived[0].clone(); MAX_ARCHIVED_CHARACTERS + 1]
            }
            _ => unreachable!(),
        }
        assert!(state.validate(owner).is_err(), "{mutation}");
        // Even a recomputed checksum must not make a malformed archive readable.
        let bytes = storage::encode(&profile.identity, &state).unwrap();
        assert!(
            matches!(storage::decode(&bytes), Err(Error::Corrupt)),
            "{mutation}"
        );
    }
}
