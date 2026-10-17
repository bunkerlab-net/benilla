//! Local-only P2P identity and exploration character persistence, independent of Bevy.
//!
//! Creating a profile is an explicit user action: opening a missing or corrupt profile never
//! generates a replacement identity. Keep its entire directory private; it contains an
//! unencrypted signing key and a previous-good snapshot. Neither is safe to share.
//!
//! This is not a realm database or an anti-cheat system. The local owner can copy, edit, or
//! roll back saves. There is no XP, inventory, spell, quest, loot, or combat authority here.
//! Character creation fixes level at one. `SpawnPolicy::NorthshireMeetingV1` deliberately
//! identifies legacy characters at the exploration meeting point. Normal app creation
//! supplies racial defaults; configured creation can use a private source-v2 capture.
//!
//! See the crate README for durability, concurrency, platform, and appearance limitations.
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, fmt, path::Path, sync::Arc};
use zeroize::Zeroizing;
pub mod recovery;
pub mod starting_location;
mod storage;
pub mod world_setup;

pub type CharacterId = [u8; 16];
pub type OwnerId = [u8; 16];
pub const FORMAT_VERSION: u32 = 2;
pub const MAX_CHARACTERS: usize = 10;
/// Shared active + archive budget. Transfers never need an eviction or irreversible purge.
pub const MAX_RETAINED_CHARACTERS: usize = 32;
pub const MAX_ARCHIVED_CHARACTERS: usize = MAX_RETAINED_CHARACTERS;
pub const MAX_FILE_BYTES: u64 = 64 * 1024;
pub const OWNER_ID_DOMAIN: &[u8] = b"benilla-owner-id-v1\0";
pub const TRANSPORT_IDENTITY_DOMAIN: &[u8] = b"benilla-transport-identity-v1\0";

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Invalid(&'static str),
    AlreadyExists,
    Locked,
    Corrupt,
    UnsupportedVersion,
    ConcurrentModification,
    /// A rename succeeded but its directory sync failed. Reopen before further mutations.
    CommitUncertain,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "profile I/O: {e}"),
            Self::Invalid(s) => f.write_str(s),
            Self::AlreadyExists => f.write_str("profile already exists; open it instead"),
            Self::Locked => f.write_str("profile is already open in another session"),
            Self::Corrupt => {
                f.write_str("profile is corrupt; preserved files require explicit recovery")
            }
            Self::UnsupportedVersion => f.write_str("unsupported profile format or spawn policy"),
            Self::ConcurrentModification => {
                f.write_str("profile changed on disk; reopen before saving")
            }
            Self::CommitUncertain => {
                f.write_str("profile commit durability is uncertain; reopen before saving")
            }
        }
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

/// Immutable signing capability. No Debug, serialization, or secret-key accessor.
/// `SigningKey` zeroizes on final drop; clones of the Arc retain the same capability.
pub struct ProfileIdentity {
    key: SigningKey,
}
impl ProfileIdentity {
    pub fn public_key(&self) -> [u8; 32] {
        self.key.verifying_key().to_bytes()
    }
    pub fn owner_id(&self) -> OwnerId {
        owner_id_from_public_key(&self.public_key())
    }
    /// Sign only the domain-bound digest of a transport's canonical connection transcript.
    /// The transport must include its fresh authenticated handshake and both parties' claims.
    pub fn sign_transport_challenge(&self, challenge: &[u8; 32]) -> [u8; 64] {
        self.key.sign(&transport_message(challenge)).to_bytes()
    }
}
pub fn owner_id_from_public_key(public_key: &[u8; 32]) -> OwnerId {
    let mut hash = Sha256::new();
    hash.update(OWNER_ID_DOMAIN);
    hash.update(public_key);
    let mut id = [0; 16];
    id.copy_from_slice(&hash.finalize()[..16]);
    id
}
fn transport_message(challenge: &[u8; 32]) -> Vec<u8> {
    [TRANSPORT_IDENTITY_DOMAIN, challenge.as_slice()].concat()
}
pub fn verify_transport_challenge(
    public_key: &[u8; 32],
    challenge: &[u8; 32],
    signature: &[u8; 64],
) -> bool {
    VerifyingKey::from_bytes(public_key).is_ok_and(|key| {
        key.verify_strict(
            &transport_message(challenge),
            &Signature::from_bytes(signature),
        )
        .is_ok()
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Appearance {
    pub skin: u8,
    pub face: u8,
    pub hair_style: u8,
    pub hair_color: u8,
    pub facial_hair: u8,
}
impl Appearance {
    /// Safety bounds only; the app must resolve actual options from the player's client DBCs.
    pub fn validate(&self) -> Result<(), Error> {
        if [self.skin, self.face, self.hair_style, self.hair_color]
            .iter()
            .any(|v| *v > 15)
            || self.facial_hair > 8
        {
            return Err(Error::Invalid(
                "appearance is outside the supported exploration range",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpawnPolicy {
    NorthshireMeetingV1,
    /// App-provided racial default at creation; reopening uses only the saved pose.
    ClassicRacialV1,
    /// Explicit opt-in source position; reopening always uses Character::position.
    ClassicSourceV1 {
        capture_sha256: [u8; 32],
    },
}
impl SpawnPolicy {
    pub fn position(self) -> Result<Position, Error> {
        if !matches!(self, Self::NorthshireMeetingV1) {
            return Err(Error::Invalid(
                "source starts require their captured position",
            ));
        }
        // Existing fork exploration spawn, not a bundled world-database/race-start table.
        Ok(Position {
            map_id: 0,
            zone_id: 12,
            position: [-8949.95, -132.49, 83.9],
            orientation: 0.0,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub map_id: u32,
    pub zone_id: u32,
    pub position: [f32; 3],
    pub orientation: f32,
}
impl Position {
    pub fn validate(&self) -> Result<(), Error> {
        // This slice supports the two original open-world continents, not instances/transports.
        if !matches!(self.map_id, 0 | 1)
            || self.zone_id > 10_000
            || self
                .position
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 20_000.0)
            || !self.orientation.is_finite()
            || self.orientation.abs() > 100.0
        {
            return Err(Error::Invalid(
                "position is outside the supported exploration bounds",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewCharacter {
    pub name: String,
    pub race: u8,
    pub class: u8,
    pub gender: u8,
    pub appearance: Appearance,
}
impl NewCharacter {
    pub fn validate(&self) -> Result<(), Error> {
        canonical_name(&self.name)?;
        validate_race_class(self.race, self.class)?;
        if self.gender > 1 {
            return Err(Error::Invalid("gender must be 0 or 1"));
        }
        self.appearance.validate()
    }
}

/// Public state only. Mutating a clone never changes the persisted profile.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Character {
    pub id: CharacterId,
    pub owner_id: OwnerId,
    pub name: String,
    pub race: u8,
    pub class: u8,
    pub gender: u8,
    pub appearance: Appearance,
    pub level: u8,
    pub spawn_policy: SpawnPolicy,
    pub position: Position,
}
impl Character {
    pub fn validate(&self) -> Result<(), Error> {
        if self.id == [0; 16] || self.owner_id == [0; 16] || self.level != 1 {
            return Err(Error::Invalid(
                "invalid character identity or unsupported level",
            ));
        }
        let creation = NewCharacter {
            name: self.name.clone(),
            race: self.race,
            class: self.class,
            gender: self.gender,
            appearance: self.appearance,
        };
        creation.validate()?;
        if canonical_name(&self.name)? != self.name {
            return Err(Error::Invalid("character name is not canonical"));
        }
        self.position.validate()
    }
    /// Version-one compact, field-ordered JSON, containing no local signing secret.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, Error> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|_| Error::Invalid("cannot serialize character"))
    }
}

/// Classic 1.12 playable combinations, with the game's ChrRaces/ChrClasses numeric IDs.
/// The app still obtains display models and cosmetic choices from the player's own data.
pub fn valid_classes(race: u8) -> &'static [u8] {
    match race {
        1 => &[1, 2, 4, 5, 8, 9],
        2 => &[1, 3, 4, 7, 9],
        3 => &[1, 2, 3, 4, 5],
        4 => &[1, 3, 4, 5, 11],
        5 => &[1, 4, 5, 8, 9],
        6 => &[1, 3, 7, 11],
        7 => &[1, 4, 8, 9],
        8 => &[1, 3, 4, 5, 7, 8],
        _ => &[],
    }
}
pub fn validate_race_class(race: u8, class: u8) -> Result<(), Error> {
    if !valid_classes(race).contains(&class) {
        return Err(Error::Invalid("invalid Classic race/class combination"));
    }
    Ok(())
}
fn canonical_name(name: &str) -> Result<String, Error> {
    if !(2..=12).contains(&name.len()) || !name.bytes().all(|c| c.is_ascii_alphabetic()) {
        return Err(Error::Invalid(
            "character name must contain 2 to 12 ASCII letters",
        ));
    }
    let mut result = name.to_ascii_lowercase();
    result[..1].make_ascii_uppercase();
    Ok(result)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    revision: u64,
    selected: Option<CharacterId>,
    characters: Vec<Character>,
    archived: Vec<Character>,
}
impl State {
    fn validate(&self, owner: OwnerId) -> Result<(), Error> {
        if self.characters.len() > MAX_CHARACTERS
            || self.characters.len() + self.archived.len() > MAX_RETAINED_CHARACTERS
        {
            return Err(Error::Corrupt);
        }
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        for character in &self.characters {
            character.validate().map_err(|_| Error::Corrupt)?;
            if character.owner_id != owner
                || !ids.insert(character.id)
                || !names.insert(character.name.to_ascii_lowercase())
            {
                return Err(Error::Corrupt);
            }
        }
        if self.selected.is_some_and(|id| !ids.contains(&id)) {
            return Err(Error::Corrupt);
        }
        // Archived names may be reused by active characters, but IDs are unique forever.
        for character in &self.archived {
            character.validate().map_err(|_| Error::Corrupt)?;
            if character.owner_id != owner || !ids.insert(character.id) {
                return Err(Error::Corrupt);
            }
        }
        Ok(())
    }
}

/// One exclusively locked local profile directory; key and state are never logged or Debug.
/// Pass an app-resolved path under `benilla_app::config_dir()`, not the game install.
pub struct ProfileStore {
    storage: storage::Storage,
    identity: Arc<ProfileIdentity>,
    state: State,
    disk_hash: [u8; 32],
    uncertain: bool,
}
impl ProfileStore {
    /// Explicitly create a new persistent local identity. Never call as an open fallback.
    /// Parent directories must already exist. A pre-existing profile is never overwritten.
    pub fn create(directory: impl AsRef<Path>) -> Result<Self, Error> {
        let storage = storage::Storage::acquire(directory.as_ref(), true)?;
        storage.ensure_absent()?;
        let mut seed = Zeroizing::new([0; 32]);
        getrandom::fill(seed.as_mut())
            .map_err(|_| Error::Invalid("secure random source unavailable"))?;
        let identity = Arc::new(ProfileIdentity {
            key: SigningKey::from_bytes(&seed),
        });
        let state = State {
            revision: 0,
            selected: None,
            characters: Vec::new(),
            archived: Vec::new(),
        };
        let bytes = storage::encode(&identity, &state)?;
        storage.write(&bytes, None)?;
        Ok(Self {
            storage,
            identity,
            state,
            disk_hash: Sha256::digest(&bytes).into(),
            uncertain: false,
        })
    }
    /// Open and validate existing state. Missing/corrupt state fails closed without recovery.
    pub fn open(directory: impl AsRef<Path>) -> Result<Self, Error> {
        let storage = storage::Storage::acquire(directory.as_ref(), false)?;
        let bytes = storage.read()?;
        let (identity, state) = storage::decode(&bytes)?;
        Ok(Self {
            storage,
            identity: Arc::new(identity),
            state,
            disk_hash: Sha256::digest(&bytes).into(),
            uncertain: false,
        })
    }
    pub fn identity(&self) -> Arc<ProfileIdentity> {
        Arc::clone(&self.identity)
    }
    pub fn public_key(&self) -> [u8; 32] {
        self.identity.public_key()
    }
    pub fn owner_id(&self) -> OwnerId {
        self.identity.owner_id()
    }
    pub fn characters(&self) -> &[Character] {
        &self.state.characters
    }
    /// Recoverable records excluded from the active roster and world entry.
    pub fn archived_characters(&self) -> &[Character] {
        &self.state.archived
    }
    pub fn selected_character(&self) -> Option<&Character> {
        self.state
            .selected
            .and_then(|id| self.state.characters.iter().find(|c| c.id == id))
    }
    pub fn character(&self, id: CharacterId) -> Option<&Character> {
        self.state.characters.iter().find(|c| c.id == id)
    }
    pub fn create_character(&mut self, input: NewCharacter) -> Result<CharacterId, Error> {
        self.create_character_at(
            input,
            SpawnPolicy::NorthshireMeetingV1,
            SpawnPolicy::NorthshireMeetingV1.position()?,
        )
    }
    /// Persist an app-selected racial starting pose for a new exploration character.
    /// Existing characters and legacy policy records are never reinterpreted.
    pub fn create_character_at_racial_start(
        &mut self,
        input: NewCharacter,
        position: Position,
    ) -> Result<CharacterId, Error> {
        self.create_character_at(input, SpawnPolicy::ClassicRacialV1, position)
    }
    /// Explicit opt-in: uses only this selected local capture; never falls back to Northshire.
    /// This does not admit native gameplay or alter an existing character.
    pub fn create_character_from_start(
        &mut self,
        input: NewCharacter,
        start: &starting_location::ClassicStart,
    ) -> Result<CharacterId, Error> {
        input.validate()?;
        let position = start.position_for(input.race, input.class)?;
        self.create_character_at(
            input,
            SpawnPolicy::ClassicSourceV1 {
                capture_sha256: start.capture_sha256(),
            },
            position,
        )
    }
    fn create_character_at(
        &mut self,
        input: NewCharacter,
        policy: SpawnPolicy,
        position: Position,
    ) -> Result<CharacterId, Error> {
        input.validate()?;
        position.validate()?;
        let name = canonical_name(&input.name)?;
        if self.state.characters.len() >= MAX_CHARACTERS {
            return Err(Error::Invalid("profile has reached its character limit"));
        }
        if self.state.characters.len() + self.state.archived.len() >= MAX_RETAINED_CHARACTERS {
            return Err(Error::Invalid("profile has reached its retained-character limit; restore an archived character instead"));
        }
        if self.state.characters.iter().any(|c| c.name == name) {
            return Err(Error::Invalid(
                "character name already exists in this profile",
            ));
        }
        let id = loop {
            let mut id = [0; 16];
            getrandom::fill(&mut id)
                .map_err(|_| Error::Invalid("secure random source unavailable"))?;
            if id != [0; 16]
                && !self
                    .state
                    .characters
                    .iter()
                    .chain(&self.state.archived)
                    .any(|c| c.id == id)
            {
                break id;
            }
        };
        let character = Character {
            id,
            owner_id: self.owner_id(),
            name,
            race: input.race,
            class: input.class,
            gender: input.gender,
            appearance: input.appearance,
            level: 1,
            spawn_policy: policy,
            position,
        };
        let mut next = self.state.clone();
        next.characters.push(character);
        next.selected = Some(id);
        self.commit(next)?;
        Ok(id)
    }
    /// Explicitly confirmed local removal only: retain the full record and every world save.
    /// The caller must bind its UI target to this owner and ID, never a mutable roster index.
    pub fn archive_character(&mut self, owner: OwnerId, id: CharacterId) -> Result<(), Error> {
        if owner != self.owner_id() {
            return Err(Error::Invalid(
                "character owner does not match this profile",
            ));
        }
        let index = self
            .state
            .characters
            .iter()
            .position(|c| c.id == id)
            .ok_or(Error::Invalid(
                "character does not belong to the active roster",
            ))?;
        if self.state.archived.len() >= MAX_ARCHIVED_CHARACTERS {
            return Err(Error::Invalid(
                "character archive is full; restore an archived character first",
            ));
        }
        let mut next = self.state.clone();
        next.archived.push(next.characters.remove(index));
        if next.selected == Some(id) {
            next.selected = None;
        }
        self.commit(next)
    }
    /// Restore the original identity and position without replacing any active character.
    /// Restoration does not select a different character or modify referenced world saves.
    pub fn restore_character(&mut self, owner: OwnerId, id: CharacterId) -> Result<(), Error> {
        if owner != self.owner_id() {
            return Err(Error::Invalid(
                "character owner does not match this profile",
            ));
        }
        let index = self
            .state
            .archived
            .iter()
            .position(|c| c.id == id)
            .ok_or(Error::Invalid("character is not in this profile's archive"))?;
        if self.state.characters.len() >= MAX_CHARACTERS {
            return Err(Error::Invalid(
                "active roster is full; no character was replaced",
            ));
        }
        let archived = &self.state.archived[index];
        if self
            .state
            .characters
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(&archived.name))
        {
            return Err(Error::Invalid(
                "an active character has this name; no character was replaced",
            ));
        }
        let mut next = self.state.clone();
        next.characters.push(next.archived.remove(index));
        self.commit(next)
    }
    pub fn select_character(&mut self, id: CharacterId) -> Result<(), Error> {
        if self.character(id).is_none() {
            return Err(Error::Invalid("character does not belong to this profile"));
        }
        if self.state.selected == Some(id) {
            return Ok(());
        }
        let mut next = self.state.clone();
        next.selected = Some(id);
        self.commit(next)
    }
    /// Local-owner checkpoint only. Never call with positions obtained from remote peer frames.
    /// Bounds are a corruption guard, not collision, speed, zone, or anti-cheat validation.
    pub fn save_position(&mut self, id: CharacterId, position: Position) -> Result<(), Error> {
        position.validate()?;
        let mut next = self.state.clone();
        let character = next
            .characters
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or(Error::Invalid("character does not belong to this profile"))?;
        if character.position == position {
            // An unchanged pose still represents a checkpoint: do not claim success if
            // the last commit was uncertain or another writer replaced the saved profile.
            if self.uncertain {
                return Err(Error::CommitUncertain);
            }
            self.storage.verified_current(self.disk_hash)?;
            return Ok(());
        }
        character.position = position;
        self.commit(next)
    }
    fn commit(&mut self, mut next: State) -> Result<(), Error> {
        if self.uncertain {
            return Err(Error::CommitUncertain);
        }
        next.revision = self
            .state
            .revision
            .checked_add(1)
            .ok_or(Error::Invalid("profile revision exhausted"))?;
        next.validate(self.owner_id())?;
        let bytes = storage::encode(&self.identity, &next)?;
        if let Err(error) = self.storage.write(&bytes, Some(self.disk_hash)) {
            if matches!(error, Error::CommitUncertain) {
                self.uncertain = true;
            }
            return Err(error);
        }
        self.disk_hash = Sha256::digest(&bytes).into();
        self.state = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
