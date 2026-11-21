// SPDX-License-Identifier: GPL-3.0-or-later
//! Fork-only deterministic admin transactions. Neither a PSK nor a claimed actor grants access.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const ADMIN_POLICY: &str = "benilla-explicit-world-admin-v1";
pub const CLASSIC_LEVEL_CEILING: u32 = 60;
pub const MAX_ADMIN_RECEIPTS: usize = 1024;
/// Keep historical item witnesses from consuming the checkpoint's entire state budget.
pub const MAX_ADMIN_RECEIPT_BYTES: usize = 1024 * 1024;
pub const MAX_ADMIN_SINGLE_RECEIPT_BYTES: usize = 32 * 1024;
pub const MAX_ADMIN_ALLOWLIST: usize = 64;
/// A correlation/idempotency value, never a credential or proof of authentication.
pub type AdminRequestKey = [u8; 16];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminPolicy {
    pub world: crate::WorldId,
    pub world_owner: crate::OwnerId,
    pub allowlist: BTreeSet<crate::OwnerId>,
    pub revision: u64,
}
impl AdminPolicy {
    pub fn validate(&self) -> Result<(), NativeError> {
        if self.world == [0; 16]
            || self.world_owner == [0; 16]
            || self.revision == 0
            || self.allowlist.len() > MAX_ADMIN_ALLOWLIST
            || self.allowlist.contains(&[0; 16])
            || self.allowlist.contains(&self.world_owner)
        {
            return Err(NativeError::InvalidProfile);
        }
        Ok(())
    }
    pub fn permits(&self, authenticated_owner: crate::OwnerId) -> bool {
        authenticated_owner == self.world_owner || self.allowlist.contains(&authenticated_owner)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AdminOperation {
    GainOneLevel,
    SetXpGain { enabled: bool },
    GrantItem { item: u32, count: u32 },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminRequest {
    pub key: AdminRequestKey,
    pub world: crate::WorldId,
    pub player: crate::PlayerId,
    pub policy_revision: u64,
    pub operation: AdminOperation,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AdminRejection {
    Disabled,
    Unauthorized,
    WrongContext,
    StalePolicy,
    InvalidKey,
    RequestConflict,
    ReceiptCapacity,
    ProgressionUnavailable,
    InventoryUnavailable,
    InvalidItemGrant,
    UnsupportedItemGrant,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AdminEffect {
    ItemGrant(Box<benilla_sim::inventory::GrantReceipt>),
    Level {
        before: u32,
        after: u32,
        xp: u32,
        supported_maximum: u32,
    },
    XpGain {
        before: bool,
        enabled: bool,
        xp: u32,
        level: u32,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminReceipt {
    pub request: AdminRequest,
    pub tick: u64,
    pub sequence: u64,
    pub effect: AdminEffect,
}
impl AdminReceipt {
    pub fn digest(&self) -> crate::Digest {
        crate::digest(b"benilla-admin-receipt-v1\0", &(ADMIN_POLICY, self))
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminItemReadiness {
    pub item: u32,
    pub grant: Result<(), String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminSnapshot {
    pub world: crate::WorldId,
    pub player: crate::PlayerId,
    pub policy_revision: u64,
    pub level: u32,
    pub xp: u32,
    pub xp_enabled: bool,
    pub configured_maximum: u32,
    pub supported_maximum: u32,
    pub classic_ceiling: u32,
    pub receipts_remaining: usize,
    pub items: Vec<AdminItemReadiness>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminState {
    // Never evicted: replaying an old ID can never become a new grant.
    receipts: Vec<AdminReceipt>,
    /// Revalidated on restore, maintained on insertion; snapshots do not serialize the ledger.
    receipt_bytes: u32,
}
impl Default for AdminState {
    fn default() -> Self {
        Self {
            receipts: Vec::new(),
            receipt_bytes: 2,
        }
    }
}
impl AdminState {
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    fn serialized_bytes(&self) -> Result<usize, NativeError> {
        serde_json::to_vec(&self.receipts)
            .map(|v| v.len())
            .map_err(|_| NativeError::InvalidState)
    }
    fn has_capacity(&self) -> bool {
        self.receipts.len() < MAX_ADMIN_RECEIPTS
            && (self.receipt_bytes as usize).saturating_add(MAX_ADMIN_SINGLE_RECEIPT_BYTES + 1)
                <= MAX_ADMIN_RECEIPT_BYTES
    }
    pub fn receipt(&self, key: &AdminRequestKey) -> Option<&AdminReceipt> {
        self.receipts
            .binary_search_by_key(key, |r| r.request.key)
            .ok()
            .map(|i| &self.receipts[i])
    }
    pub(in crate::native) fn insert(&mut self, receipt: AdminReceipt) {
        match self
            .receipts
            .binary_search_by_key(&receipt.request.key, |r| r.request.key)
        {
            Ok(_) => unreachable!("admin receipt identity preflighted"),
            Err(i) => {
                let size = serde_json::to_vec(&receipt)
                    .expect("fixed admin receipt schema serializes")
                    .len();
                self.receipt_bytes = self
                    .receipt_bytes
                    .checked_add(
                        u32::try_from(size + usize::from(!self.receipts.is_empty()))
                            .expect("bounded serialized receipt"),
                    )
                    .expect("bounded admin receipt ledger");
                self.receipts.insert(i, receipt);
            }
        }
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(in crate::native) fn validate(
        &self,
        profile: &Profile,
        state: &State,
    ) -> Result<(), NativeError> {
        if self.receipts.len() > MAX_ADMIN_RECEIPTS
            || self.receipt_bytes as usize != self.serialized_bytes()?
            || self.receipt_bytes as usize > MAX_ADMIN_RECEIPT_BYTES
            || self
                .receipts
                .windows(2)
                .any(|w| w[0].request.key >= w[1].request.key)
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        let xp_enabled = state
            .quest_progression
            .as_ref()
            .is_none_or(|q| q.xp_enabled);
        let Some(policy) = &profile.admin else {
            return if self.receipts.is_empty() && xp_enabled {
                Ok(())
            } else {
                Err(NativeError::InvalidCheckpoint)
            };
        };
        if policy.world != state.world {
            return Err(NativeError::InvalidCheckpoint);
        }
        let g = profile
            .quest_progression
            .as_ref()
            .ok_or(NativeError::InvalidCheckpoint)?;
        let q = state
            .quest_progression
            .as_ref()
            .ok_or(NativeError::InvalidCheckpoint)?;
        let max = g.admin_supported_maximum();
        let mut sequences = BTreeSet::new();
        let mut ordered: Vec<_> = self.receipts.iter().collect();
        ordered.sort_by_key(|r| r.sequence);
        let mut expected_xp_enabled = true;
        let mut previous_tick = 0;
        for receipt in ordered {
            let request = &receipt.request;
            if request.key == [0; 16]
                || self.receipt(&request.key) != Some(receipt)
                || request.world != state.world
                || request.player != state.player
                || !policy.permits(state.player.owner)
                || request.policy_revision != policy.revision
                || receipt.tick == 0
                || receipt.tick > state.tick
                || receipt.tick < previous_tick
                || receipt.sequence >= state.next_sequence
                || !sequences.insert(receipt.sequence)
            {
                return Err(NativeError::InvalidCheckpoint);
            }
            if serde_json::to_vec(receipt)
                .map_err(|_| NativeError::InvalidCheckpoint)?
                .len()
                > MAX_ADMIN_SINGLE_RECEIPT_BYTES
            {
                return Err(NativeError::InvalidCheckpoint);
            }
            previous_tick = receipt.tick;
            match (&request.operation, &receipt.effect) {
                (
                    AdminOperation::GainOneLevel,
                    AdminEffect::Level {
                        before,
                        after,
                        supported_maximum,
                        ..
                    },
                ) if *before >= 1
                    && *before <= max
                    && *after == (*before + 1).min(max)
                    && *after <= q.player.view().level
                    && *supported_maximum == max => {}
                (
                    AdminOperation::SetXpGain { enabled },
                    AdminEffect::XpGain {
                        before,
                        enabled: result,
                        level,
                        ..
                    },
                ) if enabled == result
                    && *before == expected_xp_enabled
                    && *level >= 1
                    && *level <= q.player.view().level =>
                {
                    expected_xp_enabled = *enabled;
                }
                (AdminOperation::GrantItem { item, count }, AdminEffect::ItemGrant(grant))
                    if grant.item == *item
                        && grant.requested == *count
                        && grant.target == benilla_sim::inventory::GrantTarget::SelfPlayer =>
                {
                    let catalog = &g
                        .loot
                        .as_ref()
                        .ok_or(NativeError::InvalidCheckpoint)?
                        .inventory;
                    let inventory = q.inventory.as_ref().ok_or(NativeError::InvalidCheckpoint)?;
                    grant.validate(catalog, &inventory.equipped)?;
                    if grant
                        .stored
                        .as_ref()
                        .is_some_and(|s| s.after_next_item > inventory.next_item)
                        || grant
                            .zero_before
                            .as_ref()
                            .is_some_and(|s| s.next_item > inventory.next_item)
                    {
                        return Err(NativeError::InvalidCheckpoint);
                    }
                }
                _ => return Err(NativeError::InvalidCheckpoint),
            }
        }
        if expected_xp_enabled != xp_enabled {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok(())
    }
}
impl NativeWorld {
    pub fn admin_receipt(&self, key: &AdminRequestKey) -> Option<&AdminReceipt> {
        self.state.admin.receipt(key)
    }
    pub fn admin_snapshot(
        &self,
        authenticated_owner: crate::OwnerId,
    ) -> Result<AdminSnapshot, AdminRejection> {
        if authenticated_owner != self.state.player.owner {
            return Err(AdminRejection::Unauthorized);
        }
        let policy = self
            .profile
            .admin
            .as_ref()
            .ok_or(AdminRejection::Disabled)?;
        if policy.world != self.state.world {
            return Err(AdminRejection::WrongContext);
        }
        if !policy.permits(authenticated_owner) {
            return Err(AdminRejection::Unauthorized);
        }
        let g = self
            .profile
            .quest_progression
            .as_ref()
            .ok_or(AdminRejection::ProgressionUnavailable)?;
        let q = self
            .state
            .quest_progression
            .as_ref()
            .ok_or(AdminRejection::ProgressionUnavailable)?;
        Ok(AdminSnapshot {
            world: self.state.world,
            player: self.state.player,
            policy_revision: policy.revision,
            level: q.player.view().level,
            xp: q.player.view().xp,
            xp_enabled: q.xp_enabled,
            configured_maximum: g.maximum_player_level(),
            supported_maximum: g.admin_supported_maximum(),
            classic_ceiling: CLASSIC_LEVEL_CEILING,
            receipts_remaining: if self.state.admin.has_capacity() {
                MAX_ADMIN_RECEIPTS - self.state.admin.receipts.len()
            } else {
                0
            },
            items: g.loot.as_ref().map_or_else(Vec::new, |g| {
                g.inventory
                    .items()
                    .map(|item| AdminItemReadiness {
                        item: item.entry,
                        grant: g
                            .inventory
                            .grant_ready(item.entry)
                            .map(|_| ())
                            .map_err(|e| e.to_string()),
                    })
                    .collect()
            }),
        })
    }
    /// Admission of new requests. An exact committed retry is queried separately, before
    /// current session availability; it never causes a mutation or grants new permission.
    pub fn validate_admin_request(
        &self,
        authenticated_owner: crate::OwnerId,
        request: &AdminRequest,
    ) -> Result<(), AdminRejection> {
        admin_request_gate(&self.profile, &self.state, authenticated_owner, request)
    }
}
pub(super) fn admin_request_gate(
    profile: &Profile,
    state: &State,
    authenticated_owner: crate::OwnerId,
    request: &AdminRequest,
) -> Result<(), AdminRejection> {
    if authenticated_owner != state.player.owner {
        return Err(AdminRejection::Unauthorized);
    }
    if request.world != state.world || request.player != state.player {
        return Err(AdminRejection::WrongContext);
    }
    if request.key == [0; 16] {
        return Err(AdminRejection::InvalidKey);
    }
    if let Some(receipt) = state.admin.receipt(&request.key) {
        return if &receipt.request == request {
            Ok(())
        } else {
            Err(AdminRejection::RequestConflict)
        };
    }
    let policy = profile.admin.as_ref().ok_or(AdminRejection::Disabled)?;
    if policy.world != state.world {
        return Err(AdminRejection::WrongContext);
    }
    if !policy.permits(authenticated_owner) {
        return Err(AdminRejection::Unauthorized);
    }
    if request.policy_revision != policy.revision {
        return Err(AdminRejection::StalePolicy);
    }
    if profile.quest_progression.is_none() || state.quest_progression.is_none() {
        return Err(AdminRejection::ProgressionUnavailable);
    }
    if let AdminOperation::GrantItem { item, count } = request.operation {
        if count == 0 || count > benilla_sim::inventory::MAX_AWARD_COUNT {
            return Err(AdminRejection::InvalidItemGrant);
        }
        let catalog = &profile
            .quest_progression
            .as_ref()
            .and_then(|g| g.loot.as_ref())
            .ok_or(AdminRejection::InventoryUnavailable)?
            .inventory;
        if state
            .quest_progression
            .as_ref()
            .and_then(|q| q.inventory.as_ref())
            .is_none()
        {
            return Err(AdminRejection::InventoryUnavailable);
        }
        catalog
            .item(item)
            .map_err(|_| AdminRejection::InvalidItemGrant)?;
        catalog
            .grant_ready(item)
            .map_err(|_| AdminRejection::UnsupportedItemGrant)?;
    }
    if !state.admin.has_capacity() {
        return Err(AdminRejection::ReceiptCapacity);
    }
    Ok(())
}

#[cfg(test)]
mod byte_budget_tests {
    use super::*;
    use benilla_sim::inventory as inv;
    #[test]
    fn maximum_width_item_witness_fits_reserved_headroom_and_capacity_closes_before_checkpoint_limit(
    ) {
        let mut item = crate::native::synthetic_loot_fixture::item(u32::MAX);
        item.stackable = 255;
        item.maximum_durability = u32::MAX;
        for s in &mut item.spells {
            s.charges = i32::MIN;
            s.id = u32::MAX;
            s.cooldown = i32::MIN;
            s.category = u32::MAX;
            s.category_cooldown = i32::MIN;
        }
        for bag in [false, true] {
            if bag {
                item.inventory_type = 18;
                item.container_slots = 36;
            }
            let catalog =
                crate::native::synthetic_loot_fixture::inventory_catalog(vec![item.clone()]);
            let mut inventory =
                inv::State::new(BTreeMap::from([(u32::MAX, 19)]), u64::MAX - 100).unwrap();
            inventory
                .grant_item(
                    &catalog,
                    u32::MAX,
                    4079,
                    inv::GrantTarget::SelfPlayer,
                    |_, _, _| Ok(()),
                )
                .unwrap();
            let grant = inventory
                .grant_item(
                    &catalog,
                    u32::MAX,
                    10000,
                    inv::GrantTarget::SelfPlayer,
                    |_, _, _| Ok(()),
                )
                .unwrap();
            let receipt = AdminReceipt {
                request: AdminRequest {
                    key: [255; 16],
                    world: [255; 16],
                    player: crate::PlayerId {
                        owner: [255; 16],
                        character: [255; 16],
                    },
                    policy_revision: u64::MAX,
                    operation: AdminOperation::GrantItem {
                        item: u32::MAX,
                        count: 10000,
                    },
                },
                tick: u64::MAX,
                sequence: u64::MAX,
                effect: AdminEffect::ItemGrant(Box::new(grant)),
            };
            let size = serde_json::to_vec(&receipt).unwrap().len();
            assert!(size < MAX_ADMIN_SINGLE_RECEIPT_BYTES, "{size}");
            let mut state = AdminState::default();
            let mut n = 0u64;
            while state.has_capacity() {
                let mut receipt = receipt.clone();
                receipt.request.key[..8].copy_from_slice(&n.to_be_bytes());
                state.insert(receipt);
                n += 1;
                assert!(state.serialized_bytes().unwrap() <= MAX_ADMIN_RECEIPT_BYTES);
            }
            assert!(n < MAX_ADMIN_RECEIPTS as u64);
            assert!(
                state.serialized_bytes().unwrap() + MAX_ADMIN_SINGLE_RECEIPT_BYTES + 1
                    > MAX_ADMIN_RECEIPT_BYTES
            );
            assert!(state.serialized_bytes().unwrap() < crate::native::MAX_BYTES / 4);
            assert_eq!(
                state.receipt_bytes as usize,
                state.serialized_bytes().unwrap()
            );
        }
    }
}
