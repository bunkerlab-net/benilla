// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded current checkpoints and inert app-owned Wander reconstruction data.
//!
//! This store neither restores a world nor interprets a recipe. The app must validate its
//! typed witness before saving, execute fresh native reconstruction when opening, and bind
//! that result to the exact checkpoint before publishing runtime and provider ownership.
use super::{Error, Identity, Receipt};
use crate::{hash, storage::Storage, Digest};
use benilla_world_runtime::native::{Checkpoint, NativeError, NativeWorld, NATIVE_FORMAT_VERSION};
use sha2::{Digest as _, Sha256};
use std::path::Path;

const MAGIC: &[u8; 8] = b"BNNWND1\0";
const VERSION: u32 = 1;
const HEADER: usize = 60;
const OVERHEAD: usize = HEADER + 32;
/// Combined canonical checkpoint and opaque recipe payload, excluding fixed framing.
pub const MAX_PAYLOAD_BYTES: usize = 8 * 1024 * 1024;
const _: () = assert!(MAX_PAYLOAD_BYTES + OVERHEAD <= crate::MAX_CHECKPOINT_BYTES);

/// Untrusted saved expectations. Possession of these bytes grants no runtime capability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recipe {
    checkpoint: Checkpoint,
    recipe_bytes: Vec<u8>,
}
impl Recipe {
    pub fn checkpoint(&self) -> &Checkpoint {
        &self.checkpoint
    }
    pub fn recipe_bytes(&self) -> &[u8] {
        &self.recipe_bytes
    }
}

/// A captured live checkpoint, bound to one exact predecessor generation.
pub struct Prepared {
    envelope: Envelope,
    bytes: Vec<u8>,
}

/// Exclusive-writer storage containing no restored or privately replayed native world.
/// Only the current bounded snapshot and recipe survive a successful publication.
pub struct WanderStore {
    storage: Storage,
    identity: Identity,
    generation: u64,
    digest: Digest,
    previous_digest: Option<Digest>,
    recipe: Recipe,
    blocked: bool,
}
impl WanderStore {
    /// Start from the actual current world, including a nonzero admitted Wander boundary.
    /// The caller owns typed witness validation. No saved-session reopen guard is relaxed.
    pub fn create(
        path: impl AsRef<Path>,
        world: &mut NativeWorld,
        recipe_bytes: &[u8],
    ) -> Result<(Self, Receipt), Error> {
        let checkpoint = world.checkpoint()?;
        let envelope = Envelope {
            generation: 1,
            parent: [0; 32],
            recipe: Recipe {
                checkpoint,
                recipe_bytes: bounded_recipe_bytes(recipe_bytes)?,
            },
        };
        let bytes = envelope.encode()?;
        let storage = Storage::acquire(path.as_ref(), true)?;
        storage.write_checked(&bytes, None, Envelope::validate)?;
        let digest = hash(&bytes);
        let receipt = Receipt {
            generation: 1,
            digest,
            checkpoint: envelope.recipe.checkpoint.clone(),
        };
        receipt.acknowledge(world)?;
        Ok((
            Self {
                storage,
                identity: Identity::of(&envelope.recipe.checkpoint),
                generation: 1,
                digest,
                previous_digest: None,
                recipe: envelope.recipe,
                blocked: false,
            },
            receipt,
        ))
    }

    /// Acquire the lock and decode inert data only. There is deliberately no constructor
    /// callback and no NativeWorld result, even for a valid, checksummed recipe.
    pub fn open(path: impl AsRef<Path>, identity: Identity) -> Result<(Self, Recipe), Error> {
        let storage = Storage::acquire(path.as_ref(), false)?;
        let bytes = storage.read()?;
        let envelope = Envelope::decode(&bytes)?;
        if !identity.matches(&envelope.recipe.checkpoint) {
            return Err(crate::Error::Incompatible.into());
        }
        let previous_digest = match storage.read_previous() {
            Ok(previous) => {
                let prior = Envelope::decode(&previous)?;
                if !identity.matches(&prior.recipe.checkpoint) {
                    return Err(crate::Error::Incompatible.into());
                }
                // A failed backup-first write may leave an exact duplicate of current.
                if previous != bytes {
                    if prior.generation.checked_add(1) != Some(envelope.generation)
                        || hash(&previous) != envelope.parent
                    {
                        return Err(crate::Error::DivergentHistory.into());
                    }
                    validate_successor(&prior.recipe.checkpoint, &envelope.recipe.checkpoint)?;
                }
                Some(hash(&previous))
            }
            Err(crate::Error::Io(error))
                if error.kind() == std::io::ErrorKind::NotFound && envelope.generation == 1 =>
            {
                None
            }
            Err(error) => return Err(error.into()),
        };
        let recipe = envelope.recipe;
        Ok((
            Self {
                storage,
                identity,
                generation: envelope.generation,
                digest: hash(&bytes),
                previous_digest,
                recipe: recipe.clone(),
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

    pub fn prepare(&self, world: &NativeWorld, recipe_bytes: &[u8]) -> Result<Prepared, Error> {
        if self.blocked {
            return Err(crate::Error::CommitUncertain.into());
        }
        let checkpoint = world.checkpoint()?;
        if !self.identity.matches(&checkpoint) {
            return Err(crate::Error::Incompatible.into());
        }
        validate_live_suffix(&self.recipe.checkpoint, &checkpoint, world)?;
        let envelope = Envelope {
            generation: self
                .generation
                .checked_add(1)
                .ok_or(crate::Error::Invalid("Wander store generation exhausted"))?,
            parent: self.digest,
            recipe: Recipe {
                checkpoint,
                recipe_bytes: bounded_recipe_bytes(recipe_bytes)?,
            },
        };
        let bytes = envelope.encode()?;
        Ok(Prepared { envelope, bytes })
    }

    /// Publication alone never clears the caller's journal; the receipt acknowledges only
    /// its byte-exact live checkpoint, so a newer tick cannot be discarded by a stale save.
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
        self.recipe = prepared.envelope.recipe;
        Ok(Receipt {
            generation: self.generation,
            digest: self.digest,
            checkpoint: self.recipe.checkpoint.clone(),
        })
    }

    pub fn checkpoint(
        &mut self,
        world: &mut NativeWorld,
        recipe_bytes: &[u8],
    ) -> Result<Receipt, Error> {
        let prepared = self.prepare(world, recipe_bytes)?;
        let receipt = self.commit(prepared)?;
        receipt.acknowledge(world)?;
        Ok(receipt)
    }
}

fn bounded_recipe_bytes(bytes: &[u8]) -> Result<Vec<u8>, Error> {
    if bytes.len() > MAX_PAYLOAD_BYTES {
        return Err(NativeError::TooLarge.into());
    }
    Ok(bytes.to_vec())
}

fn validate_successor(prior: &Checkpoint, next: &Checkpoint) -> Result<(), Error> {
    let old = prior.identity();
    let new = next.identity();
    if new.tick < old.tick || (new.tick == old.tick && prior != next) {
        return Err(crate::Error::DivergentHistory.into());
    }
    Ok(())
}

fn validate_live_suffix(
    prior: &Checkpoint,
    next: &Checkpoint,
    world: &NativeWorld,
) -> Result<(), Error> {
    validate_successor(prior, next)?;
    let mut boundary = prior.identity();
    let saved_tick = boundary.tick;
    for record in world
        .records()
        .iter()
        .filter(|record| record.tick > saved_tick)
    {
        if record.version != NATIVE_FORMAT_VERSION
            || record.profile != boundary.profile
            || boundary.tick.checked_add(1) != Some(record.tick)
            || record.before != boundary.state_digest
            || record.previous_record != boundary.history_tip
        {
            return Err(crate::Error::DivergentHistory.into());
        }
        boundary.tick = record.tick;
        boundary.state_digest = record.after;
        boundary.history_tip = record.digest();
    }
    if boundary != next.identity() {
        return Err(crate::Error::DivergentHistory.into());
    }
    Ok(())
}

struct Envelope {
    generation: u64,
    parent: Digest,
    recipe: Recipe,
}
impl Envelope {
    fn validate_origin(&self) -> Result<(), Error> {
        if self.generation == 0 || (self.generation == 1) != (self.parent == [0; 32]) {
            return Err(crate::Error::Corrupt.into());
        }
        Ok(())
    }

    fn encode(&self) -> Result<Vec<u8>, Error> {
        self.validate_origin()?;
        let snapshot = self.recipe.checkpoint.encode()?;
        if snapshot
            .len()
            .saturating_add(self.recipe.recipe_bytes.len())
            > MAX_PAYLOAD_BYTES
        {
            return Err(NativeError::TooLarge.into());
        }
        let mut bytes =
            Vec::with_capacity(OVERHEAD + snapshot.len() + self.recipe.recipe_bytes.len());
        bytes.extend(MAGIC);
        bytes.extend(VERSION.to_le_bytes());
        bytes.extend(self.generation.to_le_bytes());
        bytes.extend(self.parent);
        bytes.extend((snapshot.len() as u32).to_le_bytes());
        bytes.extend((self.recipe.recipe_bytes.len() as u32).to_le_bytes());
        bytes.extend(snapshot);
        bytes.extend(&self.recipe.recipe_bytes);
        bytes.extend(checksum(&bytes));
        Ok(bytes)
    }

    fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < OVERHEAD
            || bytes.len() > MAX_PAYLOAD_BYTES + OVERHEAD
            || &bytes[..8] != MAGIC
        {
            return Err(crate::Error::Corrupt.into());
        }
        if u32::from_le_bytes(bytes[8..12].try_into().unwrap()) != VERSION {
            return Err(crate::Error::UnsupportedVersion.into());
        }
        let generation = u64::from_le_bytes(bytes[12..20].try_into().unwrap());
        let parent = bytes[20..52].try_into().unwrap();
        let sn = u32::from_le_bytes(bytes[52..56].try_into().unwrap()) as usize;
        let rn = u32::from_le_bytes(bytes[56..60].try_into().unwrap()) as usize;
        if sn > MAX_PAYLOAD_BYTES
            || rn > MAX_PAYLOAD_BYTES
            || sn.saturating_add(rn) > MAX_PAYLOAD_BYTES
            || bytes.len() != OVERHEAD + sn + rn
            || checksum(&bytes[..bytes.len() - 32]) != bytes[bytes.len() - 32..]
        {
            return Err(crate::Error::Corrupt.into());
        }
        let envelope = Self {
            generation,
            parent,
            recipe: Recipe {
                checkpoint: Checkpoint::decode(&bytes[HEADER..HEADER + sn])?,
                recipe_bytes: bytes[HEADER + sn..HEADER + sn + rn].to_vec(),
            },
        };
        envelope.validate_origin()?;
        Ok(envelope)
    }

    fn validate(bytes: &[u8]) -> Result<(), crate::Error> {
        Self::decode(bytes)
            .map(|_| ())
            .map_err(|_| crate::Error::Corrupt)
    }
}

fn checksum(bytes: &[u8]) -> Digest {
    let mut hash = Sha256::new();
    hash.update(b"benilla-native-wander-reconstruction-store-v1\0");
    hash.update(bytes);
    hash.finalize().into()
}

#[cfg(test)]
mod tests;
