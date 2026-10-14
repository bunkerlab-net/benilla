// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, Chat/Level3.cpp HandleAddItemCommand and Entities/Bag.cpp.
//! Positive admin acquisition, separate from ordinary all-or-nothing loot storage.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum InstanceKind {
    Item,
    /// A real, empty carried container. It does not add usable inventory slots.
    EmptyBag {
        slots: u32,
    },
}
impl InstanceKind {
    pub fn is_container(self) -> bool {
        matches!(self, Self::EmptyBag { .. })
    }
    pub(super) fn created(item: &Item) -> Self {
        if item.inventory_type == 18 {
            Self::EmptyBag {
                slots: item.container_slots,
            }
        } else {
            Self::Item
        }
    }
}
pub(super) fn creation_charges(item: &Item) -> [i32; 5] {
    // NewItemOrBag selects by InventoryType only; Bag::Create does not call Item::Create.
    if item.inventory_type == 18 {
        [0; 5]
    } else {
        item.spells.map(|s| s.charges)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum GrantPolicy {
    CmangosAddItemV1,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum GrantTarget {
    SelfPlayer,
    OtherPlayer,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantReceipt {
    pub policy: GrantPolicy,
    pub target: GrantTarget,
    pub item: u32,
    pub requested: u32,
    pub granted: u32,
    pub ungranted: u32,
    pub storage_result: InventoryResult,
    pub destinations: Vec<Destination>,
    pub stored: Option<Stored>,
    /// Zero outcomes also retain a recomputable capacity/unique witness.
    pub zero_before: Option<State>,
}
impl Catalog {
    fn storage_dependencies_ready(&self) -> Result<(), Error> {
        for evidence in [
            self.audit.item_creation_and_loot_scripts,
            self.audit.equipped_bags,
            self.audit.bank_and_keyring_items,
            self.audit.trade_or_temporary_item_loot,
            self.audit.additional_item_mutations,
        ] {
            if evidence != Evidence::Absent {
                return Err(Error::Unsupported("inventory_dependency_not_absent"));
            }
        }
        Ok(())
    }
    /// Creation and carried-lifecycle closure, independent of catalog visibility.
    /// Equip/use remain unimplemented; retained spells, sets and StartQuest do not run here.
    pub fn grant_ready(&self, entry: u32) -> Result<&Item, Error> {
        self.storage_dependencies_ready()?;
        let item = self.item(entry)?;
        if item.random_property != 0 {
            return Err(Error::Unsupported("item_random_property_creation"));
        }
        if item.script_id != 0 {
            return Err(Error::Unsupported("item_loot_script"));
        }
        if item.duration != 0 || item.area != 0 || item.map != 0 {
            return Err(Error::Unsupported("item_carried_lifecycle"));
        }
        if item.bag_family == 9 {
            return Err(Error::Unsupported("keyring_storage"));
        }
        if item.extra_flags != 0 || item.flags & !0x0000_0800 != 0 {
            return Err(Error::Unsupported("item_flags_dependency"));
        }
        if item.inventory_type == 18 && item.container_slots > 36 {
            return Err(Error::Invalid("bag_capacity"));
        }
        Ok(item)
    }
    /// Catalog presence never claims a working equipment or item-use path.
    pub fn equip_ready(&self, entry: u32) -> Result<&Item, Error> {
        self.item(entry)?;
        Err(Error::Unsupported("carried_item_equipment_not_implemented"))
    }
    pub fn use_ready(&self, entry: u32) -> Result<&Item, Error> {
        self.item(entry)?;
        Err(Error::Unsupported("carried_item_use_not_implemented"))
    }
    /// The existing loot/reward admission remains deliberately narrower than admin creation.
    pub fn ordinary_loot_ready(&self, entry: u32) -> Result<&Item, Error> {
        let item = self.grant_ready(entry)?;
        if matches!(item.class, 1 | 11)
            || matches!(item.inventory_type, 18 | 27)
            || item.bag_family != 0
            || item.container_slots != 0
            || item.start_quest != 0
            || item.item_set != 0
            || item.lock_id != 0
            || item.spells.iter().any(|s| {
                s.id != 0
                    || s.trigger != 0
                    || s.charges != 0
                    || s.ppm_rate_bits != 0
                    || s.cooldown != 0
                    || s.category != 0
                    || s.category_cooldown != 0
            })
        {
            return Err(Error::Unsupported("ordinary_loot_item_dependency"));
        }
        Ok(item)
    }
}
impl State {
    /// Trusted host recomputes destinations. Partial capacity/unique results are receipts,
    /// never overflow mail or a unique-count bypass. The callback is staged, pre-store.
    pub fn grant_item(
        &mut self,
        catalog: &Catalog,
        item: u32,
        requested: u32,
        target: GrantTarget,
        before_store: impl FnOnce(&State, u32, u32) -> Result<(), Error>,
    ) -> Result<GrantReceipt, Error> {
        catalog.grant_ready(item)?;
        let plan = self.can_store_plan(catalog, item, requested, &mut Vec::new())?;
        let granted = requested
            .checked_sub(plan.no_space_count)
            .ok_or(Error::Invalid("grant_plan"))?;
        let mut receipt = GrantReceipt {
            policy: GrantPolicy::CmangosAddItemV1,
            target,
            item,
            requested,
            granted,
            ungranted: plan.no_space_count,
            storage_result: plan.result,
            destinations: plan.destinations.clone(),
            stored: None,
            zero_before: (granted == 0).then(|| self.clone()),
        };
        if granted == 0 {
            return Ok(receipt);
        }
        let mut next = self.clone();
        let mut stored = next.store_plan(catalog, item, granted, plan, before_store)?;
        if target == GrantTarget::SelfPlayer {
            // HandleAddItemCommand clears every destination, including an old bound stack.
            for destination in &stored.destinations {
                next.slots
                    .get_mut(&destination.slot)
                    .ok_or(Error::Invalid("grant_destination"))?
                    .bound = false;
            }
            stored.after_slots = next.slots.clone();
            stored.last = next.slots[&stored.last_slot].clone();
        }
        next.validate(catalog)?;
        receipt.stored = Some(stored);
        *self = next;
        Ok(receipt)
    }
}
impl GrantReceipt {
    /// Recompute positive receipts from their complete pre-store witness. The runtime also
    /// binds this witness to its authenticated journal/checkpoint ancestry.
    pub fn validate(&self, catalog: &Catalog, equipped: &BTreeMap<u32, u32>) -> Result<(), Error> {
        catalog.grant_ready(self.item)?;
        if self.requested == 0
            || self.requested > MAX_AWARD_COUNT
            || self.granted.checked_add(self.ungranted) != Some(self.requested)
            || (self.storage_result == InventoryResult::Ok) != (self.ungranted == 0)
            || self
                .destinations
                .iter()
                .map(|d| u64::from(d.count))
                .sum::<u64>()
                != u64::from(self.granted)
        {
            return Err(Error::Invalid("grant_receipt"));
        }
        if let Some(stored) = &self.stored {
            if self.zero_before.is_some() {
                return Err(Error::Invalid("grant_receipt_witness"));
            }
            let mut before = State {
                slots: stored.before_slots.clone(),
                equipped: equipped.clone(),
                next_item: stored.before_next_item,
            };
            let computed = before.grant_item(
                catalog,
                self.item,
                self.requested,
                self.target,
                |_, _, _| Ok(()),
            )?;
            if computed != *self || before.next_item != stored.after_next_item {
                return Err(Error::Invalid("grant_receipt_transition"));
            }
        } else {
            let mut before = self
                .zero_before
                .clone()
                .ok_or(Error::Invalid("grant_receipt_witness"))?;
            if self.granted != 0 || before.equipped != *equipped {
                return Err(Error::Invalid("grant_receipt_empty"));
            }
            let computed = before.grant_item(
                catalog,
                self.item,
                self.requested,
                self.target,
                |_, _, _| Err(Error::Callback("zero grant unexpectedly stores")),
            )?;
            if computed != *self {
                return Err(Error::Invalid("grant_receipt_zero_transition"));
            }
        }
        Ok(())
    }
}
