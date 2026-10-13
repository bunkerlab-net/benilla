// SPDX-License-Identifier: GPL-3.0-or-later
//! Separate local world persistence, never character profiles or signing credentials.
//! Publication replaces one bounded envelope. Previous-good retention is a separate
//! operation, not a portable atomic multi-file transaction. See README for platform limits.
use benilla_world_data::WorldData;
use benilla_world_runtime::{Digest, JoinBoundary, Journal, Rules, World, WorldId};
use sha2::{Digest as _, Sha256};
use std::{fmt, path::Path};
mod envelope;
mod storage;
use envelope::Envelope;
use storage::Storage;

pub const FORMAT_VERSION: u32 = 1;
/// One base snapshot, its journal, fixed framing, and checksum. No JSON byte-array expansion.
pub const MAX_CHECKPOINT_BYTES: usize = benilla_world_runtime::MAX_SNAPSHOT_BYTES
    + benilla_world_runtime::MAX_JOURNAL_BYTES
    + envelope::OVERHEAD;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Invalid(&'static str),
    AlreadyExists,
    Locked,
    Corrupt,
    UnsupportedVersion,
    Incompatible,
    DivergentHistory,
    StalePrepared,
    ConcurrentModification,
    /// Publication may have occurred. Keep the runtime journal; close and reopen this store.
    CommitUncertain,
    Runtime(benilla_world_runtime::Error),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "world store I/O: {e}"),
            Self::Invalid(s) => f.write_str(s),
            Self::AlreadyExists => f.write_str("world checkpoint or recovery files already exist"),
            Self::Locked => f.write_str("world is already open in another session"),
            Self::Corrupt => f.write_str("world checkpoint is corrupt; explicit recovery required"),
            Self::UnsupportedVersion => f.write_str("unsupported world checkpoint version"),
            Self::Incompatible => f.write_str("world identity, content, or rules do not match"),
            Self::DivergentHistory => {
                f.write_str("world history diverged; no merge or replacement performed")
            }
            Self::StalePrepared => f.write_str("prepared checkpoint is stale; capture it again"),
            Self::ConcurrentModification => {
                f.write_str("world files changed externally; reopen before saving")
            }
            Self::CommitUncertain => {
                f.write_str("world commit durability is uncertain; reopen before saving")
            }
            Self::Runtime(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}
impl From<benilla_world_runtime::Error> for Error {
    fn from(value: benilla_world_runtime::Error) -> Self {
        Self::Runtime(value)
    }
}

/// The selected world is explicit and never inferred from whichever file happens to exist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldIdentity {
    pub world: WorldId,
    pub content_digest: Digest,
    pub rules_digest: Digest,
}
impl WorldIdentity {
    pub fn of(world: &World) -> Self {
        Self {
            world: world.state().world,
            content_digest: world.data().content_digest(),
            rules_digest: world.rules().digest(),
        }
    }
    fn matches(&self, world: &World) -> bool {
        *self == Self::of(world)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Durability {
    /// Both file bytes and containing directory were synced; hardware/filesystem must honor it.
    FileAndDirectorySynced,
    /// Windows std cannot portably sync directory entries. Power-loss rollback remains possible.
    FileSyncedDirectoryBestEffort,
}
fn durability() -> Durability {
    if cfg!(unix) {
        Durability::FileAndDirectorySynced
    } else {
        Durability::FileSyncedDirectoryBestEffort
    }
}

/// Only successful verified publication can construct this receipt. No deserialization API.
#[derive(Debug)]
pub struct CommitReceipt {
    generation: u64,
    checkpoint_digest: Digest,
    boundary: JoinBoundary,
    snapshot_digest: Digest,
    durability: Durability,
}
impl CommitReceipt {
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn checkpoint_digest(&self) -> Digest {
        self.checkpoint_digest
    }
    pub fn boundary(&self) -> &JoinBoundary {
        &self.boundary
    }
    pub fn durability(&self) -> Durability {
        self.durability
    }
    /// A receipt for an earlier tick or another chain cannot discard newer runtime history.
    pub fn acknowledge(&self, world: &mut World) -> Result<(), Error> {
        if world.join_boundary() != self.boundary
            || hash(&world.snapshot_bytes()?) != self.snapshot_digest
        {
            return Err(benilla_world_runtime::Error::StaleCheckpoint.into());
        }
        world.checkpoint_committed(&self.boundary)?;
        Ok(())
    }
}

/// Immutable captured state. Keep simulation paused for a synchronous checkpoint, or handle
/// stale acknowledgement explicitly; preparing never clears or mutates the runtime journal.
pub struct PreparedCheckpoint {
    parent_digest: Digest,
    generation: u64,
    envelope: Envelope,
    bytes: Vec<u8>,
    final_snapshot: Vec<u8>,
    boundary: JoinBoundary,
}

pub struct WorldStore {
    storage: Storage,
    identity: WorldIdentity,
    envelope: Envelope,
    digest: Digest,
    previous_digest: Option<Digest>,
    final_snapshot: Vec<u8>,
    boundary: JoinBoundary,
    blocked: bool,
}
impl WorldStore {
    /// Explicit new-world action, accepting only fresh genesis with no uncheckpointed ticks.
    /// The caller supplies a dedicated leaf under local state, separate from p2p-profile.
    pub fn create(path: impl AsRef<Path>, world: &World) -> Result<(Self, CommitReceipt), Error> {
        if world.state().tick != 0 || world.state().revision != 0 {
            return Err(Error::Invalid(
                "new world creation requires a fresh genesis",
            ));
        }
        let snapshot = world.snapshot_bytes()?;
        let journal = world.journal_bytes()?;
        let envelope = Envelope {
            generation: 1,
            parent_digest: [0; 32],
            snapshot: snapshot.clone(),
            journal,
        };
        let restored = envelope.restore(
            &WorldIdentity::of(world),
            world.data().clone(),
            world.rules().clone(),
        )?;
        if restored.snapshot_bytes()? != snapshot {
            return Err(Error::DivergentHistory);
        }
        let bytes = envelope.encode()?;
        let storage = Storage::acquire(path.as_ref(), true)?;
        storage.write(&bytes, None)?;
        let boundary = world.join_boundary();
        let digest = hash(&bytes);
        let receipt = receipt(envelope.generation, digest, &boundary, &snapshot);
        Ok((
            Self {
                storage,
                identity: WorldIdentity::of(world),
                envelope,
                digest,
                previous_digest: None,
                final_snapshot: snapshot,
                boundary,
                blocked: false,
            },
            receipt,
        ))
    }
    /// Never creates a replacement world, falls back to previous, or guesses content/rules.
    pub fn open(
        path: impl AsRef<Path>,
        identity: WorldIdentity,
        data: WorldData,
        rules: Rules,
    ) -> Result<(Self, World), Error> {
        if identity.world == [0; 16]
            || identity.content_digest != data.content_digest()
            || identity.rules_digest != rules.digest()
        {
            return Err(Error::Incompatible);
        }
        let storage = Storage::acquire(path.as_ref(), false)?;
        let bytes = storage.read()?;
        let envelope = Envelope::decode(&bytes)?;
        let mut world = envelope.restore(&identity, data.clone(), rules.clone())?;
        let previous_digest = match storage.read_previous() {
            Ok(previous) => {
                let prior = Envelope::decode(&previous)?;
                prior.restore(&identity, data, rules)?;
                // An interrupted backup-first save can legitimately leave duplicate copies.
                if previous != bytes
                    && (prior.generation.checked_add(1) != Some(envelope.generation)
                        || hash(&previous) != envelope.parent_digest)
                {
                    return Err(Error::DivergentHistory);
                }
                Some(hash(&previous))
            }
            Err(Error::Io(e))
                if e.kind() == std::io::ErrorKind::NotFound && envelope.generation == 1 =>
            {
                None
            }
            Err(error) => return Err(error),
        };
        let boundary = world.join_boundary();
        let final_snapshot = world.snapshot_bytes()?;
        // The validated on-disk journal is now represented by this final snapshot in memory.
        world.checkpoint_committed(&boundary)?;
        Ok((
            Self {
                storage,
                identity,
                envelope,
                digest: hash(&bytes),
                previous_digest,
                final_snapshot,
                boundary,
                blocked: false,
            },
            world,
        ))
    }
    pub fn identity(&self) -> &WorldIdentity {
        &self.identity
    }
    pub fn generation(&self) -> u64 {
        self.envelope.generation
    }
    pub fn boundary(&self) -> &JoinBoundary {
        &self.boundary
    }
    pub fn durability(&self) -> Durability {
        durability()
    }
    pub fn requires_reopen(&self) -> bool {
        self.blocked
    }

    pub fn prepare(&self, world: &World) -> Result<PreparedCheckpoint, Error> {
        if self.blocked {
            return Err(Error::CommitUncertain);
        }
        if !self.identity.matches(world) {
            return Err(Error::Incompatible);
        }
        let journal = world.journal_bytes()?;
        let candidate: Journal = serde_json::from_slice(&journal).map_err(|_| Error::Corrupt)?;
        let saved: Journal =
            serde_json::from_slice(&self.envelope.journal).map_err(|_| Error::Corrupt)?;
        let snapshot = if candidate.base == self.boundary {
            self.final_snapshot.clone()
        } else if candidate.base == saved.base
            && candidate.base_journal_tip == saved.base_journal_tip
            && candidate.records.starts_with(&saved.records)
        {
            // A successful asynchronous publication may not yet have been acknowledged.
            self.envelope.snapshot.clone()
        } else {
            return Err(Error::DivergentHistory);
        };
        let generation = self
            .envelope
            .generation
            .checked_add(1)
            .ok_or(Error::Invalid("checkpoint generation exhausted"))?;
        let envelope = Envelope {
            generation,
            parent_digest: self.digest,
            snapshot,
            journal,
        };
        let replayed =
            envelope.restore(&self.identity, world.data().clone(), world.rules().clone())?;
        let final_snapshot = world.snapshot_bytes()?;
        if replayed.snapshot_bytes()? != final_snapshot || world.state().tick < self.boundary.tick {
            return Err(Error::DivergentHistory);
        }
        let bytes = envelope.encode()?;
        Ok(PreparedCheckpoint {
            parent_digest: self.digest,
            generation,
            envelope,
            bytes,
            final_snapshot,
            boundary: world.join_boundary(),
        })
    }
    /// Verify optimistic generation and disk bytes, preserve previous-good, then publish once.
    /// On CommitUncertain/ConcurrentModification this instance latches and cannot write again.
    pub fn commit(&mut self, prepared: PreparedCheckpoint) -> Result<CommitReceipt, Error> {
        if self.blocked {
            return Err(Error::CommitUncertain);
        }
        if prepared.parent_digest != self.digest
            || Some(prepared.generation) != self.envelope.generation.checked_add(1)
        {
            return Err(Error::StalePrepared);
        }
        if let Err(error) = self
            .storage
            .write(&prepared.bytes, Some((self.digest, self.previous_digest)))
        {
            if matches!(
                error,
                Error::CommitUncertain | Error::ConcurrentModification
            ) {
                self.blocked = true;
            }
            return Err(error);
        }
        let digest = hash(&prepared.bytes);
        let receipt = receipt(
            prepared.generation,
            digest,
            &prepared.boundary,
            &prepared.final_snapshot,
        );
        self.envelope = prepared.envelope;
        self.final_snapshot = prepared.final_snapshot;
        self.boundary = prepared.boundary;
        self.previous_digest = Some(self.digest);
        self.digest = digest;
        Ok(receipt)
    }
    /// Synchronous convenience: preparation, durable publication, then exact acknowledgement.
    pub fn checkpoint(&mut self, world: &mut World) -> Result<CommitReceipt, Error> {
        let prepared = self.prepare(world)?;
        let receipt = self.commit(prepared)?;
        receipt.acknowledge(world)?;
        Ok(receipt)
    }
}
fn receipt(
    generation: u64,
    checkpoint_digest: Digest,
    boundary: &JoinBoundary,
    snapshot: &[u8],
) -> CommitReceipt {
    CommitReceipt {
        generation,
        checkpoint_digest,
        boundary: boundary.clone(),
        snapshot_digest: hash(snapshot),
        durability: durability(),
    }
}
fn hash(bytes: &[u8]) -> Digest {
    Sha256::digest(bytes).into()
}

#[cfg(test)]
mod tests;
