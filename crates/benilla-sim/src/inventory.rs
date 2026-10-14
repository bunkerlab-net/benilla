// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f.
// CMaNGOS Project contributors; upstream GPL-2.0-or-later, translation GPL-3.0-or-later.
//! Ordinary backpack CanStoreNewItem/StoreNewItem. See ../INVENTORY.md.
//! The caller commits inventory and staged quest callbacks in the same transaction.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const UPSTREAM_COMMIT: &str = "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f";
pub const POLICY_ID: &str = "cmangos-8ec338a1-ordinary-backpack-v2";
pub const BACKPACK_START: u8 = 23;
pub const BACKPACK_END: u8 = 39;
pub const FIRST_SLOT: u8 = BACKPACK_START;
pub const END_SLOT: u8 = BACKPACK_END;
pub const MAX_AWARD_COUNT: u32 = 10_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid(&'static str),
    Unsupported(&'static str),
    Rejected(Plan),
    Exhausted,
    Callback(&'static str),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Classic backpack storage: {self:?}")
    }
}
impl std::error::Error for Error {}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Evidence {
    Absent,
    Present,
    Unknown,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Audit {
    pub item_creation_and_loot_scripts: Evidence,
    pub equipped_bags: Evidence,
    pub bank_and_keyring_items: Evidence,
    pub trade_or_temporary_item_loot: Evidence,
    pub additional_item_mutations: Evidence,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spell {
    pub id: u32,
    pub trigger: u32,
    pub charges: i32,
}
/// Loader-resolved item_template fields and exact dependency absence. No defaults.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub entry: u32,
    pub class: u32,
    pub inventory_type: u32,
    pub display_id: u32,
    pub quality: u32,
    pub flags: u32,
    pub extra_flags: u32,
    pub stackable: i32,
    pub maximum_count: i32,
    pub bonding: u32,
    pub maximum_durability: u32,
    pub bag_family: u32,
    pub container_slots: u32,
    pub random_property: u32,
    pub start_quest: u32,
    pub duration: i32,
    pub item_set: u32,
    pub area: u32,
    pub map: u32,
    pub lock_id: u32,
    pub spells: [Spell; 5],
}

/// A resolved, admitted template catalog. There are no inferred row defaults.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    rows: BTreeMap<u32, Item>,
    audit: Audit,
}
impl Catalog {
    pub fn new(rows: Vec<Item>, audit: Audit) -> Result<Self, Error> {
        let mut map = BTreeMap::new();
        for row in rows {
            if map.insert(row.entry, row).is_some() {
                return Err(Error::Invalid("duplicate_item"));
            }
        }
        let catalog = Self { rows: map, audit };
        catalog.validate()?;
        Ok(catalog)
    }
    pub fn validate(&self) -> Result<(), Error> {
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
        for (&entry, item) in &self.rows {
            if entry == 0
                || entry != item.entry
                || item.display_id == 0
                || item.class >= 16
                || item.inventory_type >= 29
                || item.quality > 6
                || !(1..=255).contains(&item.stackable)
                || item.maximum_count < 0
                || item.bonding > 4
            {
                return Err(Error::Invalid("item_template"));
            }
            if matches!(item.class, 1 | 11)
                || matches!(item.inventory_type, 18 | 27)
                || item.bag_family != 0
                || item.container_slots != 0
                || item.random_property != 0
                || item.start_quest != 0
                || item.duration != 0
                || item.item_set != 0
                || item.area != 0
                || item.map != 0
                || item.lock_id != 0
                || item.extra_flags != 0
                || item.flags & !0x0000_0800 != 0
                || item
                    .spells
                    .iter()
                    .any(|spell| spell.id != 0 || spell.trigger != 0 || spell.charges != 0)
            {
                return Err(Error::Unsupported("item_template_dependency"));
            }
        }
        Ok(())
    }
    pub fn item(&self, entry: u32) -> Result<&Item, Error> {
        self.rows.get(&entry).ok_or(Error::Invalid("unknown_item"))
    }
    pub fn row(&self, entry: u32) -> Result<&Item, Error> {
        self.item(entry)
    }
    pub fn items(&self) -> impl Iterator<Item = &Item> {
        self.rows.values()
    }
    pub fn hash(&self) -> [u8; 32] {
        let mut digest = Sha256::new();
        digest.update(POLICY_ID.as_bytes());
        digest.update([0]);
        digest.update((self.rows.len() as u64).to_le_bytes());
        for (entry, item) in &self.rows {
            digest.update(entry.to_le_bytes());
            for value in [
                item.entry,
                item.class,
                item.inventory_type,
                item.display_id,
                item.quality,
                item.flags,
                item.extra_flags,
            ] {
                digest.update(value.to_le_bytes());
            }
            digest.update(item.stackable.to_le_bytes());
            digest.update(item.maximum_count.to_le_bytes());
            for value in [
                item.bonding,
                item.maximum_durability,
                item.bag_family,
                item.container_slots,
                item.random_property,
                item.start_quest,
            ] {
                digest.update(value.to_le_bytes());
            }
            digest.update(item.duration.to_le_bytes());
            for value in [item.item_set, item.area, item.map, item.lock_id] {
                digest.update(value.to_le_bytes());
            }
            for spell in item.spells {
                digest.update(spell.id.to_le_bytes());
                digest.update(spell.trigger.to_le_bytes());
                digest.update(spell.charges.to_le_bytes());
            }
        }
        for evidence in [
            self.audit.item_creation_and_loot_scripts,
            self.audit.equipped_bags,
            self.audit.bank_and_keyring_items,
            self.audit.trade_or_temporary_item_loot,
            self.audit.additional_item_mutations,
        ] {
            digest.update([match evidence {
                Evidence::Absent => 0,
                Evidence::Present => 1,
                Evidence::Unknown => 2,
            }]);
        }
        digest.finalize().into()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Instance {
    pub id: u64,
    pub item: u32,
    pub count: u32,
    pub bound: bool,
    pub charges: [i32; 5],
    pub durability: u32,
    pub maximum_durability: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Destination {
    pub slot: u8,
    /// Validated to the source ItemPosCount uint8 domain, 1..=255.
    pub count: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[repr(u8)]
pub enum InventoryResult {
    Ok = 0,
    CantCarryMoreOfThis = 17,
    ItemNotFound = 23,
    InventoryFull = 50,
}
impl InventoryResult {
    pub fn protocol(self) -> u8 {
        self as u8
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub result: InventoryResult,
    /// Includes earlier reservations when a shared vector was supplied.
    pub destinations: Vec<Destination>,
    pub no_space_count: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stored {
    pub item: u32,
    /// Full awarded count, not the count of the source's returned last stack.
    pub count: u32,
    pub before_slots: BTreeMap<u8, Instance>,
    pub after_slots: BTreeMap<u8, Instance>,
    pub destinations: Vec<Destination>,
    pub last: Instance,
    pub last_slot: u8,
    pub last_count: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub slots: BTreeMap<u8, Instance>,
    /// Immutable item-entry counts from the separately validated starter equipment.
    pub equipped: BTreeMap<u32, u32>,
    pub next_item: u64,
}
impl State {
    pub fn new(equipped: BTreeMap<u32, u32>, next_item: u64) -> Result<Self, Error> {
        let state = Self {
            slots: BTreeMap::new(),
            equipped,
            next_item,
        };
        state.validate_equipped()?;
        if next_item == 0 {
            return Err(Error::Invalid("next_item"));
        }
        Ok(state)
    }
    fn validate_equipped(&self) -> Result<(), Error> {
        let mut count = 0u32;
        for (&item, &amount) in &self.equipped {
            if item == 0 || amount == 0 || amount > 19 {
                return Err(Error::Invalid("equipped_count"));
            }
            count = count
                .checked_add(amount)
                .ok_or(Error::Invalid("equipped_count"))?;
            if count > 19 {
                return Err(Error::Invalid("equipped_count"));
            }
        }
        Ok(())
    }
    pub fn validate(&self, catalog: &Catalog) -> Result<(), Error> {
        catalog.validate()?;
        self.validate_equipped()?;
        if self.next_item == 0 {
            return Err(Error::Invalid("next_item"));
        }
        let mut ids = BTreeSet::new();
        for (&slot, instance) in &self.slots {
            let item = catalog.item(instance.item)?;
            if !(BACKPACK_START..BACKPACK_END).contains(&slot)
                || instance.id == 0
                || instance.id >= self.next_item
                || !ids.insert(instance.id)
                || instance.count == 0
                || instance.count > item.stackable as u32
                || instance.bound != matches!(item.bonding, 1 | 4)
                || instance.charges != item.spells.map(|spell| spell.charges)
                || instance.maximum_durability != item.maximum_durability
                || instance.durability != item.maximum_durability
            {
                return Err(Error::Invalid("backpack_instance"));
            }
        }
        for &entry in self.equipped.keys() {
            catalog.item(entry)?;
        }
        for item in catalog.items() {
            if item.maximum_count > 0 && self.item_count(item.entry)? > item.maximum_count as u32 {
                return Err(Error::Invalid("existing_unique_count"));
            }
        }
        Ok(())
    }
    pub fn item_count(&self, item: u32) -> Result<u32, Error> {
        self.validate_equipped()?;
        self.slots
            .values()
            .filter(|instance| instance.item == item)
            .try_fold(
                self.equipped.get(&item).copied().unwrap_or(0),
                |count, instance| {
                    count
                        .checked_add(instance.count)
                        .ok_or(Error::Invalid("item_count_overflow"))
                },
            )
    }
    pub fn can_store(&self, catalog: &Catalog, item: u32, count: u32) -> Result<Plan, Error> {
        self.can_store_with_destinations(catalog, item, count, &mut Vec::new())
    }
    pub fn can_store_with_destinations(
        &self,
        catalog: &Catalog,
        item: u32,
        count: u32,
        destinations: &mut Vec<Destination>,
    ) -> Result<Plan, Error> {
        self.validate(catalog)?;
        if count == 0 || count > MAX_AWARD_COUNT {
            return Err(Error::Invalid("award_count"));
        }
        let mut reserved = BTreeSet::new();
        for destination in destinations.iter() {
            if !(BACKPACK_START..BACKPACK_END).contains(&destination.slot)
                || !(1..=255).contains(&destination.count)
                || !reserved.insert(destination.slot)
            {
                return Err(Error::Invalid("reserved_destination"));
            }
        }
        let Some(proto) = catalog.rows.get(&item) else {
            return Ok(Plan {
                result: InventoryResult::ItemNotFound,
                destinations: destinations.clone(),
                no_space_count: count,
            });
        };
        let current = self.item_count(item)?;
        let no_similar = if proto.maximum_count > 0 {
            current
                .checked_add(count)
                .ok_or(Error::Invalid("item_count_overflow"))?
                .saturating_sub(proto.maximum_count as u32)
        } else {
            0
        };
        let mut remaining = count
            .checked_sub(no_similar)
            .ok_or(Error::Invalid("existing_unique_count"))?;
        if remaining != 0 {
            // CanBeMergedPartlyWith accepts only same-entry non-full, non-looting stacks.
            // ItemPosCount::isContainedIn reserves entire positions across quest preflights.
            for merge in [true, false] {
                if merge && proto.stackable == 1 {
                    continue;
                }
                for slot in BACKPACK_START..BACKPACK_END {
                    if reserved.contains(&slot) {
                        continue;
                    }
                    let capacity = match self.slots.get(&slot) {
                        Some(old)
                            if merge && old.item == item && old.count < proto.stackable as u32 =>
                        {
                            proto.stackable as u32 - old.count
                        }
                        None if !merge => proto.stackable as u32,
                        _ => continue,
                    };
                    let count = capacity.min(remaining);
                    destinations.push(Destination { slot, count });
                    reserved.insert(slot);
                    remaining -= count;
                    if remaining == 0 {
                        break;
                    }
                }
                if remaining == 0 {
                    break;
                }
            }
        }
        let result = if remaining != 0 {
            InventoryResult::InventoryFull
        } else if no_similar != 0 {
            InventoryResult::CantCarryMoreOfThis
        } else {
            InventoryResult::Ok
        };
        Ok(Plan {
            result,
            destinations: destinations.clone(),
            no_space_count: remaining + no_similar,
        })
    }
    /// Store only a complete fresh plan. Callback sees pre-store counts and the
    /// original ID already reserved, matching ItemAddedQuestCheck's source position.
    /// The callback must mutate only the caller's isolated transaction host.
    pub fn store_new(
        &mut self,
        catalog: &Catalog,
        item: u32,
        count: u32,
        before_store: impl FnOnce(&State, u32, u32) -> Result<(), Error>,
    ) -> Result<Stored, Error> {
        let plan = self.can_store(catalog, item, count)?;
        if plan.result != InventoryResult::Ok {
            return Err(Error::Rejected(plan));
        }
        let proto = catalog.item(item)?;
        let clone_count = plan
            .destinations
            .iter()
            .take(plan.destinations.len().saturating_sub(1))
            .filter(|destination| !self.slots.contains_key(&destination.slot))
            .count() as u64;
        self.next_item
            .checked_add(1 + clone_count)
            .ok_or(Error::Exhausted)?;
        let mut next = self.clone();
        let original = next.allocate()?;
        before_store(&next, item, count)?;
        for (index, destination) in plan.destinations.iter().enumerate() {
            if let Some(old) = next.slots.get_mut(&destination.slot) {
                old.count += destination.count;
                old.bound = matches!(proto.bonding, 1 | 4);
            } else {
                let id = if index + 1 == plan.destinations.len() {
                    original
                } else {
                    next.allocate()?
                };
                next.slots.insert(
                    destination.slot,
                    Instance {
                        id,
                        item,
                        count: destination.count,
                        bound: matches!(proto.bonding, 1 | 4),
                        charges: proto.spells.map(|spell| spell.charges),
                        durability: proto.maximum_durability,
                        maximum_durability: proto.maximum_durability,
                    },
                );
            }
        }
        next.validate(catalog)?;
        let last_slot = plan
            .destinations
            .last()
            .ok_or(Error::Invalid("empty_storage_plan"))?
            .slot;
        let last = next.slots[&last_slot].clone();
        let stored = Stored {
            item,
            count,
            before_slots: self.slots.clone(),
            after_slots: next.slots.clone(),
            destinations: plan.destinations,
            last_count: last.count,
            last_slot,
            last,
        };
        *self = next;
        Ok(stored)
    }
    fn allocate(&mut self) -> Result<u64, Error> {
        let id = self.next_item;
        self.next_item = id.checked_add(1).ok_or(Error::Exhausted)?;
        Ok(id)
    }
    /// Carried-slot DestroyItemCount steps in source backpack traversal order.
    /// Equipment is an immutable dependency and cannot satisfy a carried removal.
    pub fn removal_steps(
        &self,
        catalog: &Catalog,
        item: u32,
        count: u32,
    ) -> Result<Vec<u32>, Error> {
        self.validate(catalog)?;
        catalog.item(item)?;
        if count == 0 || count > MAX_AWARD_COUNT {
            return Err(Error::Invalid("removal_count"));
        }
        let mut remaining = count;
        let mut steps = Vec::new();
        for instance in self.slots.values().filter(|instance| instance.item == item) {
            let take = remaining.min(instance.count);
            steps.push(take);
            remaining -= take;
            if remaining == 0 {
                return Ok(steps);
            }
        }
        Err(Error::Invalid("insufficient_carried_items"))
    }
    pub fn remove_first_stack(
        &mut self,
        catalog: &Catalog,
        item: u32,
        count: u32,
    ) -> Result<(), Error> {
        self.validate(catalog)?;
        let (&slot, instance) = self
            .slots
            .iter()
            .find(|(_, instance)| instance.item == item)
            .ok_or(Error::Invalid("missing_carried_stack"))?;
        if count == 0 || count > instance.count {
            return Err(Error::Invalid("removal_step_count"));
        }
        if count == instance.count {
            self.slots.remove(&slot);
        } else {
            self.slots.get_mut(&slot).expect("found stack").count -= count;
        }
        Ok(())
    }
}
