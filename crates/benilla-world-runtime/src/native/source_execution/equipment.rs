// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! World.cpp:1015; ObjectMgr.cpp:1561-1603,4057; Creature.cpp:388-429,1771-1790,2747-2770.
use super::*;
use crate::native::production_compiler::VerifiedCatalog;
use std::collections::BTreeMap;
pub type LoadedEquipmentRow = SqlTemplateRow<4, 0>;
pub const EQUIPMENT_QUERIES: [&str; 3] = [
    "SELECT MAX(entry) FROM creature_equip_template",
    "SELECT COUNT(*) FROM creature_equip_template",
    "SELECT * FROM creature_equip_template",
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EquipmentItem {
    pub display: u32,
    pub class: u32,
    pub subclass: u32,
    pub material: u32,
    pub inventory_type: u32,
    pub sheath: u32,
}
#[derive(Debug)]
pub struct LoadedEquipmentTemplates {
    rows: BTreeMap<u32, [u32; 3]>,
    items: BTreeMap<u32, EquipmentItem>,
    pub notices: Vec<(u32, usize, u32)>,
    pub item_fixups: [usize; 4],
}
impl LoadedEquipmentTemplates {
    pub fn load(
        rows: Vec<LoadedEquipmentRow>,
        mut items: BTreeMap<u32, EquipmentItem>,
    ) -> Result<Self> {
        require(
            !rows.iter().any(|r| r.words[0] == u32::MAX),
            "execution.equipment.SQLStorageImpl_317.max_entry_index_undefined",
        )?;
        require(
            rows.windows(2).all(|p| p[0].words[0] < p[1].words[0]),
            "execution.equipment.primary_order",
        )?;
        require(
            !items.contains_key(&u32::MAX),
            "execution.equipment.item_SQLStorageImpl_317.max_entry_index_undefined",
        )?;
        let mut item_fixups = [0; 4];
        for (_, item) in items.range_mut(1..) {
            // ObjectMgr.cpp:2580-2589 fixes Class before indexing MaxItemSubclassValues.
            if item.class >= 16 {
                item.class = 15;
                item_fixups[0] += 1;
            }
            const SUBCLASSES: [u32; 16] = [9, 8, 21, 1, 10, 1, 5, 14, 1, 11, 1, 4, 1, 2, 1, 6];
            if item.subclass >= SUBCLASSES[item.class as usize] {
                item.subclass = 0;
                item_fixups[1] += 1;
            }
            // ObjectMgr.cpp:2604-2607,2763-2766; other loop fixups never write these six fields.
            if item.inventory_type >= 29 {
                item.inventory_type = 0;
                item_fixups[2] += 1;
            }
            if item.sheath >= 8 {
                item.sheath = 0;
                item_fixups[3] += 1;
            }
        }
        let mut this = Self {
            rows: BTreeMap::new(),
            items,
            notices: Vec::new(),
            item_fixups,
        };
        for row in rows {
            let [entry, a, b, c] = row.words;
            this.rows.insert(entry, [a, b, c]);
        }
        // ObjectMgr.cpp:1565-1597 validates ascending storage IDs, then slots 0,1,2; no row is erased.
        for (&entry, slots) in &mut this.rows {
            for (slot, id) in slots.iter_mut().enumerate() {
                if *id == 0 {
                    continue;
                }
                let Some(item) = this.items.get(id) else {
                    this.notices.push((entry, slot, 1580));
                    *id = 0;
                    continue;
                };
                if !matches!(
                    item.inventory_type,
                    13 | 14 | 15 | 17 | 21 | 22 | 23 | 25 | 26 | 28
                ) {
                    this.notices.push((entry, slot, 1596));
                    *id = 0;
                }
            }
        }
        Ok(this)
    }
    pub fn get(&self, entry: u32) -> Option<&[u32; 3]> {
        self.rows.get(&entry)
    }
    pub fn len(&self) -> usize {
        self.rows.len()
    }
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
    pub fn catalog_diagnostic(catalog: &mut VerifiedCatalog) -> Result<Self> {
        let rows = catalog.execution_equipment_rows()?;
        let items = catalog.execution_equipment_items()?;
        Self::load(rows, items)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceVirtualItem {
    pub display: u32,
    pub info: [u32; 2],
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceCreatureEquipment {
    pub id: u32,
    pub slots: [SourceVirtualItem; 3],
    pub missing_items: Vec<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CreatureEquipmentBranch {
    EventEquipment,
    EventEntry,
    EventUnchanged,
    NormalTemplate,
    DifficultyTemplate,
    SpawnEquipment,
    SpawnNone,
}
impl CreatureEquipmentBranch {
    pub fn select(
        event: Option<(u32, u32)>,
        spawn: Option<i32>,
        normal: u32,
        difficulty: u32,
    ) -> (Self, Option<(u32, bool)>, bool) {
        if let Some((equipment, entry)) = event {
            if equipment != 0 {
                (Self::EventEquipment, Some((equipment, false)), false)
            } else if entry != 0 {
                (Self::EventEntry, Some((difficulty, true)), false)
            } else {
                (Self::EventUnchanged, None, false)
            }
        } else if spawn.is_none() || spawn == Some(-1) {
            if difficulty == 0 {
                (Self::NormalTemplate, Some((normal, true)), true)
            } else {
                (Self::DifficultyTemplate, Some((difficulty, false)), true)
            }
        } else if let Some(id) = spawn.filter(|id| *id != 0) {
            (Self::SpawnEquipment, Some((id as u32, false)), false)
        } else {
            (Self::SpawnNone, None, false)
        }
    }
}
impl SourceCreatureEquipment {
    pub fn set_virtual_item(
        &mut self,
        slot: usize,
        item: u32,
        templates: &LoadedEquipmentTemplates,
    ) {
        // Creature.cpp:2749-2761 clears zero before lookup; an absent prototype leaves the slot unchanged.
        if item == 0 {
            self.slots[slot] = SourceVirtualItem::default();
            return;
        }
        let Some(proto) = templates.items.get(&item) else {
            self.missing_items.push(item);
            return;
        };
        let old = self.slots[slot].info[1];
        self.slots[slot] = SourceVirtualItem {
            display: proto.display,
            info: [
                u32::from_le_bytes([
                    proto.class as u8,
                    proto.subclass as u8,
                    proto.material as u8,
                    proto.inventory_type as u8,
                ]),
                (old & !255) | u32::from(proto.sheath as u8),
            ],
        };
    }
    pub fn load(&mut self, entry: u32, force: bool, templates: &LoadedEquipmentTemplates) {
        if entry == 0 {
            if force {
                for slot in 0..3 {
                    self.set_virtual_item(slot, 0, templates);
                }
                self.id = 0;
            }
            return;
        }
        if let Some(slots) = templates.get(entry) {
            self.id = entry;
            for (slot, &item) in slots.iter().enumerate() {
                self.set_virtual_item(slot, item, templates);
            }
        }
    }
    pub fn init_entry(
        &mut self,
        event: Option<(u32, u32)>,
        spawn: Option<i32>,
        normal: u32,
        difficulty: u32,
        can_wield_loot: bool,
        templates: &LoadedEquipmentTemplates,
    ) -> Result<CreatureEquipmentBranch> {
        let (branch, call, loot) =
            CreatureEquipmentBranch::select(event, spawn, normal, difficulty);
        if let Some((id, force)) = call {
            self.load(id, force, templates);
        }
        if loot && can_wield_loot {
            return Err(refuse(
                "execution.equipment.Creature_405.PrepareBodyLootState_owner",
            ));
        }
        Ok(branch)
    }
}
#[cfg(test)]
#[path = "equipment_tests.rs"]
mod tests;
