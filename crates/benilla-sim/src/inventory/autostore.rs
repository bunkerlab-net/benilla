// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
//! HandleAutoStoreBagItemOpcode: ordinary backpack to ordinary backpack only.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AutoStorePolicy {
    CmangosBackpackAutoStoreV1,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AutoStoreOutcome {
    IgnoredEmptySource,
    Unchanged,
    Stored,
}
/// Exact detach/store observations; the host authenticates the complete pre-state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutoStoreReceipt {
    pub policy: AutoStorePolicy,
    pub source: u8,
    pub outcome: AutoStoreOutcome,
    pub destinations: Vec<Destination>,
    pub views: Vec<State>,
    pub before: State,
    pub after: State,
}
impl State {
    pub fn autostore_backpack(
        &mut self,
        catalog: &Catalog,
        source: u8,
    ) -> Result<AutoStoreReceipt, Error> {
        if !(BACKPACK_START..BACKPACK_END).contains(&source) {
            return Err(Error::Unsupported("backpack_autostore_position"));
        }
        catalog.storage_dependencies_ready()?;
        self.validate(catalog)?;
        let mut receipt = AutoStoreReceipt {
            policy: AutoStorePolicy::CmangosBackpackAutoStoreV1,
            source,
            outcome: AutoStoreOutcome::IgnoredEmptySource,
            destinations: vec![],
            views: vec![],
            before: self.clone(),
            after: self.clone(),
        };
        let Some(src) = self.slots.get(&source).cloned() else {
            return Ok(receipt);
        };
        if src.kind.is_container() {
            return Err(Error::Unsupported("backpack_autostore_container"));
        }
        let proto = catalog.grant_ready(src.item)?;
        let mut remaining = src.count;
        // CanStore ignores the source pointer: merge other stacks, then empty slots.
        for (&slot, dst) in &self.slots {
            if remaining == 0 {
                break;
            }
            if slot == source || dst.item != src.item || dst.kind.is_container() {
                continue;
            }
            let count = remaining.min(proto.stackable as u32 - dst.count);
            if count != 0 {
                receipt.destinations.push(Destination { slot, count });
                remaining -= count;
            }
        }
        if remaining != 0 {
            let slot = (BACKPACK_START..BACKPACK_END)
                .find(|slot| *slot == source || !self.slots.contains_key(slot))
                .ok_or(Error::Invalid("backpack_autostore_capacity"))?;
            receipt.destinations.push(Destination {
                slot,
                count: remaining,
            });
        }
        if receipt.destinations.len() == 1 && receipt.destinations[0].slot == source {
            // Source sends EQUIP_ERR_NONE to clear the gray cursor, without binding.
            receipt.outcome = AutoStoreOutcome::Unchanged;
            return Ok(receipt);
        }
        let mut next = self.clone();
        next.slots.remove(&source);
        receipt.views.push(next.clone());
        for (index, dest) in receipt.destinations.iter().enumerate() {
            if let Some(dst) = next.slots.get_mut(&dest.slot) {
                dst.count += dest.count;
                movement::bind_on_store(dst, proto);
            } else {
                // <= one stack of input, with the vacated source available: an empty
                // destination can only be final. Thus source StoreItem never clones.
                if index + 1 != receipt.destinations.len() {
                    return Err(Error::Invalid("backpack_autostore_clone"));
                }
                let mut moved = src.clone();
                moved.count = dest.count;
                movement::bind_on_store(&mut moved, proto);
                next.slots.insert(dest.slot, moved);
            }
            receipt.views.push(next.clone());
        }
        next.validate(catalog)?;
        receipt.outcome = AutoStoreOutcome::Stored;
        receipt.after = next;
        *self = receipt.after.clone();
        Ok(receipt)
    }
}
impl AutoStoreReceipt {
    pub fn validate(&self, catalog: &Catalog) -> Result<(), Error> {
        let mut before = self.before.clone();
        if before.autostore_backpack(catalog, self.source)? != *self {
            return Err(Error::Invalid("backpack_autostore_receipt"));
        }
        Ok(())
    }
}
