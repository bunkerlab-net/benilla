// SPDX-License-Identifier: GPL-3.0-or-later
//! Durable native-state checkpoints. Decoding grants no native gameplay capability.
//! The caller must restore through the runtime's admitted profile/domain constructor.
use super::{durability, hash, storage::Storage, Digest, Durability};
use benilla_world_runtime::native::{
    decode_records, encode_records, Checkpoint, CheckpointIdentity, Domain, NativeError,
    NativeWorld, TickRecord, MAX_BYTES,
};
use benilla_world_runtime::{PlayerId, WorldId};
use sha2::{Digest as _, Sha256};
use std::{fmt, path::Path};

const MAGIC: &[u8; 8] = b"BNNATV1\0";
const HEADER: usize = 60;
const OVERHEAD: usize = HEADER + 32;
const _: () = assert!(2 * MAX_BYTES + OVERHEAD <= super::MAX_CHECKPOINT_BYTES);

#[derive(Debug)]
pub enum Error {
    Store(super::Error),
    Runtime(NativeError),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(e) => e.fmt(f),
            Self::Runtime(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for Error {}
impl From<super::Error> for Error {
    fn from(e: super::Error) -> Self {
        Self::Store(e)
    }
}
impl From<NativeError> for Error {
    fn from(e: NativeError) -> Self {
        Self::Runtime(e)
    }
}

/// Selected identity includes owner and character, plus the entire admitted profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub world: WorldId,
    pub player: PlayerId,
    pub profile: Digest,
    pub domain: Domain,
}
impl Identity {
    pub fn of(checkpoint: &Checkpoint) -> Self {
        let i = checkpoint.identity();
        Self {
            world: i.world,
            player: i.player,
            profile: i.profile,
            domain: i.domain,
        }
    }
    fn matches(&self, checkpoint: &Checkpoint) -> bool {
        *self == Self::of(checkpoint)
    }
}

/// Only verified successful publication creates this non-deserializable receipt.
pub struct Receipt {
    generation: u64,
    digest: Digest,
    checkpoint: Checkpoint,
}
impl Receipt {
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn digest(&self) -> Digest {
        self.digest
    }
    pub fn boundary(&self) -> CheckpointIdentity {
        self.checkpoint.identity()
    }
    pub fn durability(&self) -> Durability {
        durability()
    }
    pub fn acknowledge(&self, world: &mut NativeWorld) -> Result<(), Error> {
        world.checkpoint_committed(&self.checkpoint)?;
        Ok(())
    }
}

/// Only the store's replay-verified durable world can construct this result.
/// Envelope digest/generation and exact boundary accompany the original operation receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DurableAdminReceipt {
    receipt: benilla_world_runtime::native::AdminReceipt,
    generation: u64,
    envelope_digest: Digest,
    boundary: CheckpointIdentity,
}
impl DurableAdminReceipt {
    pub fn receipt(&self) -> &benilla_world_runtime::native::AdminReceipt {
        &self.receipt
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn envelope_digest(&self) -> Digest {
        self.envelope_digest
    }
    pub fn boundary(&self) -> &CheckpointIdentity {
        &self.boundary
    }
}

pub struct Prepared {
    envelope: Envelope,
    bytes: Vec<u8>,
    final_world: NativeWorld,
}

pub struct NativeStore {
    storage: Storage,
    identity: Identity,
    generation: u64,
    digest: Digest,
    previous_digest: Option<Digest>,
    // Admitted and replay-verified base, always journal-cleared after durable publication.
    final_world: NativeWorld,
    committed_boundary: CheckpointIdentity,
    blocked: bool,
}
impl NativeStore {
    pub fn create(path: impl AsRef<Path>, world: &NativeWorld) -> Result<(Self, Receipt), Error> {
        let checkpoint = world.checkpoint()?;
        if checkpoint.identity().tick != 0 || !world.records().is_empty() {
            return Err(super::Error::Invalid("native creation requires fresh genesis").into());
        }
        let envelope = Envelope {
            generation: 1,
            parent: [0; 32],
            base: checkpoint.clone(),
            records: vec![],
        };
        let bytes = envelope.encode()?;
        let storage = Storage::acquire(path.as_ref(), true)?;
        storage.write_checked(&bytes, None, Envelope::validate)?;
        let digest = hash(&bytes);
        let mut final_world = world.clone();
        final_world.checkpoint_committed(&checkpoint)?;
        Ok((
            Self {
                storage,
                identity: Identity::of(&checkpoint),
                generation: 1,
                digest,
                previous_digest: None,
                final_world,
                committed_boundary: checkpoint.identity(),
                blocked: false,
            },
            Receipt {
                generation: 1,
                digest,
                checkpoint,
            },
        ))
    }

    /// Restore is a local capability-bearing function, never supplied by network input.
    /// It must validate the selected profile and checkpoint through the runtime constructor.
    /// The store independently checks identity and byte-exact restored state before replay.
    pub fn open(
        path: impl AsRef<Path>,
        identity: Identity,
        mut restore: impl FnMut(Checkpoint) -> Result<NativeWorld, NativeError>,
    ) -> Result<(Self, NativeWorld), Error> {
        let storage = Storage::acquire(path.as_ref(), false)?;
        let bytes = storage.read()?;
        let envelope = Envelope::decode(&bytes)?;
        let mut world = envelope.restore(&identity, &mut restore)?;
        let previous_digest = match storage.read_previous() {
            Ok(previous) => {
                let prior = Envelope::decode(&previous)?;
                let prior_world = prior.restore(&identity, &mut restore)?;
                if previous != bytes
                    && (prior.generation.checked_add(1) != Some(envelope.generation)
                        || hash(&previous) != envelope.parent
                        || prior_world.checkpoint()? != envelope.base)
                {
                    return Err(super::Error::DivergentHistory.into());
                }
                Some(hash(&previous))
            }
            Err(super::Error::Io(e))
                if e.kind() == std::io::ErrorKind::NotFound && envelope.generation == 1 =>
            {
                None
            }
            Err(e) => return Err(e.into()),
        };
        let checkpoint = world.checkpoint()?;
        world.checkpoint_committed(&checkpoint)?;
        Ok((
            Self {
                storage,
                identity,
                generation: envelope.generation,
                digest: hash(&bytes),
                previous_digest,
                final_world: world.clone(),
                committed_boundary: checkpoint.identity(),
                blocked: false,
            },
            world,
        ))
    }
    pub fn identity(&self) -> &Identity {
        &self.identity
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    /// Reads the successfully published (or opened and replay-verified) boundary, never
    /// the caller's potentially newer unsaved simulation state.
    pub fn committed_admin_snapshot(
        &self,
        authenticated_owner: benilla_world_runtime::OwnerId,
    ) -> Result<
        benilla_world_runtime::native::AdminSnapshot,
        benilla_world_runtime::native::AdminRejection,
    > {
        self.final_world.admin_snapshot(authenticated_owner)
    }
    pub fn committed_admin_receipt(
        &self,
        request: &benilla_world_runtime::native::AdminRequest,
    ) -> Option<DurableAdminReceipt> {
        let receipt = self.final_world.admin_receipt(&request.key)?;
        if &receipt.request != request {
            return None;
        }
        Some(DurableAdminReceipt {
            receipt: receipt.clone(),
            generation: self.generation,
            envelope_digest: self.digest,
            boundary: self.committed_boundary.clone(),
        })
    }
    pub fn requires_reopen(&self) -> bool {
        self.blocked
    }

    pub fn prepare(&self, world: &NativeWorld) -> Result<Prepared, Error> {
        if self.blocked {
            return Err(super::Error::CommitUncertain.into());
        }
        let expected = world.checkpoint()?;
        if !self.identity.matches(&expected) {
            return Err(super::Error::Incompatible.into());
        }
        let base = self.final_world.checkpoint()?;
        let tick = base.identity().tick;
        // A prior successful commit may not yet have been acknowledged. Only its verified
        // suffix is needed; replay binds the exact before-state and history tip at our base.
        let records: Vec<_> = world
            .records()
            .iter()
            .filter(|r| r.tick > tick)
            .cloned()
            .collect();
        let mut final_world = self.final_world.clone();
        final_world.replay(&records)?;
        if final_world.checkpoint()? != expected {
            return Err(super::Error::DivergentHistory.into());
        }
        let envelope = Envelope {
            generation: self
                .generation
                .checked_add(1)
                .ok_or(super::Error::Invalid("native generation exhausted"))?,
            parent: self.digest,
            base,
            records,
        };
        let bytes = envelope.encode()?;
        Ok(Prepared {
            envelope,
            bytes,
            final_world,
        })
    }
    pub fn commit(&mut self, prepared: Prepared) -> Result<Receipt, Error> {
        if self.blocked {
            return Err(super::Error::CommitUncertain.into());
        }
        if prepared.envelope.parent != self.digest
            || Some(prepared.envelope.generation) != self.generation.checked_add(1)
        {
            return Err(super::Error::StalePrepared.into());
        }
        // Finish all fallible runtime capture before touching the filesystem.
        let checkpoint = prepared.final_world.checkpoint()?;
        if let Err(e) = self.storage.write_checked(
            &prepared.bytes,
            Some((self.digest, self.previous_digest)),
            Envelope::validate,
        ) {
            if matches!(
                e,
                super::Error::CommitUncertain | super::Error::ConcurrentModification
            ) {
                self.blocked = true;
            }
            return Err(e.into());
        }
        let digest = hash(&prepared.bytes);
        self.previous_digest = Some(self.digest);
        self.digest = digest;
        self.generation = prepared.envelope.generation;
        self.final_world = prepared.final_world;
        self.final_world.checkpoint_committed(&checkpoint)?;
        self.committed_boundary = checkpoint.identity();
        Ok(Receipt {
            generation: self.generation,
            digest,
            checkpoint,
        })
    }
    pub fn checkpoint(&mut self, world: &mut NativeWorld) -> Result<Receipt, Error> {
        let prepared = self.prepare(world)?;
        let receipt = self.commit(prepared)?;
        receipt.acknowledge(world)?;
        Ok(receipt)
    }
}

struct Envelope {
    generation: u64,
    parent: Digest,
    base: Checkpoint,
    records: Vec<TickRecord>,
}
impl Envelope {
    fn validate_origin(&self) -> Result<(), Error> {
        let i = self.base.identity();
        if self.generation == 0
            || (self.generation == 1) != (self.parent == [0; 32])
            || (self.generation == 1
                && (i.tick != 0 || i.history_tip != [0; 32] || !self.records.is_empty()))
        {
            return Err(super::Error::Corrupt.into());
        }
        Ok(())
    }
    fn encode(&self) -> Result<Vec<u8>, Error> {
        let snapshot = self.base.encode()?;
        let journal = encode_records(&self.records)?;
        self.validate_origin()?;
        let mut bytes = Vec::with_capacity(OVERHEAD + snapshot.len() + journal.len());
        bytes.extend(MAGIC);
        bytes.extend(1u32.to_le_bytes());
        bytes.extend(self.generation.to_le_bytes());
        bytes.extend(self.parent);
        bytes.extend((snapshot.len() as u32).to_le_bytes());
        bytes.extend((journal.len() as u32).to_le_bytes());
        bytes.extend(snapshot);
        bytes.extend(journal);
        bytes.extend(checksum(&bytes));
        Ok(bytes)
    }
    fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < OVERHEAD
            || bytes.len() > super::MAX_CHECKPOINT_BYTES
            || &bytes[..8] != MAGIC
        {
            return Err(super::Error::Corrupt.into());
        }
        if u32::from_le_bytes(bytes[8..12].try_into().unwrap()) != 1 {
            return Err(super::Error::UnsupportedVersion.into());
        }
        let generation = u64::from_le_bytes(bytes[12..20].try_into().unwrap());
        let parent = bytes[20..52].try_into().unwrap();
        let sn = u32::from_le_bytes(bytes[52..56].try_into().unwrap()) as usize;
        let jn = u32::from_le_bytes(bytes[56..60].try_into().unwrap()) as usize;
        if generation == 0
            || (generation == 1) != (parent == [0; 32])
            || sn > MAX_BYTES
            || jn > MAX_BYTES
            || bytes.len() != OVERHEAD + sn + jn
            || checksum(&bytes[..bytes.len() - 32]) != bytes[bytes.len() - 32..]
        {
            return Err(super::Error::Corrupt.into());
        }
        let value = Self {
            generation,
            parent,
            base: Checkpoint::decode(&bytes[HEADER..HEADER + sn])?,
            records: decode_records(&bytes[HEADER + sn..HEADER + sn + jn])?,
        };
        value.validate_origin()?;
        Ok(value)
    }
    fn validate(bytes: &[u8]) -> Result<(), super::Error> {
        Self::decode(bytes)
            .map(|_| ())
            .map_err(|_| super::Error::Corrupt)
    }
    fn restore(
        &self,
        identity: &Identity,
        restore: &mut impl FnMut(Checkpoint) -> Result<NativeWorld, NativeError>,
    ) -> Result<NativeWorld, Error> {
        if !identity.matches(&self.base) {
            return Err(super::Error::Incompatible.into());
        }
        let mut world = restore(self.base.clone())?;
        if world.checkpoint()? != self.base {
            return Err(super::Error::Incompatible.into());
        }
        world.replay(&self.records)?;
        if !identity.matches(&world.checkpoint()?) {
            return Err(super::Error::Incompatible.into());
        }
        Ok(world)
    }
}
fn checksum(bytes: &[u8]) -> Digest {
    let mut h = Sha256::new();
    h.update(b"benilla-native-store-v1\0");
    h.update(bytes);
    h.finalize().into()
}

#[cfg(test)]
mod tests;
