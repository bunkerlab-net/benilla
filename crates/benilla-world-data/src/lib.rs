// SPDX-License-Identifier: GPL-3.0-or-later
//! Local-only world-data validation. No SQL execution, asset download, networking,
//! filesystem writes, script interpreter, or gameplay runtime is provided.
#![forbid(unsafe_code)]

pub mod model;
mod validate;

pub use model::*;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fmt, io::Read};

pub const HARD_MAX_BYTES: usize = 8 * 1024 * 1024;
pub const HARD_MAX_RECORDS: usize = 100_000;
pub const HARD_MAX_STRING_BYTES: usize = 16_384;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_bytes: usize,
    /// Combined top-level row count, including relations.
    pub max_records: usize,
    pub max_string_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: HARD_MAX_BYTES,
            max_records: HARD_MAX_RECORDS,
            max_string_bytes: HARD_MAX_STRING_BYTES,
        }
    }
}
impl Limits {
    fn check(self) -> Result<Self, Error> {
        if self.max_bytes == 0
            || self.max_bytes > HARD_MAX_BYTES
            || self.max_records == 0
            || self.max_records > HARD_MAX_RECORDS
            || self.max_string_bytes == 0
            || self.max_string_bytes > HARD_MAX_STRING_BYTES
        {
            return Err(Error::InvalidLimits);
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidLimits,
    TooLarge,
    TooManyRecords,
    Io,
    Json { line: usize, column: usize },
    Invalid { field: &'static str, row: u64 },
    Duplicate { table: &'static str, row: u64 },
    MissingReference { field: &'static str, row: u64 },
    UnsupportedFeatures,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "world data: {self:?}")
    }
}
impl std::error::Error for Error {}

/// An immutable validated export with stable indexes and a canonical SHA-256.
/// Hash equality establishes identical normalized input, not trusted authorship.
#[derive(Clone, Debug)]
pub struct WorldData {
    export: WorldExport,
    digest: [u8; 32],
    creatures: BTreeMap<u32, usize>,
    spawns: BTreeMap<u64, usize>,
    quests: BTreeMap<u32, usize>,
    items: BTreeMap<u32, usize>,
    abilities: BTreeMap<u32, usize>,
}
impl WorldData {
    pub fn export(&self) -> &WorldExport {
        &self.export
    }
    pub fn content_digest(&self) -> [u8; 32] {
        self.digest
    }
    pub fn creature(&self, entry: u32) -> Option<&CreatureTemplate> {
        self.creatures
            .get(&entry)
            .map(|&i| &self.export.creatures[i])
    }
    pub fn spawn(&self, id: u64) -> Option<&CreatureSpawn> {
        self.spawns.get(&id).map(|&i| &self.export.spawns[i])
    }
    pub fn quest(&self, entry: u32) -> Option<&KillQuest> {
        self.quests.get(&entry).map(|&i| &self.export.quests[i])
    }
    pub fn item(&self, entry: u32) -> Option<&PlainItem> {
        self.items.get(&entry).map(|&i| &self.export.items[i])
    }
    pub fn ability(&self, entry: u32) -> Option<&DirectAbility> {
        self.abilities
            .get(&entry)
            .map(|&i| &self.export.abilities[i])
    }
}

/// Bounded read; consumes at most max_bytes + 1 bytes and never executes input.
pub fn load_reader(reader: impl Read, limits: Limits) -> Result<WorldData, Error> {
    let limits = limits.check()?;
    let mut bytes = Vec::new();
    reader
        .take((limits.max_bytes + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Io)?;
    load_json(&bytes, limits)
}

/// Unknown fields, unsupported source revisions, and dangling references fail closed.
/// serde_json's default recursion limit is retained. Input size bounds allocations;
/// row and string limits are checked after deserialization.
pub fn load_json(bytes: &[u8], limits: Limits) -> Result<WorldData, Error> {
    let limits = limits.check()?;
    if bytes.len() > limits.max_bytes {
        return Err(Error::TooLarge);
    }
    let mut export: WorldExport = serde_json::from_slice(bytes).map_err(|e| Error::Json {
        line: e.line(),
        column: e.column(),
    })?;
    validate::validate(&export, limits)?;
    export.creatures.sort_by_key(|x| x.entry);
    export.spawns.sort_by_key(|x| x.spawn_id);
    export.quests.sort_by_key(|x| x.entry);
    export
        .quest_relations
        .sort_by_key(|x| (x.creature_entry, x.quest_entry, x.kind));
    export.items.sort_by_key(|x| x.entry);
    export.abilities.sort_by_key(|x| x.entry);
    // Integer-only structs, explicit field order, and sorted rows define v1 hashing.
    let canonical = serde_json::to_vec(&export).map_err(|_| Error::Invalid {
        field: "canonical_encoding",
        row: 0,
    })?;
    let mut hash = Sha256::new();
    hash.update(b"benilla-world-data-v1\0");
    hash.update(canonical);
    let digest = hash.finalize().into();
    Ok(WorldData {
        creatures: export
            .creatures
            .iter()
            .enumerate()
            .map(|(i, x)| (x.entry, i))
            .collect(),
        spawns: export
            .spawns
            .iter()
            .enumerate()
            .map(|(i, x)| (x.spawn_id, i))
            .collect(),
        quests: export
            .quests
            .iter()
            .enumerate()
            .map(|(i, x)| (x.entry, i))
            .collect(),
        items: export
            .items
            .iter()
            .enumerate()
            .map(|(i, x)| (x.entry, i))
            .collect(),
        abilities: export
            .abilities
            .iter()
            .enumerate()
            .map(|(i, x)| (x.entry, i))
            .collect(),
        export,
        digest,
    })
}
