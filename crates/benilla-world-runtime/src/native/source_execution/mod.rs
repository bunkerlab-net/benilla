// SPDX-License-Identifier: GPL-3.0-or-later
//! Immutable source inputs and non-admitting differential diagnostics.
//! Expected outcomes have no conversion to an admissible input.
mod cells;
mod clock;
mod compare;
mod grid_loader;
mod grids;
mod guids;
pub use grid_loader::*;
pub use grids::*;
mod seal8_grids;
pub use seal8_grids::{compare_seal8_unsealed_grids, Seal8GridReport};
#[cfg(test)]
mod grid_tests;
mod inputs;
mod messages;
mod objects;
mod path_distance;
mod scheduler;
mod seal8_distance;
mod seal8_objects;
mod startup;
mod trace;
mod world;
pub use cells::*;
pub use guids::*;
pub use objects::*;
pub use seal8_objects::{compare_seal8_unsealed_objects, Seal8ObjectReport};
#[cfg(test)]
mod object_tests;
pub use startup::*;
#[cfg(test)]
mod clock_tests;
pub use clock::*;
pub use compare::*;
pub use inputs::*;
pub use messages::*;
pub use path_distance::*;
pub use scheduler::*;
pub use seal8_distance::*;
pub use trace::*;
pub use world::*;
mod original_rng;
mod original_scheduler;
pub use original_scheduler::{compare_original_unsealed_scheduler, OriginalSchedulerReport};
mod retained_seal8;
mod seal8_clock;
pub use seal8_clock::{compare_seal8_unsealed_clocks, Seal8ClockReport};
#[cfg(test)]
mod tests;
pub use original_rng::{compare_original_unsealed_rng, OriginalRngReport};

use super::production_compiler::{json, ExecutionFile};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

pub type Result<T, E = Refusal> = std::result::Result<T, E>;
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Refusal {
    pub id: String,
}
impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.id)
    }
}
impl std::error::Error for Refusal {}
impl From<super::production_compiler::ProductionDiagnostic> for Refusal {
    fn from(value: super::production_compiler::ProductionDiagnostic) -> Self {
        Self { id: value.id }
    }
}
fn refuse(id: &str) -> Refusal {
    Refusal { id: id.into() }
}
fn require(condition: bool, id: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(refuse(id))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Sha256Id(String);
impl Sha256Id {
    pub fn parse(value: &str) -> Result<Self> {
        require(
            value.len() == 64
                && value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "execution.sha256",
        )?;
        Ok(Self(value.into()))
    }
    pub fn of(bytes: &[u8]) -> Self {
        Self(format!("{:x}", Sha256::digest(bytes)))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl<'de> Deserialize<'de> for Sha256Id {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}
fn decode<T: DeserializeOwned>(bytes: &[u8], limit: usize) -> Result<T> {
    let value = json::decode_with_limits(bytes, limit, 32, 100_000)?;
    require(
        serde_json::to_vec(&value).map_err(|_| refuse("execution.json"))? == bytes,
        "execution.canonical_json",
    )?;
    serde_json::from_value(value).map_err(|_| refuse("execution.schema"))
}
fn metadata(path: &Path, limit: usize) -> Result<(ExecutionFile, Vec<u8>)> {
    let held = ExecutionFile::open(path, limit as u64)?;
    let mut raw = vec![0; held.len() as usize];
    held.read_into(0, &mut raw)?;
    Ok((held, raw))
}
fn relative(root: &Path, name: &str) -> Result<std::path::PathBuf> {
    require(
        !name.is_empty()
            && Path::new(name)
                .components()
                .all(|p| matches!(p, std::path::Component::Normal(_))),
        "execution.relative_path",
    )?;
    Ok(root.join(name))
}
fn required_option<'de, D, T>(d: D) -> std::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(d)
}

/// Resource bounds refuse before reading work, never discard records to fit.
#[derive(Clone, Copy, Debug)]
pub struct TraceLimits {
    pub record_bytes: usize,
    pub chunk_bytes: usize,
    pub pending_records: usize,
    pub history_events: u64,
    pub threads: usize,
}
impl Default for TraceLimits {
    fn default() -> Self {
        Self {
            record_bytes: 65_536,
            chunk_bytes: 1_048_576,
            pending_records: 256,
            history_events: 100_000,
            threads: 64,
        }
    }
}
impl TraceLimits {
    fn validate(self) -> Result<()> {
        require(
            self.record_bytes > 0
                && self.record_bytes <= self.chunk_bytes
                && self.chunk_bytes <= 1_048_576
                && self.pending_records > 0
                && self.history_events > 0
                && self.threads > 0,
            "execution.memory_budget",
        )
    }
}
