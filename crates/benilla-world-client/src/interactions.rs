// SPDX-License-Identifier: GPL-3.0-or-later
//! Guarded presentation for the supported plain-item and kill-quest workflow.
use crate::*;

fn item_guid(world: runtime::WorldId, owner: PlayerId, instance: u64) -> u64 {
    let digest = hash(b"benilla-world-item-guid-v1\0", &(world, owner, instance));
    let mut bytes = [0; 8];
    bytes[..6].copy_from_slice(&digest[..6]);
    0x4000_0000_0000_0000 | u64::from_le_bytes(bytes).max(1)
}
impl Projection {
    pub fn inventory_guid(&self, world: &World, instance: u64) -> Result<u64, Error> {
        self.adapter.check_world(world)?;
        if !world
            .player(self.adapter.selected)
            .ok_or(Error::UnknownPlayer)?
            .inventory
            .iter()
            .any(|i| i.instance == instance)
        {
            return Err(Error::UnknownTarget);
        }
        Ok(item_guid(
            world.state().world,
            self.adapter.selected,
            instance,
        ))
    }
    pub(crate) fn item_events(
        &self,
        world: &World,
        initial: bool,
    ) -> Result<Vec<SessionEvent>, Error> {
        let p = world
            .player(self.adapter.selected)
            .ok_or(Error::UnknownPlayer)?;
        let old = self
            .previous
            .players
            .iter()
            .find(|p| p.id == self.adapter.selected)
            .ok_or(Error::UnknownPlayer)?;
        let mut seen = BTreeSet::new();
        let mut events = Vec::new();
        for i in &p.inventory {
            let guid = item_guid(world.state().world, p.id, i.instance);
            if !seen.insert(guid) {
                return Err(Error::GuidCollision);
            }
            match old
                .inventory
                .iter()
                .find(|v| v.instance == i.instance)
                .filter(|_| !initial)
            {
                None => {
                    let owner = self.players[&p.id];
                    events.push(SessionEvent::ItemCreate {
                        guid,
                        container: false,
                        fields: ObjectFields::from_pairs(&[
                            (0, guid as u32),
                            (1, (guid >> 32) as u32),
                            (2, 3),
                            (3, i.item_entry),
                            (4, 1.0f32.to_bits()),
                            (6, owner as u32),
                            (7, (owner >> 32) as u32),
                            (8, owner as u32),
                            (9, (owner >> 32) as u32),
                            (14, u32::from(i.count)),
                        ])
                        .into_created(protocol::ObjectType::Item),
                    });
                }
                Some(previous) if previous != i => events.push(SessionEvent::ObjectValues {
                    guid,
                    fields: ObjectFields::from_pairs(&[(14, u32::from(i.count))]),
                }),
                _ => {}
            }
        }
        if !initial {
            let from_npc = world.last_tick().is_some_and(|r| r.result.events.iter().any(|e| matches!(e, runtime::Event::QuestRewarded { player, .. } if *player == p.id)));
            for i in &p.inventory {
                let before_count = old
                    .inventory
                    .iter()
                    .find(|v| v.instance == i.instance)
                    .map_or(0, |v| v.count);
                if i.count > before_count {
                    events.push(SessionEvent::ItemPushResult(protocol::ItemPushResult {
                        player_guid: self.players[&p.id],
                        from_npc,
                        created: false,
                        show_in_chat: true,
                        bag_slot: protocol::BAG_PLAYER_INVENTORY,
                        item_slot: if before_count == 0 {
                            u32::from(protocol::SLOT_PACK_FIRST) + u32::from(i.slot)
                        } else {
                            u32::MAX
                        },
                        item_entry: i.item_entry,
                        suffix_factor: 0,
                        random_property_id: 0,
                        count: u32::from(i.count - before_count),
                    }));
                }
            }
            for i in &old.inventory {
                if !p.inventory.iter().any(|v| v.instance == i.instance) {
                    events.push(SessionEvent::ObjectDestroyed(item_guid(
                        world.state().world,
                        p.id,
                        i.instance,
                    )));
                }
            }
        }
        Ok(events)
    }
    pub(crate) fn pack_pairs(&self, world: &World) -> Result<Vec<(u16, u32)>, Error> {
        let p = world
            .player(self.adapter.selected)
            .ok_or(Error::UnknownPlayer)?;
        let mut pairs = Vec::with_capacity(32);
        for slot in 0..16u16 {
            let guid = p
                .inventory
                .iter()
                .find(|i| i.slot == slot)
                .map_or(0, |i| item_guid(world.state().world, p.id, i.instance));
            pairs.extend([
                (532 + 2 * slot, guid as u32),
                (533 + 2 * slot, (guid >> 32) as u32),
            ]);
        }
        Ok(pairs)
    }
    /// Query-only metadata response. Never creates an item or grants inventory.
    pub fn item_template(&self, entry: u32) -> Result<SessionEvent, Error> {
        let mut info = self
            .metadata
            .items
            .get(&entry)
            .cloned()
            .ok_or(Error::MissingItemMetadata(entry))?;
        info.name = plain_ui_text(&info.name);
        info.description = plain_ui_text(&info.description);
        Ok(SessionEvent::ItemTemplate {
            entry,
            info: Some(Box::new(info)),
        })
    }
    pub(crate) fn loot_response(&self, world: &World, id: NpcId) -> Result<SessionEvent, Error> {
        let p = world
            .player(self.adapter.selected)
            .ok_or(Error::UnknownPlayer)?;
        let npc = world
            .npc(id.spawn)
            .filter(|n| n.id == id && n.tagged_by == Some(p.id) && n.health == 0)
            .ok_or(Error::UnknownTarget)?;
        if p.active_loot != Some(id) {
            return Err(Error::LootNotOpen);
        }
        let mut items = Vec::new();
        for (slot, item) in npc.loot_items.iter().enumerate() {
            if let Some(item) = item {
                let info = self
                    .metadata
                    .items
                    .get(&item.item_entry)
                    .ok_or(Error::MissingItemMetadata(item.item_entry))?;
                items.push(protocol::LootItem {
                    slot: slot.try_into().map_err(|_| Error::Unrepresentable)?,
                    item_id: item.item_entry,
                    count: u32::from(item.count),
                    display_info_id: info.display_info_id,
                    random_property_id: 0,
                    slot_type: protocol::slot_type::ALLOW_LOOT,
                });
            }
        }
        Ok(SessionEvent::LootResponse {
            guid: npc_guid(world, id)?,
            loot_type: protocol::loot_type::CORPSE,
            gold: npc.loot_money,
            items,
        })
    }
    fn query_world(&self, world: &World) -> Result<(), Error> {
        self.adapter.check_world(world)?;
        if self.previous != *world.state() {
            return Err(Error::StaleProjection);
        }
        Ok(())
    }
    fn giver_entry(
        &self,
        world: &World,
        giver: NpcHandle,
        require_range: bool,
    ) -> Result<u32, Error> {
        self.query_world(world)?;
        let npc = world
            .npc(giver.id.spawn)
            .filter(|n| n.id == giver.id && n.health > 0)
            .ok_or(Error::UnknownTarget)?;
        let spawn = world
            .data()
            .spawn(npc.id.spawn)
            .ok_or(Error::UnknownTarget)?;
        let p = world
            .player(self.adapter.selected)
            .ok_or(Error::UnknownPlayer)?;
        if p.health == 0 {
            return Err(Error::UnavailableQuest);
        }
        if require_range {
            let dist: u64 = p
                .position
                .xyz
                .into_iter()
                .zip(spawn.position_milliunits)
                .map(|(a, b)| {
                    let d = i64::from(a) - i64::from(b);
                    (d * d) as u64
                })
                .sum();
            if p.position.map != spawn.map_id
                || dist > u64::from(world.rules().interact_range).pow(2)
            {
                return Err(Error::OutOfRange);
            }
        }
        Ok(spawn.creature_entry)
    }
    fn relation(&self, world: &World, entry: u32, quest: u32, kind: QuestRelationKind) -> bool {
        world
            .data()
            .export()
            .quest_relations
            .iter()
            .any(|r| r.creature_entry == entry && r.quest_entry == quest && r.kind == kind)
    }
    fn quest_icon(&self, world: &World, entry: u32, quest: u32) -> u32 {
        use protocol::dialog_status as d;
        let p = world.player(self.adapter.selected).unwrap();
        let q = world.data().quest(quest).unwrap();
        if let Some(progress) = p.quests.get(&quest) {
            if progress.turned_in
                || !self.relation(world, entry, quest, QuestRelationKind::Finishes)
            {
                return d::NONE;
            }
            if progress
                .counts
                .iter()
                .zip(&q.objectives)
                .all(|(n, o)| *n == o.count)
            {
                d::REWARD2
            } else {
                d::INCOMPLETE
            }
        } else if self.relation(world, entry, quest, QuestRelationKind::Starts) {
            if p.level >= q.min_level && p.level <= q.max_level {
                d::AVAILABLE
            } else {
                d::UNAVAILABLE
            }
        } else {
            d::NONE
        }
    }
    pub(crate) fn quest_status_events(&self, world: &World) -> Result<Vec<SessionEvent>, Error> {
        let mut events = Vec::new();
        for npc in &world.state().npcs {
            let entry = world
                .data()
                .spawn(npc.id.spawn)
                .ok_or(Error::UnknownTarget)?
                .creature_entry;
            if !world
                .data()
                .export()
                .quest_relations
                .iter()
                .any(|r| r.creature_entry == entry)
            {
                continue;
            }
            let status =
                if npc.health == 0 || world.player(self.adapter.selected).unwrap().health == 0 {
                    0
                } else {
                    world
                        .data()
                        .export()
                        .quest_relations
                        .iter()
                        .filter(|r| r.creature_entry == entry)
                        .map(|r| self.quest_icon(world, entry, r.quest_entry))
                        .max()
                        .unwrap_or(0)
                };
            events.push(SessionEvent::QuestGiverStatus {
                npc: npc_guid(world, npc.id)?,
                status,
            });
        }
        Ok(events)
    }
    /// Status may be queried for a visible NPC without interaction distance. Panels cannot.
    pub fn quest_status(&self, world: &World, giver: NpcHandle) -> Result<SessionEvent, Error> {
        let entry = self.giver_entry(world, giver, false)?;
        let status = world
            .data()
            .export()
            .quest_relations
            .iter()
            .filter(|r| r.creature_entry == entry)
            .map(|r| self.quest_icon(world, entry, r.quest_entry))
            .max()
            .unwrap_or(0);
        Ok(SessionEvent::QuestGiverStatus {
            npc: npc_guid(world, giver.id)?,
            status,
        })
    }
    pub fn quest_greeting(&self, world: &World, giver: NpcHandle) -> Result<SessionEvent, Error> {
        let entry = self.giver_entry(world, giver, true)?;
        let mut quests = BTreeMap::new();
        for r in &world.data().export().quest_relations {
            if r.creature_entry != entry {
                continue;
            }
            let icon = self.quest_icon(world, entry, r.quest_entry);
            if icon == protocol::dialog_status::NONE || icon == protocol::dialog_status::UNAVAILABLE
            {
                continue;
            }
            let template = &self.metadata.quests[&r.quest_entry].template;
            quests.insert(
                r.quest_entry,
                protocol::QuestListEntry {
                    quest_id: r.quest_entry,
                    icon,
                    level: template.level,
                    title: plain_ui_text(&template.title),
                },
            );
        }
        Ok(SessionEvent::QuestGreeting(protocol::QuestGiverList {
            npc: npc_guid(world, giver.id)?,
            greeting: plain_ui_text(
                self.metadata
                    .greetings
                    .get(&entry)
                    .ok_or(Error::MissingQuestGreeting(entry))?,
            ),
            emote_delay: 0,
            emote: 0,
            quests: quests.into_values().collect(),
        }))
    }
    pub fn quest_template(&self, quest: u32) -> Result<SessionEvent, Error> {
        let mut t = self
            .metadata
            .quests
            .get(&quest)
            .ok_or(Error::MissingQuestMetadata(quest))?
            .template
            .clone();
        t.title = plain_ui_text(&t.title);
        t.details = plain_ui_text(&t.details);
        t.objectives_text = plain_ui_text(&t.objectives_text);
        t.end_text = plain_ui_text(&t.end_text);
        for o in &mut t.objectives {
            o.text = plain_ui_text(&o.text);
        }
        Ok(SessionEvent::QuestTemplate(Box::new(t)))
    }
    fn reward_items(
        &self,
        world: &World,
        quest: u32,
    ) -> Result<Vec<protocol::QuestRewardItem>, Error> {
        world
            .data()
            .quest(quest)
            .ok_or(Error::UnavailableQuest)?
            .reward_items
            .iter()
            .map(|i| {
                Ok(protocol::QuestRewardItem {
                    item_id: i.item_entry,
                    count: u32::from(i.count),
                    display_id: self
                        .metadata
                        .items
                        .get(&i.item_entry)
                        .ok_or(Error::MissingItemMetadata(i.item_entry))?
                        .display_info_id,
                })
            })
            .collect()
    }
    pub fn quest_detail(
        &self,
        world: &World,
        quest: u32,
        giver: NpcHandle,
    ) -> Result<SessionEvent, Error> {
        let entry = self.giver_entry(world, giver, true)?;
        if !self.relation(world, entry, quest, QuestRelationKind::Starts)
            || self.quest_icon(world, entry, quest) != protocol::dialog_status::AVAILABLE
        {
            return Err(Error::UnavailableQuest);
        }
        let t = &self
            .metadata
            .quests
            .get(&quest)
            .ok_or(Error::MissingQuestMetadata(quest))?
            .template;
        Ok(SessionEvent::QuestDetail(protocol::QuestDetails {
            npc: npc_guid(world, giver.id)?,
            quest_id: quest,
            title: plain_ui_text(&t.title),
            details: plain_ui_text(&t.details),
            objectives: plain_ui_text(&t.objectives_text),
            auto_finish: 0,
            choices: Vec::new(),
            rewards: self.reward_items(world, quest)?,
            money: t.money,
            reward_spell: 0,
        }))
    }
    pub fn quest_progress(
        &self,
        world: &World,
        quest: u32,
        giver: NpcHandle,
    ) -> Result<SessionEvent, Error> {
        let entry = self.giver_entry(world, giver, true)?;
        let icon = self.quest_icon_checked(world, entry, quest)?;
        let q = &self.metadata.quests[&quest];
        Ok(SessionEvent::QuestProgress(protocol::QuestRequestItems {
            npc: npc_guid(world, giver.id)?,
            quest_id: quest,
            title: plain_ui_text(&q.template.title),
            request_text: plain_ui_text(&q.request_text),
            emote: 0,
            close_on_cancel: 0,
            required_money: 0,
            required_items: Vec::new(),
            is_complete: icon == protocol::dialog_status::REWARD2,
        }))
    }
    fn quest_icon_checked(&self, world: &World, entry: u32, quest: u32) -> Result<u32, Error> {
        if !self.metadata.quests.contains_key(&quest)
            || !self.relation(world, entry, quest, QuestRelationKind::Finishes)
        {
            return Err(Error::UnavailableQuest);
        }
        let icon = self.quest_icon(world, entry, quest);
        if icon != protocol::dialog_status::INCOMPLETE && icon != protocol::dialog_status::REWARD2 {
            return Err(Error::UnavailableQuest);
        }
        Ok(icon)
    }
    /// Read-only reward confirmation panel. Only an accepted TurnInQuest command awards it.
    pub fn quest_offer(
        &self,
        world: &World,
        quest: u32,
        giver: NpcHandle,
    ) -> Result<SessionEvent, Error> {
        let entry = self.giver_entry(world, giver, true)?;
        if self.quest_icon_checked(world, entry, quest)? != protocol::dialog_status::REWARD2 {
            return Err(Error::UnavailableQuest);
        }
        let q = &self.metadata.quests[&quest];
        Ok(SessionEvent::QuestOffer(protocol::QuestOfferReward {
            npc: npc_guid(world, giver.id)?,
            quest_id: quest,
            title: plain_ui_text(&q.template.title),
            offer_text: plain_ui_text(&q.offer_text),
            auto_finish: 0,
            choices: Vec::new(),
            rewards: self.reward_items(world, quest)?,
            money: q.template.money,
            quest_flags: q.template.flags,
            reward_spell: 0,
        }))
    }
}
