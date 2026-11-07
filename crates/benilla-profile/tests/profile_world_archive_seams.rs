//! Synthetic cross-feature invariants: a signed setup sidecar must survive the exact
//! legacy-profile migration, production deletion edge, and distributed-player recovery.
//! No Bevy, networking, real profile, game asset, or native admission is involved.
#[path = "../../benilla-app/src/net/p2p/character_actions.rs"]
mod character_actions;

use benilla_profile::{
    owner_id_from_public_key, recovery, verify_transport_challenge,
    world_setup::{SetupOperation, SetupRequest, SetupRequestId, WorldDeclaration},
    Appearance, Character, CharacterId, Error, OwnerId, ProfileStore, SpawnPolicy,
};
use benilla_protocol::{CharAction, SessionEvent};
use ed25519_dalek::SigningKey;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let mut random = [0; 16];
        getrandom::fill(&mut random).unwrap();
        let path = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("benilla-profile-world-seam-{}", hex(random)));
        private_directory(&path);
        Self(path)
    }
    fn profile_path(&self) -> PathBuf {
        self.0.join("profile")
    }
    fn legacy_profile(&self) -> (Vec<u8>, Vec<Character>, [u8; 32]) {
        // This fixed, public test seed belongs only to the temporary synthetic fixture.
        let secret = [113; 32];
        let public_key = SigningKey::from_bytes(&secret).verifying_key().to_bytes();
        let owner = owner_id_from_public_key(&public_key);
        let characters = vec![
            character(owner, [1; 16], "Alice"),
            character(owner, [2; 16], "Beryl"),
        ];
        // Freeze the old schema independently from the implementation's private State.
        #[derive(Serialize)]
        struct LegacyState<'a> {
            revision: u64,
            selected: Option<CharacterId>,
            characters: &'a [Character],
        }
        #[derive(Serialize)]
        struct LegacyPayload<'a> {
            secret: [u8; 32],
            public_key: [u8; 32],
            owner_id: OwnerId,
            state: LegacyState<'a>,
        }
        #[derive(Serialize)]
        struct LegacyEnvelope<'a> {
            format_version: u32,
            payload: LegacyPayload<'a>,
            checksum: [u8; 32],
        }
        let payload = LegacyPayload {
            secret,
            public_key,
            owner_id: owner,
            state: LegacyState {
                revision: 37,
                selected: Some(characters[0].id),
                characters: &characters,
            },
        };
        let mut hash = Sha256::new();
        hash.update(b"benilla-profile-v1\0");
        hash.update(serde_json::to_vec(&payload).unwrap());
        let bytes = serde_json::to_vec(&LegacyEnvelope {
            format_version: 1,
            payload,
            checksum: hash.finalize().into(),
        })
        .unwrap();
        private_directory(&self.profile_path());
        private_file(&self.profile_path().join("profile.json"), &bytes);
        (bytes, characters, public_key)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn private_directory(path: &Path) {
    #[allow(unused_mut)]
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).unwrap();
}
fn private_file(path: &Path, bytes: &[u8]) {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).unwrap().write_all(bytes).unwrap();
}
fn character(owner: OwnerId, id: CharacterId, name: &str) -> Character {
    let mut position = SpawnPolicy::NorthshireMeetingV1.position().unwrap();
    position.position = [-8931.25, -117.5, 84.125];
    position.orientation = 1.25;
    Character {
        id,
        owner_id: owner,
        name: name.into(),
        race: 1,
        class: 1,
        gender: 0,
        appearance: Appearance {
            skin: 2,
            face: 3,
            hair_style: 4,
            hair_color: 5,
            facial_hair: 1,
        },
        level: 1,
        spawn_policy: SpawnPolicy::NorthshireMeetingV1,
        position,
    }
}
fn hex(id: [u8; 16]) -> String {
    id.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn request(sequence: u64, revision: u64, label: &str) -> SetupRequest {
    SetupRequest {
        id: SetupRequestId {
            session: [91; 16],
            sequence,
        },
        expected_revision: revision,
        operation: SetupOperation::Create {
            label: label.into(),
            allowlist: [[73; 16], [74; 16]].into(),
        },
    }
}

#[test]
fn legacy_setup_production_delete_reopen_and_player_restore_preserve_every_identity() {
    let fixture = Fixture::new();
    let path = fixture.profile_path();
    let (legacy, original, key) = fixture.legacy_profile();
    let checkpoint = fixture.0.join("synthetic-world-checkpoint");
    private_file(
        &checkpoint,
        b"unchanged world checkpoint for original character ID",
    );
    let mut profile = ProfileStore::open(&path).unwrap();
    assert_eq!(profile.characters(), original);
    assert_eq!(profile.public_key(), key);
    let owner = profile.owner_id();
    let first_guid = character_actions::character_guid(&original[0]);
    let create = request(1, 0, "Original world");
    let world = profile.apply_world_setup(create.clone()).unwrap();
    let declaration = profile.world_setups().unwrap().worlds[0].clone();
    let digest = declaration.digest().unwrap();
    let setup_bytes = fs::read(path.join("local-worlds.json")).unwrap();
    assert_eq!(fs::read(path.join("profile.json")).unwrap(), legacy);
    assert!(!path.join("profile.previous.json").exists());
    assert_eq!(declaration.policy.world_owner, owner);
    assert_eq!(declaration.public_key, key);
    let events = RefCell::new(Vec::new());
    character_actions::delete(
        &mut profile,
        first_guid,
        |event| {
            assert!(matches!(
                event,
                SessionEvent::CharActionResult {
                    action: CharAction::Delete,
                    code: 0x39
                }
            ));
            events.borrow_mut().push("success");
            Ok(())
        },
        |saved| {
            assert_eq!(saved.characters(), &original[1..]);
            assert_eq!(saved.archived_characters(), &original[..1]);
            assert!(saved.selected_character().is_none());
            assert_eq!(
                fs::read(path.join("profile.previous.json")).unwrap(),
                legacy
            );
            let disk: serde_json::Value =
                serde_json::from_slice(&fs::read(path.join("profile.json")).unwrap()).unwrap();
            assert_eq!(disk["format_version"], 2);
            assert_eq!(disk["payload"]["state"]["revision"], 38);
            events.borrow_mut().push("durable roster");
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(*events.borrow(), ["durable roster", "success"]);
    assert_eq!(
        fs::read(path.join("local-worlds.json")).unwrap(),
        setup_bytes
    );
    drop(profile);

    let mut profile = ProfileStore::open(&path).unwrap();
    assert_eq!(profile.public_key(), key);
    assert_eq!(profile.owner_id(), owner);
    assert_eq!(profile.apply_world_setup(create).unwrap(), world);
    let saved = profile.world_setups().unwrap();
    assert_eq!(saved.worlds, [declaration]);
    assert_eq!(saved.selected, Some(world));
    assert_eq!(saved.revision, 1);
    assert_eq!(saved.worlds[0].digest().unwrap(), digest);
    assert_eq!(
        fs::read(path.join("local-worlds.json")).unwrap(),
        setup_bytes
    );
    drop(profile);

    let mut output = Vec::new();
    recovery::run(&[OsString::from("list-archived")], Some(&path), &mut output).unwrap();
    assert_eq!(
        String::from_utf8(output).unwrap(),
        format!("{}  Alice\n", hex(original[0].id))
    );
    recovery::run(
        &[
            OsString::from("restore"),
            OsString::from(hex(original[0].id)),
            OsString::from("--confirm"),
        ],
        Some(&path),
        &mut Vec::new(),
    )
    .unwrap();
    let mut profile = ProfileStore::open(&path).unwrap();
    assert_eq!(profile.character(original[0].id), Some(&original[0]));
    assert_eq!(profile.character(original[1].id), Some(&original[1]));
    assert!(profile.archived_characters().is_empty());
    assert!(profile.selected_character().is_none());
    assert_eq!(
        character_actions::character_guid(profile.character(original[0].id).unwrap()),
        first_guid
    );
    assert_eq!(profile.world_setups().unwrap(), saved);
    assert_eq!(
        fs::read(path.join("local-worlds.json")).unwrap(),
        setup_bytes
    );
    assert_eq!(
        fs::read(checkpoint).unwrap(),
        b"unchanged world checkpoint for original character ID"
    );
    let challenge = [35; 32];
    assert!(verify_transport_challenge(
        &key,
        &challenge,
        &profile.identity().sign_transport_challenge(&challenge)
    ));
}

#[test]
fn failed_legacy_migration_and_failed_setup_commit_leave_the_other_file_unchanged() {
    let fixture = Fixture::new();
    let path = fixture.profile_path();
    let (legacy, original, _) = fixture.legacy_profile();
    let mut profile = ProfileStore::open(&path).unwrap();
    profile
        .apply_world_setup(request(1, 0, "Preserved world"))
        .unwrap();
    let setup_bytes = fs::read(path.join("local-worlds.json")).unwrap();
    let saved = profile.world_setups().unwrap();
    // A private directory at the backup filename forces a real precommit failure.
    private_directory(&path.join("profile.previous.json"));
    assert!(profile
        .archive_character(profile.owner_id(), original[0].id)
        .is_err());
    assert_eq!(profile.characters(), original);
    assert!(profile.archived_characters().is_empty());
    assert_eq!(fs::read(path.join("profile.json")).unwrap(), legacy);
    assert_eq!(
        fs::read(path.join("local-worlds.json")).unwrap(),
        setup_bytes
    );
    fs::remove_dir(path.join("profile.previous.json")).unwrap();
    profile
        .archive_character(profile.owner_id(), original[0].id)
        .unwrap();
    let migrated = fs::read(path.join("profile.json")).unwrap();
    let previous = fs::read(path.join("profile.previous.json")).unwrap();
    private_directory(&path.join("local-worlds.previous.json"));
    assert!(profile
        .apply_world_setup(request(2, 1, "Refused world"))
        .is_err());
    assert_eq!(fs::read(path.join("profile.json")).unwrap(), migrated);
    assert_eq!(
        fs::read(path.join("profile.previous.json")).unwrap(),
        previous
    );
    assert_eq!(
        fs::read(path.join("local-worlds.json")).unwrap(),
        setup_bytes
    );
    assert_eq!(profile.world_setups().unwrap(), saved);
    drop(profile);
    let mut profile = ProfileStore::open(&path).unwrap();
    assert_eq!(profile.archived_characters(), &original[..1]);
    assert_eq!(profile.world_setups().unwrap(), saved);
}

#[test]
fn corrupt_legacy_profile_cannot_open_or_recover_through_valid_setup_sidecar() {
    let fixture = Fixture::new();
    let path = fixture.profile_path();
    let (legacy, original, _) = fixture.legacy_profile();
    let mut profile = ProfileStore::open(&path).unwrap();
    profile
        .apply_world_setup(request(1, 0, "Untouched world"))
        .unwrap();
    let setup_bytes = fs::read(path.join("local-worlds.json")).unwrap();
    drop(profile);
    let mut corrupt: serde_json::Value = serde_json::from_slice(&legacy).unwrap();
    corrupt["payload"]["state"]["characters"][0]["name"] = "Changed".into();
    let corrupt = serde_json::to_vec(&corrupt).unwrap();
    fs::write(path.join("profile.json"), &corrupt).unwrap();
    assert!(matches!(ProfileStore::open(&path), Err(Error::Corrupt)));
    let mut output = Vec::new();
    assert!(recovery::run(
        &[
            OsString::from("restore"),
            OsString::from(hex(original[0].id)),
            OsString::from("--confirm")
        ],
        Some(&path),
        &mut output,
    )
    .is_err());
    assert!(output.is_empty());
    assert_eq!(fs::read(path.join("profile.json")).unwrap(), corrupt);
    assert_eq!(
        fs::read(path.join("local-worlds.json")).unwrap(),
        setup_bytes
    );
    assert!(!path.join("profile.previous.json").exists());
}

#[test]
fn migration_keeps_domain_separation_and_replay_cannot_reselect_an_old_result() {
    let fixture = Fixture::new();
    let path = fixture.profile_path();
    let (_, original, _) = fixture.legacy_profile();
    let mut profile = ProfileStore::open(&path).unwrap();
    let first = request(1, 0, "First world");
    let first_world = profile.apply_world_setup(first.clone()).unwrap();
    profile
        .archive_character(profile.owner_id(), original[0].id)
        .unwrap();
    let second_world = profile
        .apply_world_setup(request(2, 1, "Second world"))
        .unwrap();
    assert_ne!(first_world, second_world);
    let snapshot = profile.world_setups().unwrap();
    let setup_bytes = fs::read(path.join("local-worlds.json")).unwrap();
    assert_eq!(profile.apply_world_setup(first).unwrap(), first_world);
    assert_eq!(profile.world_setups().unwrap().selected, Some(second_world));
    assert!(profile
        .apply_world_setup(request(1, 0, "Conflicting request"))
        .is_err());
    assert_eq!(
        fs::read(path.join("local-worlds.json")).unwrap(),
        setup_bytes
    );

    let declaration = &snapshot.worlds[0];
    let transport_signature = profile
        .identity()
        .sign_transport_challenge(&declaration.digest().unwrap());
    let mut substituted = serde_json::to_value(declaration).unwrap();
    substituted["signature"] = serde_json::to_value(transport_signature.as_slice()).unwrap();
    let substituted: WorldDeclaration = serde_json::from_value(substituted).unwrap();
    assert!(substituted.verify().is_err());
    let other = Fixture::new();
    let mut foreign = ProfileStore::create(other.profile_path()).unwrap();
    private_file(
        &other.profile_path().join("local-worlds.json"),
        &setup_bytes,
    );
    assert!(matches!(foreign.world_setups(), Err(Error::Corrupt)));
    assert!(foreign
        .apply_world_setup(request(1, 0, "Cannot claim foreign world"))
        .is_err());
    assert_eq!(
        fs::read(other.profile_path().join("local-worlds.json")).unwrap(),
        setup_bytes
    );
}
