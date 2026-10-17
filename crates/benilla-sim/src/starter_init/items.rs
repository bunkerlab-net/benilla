// SPDX-License-Identifier: GPL-3.0-or-later
// Pinned Player::StoreNewItemInBestSlots/CanUseItem/_ApplyItemBonuses and Item::GetSkill.
use super::{missing, unsupported, Init, InitError, InitEvent, Result};
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemStat {
    pub kind: u32,
    pub value: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemSpell {
    pub id: u32,
    pub trigger: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemDamage {
    pub min_bits: u32,
    pub max_bits: u32,
    pub school: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemRow {
    pub id: u32,
    pub class: u32,
    pub subclass: u32,
    pub inventory_type: u32,
    pub allowable_class: u32,
    pub allowable_race: u32,
    pub required_level: u32,
    pub required_skill: u16,
    pub required_skill_rank: u32,
    pub required_spell: u32,
    pub required_honor_rank: u32,
    pub required_reputation_faction: u32,
    pub flags: u32,
    pub unique_max_count: u32,
    pub stackable: u32,
    pub bag_family: u32,
    pub container_slots: u32,
    pub item_set: u32,
    pub random_property: u32,
    pub duration: i32,
    pub scripted: bool,
    pub stats: [ItemStat; 10],
    pub armor: u32,
    pub resistances: [u32; 6],
    pub block: u32,
    pub spells: [ItemSpell; 5],
    pub damage: [ItemDamage; 5],
    pub delay_ms: u32,
    pub max_durability: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum GrantSource {
    Outfit,
    Custom,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemGrant {
    pub source: GrantSource,
    pub source_order: u32,
    pub item: u32,
    pub count: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InventoryStack {
    pub item: u32,
    pub count: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EquippedItem {
    pub item: u32,
    pub slot: u8,
    pub weapon_skill: u16,
    pub damage: [ItemDamage; 5],
    pub delay_ms: u32,
    pub durability: u32,
    pub max_durability: u32,
}
// Source enum constants, not race/class content IDs. Item::GetSkill indexes.
pub(super) fn skill(item: &ItemRow) -> u16 {
    const WEAPON: [u16; 21] = [
        44, 172, 45, 46, 54, 160, 229, 43, 55, 0, 136, 0, 0, 162, 0, 173, 176, 253, 226, 228, 356,
    ];
    const ARMOR: [u16; 10] = [0, 415, 414, 413, 293, 0, 433, 0, 0, 0];
    match item.class {
        2 => WEAPON.get(item.subclass as usize).copied().unwrap_or(0),
        4 => ARMOR.get(item.subclass as usize).copied().unwrap_or(0),
        _ => 0,
    }
}
fn slots(item: &ItemRow) -> Result<Vec<u8>> {
    let slots = match item.inventory_type {
        0 => vec![],
        1..=10 => vec![(item.inventory_type - 1) as u8],
        11 => vec![10, 11],
        12 => vec![12, 13],
        13 | 17 | 21 => vec![15],
        16 => vec![14],
        19 => vec![18],
        20 => vec![4],
        _ => {
            return Err(unsupported(
                "initial item placement",
                item.id,
                item.inventory_type,
            ))
        }
    };
    Ok(slots)
}
impl Init<'_> {
    pub(super) fn items(&mut self) -> Result<(BTreeMap<u8, EquippedItem>, Vec<InventoryStack>)> {
        if self.input.item_creation_hooks {
            return Err(unsupported("item creation hooks", 0, 0));
        }
        let grants = self
            .input
            .item_grants
            .require("complete outfit/custom grants", 0)?;
        if grants
            .windows(2)
            .any(|w| (w[0].source, w[0].source_order) >= (w[1].source, w[1].source_order))
        {
            return Err(InitError::Invalid("outfit/custom source order"));
        }
        let mut equipped = BTreeMap::new();
        let mut inventory: Vec<InventoryStack> = vec![];
        let mut counts: BTreeMap<u32, u32> = BTreeMap::new();
        for grant in grants {
            self.step()?;
            let item = self
                .input
                .items
                .iter()
                .find(|i| i.id == grant.item)
                .ok_or(missing("item_template", grant.item))?;
            if grant.count == 0
                || grant.count > 10_000
                || item.stackable == 0
                || item.stackable > 10_000
            {
                return Err(InitError::Invalid("item grant/stack count"));
            }
            if item.item_set != 0
                || item.random_property != 0
                || item.duration != 0
                || item.scripted
                || item.bag_family != 0
                || item.container_slots != 0
                || item.block != 0
            {
                return Err(unsupported(
                    "initial item set/random/duration/script/bag/block",
                    item.id,
                    0,
                ));
            }
            // Unique-equipped and special flags need their own source admission.
            if item.flags != 0 {
                return Err(unsupported("initial item flags", item.id, item.flags));
            }
            let count = counts.entry(item.id).or_default();
            *count = count
                .checked_add(grant.count)
                .ok_or(InitError::Numeric("item count"))?;
            if item.unique_max_count > 0 && *count > item.unique_max_count {
                return Err(unsupported("initial unique item count", item.id, *count));
            }
            let possible = slots(item)?;
            let race = 1u32 << (self.input.race - 1);
            let class = 1u32 << (self.input.class - 1);
            if item.required_reputation_faction != 0 || item.required_honor_rank != 0 {
                return Err(unsupported("initial item reputation/honor", item.id, 0));
            }
            let proficiency = if item.class == 2 && item.subclass == 13 {
                473
            } else {
                skill(item)
            };
            let usable = item.allowable_class & class != 0
                && item.allowable_race & race != 0
                && item.required_level <= 1
                && (item.required_skill == 0
                    || (self.skill_value(item.required_skill) != 0
                        && u32::from(self.skill_value(item.required_skill))
                            >= item.required_skill_rank))
                && (item.required_spell == 0 || self.known.contains_key(&item.required_spell))
                && (proficiency == 0 || self.skill_value(proficiency) != 0);
            let mut remaining = grant.count;
            if usable {
                for slot in possible {
                    if remaining == 0 {
                        break;
                    }
                    if equipped.contains_key(&slot) {
                        continue;
                    }
                    self.apply_item(item)?;
                    equipped.insert(
                        slot,
                        EquippedItem {
                            item: item.id,
                            slot,
                            weapon_skill: skill(item),
                            damage: item.damage,
                            delay_ms: item.delay_ms,
                            durability: item.max_durability,
                            max_durability: item.max_durability,
                        },
                    );
                    self.trace.push(InitEvent::Equip {
                        item: item.id,
                        slot,
                    });
                    remaining -= 1;
                }
            }
            if remaining > 0 {
                // Source initial storage is explicitly main backpack. No special bags
                // or random properties admitted, so merge then first empty 16 slots.
                self.trace.push(InitEvent::Store {
                    item: item.id,
                    count: remaining,
                });
                for stack in inventory.iter_mut().filter(|s| s.item == item.id) {
                    let added = remaining.min(item.stackable - stack.count);
                    stack.count += added;
                    remaining -= added;
                }
                while remaining > 0 {
                    if inventory.len() == 16 {
                        return Err(unsupported("initial backpack capacity", item.id, remaining));
                    }
                    let count = remaining.min(item.stackable);
                    inventory.push(InventoryStack {
                        item: item.id,
                        count,
                    });
                    remaining -= count;
                }
            }
        }
        Ok((equipped, inventory))
    }
    fn apply_item(&mut self, item: &ItemRow) -> Result<()> {
        // Unknown stored-item use spells are not invoked at initialization. Any
        // on-equip/proc spell on equipped items remains a critical blocker.
        for spell in item.spells {
            if spell.id != 0 && spell.trigger != 0 {
                return Err(unsupported(
                    "initial item equip/proc spell",
                    item.id,
                    spell.id,
                ));
            }
        }
        for stat in item.stats {
            if stat.value == 0 {
                continue;
            }
            match stat.kind {
                0 => self.stats.resource(true, 0, stat.value)?,
                1 => self.stats.resource(false, 0, stat.value)?,
                3 => self.stats.attribute(1, 0, stat.value)?,
                4 => self.stats.attribute(0, 0, stat.value)?,
                5 => self.stats.attribute(3, 0, stat.value)?,
                6 => self.stats.attribute(4, 0, stat.value)?,
                7 => self.stats.attribute(2, 0, stat.value)?,
                _ => return Err(unsupported("initial item stat", item.id, stat.kind)),
            }
            self.stat_trace();
        }
        if item.armor > 1_000_000 || item.resistances.iter().any(|x| *x > 1_000_000) {
            return Err(InitError::Numeric("item resistance bound"));
        }
        if item.armor != 0 {
            self.stats.resistance(0, 0, item.armor as i32)?;
            self.stat_trace();
        }
        for (i, amount) in item.resistances.iter().enumerate() {
            if *amount != 0 {
                self.stats.resistance(i + 1, 0, *amount as i32)?;
                self.stat_trace();
            }
        }
        // Preserve raw physical weapon inputs, never infer an attack damage roll.
        // Prove every line is finite and nonnegative before making it available.
        for d in item.damage {
            if d.school > 6
                || d.min_bits & 0x80000000 != 0
                || d.max_bits & 0x80000000 != 0
                || d.min_bits >= 0x7f800000
                || d.max_bits >= 0x7f800000
                || d.min_bits > d.max_bits
            {
                return Err(unsupported("initial item damage", item.id, 0));
            }
        }
        Ok(())
    }
}
