// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Entities/ItemHandler.cpp:284-411.
//! Base-locale item-query inputs from a checked raw draft. No world admission.
use crate::{ClientCatalog, Error, NativeItemSpellQuery};
use benilla_protocol::messages as protocol;
use benilla_world_runtime::native::CheckedLootDraft;
use std::collections::BTreeMap;

pub fn compile(
    draft: &CheckedLootDraft,
    catalog: &impl ClientCatalog,
) -> Result<BTreeMap<u32, protocol::ItemInfo>, Error> {
    draft
        .client_inputs()
        .iter()
        .map(|(&entry, row)| {
            let item = draft
                .items()
                .item(entry)
                .map_err(|_| Error::InvalidItemMetadata(entry))?;
            let number = |field: &str| -> Result<u32, Error> {
                row[field]
                    .as_i64()
                    .filter(|n| (i64::from(i32::MIN)..=i64::from(u32::MAX)).contains(n))
                    .map(|n| n as u32)
                    .ok_or(Error::InvalidItemMetadata(entry))
            };
            let text = |field: &str| -> Result<String, Error> {
                let text = row[field]
                    .as_str()
                    .ok_or(Error::InvalidItemMetadata(entry))?;
                if !super::loot::valid_text(text) {
                    return Err(Error::InvalidItemMetadata(entry));
                }
                Ok(text.into())
            };
            let decimal = |field: &str| -> Result<f32, Error> {
                row[field]
                    .as_i64()
                    .map(|n| n as f32)
                    .or_else(|| {
                        row[field]["decimal"]
                            .as_str()
                            .and_then(|s| s.parse::<f32>().ok())
                    })
                    .filter(|v| v.is_finite())
                    .ok_or(Error::InvalidItemMetadata(entry))
            };
            if !catalog.contains_item_display(item.display_id) {
                return Err(Error::UnboundItemDisplay(entry));
            }
            let mut spells = Vec::new();
            for (index, spell) in item.spells.iter().enumerate().filter(|(_, s)| s.id != 0) {
                let query = catalog
                    .native_item_spell_query(spell.id)
                    .ok_or(Error::InvalidItemMetadata(entry))?;
                let NativeItemSpellQuery::Present {
                    recovery_time,
                    category,
                    category_recovery_time,
                } = query
                else {
                    continue;
                };
                let (cooldown_ms, category, category_cooldown_ms) =
                    if spell.cooldown >= 0 || spell.category_cooldown >= 0 {
                        (spell.cooldown, spell.category, spell.category_cooldown)
                    } else {
                        (
                            recovery_time as i32,
                            category,
                            category_recovery_time as i32,
                        )
                    };
                spells.push(protocol::ItemSpellEntry {
                    index: index as u8,
                    spell_id: spell.id,
                    trigger: spell.trigger,
                    charges: spell.charges,
                    cooldown_ms,
                    category,
                    category_cooldown_ms,
                });
            }
            let use_spell =
                spells
                    .iter()
                    .find(|s| s.trigger == 0)
                    .map(|s| protocol::ItemUseSpell {
                        spell_id: s.spell_id,
                        cooldown_ms: s.cooldown_ms,
                        category: s.category,
                        category_cooldown_ms: s.category_cooldown_ms,
                    });
            let mut stats = Vec::new();
            for slot in 1..=10 {
                let stat = (
                    number(&format!("stat_type{slot}"))?,
                    number(&format!("stat_value{slot}"))? as i32,
                );
                if stat != (0, 0) {
                    stats.push(stat);
                }
            }
            let mut damages = Vec::new();
            for slot in 1..=5 {
                let damage = protocol::ItemDamage {
                    min: decimal(&format!("dmg_min{slot}"))?,
                    max: decimal(&format!("dmg_max{slot}"))?,
                    school: number(&format!("dmg_type{slot}"))?,
                };
                if damage.max > 0.0 {
                    damages.push(damage);
                }
            }
            let info = protocol::ItemInfo {
                class: item.class,
                subclass: if item.class == 0 {
                    0
                } else {
                    number("subclass")?
                },
                name: text("name")?,
                display_info_id: item.display_id,
                quality: item.quality,
                flags: item.flags,
                buy_price: number("BuyPrice")?,
                sell_price: number("SellPrice")?,
                inventory_type: item.inventory_type,
                allowable_class: number("AllowableClass")? as i32,
                allowable_race: number("AllowableRace")? as i32,
                item_level: number("ItemLevel")?,
                required_level: number("RequiredLevel")?,
                required_skill: number("RequiredSkill")?,
                required_skill_rank: number("RequiredSkillRank")?,
                required_spell: number("requiredspell")?,
                required_honor_rank: number("requiredhonorrank")?,
                required_city_rank: number("RequiredCityRank")?,
                required_rep_faction: number("RequiredReputationFaction")?,
                required_rep_rank: if number("RequiredReputationFaction")? == 0 {
                    0
                } else {
                    number("RequiredReputationRank")?
                },
                max_count: item.maximum_count as u32,
                stackable: item.stackable as u32,
                container_slots: item.container_slots,
                stats,
                damages,
                dmg_min: decimal("dmg_min1")?,
                dmg_max: decimal("dmg_max1")?,
                dmg_type: number("dmg_type1")?,
                armor: number("armor")?,
                resistances: [
                    number("holy_res")? as i32,
                    number("fire_res")? as i32,
                    number("nature_res")? as i32,
                    number("frost_res")? as i32,
                    number("shadow_res")? as i32,
                    number("arcane_res")? as i32,
                ],
                delay_ms: number("delay")?,
                ammo_type: number("ammo_type")?,
                ranged_mod_range: decimal("RangedModRange")?,
                spell_charges_0: spells
                    .iter()
                    .find(|s| s.index == 0)
                    .map_or(0, |s| s.charges),
                spells,
                use_spell,
                bonding: item.bonding,
                description: text("description")?,
                page_text: number("PageText")?,
                language_id: number("LanguageID")?,
                page_material: number("PageMaterial")?,
                start_quest: item.start_quest,
                lock_id: item.lock_id,
                material: number("Material")?,
                sheath: number("sheath")?,
                random_property: item.random_property,
                block: number("block")?,
                item_set: item.item_set,
                max_durability: item.maximum_durability,
                area: item.area,
                map: item.map,
                bag_family: item.bag_family,
            };
            if info.name.is_empty() {
                return Err(Error::InvalidItemMetadata(entry));
            }
            Ok((entry, info))
        })
        .collect()
}
