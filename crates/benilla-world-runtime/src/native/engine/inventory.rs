// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Player::SwapItem, Project contributors; upstream GPL-2.0-or-later.
use super::*;
use benilla_sim::inventory as inv;

impl<A: Authority> Frame<'_, A> {
    pub(super) fn equipment_move(
        &mut self,
        request: &EquipmentMoveRequest,
    ) -> Result<Option<Rejection>, NativeError> {
        use quest_progression::equipment_movement as movement;
        self.profile
            .interactions
            .as_ref()
            .ok_or(NativeError::UnsupportedMechanic(
                "armor source interaction owner",
            ))?
            .require_armor_movement(self.profile)?;
        let g = self
            .profile
            .quest_progression
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let q = self
            .state
            .quest_progression
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let inventory = q.inventory.as_ref().ok_or(NativeError::InvalidState)?;
        let custody = movement::placements(g, &q.derived_calls)?;
        let Some((&instance, _)) = custody.iter().find(|(_, slot)| **slot == request.source) else {
            let occupied = inventory.slots.contains_key(&request.source)
                || self
                    .state
                    .combat
                    .as_ref()
                    .is_some_and(|c| c.equipment.items().any(|i| i.slot == request.source));
            return if occupied {
                Err(NativeError::UnsupportedMechanic(
                    "nonregistry equipment movement",
                ))
            } else {
                Ok(None)
            };
        };
        if request.source == request.destination {
            return Ok(None);
        }
        let combat = self
            .state
            .combat
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let origin = g
            .loot
            .as_ref()
            .ok_or(NativeError::InvalidState)?
            .equipment_origin(instance)
            .ok_or(NativeError::InvalidState)?;
        let equips = request.destination == origin.genesis_slot;
        let generic_slot = if equips && self.state.player_alive() && !combat.in_combat {
            player_melee::casting::GenericSlot::capture(self.state)?
        } else {
            None
        };
        let reason = if !self.state.player_alive() {
            Some(38)
        } else if combat.in_combat {
            Some(60)
        } else if equips
            && generic_slot
                .as_ref()
                .map(|s| s.blocks_pre_unit(self.profile, self.state, self.state.now_ms))
                .transpose()?
                .unwrap_or(false)
        {
            Some(39)
        } else {
            None
        };
        if let Some(reason) = reason {
            self.events.push(Event::EquipmentMoveRefused {
                request: *request,
                instance,
                reason,
            });
            return Ok(Some(Rejection::Inventory {
                reason,
                source: request.source,
                destination: request.destination,
            }));
        }
        let receipt = EquipmentMoveReceipt::build(
            g,
            *request,
            instance,
            inventory.clone(),
            q.inventory_receipts.clone(),
            movement::MoveContext {
                world: self.state.world,
                player: self.state.player,
                profile: self.profile.digest(),
                sequence: self.state.next_sequence,
                captured_ms: self.state.now_ms,
                in_combat: combat.in_combat,
                generic_slot,
            },
            &custody,
        )?;
        let (next, after) = q.armor_membership(
            g,
            combat,
            receipt.clone(),
            self.state.health,
            self.state.resources.caster.mana,
        )?;
        self.state.combat = Some(after);
        self.commit_quest_progression(next)?;
        self.events.push(Event::EquipmentMoved(Box::new(receipt)));
        Ok(None)
    }
    pub(super) fn inventory_autostore(
        &mut self,
        request: &InventoryAutoStoreRequest,
    ) -> Result<Option<Rejection>, NativeError> {
        let genesis = self
            .profile
            .quest_progression
            .as_ref()
            .ok_or(NativeError::UnsupportedMechanic("native inventory host"))?;
        let catalog = &genesis
            .loot
            .as_ref()
            .ok_or(NativeError::UnsupportedMechanic("native inventory catalog"))?
            .inventory;
        let mut next = self
            .state
            .quest_progression
            .clone()
            .ok_or(NativeError::InvalidState)?;
        quest_progression::equipment_movement::reject_registered_slots(
            genesis,
            next.inventory.as_ref().ok_or(NativeError::InvalidState)?,
            &[request.source],
        )?;
        let receipt = next
            .inventory
            .as_mut()
            .ok_or(NativeError::InvalidState)?
            .autostore_backpack(catalog, request.source)?;
        if receipt.before != receipt.after {
            super::super::loot::update_inventory_receipts(
                &mut next,
                crate::digest(
                    b"benilla-native-backpack-autostore-v1\0",
                    &(
                        self.state.world,
                        self.state.player,
                        self.state.tick,
                        self.state.next_sequence,
                        request,
                        &receipt,
                    ),
                ),
            )?;
            self.state.quest_progression = Some(next);
        }
        self.events
            .push(Event::InventoryAutoStored(Box::new(receipt)));
        Ok(None)
    }
    pub(super) fn inventory_destroy(
        &mut self,
        request: &InventoryDestroyRequest,
    ) -> Result<Option<Rejection>, NativeError> {
        let genesis = self
            .profile
            .quest_progression
            .as_ref()
            .ok_or(NativeError::UnsupportedMechanic("native inventory host"))?;
        let catalog = &genesis
            .loot
            .as_ref()
            .ok_or(NativeError::UnsupportedMechanic("native inventory catalog"))?
            .inventory;
        let current = self
            .state
            .quest_progression
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        quest_progression::equipment_movement::reject_registered_slots(
            genesis,
            current
                .inventory
                .as_ref()
                .ok_or(NativeError::InvalidState)?,
            &[request.slot],
        )?;
        let receipt = current
            .inventory
            .as_ref()
            .ok_or(NativeError::InvalidState)?
            .plan_destroy_backpack(catalog, request.slot, request.count)?;
        if receipt.result != inv::InventoryResult::Ok {
            let reason = receipt.result.protocol();
            self.events.push(Event::QuestProgression(
                QuestProgressionEvent::InventoryDestroyed(Box::new(receipt)),
            ));
            return Ok(Some(Rejection::Inventory {
                reason,
                source: request.slot,
                destination: request.slot,
            }));
        }
        // Source quest counters observe pre-removal aggregate counts. The host then
        // consumes this exact selected instance once, never first-stack-by-entry.
        let combat = self
            .state
            .combat
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let (next, events) = current.destroy_selected(genesis, combat, receipt)?;
        self.commit_quest_progression(next)?;
        self.events
            .extend(events.into_iter().map(Event::QuestProgression));
        Ok(None)
    }
    pub(super) fn inventory_split(
        &mut self,
        request: &InventorySplitRequest,
    ) -> Result<Option<Rejection>, NativeError> {
        let genesis = self
            .profile
            .quest_progression
            .as_ref()
            .ok_or(NativeError::UnsupportedMechanic("native inventory host"))?;
        let catalog = &genesis
            .loot
            .as_ref()
            .ok_or(NativeError::UnsupportedMechanic("native inventory catalog"))?
            .inventory;
        let mut next = self
            .state
            .quest_progression
            .clone()
            .ok_or(NativeError::InvalidState)?;
        let inventory = next.inventory.as_mut().ok_or(NativeError::InvalidState)?;
        quest_progression::equipment_movement::reject_registered_slots(
            genesis,
            inventory,
            &[request.source, request.destination],
        )?;
        // SplitItem has no ordinary-backpack alive guard, unlike SwapItem.
        let receipt = inventory.split_backpack(
            catalog,
            request.source,
            request.destination,
            request.count,
        )?;
        let rejection =
            (receipt.result != inv::InventoryResult::Ok).then_some(Rejection::Inventory {
                reason: receipt.result.protocol(),
                source: request.source,
                destination: request.destination,
            });
        if receipt.before != receipt.after {
            // Includes allocated-and-retired clones on refused destination storage.
            super::super::loot::update_inventory_receipts(
                &mut next,
                crate::digest(
                    b"benilla-native-backpack-split-v1\0",
                    &(
                        self.state.world,
                        self.state.player,
                        self.state.tick,
                        self.state.next_sequence,
                        request,
                        &receipt,
                    ),
                ),
            )?;
            self.state.quest_progression = Some(next);
        }
        self.events.push(Event::InventorySplit(Box::new(receipt)));
        Ok(rejection)
    }
    pub(super) fn inventory_move(
        &mut self,
        request: &InventoryMoveRequest,
    ) -> Result<Option<Rejection>, NativeError> {
        let genesis = self
            .profile
            .quest_progression
            .as_ref()
            .ok_or(NativeError::UnsupportedMechanic("native inventory host"))?;
        let catalog = &genesis
            .loot
            .as_ref()
            .ok_or(NativeError::UnsupportedMechanic("native inventory catalog"))?
            .inventory;
        let mut next = self
            .state
            .quest_progression
            .clone()
            .ok_or(NativeError::InvalidState)?;
        let inventory = next.inventory.as_mut().ok_or(NativeError::InvalidState)?;
        quest_progression::equipment_movement::reject_registered_slots(
            genesis,
            inventory,
            &[request.source, request.destination],
        )?;
        let receipt = inventory.swap_backpack(
            catalog,
            request.source,
            request.destination,
            self.state.player_alive(),
        )?;
        let rejection =
            (receipt.result != inv::InventoryResult::Ok).then_some(Rejection::Inventory {
                reason: receipt.result.protocol(),
                source: request.source,
                destination: request.destination,
            });
        if receipt.before != receipt.after {
            super::super::loot::update_inventory_receipts(
                &mut next,
                crate::digest(
                    b"benilla-native-backpack-movement-v1\0",
                    &(
                        self.state.world,
                        self.state.player,
                        self.state.tick,
                        self.state.next_sequence,
                        request,
                        &receipt,
                    ),
                ),
            )?;
            // Relocation does not acquire items, rebase resources or credit quest objectives.
            self.state.quest_progression = Some(next);
        }
        self.events.push(Event::InventoryMoved(Box::new(receipt)));
        Ok(rejection)
    }
}
