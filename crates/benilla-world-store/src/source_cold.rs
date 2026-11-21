// SPDX-License-Identifier: GPL-3.0-or-later
//! Checked publication of the source database image, separate from native checkpoints.

use crate::{durability, hash, storage::Storage, Durability, Error};
use benilla_world_runtime::{
    native::source_execution::source_rows::{
        ActionRow, AuraRow, CharacterRow, CooldownRow, DragonStateRow, EnteredInstanceRow,
        HomebindRow, InventoryRow, ItemInstanceRow, QuestRow, RepRow, RespawnRow, RowOperation,
        SkillRow, SpellRow, TutorialRow,
    },
    Digest, PlayerId, WorldId,
};
use serde::{Deserialize, Serialize};
use std::{io::ErrorKind, path::Path};

mod rows;
pub use rows::OperationCount;

const DIRECTORY: &str = "source-cold";
const MAGIC: &[u8; 8] = b"BNSRCDB\0";
const VERSION: u32 = 1;
const HEADER: usize = 8 + 4 + 8 + 32;
const CHECKSUM: usize = 32;
const MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;
const _: () = assert!(MAX_SOURCE_BYTES <= crate::MAX_CHECKPOINT_BYTES);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Domain {
    HumanMageNorthshire,
}

/// Supply the verified local identity before opening a candidate file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub world: WorldId,
    pub player: PlayerId,
    pub account: u32,
    pub guid: u32,
    pub domain: Domain,
    pub catalog: Digest,
    pub logic_policy: Digest,
    pub world_product: Digest,
}

impl Identity {
    fn validate(&self) -> Result<(), Error> {
        if self.account == 0 || self.guid == 0 {
            return Err(Error::Invalid(
                "source account and character GUID must be nonzero",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rows {
    character: Option<Box<CharacterRow>>,
    inventory: Vec<InventoryRow>,
    items: Vec<ItemInstanceRow>,
    skills: Vec<SkillRow>,
    actions: Vec<ActionRow>,
    spells: Vec<SpellRow>,
    quests: Vec<QuestRow>,
    reputation: Vec<RepRow>,
    tutorial: Option<TutorialRow>,
    homebind: Option<HomebindRow>,
    respawns: Vec<RespawnRow>,
    dragons: Option<DragonStateRow>,
    cooldowns: Vec<CooldownRow>,
    auras: Vec<AuraRow>,
    entered_instances: Vec<EnteredInstanceRow>,
}

impl Rows {
    pub fn character(&self) -> Option<&CharacterRow> {
        self.character.as_deref()
    }

    fn validate_character(&self, selected: &Identity) -> Result<(), Error> {
        let Some(row) = self.character() else {
            return Ok(());
        };
        if (row.account, row.guid) != (selected.account, selected.guid) {
            return Err(Error::Incompatible);
        }
        match selected.domain {
            Domain::HumanMageNorthshire
                if (row.race, row.class, row.map) == (1, 8, 0)
                    && row.gender <= 1
                    && row.transport_guid == 0
                    && row.taxi_path.is_empty() => {}
            _ => {
                return Err(Error::Invalid(
                    "character is outside the source store domain",
                ))
            }
        }
        if row.name.is_empty()
            || row.name.chars().count() > 12
            || row.name.chars().any(|c| u32::from(c) > 0xffff || c == '\0')
            || row
                .xyz
                .iter()
                .chain(&row.transport_pose)
                .chain([&row.orientation, &row.rest_bonus, &row.stored_honor_rating])
                .any(|word| !word.as_float().is_finite())
        {
            return Err(Error::Invalid("invalid source character columns"));
        }
        Ok(())
    }
}

/// Issued only after publication and exact readback through the locked directory.
#[derive(Debug)]
pub struct Receipt {
    generation: u64,
    digest: Digest,
    operations: Vec<OperationCount>,
}

impl Receipt {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn digest(&self) -> Digest {
        self.digest
    }

    pub fn durability(&self) -> Durability {
        durability()
    }

    pub fn operation_counts(&self) -> &[OperationCount] {
        &self.operations
    }
}

pub struct Prepared {
    generation: u64,
    parent: Digest,
    bytes: Vec<u8>,
    operations: Vec<OperationCount>,
}

/// Holds the source directory lock. Only publication metadata stays resident.
pub struct SourceColdStore {
    storage: Storage,
    identity: Identity,
    generation: u64,
    digest: Digest,
    previous: Option<Digest>,
    blocked: bool,
}

impl SourceColdStore {
    /// Create an empty source image inside an existing, locally selected world directory.
    pub fn create(
        world_dir: impl AsRef<Path>,
        identity: Identity,
    ) -> Result<(Self, Receipt), Error> {
        identity.validate()?;
        let initial = Header {
            generation: 1,
            parent: [0; 32],
        };
        let bytes = initial.encode(&identity, &Rows::default())?;
        let storage = Storage::acquire(&world_dir.as_ref().join(DIRECTORY), true)?;
        storage.write_checked(&bytes, None, Header::validate)?;
        let digest = hash(&bytes);
        Ok((
            Self {
                storage,
                identity,
                generation: initial.generation,
                digest,
                previous: None,
                blocked: false,
            },
            Receipt {
                generation: initial.generation,
                digest,
                operations: Vec::new(),
            },
        ))
    }

    /// Validate current and its predecessor. Never recover by selecting previous as current.
    pub fn open(world_dir: impl AsRef<Path>, identity: Identity) -> Result<(Self, Rows), Error> {
        identity.validate()?;
        let storage = Storage::acquire(&world_dir.as_ref().join(DIRECTORY), false)?;
        let bytes = storage.read()?;
        let (header, rows) = Header::decode(&bytes, &identity)?;
        let digest = hash(&bytes);
        let previous = match storage.read_previous() {
            // A failed backup-first write may already have copied current to previous.
            Ok(prior) if prior == bytes => Some(digest),
            Ok(prior) => {
                let (previous, _) = Header::decode(&prior, &identity)?;
                let parent = hash(&prior);
                if previous.generation.checked_add(1) != Some(header.generation)
                    || parent != header.parent
                {
                    return Err(Error::DivergentHistory);
                }
                Some(parent)
            }
            Err(Error::Io(error)) if error.kind() == ErrorKind::NotFound => {
                if header.generation != 1 {
                    return Err(Error::DivergentHistory);
                }
                None
            }
            Err(error) => return Err(error),
        };
        Ok((
            Self {
                storage,
                identity,
                generation: header.generation,
                digest,
                previous,
                blocked: false,
            },
            rows,
        ))
    }

    pub fn identity(&self) -> &Identity {
        &self.identity
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn requires_reopen(&self) -> bool {
        self.blocked
    }

    fn ensure_open(&self) -> Result<(), Error> {
        if self.blocked {
            return Err(Error::CommitUncertain);
        }
        Ok(())
    }

    pub fn read_rows(&mut self) -> Result<Rows, Error> {
        self.ensure_open()?;
        match self.storage.read() {
            Ok(bytes) if hash(&bytes) == self.digest => {
                Header::decode(&bytes, &self.identity).map(|(_, rows)| rows)
            }
            _ => {
                self.blocked = true;
                Err(Error::ConcurrentModification)
            }
        }
    }

    /// Apply statements in source order and stage the checked private image.
    pub fn prepare(
        &mut self,
        operations: impl IntoIterator<Item = RowOperation>,
    ) -> Result<Prepared, Error> {
        const MAX_OPERATIONS: usize = 131_072;
        let mut rows = self.read_rows()?;
        let mut counts = Vec::new();
        for operation in operations {
            if counts.len() == MAX_OPERATIONS {
                return Err(Error::Invalid("source batch limit exceeded"));
            }
            counts.push(rows.apply(&self.identity, operation)?);
        }
        let mut prepared = self.prepare_rows(&rows)?;
        prepared.operations = counts;
        Ok(prepared)
    }

    fn prepare_rows(&self, rows: &Rows) -> Result<Prepared, Error> {
        self.ensure_open()?;
        rows.validate(&self.identity)?;
        let generation = self
            .generation
            .checked_add(1)
            .ok_or(Error::Invalid("source generation exhausted"))?;
        let parent = self.digest;
        let bytes = Header { generation, parent }.encode(&self.identity, rows)?;
        Ok(Prepared {
            generation,
            parent,
            bytes,
            operations: Vec::new(),
        })
    }

    pub fn commit(&mut self, prepared: Prepared) -> Result<Receipt, Error> {
        self.ensure_open()?;
        if prepared.parent != self.digest
            || Some(prepared.generation) != self.generation.checked_add(1)
        {
            return Err(Error::StalePrepared);
        }
        if let Err(error) = self.storage.write_checked(
            &prepared.bytes,
            Some((self.digest, self.previous)),
            Header::validate,
        ) {
            // Source gameplay must stay stopped after any failed publication.
            self.blocked = true;
            return Err(error);
        }
        self.previous = Some(self.digest);
        self.generation = prepared.generation;
        self.digest = hash(&prepared.bytes);
        Ok(Receipt {
            generation: self.generation,
            digest: self.digest,
            operations: prepared.operations,
        })
    }
}

struct Header {
    generation: u64,
    parent: Digest,
}

impl Header {
    fn encode(&self, identity: &Identity, rows: &Rows) -> Result<Vec<u8>, Error> {
        let mut bytes = Vec::with_capacity(HEADER + CHECKSUM);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.generation.to_le_bytes());
        bytes.extend_from_slice(&self.parent);
        serde_json::to_writer(&mut bytes, &(identity, rows)).map_err(|_| Error::Corrupt)?;
        if bytes.len() > MAX_SOURCE_BYTES - CHECKSUM {
            return Err(Error::Invalid("source row image exceeds its byte limit"));
        }
        bytes.extend_from_slice(&hash(&bytes));
        Ok(bytes)
    }

    fn frame(bytes: &[u8]) -> Result<(Self, &[u8]), Error> {
        if !(HEADER + CHECKSUM..=MAX_SOURCE_BYTES).contains(&bytes.len()) || &bytes[..8] != MAGIC {
            return Err(Error::Corrupt);
        }
        let end = bytes.len() - CHECKSUM;
        if bytes[end..] != hash(&bytes[..end]) {
            return Err(Error::Corrupt);
        }
        if bytes[8..12] != VERSION.to_le_bytes() {
            return Err(Error::UnsupportedVersion);
        }
        let generation = u64::from_le_bytes(bytes[12..20].try_into().map_err(|_| Error::Corrupt)?);
        let parent = bytes[20..HEADER].try_into().map_err(|_| Error::Corrupt)?;
        if generation == 0 || (generation == 1) != (parent == [0; 32]) {
            return Err(Error::Corrupt);
        }
        Ok((Self { generation, parent }, &bytes[HEADER..end]))
    }

    fn validate(bytes: &[u8]) -> Result<(), Error> {
        Self::frame(bytes).map(|_| ())
    }

    fn decode(bytes: &[u8], selected: &Identity) -> Result<(Self, Rows), Error> {
        let (header, payload) = Self::frame(bytes)?;
        let (identity, rows): (Identity, Rows) =
            serde_json::from_slice(payload).map_err(|_| Error::Corrupt)?;
        if identity != *selected {
            return Err(Error::Incompatible);
        }
        rows.validate(selected)?;
        Ok((header, rows))
    }
}

#[cfg(test)]
#[path = "source_cold_tests.rs"]
mod tests;
