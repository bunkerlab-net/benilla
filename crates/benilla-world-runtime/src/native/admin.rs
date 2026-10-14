// SPDX-License-Identifier: GPL-3.0-or-later
//! Fork-only deterministic admin transactions. Neither a PSK nor a claimed actor grants access.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const ADMIN_POLICY: &str = "benilla-explicit-world-admin-v1";
pub const CLASSIC_LEVEL_CEILING: u32 = 60;
pub const MAX_ADMIN_RECEIPTS: usize = 1024;
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
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AdminEffect {
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
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct AdminState {
    // Never evicted: replaying an old ID can never become a new grant.
    receipts: Vec<AdminReceipt>,
}
impl AdminState {
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
            Err(i) => self.receipts.insert(i, receipt),
        }
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(in crate::native) fn validate(
        &self,
        profile: &Profile,
        state: &State,
    ) -> Result<(), NativeError> {
        if self.receipts.len() > MAX_ADMIN_RECEIPTS
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
            receipts_remaining: MAX_ADMIN_RECEIPTS - self.state.admin.receipts.len(),
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
    if state.admin.receipts.len() >= MAX_ADMIN_RECEIPTS {
        return Err(AdminRejection::ReceiptCapacity);
    }
    Ok(())
}
