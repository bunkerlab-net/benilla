// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Unit.h:329-402,
// Unit.cpp, Creature.cpp, HomeMovementGenerator.cpp, TargetedMovementGenerator.cpp,
// and PathFinder.cpp. Copyright CMaNGOS contributors; upstream GPL-2.0-or-later.
//! One source UnitState bit, not complete UnitState or movement/config ownership.
use super::*;
use serde::{Deserialize, Serialize};

pub const OWNER_IGNORE_PATHFINDING_POLICY: &str =
    "cmangos-8ec338a1-native-owner-ignore-pathfinding-v1";
pub const UNIT_STAT_IGNORE_PATHFINDING: u32 = 0x1000_0000;
/// Complete LF-normalized files in the provenance-header order. These are source
/// guards, not an oracle executing the entire Unit lifecycle.
pub const OWNER_IGNORE_PATHFINDING_SOURCE_SHA256: [&str; 6] = [
    "2d9db8de53e4881a6684eeb2e31bc904019ee5e620aa067ecd68bd5d07c5d8a1",
    "03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0",
    "27ae75c9e90febd0827fd60f9c9fe27bfeeab34e476d10ac784ab1039d4b8302",
    "d225dd76bd02a33fcfdd17dc0d8a2a953f60f99349f59224e20ef678dbb144ac",
    "eaa569e520a7b7a37d3011822e0381d41ab49b9f2dc6ffa1e43d6344bc14ff15",
    "0fc2684275621af341e104d7593c7f66e5baa6251e088548deb5c547625294ef",
];

/// Frozen runtime receipt. Deserialization is transcript data, not a live owner
/// constructor or authentication. Replay must regenerate this receipt from the
/// actual staged capability owner and current executable ability set.
/// ```compile_fail
/// use benilla_world_runtime::native::OwnerIgnorePathfindingReceipt;
/// let receipt = OwnerIgnorePathfindingReceipt { projected_unit_state: 0 };
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerIgnorePathfindingReceipt {
    policy: String,
    npc: crate::NpcId,
    profile_binding: crate::Digest,
    source_binding: crate::Digest,
    capability_binding: crate::Digest,
    /// Captured from State::abilities, including learned/ranked abilities. This
    /// immutable transcript copy is not a second live ability or UnitState owner.
    live_abilities: Vec<Ability>,
    projected_unit_state: u32,
}
impl OwnerIgnorePathfindingReceipt {
    pub(super) fn mint(
        profile: &Profile,
        state: &State,
        npc: crate::NpcId,
        capabilities: Option<&CreatureCapabilityState>,
    ) -> Result<Option<Self>, NativeError> {
        let Some(capabilities) = capabilities else {
            return Ok(None);
        };
        capabilities.validate(profile, npc)?;
        if profile.interactions.is_none() {
            return Ok(None);
        }
        let live_abilities = state.abilities(profile);
        let (source_binding, projected_unit_state) =
            Self::source(profile, npc, capabilities.witness(), live_abilities)?;
        // Chase/ChaseMove and melee mutate different bits. Home clears ALL_DYN_STATES,
        // which explicitly excludes IGNORE_PATHFINDING. Complete source/control/
        // aura/script exclusions leave only the captured initial value for this bit.
        Ok(Some(Self {
            policy: OWNER_IGNORE_PATHFINDING_POLICY.into(),
            npc,
            profile_binding: profile.digest(),
            source_binding,
            capability_binding: capabilities.witness().source_binding(),
            live_abilities: live_abilities.to_vec(),
            projected_unit_state,
        }))
    }
    fn source(
        profile: &Profile,
        npc: crate::NpcId,
        capabilities: &CreatureCapabilityWitness,
        live_abilities: &[Ability],
    ) -> Result<(crate::Digest, u32), NativeError> {
        capabilities.validate_for(profile, npc)?;
        let interactions =
            profile
                .interactions
                .as_ref()
                .ok_or(NativeError::AuthorityUnavailable(
                    "owned IGNORE_PATHFINDING interaction sources",
                ))?;
        let (interaction_binding, projected) =
            interactions.owner_ignore_pathfinding_source(profile, npc)?;
        let motion = profile
            .source_motion
            .as_ref()
            .ok_or(NativeError::AuthorityUnavailable(
                "owned IGNORE_PATHFINDING motion closure",
            ))?;
        motion.validate(profile)?;
        if live_abilities.len() > 64 {
            return Err(NativeError::TooLarge);
        }
        interactions::validate_abilities(live_abilities)?;
        for ability in live_abilities {
            ability.validate(profile.tick_ms)?;
        }
        Ok((
            crate::digest(
                b"benilla-owner-ignore-pathfinding-source-v1\0",
                &(
                    OWNER_IGNORE_PATHFINDING_POLICY,
                    OWNER_IGNORE_PATHFINDING_SOURCE_SHA256,
                    capabilities.source_binding(),
                    interaction_binding,
                    motion,
                    live_abilities,
                ),
            ),
            projected,
        ))
    }
    pub fn npc(&self) -> crate::NpcId {
        self.npc
    }
    pub fn source_binding(&self) -> crate::Digest {
        self.source_binding
    }
    pub fn profile_binding(&self) -> crate::Digest {
        self.profile_binding
    }
    pub fn policy(&self) -> &str {
        &self.policy
    }
    /// Only IGNORE_PATHFINDING is represented; all other UnitState bits are omitted.
    pub fn projected_unit_state(&self) -> u32 {
        self.projected_unit_state
    }
    pub fn ignore_pathfinding(&self) -> bool {
        self.projected_unit_state & UNIT_STAT_IGNORE_PATHFINDING != 0
    }
}
impl CreatureRouteRequest {
    /// Validate source/profile/generation and request identity. This bit is an
    /// invariant independent of path endpoints and cache contents; it proves no
    /// MMap configuration, tile presence, full calculate result or Route admission.
    pub fn require_owner_ignore_pathfinding(
        &self,
        profile: &Profile,
    ) -> Result<&OwnerIgnorePathfindingReceipt, NativeError> {
        profile.validate()?;
        self.owner.validate_for(profile, self.owner.npc())?;
        let identity = self.request.binding.identity;
        if identity.creature != self.owner.npc().spawn
            || identity.map != profile.map_id
            || identity.content.0 != profile.content_digest
            || identity.geometry.0 != profile.geometry_revision
            || self.owner_position != self.request.origin
        {
            return Err(NativeError::AuthorityBinding);
        }
        let receipt =
            self.owner_ignore_pathfinding
                .as_ref()
                .ok_or(NativeError::AuthorityUnavailable(
                    "owned IGNORE_PATHFINDING state",
                ))?;
        let (source_binding, projected) = OwnerIgnorePathfindingReceipt::source(
            profile,
            self.owner.npc(),
            &self.owner,
            &receipt.live_abilities,
        )?;
        if receipt.policy != OWNER_IGNORE_PATHFINDING_POLICY
            || receipt.npc != self.owner.npc()
            || receipt.profile_binding != profile.digest()
            || receipt.capability_binding != self.owner.source_binding()
            || receipt.source_binding != source_binding
            || receipt.projected_unit_state != projected
        {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(receipt)
    }
}
