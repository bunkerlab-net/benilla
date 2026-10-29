// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Player::SwapItem, Project contributors; upstream GPL-2.0-or-later.
use super::*;
use benilla_sim::inventory as inv;
use quest_progression::equipment_movement::EquipmentMoveOperation;

// SwapItem checks death first; AutoEquip calls CanEquipItem before CanUseItem.
fn equipment_guard_error(
    operation: EquipmentMoveOperation,
    alive: bool,
    armor_combat: bool,
    casting: bool,
) -> Option<u8> {
    if operation == EquipmentMoveOperation::Swap && !alive {
        Some(38)
    } else if armor_combat {
        Some(60)
    } else if casting {
        Some(39)
    } else if !alive {
        Some(38)
    } else {
        None
    }
}

impl<A: Authority> Frame<'_, A> {
    pub(super) fn equipment_move(
        &mut self,
        request: &EquipmentMoveRequest,
    ) -> Result<Option<Rejection>, NativeError> {
        self.equipment_move_inner(request, EquipmentMoveOperation::Swap)
    }

    pub(super) fn equipment_auto_equip(
        &mut self,
        source: u8,
    ) -> Result<Option<Rejection>, NativeError> {
        use quest_progression::equipment_movement as movement;
        if !(inv::BACKPACK_START..inv::BACKPACK_END).contains(&source) {
            return Err(NativeError::UnsupportedMechanic(
                "auto-equip carried original source",
            ));
        }
        let g = self
            .profile
            .quest_progression
            .as_ref()
            .ok_or(NativeError::UnsupportedMechanic("native inventory host"))?;
        let inventory = self
            .state
            .quest_progression
            .as_ref()
            .and_then(|q| q.inventory.as_ref())
            .ok_or(NativeError::InvalidState)?;
        // HandleAutoEquipItemOpcode ignores an empty source before CanEquipItem.
        let Some(item) = inventory.slots.get(&source) else {
            return Ok(None);
        };
        let origin = g
            .loot
            .as_ref()
            .and_then(|l| l.equipment_origin(item.id))
            .ok_or(NativeError::UnsupportedMechanic(
                "auto-equip original equipment only",
            ))?;
        let destination = movement::auto_equip_slot(g, origin)?;
        let combat = self
            .state
            .combat
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        if combat.equipment.item(destination).is_some() || combat.equipment.item(16).is_some() {
            return Err(NativeError::UnsupportedMechanic(
                "auto-equip occupied slot or offhand dependency",
            ));
        }
        self.equipment_move_inner(
            &EquipmentMoveRequest {
                source,
                destination,
            },
            EquipmentMoveOperation::AutoEquip,
        )
    }

    fn equipment_move_inner(
        &mut self,
        request: &EquipmentMoveRequest,
        operation: EquipmentMoveOperation,
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
        let Some((&instance, _)) = custody
            .iter()
            .find(|(_, slot)| **slot == movement::EquipmentPlacement::Slot(request.source))
        else {
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
        let mainhand = origin.genesis_slot == 15;
        let equips = request.destination == origin.genesis_slot;
        let generic_slot = if equips
            && (!combat.in_combat || mainhand)
            && (self.state.player_alive() || operation == EquipmentMoveOperation::AutoEquip)
        {
            player_melee::casting::GenericSlot::capture(self.state)?
        } else {
            None
        };
        let casting = equips
            && generic_slot
                .as_ref()
                .map(|s| s.blocks_pre_unit(self.profile, self.state, self.state.now_ms))
                .transpose()?
                .unwrap_or(false);
        let reason = equipment_guard_error(
            operation,
            self.state.player_alive(),
            combat.in_combat && !mainhand,
            casting,
        );
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
        // Weapons skip the source armor combat guard. Preserve real cast/dead
        // refusals first; only a source-eligible combat switch reaches this host
        // limitation (the reset/GCD callback is not yet composed).
        if mainhand && combat.in_combat {
            return Err(NativeError::UnsupportedMechanic(
                "mainhand movement combat cooldown",
            ));
        }
        let receipt = EquipmentMoveReceipt::build(
            g,
            *request,
            instance,
            inventory.clone(),
            q.inventory_receipts.clone(),
            movement::MoveContext {
                operation,
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
        let registered = next
            .inventory
            .as_ref()
            .and_then(|inventory| inventory.slots.get(&request.source))
            .and_then(|item| genesis.loot.as_ref()?.equipment_origin(item.id))
            .is_some();
        if registered {
            self.profile
                .interactions
                .as_ref()
                .ok_or(NativeError::UnsupportedMechanic(
                    "armor source interaction owner",
                ))?
                .require_armor_movement(self.profile)?;
            let receipt = next
                .inventory
                .clone()
                .ok_or(NativeError::InvalidState)?
                .autostore_backpack(catalog, request.source)?;
            if receipt.before != receipt.after {
                let custody = quest_progression::equipment_movement::placements(
                    genesis,
                    &next.derived_calls,
                )?;
                let movement =
                    quest_progression::equipment_movement::EquipmentAutoStoreReceipt::build(
                        genesis,
                        receipt.clone(),
                        next.inventory_receipts.clone(),
                        self.state,
                        self.profile,
                        &custody,
                    )?;
                next = next.armor_autostore(
                    genesis,
                    self.state
                        .combat
                        .as_ref()
                        .ok_or(NativeError::InvalidState)?,
                    movement,
                    self.state.health,
                    self.state.resources.caster.mana,
                )?;
                self.commit_quest_progression(next)?;
            }
            self.events
                .push(Event::InventoryAutoStored(Box::new(receipt)));
            return Ok(None);
        }
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
        let registered = current
            .inventory
            .as_ref()
            .and_then(|inventory| inventory.slots.get(&request.slot))
            .and_then(|item| genesis.loot.as_ref()?.equipment_origin(item.id))
            .is_some();
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
        let (next, events) = if registered {
            self.profile
                .interactions
                .as_ref()
                .ok_or(NativeError::UnsupportedMechanic(
                    "armor source interaction owner",
                ))?
                .require_armor_movement(self.profile)?;
            let custody =
                quest_progression::equipment_movement::placements(genesis, &current.derived_calls)?;
            let destruction =
                quest_progression::equipment_movement::EquipmentDestroyReceipt::build(
                    genesis,
                    receipt,
                    current.inventory_receipts.clone(),
                    self.state,
                    self.profile,
                    &custody,
                )?;
            current.armor_destroy(
                genesis,
                combat,
                destruction,
                self.state.health,
                self.state.resources.caster.mana,
            )?
        } else {
            current.destroy_selected(genesis, combat, receipt)?
        };
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
        let registered = next
            .inventory
            .as_ref()
            .and_then(|i| i.slots.get(&request.source))
            .and_then(|item| genesis.loot.as_ref()?.equipment_origin(item.id));
        if registered.is_some() {
            self.profile
                .interactions
                .as_ref()
                .ok_or(NativeError::UnsupportedMechanic(
                    "armor source interaction owner",
                ))?
                .require_armor_movement(self.profile)?;
            let before = next.inventory.as_ref().ok_or(NativeError::InvalidState)?;
            if request.source != request.destination
                && before.slots.contains_key(&request.destination)
            {
                return Err(NativeError::UnsupportedMechanic(
                    "registered armor occupied destination",
                ));
            }
            let mut staged = before.clone();
            let receipt = staged.swap_backpack(
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
                let custody = quest_progression::equipment_movement::placements(
                    genesis,
                    &next.derived_calls,
                )?;
                let movement =
                    quest_progression::equipment_movement::EquipmentRelocationReceipt::build(
                        genesis,
                        receipt.clone(),
                        next.inventory_receipts.clone(),
                        self.state,
                        self.profile,
                        &custody,
                    )?;
                next = next.armor_relocation(
                    genesis,
                    self.state
                        .combat
                        .as_ref()
                        .ok_or(NativeError::InvalidState)?,
                    movement,
                    self.state.health,
                    self.state.resources.caster.mana,
                )?;
                self.commit_quest_progression(next)?;
            }
            self.events.push(Event::InventoryMoved(Box::new(receipt)));
            return Ok(rejection);
        }
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

#[cfg(test)]
mod guard_tests {
    use super::*;
    #[test]
    fn mainhand_auto_equip_skips_armor_combat_guard_not_cast_or_dead_guards() {
        for alive in [false, true] {
            for combat in [false, true] {
                for casting in [false, true] {
                    // The actual mainhand owner supplies armor_combat=false even
                    // in combat. The separate host cooldown hold follows these
                    // source refusals; it must not replace cast39 or dead38.
                    let source = equipment_guard_error(
                        EquipmentMoveOperation::AutoEquip,
                        alive,
                        false,
                        casting,
                    );
                    assert_eq!(
                        source,
                        if casting {
                            Some(39)
                        } else if !alive {
                            Some(38)
                        } else {
                            None
                        }
                    );
                    let host_hold = source.is_none() && combat;
                    assert_eq!(host_hold, alive && !casting && combat);
                }
            }
        }
    }
    #[test]
    fn auto_equip_retains_can_equip_before_can_use_order() {
        for alive in [false, true] {
            for combat in [false, true] {
                for casting in [false, true] {
                    let expected_auto = if combat {
                        Some(60)
                    } else if casting {
                        Some(39)
                    } else if !alive {
                        Some(38)
                    } else {
                        None
                    };
                    let expected_swap = if !alive {
                        Some(38)
                    } else if combat {
                        Some(60)
                    } else if casting {
                        Some(39)
                    } else {
                        None
                    };
                    assert_eq!(
                        equipment_guard_error(
                            EquipmentMoveOperation::AutoEquip,
                            alive,
                            combat,
                            casting
                        ),
                        expected_auto
                    );
                    assert_eq!(
                        equipment_guard_error(EquipmentMoveOperation::Swap, alive, combat, casting),
                        expected_swap
                    );
                }
            }
        }
    }
}
