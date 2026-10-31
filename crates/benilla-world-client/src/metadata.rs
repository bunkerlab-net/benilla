// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{ClientCatalog, Error};
use benilla_protocol::messages as p;
use benilla_world_runtime::World;
use std::collections::BTreeMap;

/// Full externally resolved quest presentation. No native values are synthesized here.
#[derive(Clone, Debug)]
pub struct QuestMetadata {
    pub template: p::QuestTemplate,
    pub request_text: String,
    pub offer_text: String,
}
fn valid_text(value: &str) -> bool {
    value.len() <= 16384
        && value
            .chars()
            .all(|c| !c.is_control() || matches!(c, '\n' | '\t'))
}
pub(crate) struct Metadata {
    pub items: BTreeMap<u32, p::ItemInfo>,
    pub quests: BTreeMap<u32, QuestMetadata>,
    pub greetings: BTreeMap<u32, String>,
}
impl Metadata {
    pub fn load(world: &World, catalog: &impl ClientCatalog) -> Result<Self, Error> {
        if world.data().export().items.len() > 4096 {
            return Err(Error::Unrepresentable);
        }
        let mut items = BTreeMap::new();
        for item in &world.data().export().items {
            let value = catalog
                .item_metadata(item.entry)
                .ok_or(Error::MissingItemMetadata(item.entry))?;
            // Only ordinary, non-equippable, non-usable, unrestricted plain stacks have
            // authoritative support. Preserve supplied cosmetic/vendor metadata, not defaults.
            if value.name != item.name
                || value.display_info_id != item.display_id
                || value.stackable != u32::from(item.max_stack)
                || value.quality > 6
                || value.class > 15
                || !valid_text(&value.description)
                || value.flags != 0
                || value.inventory_type != 0
                || value.allowable_class != -1
                || value.allowable_race != -1
                || value.required_level != 0
                || value.required_skill != 0
                || value.required_skill_rank != 0
                || value.required_spell != 0
                || value.required_honor_rank != 0
                || value.required_city_rank != 0
                || value.required_rep_faction != 0
                || value.required_rep_rank != 0
                || value.max_count != 0
                || value.container_slots != 0
                || !value.stats.is_empty()
                || !value.damages.is_empty()
                || value.dmg_min != 0.0
                || value.dmg_max != 0.0
                || value.dmg_type != 0
                || value.armor != 0
                || value.resistances != [0; 6]
                || value.delay_ms != 0
                || value.ammo_type != 0
                || value.ranged_mod_range != 0.0
                || !value.spells.is_empty()
                || value.spell_charges_0 != 0
                || value.use_spell.is_some()
                || value.bonding != 0
                || value.page_text != 0
                || value.language_id != 0
                || value.page_material != 0
                || value.start_quest != 0
                || value.lock_id != 0
                || value.random_property != 0
                || value.block != 0
                || value.item_set != 0
                || value.max_durability != 0
                || value.area != 0
                || value.map != 0
                || value.bag_family != 0
            {
                return Err(Error::InvalidItemMetadata(item.entry));
            }
            items.insert(item.entry, value);
        }
        let mut quests = BTreeMap::new();
        for q in &world.data().export().quests {
            let value = catalog
                .quest_metadata(q.entry)
                .ok_or(Error::MissingQuestMetadata(q.entry))?;
            let t = &value.template;
            let mut rewards = [(0, 0); 4];
            for (i, item) in q.reward_items.iter().enumerate() {
                rewards[i] = (item.item_entry, u32::from(item.count));
            }
            if t.quest_id != q.entry
                || t.title != q.title
                || t.details != q.details
                || t.method != 2
                || !(1..=60).contains(&t.level)
                || t.quest_type != 0
                || t.rep_objective_faction != 0
                || t.rep_objective_value != 0
                || t.next_quest_in_chain != 0
                || t.money != q.reward_money as i32
                || t.money_max_level != 0
                || t.reward_spell != 0
                || t.src_item_id != 0
                || t.flags != 0
                || t.rewards != rewards
                || t.choices != [(0, 0); 6]
                || !t.point_x.is_finite()
                || !t.point_y.is_finite()
                || !catalog.contains_map(t.point_map_id)
                || !valid_text(&t.objectives_text)
                || !valid_text(&t.end_text)
                || !valid_text(&value.request_text)
                || !valid_text(&value.offer_text)
            {
                return Err(Error::InvalidQuestMetadata(q.entry));
            }
            for (i, slot) in t.objectives.iter().enumerate() {
                let expected = q
                    .objectives
                    .get(i)
                    .map_or((0, 0), |o| (o.creature_entry, u32::from(o.count)));
                if (slot.creature_or_go, slot.required_count) != expected
                    || slot.item_id != 0
                    || slot.item_count != 0
                    || !valid_text(&slot.text)
                {
                    return Err(Error::InvalidQuestMetadata(q.entry));
                }
            }
            quests.insert(q.entry, value);
        }
        let mut greetings = BTreeMap::new();
        for relation in &world.data().export().quest_relations {
            if greetings.contains_key(&relation.creature_entry) {
                continue;
            }
            let text = catalog
                .quest_greeting(relation.creature_entry)
                .ok_or(Error::MissingQuestGreeting(relation.creature_entry))?;
            if !valid_text(&text) {
                return Err(Error::Unrepresentable);
            }
            greetings.insert(relation.creature_entry, text);
        }
        Ok(Self {
            items,
            quests,
            greetings,
        })
    }
}
