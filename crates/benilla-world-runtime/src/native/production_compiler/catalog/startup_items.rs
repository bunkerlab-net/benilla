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
        let [entry, property, class, subclass, flags, buy, inventory, classes, races, level, skill, spell, honor, faction, max_count, stack, bond, durability, duration] = [
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
        ];
        let bag_family = position(&cols, "BagFamily")?;
        let [item_set, armor, block, delay, dmg_min, dmg_max, dmg_school, dmg_second] = [
            position(&cols, "itemset")?,
            position(&cols, "armor")?,
            position(&cols, "block")?,
            position(&cols, "delay")?,
            position(&cols, "dmg_min1")?,
            position(&cols, "dmg_max1")?,
            position(&cols, "dmg_type1")?,
            position(&cols, "dmg_max2")?,
        ];
        let stat_cols = positions(
            &cols,
            [
                "stat_value1",
                "stat_value2",
                "stat_value3",
                "stat_value4",
                "stat_value5",
                "stat_value6",
                "stat_value7",
                "stat_value8",
                "stat_value9",
                "stat_value10",
            ],
        )?;
        let resist_cols = positions(
            &cols,
            [
                "holy_res",
                "fire_res",
                "nature_res",
                "frost_res",
                "shadow_res",
                "arcane_res",
            ],
        )?;
        let spell_cols = positions(
            &cols,
            [
                "spellid_1",
                "spellid_2",
                "spellid_3",
                "spellid_4",
                "spellid_5",
            ],
        )?;
        let trigger_cols = positions(
            &cols,
            [
                "spelltrigger_1",
                "spelltrigger_2",
                "spelltrigger_3",
                "spelltrigger_4",
                "spelltrigger_5",
            ],
        )?;
        let charge_cols = positions(
            &cols,
            [
                "spellcharges_1",
                "spellcharges_2",
                "spellcharges_3",
                "spellcharges_4",
                "spellcharges_5",
            ],
        )?;
        let mut rows = BTreeMap::new();
        self.scan("item_template", 0, 256, |row| {
            let mut cells = [None; 256];
            fill(&row, &mut cells)?;
            let raw = |column: usize| {
                cells
                    .get(column)
                    .copied()
                    .flatten()
                    .ok_or_else(|| fail("startup_items.item_template_cell"))
            };
            let word = |column| uint(Some(raw(column)?));
            let bits = |column| -> Result<u32> { Ok(floating(Some(raw(column)?))?.to_bits()) };
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
                    spell_charges: item_words(charge_cols, &word)?.map(|word| word as i32),
                    item_set: word(item_set)?,
                    stat_values: item_words(stat_cols, &word)?.map(|raw| raw as i32),
                    armor: word(armor)?,
                    block: word(block)?,
                    resistances: item_words(resist_cols, &word)?,
                    first_damage: [bits(dmg_min)?, bits(dmg_max)?, word(dmg_school)?],
                    second_damage_max: bits(dmg_second)?,
                    delay: word(delay)?,
                    spell_ids: item_words(spell_cols, &word)?,
                    spell_triggers: item_words(trigger_cols, &word)?,
                    bag_family: word(bag_family)?,
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

fn positions<const N: usize>(cols: &[String], names: [&str; N]) -> Result<[usize; N]> {
    let mut indexes = [0; N];
    for (index, name) in indexes.iter_mut().zip(names) {
        *index = position(cols, name)?;
    }
    Ok(indexes)
}

fn item_words<const N: usize>(
    cols: [usize; N],
    read: &impl Fn(usize) -> Result<u32>,
) -> Result<[u32; N]> {
    let mut words = [0; N];
    for (word, column) in words.iter_mut().zip(cols) {
        *word = read(column)?;
    }
    Ok(words)
}
