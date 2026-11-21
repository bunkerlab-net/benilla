// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored starting-item lifecycle fixture, not captured installation content.
use super::*;
use benilla_sim::{inventory as inv, starter_init as init};
pub fn source(id: u32) -> SpellAuthoritySource {
    let mut source = synthetic_frost_armor_fixture::aura_source(168);
    source.words.fill(0);
    let food = id == 433;
    for (i, v) in [
        (0, id),
        (2, if food { 11 } else { 59 }),
        (6, 0x1800_0100),
        (18, 1),
        (20, 60_000),
        (21, 1),
        (22, 0x40080),
        (25, 101),
        (28, 5),
        (29, if food { 0 } else { 5 }),
        (30, 85),
        (36, 1),
        (58, u32::MAX),
        (59, if food { 0 } else { u32::MAX }),
        (61, 6),
        (64, 1),
        (67, 1),
        (76, if food { 16 } else { 41 }),
        (82, 1),
        (91, if food { 84 } else { 85 }),
        (97, if food { 0x3f80_0000 } else { 0 }),
        (166, u32::MAX),
        (167, 0x3f80_0000),
        (168, 0x3f80_0000),
        (169, 0x3f80_0000),
    ] {
        source.words[i] = v;
    }
    source.duration = Some(SpellDurationSource {
        id: 85,
        base_ms: 10_000,
        per_level_ms: 0,
        maximum_ms: 10_000,
    });
    source.hooks.spell_script_names = SpellSourceRows::Complete(if food {
        vec!["spell_food_animation".into()]
    } else {
        vec![]
    });
    source
}
pub(in crate::native) fn augment(
    initial: &mut init::StarterInput,
    progression: &mut QuestProgressionInput,
) {
    let mut items = progression
        .loot
        .as_ref()
        .unwrap()
        .inventory
        .items()
        .cloned()
        .collect::<Vec<_>>();
    for (entry, id, category) in [(159, 430, 59), (2070, 433, 11)] {
        let source = source(id);
        let mut row = initial.spells[0].clone();
        row.id = id;
        row.words = source.words.try_into().unwrap();
        row.duration_ms = Some(10_000);
        initial.spells.push(row);
        let mut item = initial.items[0].clone();
        item.id = entry;
        item.class = 0;
        item.subclass = 0;
        item.inventory_type = 0;
        item.required_level = 1;
        item.required_skill = 0;
        item.required_skill_rank = 0;
        item.required_spell = 0;
        item.required_honor_rank = 0;
        item.required_reputation_faction = 0;
        item.flags = 0;
        item.stackable = 20;
        item.unique_max_count = 0;
        item.armor = 0;
        item.resistances = [0; 6];
        item.block = 0;
        item.delay_ms = 0;
        item.max_durability = 0;
        for stat in &mut item.stats {
            stat.kind = 0;
            stat.value = 0;
        }
        for damage in &mut item.damage {
            damage.min_bits = 0;
            damage.max_bits = 0;
            damage.school = 0;
        }
        for spell in &mut item.spells {
            spell.id = 0;
            spell.trigger = 0;
        }
        item.spells[0].id = id;
        initial.items.push(item);
        let init::Rows::Complete(grants) = &mut initial.item_grants else {
            panic!("authored grants")
        };
        let source_order = grants.iter().map(|g| g.source_order).max().unwrap_or(0) + 1;
        grants.push(init::ItemGrant {
            source: init::GrantSource::Custom,
            source_order,
            item: entry,
            count: 4,
        });
        let mut item = synthetic_loot_fixture::item(entry);
        item.class = 0;
        item.stackable = 20;
        item.spells[0] = inv::Spell {
            id,
            trigger: 0,
            charges: -1,
            ppm_rate_bits: 0,
            cooldown: 0,
            category,
            category_cooldown: 1000,
        };
        items.push(item);
    }
    progression.loot.as_mut().unwrap().inventory = synthetic_loot_fixture::inventory_catalog(items);
}
pub fn profile() -> Profile {
    synthetic_frost_armor_fixture::food_drink_profile()
}
pub use synthetic_frost_armor_fixture::SpatialOnly;
