// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, Player::StoreNewItem, Item::Create and Bag::Create.
//! Corpse acquisition is all-or-nothing and never performs admin self-unbinding.
use super::*;

pub const CORPSE_POLICY_ID: &str = "cmangos-8ec338a1-corpse-carried-items-v1";

impl Catalog {
    /// Only carried creation is admitted; on-use spells and unused-slot metadata
    /// do not execute in StoreNewItem. Quest rewards retain ordinary_loot_ready.
    pub fn corpse_carried_ready(&self, entry: u32) -> Result<&Item, Error> {
        let item = self.grant_ready(entry)?;
        let bag = item.class == 1
            && item.inventory_type == 18
            && (1..=36).contains(&item.container_slots)
            && item.stackable == 1;
        if item.class == 11
            || item.inventory_type == 27
            || item.bag_family != 0
            || item.start_quest != 0
            || item.item_set != 0
            || item.lock_id != 0
            || (!bag && (item.class == 1 || item.inventory_type == 18 || item.container_slots != 0))
            || item.spells.iter().any(|s| s.id != 0 && s.trigger != 0)
        {
            return Err(Error::Unsupported("corpse_carried_item_dependency"));
        }
        Ok(item)
    }
}
impl State {
    /// The same source backpack/unique plan, guarded only for corpse acquisition.
    pub fn can_store_corpse_item(
        &self,
        catalog: &Catalog,
        item: u32,
        count: u32,
    ) -> Result<Plan, Error> {
        if catalog.rows.contains_key(&item) {
            catalog.corpse_carried_ready(item)?;
        }
        self.can_store_plan(catalog, item, count, &mut Vec::new())
    }
    /// Create before ItemAddedQuestCheck, then store, retaining source binding.
    /// The callback must mutate only the caller's isolated transaction host.
    pub fn store_corpse_item(
        &mut self,
        catalog: &Catalog,
        item: u32,
        count: u32,
        before_store: impl FnOnce(&State, u32, u32) -> Result<(), Error>,
    ) -> Result<Stored, Error> {
        let plan = self.can_store_corpse_item(catalog, item, count)?;
        if plan.result != InventoryResult::Ok {
            return Err(Error::Rejected(plan));
        }
        self.store_plan(catalog, item, count, plan, before_store)
    }
}
