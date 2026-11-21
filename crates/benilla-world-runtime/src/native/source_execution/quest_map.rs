// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! World.cpp:1101,1104. ObjectMgr's loaded quest templates and relations.
use super::{identity_table::IdentityTable, *};
use std::collections::{BTreeMap, BTreeSet};

mod catalog;
pub use catalog::{QuestCatalogInputs, QUEST_TEMPLATE_QUERY};
mod validation;

const EXPLORATION: u32 = 0x02;

#[derive(Debug, Default)]
pub struct SourceQuestTemplate {
    pub id: u32,
    pub method: u32,
    pub zone_or_sort: i32,
    pub min_level: u32,
    pub max_level: u32,
    pub quest_level: u32,
    pub kind: u32,
    pub required_classes: u32,
    pub required_races: u32,
    pub required_skill: u32,
    pub required_skill_value: u32,
    pub rep_objective_faction: u32,
    pub rep_objective_value: i32,
    pub required_min_rep_faction: u32,
    pub required_min_rep_value: i32,
    pub required_max_rep_faction: u32,
    pub required_max_rep_value: i32,
    pub suggested_players: u32,
    pub limit_time: u32,
    pub quest_flags: u32,
    pub special_flags: u32,
    pub prev_quest_id: i32,
    pub next_quest_id: i32,
    pub exclusive_group: i32,
    pub next_quest_in_chain: u32,
    pub source_item: u32,
    pub source_item_count: u32,
    pub source_spell: u32,
    pub title: Box<[u8]>,
    pub details: Box<[u8]>,
    pub objectives: Box<[u8]>,
    pub offer_reward_text: Box<[u8]>,
    pub request_items_text: Box<[u8]>,
    pub end_text: Box<[u8]>,
    pub objective_text: [Box<[u8]>; 4],
    pub required_items: [u32; 4],
    pub required_item_counts: [u32; 4],
    pub required_source_items: [u32; 4],
    pub required_source_item_counts: [u32; 4],
    pub required_targets: [i32; 4],
    pub required_target_counts: [u32; 4],
    pub required_spells: [u32; 4],
    pub choice_items: [u32; 6],
    pub choice_item_counts: [u32; 6],
    pub reward_items: [u32; 4],
    pub reward_item_counts: [u32; 4],
    pub reward_factions: [u32; 5],
    pub reward_rep_values: [i32; 5],
    pub reward_or_required_money: i32,
    pub reward_money_max_level: u32,
    pub reward_spell: u32,
    pub reward_spell_cast: u32,
    pub reward_mail_template: u32,
    pub reward_mail_delay: u32,
    pub point_map: u32,
    pub point_x: f32,
    pub point_y: f32,
    pub point_opt: u32,
    pub details_emotes: [u32; 4],
    pub details_emote_delays: [u32; 4],
    pub incomplete_emote: u32,
    pub incomplete_emote_delay: u32,
    pub complete_emote: u32,
    pub complete_emote_delay: u32,
    pub offer_reward_emotes: [u32; 4],
    pub offer_reward_emote_delays: [u32; 4],
    pub start_script: u32,
    pub complete_script: u32,
    pub required_condition: u32,
    pub breadcrumb_for_quest_id: u32,
    pub reward_faction_flags: u32,
    active: bool,
    prev_quests: Vec<i32>,
    prev_chain_quests: Vec<u32>,
    dependent_breadcrumbs: Vec<u32>,
    required_items_count: usize,
    required_targets_count: usize,
    choice_items_count: usize,
    reward_items_count: usize,
    details_emotes_count: usize,
}

impl SourceQuestTemplate {
    /// QuestDef.cpp:23-140 reads the fields of the ObjectMgr.cpp:4239 query.
    pub(crate) fn from_decoded_fields(words: [u32; 131], text: [Box<[u8]>; 10]) -> Self {
        let [title, details, objectives, offer_reward_text, request_items_text, end_text, a, b, c, d] =
            text;
        fn array<const N: usize>(words: &[u32; 131], start: usize) -> [u32; N] {
            std::array::from_fn(|i| words[start + i])
        }
        Self {
            id: words[0],
            method: words[1],
            zone_or_sort: words[2] as i32,
            min_level: words[3],
            max_level: words[4],
            quest_level: words[5],
            kind: words[6],
            required_classes: words[7],
            required_races: words[8],
            required_skill: words[9],
            required_skill_value: words[10],
            rep_objective_faction: words[11],
            rep_objective_value: words[12] as i32,
            required_min_rep_faction: words[13],
            required_min_rep_value: words[14] as i32,
            required_max_rep_faction: words[15],
            required_max_rep_value: words[16] as i32,
            suggested_players: words[17],
            limit_time: words[18],
            quest_flags: u32::from(words[19] as u16),
            special_flags: u32::from(words[20] as u16),
            prev_quest_id: words[21] as i32,
            next_quest_id: words[22] as i32,
            exclusive_group: words[23] as i32,
            next_quest_in_chain: words[24],
            source_item: words[25],
            source_item_count: words[26],
            source_spell: words[27],
            title,
            details,
            objectives,
            offer_reward_text,
            request_items_text,
            end_text,
            objective_text: [a, b, c, d],
            required_items: array(&words, 38),
            required_item_counts: array(&words, 42),
            required_source_items: array(&words, 46),
            required_source_item_counts: array(&words, 50),
            required_targets: array(&words, 54).map(|word| word as i32),
            required_target_counts: array(&words, 58),
            required_spells: array(&words, 62),
            choice_items: array(&words, 66),
            choice_item_counts: array(&words, 72),
            reward_items: array(&words, 78),
            reward_item_counts: array(&words, 82),
            reward_factions: array(&words, 86),
            reward_rep_values: array(&words, 91).map(|word| word as i32),
            reward_or_required_money: words[96] as i32,
            reward_money_max_level: words[97],
            reward_spell: words[98],
            reward_spell_cast: words[99],
            reward_mail_template: words[100],
            reward_mail_delay: words[101],
            point_map: words[102],
            point_x: f32::from_bits(words[103]),
            point_y: f32::from_bits(words[104]),
            point_opt: words[105],
            details_emotes: array(&words, 106),
            details_emote_delays: array(&words, 110),
            incomplete_emote: words[114],
            incomplete_emote_delay: words[115],
            complete_emote: words[116],
            complete_emote_delay: words[117],
            offer_reward_emotes: array(&words, 118),
            offer_reward_emote_delays: array(&words, 122),
            start_script: words[126],
            complete_script: words[127],
            required_condition: words[128],
            breadcrumb_for_quest_id: words[129],
            reward_faction_flags: words[130],
            ..Self::default()
        }
    }

    // QuestDef.cpp:114-123,142-168 caches these before the post-load corrections.
    fn initialize(&mut self) {
        self.active = true;
        self.required_items_count = self.required_items.iter().filter(|&&id| id != 0).count();
        self.required_targets_count = self.required_targets.iter().filter(|&&id| id != 0).count();
        self.choice_items_count = self.choice_items.iter().filter(|&&id| id != 0).count();
        self.reward_items_count = self.reward_items.iter().filter(|&&id| id != 0).count();
        self.details_emotes_count = self
            .details_emotes
            .iter()
            .rposition(|&id| id != 0)
            .map_or(0, |i| i + 1);
    }

    pub fn is_active(&self) -> bool {
        self.active
    }
    pub fn required_items_count(&self) -> usize {
        self.required_items_count
    }
    pub fn required_targets_count(&self) -> usize {
        self.required_targets_count
    }
    pub fn choice_items_count(&self) -> usize {
        self.choice_items_count
    }
    pub fn reward_items_count(&self) -> usize {
        self.reward_items_count
    }
    pub fn details_emotes_count(&self) -> usize {
        self.details_emotes_count
    }
    pub fn prev_quests(&self) -> &[i32] {
        &self.prev_quests
    }
    pub fn prev_chain_quests(&self) -> &[u32] {
        &self.prev_chain_quests
    }
    pub fn dependent_breadcrumbs(&self) -> &[u32] {
        &self.dependent_breadcrumbs
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum QuestLookup {
    Area,
    QuestSort,
    SkillLine,
    Faction,
    Item,
    Creature,
    Talent,
    MailTemplate,
}

impl QuestLookup {
    fn name(self) -> &'static str {
        match self {
            Self::Area => "area_dbc",
            Self::QuestSort => "quest_sort_dbc",
            Self::SkillLine => "skill_line_dbc",
            Self::Faction => "faction_dbc",
            Self::Item => "item_template",
            Self::Creature => "creature_template",
            Self::Talent => "talent_dbc",
            Self::MailTemplate => "mail_template_dbc",
        }
    }
}

/// An unavailable input returns an error. A checked negative lookup returns false or None.
pub trait QuestLoaderOwner {
    fn contains(&mut self, _kind: QuestLookup, _id: u32) -> Result<bool> {
        Err(unbound())
    }
    fn gameobject_type(&mut self, _id: u32) -> Result<Option<u32>> {
        Err(unbound())
    }
    fn spell_effects(&mut self, _id: u32) -> Result<Option<[(u32, i32); 3]>> {
        Err(unbound())
    }
    fn spell_valid(&mut self, _id: u32, _notices: &mut Vec<SpellValidityNotice>) -> Result<bool> {
        Err(unbound())
    }
    fn max_skill(&mut self) -> Result<u16> {
        Err(unbound())
    }
}

fn unbound() -> Refusal {
    refuse("execution.quests.unbound_input")
}

fn at<T>(value: Result<T>, line: u32, input: &str) -> Result<T> {
    value.map_err(|error| {
        if error.id == "execution.quests.unbound_input" {
            refuse(&format!("execution.quests.ObjectMgr_{line}.{input}_owner"))
        } else {
            error
        }
    })
}

fn contains(
    owner: &mut impl QuestLoaderOwner,
    line: u32,
    kind: QuestLookup,
    id: u32,
) -> Result<bool> {
    at(owner.contains(kind, id), line, kind.name())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestRelationKind {
    GameObjectStart,
    GameObjectEnd,
    CreatureStart,
    CreatureEnd,
}
impl QuestRelationKind {
    const SOURCE_ORDER: [Self; 4] = [
        Self::GameObjectStart,
        Self::GameObjectEnd,
        Self::CreatureStart,
        Self::CreatureEnd,
    ];
    pub fn table(self) -> &'static str {
        match self {
            Self::GameObjectStart => "gameobject_questrelation",
            Self::GameObjectEnd => "gameobject_involvedrelation",
            Self::CreatureStart => "creature_questrelation",
            Self::CreatureEnd => "creature_involvedrelation",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuestLoadNotice {
    pub line: u32,
    pub quest: u32,
    /// One-based field slot, referenced ID, or zero for a row-level notice.
    pub operand: u32,
    pub relation: Option<QuestRelationKind>,
}
fn notice(out: &mut Vec<QuestLoadNotice>, line: u32, quest: u32, operand: u32) {
    out.push(QuestLoadNotice {
        line,
        quest,
        operand,
        relation: None,
    });
}

#[derive(Debug, Default)]
pub struct SourceQuestMap {
    templates: IdentityTable<SourceQuestTemplate>,
    exclusive_groups: BTreeMap<i32, Vec<u32>>,
    relations: [BTreeMap<u32, Vec<u32>>; 4],
    area_triggers: Option<BTreeMap<u32, u32>>,
    notices: Vec<QuestLoadNotice>,
    spell_validity_notices: Vec<SpellValidityNotice>,
}

impl SourceQuestMap {
    /// ObjectMgr.cpp:4231-4875. Rows must arrive in SQL cursor order.
    pub fn load_quests(
        rows: impl IntoIterator<Item = SourceQuestTemplate>,
        owner: &mut impl QuestLoaderOwner,
    ) -> Result<Self> {
        let mut map = Self::default();
        for quest in rows {
            map.insert_quest(quest)?;
        }
        map.validate_templates(owner)?;
        Ok(map)
    }

    fn insert_quest(&mut self, mut quest: SourceQuestTemplate) -> Result<()> {
        quest.initialize();
        // ObjectMgr.cpp:4293-4295 and ObjectMgr.h:486 use unordered_map::try_emplace.
        self.templates
            .insert(u64::from(quest.id), quest, usize::MAX)?;
        Ok(())
    }

    fn validate_templates(&mut self, owner: &mut impl QuestLoaderOwner) -> Result<()> {
        if self.quest_count() == 0 {
            notice(&mut self.notices, 4279, 0, 0);
            return Ok(());
        }
        let mut used_mail = BTreeSet::new();
        let mut next = self.templates.first_key();
        while let Some(id) = next {
            next = self.templates.next_key(id);
            self.templates
                .get_mut(id)
                .ok_or_else(|| refuse("execution.quests.quest_index"))?
                .validate(
                    owner,
                    &mut used_mail,
                    &mut self.notices,
                    &mut self.spell_validity_notices,
                )?;
            self.validate_links(id as u32)?;
        }
        self.load_dependent_breadcrumbs()
    }

    pub fn quest_count(&self) -> usize {
        self.templates.len()
    }
    pub fn template(&self, id: u32) -> Option<&SourceQuestTemplate> {
        self.templates.get(u64::from(id))
    }
    pub fn templates(&self) -> impl Iterator<Item = &SourceQuestTemplate> {
        self.templates.iter().map(|(_, quest)| quest)
    }
    pub fn exclusive_group(&self, group: i32) -> &[u32] {
        self.exclusive_groups.get(&group).map_or(&[], Vec::as_slice)
    }
    pub fn relations(&self, kind: QuestRelationKind, giver: u32) -> &[u32] {
        self.relations[kind as usize]
            .get(&giver)
            .map_or(&[], Vec::as_slice)
    }
    pub fn notices(&self) -> &[QuestLoadNotice] {
        &self.notices
    }
    pub fn spell_validity_notices(&self) -> &[SpellValidityNotice] {
        &self.spell_validity_notices
    }

    /// GameEventMgr.cpp:1062-1069 changes the loaded Quest's activity.
    pub fn set_active(&mut self, id: u32, active: bool) -> Result<()> {
        self.templates
            .get_mut(u64::from(id))
            .ok_or_else(|| refuse("execution.event.quest_definition_missing"))?
            .active = active;
        Ok(())
    }

    // ObjectMgr.cpp:4790-4848. Update both sides in the original map iteration.
    fn validate_links(&mut self, id: u32) -> Result<()> {
        let quest = self
            .template(id)
            .ok_or_else(|| refuse("execution.quests.quest_index"))?;
        let (chain, prev, next, group, limit, breadcrumb) = (
            quest.next_quest_in_chain,
            quest.prev_quest_id,
            quest.next_quest_id,
            quest.exclusive_group,
            quest.limit_time,
            quest.breadcrumb_for_quest_id,
        );
        if chain != 0 {
            if let Some(target) = self.templates.get_mut(u64::from(chain)) {
                target.prev_chain_quests.push(id);
            } else {
                notice(&mut self.notices, 4795, id, chain);
                self.loaded_mut(id)?.next_quest_in_chain = 0;
            }
        }
        if prev != 0 {
            let key = prev
                .checked_abs()
                .ok_or_else(|| refuse("execution.quests.ObjectMgr_4806.prev_quest_abs_undefined"))?
                as u32;
            match self.template(key) {
                None => notice(&mut self.notices, 4808, id, key),
                Some(target) if target.breadcrumb_for_quest_id != 0 => {
                    notice(&mut self.notices, 4810, id, key)
                }
                Some(_) => self.loaded_mut(id)?.prev_quests.push(prev),
            }
        }
        if next != 0 {
            let key = next
                .checked_abs()
                .ok_or_else(|| refuse("execution.quests.ObjectMgr_4817.next_quest_abs_undefined"))?
                as u32;
            if let Some(target) = self.templates.get_mut(u64::from(key)) {
                let signed = if next < 0 {
                    (id as i32).checked_neg().ok_or_else(|| {
                        refuse("execution.quests.ObjectMgr_4824.quest_id_negation_undefined")
                    })?
                } else {
                    id as i32
                };
                target.prev_quests.push(signed);
            } else {
                notice(&mut self.notices, 4820, id, key);
            }
        }
        if group != 0 {
            self.exclusive_groups.entry(group).or_default().push(id);
        }
        if limit != 0 {
            self.loaded_mut(id)?.special_flags |= 0x40;
        }
        if breadcrumb != 0 {
            if self.template(breadcrumb).is_none() {
                notice(&mut self.notices, 4840, id, breadcrumb);
                self.loaded_mut(id)?.breadcrumb_for_quest_id = 0;
            } else if next != 0 {
                notice(&mut self.notices, 4846, id, breadcrumb);
            }
        }
        Ok(())
    }

    fn loaded_mut(&mut self, id: u32) -> Result<&mut SourceQuestTemplate> {
        self.templates
            .get_mut(u64::from(id))
            .ok_or_else(|| refuse("execution.quests.quest_index"))
    }

    // ObjectMgr.cpp:4851-4875 walks each breadcrumb path and cuts detected loops.
    fn load_dependent_breadcrumbs(&mut self) -> Result<()> {
        let mut next = self.templates.first_key();
        let mut seen = BTreeSet::new();
        while let Some(id) = next {
            next = self.templates.next_key(id);
            let origin = id as u32;
            let mut current = origin;
            let mut target = self.loaded_mut(current)?.breadcrumb_for_quest_id;
            while target != 0 {
                if !seen.insert(current) {
                    notice(&mut self.notices, 4864, origin, target);
                    self.loaded_mut(current)?.breadcrumb_for_quest_id = 0;
                    break;
                }
                let quest = self.loaded_mut(target)?;
                quest.dependent_breadcrumbs.push(origin);
                current = target;
                target = quest.breadcrumb_for_quest_id;
            }
            seen.clear();
        }
        Ok(())
    }

    /// ObjectMgr.h:676-681 issues the four SELECTs in this order.
    pub fn load_quest_relations(
        &mut self,
        mut rows: impl FnMut(QuestRelationKind) -> Result<Vec<[u32; 2]>>,
        owner: &mut impl QuestLoaderOwner,
    ) -> Result<()> {
        self.notices.retain(|notice| notice.relation.is_none());
        for kind in QuestRelationKind::SOURCE_ORDER {
            let relations = &mut self.relations[kind as usize];
            relations.clear();
            let rows = rows(kind)?;
            load_relations_helper(kind, rows, &self.templates, relations, &mut self.notices);
            validate_relation_givers(kind, relations, owner, &mut self.notices)?;
        }
        Ok(())
    }

    /// ObjectMgr.cpp:6244-6295. Rows arrive in SQL cursor order.
    pub fn load_area_triggers(
        &mut self,
        rows: impl IntoIterator<Item = [u32; 2]>,
        mut contains_trigger: impl FnMut(u32) -> Result<bool>,
    ) -> Result<()> {
        self.area_triggers = None;
        self.notices
            .retain(|notice| !matches!(notice.line, 6274 | 6281 | 6287));
        let mut triggers = BTreeMap::new();
        for [trigger, quest] in rows {
            if !at(contains_trigger(trigger), 6271, "area_trigger_dbc")? {
                notice(&mut self.notices, 6274, quest, trigger);
                continue;
            }
            let Some(template) = self.templates.get_mut(u64::from(quest)) else {
                notice(&mut self.notices, 6281, quest, trigger);
                continue;
            };
            if template.special_flags & EXPLORATION == 0 {
                notice(&mut self.notices, 6287, quest, trigger);
                template.special_flags |= EXPLORATION; // ObjectMgr.cpp:6290.
            }
            triggers.insert(trigger, quest); // ObjectMgr.cpp:6295.
        }
        self.area_triggers = Some(triggers);
        Ok(())
    }

    pub fn area_trigger_quest(&self, trigger: u32) -> Result<Option<u32>> {
        Ok(self.area_triggers()?.get(&trigger).copied())
    }

    fn area_triggers(&self) -> Result<&BTreeMap<u32, u32>> {
        self.area_triggers
            .as_ref()
            .ok_or_else(|| refuse("execution.quests.World_1146.quest_area_triggers_owner"))
    }
}

impl DbScriptLoaderOwner for SourceQuestMap {
    fn contains(&mut self, kind: DbScriptLookup, id: u32) -> Result<bool> {
        if kind == DbScriptLookup::Quest {
            return Ok(self.template(id).is_some());
        }
        Err(refuse(&format!(
            "execution.db_scripts.{}_owner",
            kind.owner()
        )))
    }

    fn quest_exploration(&mut self, id: u32) -> Result<Option<bool>> {
        self.area_triggers()?;
        Ok(self
            .template(id)
            .map(|quest| quest.special_flags & EXPLORATION != 0))
    }

    fn set_quest_exploration(&mut self, id: u32) -> Result<()> {
        self.area_triggers()?;
        let quest = self
            .templates
            .get_mut(u64::from(id))
            .ok_or_else(|| refuse("execution.quests.ScriptMgr_330.quest_template_missing"))?;
        quest.special_flags |= EXPLORATION;
        Ok(())
    }
}

// ObjectMgr.cpp:7940-7982. Giver validity is checked after insertion.
fn load_relations_helper(
    kind: QuestRelationKind,
    rows: Vec<[u32; 2]>,
    quests: &IdentityTable<SourceQuestTemplate>,
    relations: &mut BTreeMap<u32, Vec<u32>>,
    out: &mut Vec<QuestLoadNotice>,
) {
    if rows.is_empty() {
        out.push(QuestLoadNotice {
            line: 7955,
            quest: 0,
            operand: 0,
            relation: Some(kind),
        });
    }
    for [giver, quest] in rows {
        if quests.get(u64::from(quest)).is_none() {
            out.push(QuestLoadNotice {
                line: 7971,
                quest,
                operand: giver,
                relation: Some(kind),
            });
        } else {
            relations.entry(giver).or_default().push(quest);
        }
    }
}

// ObjectMgr.cpp:7985-8034. Missing givers and bad types only log.
fn validate_relation_givers(
    kind: QuestRelationKind,
    relations: &BTreeMap<u32, Vec<u32>>,
    owner: &mut impl QuestLoaderOwner,
    out: &mut Vec<QuestLoadNotice>,
) -> Result<()> {
    use QuestRelationKind::*;
    for (&giver, quests) in relations {
        for &quest in quests {
            let line = match kind {
                GameObjectStart | GameObjectEnd => {
                    let (lookup, missing, bad_type) = if kind == GameObjectStart {
                        (7991, 7993, 7995)
                    } else {
                        (8005, 8007, 8009)
                    };
                    match at(owner.gameobject_type(giver), lookup, "gameobject_template")? {
                        None => missing,
                        Some(2) => continue, // SharedDefines.h:557.
                        Some(_) => bad_type,
                    }
                }
                CreatureStart | CreatureEnd => {
                    let (lookup, missing) = if kind == CreatureStart {
                        (8019, 8021)
                    } else {
                        (8031, 8033)
                    };
                    if contains(owner, lookup, QuestLookup::Creature, giver)? {
                        continue;
                    }
                    missing
                }
            };
            out.push(QuestLoadNotice {
                line,
                quest,
                operand: giver,
                relation: Some(kind),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "quest_map_tests.rs"]
mod tests;
