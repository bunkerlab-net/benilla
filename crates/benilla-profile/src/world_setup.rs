//! Explicit saved local-world declarations, independent of exploration characters.
//!
//! These are intentions for a future validated native genesis, never gameplay admission or
//! grants. The world ID is freshly generated here; no API claims an existing/foreign world.
//! Policies are immutable: changing admins requires creating a distinct world declaration.
//! Local owners can roll back/copy their directory; signatures do not provide anti-rollback.
use crate::{owner_id_from_public_key, Error, OwnerId, ProfileStore};
use ed25519_dalek::{Signature, Signer, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub type WorldId = [u8; 16];
pub const MAX_WORLDS: usize = 16;
pub const MAX_ADMINS: usize = 64;
pub const MAX_SETUP_RECEIPTS: usize = 256;
pub const MAX_SETUP_BYTES: u64 = 256 * 1024;
/// Public correlation nonce for a setup session; it carries no authority or secret.
pub fn new_request_session() -> Result<[u8; 16], Error> {
    loop {
        let mut session = [0; 16];
        getrandom::fill(&mut session)
            .map_err(|_| Error::Invalid("secure random source unavailable"))?;
        if session != [0; 16] {
            return Ok(session);
        }
    }
}
const DECLARATION_DOMAIN: &[u8] = b"benilla-local-world-declaration-v1\0";
const STORE_DOMAIN: &[u8] = b"benilla-local-world-setups-v1\0";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldPolicy {
    pub world: WorldId,
    pub world_owner: OwnerId,
    pub allowlist: BTreeSet<OwnerId>,
    pub revision: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldDeclaration {
    pub format_version: u32,
    pub label: String,
    pub public_key: [u8; 32],
    pub policy: WorldPolicy,
    signature: Vec<u8>,
}
impl WorldDeclaration {
    fn message(&self) -> Result<Vec<u8>, Error> {
        let value = (
            &self.format_version,
            &self.label,
            &self.public_key,
            &self.policy,
        );
        message(DECLARATION_DOMAIN, &value)
    }
    /// Must be checked before binding the exact digest into a future native genesis/profile.
    /// Signature validity establishes this local declaration, not ownership of another world.
    pub fn verify(&self) -> Result<(), Error> {
        validate_label(&self.label)?;
        validate_admins(self.policy.world_owner, &self.policy.allowlist)?;
        if self.format_version != 1
            || self.policy.world == [0; 16]
            || self.policy.revision != 1
            || self.policy.world_owner != owner_id_from_public_key(&self.public_key)
        {
            return Err(Error::Invalid("invalid local world declaration"));
        }
        verify(&self.public_key, &self.message()?, &self.signature)
    }
    pub fn digest(&self) -> Result<[u8; 32], Error> {
        self.verify()?;
        Ok(Sha256::digest(message(DECLARATION_DOMAIN, self)?).into())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupRequestId {
    /// Fresh per-setup-session public correlation nonce, not a credential.
    pub session: [u8; 16],
    pub sequence: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SetupOperation {
    Create {
        label: String,
        allowlist: BTreeSet<OwnerId>,
    },
    Select {
        world: WorldId,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupRequest {
    pub id: SetupRequestId,
    pub expected_revision: u64,
    pub operation: SetupOperation,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    request: SetupRequest,
    world: WorldId,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupSnapshot {
    pub revision: u64,
    pub selected: Option<WorldId>,
    pub worlds: Vec<WorldDeclaration>,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    snapshot: SetupSnapshot,
    receipts: Vec<Receipt>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    format_version: u32,
    owner: OwnerId,
    public_key: [u8; 32],
    state: State,
    signature: Vec<u8>,
}
impl Envelope {
    fn message(&self) -> Result<Vec<u8>, Error> {
        message(
            STORE_DOMAIN,
            &(
                &self.format_version,
                &self.owner,
                &self.public_key,
                &self.state,
            ),
        )
    }
}
fn message(domain: &[u8], value: &impl Serialize) -> Result<Vec<u8>, Error> {
    let mut bytes = domain.to_vec();
    bytes.extend(serde_json::to_vec(value).map_err(|_| Error::Corrupt)?);
    Ok(bytes)
}
fn verify(public_key: &[u8; 32], message: &[u8], signature: &[u8]) -> Result<(), Error> {
    let signature: &[u8; 64] = signature.try_into().map_err(|_| Error::Corrupt)?;
    VerifyingKey::from_bytes(public_key)
        .and_then(|key| key.verify_strict(message, &Signature::from_bytes(signature)))
        .map_err(|_| Error::Corrupt)
}
fn validate_label(label: &str) -> Result<(), Error> {
    if label.is_empty()
        || label.len() > 40
        || label.trim() != label
        || !label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b' ' | b'-' | b'_'))
    {
        return Err(Error::Invalid(
            "world name needs 1-40 letters, numbers, spaces, - or _",
        ));
    }
    Ok(())
}
fn validate_admins(owner: OwnerId, admins: &BTreeSet<OwnerId>) -> Result<(), Error> {
    if owner == [0; 16]
        || admins.len() > MAX_ADMINS
        || admins.contains(&[0; 16])
        || admins.contains(&owner)
    {
        return Err(Error::Invalid(
            "use at most 64 distinct nonzero other admin owner IDs",
        ));
    }
    Ok(())
}
fn validate_request(request: &SetupRequest, owner: OwnerId) -> Result<(), Error> {
    if request.id.session == [0; 16] || request.id.sequence == 0 {
        return Err(Error::Invalid("invalid world setup request ID"));
    }
    match &request.operation {
        SetupOperation::Create { label, allowlist } => {
            validate_label(label)?;
            validate_admins(owner, allowlist)
        }
        SetupOperation::Select { world } if *world != [0; 16] => Ok(()),
        _ => Err(Error::Invalid("invalid local world selection")),
    }
}
impl State {
    fn validate(&self, key: [u8; 32], owner: OwnerId) -> Result<(), Error> {
        let snapshot = &self.snapshot;
        if snapshot.worlds.len() > MAX_WORLDS
            || self.receipts.len() > MAX_SETUP_RECEIPTS
            || snapshot.revision != self.receipts.len() as u64
            || snapshot.worlds.len() > self.receipts.len()
        {
            return Err(Error::Corrupt);
        }
        let mut worlds = BTreeSet::new();
        for world in &snapshot.worlds {
            world.verify()?;
            if world.public_key != key
                || world.policy.world_owner != owner
                || !worlds.insert(world.policy.world)
            {
                return Err(Error::Corrupt);
            }
        }
        if snapshot.selected.is_some_and(|id| !worlds.contains(&id)) {
            return Err(Error::Corrupt);
        }
        let mut requests = BTreeSet::new();
        let mut created = BTreeSet::new();
        for (index, receipt) in self.receipts.iter().enumerate() {
            validate_request(&receipt.request, owner)?;
            if receipt.request.expected_revision != index as u64
                || !requests.insert((receipt.request.id.session, receipt.request.id.sequence))
                || !worlds.contains(&receipt.world)
            {
                return Err(Error::Corrupt);
            }
            match &receipt.request.operation {
                SetupOperation::Create { label, allowlist } => {
                    let world = snapshot
                        .worlds
                        .iter()
                        .find(|w| w.policy.world == receipt.world)
                        .ok_or(Error::Corrupt)?;
                    if world.label != *label
                        || world.policy.allowlist != *allowlist
                        || !created.insert(receipt.world)
                    {
                        return Err(Error::Corrupt);
                    }
                }
                SetupOperation::Select { world }
                    if *world == receipt.world && created.contains(world) => {}
                _ => return Err(Error::Corrupt),
            }
        }
        if worlds != created || snapshot.selected != self.receipts.last().map(|r| r.world) {
            return Err(Error::Corrupt);
        }
        Ok(())
    }
}

impl ProfileStore {
    fn read_world_setups(&mut self) -> Result<(State, Option<[u8; 32]>), Error> {
        if self.uncertain {
            return Err(Error::CommitUncertain);
        }
        // Never sign with a stale profile identity after its persisted file has been replaced.
        if <[u8; 32]>::from(Sha256::digest(&self.storage.read()?)) != self.disk_hash {
            return Err(Error::ConcurrentModification);
        }
        let Some(bytes) = self.storage.read_world_setups()? else {
            return Ok((State::default(), None));
        };
        let envelope: Envelope = serde_json::from_slice(&bytes).map_err(|_| Error::Corrupt)?;
        if envelope.format_version != 1 {
            return Err(Error::UnsupportedVersion);
        }
        if envelope.owner != self.owner_id() || envelope.public_key != self.public_key() {
            return Err(Error::Corrupt);
        }
        verify(
            &envelope.public_key,
            &envelope.message()?,
            &envelope.signature,
        )?;
        envelope
            .state
            .validate(self.public_key(), self.owner_id())?;
        Ok((envelope.state, Some(Sha256::digest(&bytes).into())))
    }
    pub fn world_setups(&mut self) -> Result<SetupSnapshot, Error> {
        self.read_world_setups().map(|(state, _)| state.snapshot)
    }
    /// Atomically save declaration, explicit selection and replay receipt in a separate file.
    /// Reusing a request ID with identical data returns its saved result; conflicting data fails.
    pub fn apply_world_setup(&mut self, request: SetupRequest) -> Result<WorldId, Error> {
        validate_request(&request, self.owner_id())?;
        let (mut state, expected) = self.read_world_setups()?;
        if let Some(receipt) = state.receipts.iter().find(|r| r.request.id == request.id) {
            return if receipt.request == request {
                Ok(receipt.world)
            } else {
                Err(Error::Invalid("world setup request ID conflict"))
            };
        }
        if request.expected_revision != state.snapshot.revision {
            return Err(Error::ConcurrentModification);
        }
        if state.receipts.len() >= MAX_SETUP_RECEIPTS {
            return Err(Error::Invalid("world setup history is full"));
        }
        let world = match &request.operation {
            SetupOperation::Create { label, allowlist } => {
                if state.snapshot.worlds.len() >= MAX_WORLDS {
                    return Err(Error::Invalid("at most 16 local world setups may be saved"));
                }
                let world = loop {
                    let mut id = [0; 16];
                    getrandom::fill(&mut id)
                        .map_err(|_| Error::Invalid("secure random source unavailable"))?;
                    if id != [0; 16] && !state.snapshot.worlds.iter().any(|w| w.policy.world == id)
                    {
                        break id;
                    }
                };
                let mut declaration = WorldDeclaration {
                    format_version: 1,
                    label: label.clone(),
                    public_key: self.public_key(),
                    policy: WorldPolicy {
                        world,
                        world_owner: self.owner_id(),
                        allowlist: allowlist.clone(),
                        revision: 1,
                    },
                    signature: Vec::new(),
                };
                declaration.signature = self
                    .identity
                    .key
                    .sign(&declaration.message()?)
                    .to_bytes()
                    .to_vec();
                state.snapshot.worlds.push(declaration);
                world
            }
            SetupOperation::Select { world } => {
                if !state
                    .snapshot
                    .worlds
                    .iter()
                    .any(|w| w.policy.world == *world)
                {
                    return Err(Error::Invalid("choose a world saved by this profile"));
                }
                *world
            }
        };
        state.snapshot.selected = Some(world);
        state.snapshot.revision += 1;
        state.receipts.push(Receipt { request, world });
        state.validate(self.public_key(), self.owner_id())?;
        let mut envelope = Envelope {
            format_version: 1,
            owner: self.owner_id(),
            public_key: self.public_key(),
            state,
            signature: Vec::new(),
        };
        envelope.signature = self
            .identity
            .key
            .sign(&envelope.message()?)
            .to_bytes()
            .to_vec();
        let bytes = serde_json::to_vec(&envelope).map_err(|_| Error::Corrupt)?;
        if bytes.len() as u64 > MAX_SETUP_BYTES {
            return Err(Error::Invalid("world setup file exceeds its size limit"));
        }
        if let Err(error) = self.storage.write_world_setups(&bytes, expected) {
            if matches!(error, Error::CommitUncertain) {
                self.uncertain = true;
            }
            return Err(error);
        }
        Ok(world)
    }
}

#[cfg(test)]
mod tests;
