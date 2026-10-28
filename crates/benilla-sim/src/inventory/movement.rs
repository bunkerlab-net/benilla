// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f.
// CMaNGOS Project contributors; upstream GPL-2.0-or-later, translation GPL-3.0-or-later.
//! Exact movement within the sixteen ordinary backpack slots, without allocation.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum MovementPolicy {
    CmangosBackpackSwapV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum MovementOutcome {
    /// HandleSwapInvItemOpcode/HandleSwapItem ignores this before calling SwapItem.
    IgnoredSameSlot,
    /// SwapItem checks the source before the player's alive state.
    IgnoredEmptySource,
    Rejected,
    Moved,
    Swapped,
    Merged {
        transferred: u32,
        removed_source: bool,
    },
}

/// Full deterministic witness, including source no-ops and legitimate refusals.
/// The host binds its pre-state and alive input to authenticated runtime history.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MovementReceipt {
    pub policy: MovementPolicy,
    pub source: u8,
    pub destination: u8,
    pub alive: bool,
    /// OK for successful movement and ignored input. Only non-OK sends an error.
    pub result: InventoryResult,
    pub outcome: MovementOutcome,
    pub before: State,
    pub after: State,
}

impl State {
    /// Apply the inventory-only SwapItem branch after the packet same-slot guard.
    /// Unsupported topology and invalid state fail before mutation; source errors
    /// return a receipt. Instance IDs and the allocator are never regenerated.
    /// Equipment is from a separately validated immutable loadout. Ambiguous
    /// two-hand/offhand combinations reject because counts cannot locate slots.
    pub fn swap_backpack(
        &mut self,
        catalog: &Catalog,
        source: u8,
        destination: u8,
        alive: bool,
    ) -> Result<MovementReceipt, Error> {
        if !(BACKPACK_START..BACKPACK_END).contains(&source)
            || !(BACKPACK_START..BACKPACK_END).contains(&destination)
        {
            return Err(Error::Unsupported("backpack_movement_position"));
        }
        catalog.storage_dependencies_ready()?;
        self.validate(catalog)?;
        self.movement_equipment_ready(catalog)?;
        let before = self.clone();
        let mut after = before.clone();
        let mut result = InventoryResult::Ok;
        let outcome = if source == destination {
            MovementOutcome::IgnoredSameSlot
        } else if let Some(mut src) = before.slots.get(&source).cloned() {
            if !alive {
                result = InventoryResult::YouAreDead;
                MovementOutcome::Rejected
            } else {
                let src_proto = catalog.grant_ready(src.item)?;
                if let Some(mut dst) = before.slots.get(&destination).cloned() {
                    // CanBeMergedPartlyWith compares entry/capacity, not binding.
                    // CanStoreItem ignores pSrcItem: its original slot guarantees
                    // capacity for a remainder, even in a completely full backpack.
                    if !src.kind.is_container()
                        && !dst.kind.is_container()
                        && src.item == dst.item
                        && dst.count < src_proto.stackable as u32
                    {
                        let transferred = src.count.min(src_proto.stackable as u32 - dst.count);
                        let removed_source = transferred == src.count;
                        dst.count += transferred;
                        if removed_source {
                            // The complete merge reaches _StoreItem; partial fill
                            // only calls SetCount and must not change binding.
                            bind_on_store(&mut dst, src_proto);
                            after.slots.remove(&source);
                        } else {
                            src.count -= transferred;
                            after.slots.insert(source, src);
                        }
                        after.slots.insert(destination, dst);
                        MovementOutcome::Merged {
                            transferred,
                            removed_source,
                        }
                    } else {
                        // Full target stacks and both empty Bag objects take the
                        // real-swap branch, retaining both instance identities.
                        bind_on_store(&mut src, src_proto);
                        let dst_proto = catalog.grant_ready(dst.item)?;
                        bind_on_store(&mut dst, dst_proto);
                        after.slots.insert(destination, src);
                        after.slots.insert(source, dst);
                        MovementOutcome::Swapped
                    }
                } else {
                    bind_on_store(&mut src, src_proto);
                    after.slots.remove(&source);
                    after.slots.insert(destination, src);
                    MovementOutcome::Moved
                }
            }
        } else {
            MovementOutcome::IgnoredEmptySource
        };
        after.validate(catalog)?;
        let receipt = MovementReceipt {
            policy: MovementPolicy::CmangosBackpackSwapV1,
            source,
            destination,
            alive,
            result,
            outcome,
            before,
            after,
        };
        *self = receipt.after.clone();
        Ok(receipt)
    }

    fn movement_equipment_ready(&self, catalog: &Catalog) -> Result<(), Error> {
        let mut two_handed = 0;
        let mut offhand_compatible = false;
        for (&entry, &count) in &self.equipped {
            match catalog.item(entry)?.inventory_type {
                17 => two_handed += count,
                13 | 14 | 22 | 23 => offhand_compatible = true,
                _ => {}
            }
        }
        // AutoUnequipOffhandIfNeed can move equipment after a real swap. Counts
        // cannot distinguish its main/offhand slots, so ambiguous loadouts reject.
        if two_handed > 1 || (two_handed != 0 && offhand_compatible) {
            return Err(Error::Unsupported("backpack_movement_offhand_dependency"));
        }
        Ok(())
    }
}

pub(super) fn bind_on_store(instance: &mut Instance, item: &Item) {
    // _StoreItem never unbinds; BIND_WHEN_EQUIPPED does not apply in backpack slots.
    if matches!(item.bonding, 1 | 4) {
        instance.bound = true;
    }
}

impl MovementReceipt {
    /// Recompute the exact transition, including complete untouched metadata.
    /// This proves local semantics; the runtime separately authenticates ancestry.
    pub fn validate(&self, catalog: &Catalog) -> Result<(), Error> {
        let mut before = self.before.clone();
        let computed = before.swap_backpack(catalog, self.source, self.destination, self.alive)?;
        if computed != *self {
            return Err(Error::Invalid("backpack_movement_receipt"));
        }
        Ok(())
    }
}
