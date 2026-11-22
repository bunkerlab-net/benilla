// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, ObjectMgr.cpp:4882-4990,5775-5909; Player.cpp:11963-11972,12087-12181; GossipDef.cpp:300-585.
// Upstream GPL-2.0-or-later, Copyright CMaNGOS contributors; Rust GPL-3.0-or-later, without warranty.
//! Source-backed quest metadata only, not a partial ClientCatalog or admission token.
use crate::{NativeQuestGreeting, NativeQuestPresentation, QuestMetadata};
use benilla_protocol::messages as p;
use benilla_sim::quest_lifecycle as q;
use benilla_world_runtime::native::{self as n, EventAiLocale};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestText {
    pub title: String,
    pub details: String,
    pub objectives: String,
    pub end: String,
    pub request: String,
    pub offer: String,
    pub objective: [String; 4],
}
impl QuestText {
    fn strings(&self) -> impl Iterator<Item = &String> {
        [
            &self.title,
            &self.details,
            &self.objectives,
            &self.end,
            &self.request,
            &self.offer,
        ]
        .into_iter()
        .chain(&self.objective)
    }
    fn localized(&self, locale: Option<&Self>) -> Self {
        let choose = |base: &String, text: Option<&String>| {
            text.filter(|s| !s.is_empty()).unwrap_or(base).clone()
        };
        Self {
            title: choose(&self.title, locale.map(|l| &l.title)),
            details: choose(&self.details, locale.map(|l| &l.details)),
            objectives: choose(&self.objectives, locale.map(|l| &l.objectives)),
            end: choose(&self.end, locale.map(|l| &l.end)),
            request: choose(&self.request, locale.map(|l| &l.request)),
            offer: choose(&self.offer, locale.map(|l| &l.offer)),
            objective: std::array::from_fn(|i| {
                choose(&self.objective[i], locale.map(|l| &l.objective[i]))
            }),
        }
    }
}
#[derive(Clone, Debug)]
pub struct CapturedQuestText {
    pub quest: u32,
    pub zone_or_sort: i32,
    pub point_map: u32,
    pub point_x_bits: u32,
    pub point_y_bits: u32,
    pub point_option: u32,
    pub text: QuestText,
    pub presentation: NativeQuestPresentation,
}
#[derive(Clone, Debug)]
pub struct CapturedQuestLocale {
    pub quest: u32,
    pub locale: EventAiLocale,
    pub text: QuestText,
}
#[derive(Clone, Debug)]
pub struct CapturedGreeting {
    pub entry: u32,
    pub kind: u32,
    pub text: String,
    pub emote: u32,
    pub delay: u32,
}
#[derive(Clone, Debug)]
pub struct CapturedGreetingLocale {
    pub entry: u32,
    pub kind: u32,
    pub locale: EventAiLocale,
    pub text: String,
}
#[derive(Clone, Debug)]
pub struct CapturedNpcTextOption {
    pub probability_bits: u32,
    pub text0: String,
    pub text1: String,
    pub language: u32,
    pub emotes: [(u32, u32); 3],
    pub broadcast: u32,
}
#[derive(Clone, Debug)]
pub struct CapturedNpcText {
    pub id: u32,
    pub options: [CapturedNpcTextOption; 8],
}
#[derive(Clone, Debug)]
pub struct CapturedNpcTextLocale {
    pub id: u32,
    pub locale: EventAiLocale,
    pub text: [(String, String); 8],
}
#[derive(Clone, Debug)]
pub struct CapturedGreetingSpawn {
    pub spawn: u64,
    pub entry: u32,
    pub generation: u32,
    pub gender: u8,
    /// Some(None) is a captured missing GUID lookup; None is unknown.
    pub npc_gossip: Option<Option<u32>>,
}
#[derive(Clone, Debug)]
pub struct CapturedQuestPresentation {
    pub source: [u8; 32],
    pub quests: Option<Vec<CapturedQuestText>>,
    pub quest_locales: Option<Vec<CapturedQuestLocale>>,
    pub greetings: Option<Vec<CapturedGreeting>>,
    pub greeting_locales: Option<Vec<CapturedGreetingLocale>>,
    pub spawns: Option<Vec<CapturedGreetingSpawn>>,
    /// Complete selected GetGossipText lookups, including known missing keys.
    pub npc_text_keys: Vec<u32>,
    pub npc_text: Option<Vec<CapturedNpcText>>,
    pub npc_text_locales: Option<Vec<CapturedNpcTextLocale>>,
    pub broadcasts: Option<Vec<n::EventAiBroadcastSource>>,
    pub broadcast_locales: Option<Vec<n::EventAiBroadcastLocale>>,
    /// Complete Emotes.dbc store, independently extracted with version 4.
    pub emotes: Option<Vec<u32>>,
    pub maps: Option<Vec<u32>>,
    pub items: Option<Vec<u32>>,
    /// Explicit session DB-locale mapping, not the process locale.
    pub locale_mapping: Vec<(i32, EventAiLocale)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestPresentationError {
    pub key: u32,
    pub field: &'static str,
    pub reason: &'static str,
}
type Result<T, E = QuestPresentationError> = std::result::Result<T, E>;
fn error(key: u32, field: &'static str, reason: &'static str) -> QuestPresentationError {
    QuestPresentationError { key, field, reason }
}
fn rows<'a, T>(value: &'a Option<Vec<T>>, field: &'static str, limit: usize) -> Result<&'a [T]> {
    let value = value
        .as_deref()
        .ok_or(error(0, field, "unknown_coverage"))?;
    if value.len() > limit {
        return Err(error(0, field, "row_limit"));
    }
    Ok(value)
}
fn text(key: u32, value: &str, bytes: &mut usize) -> Result<()> {
    *bytes += value.len();
    if value.len() > 16384
        || *bytes > 8 * 1024 * 1024
        || value
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
    {
        return Err(error(key, "text", "invalid_or_over_budget"));
    }
    Ok(())
}
#[derive(Clone, Debug)]
pub struct CompiledQuestPresentation {
    quests: BTreeMap<u32, QuestMetadata>,
    presentation: BTreeMap<u32, NativeQuestPresentation>,
    greetings: BTreeMap<u32, NativeQuestGreeting>,
    npc_text: BTreeMap<u32, Vec<p::NpcTextBlock>>,
    script_text: BTreeMap<u32, String>,
    digest: [u8; 32],
}
impl CapturedQuestPresentation {
    pub fn compile(
        &self,
        gameplay: &q::CompiledOrdinaryQuests,
        database_locale: i32,
    ) -> Result<CompiledQuestPresentation> {
        if self.source != gameplay.source() {
            return Err(error(0, "source", "binding_mismatch"));
        }
        if self.locale_mapping.len() > 8
            || !self.locale_mapping.contains(&(-1, EventAiLocale::EnUs))
            || self
                .locale_mapping
                .iter()
                .map(|p| p.0)
                .collect::<BTreeSet<_>>()
                .len()
                != self.locale_mapping.len()
            || self
                .locale_mapping
                .iter()
                .map(|p| p.1)
                .collect::<BTreeSet<_>>()
                .len()
                != self.locale_mapping.len()
            || self.locale_mapping.iter().any(|(idx, locale)| {
                *idx < -1 || *idx > 6 || (*idx == -1) != (*locale == EventAiLocale::EnUs)
            })
        {
            return Err(error(0, "locale_mapping", "invalid_mapping"));
        }
        let locale = self
            .locale_mapping
            .iter()
            .find(|p| p.0 == database_locale)
            .map(|p| p.1)
            .ok_or(error(0, "locale", "unknown_session_locale"))?;
        let quests = rows(&self.quests, "quest_template", 1024)?;
        let locales = rows(&self.quest_locales, "locales_quest", 8192)?;
        let greetings = rows(&self.greetings, "questgiver_greeting", 4096)?;
        let greeting_locales = rows(&self.greeting_locales, "locales_questgiver_greeting", 32768)?;
        let spawns = rows(&self.spawns, "creature", 4096)?;
        let npc_text = rows(&self.npc_text, "npc_text", 4096)?;
        let npc_locales = rows(&self.npc_text_locales, "locales_npc_text", 32768)?;
        let broadcasts = rows(&self.broadcasts, "broadcast_text", 4096)?;
        let broadcast_locales = rows(&self.broadcast_locales, "broadcast_text_locale", 32768)?;
        let emotes = rows(&self.emotes, "Emotes", 100000)?;
        let maps = rows(&self.maps, "Map", 4096)?;
        let items = rows(&self.items, "item_template", 4096)?;
        if emotes.iter().collect::<BTreeSet<_>>().len() != emotes.len() {
            return Err(error(0, "Emotes", "duplicate_key"));
        }
        for (keys, field) in [(maps, "Map"), (items, "item_template")] {
            if keys.iter().collect::<BTreeSet<_>>().len() != keys.len() {
                return Err(error(0, field, "duplicate_key"));
            }
        }
        let mut bytes = 0;
        let mut by_locale = BTreeMap::new();
        let ids: BTreeSet<_> = gameplay.catalog().ids().collect();
        for row in locales {
            for value in row.text.strings() {
                text(row.quest, value, &mut bytes)?;
            }
            if row.locale == EventAiLocale::EnUs
                || !ids.contains(&row.quest)
                || by_locale
                    .insert((row.quest, row.locale), &row.text)
                    .is_some()
            {
                return Err(error(
                    row.quest,
                    "locales_quest",
                    "duplicate_or_unselected_key",
                ));
            }
        }
        let mut out = CompiledQuestPresentation {
            quests: BTreeMap::new(),
            presentation: BTreeMap::new(),
            greetings: BTreeMap::new(),
            npc_text: BTreeMap::new(),
            script_text: BTreeMap::new(),
            digest: [0; 32],
        };
        for raw in quests {
            let row = gameplay
                .catalog()
                .row(raw.quest)
                .map_err(|_| error(raw.quest, "quest", "unselected_key"))?;
            for value in raw.text.strings() {
                text(raw.quest, value, &mut bytes)?;
            }
            let present = &raw.presentation;
            if present.complete_emote_delay != 0 || present.incomplete_emote_delay != 0 {
                return Err(error(raw.quest, "emotes", "unsupported_packet_shape"));
            }
            for &(_, emote) in present.details_emotes.iter().chain(&present.offer_emotes) {
                if emote != 0 && !emotes.contains(&emote) {
                    return Err(error(raw.quest, "Emotes", "missing_lookup"));
                }
            }
            if !maps.contains(&raw.point_map)
                || !f32::from_bits(raw.point_x_bits).is_finite()
                || !f32::from_bits(raw.point_y_bits).is_finite()
            {
                return Err(error(raw.quest, "point", "invalid_map_or_coordinates"));
            }
            for id in row
                .required_item
                .into_iter()
                .chain(row.excluded.reward_item)
                .chain(row.excluded.reward_choice_item)
                .chain([row.excluded.source_item])
                .filter(|id| *id != 0)
            {
                if !items.contains(&id) {
                    return Err(error(id, "item_template", "missing_lookup"));
                }
            }
            let strings = raw
                .text
                .localized(by_locale.get(&(row.id, locale)).copied());
            let metadata = QuestMetadata {
                template: p::QuestTemplate {
                    quest_id: row.id,
                    method: row.method,
                    level: row.reward.xp.quest_level,
                    zone_or_sort: raw.zone_or_sort,
                    quest_type: row.excluded.quest_type,
                    rep_objective_faction: row.excluded.rep_objective_faction,
                    rep_objective_value: row.excluded.rep_objective_value as u32,
                    opposite_rep_faction: 0,
                    opposite_rep_value: 0,
                    next_quest_in_chain: row.next_in_chain,
                    money: row.reward.reward_or_required_money,
                    money_max_level: row.reward.xp.reward_money_max_level,
                    reward_spell: row.excluded.reward_spell,
                    src_item_id: row.excluded.source_item,
                    flags: u32::from(row.reward.quest_flags),
                    rewards: std::array::from_fn(|i| {
                        (
                            row.excluded.reward_item[i],
                            row.excluded.reward_item_count[i],
                        )
                    }),
                    choices: std::array::from_fn(|i| {
                        (
                            row.excluded.reward_choice_item[i],
                            row.excluded.reward_choice_count[i],
                        )
                    }),
                    point_map_id: raw.point_map,
                    point_x: f32::from_bits(raw.point_x_bits),
                    point_y: f32::from_bits(raw.point_y_bits),
                    point_opt: raw.point_option,
                    title: strings.title,
                    details: strings.details,
                    objectives_text: strings.objectives,
                    end_text: strings.end,
                    objectives: std::array::from_fn(|i| p::QuestObjective {
                        creature_or_go: row.required_creature[i] as u32,
                        required_count: row.required_creature_count[i],
                        item_id: row.required_item[i],
                        item_count: row.required_item_count[i],
                        text: strings.objective[i].clone(),
                    }),
                },
                request_text: strings.request,
                offer_text: strings.offer,
            };
            if out.quests.insert(row.id, metadata).is_some() {
                return Err(error(row.id, "quest", "duplicate_key"));
            }
            out.presentation.insert(row.id, present.clone());
        }
        if out.quests.keys().copied().collect::<BTreeSet<_>>() != ids {
            return Err(error(0, "quest", "coverage_mismatch"));
        }
        let entries: BTreeSet<_> = gameplay
            .givers()
            .iter()
            .map(|giver| giver.entry)
            .chain(
                gameplay
                    .catalog()
                    .outside_endpoints()
                    .iter()
                    .map(|e| e.creature),
            )
            .collect();
        let mut by_greeting = BTreeMap::new();
        for row in greetings {
            text(row.entry, &row.text, &mut bytes)?;
            if row.kind != 0
                || !entries.contains(&row.entry)
                || !emotes.contains(&row.emote)
                || by_greeting.insert(row.entry, row).is_some()
            {
                return Err(error(
                    row.entry,
                    "questgiver_greeting",
                    "invalid_type_emote_or_key",
                ));
            }
        }
        let mut greeting_translations = BTreeMap::new();
        for row in greeting_locales {
            text(row.entry, &row.text, &mut bytes)?;
            if row.locale == EventAiLocale::EnUs
                || row.kind != 0
                || !by_greeting.contains_key(&row.entry)
                || greeting_translations
                    .insert((row.entry, row.locale), &row.text)
                    .is_some()
            {
                return Err(error(
                    row.entry,
                    "locales_questgiver_greeting",
                    "invalid_key",
                ));
            }
        }
        let mut npc_by_id = BTreeMap::new();
        for row in npc_text {
            for option in &row.options {
                text(row.id, &option.text0, &mut bytes)?;
                text(row.id, &option.text1, &mut bytes)?;
                if !f32::from_bits(option.probability_bits).is_finite() {
                    return Err(error(row.id, "probability", "invalid_numeric"));
                }
            }
            if !self.npc_text_keys.contains(&row.id) || npc_by_id.insert(row.id, row).is_some() {
                return Err(error(row.id, "npc_text", "invalid_key"));
            }
        }
        let mut npc_translations = BTreeMap::new();
        for row in npc_locales {
            for (a, b) in &row.text {
                text(row.id, a, &mut bytes)?;
                text(row.id, b, &mut bytes)?;
            }
            if !npc_by_id.contains_key(&row.id)
                || npc_translations.insert((row.id, row.locale), row).is_some()
            {
                return Err(error(row.id, "locales_npc_text", "invalid_key"));
            }
        }
        let mut broadcast_by_id = BTreeMap::new();
        for row in broadcasts {
            for value in [&row.text, &row.text1].into_iter().flatten() {
                text(row.id as u32, value, &mut bytes)?;
            }
            if row.id <= 0 || broadcast_by_id.insert(row.id as u32, row).is_some() {
                return Err(error(row.id as u32, "broadcast_text", "invalid_key"));
            }
        }
        let mut broadcast_translations = BTreeMap::new();
        for row in broadcast_locales {
            for value in [&row.text, &row.text1].into_iter().flatten() {
                text(row.id as u32, value, &mut bytes)?;
            }
            let local = EventAiLocale::from_source_name(&row.locale);
            if !broadcast_by_id.contains_key(&(row.id as u32))
                || broadcast_translations
                    .insert((row.id as u32, local), row)
                    .is_some()
            {
                return Err(error(row.id as u32, "broadcast_text_locale", "invalid_key"));
            }
        }
        let mut seen = BTreeSet::new();
        let mut text_keys = BTreeSet::new();
        let mut broadcast_keys = BTreeSet::new();
        for id in gameplay.catalog().mechanics().script_broadcast_ids() {
            let row =
                broadcast_by_id
                    .get(&id)
                    .ok_or(error(id, "broadcast_text", "missing_lookup"))?;
            if row.chat_type != 0
                || row.language != 0
                || row.sounds != [0; 2]
                || row.emote_ids != [0; 3]
                || row.emote_delays != [0; 3]
                || (row.text.as_deref().unwrap_or("").is_empty()
                    && !row.text1.as_deref().unwrap_or("").is_empty())
            {
                return Err(error(id, "broadcast_text", "unsupported_script_speech"));
            }
            let value = n::localized_broadcast_text(
                row,
                broadcast_translations.get(&(id, locale)).copied(),
                locale,
                0,
            )
            .map_err(|_| error(id, "broadcast_text", "invalid_locale"))?;
            out.script_text.insert(id, value.to_owned());
            broadcast_keys.insert(id);
        }
        for id in gameplay.catalog().mechanics().gossip_text_ids() {
            text_keys.insert(id);
            let row = npc_by_id
                .get(&id)
                .ok_or(error(id, "npc_text", "missing_lookup"))?;
            let mut blocks = Vec::with_capacity(8);
            for (index, option) in row.options.iter().enumerate() {
                let (male, female) = if option.broadcast != 0 {
                    broadcast_keys.insert(option.broadcast);
                    let base = broadcast_by_id.get(&option.broadcast).ok_or(error(
                        option.broadcast,
                        "broadcast_text",
                        "missing_lookup",
                    ))?;
                    let translation = broadcast_translations
                        .get(&(option.broadcast, locale))
                        .copied();
                    let localize = |gender| {
                        n::localized_broadcast_text(base, translation, locale, gender)
                            .map(str::to_owned)
                            .map_err(|_| {
                                error(option.broadcast, "broadcast_text", "invalid_locale")
                            })
                    };
                    (localize(0)?, localize(1)?)
                } else {
                    let local = npc_translations.get(&(id, locale)).map(|l| &l.text[index]);
                    let a = local
                        .map(|p| &p.0)
                        .filter(|s| !s.is_empty())
                        .unwrap_or(&option.text0);
                    let b = local
                        .map(|p| &p.1)
                        .filter(|s| !s.is_empty())
                        .unwrap_or(&option.text1);
                    (a.clone(), b.clone())
                };
                blocks.push(p::NpcTextBlock {
                    probability: f32::from_bits(option.probability_bits),
                    male,
                    female,
                });
            }
            out.npc_text.insert(id, blocks);
        }
        for spawn in spawns {
            if spawn.gender > 2
                || !seen.insert(spawn.spawn)
                || !gameplay.givers().iter().any(|g| {
                    g.spawn == spawn.spawn
                        && g.entry == spawn.entry
                        && g.generation == spawn.generation
                })
            {
                return Err(error(spawn.entry, "creature", "binding_mismatch"));
            }
            let text_id = spawn
                .npc_gossip
                .ok_or(error(spawn.entry, "npc_gossip", "unknown_lookup"))?
                .filter(|id| *id != 0)
                .unwrap_or(0xffffff);
            text_keys.insert(text_id);
            let greeting = if let Some(row) = by_greeting.get(&spawn.entry) {
                NativeQuestGreeting {
                    has_greeting_row: true,
                    text: greeting_translations
                        .get(&(spawn.entry, locale))
                        .filter(|s| !s.is_empty())
                        .map_or(&row.text, |s| *s)
                        .clone(),
                    emote_delay: row.delay,
                    emote: row.emote,
                }
            } else if let Some(row) = npc_by_id.get(&text_id) {
                let option = &row.options[0];
                let title = if option.broadcast != 0 {
                    broadcast_keys.insert(option.broadcast);
                    let base = broadcast_by_id.get(&option.broadcast).ok_or(error(
                        option.broadcast,
                        "broadcast_text",
                        "missing_lookup",
                    ))?;
                    n::localized_broadcast_text(
                        base,
                        broadcast_translations
                            .get(&(option.broadcast, locale))
                            .copied(),
                        locale,
                        spawn.gender,
                    )
                    .map_err(|_| {
                        error(
                            option.broadcast,
                            "broadcast_text",
                            "invalid_locale_or_gender",
                        )
                    })?
                    .to_owned()
                } else {
                    let translated = npc_translations.get(&(text_id, locale)).map(|l| &l.text[0]);
                    let a = translated
                        .map(|p| &p.0)
                        .filter(|s| !s.is_empty())
                        .unwrap_or(&option.text0);
                    let b = translated
                        .map(|p| &p.1)
                        .filter(|s| !s.is_empty())
                        .unwrap_or(&option.text1);
                    if a.is_empty() {
                        b.clone()
                    } else {
                        a.clone()
                    }
                };
                NativeQuestGreeting {
                    has_greeting_row: false,
                    text: title,
                    emote_delay: option.emotes[0].0,
                    emote: option.emotes[0].1,
                }
            } else {
                NativeQuestGreeting {
                    has_greeting_row: false,
                    text: String::new(),
                    emote_delay: 0,
                    emote: 0,
                }
            };
            if out
                .greetings
                .get(&spawn.entry)
                .is_some_and(|previous| previous != &greeting)
            {
                return Err(error(
                    spawn.entry,
                    "greeting",
                    "incompatible_spawn_projection",
                ));
            }
            out.greetings.insert(spawn.entry, greeting);
        }
        if seen != gameplay.givers().iter().map(|g| g.spawn).collect()
            || self.npc_text_keys.len() != text_keys.len()
            || self.npc_text_keys.iter().copied().collect::<BTreeSet<_>>() != text_keys
            || broadcast_by_id.keys().copied().collect::<BTreeSet<_>>() != broadcast_keys
        {
            return Err(error(0, "greeting", "coverage_mismatch"));
        }
        // Exact raw text, locale/gender branches and source binding join the caller's full manifest.
        out.digest = Sha256::digest(format!(
            "benilla-captured-quest-presentation-v2:{self:?}:{database_locale}"
        ))
        .into();
        Ok(out)
    }
}
impl CompiledQuestPresentation {
    pub fn manifest_digest(&self) -> [u8; 32] {
        self.digest
    }
    pub fn quest_metadata(&self, id: u32) -> Option<&QuestMetadata> {
        self.quests.get(&id)
    }
    pub fn native_quest_presentation(&self, id: u32) -> Option<&NativeQuestPresentation> {
        self.presentation.get(&id)
    }
    pub fn native_quest_greeting(&self, entry: u32) -> Option<&NativeQuestGreeting> {
        self.greetings.get(&entry)
    }
    pub fn native_npc_text(&self, id: u32) -> Option<&[p::NpcTextBlock]> {
        self.npc_text.get(&id).map(Vec::as_slice)
    }
    pub fn native_quest_script_text(&self, id: u32) -> Option<&str> {
        self.script_text.get(&id).map(String::as_str)
    }
}

#[cfg(test)]
mod tests;
