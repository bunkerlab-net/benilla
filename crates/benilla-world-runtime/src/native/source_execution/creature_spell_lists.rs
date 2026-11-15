// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! ObjectMgr.cpp:979-1117,9754-9861; MapDataContainer.cpp:29,46-53.
use super::*;
use crate::native::production_compiler::{CreatureSpellCatalogRows, VerifiedCatalog};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub type CreatureSpellTargetRow = SqlTemplateRow<6, 1>;
pub type CreatureSpellEntryRow = SqlTemplateRow<3, 1>;
pub type CreatureSpellRow = SqlTemplateRow<13, 0>;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CreatureSpellTarget {
    pub words: [u32; 6],
    pub comment: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatureListSpell {
    pub id: u32,
    pub position: u32,
    pub spell_id: u32,
    pub flags: u32,
    pub script_id: u32,
    pub availability: u32,
    pub probability: u32,
    pub initial: [u32; 2],
    pub repeat: [u32; 2],
    // ObjectMgr.cpp:9797-9815 never writes this member on legacy spells.
    combat_condition: Option<i32>,
    pub target: Arc<CreatureSpellTarget>,
    pub disabled_for_ai: bool,
}
impl CreatureListSpell {
    /// UnitAI.cpp:1213 is the first value read, not the copies at :9815/Creature.cpp:2443.
    pub fn combat_condition(&self) -> Result<i32> {
        self.combat_condition.ok_or_else(|| {
            refuse("execution.creature_spell_lists.UnitAI_1213.unwritten_combat_condition")
        })
    }
    pub fn has_written_combat_condition(&self) -> bool {
        self.combat_condition.is_some()
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CreatureSpellList {
    pub id: u32,
    pub name: Vec<u8>,
    pub chance_support: u32,
    pub chance_ranged: u32,
    pub disabled: bool,
    pub spells: BTreeMap<u32, CreatureListSpell>,
}
#[derive(Clone, Debug, Default)]
pub struct LoadedCreatureSpellLists {
    pub lists: BTreeMap<u32, CreatureSpellList>,
    pub targeting: BTreeMap<u32, Arc<CreatureSpellTarget>>,
}
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CreatureSpellListCensus {
    pub visited: [usize; 3],
    pub loaded: [usize; 3],
    pub dropped: BTreeMap<u32, usize>,
    // ObjectMgr.cpp:982,1114 never increments count.
    pub source_logged_count: u32,
}
#[derive(Debug)]
pub struct CreatureSpellStartupCensus {
    pub modern: CreatureSpellListCensus,
    pub cooldown_rows: usize,
    pub cooldowns: usize,
    pub cooldown_dropped: BTreeMap<u32, usize>,
    pub legacy: LegacySpellListCensus,
}
impl LoadedCreatureSpellLists {
    pub fn load_catalog(
        catalog: &mut VerifiedCatalog,
        creatures: &LoadedCreatureTemplates,
    ) -> Result<(Self, CreatureSpellStartupCensus)> {
        Ok(Self::from_catalog_rows(
            catalog.execution_creature_spell_lists()?,
            creatures,
        ))
    }
    pub(crate) fn from_catalog_rows(
        rows: CreatureSpellCatalogRows,
        creatures: &LoadedCreatureTemplates,
    ) -> (Self, CreatureSpellStartupCensus) {
        let ids = creatures.rows().iter().map(|r| r.words[0]).collect();
        let (mut store, modern) =
            Self::load(rows.targets, rows.entries, rows.spells, &rows.attributes_ex);
        let mut cooldowns = BTreeMap::new();
        let cooldown_dropped =
            Self::load_cooldowns(&rows.cooldowns, &ids, &rows.attributes_ex, &mut cooldowns);
        let legacy = store.load_legacy(&rows.legacy, &ids, &rows.attributes_ex, &cooldowns);
        let census = CreatureSpellStartupCensus {
            modern,
            cooldown_rows: rows.cooldowns.len(),
            cooldowns: cooldowns.len(),
            cooldown_dropped,
            legacy,
        };
        (store, census)
    }
    pub fn load(
        targeting: Vec<CreatureSpellTargetRow>,
        entries: Vec<CreatureSpellEntryRow>,
        spells: Vec<CreatureSpellRow>,
        spell_attributes_ex: &BTreeMap<u32, u32>,
    ) -> (Self, CreatureSpellListCensus) {
        let mut store = Self::default();
        let mut census = CreatureSpellListCensus::default();
        for row in targeting {
            census.visited[0] += 1;
            if row.words[1] > 3 {
                // CreatureSpellList.h:26-32 accepts MAX itself.
                *census.dropped.entry(995).or_default() += 1;
                continue;
            }
            store.targeting.insert(
                row.words[0],
                Arc::new(CreatureSpellTarget {
                    words: row.words,
                    comment: row.strings.into_iter().next().unwrap(),
                }),
            );
            census.loaded[0] += 1;
        }
        for row in entries {
            census.visited[1] += 1;
            if row.strings[0].is_empty() {
                *census.dropped.entry(1019).or_default() += 1;
                continue;
            }
            store.lists.insert(
                row.words[0],
                CreatureSpellList {
                    id: row.words[0],
                    name: row.strings.into_iter().next().unwrap(),
                    chance_support: row.words[1],
                    chance_ranged: row.words[2],
                    disabled: false,
                    spells: BTreeMap::new(),
                },
            );
            census.loaded[1] += 1;
        }
        for row in spells {
            census.visited[2] += 1;
            let w = row.words;
            let Some(list) = store.lists.get_mut(&w[0]) else {
                *census.dropped.entry(1046).or_default() += 1;
                continue;
            };
            let info = spell_attributes_ex.get(&w[2]);
            let reason = if info.is_none() && w[2] != 2 {
                Some(1053)
            }
            // Source quirk: lookup SpellId in the position-keyed map, not Position.
            else if list.spells.contains_key(&w[2]) {
                Some(1059)
            } else if !store.targeting.contains_key(&w[5]) {
                Some(1067)
            } else if w[3] & 2 != 0 && list.chance_ranged == 0 {
                Some(1073)
            } else if w[3] & 1 != 0 && list.chance_support == 0 {
                Some(1079)
            } else if w[9] > w[10] {
                Some(1095)
            } else if w[11] > w[12] {
                Some(1101)
            } else {
                None
            };
            if let Some(reason) = reason {
                *census.dropped.entry(reason).or_default() += 1;
                continue;
            }
            let spell = CreatureListSpell {
                id: w[0],
                position: w[1],
                spell_id: w[2],
                flags: w[3],
                script_id: w[6],
                availability: w[7],
                probability: w[8],
                initial: [w[9], w[10]],
                repeat: [w[11], w[12]],
                combat_condition: Some(w[4] as i32),
                target: Arc::clone(&store.targeting[&w[5]]),
                disabled_for_ai: info.is_none_or(|v| v & 0x20000 != 0),
            };
            if let std::collections::btree_map::Entry::Vacant(entry) = list.spells.entry(w[1]) {
                entry.insert(spell);
                census.loaded[2] += 1;
            } else {
                // emplace silently retains the earlier position at ObjectMgr.cpp:1109.
                *census.dropped.entry(1109).or_default() += 1;
            }
        }
        (store, census)
    }
    /// ObjectMgr.cpp:9824-9861, including the absent min/max ordering check.
    pub fn load_cooldowns(
        rows: &[[u32; 4]],
        creatures: &BTreeSet<u32>,
        spells: &BTreeMap<u32, u32>,
        cooldowns: &mut BTreeMap<(u32, u32), [u32; 2]>,
    ) -> BTreeMap<u32, usize> {
        let mut dropped = BTreeMap::new();
        for &[entry, spell, min, max] in rows {
            let reason = if !creatures.contains(&entry) {
                Some(9837)
            } else if !spells.contains_key(&spell) {
                Some(9843)
            } else if min == 0 && max == 0 {
                Some(9850)
            } else {
                None
            };
            if let Some(reason) = reason {
                *dropped.entry(reason).or_default() += 1;
            } else {
                cooldowns.insert((entry, spell), [min, max]);
            }
        }
        dropped
    }
    /// The map[] list and targeting values are value-initialized, unlike local `spell`.
    pub fn load_legacy(
        &mut self,
        rows: &[[u32; 12]],
        creatures: &BTreeSet<u32>,
        spells: &BTreeMap<u32, u32>,
        cooldowns: &BTreeMap<(u32, u32), [u32; 2]>,
    ) -> LegacySpellListCensus {
        let mut census = LegacySpellListCensus::default();
        for row in rows {
            census.rows += 1;
            let entry = row[0];
            let key = entry.wrapping_mul(100).wrapping_add(row[1]);
            if !creatures.contains(&entry) {
                *census.dropped.entry(9768).or_default() += 1;
                continue;
            }
            if self.lists.contains_key(&key) {
                *census.dropped.entry(9774).or_default() += 1;
                continue;
            }
            let list = self.lists.entry(key).or_default();
            list.disabled = true;
            census.lists += 1;
            for (position, &id) in row[2..].iter().enumerate() {
                if id == 0 {
                    continue;
                }
                let info = spells.get(&id);
                if info.is_none() && id != 2 {
                    *census.dropped.entry(9791).or_default() += 1;
                    continue;
                }
                let target = self.targeting.entry(1).or_default();
                let repeat = cooldowns.get(&(entry, id)).copied().unwrap_or([0; 2]);
                list.spells.insert(
                    position as u32,
                    CreatureListSpell {
                        id: key,
                        position: position as u32,
                        spell_id: id,
                        flags: 0,
                        script_id: 0,
                        availability: 100,
                        probability: 0,
                        initial: [0; 2],
                        repeat,
                        combat_condition: None,
                        target: Arc::clone(target),
                        disabled_for_ai: info.is_none_or(|v| v & 0x20000 != 0),
                    },
                );
                census.unwritten_combat_conditions += 1;
            }
        }
        census
    }
    pub fn get(&self, key: u32) -> Option<&CreatureSpellList> {
        self.lists.get(&key)
    }
}
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LegacySpellListCensus {
    pub rows: usize,
    pub lists: usize,
    pub unwritten_combat_conditions: usize,
    pub dropped: BTreeMap<u32, usize>,
}
/// MapDataContainer.cpp:29 holds shared ownership, so reload does not change existing maps.
#[derive(Clone, Debug)]
pub struct CreatureSpellMapData(pub Arc<LoadedCreatureSpellLists>);
impl CreatureSpellMapData {
    pub fn get(&self, key: u32) -> Option<&CreatureSpellList> {
        self.0.get(key)
    }
}
impl CreatureSpellList {
    /// Creature.cpp:2426-2460. The caller invokes SpellListChanged only when this returns true.
    pub fn set(
        &mut self,
        key: u32,
        map: Option<&CreatureSpellMapData>,
        random: &mut dyn CreatureRandom,
    ) -> Result<bool> {
        if key == 0 {
            self.disabled = true;
            self.spells.clear();
            return Ok(true);
        }
        let map = map.ok_or_else(|| refuse("execution.creature.loaded_spell_list_store_owner"))?;
        let Some(list) = map.get(key) else {
            return Ok(false);
        };
        self.clone_from(list);
        self.filter_availability(random)?;
        Ok(true)
    }
    pub(super) fn filter_availability(&mut self, random: &mut dyn CreatureRandom) -> Result<()> {
        let mut failure = None;
        self.spells.retain(|_, spell| {
            if failure.is_some() || spell.availability >= 100 {
                return true;
            }
            match random.creature_urand(0, 100) {
                Ok(roll) => roll <= spell.availability,
                Err(error) => {
                    failure = Some(error);
                    true
                }
            }
        });
        if let Some(error) = failure {
            return Err(error);
        }
        Ok(())
    }
    /// UnitAI.cpp:104-110 checks Disabled on combat entry, not in UpdateSpellLists.
    pub fn ai_has_spells(&self) -> bool {
        !self.disabled && !self.spells.is_empty()
    }
}
#[cfg(test)]
#[path = "creature_spell_lists_tests.rs"]
mod tests;
