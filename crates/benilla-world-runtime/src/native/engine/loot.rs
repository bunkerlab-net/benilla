// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS LootMgr/LootHandler/Creature/Player.cpp, 8ec338a1; upstream
// GPL-2.0-or-later, Copyright CMaNGOS contributors, without warranty.
use super::*;
use benilla_sim::{inventory as inv, quest_lifecycle as q};
impl<A: Authority> Frame<'_, A> {
    pub(super) fn prepare_body_loot(&mut self, id: NpcId) -> Result<(), NativeError> {
        let Some(g) = self
            .profile
            .quest_progression
            .as_ref()
            .and_then(|g| g.loot.as_ref())
        else {
            return Ok(());
        };
        let npc = self
            .state
            .npcs
            .get(&id.spawn)
            .filter(|n| n.id == id)
            .ok_or(NativeError::InvalidState)?;
        if npc.health != 0
            || npc.original_loot_recipient != Some(self.state.player)
            || npc.corpse_loot.is_some()
        {
            return Err(NativeError::InvalidState);
        }
        let row = &g.creatures[&self
            .profile
            .npc(id.spawn)
            .ok_or(NativeError::InvalidState)?
            .entry];
        if row.no_loot {
            return Ok(());
        }
        let first_counter = self.state.rng_counter;
        let start = self.draws.len();
        let generated = g.catalog.generate(
            row.loot_id,
            row.minimum_money,
            row.maximum_money,
            &g.rules,
            &mut super::super::loot::RandomHost {
                state: self.state,
                corpse: id,
                draws: &mut self.draws,
            },
        )?;
        let looted = generated.items.is_empty() && generated.money == 0 && !generated.fake;
        self.state.npcs.get_mut(&id.spawn).unwrap().corpse_loot = Some(CorpseLoot {
            owner: self.state.player,
            corpse: id,
            death_at_ms: self.state.now_ms,
            first_counter,
            end_counter: self.state.rng_counter,
            generated,
            collected: Default::default(),
            money_taken: false,
            opened: false,
            looted,
            draws: self.draws[start..].to_vec(),
        });
        // PrepareBodyLootState's early Reduce is overwritten by JUST_DIED assigning
        // the full deadline later in this tick. Preserve LOOTED so later Release is a no-op.
        Ok(())
    }
    fn loot_exists(&self, id: NpcId) -> bool {
        self.state.npcs.get(&id.spawn).is_some_and(|n| {
            n.id == id
                && n.ai.phase() == ai::Phase::Corpse
                && n.original_loot_recipient == Some(self.state.player)
                && n.corpse_loot
                    .as_ref()
                    .is_some_and(|l| l.owner == self.state.player && l.corpse == id)
        })
    }
    fn loot_current(&self, id: NpcId) -> bool {
        self.loot_exists(id)
            && self
                .state
                .quest_progression
                .as_ref()
                .is_some_and(|q| q.loot_target == Some(id))
    }
    pub(super) fn open_loot(&mut self, id: NpcId) -> Result<Option<Rejection>, NativeError> {
        if self
            .profile
            .quest_progression
            .as_ref()
            .and_then(|g| g.loot.as_ref())
            .is_none()
        {
            return Err(NativeError::UnsupportedMechanic("native loot host"));
        }
        if self.state.health == 0 {
            return Ok(Some(Rejection::Dead));
        }
        let Some(npc) = self
            .state
            .npcs
            .get(&id.spawn)
            .filter(|n| n.id == id && n.ai.phase() != ai::Phase::Despawned)
        else {
            return Ok(Some(Rejection::UnknownTarget));
        };
        let p = npc.effective_profile(self.profile)?;
        let Facts::LootInteraction {
            player_in_world,
            player_stunned,
        } = self.ask(
            Some(id),
            Query::LootInteraction {
                witness: self.interaction_witness()?,
                player_position: self.state.position,
                npc_position: npc.ai.position(),
            },
        )?
        else {
            return Err(NativeError::AuthorityBinding);
        };
        if !player_in_world {
            return Ok(Some(Rejection::UnknownTarget));
        }
        if !self.state.combat.as_ref().is_some_and(|c| c.standing) {
            return Ok(Some(Rejection::Loot {
                corpse: id,
                reason: 8,
            }));
        }
        if player_stunned {
            return Ok(Some(Rejection::Loot {
                corpse: id,
                reason: 9,
            }));
        }
        let npc = &self.state.npcs[&id.spawn];
        if !ai::loot_interaction_distance(
            self.state.position,
            npc.ai.position(),
            self.profile.player.combat_reach,
            p.ai.combat_reach,
        )? {
            return Ok(Some(Rejection::Loot {
                corpse: id,
                reason: 4,
            }));
        }
        let exhausted_nonempty = self.loot_exists(id)
            && self.state.npcs[&id.spawn]
                .corpse_loot
                .as_ref()
                .is_some_and(|loot| {
                    !loot.generated.items.is_empty()
                        && loot.empty()
                        && !(loot.generated.fake && !loot.opened)
                });
        // Loot::ShowContentTo checks CanLoot only when the original item list is
        // nonempty. SendReleaseFor clears the player's loot state without opening
        // or inspecting, even after all items have already been collected.
        if exhausted_nonempty {
            self.clear_loot_target(id, true);
        } else if self.loot_exists(id) {
            let npc = self.state.npcs.get_mut(&id.spawn).unwrap();
            let loot = npc.corpse_loot.as_mut().unwrap();
            loot.opened = true;
            let row = &self
                .profile
                .quest_progression
                .as_ref()
                .unwrap()
                .loot
                .as_ref()
                .unwrap()
                .creatures[&p.entry];
            let call = ai::CorpseTimerCall {
                at_ms: self.state.now_ms,
                delay_ms: row.corpse_acceleration_decay_ms,
                inspecting: true,
            };
            npc.ai.adjust_loot_corpse_timer(call)?;
            if let Some(owner) = &npc.source_generation {
                npc.source_generation = Some(owner.record_corpse_timer_call(call)?);
            }
            let window = LootWindow {
                corpse: id,
                items: loot.items().cloned().collect(),
                money: loot.money(),
            };
            self.state.quest_progression.as_mut().unwrap().loot_target = Some(id);
            self.events.push(Event::Loot(LootEvent::Opened(window)));
        }
        // Source interrupts after ShowContentTo, including the no-loot-object path.
        let ids = self
            .state
            .casts
            .iter()
            .filter(|(_, c)| matches!(c.execution.state(), spell::CastState::Casting { .. }))
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for cast in ids {
            self.interrupt_scheduled_cast(cast, spell::CastFailure::Interrupted)?;
        }
        Ok(None)
    }
    /// Source SendReleaseForAll emits a packet; Release itself first unregisters
    /// the requesting player, so its subsequent broadcast does not include that player.
    fn clear_loot_target(&mut self, id: NpcId, send_response: bool) {
        if let Some(q) = &mut self.state.quest_progression {
            q.loot_target = None;
        }
        self.events.push(Event::Loot(LootEvent::Released {
            corpse: id,
            send_response,
        }));
    }
    pub(super) fn release_loot(&mut self, id: NpcId) -> Result<Option<Rejection>, NativeError> {
        // HandleLootReleaseOpcode resolves its explicit GUID independently of the
        // current player loot GUID, including a queued release after item-last.
        if !self.loot_exists(id) {
            return Ok(None);
        }
        let npc = self.state.npcs.get_mut(&id.spawn).unwrap();
        let loot = npc.corpse_loot.as_mut().unwrap();
        if (loot.generated.fake || loot.empty()) && !loot.looted {
            loot.looted = true;
            let entry = self
                .profile
                .npc(id.spawn)
                .ok_or(NativeError::InvalidState)?
                .entry;
            let row = &self
                .profile
                .quest_progression
                .as_ref()
                .unwrap()
                .loot
                .as_ref()
                .unwrap()
                .creatures[&entry];
            let call = ai::CorpseTimerCall {
                at_ms: self.state.now_ms,
                delay_ms: row.corpse_acceleration_decay_ms,
                inspecting: false,
            };
            npc.ai.adjust_loot_corpse_timer(call)?;
            if let Some(owner) = &npc.source_generation {
                npc.source_generation = Some(owner.record_corpse_timer_call(call)?);
            }
        }
        if self
            .state
            .quest_progression
            .as_ref()
            .is_some_and(|q| q.loot_target.is_some())
        {
            self.clear_loot_target(id, false);
        }
        Ok(None)
    }
    pub(super) fn collect_item(
        &mut self,
        id: NpcId,
        slot: u8,
    ) -> Result<Option<Rejection>, NativeError> {
        if !self.loot_current(id) {
            return Ok(None);
        }
        let corpse = self.state.npcs[&id.spawn].corpse_loot.as_ref().unwrap();
        let Some(item) = corpse.generated.items.get(slot as usize).cloned() else {
            self.events.push(Event::Loot(LootEvent::StorageFailed {
                corpse: id,
                item: 0,
                result: inv::InventoryResult::ItemNotFound,
            }));
            return self.release_loot(id);
        };
        if corpse.collected.contains(&slot) {
            return self.release_loot(id);
        }
        self.sync_quest_progression()?;
        let g = self.profile.quest_progression.as_ref().unwrap();
        let catalog = &g.loot.as_ref().unwrap().inventory;
        let current = self.state.quest_progression.as_ref().unwrap();
        let mut inventory = current.inventory.clone().ok_or(NativeError::InvalidState)?;
        let plan = inventory.can_store_corpse_item(catalog, item.item, item.count.into())?;
        if plan.result != inv::InventoryResult::Ok {
            self.events.push(Event::Loot(LootEvent::StorageFailed {
                corpse: id,
                item: item.item,
                result: plan.result,
            }));
            return Ok(None);
        }
        let mut next = current.clone();
        let mut callbacks = vec![];
        let combat = self
            .state
            .combat
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let stored = inventory.store_corpse_item(
            catalog,
            item.item,
            item.count.into(),
            |_, entry, count| {
                let (updated, events) = next
                    .reduce(g, combat, q::Command::ItemAdded { item: entry, count })
                    .map_err(|_| inv::Error::Callback("pre-store item quest callback"))?;
                next = updated;
                callbacks = events;
                Ok(())
            },
        )?;
        next.inventory = Some(inventory);
        super::super::loot::update_inventory_receipts(
            &mut next,
            crate::digest(b"benilla-native-corpse-item-v2\0", &(id, slot, &stored)),
        )?;
        self.commit_quest_progression(next)?;
        self.events
            .extend(callbacks.into_iter().map(Event::QuestProgression));
        self.events.push(Event::InventoryStored(Box::new(stored)));
        let corpse = self
            .state
            .npcs
            .get_mut(&id.spawn)
            .unwrap()
            .corpse_loot
            .as_mut()
            .unwrap();
        corpse.collected.insert(slot);
        let empty = corpse.empty();
        self.events
            .push(Event::Loot(LootEvent::ItemRemoved { corpse: id, slot }));
        self.events
            .push(Event::ItemAcquired(ItemAcquisition::CorpseLoot));
        if empty {
            self.clear_loot_target(id, true);
        }
        Ok(None)
    }
    pub(super) fn collect_money(&mut self, id: NpcId) -> Result<Option<Rejection>, NativeError> {
        if !self.loot_current(id) {
            return Ok(None);
        }
        let amount = self.state.npcs[&id.spawn]
            .corpse_loot
            .as_ref()
            .unwrap()
            .money();
        self.events
            .push(Event::Loot(LootEvent::MoneyRemoved { corpse: id }));
        self.sync_quest_progression()?;
        let mut current = self
            .state
            .quest_progression
            .clone()
            .ok_or(NativeError::InvalidState)?;
        current.money = current.money.saturating_add(amount).min(q::MAX_MONEY);
        let (next, events) = current.reduce(
            self.profile.quest_progression.as_ref().unwrap(),
            self.state.combat.as_ref().unwrap(),
            q::Command::MoneyChanged,
        )?;
        self.commit_quest_progression(next)?;
        self.events
            .extend(events.into_iter().map(Event::QuestProgression));
        let corpse = self
            .state
            .npcs
            .get_mut(&id.spawn)
            .unwrap()
            .corpse_loot
            .as_mut()
            .unwrap();
        corpse.money_taken = true;
        let empty = corpse.empty();
        if empty {
            self.release_loot(id)?;
        }
        Ok(None)
    }
}
