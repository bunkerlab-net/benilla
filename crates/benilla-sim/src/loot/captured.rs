// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, LootMgr.cpp:125-185,463-496,2606-2668.
// Copyright CMaNGOS contributors, GPL-2.0-or-later; Rust GPL-3.0-or-later, no warranty.
//! Bind observed whole-query order and independently admitted inventory leaves to existing loot.
use super::*;
use crate::inventory;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateRow {
    pub entry: u32,
    pub row: Row,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedPosition {
    pub position: u64,
    pub entry: u32,
    pub item: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    pub source_fingerprint: [u8; 32],
    pub ordered_result_fingerprint: [u8; 32],
    pub result_row_count: u64,
    /// Complete selected rows, with order deliberately unspecified.
    pub rows: Vec<TemplateRow>,
    /// Exact selected positions from the unfiltered seven-column loader query.
    pub selected_positions: Vec<SelectedPosition>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedLoot {
    /// None is unknown coverage, not an observed empty table.
    pub body: Option<Table>,
    pub references: Option<Table>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub table: &'static str,
    pub entry: u32,
    pub item: u32,
    pub reason: String,
}
impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}[{}:{}]: {}",
            self.table, self.entry, self.item, self.reason
        )
    }
}
impl std::error::Error for Refusal {}

impl CapturedLoot {
    /// Compilation is not source authentication and cannot admit a native world.
    pub fn compile(self, inventory: &inventory::Catalog) -> Result<Catalog, Refusal> {
        if self
            .body
            .as_ref()
            .map_or(0, |t| t.rows.len())
            .saturating_add(self.references.as_ref().map_or(0, |t| t.rows.len()))
            > MAX_CAPTURE_ROWS
        {
            return Err(Refusal {
                table: "loot_closure",
                entry: 0,
                item: 0,
                reason: "retained_loot_closure_limit".into(),
            });
        }
        let body = compile_table(self.body, "creature_loot_template", inventory)?;
        let references = compile_table(self.references, "reference_loot_template", inventory)?;
        Catalog::new_with_references(body, references).map_err(|error| Refusal {
            table: "loot_closure",
            entry: 0,
            item: 0,
            reason: error.to_string(),
        })
    }
}
impl Catalog {
    /// Move compiled captures into the unchanged runtime NativeLootInput contract.
    pub fn into_captures(self) -> (Vec<Capture>, Vec<Capture>) {
        (
            self.captures.into_values().collect(),
            self.reference_captures.into_values().collect(),
        )
    }
}
fn compile_table(
    table: Option<Table>,
    name: &'static str,
    inventory: &inventory::Catalog,
) -> Result<Vec<Capture>, Refusal> {
    let refuse = |entry, item, reason: &str| Refusal {
        table: name,
        entry,
        item,
        reason: reason.into(),
    };
    let table = table.ok_or_else(|| refuse(0, 0, "whole_loader_order_unknown"))?;
    if table.rows.len() > MAX_CAPTURE_ROWS
        || table.selected_positions.len() != table.rows.len()
        || table.result_row_count > 2_000_000
        || table.source_fingerprint == [0; 32]
        || table.ordered_result_fingerprint == [0; 32]
    {
        return Err(refuse(0, 0, "bounded_complete_loader_projection"));
    }
    let mut rows = BTreeMap::new();
    for row in table.rows {
        let key = (row.entry, row.row.item);
        if row.entry == 0 || rows.insert(key, row.row).is_some() {
            return Err(refuse(key.0, key.1, "duplicate_or_zero_template_key"));
        }
    }
    let mut captures = BTreeMap::<u32, Capture>::new();
    let mut previous = None;
    for position in table.selected_positions {
        if position.position >= table.result_row_count
            || previous.is_some_and(|p| p >= position.position)
        {
            return Err(refuse(
                position.entry,
                position.item,
                "loader_position_order",
            ));
        }
        previous = Some(position.position);
        let row = rows
            .remove(&(position.entry, position.item))
            .ok_or_else(|| refuse(position.entry, position.item, "loader_key_coverage"))?;
        let capture = captures.entry(position.entry).or_insert_with(|| Capture {
            upstream_commit: UPSTREAM_COMMIT.into(),
            source_fingerprint: table.source_fingerprint,
            loot_id: position.entry,
            complete_selected_projection: true,
            loader_order: LoaderOrder {
                origin: OrderOrigin::WholeTableLoaderResult,
                complete_result_observed: true,
                ordered_result_fingerprint: table.ordered_result_fingerprint,
                result_row_count: table.result_row_count,
                selected_row_positions: Vec::new(),
            },
            rows: Vec::new(),
            items: BTreeMap::new(),
        });
        if row.min_count_or_reference > 0 {
            // AllowedForPlayer admits normal items unless StartQuest adds a status dependency.
            let item = inventory
                .corpse_carried_ready(row.item)
                .map_err(|error| Refusal {
                    table: name,
                    entry: position.entry,
                    item: row.item,
                    reason: error.to_string(),
                })?;
            capture.items.insert(
                row.item,
                ItemLookup::Present(Item {
                    entry: item.entry,
                    quality: item.quality,
                    display_id: item.display_id,
                    flags: item.flags,
                    start_quest: item.start_quest,
                    extra_flags: item.extra_flags,
                    random_property: item.random_property,
                }),
            );
        }
        capture.rows.push(row);
        capture
            .loader_order
            .selected_row_positions
            .push(position.position);
    }
    if !rows.is_empty() {
        return Err(refuse(0, 0, "loader_key_coverage"));
    }
    Ok(captures.into_values().collect())
}
