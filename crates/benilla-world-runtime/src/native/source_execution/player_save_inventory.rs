// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;
use crate::native::source_execution::source_rows::{
    InventoryRow, ItemInstanceRow, RowDelete, RowInsert, RowOperation, RowUpdate,
};

// Player.h:604,610-617 and Item.h:192,176-184.
const BANK_BAG_END: usize = 69;
const BUYBACK: std::ops::Range<usize> = 69..81;
const ITEM_DYNFLAG_WRAPPED: u32 = 0x8;
const ITEM_LOOT_UNCHANGED: u8 = 2;
const ITEM_LOOT_CHANGED: u8 = 3;
const ITEM_LOOT_NEW: u8 = 4;
const ITEM_LOOT_REMOVED: u8 = 5;

impl SourcePlayer {
    // Player.cpp:15925-15995. Defer the queue clear at :15994 to transaction acknowledgement.
    pub(in super::super) fn save_inventory(
        &self,
        objects: &SourceObjects,
        items: &SourceItems,
        guid: u32,
        allow_removed_projection: bool,
        observe: &mut impl FnMut(SourcePlayerSaveObservation) -> Result<()>,
    ) -> Result<()> {
        for &handle in self.items[BUYBACK].iter().flatten() {
            require(
                items.get(handle)?.state == SourceItemState::New,
                "execution.player_save.Player_15937.buyback_owner",
            )?;
        }
        require(
            !self.enchant_durations_active,
            "execution.player_save.Player_15953.enchant_duration_owner",
        )?;
        if self.item_update_queue.is_empty() {
            return Ok(());
        }

        // Player.cpp:15964-15985 checks all occupied, non-removed entries before the first write.
        for &handle in self.item_update_queue.iter().flatten() {
            let item = items.get(handle).map_err(|_| {
                refuse("execution.player_save.Player_15984.invalid_item_queue_owner")
            })?;
            if item.state == SourceItemState::Removed {
                continue;
            }
            require(
                item.container.is_none(),
                "execution.player_save.Player_3822.inventory_bag_owner",
            )?;
            // Player.cpp:8252-8263 excludes buyback slots from GetItemByPos.
            let slot = usize::from(item.slot);
            require(
                (slot < BANK_BAG_END || KEYRING.contains(&slot))
                    && self.items[slot] == Some(handle),
                "execution.player_save.Player_15984.invalid_item_queue_owner",
            )?;
        }

        // Player.cpp:15988-15992 skips null entries without changing their neighbours' positions.
        let mut emit = |row| observe(SourcePlayerSaveObservation::Row(row));
        for &handle in self.item_update_queue.iter().flatten() {
            Self::save_item_to_inventory(
                objects,
                handle,
                items.get(handle)?,
                guid,
                allow_removed_projection,
                &mut emit,
            )?;
        }
        Ok(())
    }

    // Item.cpp:399-402,704-708; Player.cpp:15994. The projection validated every handle.
    pub(in super::super) fn acknowledge_inventory(&mut self, items: &mut SourceItems) {
        for &handle in self.item_update_queue.iter().flatten() {
            if let Some(item) = items.0.get_mut(&handle) {
                if matches!(item.state, SourceItemState::New | SourceItemState::Changed) {
                    // Nonempty loot changes refuse before publication. None, temporary and
                    // unchanged loot keep their state.
                    item.mark_unchanged();
                }
            }
        }
        self.item_update_queue.clear();
    }

    // Player.cpp:3819-3864 writes the placement before saving the Item instance.
    fn save_item_to_inventory(
        objects: &SourceObjects,
        handle: ObjectHandle,
        item: &SourceItem,
        guid: u32,
        allow_removed_projection: bool,
        emit: &mut impl FnMut(RowOperation) -> Result<()>,
    ) -> Result<()> {
        require(
            item.container.is_none(),
            "execution.player_save.Player_3822.inventory_bag_owner",
        )?;
        let item_guid = objects
            .get(handle)?
            .guid
            .ok_or_else(|| refuse("execution.player_save.Player_3836.item_guid_owner"))?
            .counter();
        match item.state {
            SourceItemState::New | SourceItemState::Changed => {
                let row = InventoryRow {
                    guid,
                    bag_guid: 0, // Player.cpp:3822, the null-container branch.
                    slot: item.slot,
                    item_guid,
                    item_template: item.entry,
                };
                // Player.cpp:3841-3849 updates even when the placement is unchanged.
                emit(if item.state == SourceItemState::New {
                    RowOperation::Insert(RowInsert::Inventory(row))
                } else {
                    RowOperation::Update(RowUpdate::Inventory(row))
                })?;
            }
            SourceItemState::Removed => {
                emit(RowOperation::Delete(RowDelete::Inventory { item_guid }))?
            }
            SourceItemState::Unchanged => {}
        }
        item.save_instance(item_guid, allow_removed_projection, emit)
    }
}

impl SourceItem {
    // Item.cpp:250-403. A successful publish will acknowledge Item state at :402.
    fn save_instance(
        &self,
        guid: u32,
        allow_removed_projection: bool,
        emit: &mut impl FnMut(RowOperation) -> Result<()>,
    ) -> Result<()> {
        let wrapped = self.flags & ITEM_DYNFLAG_WRAPPED != 0;
        match self.state {
            SourceItemState::New | SourceItemState::Changed => {
                // Item.cpp:284-310 binds these fields. The row keeps the text operands as words.
                let row = ItemInstanceRow {
                    owner_guid: self.owner.counter(),
                    entry: self.entry,
                    creator: self.creator.counter(),
                    gift_creator: self.gift_creator.counter(),
                    count: self.count,
                    duration: self.duration,
                    charges: self.charges,
                    flags: self.flags,
                    enchantments: self.enchantments,
                    random_property: self.random_property as i16,
                    durability: self.durability as u16,
                    text_id: self.text_id,
                    guid,
                };
                emit(if self.state == SourceItemState::New {
                    RowOperation::ReplaceItemInstance(row)
                } else {
                    RowOperation::Update(RowUpdate::ItemInstance(row))
                })?;
                require(
                    self.state != SourceItemState::Changed || !wrapped,
                    "execution.player_save.Item_317.gift_owner",
                )?;
            }
            SourceItemState::Removed => {
                require(
                    self.text_id == 0,
                    "execution.player_save.Item_332.item_text_owner",
                )?;
                emit(RowOperation::Delete(RowDelete::ItemInstance(guid)))?;
                require(!wrapped, "execution.player_save.Item_341.gift_owner")?;
                require(
                    !matches!(
                        self.loot_state,
                        ITEM_LOOT_UNCHANGED | ITEM_LOOT_CHANGED | ITEM_LOOT_REMOVED
                    ),
                    "execution.player_save.Item_347.item_loot_delete_owner",
                )?;
                return require(
                    allow_removed_projection,
                    "execution.player_save.Item_351.destroy_removed_owner",
                );
            }
            SourceItemState::Unchanged => return Ok(()),
        }
        require(
            !matches!(self.loot_state, ITEM_LOOT_CHANGED | ITEM_LOOT_REMOVED),
            "execution.player_save.Item_362.item_loot_delete_owner",
        )?;
        require(
            self.loot_state != ITEM_LOOT_NEW,
            "execution.player_save.Item_376.item_loot_insert_owner",
        )
    }

    // Item.cpp:704-708. The Player clears its queue separately.
    fn mark_unchanged(&mut self) {
        self.queue_pos = None;
        self.state = SourceItemState::Unchanged;
    }

    /// Item.cpp:684-769. The placement owner unlinks a removed Item before calling.
    /// A caller acknowledging `Unchanged` must clear the queue entry separately.
    pub fn set_state(
        handle: ObjectHandle,
        state: SourceItemState,
        player: &mut SourcePlayer,
        objects: &mut SourceObjects,
        items: &mut SourceItems,
        observe: &mut dyn FnMut(ItemLifetimeObservation),
    ) -> Result<()> {
        let item = items.get_mut(handle)?;
        if item.state == SourceItemState::New && state == SourceItemState::Removed {
            if let Some(pos) = item.queue_pos {
                require(
                    objects.get(player.object)?.guid == Some(item.owner),
                    "execution.item.Item_761.queue_owner",
                )?;
                if !player.item_update_queue_blocked {
                    let entry = player.item_update_queue.get_mut(pos).ok_or_else(|| {
                        refuse("execution.item.Item_768.queue_position_undefined")
                    })?;
                    *entry = None;
                    item.queue_pos = None;
                }
            }
            return items.destroy(objects, handle, observe);
        }
        if state == SourceItemState::Unchanged {
            item.mark_unchanged();
            return Ok(());
        }
        if item.state != SourceItemState::New {
            item.state = state;
        }
        if item.queue_pos.is_none() {
            require(
                objects.get(player.object)?.guid == Some(item.owner),
                "execution.item.Item_731.queue_owner",
            )?;
            if !player.item_update_queue_blocked {
                item.queue_pos = Some(player.item_update_queue.len());
                player.item_update_queue.push(Some(handle));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "player_save_inventory_tests.rs"]
pub(in super::super) mod tests;
