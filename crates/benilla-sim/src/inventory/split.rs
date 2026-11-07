// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
//! HandleSplitItemOpcode / Player::SplitItem: explicit ordinary backpack positions only.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SplitPolicy {
    CmangosBackpackSplitV1,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SplitOutcome {
    IgnoredSamePosition,
    IgnoredZeroCount,
    Rejected,
    Stored,
}

/// Ordered source count/store views, including an allocated-but-retired clone on refusal.
/// The runtime authenticates the pre-state; recomputation alone proves no ancestry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitReceipt {
    pub policy: SplitPolicy,
    pub source: u8,
    pub destination: u8,
    pub count: u8,
    pub result: InventoryResult,
    pub outcome: SplitOutcome,
    pub clone_id: Option<u64>,
    pub destinations: Vec<Destination>,
    /// Successful source-count publication, followed by each ordered StoreItem result.
    pub views: Vec<State>,
    pub before: State,
    pub after: State,
}
impl State {
    pub fn split_backpack(
        &mut self,
        catalog: &Catalog,
        source: u8,
        destination: u8,
        count: u8,
    ) -> Result<SplitReceipt, Error> {
        if !(BACKPACK_START..BACKPACK_END).contains(&source)
            || !(BACKPACK_START..BACKPACK_END).contains(&destination)
        {
            return Err(Error::Unsupported("backpack_split_position"));
        }
        catalog.storage_dependencies_ready()?;
        self.validate(catalog)?;
        let mut receipt = SplitReceipt {
            policy: SplitPolicy::CmangosBackpackSplitV1,
            source,
            destination,
            count,
            result: InventoryResult::Ok,
            outcome: SplitOutcome::Stored,
            clone_id: None,
            destinations: Vec::new(),
            views: Vec::new(),
            before: self.clone(),
            after: self.clone(),
        };
        if source == destination {
            receipt.outcome = SplitOutcome::IgnoredSamePosition;
        } else if count == 0 {
            receipt.outcome = SplitOutcome::IgnoredZeroCount;
        } else if let Some(src) = self.slots.get(&source).cloned() {
            let count = u32::from(count);
            if count == src.count {
                receipt.result = InventoryResult::CouldntSplitItems;
            } else if count > src.count {
                receipt.result = InventoryResult::TriedToSplitMoreThanCount;
            } else {
                // CloneItem calls CreateItem, then copies flags/duration/creator fields. This
                // catalog admits only fresh charges/durability and zero unsupported metadata.
                if src.kind.is_container() {
                    return Err(Error::Unsupported("backpack_split_container_source"));
                }
                let proto = catalog.grant_ready(src.item)?;
                let mut allocated = self.clone();
                let id = allocated.allocate()?;
                receipt.clone_id = Some(id);
                let clone = Instance {
                    id,
                    item: src.item,
                    count,
                    bound: src.bound,
                    charges: grants::creation_charges(proto),
                    durability: proto.maximum_durability,
                    maximum_durability: proto.maximum_durability,
                    kind: InstanceKind::created(proto),
                    duration: src.duration,
                    random_property_id: src.random_property_id,
                    enchantments: [0; 3],
                };
                // A storage refusal restores the original count, but the clone ID is consumed.
                receipt.after = allocated.clone();
                let mut reduced = allocated;
                reduced.slots.get_mut(&source).unwrap().count -= count;
                let mut remaining = count;
                if let Some(dst) = reduced.slots.get(&destination) {
                    if dst.item != src.item || dst.count >= proto.stackable as u32 {
                        receipt.result = InventoryResult::ItemCantStack;
                    } else {
                        let amount = remaining.min(proto.stackable as u32 - dst.count);
                        receipt.destinations.push(Destination {
                            slot: destination,
                            count: amount,
                        });
                        remaining -= amount;
                    }
                } else {
                    receipt.destinations.push(Destination {
                        slot: destination,
                        count: remaining,
                    });
                    remaining = 0;
                }
                if receipt.result == InventoryResult::Ok {
                    // No keyring/equipped bags in this checked topology. The original reduced
                    // source remains eligible: source CanStoreItem receives the *clone* pointer.
                    for (&slot, dst) in &reduced.slots {
                        if remaining == 0 {
                            break;
                        }
                        if slot == destination || dst.item != src.item || dst.kind.is_container() {
                            continue;
                        }
                        let amount = remaining.min(proto.stackable as u32 - dst.count);
                        if amount != 0 {
                            receipt.destinations.push(Destination {
                                slot,
                                count: amount,
                            });
                            remaining -= amount;
                        }
                    }
                    // The reduced source always has room for any remainder; another empty
                    // destination/extra clone would indicate an unsupported invariant change.
                    if remaining != 0 {
                        return Err(Error::Invalid("backpack_split_remainder"));
                    }
                    receipt.views.push(reduced.clone());
                    for (index, dest) in receipt.destinations.iter().enumerate() {
                        if let Some(dst) = reduced.slots.get_mut(&dest.slot) {
                            dst.count += dest.count;
                            super::movement::bind_on_store(dst, proto);
                        } else {
                            if index + 1 != receipt.destinations.len() {
                                return Err(Error::Unsupported("backpack_split_extra_clone"));
                            }
                            let mut item = clone.clone();
                            item.count = dest.count;
                            super::movement::bind_on_store(&mut item, proto);
                            reduced.slots.insert(dest.slot, item);
                        }
                        receipt.views.push(reduced.clone());
                    }
                    receipt.after = reduced;
                }
            }
        } else {
            receipt.result = InventoryResult::ItemNotFound;
        }
        if receipt.result != InventoryResult::Ok {
            receipt.outcome = SplitOutcome::Rejected;
        }
        receipt.after.validate(catalog)?;
        *self = receipt.after.clone();
        Ok(receipt)
    }
}
impl SplitReceipt {
    pub fn validate(&self, catalog: &Catalog) -> Result<(), Error> {
        let mut before = self.before.clone();
        if before.split_backpack(catalog, self.source, self.destination, self.count)? != *self {
            return Err(Error::Invalid("backpack_split_receipt"));
        }
        Ok(())
    }
}
