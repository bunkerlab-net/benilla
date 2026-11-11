// SPDX-License-Identifier: GPL-3.0-or-later
//! Effective SpecialFlags only, not a complete loaded quest gameplay manager.
//! World.cpp:1101,1146,1217-1226 loads quests, triggers, then DB scripts.
use super::*;
use std::collections::BTreeMap;

const EXPLORATION: u32 = 0x02;
const DELIVER: u32 = 0x08;
const KILL_SPEAK: u32 = 0x30;
const TIMED: u32 = 0x40;

#[derive(Clone, Debug, Default)]
pub struct QuestFlagRow {
    pub id: u32,
    pub special_flags: u16,
    pub required_items: [u32; 4],
    pub required_spells: [u32; 4],
    pub required_targets: [i32; 4],
    pub limit_time: u32,
}

pub trait QuestFlagLoaderOwner {
    /// None is a checked absent SpellEntry, not an unavailable store.
    fn spell_effects(&mut self, _id: u32) -> Result<Option<[(u32, i32); 3]>> {
        Err(refuse("execution.quest_flags.spell_template_owner"))
    }
    fn area_trigger(&mut self, _id: u32) -> Result<bool> {
        Err(refuse("execution.quest_flags.area_trigger_dbc_owner"))
    }
}

#[derive(Debug)]
pub struct SourceQuestFlags {
    flags: BTreeMap<u32, u32>,
    // Unique quests changed at ObjectMgr.cpp:4513,4584,4618,4833,6290.
    changes: [usize; 5],
    area_triggers_loaded: bool,
}
impl SourceQuestFlags {
    pub fn load_quests(
        rows: impl IntoIterator<Item = QuestFlagRow>,
        owner: &mut impl QuestFlagLoaderOwner,
    ) -> Result<Self> {
        // ObjectMgr.cpp:4293-4295 keeps the first insertion. QuestMap is unordered,
        // ObjectMgr.h:486; these per-quest OR mutations have no cross-quest reads.
        let mut quests = BTreeMap::new();
        for row in rows {
            quests.entry(row.id).or_insert(row);
        }
        let mut result = Self {
            flags: BTreeMap::new(),
            changes: [0; 5],
            area_triggers_loaded: false,
        };
        for (id, row) in quests {
            // QuestDef.cpp:45 reads uint16; ObjectMgr.cpp:4314-4317 only logs.
            let mut flags = u32::from(row.special_flags);
            if row.required_items.iter().any(|&item| item != 0) {
                // :4504-4519 sets DELIVER before checking item existence/count.
                result.changes[0] += usize::from(flags & DELIVER == 0);
                flags |= DELIVER;
            }
            let before = flags;
            for (&spell, &target) in row.required_spells.iter().zip(&row.required_targets) {
                if spell == 0 {
                    continue;
                }
                let Some(effects) = owner.spell_effects(spell)? else {
                    continue; // ObjectMgr.cpp:4556-4561.
                };
                if target == 0
                    && effects
                        .iter()
                        .any(|&(effect, misc)| (effect == 16 && misc as u32 == id) || effect == 61)
                {
                    flags |= EXPLORATION; // ObjectMgr.cpp:4564-4584.
                }
            }
            result.changes[1] += usize::from(flags != before);
            // ObjectMgr.cpp:4600 negates the signed target before setting flags.
            require(
                !row.required_targets.contains(&i32::MIN),
                "execution.quest_flags.target_negation_undefined",
            )?;
            if row.required_targets.iter().any(|&target| target != 0) {
                // :4599-4618 uses cached id even after a missing target is zeroed.
                result.changes[2] += usize::from(flags & KILL_SPEAK != KILL_SPEAK);
                flags |= KILL_SPEAK;
            }
            if row.limit_time != 0 {
                result.changes[3] += usize::from(flags & TIMED == 0);
                flags |= TIMED; // ObjectMgr.cpp:4832-4833.
            }
            result.flags.insert(id, flags);
        }
        Ok(result)
    }
    pub fn load_area_triggers(
        &mut self,
        rows: impl IntoIterator<Item = [u32; 2]>,
        owner: &mut impl QuestFlagLoaderOwner,
    ) -> Result<()> {
        self.area_triggers_loaded = false;
        for [trigger, quest] in rows {
            // ObjectMgr.cpp:6271-6283 checks DBC existence before quest existence.
            if !owner.area_trigger(trigger)? {
                continue;
            }
            if let Some(flags) = self.flags.get_mut(&quest) {
                self.changes[4] += usize::from(*flags & EXPLORATION == 0);
                *flags |= EXPLORATION; // ObjectMgr.cpp:6285-6290.
            }
        }
        self.area_triggers_loaded = true;
        Ok(())
    }
    pub fn flags(&self, id: u32) -> Option<u32> {
        self.flags.get(&id).copied()
    }
    pub fn mutation_counts(&self) -> [usize; 5] {
        self.changes
    }
    pub fn quest_count(&self) -> usize {
        self.flags.len()
    }
}
impl DbScriptLoaderOwner for SourceQuestFlags {
    fn quest_exploration(&mut self, id: u32) -> Result<Option<bool>> {
        require(
            self.area_triggers_loaded,
            "execution.quest_flags.quest_area_triggers_owner",
        )?;
        Ok(self.flags(id).map(|flags| flags & EXPLORATION != 0))
    }
    fn set_quest_exploration(&mut self, id: u32) -> Result<()> {
        require(
            self.area_triggers_loaded,
            "execution.quest_flags.quest_area_triggers_owner",
        )?;
        let flags = self
            .flags
            .get_mut(&id)
            .ok_or_else(|| refuse("execution.quest_flags.quest_missing"))?;
        *flags |= EXPLORATION; // ScriptMgr.cpp:330, before distance validation.
        Ok(())
    }
}

#[cfg(test)]
#[path = "quest_flags_tests.rs"]
mod tests;
