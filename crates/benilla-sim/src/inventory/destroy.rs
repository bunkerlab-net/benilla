// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
//! Selected ordinary backpack destruction plan. Quest callbacks must run before commit.
use super::*;
pub const NO_USER_DESTROY: u32 = 0x20;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum DestroyPolicy {
    CmangosBackpackDestroyV1,
    CmangosCastItemConsumeV1,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum DestroyOutcome {
    Rejected,
    Partial,
    Whole,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DestroyReceipt {
    pub policy: DestroyPolicy,
    pub slot: u8,
    pub requested: u8,
    pub removed: u32,
    pub result: InventoryResult,
    pub outcome: DestroyOutcome,
    pub before: State,
    pub after: State,
}
impl State {
    /// Does not mutate the inventory: ItemRemovedQuestCheck must observe `before` first.
    /// No unequip, equipped-bag traversal, item scripts/durations or wrapped items admitted.
    pub fn plan_destroy_backpack(
        &self,
        catalog: &Catalog,
        slot: u8,
        requested: u8,
    ) -> Result<DestroyReceipt, Error> {
        if !(BACKPACK_START..BACKPACK_END).contains(&slot) {
            return Err(Error::Unsupported("backpack_destroy_position"));
        }
        catalog.storage_dependencies_ready()?;
        self.validate(catalog)?;
        let mut receipt = DestroyReceipt {
            policy: DestroyPolicy::CmangosBackpackDestroyV1,
            slot,
            requested,
            removed: 0,
            result: InventoryResult::Ok,
            outcome: DestroyOutcome::Rejected,
            before: self.clone(),
            after: self.clone(),
        };
        if let Some(item) = self.slots.get(&slot) {
            let proto = catalog.grant_ready(item.item)?;
            // Protection is a prototype flag, NOT the instance's ordinary soulbound bit.
            if proto.flags & NO_USER_DESTROY != 0 {
                receipt.result = InventoryResult::CantDropSoulbound;
            } else {
                if item.kind.is_container() {
                    return Err(Error::Unsupported("backpack_destroy_container"));
                }
                receipt.removed = if requested == 0 {
                    item.count
                } else {
                    u32::from(requested).min(item.count)
                };
                if receipt.removed == item.count {
                    receipt.after.slots.remove(&slot);
                    receipt.outcome = DestroyOutcome::Whole;
                } else {
                    receipt.after.slots.get_mut(&slot).unwrap().count -= receipt.removed;
                    receipt.outcome = DestroyOutcome::Partial;
                }
            }
        } else {
            receipt.result = InventoryResult::ItemNotFound;
        }
        receipt.after.validate(catalog)?;
        Ok(receipt)
    }
    /// Spell.cpp:4314-4375. The bounded expendable branch leaves the surviving
    /// stack's charges unchanged and does not apply NO_USER_DESTROY.
    pub fn plan_take_cast_item(
        &self,
        catalog: &Catalog,
        slot: u8,
    ) -> Result<DestroyReceipt, Error> {
        if !(BACKPACK_START..BACKPACK_END).contains(&slot) {
            return Err(Error::Unsupported("cast_item_position"));
        }
        catalog.storage_dependencies_ready()?;
        self.validate(catalog)?;
        let item = self
            .slots
            .get(&slot)
            .ok_or(Error::Invalid("cast_item_missing"))?;
        let proto = catalog.grant_ready(item.item)?;
        if item.kind.is_container()
            || proto.class != 0
            || proto.spells[0].charges != -1
            || !matches!(proto.spells[0].id, 430 | 433)
            || proto.spells[0].trigger != 0
            || proto.spells[1..].iter().any(|s| s.id != 0)
        {
            return Err(Error::Unsupported("cast_item_expendable_source"));
        }
        let mut after = self.clone();
        let outcome = if item.count == 1 {
            after.slots.remove(&slot);
            DestroyOutcome::Whole
        } else {
            after.slots.get_mut(&slot).unwrap().count -= 1;
            DestroyOutcome::Partial
        };
        after.validate(catalog)?;
        Ok(DestroyReceipt {
            policy: DestroyPolicy::CmangosCastItemConsumeV1,
            slot,
            requested: 1,
            removed: 1,
            result: InventoryResult::Ok,
            outcome,
            before: self.clone(),
            after,
        })
    }
}
impl DestroyReceipt {
    pub fn item(&self) -> Option<u32> {
        self.before.slots.get(&self.slot).map(|item| item.item)
    }
    pub fn validate(&self, catalog: &Catalog) -> Result<(), Error> {
        let expected = match self.policy {
            DestroyPolicy::CmangosBackpackDestroyV1 => {
                self.before
                    .plan_destroy_backpack(catalog, self.slot, self.requested)?
            }
            DestroyPolicy::CmangosCastItemConsumeV1 => {
                self.before.plan_take_cast_item(catalog, self.slot)?
            }
        };
        if expected != *self {
            return Err(Error::Invalid("backpack_destroy_receipt"));
        }
        Ok(())
    }
}
