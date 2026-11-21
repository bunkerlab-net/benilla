// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;
use std::collections::BTreeSet;

pub const REFERENCE_LOOT_QUERY: &str = "SELECT entry, item, ChanceOrQuestChance, groupid, mincountOrRef, maxcount, condition_id FROM reference_loot_template";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReferenceLootRow {
    pub entry: u32,
    pub item: u32,
    pub chance: f32,
    pub group: u8,
    pub min_count_or_ref: i32,
    pub max_count: u32,
    pub condition: u16,
}

pub trait ReferenceLootInputs {
    fn rows(&mut self, sql: &str) -> Result<Vec<ReferenceLootRow>>;
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
}

#[derive(Default)]
pub struct SourceReferenceLootStore {
    templates: BTreeSet<u32>,
}

impl SourceReferenceLootStore {
    // World.cpp:1200; LootMgr.cpp:125-196,2971-2975 loads LootTemplates_Reference.
    pub fn load(&mut self, inputs: &mut impl ReferenceLootInputs) -> Result<usize> {
        self.templates.clear();
        let mut accepted = 0;
        for row in inputs.rows(REFERENCE_LOOT_QUERY)? {
            if row.max_count > u8::MAX.into() {
                continue;
            }
            if row.condition != 0 {
                if !inputs.condition_exists(row.condition)? {
                    continue;
                }
                if row.min_count_or_ref < 0 && !inputs.condition_without_player(row.condition)? {
                    continue;
                }
            }
            if valid_template_row(row, inputs)? {
                self.templates.insert(row.entry);
                accepted += 1;
            }
        }
        Ok(accepted)
    }

    // LootMgr.cpp:305-308,3352-3355 checks the loaded template key.
    pub fn have_loot_for(&self, id: u32) -> bool {
        self.templates.contains(&id)
    }

    pub fn template_ids(&self) -> &BTreeSet<u32> {
        &self.templates
    }
}

// LootMgr.cpp:310-366 validates rows before their template is inserted.
fn valid_template_row(
    row: ReferenceLootRow,
    inputs: &mut impl ReferenceLootInputs,
) -> Result<bool> {
    if row.group >= 128 || row.min_count_or_ref == 0 {
        return Ok(false);
    }
    let chance = row.chance.abs();
    if row.min_count_or_ref > 0 {
        Ok(inputs.item_exists(row.item)?
            && (chance != 0.0 || row.group != 0)
            && !(chance != 0.0 && chance < 0.000001)
            && row.max_count >= row.min_count_or_ref as u32)
    } else {
        Ok(!(row.chance < 0.0 || chance == 0.0 && row.group == 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Rows(Vec<ReferenceLootRow>);
    impl ReferenceLootInputs for Rows {
        fn rows(&mut self, sql: &str) -> Result<Vec<ReferenceLootRow>> {
            assert_eq!(sql, REFERENCE_LOOT_QUERY);
            Ok(self.0.clone())
        }
        fn item_exists(&mut self, id: u32) -> Result<bool> {
            Ok(id == 27)
        }
    }

    fn row(entry: u32) -> ReferenceLootRow {
        ReferenceLootRow {
            entry,
            item: 27,
            chance: 100.0,
            group: 0,
            min_count_or_ref: 1,
            max_count: 1,
            condition: 0,
        }
    }

    #[test]
    fn only_entries_with_surviving_rows_are_present() {
        let mut store = SourceReferenceLootStore::default();
        let mut input = Rows(vec![
            ReferenceLootRow {
                group: 128,
                ..row(1)
            },
            ReferenceLootRow { item: 28, ..row(2) },
            row(2),
            ReferenceLootRow {
                chance: 0.0,
                group: 0,
                ..row(3)
            },
            ReferenceLootRow {
                min_count_or_ref: -42,
                ..row(4)
            },
        ]);
        assert_eq!(store.load(&mut input).unwrap(), 2);
        assert_eq!(store.template_ids(), &BTreeSet::from([2, 4]));
        assert!(store.have_loot_for(2));
        assert!(!store.have_loot_for(1));
    }

    #[test]
    fn max_count_refuses_row_before_looking_up_condition() {
        let mut store = SourceReferenceLootStore::default();
        let needs_condition = ReferenceLootRow {
            condition: 3,
            ..row(1)
        };
        assert_eq!(
            store
                .load(&mut Rows(vec![ReferenceLootRow {
                    max_count: 256,
                    ..needs_condition
                }]))
                .unwrap(),
            0
        );
        assert_eq!(
            store.load(&mut Rows(vec![needs_condition])).unwrap_err().id,
            "execution.loot.LootMgr_162.condition_owner"
        );
    }
}
