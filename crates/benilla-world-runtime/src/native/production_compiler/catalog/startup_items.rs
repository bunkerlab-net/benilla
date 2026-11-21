// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded item prototype and random enchantment operands for startup.
use super::*;
use crate::native::source_execution::StartupItemPrototype;
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
    pub(crate) fn execution_item_prototypes(
        &mut self,
    ) -> Result<BTreeMap<u32, StartupItemPrototype>> {
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let cols = columns(&schema, "item_template")?;
        let [entry, property, class, subclass, flags, buy, inventory, classes, races, level, skill, spell, honor, faction, max_count, stack, bond, durability, duration, charge_1, charge_2, charge_3, charge_4, charge_5] = [
            position(&cols, "entry")?,
            position(&cols, "RandomProperty")?,
            position(&cols, "class")?,
            position(&cols, "subclass")?,
            position(&cols, "Flags")?,
            position(&cols, "BuyCount")?,
            position(&cols, "InventoryType")?,
            position(&cols, "AllowableClass")?,
            position(&cols, "AllowableRace")?,
            position(&cols, "RequiredLevel")?,
            position(&cols, "RequiredSkill")?,
            position(&cols, "requiredspell")?,
            position(&cols, "requiredhonorrank")?,
            position(&cols, "RequiredReputationFaction")?,
            position(&cols, "maxcount")?,
            position(&cols, "stackable")?,
            position(&cols, "bonding")?,
            position(&cols, "MaxDurability")?,
            position(&cols, "duration")?,
            position(&cols, "spellcharges_1")?,
            position(&cols, "spellcharges_2")?,
            position(&cols, "spellcharges_3")?,
            position(&cols, "spellcharges_4")?,
            position(&cols, "spellcharges_5")?,
        ];
        let mut rows = BTreeMap::new();
        self.scan("item_template", 0, 256, |row| {
            let mut cells = [None; 256];
            fill(&row, &mut cells)?;
            let word = |column: usize| {
                let raw = cells
                    .get(column)
                    .copied()
                    .flatten()
                    .ok_or_else(|| fail("startup_items.item_template_cell"))?;
                uint(Some(raw))
            };
            rows.insert(
                word(entry)?,
                StartupItemPrototype {
                    random_property: word(property)?,
                    class: word(class)?,
                    subclass: word(subclass)?,
                    flags: word(flags)?,
                    buy_count: word(buy)?,
                    inventory_type: word(inventory)?,
                    allowable_class: word(classes)?,
                    allowable_race: word(races)?,
                    required_level: word(level)?,
                    required_skill: word(skill)?,
                    required_spell: word(spell)?,
                    required_honor_rank: word(honor)?,
                    required_reputation_faction: word(faction)?,
                    max_count: word(max_count)? as i32,
                    stackable: word(stack)?,
                    bonding: word(bond)?,
                    max_durability: word(durability)?,
                    duration: word(duration)?,
                    spell_charges: [
                        word(charge_1)? as i32,
                        word(charge_2)? as i32,
                        word(charge_3)? as i32,
                        word(charge_4)? as i32,
                        word(charge_5)? as i32,
                    ],
                },
            );
            check(
                rows.len() <= DECODED / (32 + size_of::<StartupItemPrototype>()),
                "startup_items.view_limit",
            )
        })?;
        Ok(rows)
    }
}
