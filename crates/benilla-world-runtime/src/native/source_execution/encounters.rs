// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! ObjectMgr.cpp:9491-9494,5286-5356 and WorldStateVariableManager.cpp:30-138.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub const ENCOUNTER_QUERIES: [&str; 4] = [
    "SELECT MAX(id) FROM instance_dungeon_encounters",
    "SELECT COUNT(*) FROM instance_dungeon_encounters",
    "SELECT * FROM instance_dungeon_encounters",
    "SELECT entry, creditType, creditEntry, lastEncounterDungeon FROM instance_encounters",
];
pub const ENCOUNTER_SHIFT_UB: &str = "execution.world_state.encounter_index_signed_shift_ub";
pub const SPAWN_INITIALIZE_CLOCK_OWNER: &str = "execution.spawn_manager.initialize.time_owner";

#[derive(Debug, PartialEq, Eq)]
pub struct DungeonEncounterEntry {
    pub id: u32,
    pub map: u32,
    pub difficulty: u32,
    pub data: u32,
    pub index: u32,
    pub complete_world_state: u32,
    pub names: [Vec<u8>; 16],
    pub name_flags: u32,
    pub icon: u32,
}
#[derive(Debug, Default)]
pub struct LoadedDungeonEncounters {
    rows: BTreeMap<u32, Arc<DungeonEncounterEntry>>,
}
impl LoadedDungeonEncounters {
    pub fn catalog_operands(
        catalog: &mut super::super::production_compiler::VerifiedCatalog,
    ) -> Result<(Self, Vec<[u32; 4]>, EncounterReferences)> {
        Ok(catalog.execution_encounter_operands()?)
    }
    /// SQLStorageImpl.h:394 indexes by the first column; the last record wins.
    pub fn load(rows: impl IntoIterator<Item = DungeonEncounterEntry>) -> Self {
        Self {
            rows: rows
                .into_iter()
                .map(|row| (row.id, Arc::new(row)))
                .collect(),
        }
    }
    pub fn rows(&self) -> &BTreeMap<u32, Arc<DungeonEncounterEntry>> {
        &self.rows
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstanceEncounter {
    pub entry: Arc<DungeonEncounterEntry>,
    pub credit_type: u8,
    pub credit_entry: u32,
    pub last_dungeon: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EncounterSkip {
    Encounter,
    Creature,
    Spell,
    CreditType,
}
#[derive(Debug, Default, PartialEq, Eq)]
pub struct EncounterCensus {
    pub visited: usize,
    pub loaded: usize,
    pub skipped: BTreeMap<EncounterSkip, usize>,
}
pub struct EncounterReferences {
    pub creatures: BTreeSet<u32>,
    pub spells: BTreeSet<u32>,
}
#[derive(Debug, Default)]
pub struct LoadedInstanceEncounters {
    credits: BTreeMap<u32, Vec<InstanceEncounter>>,
    maps: BTreeMap<u32, Vec<InstanceEncounter>>,
}
impl LoadedInstanceEncounters {
    /// ObjectMgr.cpp:5288 clears credits only. Equivalent keys retain query order.
    pub fn load(
        previous: Option<&Self>,
        rows: Option<&[[u32; 4]]>,
        store: &LoadedDungeonEncounters,
        references: &EncounterReferences,
    ) -> (Self, EncounterCensus) {
        let mut loaded = Self {
            credits: BTreeMap::new(),
            maps: previous.map(|old| old.maps.clone()).unwrap_or_default(),
        };
        let mut census = EncounterCensus::default();
        for row in rows.unwrap_or_default() {
            census.visited += 1;
            let Some(entry) = store.rows.get(&row[0]) else {
                *census.skipped.entry(EncounterSkip::Encounter).or_default() += 1;
                continue;
            };
            // Field.h:56 narrows to uint8 before the switch.
            let credit_type = row[1] as u8;
            let skip = match credit_type {
                0 if !references.creatures.contains(&row[2]) => Some(EncounterSkip::Creature),
                1 if !references.spells.contains(&row[2]) => Some(EncounterSkip::Spell),
                0..=2 => None,
                _ => Some(EncounterSkip::CreditType),
            };
            if let Some(reason) = skip {
                *census.skipped.entry(reason).or_default() += 1;
                continue;
            }
            let encounter = InstanceEncounter {
                entry: Arc::clone(entry),
                credit_type,
                credit_entry: row[2],
                last_dungeon: row[3],
            };
            loaded
                .credits
                .entry(row[2])
                .or_default()
                .push(encounter.clone());
            loaded.maps.entry(entry.map).or_default().push(encounter);
            census.loaded += 1;
        }
        (loaded, census)
    }
    pub fn by_credit(&self) -> &BTreeMap<u32, Vec<InstanceEncounter>> {
        &self.credits
    }
    pub fn by_map(&self, map: u32) -> &[InstanceEncounter] {
        self.maps.get(&map).map(Vec::as_slice).unwrap_or_default()
    }
    pub fn shift_census(&self) -> (usize, usize) {
        let entries = self.maps.values().flatten();
        entries.fold((0, 0), |(visited, ub), row| {
            (visited + 1, ub + usize::from(row.entry.index >= 31))
        })
    }
}

/// WorldStateVariableManager.h:31-39. No executors run in SetVariable.
#[derive(Debug, Default)]
pub struct SourceWorldStateVariable {
    pub value: i32,
    pub send: bool,
    pub zone: u32,
    pub area: u32,
    pub executors: Vec<fn()>,
}
#[derive(Debug, Default)]
pub struct SourceWorldStateVariables {
    entries: BTreeMap<i32, SourceWorldStateVariable>,
}
impl SourceWorldStateVariables {
    pub fn entries(&self) -> &BTreeMap<i32, SourceWorldStateVariable> {
        &self.entries
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    #[cfg(test)]
    pub(crate) fn values(&self) -> BTreeMap<i32, i32> {
        self.entries
            .iter()
            .map(|(&id, variable)| (id, variable.value))
            .collect()
    }
    pub fn get(&self, id: i32) -> i32 {
        self.entries.get(&id).map(|v| v.value).unwrap_or(0)
    }
    pub fn set_data(&mut self, id: i32, send: bool, zone: u32, area: u32) {
        let variable = self.entries.entry(id).or_default();
        variable.send = send;
        variable.zone = zone;
        variable.area = area;
    }
    pub fn set(&mut self, id: i32, value: i32) -> Result<()> {
        let variable = self.entries.entry(id).or_default();
        if variable.value == value {
            return Ok(());
        }
        variable.value = value;
        require(
            !variable.send,
            "execution.world_state.BroadcastVariable_owner",
        )
    }
    pub fn set_encounter(&mut self, id: u32, state: bool) -> Result<()> {
        let id = id.wrapping_mul(100);
        self.set(id as i32, i32::from(state))?;
        self.set(id.wrapping_add(1) as i32, i32::from(!state))
    }
    pub fn initialize(
        &mut self,
        map: u32,
        mask: u32,
        encounters: Option<&LoadedInstanceEncounters>,
    ) -> Result<()> {
        let encounters = encounters.ok_or_else(|| refuse(DUNGEON_ENCOUNTER_BOUNDS_OWNER))?;
        for encounter in encounters.by_map(map) {
            let entry = &encounter.entry;
            // WorldStateVariableManager.cpp:38/40, before evaluating signed 1 << index.
            require(entry.index < 31, ENCOUNTER_SHIFT_UB)?;
            let complete = ((1i32 << entry.index) as u32 & mask) != 0;
            if entry.complete_world_state != 0 {
                self.set(entry.complete_world_state as i32, i32::from(complete))?;
            } else {
                self.set_encounter(entry.id, complete)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "encounters_tests.rs"]
mod tests;
