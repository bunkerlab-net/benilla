// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! ScriptDevAIMgr.cpp:148-158,426-544. Registry-only startup projection.
use super::*;
use std::collections::BTreeMap;

/// Symbolic callback slots, never executable addresses or captured registry post-state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceScriptDefinition {
    pub name: Vec<u8>,
    pub registration: u64,
    pub operation: String,
    pub source_file: String,
    pub source_line: u32,
    pub source_function: String,
    pub slots: BTreeMap<String, bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnexecutedScriptPrerequisite {
    Texts,
    CustomTexts,
    GossipTexts,
    Waypoints,
    SpellSummary,
}
impl UnexecutedScriptPrerequisite {
    pub fn owner(self) -> &'static str {
        match self {
            Self::Texts => "execution.script_registry.script_texts_owner",
            Self::CustomTexts => "execution.script_registry.custom_texts_owner",
            Self::GossipTexts => "execution.script_registry.gossip_texts_owner",
            Self::Waypoints => "execution.script_registry.scriptdev_waypoints_owner",
            Self::SpellSummary => "execution.script_registry.spell_summary_owner",
        }
    }
    pub fn source(self) -> &'static str {
        match self {
            Self::Texts => "ScriptDevAIMgr.cpp:460,29; system.cpp:48",
            Self::CustomTexts => "ScriptDevAIMgr.cpp:460,30; system.cpp:54",
            Self::GossipTexts => "ScriptDevAIMgr.cpp:460,31; system.cpp:60",
            Self::Waypoints => "ScriptDevAIMgr.cpp:460,32; system.cpp:68-114",
            Self::SpellSummary => "ScriptDevAIMgr.cpp:469; sc_creature.cpp:191-269",
        }
    }
}
const UNEXECUTED: [UnexecutedScriptPrerequisite; 5] = [
    UnexecutedScriptPrerequisite::Texts,
    UnexecutedScriptPrerequisite::CustomTexts,
    UnexecutedScriptPrerequisite::GossipTexts,
    UnexecutedScriptPrerequisite::Waypoints,
    UnexecutedScriptPrerequisite::SpellSummary,
];

#[derive(Default, Debug)]
pub struct SourceScriptRegistry {
    names: Vec<Vec<u8>>,
    scripts: Vec<Option<SourceScriptDefinition>>,
    script_count: i32,
    unexecuted: &'static [UnexecutedScriptPrerequisite],
}
impl SourceScriptRegistry {
    /// World.cpp:927 precedes :1333. Keep the empty-name write before the SQL read.
    pub fn load_script_names(
        &mut self,
        query: impl FnOnce() -> Result<Option<Vec<Vec<u8>>>>,
    ) -> Result<()> {
        self.names.push(Vec::new()); // ScriptDevAIMgr.cpp:479.
        let Some(rows) = query()? else { return Ok(()) };
        self.names.extend(rows);
        self.names.sort(); // ScriptDevAIMgr.cpp:515, including duplicate names on reload.
        Ok(())
    }
    pub fn get_script_id(&self, name: Option<&[u8]>) -> u32 {
        let Some(name) = name else { return 0 };
        let index = self.names.partition_point(|value| value.as_slice() < name);
        if self.names.get(index).is_some_and(|value| value == name) {
            index as u32
        } else {
            0
        }
    }
    /// Execute only registry writes at :467,471, not the independent outputs at :460,469.
    /// Texts write ObjectMgr's string map (ObjectMgr.cpp:8424-8554), waypoints write
    /// WaypointMgr (system.cpp:114), summary writes SpellSummary (sc_creature.cpp:191-269).
    pub fn initialize_registry(
        &mut self,
        registrations: impl IntoIterator<Item = SourceScriptDefinition>,
    ) -> Result<()> {
        self.unexecuted = &UNEXECUTED[..4];
        self.scripts.resize_with(self.names.len(), || None); // :467 preserves existing slots.
        self.unexecuted = &UNEXECUTED;
        for definition in registrations {
            // :471, captured AddScripts/RegisterSelf order.
            self.register_self(definition)?;
        }
        Ok(())
    }
    /// ScriptDevAIMgr.cpp:148-158, also reached by World.cpp:1338's Shaman registration.
    pub fn register_self(&mut self, definition: SourceScriptDefinition) -> Result<()> {
        let id = self.get_script_id(Some(&definition.name));
        if id != 0 {
            self.add_script(id, definition)?;
        }
        Ok(())
    }
    /// AddScript: ID0 is ignored even on an empty vector; overrides still increment count.
    pub fn add_script(&mut self, id: u32, definition: SourceScriptDefinition) -> Result<()> {
        if id == 0 {
            return Ok(());
        }
        let slot = self.scripts.get_mut(id as usize).ok_or_else(|| {
            refuse("execution.script_registry.ScriptDevAIMgr_431.index_undefined")
        })?;
        *slot = Some(definition);
        self.script_count = self.script_count.checked_add(1).ok_or_else(|| {
            refuse("execution.script_registry.ScriptDevAIMgr_432.count_overflow_undefined")
        })?;
        Ok(())
    }
    /// Preserve `!id || id < m_scripts.size()`, including ID0's empty-vector UB.
    pub fn get_script(&self, id: u32) -> Result<Option<&SourceScriptDefinition>> {
        if id == 0 || (id as usize) < self.scripts.len() {
            return self
                .scripts
                .get(id as usize)
                .map(Option::as_ref)
                .ok_or_else(|| {
                    refuse("execution.script_registry.ScriptDevAIMgr_439.zero_index_undefined")
                });
        }
        Ok(None)
    }
    /// World.cpp:1341, ScriptDevAIMgr.cpp:524-529. Missing slots are diagnostics, not failures.
    pub fn check_script_names(&self) -> Result<Vec<u32>> {
        let mut missing = Vec::new();
        for id in 1..self.names.len() {
            let slot = self.scripts.get(id).ok_or_else(|| {
                refuse("execution.script_registry.ScriptDevAIMgr_526.index_undefined")
            })?;
            if slot.is_none() {
                missing.push(id as u32);
            }
        }
        Ok(missing)
    }
    /// ScriptDevAIMgr.cpp:375-380. A nonnull factory must execute before its result is known.
    pub fn gameobject_ai(&self, id: u32) -> Result<()> {
        if let Some(script) = self.get_script(id)? {
            let nonnull = script
                .slots
                .get("GetGameObjectAI")
                .ok_or_else(|| refuse("execution.script_registry.GetGameObjectAI_slot_owner"))?;
            if *nonnull {
                return Err(refuse(
                    "execution.gameobject.loaded_script_get_gameobject_ai_owner",
                ));
            }
        }
        Ok(())
    }
    pub fn unexecuted_prerequisites(&self) -> &[UnexecutedScriptPrerequisite] {
        self.unexecuted
    }
    /// No script callback may read the independent, unexecuted startup products.
    pub fn require_prerequisite(&self, prerequisite: UnexecutedScriptPrerequisite) -> Result<()> {
        Err(refuse(prerequisite.owner()))
    }
    pub fn names(&self) -> &[Vec<u8>] {
        &self.names
    }
    pub fn size(&self) -> usize {
        self.scripts.len()
    }
    pub fn script_count(&self) -> i32 {
        self.script_count
    }
    pub fn null_slots(&self) -> usize {
        self.scripts.iter().filter(|slot| slot.is_none()).count()
    }
}

#[cfg(test)]
#[path = "script_registry_tests.rs"]
mod tests;
