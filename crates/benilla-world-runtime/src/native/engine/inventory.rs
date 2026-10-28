// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Player::SwapItem, Project contributors; upstream GPL-2.0-or-later.
use super::*;
use benilla_sim::inventory as inv;

impl<A: Authority> Frame<'_, A> {
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
