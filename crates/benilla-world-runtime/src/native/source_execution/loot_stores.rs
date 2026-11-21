// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::identity_table::IdentityTable;
use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum LootStoreKind {
    Creature,
    Fishing,
    Gameobject,
    Item,
    Mail,
    Pickpocketing,
    Skinning,
    Disenchant,
    Reference,
}

macro_rules! loot_table {
    ($name:literal) => {
        (
            concat!($name, "_loot_template"),
            concat!(
                "SELECT entry, item, ChanceOrQuestChance, groupid, mincountOrRef, maxcount, condition_id FROM ",
                $name,
                "_loot_template"
            ),
        )
    };
}

impl LootStoreKind {
    // LootMgr.h:473-485 orders the World.cpp:1200 loads.
    pub const LOAD_ORDER: [Self; 9] = [
        Self::Creature,
        Self::Fishing,
        Self::Gameobject,
        Self::Item,
        Self::Mail,
        Self::Pickpocketing,
        Self::Skinning,
        Self::Disenchant,
        Self::Reference,
    ];

    // LootMgr.cpp:49-57,135 names the tables and projects these columns.
    fn relation(self) -> (&'static str, &'static str) {
        match self {
            Self::Creature => loot_table!("creature"),
            Self::Fishing => loot_table!("fishing"),
            Self::Gameobject => loot_table!("gameobject"),
            Self::Item => loot_table!("item"),
            Self::Mail => loot_table!("mail"),
            Self::Pickpocketing => loot_table!("pickpocketing"),
            Self::Skinning => loot_table!("skinning"),
            Self::Disenchant => loot_table!("disenchant"),
            Self::Reference => loot_table!("reference"),
        }
    }

    pub fn table(self) -> &'static str {
        self.relation().0
    }

    pub fn query(self) -> &'static str {
        self.relation().1
    }

    // LootMgr.cpp:49-57 disables loot rate modifiers for Mail and Reference.
    pub fn rates_allowed(self) -> bool {
        !matches!(self, Self::Mail | Self::Reference)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LootRow {
    pub entry: u32,
    pub item: u32,
    pub chance: f32,
    pub group: u8,
    pub min_count_or_ref: i32,
    pub max_count: u32,
    pub condition: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LootRowRejection {
    MaxCountWidth,
    MissingCondition,
    ConditionRequiresPlayer,
    GroupWidth,
    ZeroMinimum,
    MissingItem,
    UngroupedZeroChance,
    LowChance,
    CountRange,
    NegativeReferenceChance,
}

#[derive(Debug, PartialEq)]
pub enum LootNotice {
    Empty {
        kind: LootStoreKind,
    },
    Rejected {
        kind: LootStoreKind,
        entry: u32,
        item: u32,
        reason: LootRowRejection,
    },
    GroupChanceAbove101 {
        kind: LootStoreKind,
        entry: u32,
        group: u8,
        chance: f32,
    },
    EqualChanceGroupExhausted {
        kind: LootStoreKind,
        entry: u32,
        group: u8,
        chance: f32,
    },
}

pub trait LootInputs {
    fn rows(&mut self, _kind: LootStoreKind, _sql: &str) -> Result<Vec<LootRow>> {
        Err(refuse("execution.loot.LootMgr_135.rows_owner"))
    }

    fn condition_exists(&mut self, _id: u16) -> Result<bool> {
        Err(refuse("execution.loot.LootMgr_162.condition_owner"))
    }

    fn condition_without_player(&mut self, _id: u16) -> Result<bool> {
        Err(refuse(
            "execution.loot.LootMgr_169.condition_without_player_owner",
        ))
    }

    fn item_exists(&mut self, _id: u32) -> Result<bool> {
        Err(refuse("execution.loot.LootMgr_327.item_prototype_owner"))
    }

    fn notice(&mut self, _notice: LootNotice) {}
}

#[derive(Debug)]
pub struct SourceLootItem {
    pub item_index: u32,
    pub item: u32,
    pub chance: f32,
    pub min_count_or_ref: i32,
    pub condition: u16,
    pub max_count: u8,
    group_and_quest: u8,
}

impl SourceLootItem {
    // LootMgr.h:202-218 splits signed chance into magnitude and the quest flag.
    fn new(item_index: u32, row: LootRow) -> Self {
        Self {
            item_index,
            item: row.item,
            chance: row.chance.abs(),
            min_count_or_ref: row.min_count_or_ref,
            max_count: row.max_count as u8,
            condition: row.condition,
            group_and_quest: row.group | (u8::from(row.chance < 0.0) << 7),
        }
    }

    pub fn group(&self) -> u8 {
        self.group_and_quest & 0x7f
    }

    pub fn needs_quest(&self) -> bool {
        self.group_and_quest & 0x80 != 0
    }
}

#[derive(Debug, Default)]
pub struct SourceLootGroup {
    pub explicit: Vec<SourceLootItem>,
    pub equal: Vec<SourceLootItem>,
}

impl SourceLootGroup {
    // LootMgr.cpp:2397-2403 splits explicit chance and equal chance rows.
    fn add_entry(&mut self, item: SourceLootItem) {
        if item.chance != 0.0 {
            self.explicit.push(item);
        } else {
            self.equal.push(item);
        }
    }

    // LootMgr.cpp:2547-2556 excludes quest rows from the explicit chance sum.
    fn raw_total_chance(&self) -> f32 {
        self.explicit
            .iter()
            .filter(|item| !item.needs_quest())
            .fold(0.0, |sum, item| sum + item.chance)
    }

    // LootMgr.cpp:2569-2580 reports group totals without changing the rows.
    fn verify(&self, kind: LootStoreKind, entry: u32, group: u8, inputs: &mut impl LootInputs) {
        let chance = self.raw_total_chance();
        if chance > 101.0 {
            inputs.notice(LootNotice::GroupChanceAbove101 {
                kind,
                entry,
                group,
                chance,
            });
        }
        if chance >= 100.0 && !self.equal.is_empty() {
            inputs.notice(LootNotice::EqualChanceGroupExhausted {
                kind,
                entry,
                group,
                chance,
            });
        }
    }
}

#[derive(Debug, Default)]
pub struct SourceLootTemplate {
    pub entries: Vec<SourceLootItem>,
    pub groups: Vec<SourceLootGroup>,
    row_count: u32,
}

impl SourceLootTemplate {
    // LootMgr.cpp:180-182,2606-2616 indexes admitted rows before splitting groups.
    fn add_entry(&mut self, row: LootRow) {
        self.row_count = self.row_count.wrapping_add(1);
        let item = SourceLootItem::new(self.row_count, row);
        let group = usize::from(item.group());
        if group == 0 {
            self.entries.push(item);
        } else {
            if group > self.groups.len() {
                self.groups.resize_with(group, SourceLootGroup::default);
            }
            self.groups[group - 1].add_entry(item);
        }
    }

    // LootMgr.cpp:2753-2760 verifies groups in index order.
    fn verify(&self, kind: LootStoreKind, entry: u32, inputs: &mut impl LootInputs) {
        for (index, group) in self.groups.iter().enumerate() {
            group.verify(kind, entry, (index + 1) as u8, inputs);
        }
    }

    pub fn row_count(&self) -> u32 {
        self.row_count
    }
}

#[derive(Debug, Default)]
pub struct SourceLootStore {
    templates: IdentityTable<SourceLootTemplate>,
}

impl SourceLootStore {
    // LootMgr.cpp:125-196 validates and inserts rows in query order.
    fn load(kind: LootStoreKind, inputs: &mut impl LootInputs) -> Result<Self> {
        let mut store = Self::default();
        let rows = inputs.rows(kind, kind.query())?;
        if rows.is_empty() {
            inputs.notice(LootNotice::Empty { kind });
        }
        for row in rows {
            let reason = if row.max_count > u32::from(u8::MAX) {
                Some(LootRowRejection::MaxCountWidth)
            } else if row.condition != 0 && !inputs.condition_exists(row.condition)? {
                Some(LootRowRejection::MissingCondition)
            } else if row.condition != 0
                && row.min_count_or_ref < 0
                && !inputs.condition_without_player(row.condition)?
            {
                Some(LootRowRejection::ConditionRequiresPlayer)
            } else {
                Self::valid_item_template(row, inputs)?
            };
            if let Some(reason) = reason {
                inputs.notice(LootNotice::Rejected {
                    kind,
                    entry: row.entry,
                    item: row.item,
                    reason,
                });
                continue;
            }
            let key = u64::from(row.entry);
            if let Some(template) = store.templates.get_mut(key) {
                template.add_entry(row);
            } else {
                let mut template = SourceLootTemplate::default();
                template.add_entry(row);
                store.templates.insert(key, template, usize::MAX)?;
            }
        }
        store.verify(kind, inputs);
        Ok(store)
    }

    // LootMgr.cpp:67-71 visits every template after loading the result.
    fn verify(&self, kind: LootStoreKind, inputs: &mut impl LootInputs) {
        for (entry, template) in self.templates() {
            template.verify(kind, entry, inputs);
        }
    }

    // LootMgr.cpp:219-227 looks up a full live template.
    pub fn get_loot_for(&self, entry: u32) -> Option<&SourceLootTemplate> {
        self.templates.get(u64::from(entry))
    }

    // LootMgr.cpp:305-308 checks the loaded template key.
    pub fn have_loot_for(&self, entry: u32) -> bool {
        self.get_loot_for(entry).is_some()
    }

    // LootMgr.cpp:310-366 tests item and reference rows before insertion.
    fn valid_item_template(
        row: LootRow,
        inputs: &mut impl LootInputs,
    ) -> Result<Option<LootRowRejection>> {
        use LootRowRejection::*;
        let chance = row.chance.abs();
        Ok(if row.group >= 128 {
            Some(GroupWidth)
        } else if row.min_count_or_ref == 0 {
            Some(ZeroMinimum)
        } else if row.min_count_or_ref > 0 {
            if !inputs.item_exists(row.item)? {
                Some(MissingItem)
            } else if chance == 0.0 && row.group == 0 {
                Some(UngroupedZeroChance)
            } else if chance != 0.0 && chance < 0.000001 {
                Some(LowChance)
            } else if row.max_count < row.min_count_or_ref as u32 {
                Some(CountRange)
            } else {
                None
            }
        } else if row.chance < 0.0 {
            Some(NegativeReferenceChance)
        } else if chance == 0.0 && row.group == 0 {
            Some(UngroupedZeroChance)
        } else {
            None
        })
    }

    pub fn templates(&self) -> impl Iterator<Item = (u32, &SourceLootTemplate)> {
        self.templates
            .iter()
            .map(|(id, template)| (id as u32, template))
    }
}

#[derive(Debug)]
pub struct SourceLootStores {
    stores: [SourceLootStore; LootStoreKind::LOAD_ORDER.len()],
    unused_reference_ids: BTreeSet<u32>,
}

impl SourceLootStores {
    // World.cpp:1199-1200; LootMgr.h:473-485 loads the family and collects reference IDs.
    pub fn load(inputs: &mut impl LootInputs) -> Result<Self> {
        let mut stores = std::array::from_fn(|_| SourceLootStore::default());
        for (slot, kind) in stores.iter_mut().zip(LootStoreKind::LOAD_ORDER) {
            *slot = SourceLootStore::load(kind, inputs)?;
        }
        // LootMgr.cpp:229-235,2971-2974 seeds the later reference audit.
        let unused_reference_ids = stores[LootStoreKind::Reference as usize]
            .templates()
            .map(|(entry, _)| entry)
            .collect();
        Ok(Self {
            stores,
            unused_reference_ids,
        })
    }

    pub fn store(&self, kind: LootStoreKind) -> &SourceLootStore {
        &self.stores[kind as usize]
    }

    pub fn unused_reference_ids(&self) -> &BTreeSet<u32> {
        &self.unused_reference_ids
    }
}

#[path = "loot_reference_audit.rs"]
mod reference_audit;
pub use reference_audit::LootReferenceNotice;

#[cfg(test)]
#[path = "loot_stores_tests.rs"]
mod tests;
