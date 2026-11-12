// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded operands for the startup RandomProperty validation only.
use super::*;
use spawns::{columns, fill, floating, position, uint};

impl VerifiedCatalog {
    pub(crate) fn execution_item_enchantments(&mut self) -> Result<Vec<(u32, u32, f32)>> {
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let cols = columns(&schema, "item_enchantment_template")?;
        let [entry, ench, chance] = [
            position(&cols, "entry")?,
            position(&cols, "ench")?,
            position(&cols, "chance")?,
        ];
        let mut rows = Vec::new();
        self.scan("item_enchantment_template", 0, 256, |row| {
            let mut cells = [None; 256];
            fill(&row, &mut cells)?;
            rows.push((
                uint(cells[entry])?,
                uint(cells[ench])?,
                floating(cells[chance])?,
            ));
            check(rows.len() <= DECODED / 12, "startup_items.view_limit")
        })?;
        Ok(rows)
    }
    pub(crate) fn execution_item_random_properties(&mut self) -> Result<BTreeMap<u32, u32>> {
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let cols = columns(&schema, "item_template")?;
        let [entry, property] = [
            position(&cols, "entry")?,
            position(&cols, "RandomProperty")?,
        ];
        let mut rows = BTreeMap::new();
        self.scan("item_template", 0, 256, |row| {
            let mut cells = [None; 256];
            fill(&row, &mut cells)?;
            let id = uint(cells[entry])?;
            // ObjectMgr.cpp:2563 starts at one, even if storage contains entry zero.
            if id != 0 {
                rows.insert(id, uint(cells[property])?);
            }
            check(rows.len() <= DECODED / 32, "startup_items.view_limit")
        })?;
        Ok(rows)
    }
}
