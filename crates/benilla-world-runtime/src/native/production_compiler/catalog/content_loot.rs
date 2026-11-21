// SPDX-License-Identifier: GPL-3.0-or-later
//! Pinned loot input validation, without random draws or runtime execution.
use super::*;
use crate::native::source_execution::{ConditionRow, LootRow, LootStoreKind};
use std::collections::BTreeSet;

impl VerifiedCatalog {
    pub fn execution_loot_rows(&mut self, kind: LootStoreKind) -> Result<Vec<LootRow>> {
        let table = kind.table();
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let columns = spawns::columns(&schema, table)?;
        let mut positions = [0; 7];
        for (position, name) in positions.iter_mut().zip([
            "entry",
            "item",
            "ChanceOrQuestChance",
            "groupid",
            "mincountOrRef",
            "maxcount",
            "condition_id",
        ]) {
            *position = spawns::position(&columns, name)?;
        }
        let mut rows = Vec::new();
        self.scan(table, 0, 256, |raw| {
            let mut cells = [None; 256];
            spawns::fill(&raw, &mut cells)?;
            let [entry, item, chance, group, minimum, maximum, condition] =
                positions.map(|at| cells[at]);
            rows.push(LootRow {
                entry: spawns::uint(entry)?,
                item: spawns::uint(item)?,
                chance: spawns::floating(chance)?,
                group: spawns::uint(group)? as u8,
                min_count_or_ref: spawns::uint(minimum)? as i32,
                max_count: spawns::uint(maximum)?,
                condition: spawns::uint(condition)? as u16,
            });
            check(
                rows.len() <= DECODED / std::mem::size_of::<LootRow>(),
                "loot.view_limit",
            )
        })?;
        // The restored MyISAM table is inserted in primary-key order.
        rows.sort_unstable_by_key(|row| (row.entry, row.item));
        Ok(rows)
    }

    pub fn execution_condition_rows(&mut self) -> Result<Vec<ConditionRow>> {
        let table = "conditions";
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let columns = spawns::columns(&schema, table)?;
        let mut positions = [0; 7];
        for (position, name) in positions.iter_mut().zip([
            "condition_entry",
            "type",
            "value1",
            "value2",
            "value3",
            "value4",
            "flags",
        ]) {
            *position = spawns::position(&columns, name)?;
        }
        let mut rows = Vec::new();
        self.scan(table, 0, 256, |raw| {
            let mut cells = [None; 256];
            spawns::fill(&raw, &mut cells)?;
            let [entry, kind, a, b, c, d, flags] = positions.map(|at| cells[at]);
            rows.push(ConditionRow {
                entry: spawns::uint(entry)?,
                kind: spawns::uint(kind)? as i32,
                values: [
                    spawns::uint(a)?,
                    spawns::uint(b)?,
                    spawns::uint(c)?,
                    spawns::uint(d)?,
                ],
                flags: spawns::uint(flags)? as u8,
            });
            check(
                rows.len() <= DECODED / std::mem::size_of::<ConditionRow>(),
                "conditions.view_limit",
            )
        })?;
        rows.sort_unstable_by_key(|row| row.entry);
        Ok(rows)
    }
}

pub(super) const STORES: &[&str] = &[
    "creature_loot_template",
    "disenchant_loot_template",
    "fishing_loot_template",
    "gameobject_loot_template",
    "item_loot_template",
    "mail_loot_template",
    "pickpocketing_loot_template",
    "reference_loot_template",
    "skinning_loot_template",
];
pub(super) fn status(
    row: &Value,
    items: &BTreeSet<u32>,
    conditions: &BTreeSet<u32>,
) -> Result<&'static str> {
    let n = |field: &str| row[field].as_i64().expect("loot field");
    let minimum = n("mincountorref");
    let group = n("groupid");
    let maximum = n("maxcount");
    let chance = f32::from_bits(n("chanceorquestchance") as u32);
    let condition = n("condition_id") as u32;
    Ok(if maximum > 255 {
        "skip:maxcount"
    } else if condition != 0 && !conditions.contains(&condition) {
        "skip:missing_condition"
    } else if group >= 128 {
        "skip:group"
    } else if minimum == 0 {
        "skip:mincount"
    } else if minimum > 0 {
        if !items.contains(&(n("item") as u32)) {
            "skip:missing_item"
        } else if chance == 0.0 && group == 0 {
            "skip:ungrouped_zero"
        } else if chance != 0.0 && chance.abs() < 0.000001f32 {
            "skip:low_chance"
        } else if maximum < minimum {
            "skip:count_range"
        } else if condition != 0 {
            "retained:condition_validation_pending"
        } else {
            "retained:loader_validation_pending"
        }
    } else {
        check(
            minimum != i64::from(i32::MIN),
            "content.loot_reference_negation_undefined",
        )?;
        if chance < 0.0 {
            "skip:negative_reference_chance"
        } else if chance == 0.0 && group == 0 {
            "skip:zero_reference_chance"
        } else if condition != 0 {
            "retained:condition_validation_pending"
        } else {
            "retained:loader_validation_pending"
        }
    })
}
pub(super) fn check_cycles(edges: &BTreeMap<u32, Vec<u32>>) -> Result<()> {
    let mut finished = BTreeSet::new();
    let mut steps = 0usize;
    for &root in edges.keys() {
        if finished.contains(&root) {
            continue;
        }
        let mut active = BTreeSet::new();
        let mut stack = vec![(root, false)];
        while let Some((node, exiting)) = stack.pop() {
            if exiting {
                active.remove(&node);
                finished.insert(node);
                continue;
            }
            check(!active.contains(&node), "content.loot_reference_cycle")?;
            if finished.contains(&node) {
                continue;
            }
            steps += 1;
            check(steps <= 2_000_000, "content.view_limit")?;
            active.insert(node);
            stack.push((node, true));
            if let Some(children) = edges.get(&node) {
                stack.extend(children.iter().rev().map(|&c| (c, false)));
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_siblings_are_not_reference_cycles() {
        check_cycles(&BTreeMap::from([
            (1, vec![2, 2]),
            (2, vec![3]),
            (3, vec![]),
        ]))
        .unwrap();
        assert_eq!(
            check_cycles(&BTreeMap::from([(1, vec![2]), (2, vec![1])]))
                .unwrap_err()
                .id,
            "content.loot_reference_cycle"
        );
    }
    #[test]
    fn validation_keeps_source_skip_precedence() {
        let mut row = json!({"mincountorref":1,"groupid":0,"maxcount":256,"item":99,"condition_id":99,"chanceorquestchance":0});
        let items = BTreeSet::from([1]);
        let conditions = BTreeSet::new();
        assert_eq!(status(&row, &items, &conditions).unwrap(), "skip:maxcount");
        row["maxcount"] = json!(1);
        assert_eq!(
            status(&row, &items, &conditions).unwrap(),
            "skip:missing_condition"
        );
        row["condition_id"] = json!(0);
        assert_eq!(
            status(&row, &items, &conditions).unwrap(),
            "skip:missing_item"
        );
        row["mincountorref"] = json!(-1);
        row["chanceorquestchance"] = json!(0xbf800000u32);
        assert_eq!(
            status(&row, &items, &conditions).unwrap(),
            "skip:negative_reference_chance"
        );
    }
}
