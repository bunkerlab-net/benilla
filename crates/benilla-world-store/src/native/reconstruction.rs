// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded reconstruction recipes, not restored native gameplay authority.
//! The app must execute every saved input with fresh providers and compare complete records
//! before publishing its world and providers together. Receipt replay stays private here.
use super::{Error, Identity, Receipt};
use crate::{hash, storage::Storage, Digest};
use benilla_world_runtime::native::{
    decode_records, encode_records, Checkpoint, NativeError, NativeWorld, TickRecord, MAX_BYTES,
    MAX_RECORDS,
};
use sha2::{Digest as _, Sha256};
use std::path::Path;

const MAGIC: &[u8; 8] = b"BNNREC2\0";
const LEGACY_MAGIC: &[u8; 8] = b"BNNREC1\0";
const VERSION: u32 = 2;
const HEADER: usize = 60;
const OVERHEAD: usize = HEADER + 32;
const _: () = assert!(2 * MAX_BYTES + OVERHEAD <= crate::MAX_CHECKPOINT_BYTES);

/// Inert expected data. Neither this recipe nor its checkpoint proves live query ownership.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recipe {
    anchor: Checkpoint,
    records: Vec<TickRecord>,
    expected_checkpoint: Checkpoint,
}
impl Recipe {
    pub fn anchor(&self) -> &Checkpoint {
        &self.anchor
    }
    pub fn records(&self) -> &[TickRecord] {
        &self.records
    }
    pub fn expected_checkpoint(&self) -> &Checkpoint {
        &self.expected_checkpoint
    }
}

pub struct Prepared {
    envelope: Envelope,
    bytes: Vec<u8>,
    final_world: NativeWorld,
    checkpoint: Checkpoint,
}

/// Retains a cache-free anchor and all accepted history since that anchor.
/// Only an explicit reanchor at an already durable cache-free boundary drops records.
/// It never returns its privately replayed world or resets a live native cache.
pub struct RecipeStore {
    storage: Storage,
    identity: Identity,
    generation: u64,
    digest: Digest,
    previous_digest: Option<Digest>,
    recipe: Recipe,
    final_world: NativeWorld,
    blocked: bool,
}
impl RecipeStore {
    pub fn create(path: impl AsRef<Path>, world: &NativeWorld) -> Result<(Self, Receipt), Error> {
        let checkpoint = world.checkpoint()?;
        if checkpoint.identity().tick != 0 || !world.records().is_empty() {
            return Err(crate::Error::Invalid("native recipe requires fresh genesis").into());
        }
        world.validate_saved_session_reopen()?;
        let envelope = Envelope {
            generation: 1,
            parent: [0; 32],
            anchor: checkpoint.clone(),
            records: vec![],
        };
        let bytes = envelope.encode()?;
        let mut final_world = world.clone();
        final_world.checkpoint_committed(&checkpoint)?;
        let storage = Storage::acquire(path.as_ref(), true)?;
        storage.write_checked(&bytes, None, Envelope::validate)?;
        let digest = hash(&bytes);
        Ok((
            Self {
                storage,
                identity: Identity::of(&checkpoint),
                generation: 1,
                digest,
                previous_digest: None,
                recipe: Recipe {
                    anchor: checkpoint.clone(),
                    records: vec![],
                    expected_checkpoint: checkpoint.clone(),
                },
                final_world,
                blocked: false,
            },
            Receipt {
                generation: 1,
                digest,
                checkpoint,
            },
        ))
    }

    /// Only the cache-free anchor reaches the caller's checked constructor.
    /// The private receipt replay validates expected history; it grants no playable world.
    pub fn open(
        path: impl AsRef<Path>,
        identity: Identity,
        mut restore_anchor: impl FnMut(Checkpoint) -> Result<NativeWorld, NativeError>,
    ) -> Result<(Self, Recipe), Error> {
        let storage = Storage::acquire(path.as_ref(), false)?;
        let bytes = storage.read()?;
        let envelope = Envelope::decode(&bytes)?;
        let mut final_world = envelope.restore_expected(&identity, &mut restore_anchor)?;
        let previous_digest = match storage.read_previous() {
            Ok(previous) => {
                let prior = Envelope::decode(&previous)?;
                if previous != bytes {
                    if prior.generation.checked_add(1) != Some(envelope.generation)
                        || hash(&previous) != envelope.parent
                    {
                        return Err(crate::Error::DivergentHistory.into());
                    }
                    // Validate the prior file independently, including its restored anchor.
                    let prior_world = prior.restore_expected(&identity, &mut restore_anchor)?;
                    let append = prior.anchor == envelope.anchor
                        && envelope.records.starts_with(&prior.records);
                    let reanchor = envelope.records.is_empty()
                        && envelope.anchor == prior_world.checkpoint()?;
                    if !append && !reanchor {
                        return Err(crate::Error::DivergentHistory.into());
                    }
                }
                Some(hash(&previous))
            }
            Err(crate::Error::Io(e))
                if e.kind() == std::io::ErrorKind::NotFound && envelope.generation == 1 =>
            {
                None
            }
            Err(e) => return Err(e.into()),
        };
        let checkpoint = final_world.checkpoint()?;
        final_world.checkpoint_committed(&checkpoint)?;
        let recipe = Recipe {
            anchor: envelope.anchor,
            records: envelope.records,
            expected_checkpoint: checkpoint,
        };
        Ok((
            Self {
                storage,
                identity,
                generation: envelope.generation,
                digest: hash(&bytes),
                previous_digest,
                recipe: recipe.clone(),
                final_world,
                blocked: false,
            },
            recipe,
        ))
    }

    pub fn identity(&self) -> &Identity {
        &self.identity
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn recipe(&self) -> &Recipe {
        &self.recipe
    }
    pub fn requires_reopen(&self) -> bool {
        self.blocked
    }

    pub fn prepare(&self, world: &NativeWorld) -> Result<Prepared, Error> {
        if self.blocked {
            return Err(crate::Error::CommitUncertain.into());
        }
        let checkpoint = world.checkpoint()?;
        if !self.identity.matches(&checkpoint) {
            return Err(crate::Error::Incompatible.into());
        }
        let tick = self.recipe.expected_checkpoint.identity().tick;
        let suffix: Vec<_> = world
            .records()
            .iter()
            .filter(|record| record.tick > tick)
            .cloned()
            .collect();
        if self.recipe.records.len().saturating_add(suffix.len()) > MAX_RECORDS {
            return Err(NativeError::TooLarge.into());
        }
        let mut records = self.recipe.records.clone();
        records.extend_from_slice(&suffix);
        let envelope = Envelope {
            generation: self
                .generation
                .checked_add(1)
                .ok_or(crate::Error::Invalid("native recipe generation exhausted"))?,
            parent: self.digest,
            anchor: self.recipe.anchor.clone(),
            records,
        };
        // Bound the complete retained recipe before replay or filesystem publication.
        let bytes = envelope.encode()?;
        // Exposed recipes are wire data even before close/reopen: remove process-local
        // dispatch seals rather than retaining live receipt clones from the caller.
        let envelope = Envelope::decode(&bytes)?;
        let mut final_world = self.final_world.clone();
        final_world.replay(&suffix)?;
        if final_world.checkpoint()? != checkpoint {
            return Err(crate::Error::DivergentHistory.into());
        }
        final_world.checkpoint_committed(&checkpoint)?;
        Ok(Prepared {
            envelope,
            bytes,
            final_world,
            checkpoint,
        })
    }

    /// Replace retained history with its exact already published cache-free boundary.
    /// Call `checkpoint` first and acknowledge its receipt; this never saves an unsaved
    /// suffix implicitly. The supplied live world must pass the unchanged reopen guard.
    pub fn prepare_reanchor(&self, world: &NativeWorld) -> Result<Prepared, Error> {
        if self.blocked {
            return Err(crate::Error::CommitUncertain.into());
        }
        let checkpoint = world.checkpoint()?;
        if !self.identity.matches(&checkpoint) {
            return Err(crate::Error::Incompatible.into());
        }
        if !world.records().is_empty() || checkpoint != self.recipe.expected_checkpoint {
            return Err(crate::Error::DivergentHistory.into());
        }
        world.validate_saved_session_reopen()?;
        // Keep normal preparation's complete retained bounds, private replay comparison,
        // generation checks and process-local seal removal before replacing the recipe.
        let mut prepared = self.prepare(world)?;
        prepared.envelope.anchor = checkpoint;
        prepared.envelope.records.clear();
        prepared.bytes = prepared.envelope.encode()?;
        prepared.envelope = Envelope::decode(&prepared.bytes)?;
        Ok(prepared)
    }

    pub fn commit(&mut self, prepared: Prepared) -> Result<Receipt, Error> {
        if self.blocked {
            return Err(crate::Error::CommitUncertain.into());
        }
        if prepared.envelope.parent != self.digest
            || Some(prepared.envelope.generation) != self.generation.checked_add(1)
        {
            return Err(crate::Error::StalePrepared.into());
        }
        if let Err(error) = self.storage.write_checked(
            &prepared.bytes,
            Some((self.digest, self.previous_digest)),
            Envelope::validate,
        ) {
            if matches!(
                error,
                crate::Error::CommitUncertain | crate::Error::ConcurrentModification
            ) {
                self.blocked = true;
            }
            return Err(error.into());
        }
        self.previous_digest = Some(self.digest);
        self.digest = hash(&prepared.bytes);
        self.generation = prepared.envelope.generation;
        self.recipe = Recipe {
            anchor: prepared.envelope.anchor,
            records: prepared.envelope.records,
            expected_checkpoint: prepared.checkpoint.clone(),
        };
        self.final_world = prepared.final_world;
        Ok(Receipt {
            generation: self.generation,
            digest: self.digest,
            checkpoint: prepared.checkpoint,
        })
    }

    pub fn checkpoint(&mut self, world: &mut NativeWorld) -> Result<Receipt, Error> {
        let prepared = self.prepare(world)?;
        let receipt = self.commit(prepared)?;
        receipt.acknowledge(world)?;
        Ok(receipt)
    }

    /// Publish one explicit reanchor generation, with the same atomicity as `checkpoint`.
    /// Fails without saving anything if the live boundary has not already been acknowledged.
    pub fn checkpoint_reanchor(&mut self, world: &mut NativeWorld) -> Result<Receipt, Error> {
        let prepared = self.prepare_reanchor(world)?;
        let receipt = self.commit(prepared)?;
        receipt.acknowledge(world)?;
        Ok(receipt)
    }
}

struct Envelope {
    generation: u64,
    parent: Digest,
    anchor: Checkpoint,
    records: Vec<TickRecord>,
}
impl Envelope {
    fn validate_origin(&self) -> Result<(), Error> {
        let identity = self.anchor.identity();
        if self.generation == 0
            || (self.generation == 1) != (self.parent == [0; 32])
            || (self.generation == 1
                && (identity.tick != 0
                    || identity.history_tip != [0; 32]
                    || !self.records.is_empty()))
        {
            return Err(crate::Error::Corrupt.into());
        }
        Ok(())
    }

    fn encode(&self) -> Result<Vec<u8>, Error> {
        self.validate_origin()?;
        let snapshot = self.anchor.encode()?;
        let journal = encode_records(&self.records)?;
        let mut bytes = Vec::with_capacity(OVERHEAD + snapshot.len() + journal.len());
        bytes.extend(MAGIC);
        bytes.extend(VERSION.to_le_bytes());
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
        if bytes.len() < OVERHEAD || bytes.len() > crate::MAX_CHECKPOINT_BYTES {
            return Err(crate::Error::Corrupt.into());
        }
        if &bytes[..8] == LEGACY_MAGIC {
            return Err(crate::Error::UnsupportedVersion.into());
        }
        if &bytes[..8] != MAGIC {
            return Err(crate::Error::Corrupt.into());
        }
        if u32::from_le_bytes(bytes[8..12].try_into().unwrap()) != VERSION {
            return Err(crate::Error::UnsupportedVersion.into());
        }
        let generation = u64::from_le_bytes(bytes[12..20].try_into().unwrap());
        let parent = bytes[20..52].try_into().unwrap();
        let sn = u32::from_le_bytes(bytes[52..56].try_into().unwrap()) as usize;
        let jn = u32::from_le_bytes(bytes[56..60].try_into().unwrap()) as usize;
        if sn > MAX_BYTES
            || jn > MAX_BYTES
            || bytes.len() != OVERHEAD + sn + jn
            || checksum(&bytes[..bytes.len() - 32]) != bytes[bytes.len() - 32..]
        {
            return Err(crate::Error::Corrupt.into());
        }
        let envelope = Self {
            generation,
            parent,
            anchor: Checkpoint::decode(&bytes[HEADER..HEADER + sn])?,
            records: decode_records(&bytes[HEADER + sn..HEADER + sn + jn])?,
        };
        envelope.validate_origin()?;
        Ok(envelope)
    }

    fn validate(bytes: &[u8]) -> Result<(), crate::Error> {
        Self::decode(bytes)
            .map(|_| ())
            .map_err(|_| crate::Error::Corrupt)
    }

    fn restore_expected(
        &self,
        identity: &Identity,
        restore_anchor: &mut impl FnMut(Checkpoint) -> Result<NativeWorld, NativeError>,
    ) -> Result<NativeWorld, Error> {
        if !identity.matches(&self.anchor) {
            return Err(crate::Error::Incompatible.into());
        }
        let mut world = restore_anchor(self.anchor.clone())?;
        if world.checkpoint()? != self.anchor {
            return Err(crate::Error::Incompatible.into());
        }
        world.validate_saved_session_reopen()?;
        world.checkpoint_committed(&self.anchor)?;
        world.replay(&self.records)?;
        if !identity.matches(&world.checkpoint()?) {
            return Err(crate::Error::Incompatible.into());
        }
        Ok(world)
    }
}

fn checksum(bytes: &[u8]) -> Digest {
    let mut hash = Sha256::new();
    hash.update(b"benilla-native-reconstruction-store-v2\0");
    hash.update(bytes);
    hash.finalize().into()
}

#[cfg(test)]
mod tests;
