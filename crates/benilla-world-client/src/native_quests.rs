// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic Player.cpp / QuestHandler.cpp at
// 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f. Copyright CMaNGOS contributors.
// Upstream GPL-2.0-or-later; Rust translation GPL-3.0-or-later, WITHOUT WARRANTY.
//! Source-bound, read-only native quest panels and ordered committed progression packets.
use super::*;
use benilla_sim::{player_progression as pg, quest as arithmetic, quest_lifecycle as q};

pub(super) struct Metadata {
    quests: BTreeMap<u32, crate::QuestMetadata>,
    greetings: BTreeMap<u32, crate::NativeQuestGreeting>,
    presentation: BTreeMap<u32, crate::NativeQuestPresentation>,
    player_bytes_2: Option<u32>,
}
fn valid_text(value: &str) -> bool {
    value.len() <= 16384
        && value
            .chars()
            .all(|c| !c.is_control() || matches!(c, '\n' | '\t'))
}
impl Metadata {
    pub(super) fn load(w: &n::NativeWorld, catalog: &impl ClientCatalog) -> Result<Self, Error> {
        let mut out = Self {
            quests: BTreeMap::new(),
            greetings: BTreeMap::new(),
            presentation: BTreeMap::new(),
            player_bytes_2: None,
        };
        let Some(g) = &w.profile().quest_progression else {
            return Ok(out);
        };
        out.player_bytes_2 = Some(
            catalog
                .player_bytes_2(w.state().player)
                .ok_or(Error::InvalidClientManifest)?
                & 0x00ff_ffff,
        );
        for id in g.spell_ids() {
            if id > u32::from(u16::MAX) {
                return Err(Error::Unrepresentable);
            }
            if !catalog.contains_spell(id) {
                return Err(Error::UnboundSpell(id));
            }
        }
        for id in g.quest_ids() {
            let row = g.quest(id).map_err(|_| Error::UnavailableQuest)?;
            let metadata = catalog
                .quest_metadata(id)
                .ok_or(Error::MissingQuestMetadata(id))?;
            validate_metadata(row, &metadata, catalog)?;
            let presentation = catalog
                .native_quest_presentation(id)
                .ok_or(Error::InvalidQuestMetadata(id))?;
            if presentation
                .details_emotes
                .iter()
                .chain(&presentation.offer_emotes)
                .any(|p| *p != (0, 0))
                || presentation.complete_emote_delay != 0
                || presentation.incomplete_emote_delay != 0
            {
                return Err(Error::InvalidQuestMetadata(id));
            }
            out.presentation.insert(id, presentation);
            out.quests.insert(id, metadata);
        }
        for relation in g
            .quest_relations()
            .starters
            .iter()
            .chain(&g.quest_relations().finishers)
        {
            if out.greetings.contains_key(&relation.creature) {
                continue;
            }
            let greeting = catalog
                .native_quest_greeting(relation.creature)
                .ok_or(Error::MissingQuestGreeting(relation.creature))?;
            if !valid_text(&greeting.text) {
                return Err(Error::Unrepresentable);
            }
            out.greetings.insert(relation.creature, greeting);
        }
        Ok(out)
    }
}
fn validate_metadata(
    row: &q::QuestRow,
    m: &crate::QuestMetadata,
    catalog: &impl ClientCatalog,
) -> Result<(), Error> {
    let t = &m.template;
    // Text is supplied by the verified local catalog. Numeric content is checked
    // against the admitted source row, never inferred from that text or a title.
    if t.quest_id != row.id
        || t.method != row.method
        || t.level != row.reward.xp.quest_level
        || t.quest_type != row.excluded.quest_type
        || t.rep_objective_faction != row.excluded.rep_objective_faction
        || t.rep_objective_value != row.excluded.rep_objective_value as u32
        || t.next_quest_in_chain != row.next_in_chain
        || t.money != row.reward.reward_or_required_money
        || t.money_max_level != row.reward.xp.reward_money_max_level
        || t.reward_spell != row.excluded.reward_spell
        || t.src_item_id != row.excluded.source_item
        || t.flags != u32::from(row.reward.quest_flags)
        || t.rewards
            != std::array::from_fn(|i| {
                (
                    row.excluded.reward_item[i],
                    row.excluded.reward_item_count[i],
                )
            })
        || t.choices
            != std::array::from_fn(|i| {
                (
                    row.excluded.reward_choice_item[i],
                    row.excluded.reward_choice_count[i],
                )
            })
        || !t.point_x.is_finite()
        || !t.point_y.is_finite()
        || !catalog.contains_map(t.point_map_id)
        || [
            &t.title,
            &t.details,
            &t.objectives_text,
            &t.end_text,
            &m.request_text,
            &m.offer_text,
        ]
        .iter()
        .any(|s| !valid_text(s))
    {
        return Err(Error::InvalidQuestMetadata(row.id));
    }
    for (i, objective) in t.objectives.iter().enumerate() {
        if objective.creature_or_go != row.required_creature[i] as u32
            || objective.required_count != row.required_creature_count[i]
            || objective.item_id != row.required_item[i]
            || objective.item_count != row.required_item_count[i]
            || !valid_text(&objective.text)
        {
            return Err(Error::InvalidQuestMetadata(row.id));
        }
    }
    Ok(())
}
fn genesis(w: &n::NativeWorld) -> Result<&n::QuestProgressionGenesis, Error> {
    w.profile()
        .quest_progression
        .as_ref()
        .ok_or(Error::UnavailableQuest)
}
fn state(w: &n::NativeWorld) -> Result<&n::QuestProgressionState, Error> {
    w.state()
        .quest_progression
        .as_ref()
        .ok_or(Error::UnavailableQuest)
}
fn quote(
    w: &n::NativeWorld,
    quest: u32,
    level: u32,
) -> Result<arithmetic::QuestRewardQuote, Error> {
    genesis(w)?
        .reward_quote(quest, level)
        .map_err(|_| Error::Unrepresentable)
}
fn identity(w: &n::NativeWorld, level: u8) -> Result<arithmetic::QuestPlayerIdentity, Error> {
    let player = state(w)?.player.view();
    Ok(arithmetic::QuestPlayerIdentity {
        race: player.race,
        class: player.class,
        level,
    })
}
fn relation(w: &n::NativeWorld, entry: u32, quest: u32, finishes: bool) -> Result<bool, Error> {
    let relations = genesis(w)?.quest_relations();
    Ok(if finishes {
        &relations.finishers
    } else {
        &relations.starters
    }
    .iter()
    .any(|r| r.creature == entry && r.quest == quest))
}
fn menu_icon(
    w: &n::NativeWorld,
    view: &q::QuestView,
    level: u8,
    entry: u32,
    quest: u32,
) -> Result<u32, Error> {
    let g = genesis(w)?;
    if !g.quest(quest).map_err(|_| Error::UnavailableQuest)?.active {
        return Ok(p::dialog_status::NONE);
    }
    if let Some(progress) = view.records.get(&quest) {
        if relation(w, entry, quest, true)? && !progress.rewarded {
            return Ok(match progress.status {
                q::Status::Incomplete => p::dialog_status::INCOMPLETE,
                q::Status::Complete => p::dialog_status::REWARD_REP,
            });
        }
    } else if relation(w, entry, quest, false)?
        && q::can_take_view_with_reputation(
            g.quest_catalog(),
            view,
            identity(w, level)?,
            quest,
            |faction| {
                state(w)
                    .map_err(|_| q::Error::Host("native reputation state"))?
                    .reputation(g, faction)
                    .map_err(|_| q::Error::Host("native reputation query"))
            },
        )
        .is_ok()
    {
        return Ok(p::dialog_status::AVAILABLE);
    }
    Ok(p::dialog_status::NONE)
}
fn marker_icon(
    w: &n::NativeWorld,
    view: &q::QuestView,
    level: u8,
    entry: u32,
    quest: u32,
) -> Result<u32, Error> {
    let g = genesis(w)?;
    let row = g.quest(quest).map_err(|_| Error::UnavailableQuest)?;
    if !row.active {
        return Ok(p::dialog_status::NONE);
    }
    if let Some(progress) = view.records.get(&quest) {
        if relation(w, entry, quest, true)? && !progress.rewarded {
            return Ok(if progress.status == q::Status::Complete {
                p::dialog_status::REWARD2
            } else {
                p::dialog_status::INCOMPLETE
            });
        }
        return Ok(p::dialog_status::NONE);
    }
    if !relation(w, entry, quest, false)?
        || q::can_see_start_view_with_reputation(
            g.quest_catalog(),
            view,
            identity(w, level)?,
            quest,
            |faction| {
                state(w)
                    .map_err(|_| q::Error::Host("native reputation state"))?
                    .reputation(g, faction)
                    .map_err(|_| q::Error::Host("native reputation query"))
            },
        )
        .is_err()
    {
        return Ok(p::dialog_status::NONE);
    }
    let rules = g.dialog_rules();
    let level = u32::from(level);
    if rules.high_level_hide_diff >= 0
        && (level + (rules.high_level_hide_diff as u32) < row.eligibility.minimum_level
            || level > row.eligibility.maximum_level)
    {
        return Ok(p::dialog_status::NONE);
    }
    if level < row.eligibility.minimum_level || level > row.eligibility.maximum_level {
        return Ok(p::dialog_status::UNAVAILABLE);
    }
    let quest_level = if row.reward.xp.quest_level == 0 {
        level
    } else {
        row.reward.xp.quest_level
    };
    Ok(
        if rules.low_level_hide_diff < 0 || level <= quest_level + rules.low_level_hide_diff as u32
        {
            p::dialog_status::AVAILABLE
        } else {
            p::dialog_status::CHAT
        },
    )
}
impl NativeProjection {
    fn query_world(&self, w: &n::NativeWorld) -> Result<(), Error> {
        self.check(w)?;
        if self.previous_digest != w.state_digest() {
            return Err(Error::StaleProjection);
        }
        Ok(())
    }
    fn quest_giver(&self, w: &n::NativeWorld, giver: NpcHandle, range: bool) -> Result<u32, Error> {
        self.query_world(w)?;
        let npc = w
            .state()
            .npcs
            .get(&giver.id.spawn)
            .filter(|n| {
                n.id == giver.id
                    && n.health > 0
                    && !matches!(
                        n.ai.phase(),
                        ai::Phase::Despawned
                            | ai::Phase::Corpse
                            | ai::Phase::ReturningHome
                            | ai::Phase::Combat
                    )
            })
            .ok_or(Error::UnknownTarget)?;
        let profile = w
            .profile()
            .npc(giver.id.spawn)
            .ok_or(Error::UnknownTarget)?;
        if w.state().health == 0
            || genesis(w)?
                .npc_flags(profile.entry)
                .is_none_or(|f| f & 2 == 0)
        {
            return Err(Error::UnavailableQuest);
        }
        if range
            && !ai::within_interaction_distance(
                w.state().position,
                npc.ai.position(),
                w.profile().player.combat_reach,
                profile.ai.combat_reach,
            )
            .map_err(|_| Error::Unrepresentable)?
        {
            return Err(Error::OutOfRange);
        }
        Ok(profile.entry)
    }
    fn metadata(&self, quest: u32) -> Result<&crate::QuestMetadata, Error> {
        self.quest_metadata
            .quests
            .get(&quest)
            .ok_or(Error::MissingQuestMetadata(quest))
    }
    /// Preview only: committed owner commands repeat authority checks.
    /// Empty menus produce no packet, as Player::SendPreparedQuest does.
    pub fn quest_greeting_events(
        &self,
        w: &n::NativeWorld,
        giver: NpcHandle,
    ) -> Result<Vec<SessionEvent>, Error> {
        let entry = self.quest_giver(w, giver, true)?;
        let view = state(w)?.quests.view();
        let mut quests = Vec::new();
        let mut seen = BTreeSet::new();
        let relations = genesis(w)?.quest_relations();
        // Separate source passes: an unaccepted starter must never be inserted
        // during the finisher pass merely because it also has that relation.
        for (finishes, rows) in [(true, &relations.finishers), (false, &relations.starters)] {
            for r in rows.iter().filter(|r| r.creature == entry) {
                if finishes != view.records.contains_key(&r.quest) {
                    continue;
                }
                let icon = menu_icon(w, &view, w.player_level(), entry, r.quest)?;
                if icon == p::dialog_status::NONE || !seen.insert(r.quest) {
                    continue;
                }
                let t = &self.metadata(r.quest)?.template;
                quests.push(p::QuestListEntry {
                    quest_id: r.quest,
                    icon,
                    level: t.level,
                    title: crate::plain_ui_text(&t.title),
                });
            }
        }
        if quests.is_empty() {
            return Ok(vec![]);
        }
        let greeting = self
            .quest_metadata
            .greetings
            .get(&entry)
            .ok_or(Error::MissingQuestGreeting(entry))?;
        if quests.len() == 1
            && !(greeting.has_greeting_row && genesis(w)?.dialog_rules().always_show_quest_greeting)
        {
            let item = &quests[0];
            let event = if matches!(
                item.icon,
                p::dialog_status::REWARD_REP | p::dialog_status::INCOMPLETE
            ) {
                self.quest_progress_packet(w, item.quest_id, giver, true)?
            } else {
                self.quest_detail_packet(w, item.quest_id, giver.id, w.player_level())?
            };
            return Ok(vec![event]);
        }
        Ok(vec![SessionEvent::QuestGreeting(p::QuestGiverList {
            npc: npc_guid(w, giver.id)?,
            greeting: crate::plain_ui_text(&greeting.text),
            emote_delay: greeting.emote_delay,
            emote: greeting.emote,
            quests,
        })])
    }
    /// Single-packet convenience; controllers use quest_greeting_events so an
    /// empty source menu is represented without an invented greeting packet.
    pub fn quest_greeting(
        &self,
        w: &n::NativeWorld,
        giver: NpcHandle,
    ) -> Result<SessionEvent, Error> {
        self.quest_greeting_events(w, giver)?
            .pop()
            .ok_or(Error::UnavailableQuest)
    }
    pub fn quest_template(&self, w: &n::NativeWorld, quest: u32) -> Result<SessionEvent, Error> {
        self.query_world(w)?;
        let mut t = self.metadata(quest)?.template.clone();
        t.money = if t.flags & p::quest_flags::HIDDEN_REWARDS != 0 {
            0
        } else {
            quote(w, quest, u32::from(w.player_level()))?.reward_or_required_money
        };
        if t.flags & p::quest_flags::HIDDEN_REWARDS != 0 {
            t.choices = [(0, 0); 6];
            t.rewards = [(0, 0); 4];
        }
        if t.flags & u32::from(arithmetic::QUEST_FLAGS_NO_MONEY_FROM_XP) != 0 {
            t.money_max_level = 0;
        }
        t.title = crate::plain_ui_text(&t.title);
        t.details = crate::plain_ui_text(&t.details);
        t.objectives_text = crate::plain_ui_text(&t.objectives_text);
        t.end_text = crate::plain_ui_text(&t.end_text);
        for objective in &mut t.objectives {
            objective.text = crate::plain_ui_text(&objective.text);
        }
        Ok(SessionEvent::QuestTemplate(Box::new(t)))
    }
    pub fn quest_detail(
        &self,
        w: &n::NativeWorld,
        quest: u32,
        giver: NpcHandle,
    ) -> Result<SessionEvent, Error> {
        let entry = self.quest_giver(w, giver, true)?;
        if !relation(w, entry, quest, false)?
            || menu_icon(w, &state(w)?.quests.view(), w.player_level(), entry, quest)?
                != p::dialog_status::AVAILABLE
        {
            return Err(Error::UnavailableQuest);
        }
        self.quest_detail_packet(w, quest, giver.id, w.player_level())
    }
    fn quest_detail_packet(
        &self,
        w: &n::NativeWorld,
        quest: u32,
        giver: NpcId,
        level: u8,
    ) -> Result<SessionEvent, Error> {
        let t = &self.metadata(quest)?.template;
        Ok(SessionEvent::QuestDetail(p::QuestDetails {
            npc: npc_guid(w, giver)?,
            quest_id: quest,
            title: crate::plain_ui_text(&t.title),
            details: crate::plain_ui_text(&t.details),
            objectives: crate::plain_ui_text(&t.objectives_text),
            auto_finish: 1,
            choices: if t.flags & p::quest_flags::HIDDEN_REWARDS != 0 {
                vec![]
            } else {
                self.reward_items(&t.choices)?
            },
            rewards: if t.flags & p::quest_flags::HIDDEN_REWARDS != 0 {
                vec![]
            } else {
                self.reward_items(&t.rewards)?
            },
            money: if t.flags & p::quest_flags::HIDDEN_REWARDS != 0 {
                0
            } else {
                quote(w, quest, u32::from(level))?.reward_or_required_money
            },
            reward_spell: 0,
        }))
    }
    fn quest_progress_complete(
        &self,
        w: &n::NativeWorld,
        quest: u32,
        giver: NpcHandle,
    ) -> Result<bool, Error> {
        let entry = self.quest_giver(w, giver, true)?;
        if !relation(w, entry, quest, true)? {
            return Err(Error::UnavailableQuest);
        }
        let progress = state(w)?
            .quests
            .progress(quest)
            .filter(|p| !p.rewarded)
            .ok_or(Error::UnavailableQuest)?;
        let quote = quote(w, quest, u32::from(w.player_level()))?;
        Ok(progress.status == q::Status::Complete
            && state(w)?.money >= quote.money_required_before_reward)
    }
    pub fn quest_progress(
        &self,
        w: &n::NativeWorld,
        quest: u32,
        giver: NpcHandle,
    ) -> Result<SessionEvent, Error> {
        self.quest_progress_packet(w, quest, giver, false)
    }
    fn quest_progress_packet(
        &self,
        w: &n::NativeWorld,
        quest: u32,
        giver: NpcHandle,
        close_on_cancel: bool,
    ) -> Result<SessionEvent, Error> {
        let is_complete = self.quest_progress_complete(w, quest, giver)?;
        let m = self.metadata(quest)?;
        // SendQuestGiverRequestItems retains the required-item panel even when
        // complete, unless the source request text is empty.
        if (is_complete && m.template.objectives.iter().all(|o| o.item_id == 0))
            || m.request_text.is_empty()
        {
            return self.quest_offer_packet(w, quest, giver);
        }
        let presentation = self
            .quest_metadata
            .presentation
            .get(&quest)
            .ok_or(Error::InvalidQuestMetadata(quest))?;
        Ok(SessionEvent::QuestProgress(p::QuestRequestItems {
            npc: npc_guid(w, giver.id)?,
            quest_id: quest,
            title: crate::plain_ui_text(&m.template.title),
            request_text: crate::plain_ui_text(&m.request_text),
            emote: if is_complete {
                presentation.complete_emote
            } else {
                presentation.incomplete_emote
            },
            close_on_cancel: u32::from(close_on_cancel),
            required_money: quote(w, quest, u32::from(w.player_level()))?
                .money_required_before_reward,
            required_items: {
                let row = genesis(w)?
                    .quest(quest)
                    .map_err(|_| Error::UnavailableQuest)?;
                row.required_item
                    .iter()
                    .zip(&row.required_item_count)
                    .filter(|(id, _)| **id != 0)
                    .map(|(&item_id, &count)| {
                        let info = self
                            .loot_metadata
                            .item(item_id)
                            .ok_or(Error::MissingItemMetadata(item_id))?;
                        Ok(p::QuestRequiredItem {
                            item_id,
                            count,
                            display_id: info.display_info_id,
                        })
                    })
                    .collect::<Result<Vec<_>, Error>>()?
            },
            is_complete,
        }))
    }
    pub fn quest_offer(
        &self,
        w: &n::NativeWorld,
        quest: u32,
        giver: NpcHandle,
    ) -> Result<SessionEvent, Error> {
        if !self.quest_progress_complete(w, quest, giver)? {
            return Err(Error::UnavailableQuest);
        }
        self.quest_offer_packet(w, quest, giver)
    }
    fn quest_offer_packet(
        &self,
        w: &n::NativeWorld,
        quest: u32,
        giver: NpcHandle,
    ) -> Result<SessionEvent, Error> {
        self.quest_offer_packet_at(w, quest, giver.id, w.player_level())
    }
    fn reward_items(&self, rows: &[(u32, u32)]) -> Result<Vec<p::QuestRewardItem>, Error> {
        rows.iter()
            .filter(|(id, _)| *id != 0)
            .map(|&(item_id, count)| {
                let info = self
                    .loot_metadata
                    .item(item_id)
                    .ok_or(Error::MissingItemMetadata(item_id))?;
                Ok(p::QuestRewardItem {
                    item_id,
                    count,
                    display_id: info.display_info_id,
                })
            })
            .collect()
    }
    fn quest_offer_packet_at(
        &self,
        w: &n::NativeWorld,
        quest: u32,
        giver: NpcId,
        level: u8,
    ) -> Result<SessionEvent, Error> {
        let m = self.metadata(quest)?;
        Ok(SessionEvent::QuestOffer(p::QuestOfferReward {
            npc: npc_guid(w, giver)?,
            quest_id: quest,
            title: crate::plain_ui_text(&m.template.title),
            offer_text: crate::plain_ui_text(&m.offer_text),
            auto_finish: 1,
            choices: self.reward_items(&m.template.choices)?,
            rewards: self.reward_items(&m.template.rewards)?,
            money: quote(w, quest, u32::from(level))?.reward_or_required_money,
            // The inherited DTO names this word quest_flags; pinned CMaNGOS
            // writes RewSpell here, followed by RewSpellCast. Both are admitted zero.
            quest_flags: 0,
            reward_spell: 0,
        }))
    }
    /// Core status calculation after the caller's bound generic CanInteract check.
    /// Status queries have no interaction-distance, alive, or combat gate.
    pub fn quest_status(
        &self,
        w: &n::NativeWorld,
        giver: NpcHandle,
    ) -> Result<SessionEvent, Error> {
        self.query_world(w)?;
        w.state()
            .npcs
            .get(&giver.id.spawn)
            .filter(|npc| npc.id == giver.id && npc.ai.phase() != ai::Phase::Despawned)
            .ok_or(Error::UnknownTarget)?;
        let entry = w
            .profile()
            .npc(giver.id.spawn)
            .ok_or(Error::UnknownTarget)?
            .entry;
        self.quest_status_at(
            w,
            &state(w)?.quests.view(),
            w.player_level(),
            giver.id,
            entry,
        )
    }
    fn quest_status_at(
        &self,
        w: &n::NativeWorld,
        view: &q::QuestView,
        level: u8,
        id: NpcId,
        entry: u32,
    ) -> Result<SessionEvent, Error> {
        let mut status = p::dialog_status::NONE;
        let relations = genesis(w)?.quest_relations();
        for r in relations
            .finishers
            .iter()
            .chain(&relations.starters)
            .filter(|r| r.creature == entry)
        {
            status = status.max(marker_icon(w, view, level, entry, r.quest)?);
        }
        Ok(SessionEvent::QuestGiverStatus {
            npc: npc_guid(w, id)?,
            status,
        })
    }
}

fn skill_pairs(skill: &pg::SkillState) -> [(u16, u32); 3] {
    let base = 718 + 3 * u16::from(skill.position);
    if skill.status == pg::SkillStatus::Deleted {
        return [(base, 0), (base + 1, 0), (base + 2, 0)];
    }
    [
        (base, u32::from(skill.id) | (u32::from(skill.step) << 16)),
        (
            base + 1,
            u32::from(skill.value) | (u32::from(skill.maximum) << 16),
        ),
        (
            base + 2,
            u32::from(skill.temporary_bonus as u16)
                | (u32::from(skill.permanent_bonus as u16) << 16),
        ),
    ]
}
fn log_window(view: &q::QuestView) -> [[u32; 3]; q::LOG_SIZE] {
    std::array::from_fn(|slot| {
        let Some(id) = view.log[slot] else {
            return [0; 3];
        };
        let progress = &view.records[&id];
        let counts = progress
            .creatures
            .iter()
            .enumerate()
            .fold(0, |bits, (i, n)| bits | (*n << (6 * i)));
        [
            id,
            counts | (u32::from(progress.status == q::Status::Complete) << 24),
            0,
        ]
    })
}
pub(super) fn player_pairs(w: &n::NativeWorld, metadata: &Metadata) -> Vec<(u16, u32)> {
    let Some(s) = &w.state().quest_progression else {
        return vec![];
    };
    let p = s.player.view();
    let mut pairs = vec![
        (162, p.modifiers.create_mana),
        (163, p.modifiers.create_health),
        (716, p.xp),
        (717, p.next_level_xp),
        (1176, s.money),
        (1102, p.free_talent_points),
        (1175, s.rest.experience),
    ];
    if let Some(bytes) = metadata.player_bytes_2 {
        pairs.push((194, bytes | (u32::from(s.rest.client_state) << 24)));
    }
    for (i, value) in p.numeric.powers.iter().enumerate() {
        pairs.push((23 + i as u16, *value));
    }
    for (i, value) in p.numeric.max_powers.iter().enumerate() {
        pairs.push((29 + i as u16, *value));
    }
    // Position is the native PLAYER_SKILL_INFO slot, unrelated to map iteration order.
    for skill in &p.skills {
        pairs.extend(skill_pairs(skill));
    }
    for (slot, fields) in log_window(&s.quests.view()).iter().enumerate() {
        for (offset, value) in fields.iter().enumerate() {
            pairs.push((198 + 3 * slot as u16 + offset as u16, *value));
        }
    }
    let d = &s.derived;
    pairs.extend([
        (1106, d.defense.block_display.bits()),
        (1107, d.defense.dodge_display.bits()),
        (1108, d.defense.parry_display.bits()),
        (1109, d.crit_display_percent[0].bits()),
        (1110, d.crit_display_percent[1].bits()),
    ]);
    for (i, power) in d.attack_power.iter().enumerate() {
        let base = 165 + i as u16 * 3;
        pairs.extend([
            (base, power.base),
            (
                base + 1,
                u32::from(power.positive_bonus as u16)
                    | (u32::from(power.negative_bonus as u16) << 16),
            ),
            (base + 2, power.multiplier.bits()),
        ]);
    }
    for (base, damage) in [134, 136, 171].into_iter().zip(d.damage) {
        pairs.extend([
            (base, damage.minimum.bits()),
            (base + 1, damage.maximum.bits()),
        ]);
    }
    for (i, value) in d.spell_damage_bonus.iter().enumerate() {
        pairs.push((1201 + i as u16, *value as u32));
    }
    pairs
}
pub(super) fn spell_ids(w: &n::NativeWorld) -> Vec<u32> {
    w.state().quest_progression.as_ref().map_or_else(
        || w.abilities().iter().map(|a| a.id).collect(),
        |q| {
            q.training
                .abilities
                .iter()
                .filter(|a| a.active)
                .map(|a| a.spell_id)
                .collect()
        },
    )
}
pub(super) struct Frame {
    next_training_call: usize,
    known_spells: BTreeSet<u32>,
    log: [[u32; 3]; q::LOG_SIZE],
}
impl Frame {
    pub(super) fn new(previous: &n::State) -> Self {
        Self {
            next_training_call: previous
                .quest_progression
                .as_ref()
                .map_or(0, |s| s.training.calls.len()),
            known_spells: previous
                .quest_progression
                .as_ref()
                .map_or_else(BTreeSet::new, |s| {
                    s.training
                        .abilities
                        .iter()
                        .filter(|s| s.active)
                        .map(|s| s.spell_id)
                        .collect()
                }),
            log: previous
                .quest_progression
                .as_ref()
                .map_or([[0; 3]; q::LOG_SIZE], |s| log_window(&s.quests.view())),
        }
    }
    pub(super) fn finish(&self, w: &n::NativeWorld) -> Result<(), Error> {
        if let Some(s) = &w.state().quest_progression {
            if self.next_training_call != s.training.calls.len()
                || self.known_spells != spell_ids(w).into_iter().collect()
            {
                return Err(Error::StaleProjection);
            }
        }
        Ok(())
    }
    fn slot_event(&self, guid: u64, slot: usize) -> Result<SessionEvent, Error> {
        let fields = self.log.get(slot).ok_or(Error::Unrepresentable)?;
        Ok(values(
            guid,
            &std::array::from_fn::<_, 3, _>(|i| (198 + 3 * slot as u16 + i as u16, fields[i])),
        ))
    }
}
fn values(guid: u64, pairs: &[(u16, u32)]) -> SessionEvent {
    SessionEvent::ObjectValues {
        guid,
        fields: ObjectFields::from_pairs(pairs),
    }
}
fn progression_events(
    guid: u64,
    event: &pg::Event,
    view: &pg::Snapshot,
) -> Result<Vec<SessionEvent>, Error> {
    let mut events = vec![];
    let pairs = match *event {
        pg::Event::NonKillXpLog { xp } => {
            events.push(SessionEvent::XpGain(p::XpGain {
                victim: 0,
                total: xp,
                base: xp,
                kill: false,
            }));
            vec![]
        }
        pg::Event::LevelUpPacket { words } => {
            events.push(SessionEvent::LevelUp(p::LevelUpInfo {
                level: words[0],
                health: words[1] as i32,
                powers: std::array::from_fn(|i| words[2 + i] as i32),
                stats: std::array::from_fn(|i| words[7 + i] as i32),
            }));
            vec![]
        }
        pg::Event::NextLevelXp(xp) => vec![(717, xp)],
        pg::Event::Level(level) => vec![(34, level)],
        pg::Event::SkillValue { id, .. } => skill_pairs(
            view.skills
                .iter()
                .find(|s| s.id == id)
                .ok_or(Error::Unrepresentable)?,
        )
        .to_vec(),
        pg::Event::Stat { index, value } => vec![(150 + u16::from(index), value as u32)],
        pg::Event::Armor(value) => vec![(155, value as u32)],
        pg::Event::Resistance { school, value } => vec![(155 + u16::from(school), value as u32)],
        pg::Event::CreateHealth(value) => vec![(163, value)],
        pg::Event::CreateMana(value) => vec![(162, value)],
        pg::Event::MaxHealth(value) => vec![(28, value)],
        pg::Event::MaxPower { power, value } => vec![(29 + u16::from(power), value)],
        pg::Event::Health(value) => vec![(22, value)],
        pg::Event::Power { power, value } => vec![(23 + u16::from(power), value)],
        pg::Event::InitTalentForLevel => vec![(1102, view.free_talent_points)],
        pg::Event::Xp(xp) => vec![(716, xp)],
        _ => vec![],
    };
    if !pairs.is_empty() {
        events.push(values(guid, &pairs));
    }
    Ok(events)
}
impl NativeProjection {
    fn ordered_progression_events(
        &self,
        w: &n::NativeWorld,
        event: &pg::Event,
        view: &pg::Snapshot,
        frame: &mut Frame,
    ) -> Result<Vec<SessionEvent>, Error> {
        let mut out = vec![];
        if let pg::Event::SkillTrainedSpells { id, value, step } = event {
            let state = state(w)?;
            let call = state
                .training
                .calls
                .get(frame.next_training_call)
                .ok_or(Error::StaleProjection)?;
            if call.id != *id || call.value != *value || call.step != *step || call.view != *view {
                return Err(Error::StaleProjection);
            }
            for &spell in &call.learned_spells {
                if !frame.known_spells.insert(spell) {
                    return Err(Error::StaleProjection);
                }
                out.push(SessionEvent::SpellLearned { spell_id: spell });
            }
            frame.next_training_call += 1;
        }
        out.extend(progression_events(self.player_guid, event, view)?);
        Ok(out)
    }
    pub(super) fn quest_progression_event(
        &self,
        w: &n::NativeWorld,
        event: &n::QuestProgressionEvent,
        frame: &mut Frame,
        inventory_frame: &mut loot::Frame,
    ) -> Result<Vec<SessionEvent>, Error> {
        let mut events = vec![];
        match event {
            n::QuestProgressionEvent::Reputation(event) => {
                events.extend(reputation_event(event)?);
            }
            n::QuestProgressionEvent::RewardItem(stored) => {
                events.extend(inventory_frame.reward_item(self, w, stored)?);
            }
            n::QuestProgressionEvent::RewardRefused {
                quest,
                giver,
                level,
                result,
            } => {
                if let Some(result) = result {
                    events.push(SessionEvent::InventoryFailure {
                        reason: result.protocol(),
                        required_level: None,
                        item_guid: 0,
                        item2_guid: 0,
                        bag_slot: 0,
                    });
                }
                // This callback may precede a successful second request in the same frame.
                events.push(self.quest_offer_packet_at(w, *quest, *giver, *level)?);
            }
            n::QuestProgressionEvent::ChainDetails {
                quest,
                giver,
                level,
            } => {
                events.push(self.quest_detail_packet(w, *quest, *giver, *level)?);
            }
            n::QuestProgressionEvent::PartyKill { victim } => {
                events.push(SessionEvent::PartyKillLog(p::PartyKillLog {
                    killer: self.player_guid,
                    victim: npc_guid(w, *victim)?,
                }))
            }
            n::QuestProgressionEvent::Progression(observation) => events.extend(
                self.ordered_progression_events(w, &observation.event, &observation.view, frame)?,
            ),
            n::QuestProgressionEvent::KillExperience(observation) => match &observation.event {
                pg::kill_xp::Event::KillXpLog {
                    victim_guid,
                    total_xp,
                    base_xp,
                    ..
                } => events.push(SessionEvent::XpGain(p::XpGain {
                    victim: *victim_guid,
                    total: *total_xp,
                    base: *base_xp,
                    kill: true,
                })),
                pg::kill_xp::Event::RestExperience(xp) => {
                    events.push(values(self.player_guid, &[(1175, *xp)]))
                }
                pg::kill_xp::Event::Progression(event) => events
                    .extend(self.ordered_progression_events(w, event, &observation.view, frame)?),
                pg::kill_xp::Event::RestStateByte(rest) => {
                    let bytes = self
                        .quest_metadata
                        .player_bytes_2
                        .ok_or(Error::InvalidClientManifest)?;
                    events.push(values(
                        self.player_guid,
                        &[(194, bytes | (u32::from(*rest) << 24))],
                    ));
                }
            },
            n::QuestProgressionEvent::Quest {
                event,
                level,
                money,
            } => match event {
                q::Event::LogSet { slot, quest } => {
                    *frame.log.get_mut(*slot).ok_or(Error::Unrepresentable)? =
                        [quest.unwrap_or(0), 0, 0];
                    events.push(frame.slot_event(self.player_guid, *slot)?);
                }
                q::Event::LogComplete { slot, complete } => {
                    let log = frame.log.get_mut(*slot).ok_or(Error::Unrepresentable)?;
                    log[1] = (log[1] & !(1 << 24)) | (u32::from(*complete) << 24);
                    events.push(frame.slot_event(self.player_guid, *slot)?);
                }
                // Player::UpdateForQuestWorldObjects only updates dynamic GO
                // fields. This profile has no GOs; NPC markers require separate
                // authorized status queries, never invented callback packets.
                q::Event::WorldObjects { .. } => {}
                q::Event::KillCredit {
                    quest,
                    objective,
                    count,
                    ..
                } => {
                    let row = genesis(w)?
                        .quest(*quest)
                        .map_err(|_| Error::UnavailableQuest)?;
                    if *objective >= q::OBJECTIVES || *count > 63 {
                        return Err(Error::Unrepresentable);
                    }
                    let slot = frame
                        .log
                        .iter()
                        .position(|s| s[0] == *quest)
                        .ok_or(Error::StaleProjection)?;
                    let shift = 6 * objective;
                    frame.log[slot][1] = (frame.log[slot][1] & !(63 << shift)) | (count << shift);
                    events.push(SessionEvent::QuestObjectiveKill {
                        quest_id: *quest,
                        entry: row.required_creature[*objective] as u32,
                        count: *count,
                        required: row.required_creature_count[*objective],
                    });
                    events.push(frame.slot_event(self.player_guid, slot)?);
                }
                q::Event::Host(q::HostEffect::ModifyMoney(_)) => {
                    events.push(values(self.player_guid, &[(1176, *money)]))
                }
                q::Event::RewardAnnouncement { quest, xp } => {
                    // Player::SendQuestReward branches on the level AFTER GiveXP.
                    let quote = quote(w, *quest, u32::from(*level))?;
                    let cap_money = quote.max_level_money;
                    let money = i64::from(quote.reward_or_required_money)
                        + i64::from(cap_money.unwrap_or(0));
                    let money = i32::try_from(money).map_err(|_| Error::Unrepresentable)? as u32;
                    events.push(SessionEvent::QuestComplete(p::QuestComplete {
                        quest_id: *quest,
                        xp: if cap_money.is_some() { 0 } else { *xp },
                        money,
                        // SendQuestReward reports fixed template rows even if storage skipped
                        // a later grant; selected choices never appear in this packet.
                        items: self
                            .metadata(*quest)?
                            .template
                            .rewards
                            .iter()
                            .copied()
                            .filter(|(id, _)| *id != 0)
                            .collect(),
                    }));
                }
                q::Event::ItemCredit {
                    quest,
                    objective,
                    added,
                    ..
                } => {
                    let row = genesis(w)?
                        .quest(*quest)
                        .map_err(|_| Error::UnavailableQuest)?;
                    let item_id = *row
                        .required_item
                        .get(*objective)
                        .ok_or(Error::Unrepresentable)?;
                    if item_id == 0 || *added >= 64 {
                        return Err(Error::Unrepresentable);
                    }
                    events.push(SessionEvent::QuestObjectiveItem {
                        item_id,
                        count: *added,
                    });
                }
                q::Event::Host(q::HostEffect::DestroyCarriedStack { item, count }) => {
                    events.extend(inventory_frame.remove_carried_stack(self, w, *item, *count)?);
                }
                _ => {}
            },
        }
        Ok(events)
    }
}

#[cfg(test)]
#[path = "native_quests_tests.rs"]
pub(super) mod tests;

fn reputation_event(event: &benilla_sim::reputation::Event) -> Result<Vec<SessionEvent>, Error> {
    use benilla_sim::reputation::Event;
    Ok(match event {
        Event::Visible { list_id } => vec![SessionEvent::ReputationVisible { list_id: *list_id }],
        Event::StateSent { standings } => vec![SessionEvent::ReputationDelta {
            standings: standings.clone(),
        }],
        Event::ReputationChanged { .. } => vec![],
        Event::InitialSent { factions } => vec![initial_reputation(factions.iter())?],
    })
}
fn initial_reputation<'a>(
    factions: impl Iterator<Item = &'a benilla_sim::reputation::FactionState>,
) -> Result<SessionEvent, Error> {
    let mut standings = vec![(0, 0); 64];
    for faction in factions {
        let entry = standings
            .get_mut(faction.reputation_list_id as usize)
            .ok_or(Error::Unrepresentable)?;
        // Source serializes uint8(Flags), including the byte-width conversion.
        *entry = (faction.flags as u8, faction.standing);
    }
    Ok(SessionEvent::Reputations { standings })
}
impl NativeProjection {
    pub(super) fn initial_reputations(
        &self,
        w: &n::NativeWorld,
    ) -> Result<Vec<SessionEvent>, Error> {
        w.state()
            .quest_progression
            .as_ref()
            .and_then(|q| q.reputation.as_ref())
            .map(|r| initial_reputation(r.view().factions().values()).map(|e| vec![e]))
            .transpose()
            .map(Option::unwrap_or_default)
    }
}
