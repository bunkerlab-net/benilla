// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Player.cpp:857-985; ItemPrototype.h:171.
// Upstream GPL-2.0-or-later, Copyright CMaNGOS contributors.
//! Bounded validated-source projection. This does not certify capture or admit a world.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharStartOutfitRow {
    pub id: u32,
    pub race_class_gender: u32,
    pub items: [i32; 12],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartingItemRow {
    pub item: ItemRow,
    pub buy_count: u32,
    pub first_spell_category: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CustomStartingItem {
    pub race: u8,
    pub class: u8,
    pub item: u32,
    pub amount: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedStarterItems {
    /// Complete selected CharStartOutfit lookup, including an empty result.
    pub outfits: Rows<CharStartOutfitRow>,
    /// Complete selected custom rows; canonical ordering is applied during compile.
    pub custom: Rows<CustomStartingItem>,
    pub items: Vec<StartingItemRow>,
}

impl CapturedStarterItems {
    /// Bind source rows into the existing initializer inputs without executing initial casts.
    /// initialize_mage/prepare_starter still prove skill eligibility and exact backpack capacity.
    pub fn compile(self, mut input: StarterInput) -> Result<StarterInput> {
        if self.items.len() > 256 {
            return Err(InitError::Invalid("starting item row limit"));
        }
        let outfits = self.outfits.require("CharStartOutfit", 0)?;
        let mut custom = self.custom.require("playercreateinfo_item", 0)?.to_vec();
        custom.sort_by_key(crate::canonical_order::custom_item);
        if outfits.len() > 2 || custom.len() > 256 {
            return Err(InitError::Invalid("starting item source limit"));
        }
        let packed =
            u32::from(input.race) | u32::from(input.class) << 8 | u32::from(input.gender) << 16;
        if outfits
            .iter()
            .any(|r| r.id == 0 || r.race_class_gender != packed)
        {
            return Err(InitError::Invalid("CharStartOutfit selection"));
        }
        let outfit = match outfits {
            [] => return Err(missing("CharStartOutfit", packed)),
            [row] => row,
            _ => return Err(InitError::Invalid("ambiguous CharStartOutfit")),
        };
        let mut ids = BTreeSet::new();
        for row in &self.items {
            if row.item.id == 0 || !ids.insert(row.item.id) {
                return Err(InitError::Invalid("duplicate/zero starting item"));
            }
        }
        let mut grants = Vec::with_capacity(12 + custom.len());
        for (order, &id) in outfit.items.iter().enumerate() {
            if id <= 0 {
                continue;
            }
            let row = self
                .items
                .iter()
                .find(|row| row.item.id == id as u32)
                .ok_or(missing("item_template", id as u32))?;
            let mut count = row.buy_count;
            // Food subclass is 5, not 0; Player::Create:887-903 and ItemPrototype.h:171.
            if row.item.class == 0 && row.item.subclass == 5 {
                match row.first_spell_category {
                    11 if row.item.stackable > 4 => count = 4,
                    59 if row.item.stackable > 2 => count = 2,
                    _ => {}
                }
            }
            if row.buy_count == 0 || row.buy_count > 10_000 {
                return Err(InitError::Invalid("outfit BuyCount"));
            }
            grants.push(ItemGrant {
                source: GrantSource::Outfit,
                source_order: order as u32,
                item: id as u32,
                count,
            });
        }
        for (order, row) in custom.iter().enumerate() {
            if row.race != input.race || row.class != input.class {
                return Err(InitError::Invalid("playercreateinfo_item selection"));
            }
            grants.push(ItemGrant {
                source: GrantSource::Custom,
                source_order: order as u32,
                item: row.item,
                count: row.amount,
            });
        }
        input.item_grants = Rows::Complete(grants);
        input.items = self.items.into_iter().map(|row| row.item).collect();
        super::items::validate_grants(&input)?;
        Ok(input)
    }
}
