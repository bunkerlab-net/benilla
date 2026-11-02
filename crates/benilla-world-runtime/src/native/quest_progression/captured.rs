// SPDX-License-Identifier: GPL-3.0-or-later
//! Binds the checked ordinary loader output to independently constructed progression inputs.
use super::*;
impl Profile {
    pub fn initialize_captured_quest_progression(
        &mut self,
        starter: &init::StarterInput,
        captured: &q::CompiledOrdinaryQuests,
        mut input: QuestProgressionInput,
    ) -> Result<(), NativeError> {
        let interactions = self
            .interactions
            .as_ref()
            .ok_or(NativeError::InvalidProfile)?;
        for giver in captured.givers() {
            let npc = self.npc(giver.spawn).ok_or(NativeError::InvalidProfile)?;
            let source = interactions
                .quest_giver_source(giver.entry)
                .ok_or(NativeError::InvalidProfile)?;
            if npc.entry != giver.entry
                || giver.generation != 0
                || u64::from(giver.database_guid) != giver.spawn
                || source.npc_flags != giver.npc_flags
                || source.unit_flags != giver.unit_flags
            {
                return Err(NativeError::AuthorityBinding);
            }
        }
        let mut npcs = BTreeMap::new();
        for giver in captured.givers() {
            let row = QuestNpcRow {
                entry: giver.entry,
                npc_flags: giver.npc_flags,
                unit_flags: giver.unit_flags,
            };
            if npcs
                .get(&row.entry)
                .is_some_and(|previous| previous != &row)
            {
                return Err(NativeError::AuthorityBinding);
            }
            npcs.insert(row.entry, row);
        }
        input.quests = captured.quests().to_vec();
        input.relations = captured.relations().clone();
        input.quest_audit = captured.audit().clone();
        input.quest_relationships = captured.relationships().clone();
        input.npc_rows = npcs.into_values().collect();
        self.initialize_quest_progression(starter, input)
    }
}
