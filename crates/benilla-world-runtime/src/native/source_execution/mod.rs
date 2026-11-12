// SPDX-License-Identifier: GPL-3.0-or-later
//! Immutable source inputs and non-admitting differential diagnostics.
//! Expected outcomes have no conversion to an admissible input.
mod active;
pub use active::*;
mod ai_selection;
pub use ai_selection::*;
mod ai;
pub use ai::*;
mod ai_inputs;
pub use ai_inputs::*;
#[cfg(test)]
mod ai_selection_tests;
mod event_ai_inputs;
pub use event_ai_inputs::*;
mod event_ai_catalog;
pub use event_ai_catalog::*;
mod event_ai_state;
pub use event_ai_state::*;
mod event_ai_config;
pub use event_ai_config::*;
mod event_ai_dispatch;
#[cfg(test)]
mod event_ai_retained_tests;
#[cfg(test)]
mod event_ai_tests;
pub use event_ai_dispatch::*;
mod event_ai_actions;
pub use event_ai_actions::*;
#[cfg(test)]
mod event_ai_dispatch_tests;
mod threat;
pub use threat::*;
mod combat;
pub use combat::*;
mod unit_ai;
pub use unit_ai::*;
mod unit_events;
pub use unit_events::*;
mod assistance;
pub use assistance::*;
#[cfg(test)]
mod assistance_tests;
mod unit_update;
pub use unit_update::*;
mod select_hostile_target;
pub use select_hostile_target::*;
mod motion_master;
#[cfg(test)]
mod select_hostile_target_tests;
#[cfg(test)]
mod unit_events_census_tests;
#[cfg(test)]
mod unit_events_tests;
pub use motion_master::*;
mod motion;
pub use motion::*;
mod motion_dependencies;
pub use motion_dependencies::*;
mod creature_ai;
#[cfg(test)]
mod motion_census_tests;
#[cfg(test)]
mod motion_master_tests;
pub use creature_ai::*;
#[cfg(test)]
mod combat_tests;
mod seal8_active;
#[cfg(test)]
mod threat_tests;
#[cfg(test)]
mod unit_ai_census_tests;
#[cfg(test)]
mod unit_ai_tests;
pub use seal8_active::{compare_seal8_unsealed_active, Seal8ActiveDifference, Seal8ActiveReport};
#[cfg(test)]
mod active_tests;
mod cells;
mod clock;
mod compare;
mod creatures;
pub use creatures::*;
mod seal8_creatures;
pub use seal8_creatures::{
    compare_seal8_unsealed_creatures, CreatureConstructorDifference, Seal8CreatureReport,
};
mod creature_lifecycle;
#[cfg(test)]
mod creature_tests;
pub use creature_lifecycle::*;
mod grid_loader;
mod grids;
mod guids;
#[cfg(test)]
mod pet_number_tests;
pub use grid_loader::*;
pub use grids::*;
mod seal8_grids;
pub use seal8_grids::{compare_seal8_unsealed_grids, Seal8GridReport};
#[cfg(test)]
mod grid_tests;
mod inputs;
mod manager_dispatch;
mod manager_inputs;
mod managers;
pub use manager_dispatch::*;
mod manager_validation;
pub use manager_validation::*;
mod world_managers;
pub use world_managers::*;
mod messages;
mod objects;
mod path_distance;
mod persistent_state;
mod scheduler;
mod spawn_manager;
#[cfg(test)]
mod world_manager_tests;
pub use spawn_manager::*;
mod seal8_distance;
mod seal8_objects;
#[cfg(test)]
mod spawn_manager_tests;
mod startup;
mod startup_items;
pub use startup_items::*;
mod trace;
mod world;
pub use manager_inputs::*;
pub use managers::*;
pub use persistent_state::*;
#[cfg(test)]
mod manager_tests;
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
mod gameobjects;
pub use gameobjects::*;
mod gameobject_lifecycle;
pub use gameobject_lifecycle::*;
#[cfg(test)]
mod gameobject_tests;
mod seal8_gameobjects;
pub use seal8_gameobjects::{compare_seal8_unsealed_gameobjects, Seal8GameObjectReport};
pub(crate) mod identity_table;
mod update_set;
mod visitors;
pub use update_set::*;
pub use visitors::*;
mod seal8_update_set;
#[cfg(test)]
mod update_set_tests;
pub use seal8_update_set::*;
mod residency;
pub use residency::*;
mod creature_linking;
#[cfg(test)]
mod residency_tests;
pub use creature_linking::*;
#[cfg(test)]
mod creature_linking_tests;
mod seal8_linking;
pub use seal8_linking::{inspect_seal8_unsealed_linking, Seal8LinkingReport};
mod pools;
pub use pools::*;
#[cfg(test)]
mod pool_tests;
mod seal8_pools;
pub use seal8_pools::{compare_seal8_unsealed_pools, Seal8PoolReport};
mod transports;
pub use transports::*;
mod seal8_transports;
pub use seal8_transports::{inspect_seal8_unsealed_transports, Seal8TransportReport};
mod spawn_groups;
pub use spawn_groups::*;
mod effective_spawns;
pub use effective_spawns::*;
mod instance_templates;
pub use instance_templates::*;
mod spawn_loader_config;
pub use spawn_loader_config::*;
mod formations;
pub use formations::*;
#[cfg(test)]
mod formation_tests;
mod game_events;
#[cfg(test)]
mod spawn_group_tests;
pub use game_events::*;
mod game_event_effects;
pub use game_event_effects::*;
mod game_event_dispatch;
pub use game_event_dispatch::*;
#[cfg(test)]
mod game_event_tests;
mod seal8_game_events;
pub use seal8_game_events::{compare_seal8_unsealed_event_calendar, Seal8EventCalendarReport};
mod weather;
pub use weather::*;
mod seal8_weather;
#[cfg(test)]
mod weather_tests;
pub use seal8_weather::{compare_seal8_unsealed_weather, Seal8WeatherReport};
mod corpse_inputs;
pub use corpse_inputs::*;
mod corpses;
pub use corpses::*;
#[cfg(test)]
mod corpse_tests;
mod seal8_corpses;
pub use seal8_corpses::{compare_seal8_unsealed_corpses, Seal8CorpseReport};

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

mod spell_template;
pub use spell_template::*;
mod spell_inputs;
pub use spell_inputs::*;
mod db_scripts;
pub use db_scripts::*;
mod db_scripts_catalog;
pub use db_scripts_catalog::*;
mod db_script_execution;
pub use db_script_execution::*;
