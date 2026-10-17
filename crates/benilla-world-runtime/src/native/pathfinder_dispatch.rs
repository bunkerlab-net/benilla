// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright CMaNGOS contributors; upstream GPL-2.0-or-later, Rust GPL-3.0-or-later.
// CMaNGOS 8ec338a1 TargetedMovementGenerator.cpp:482-500 and PathFinder.cpp:37-106.
//! Modeled Chase, initialized Wander or stack-local Home dispatch before path selection. This does
//! not allocate a native query, prove its presence, or admit a Route result.
use super::*;
use benilla_sim::creature_motion as m;
use serde::{Deserialize, Deserializer, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub const PATHFINDER_DISPATCH_POLICY: &str =
    "cmangos-8ec338a1-native-pathfinder-generator-constructor-dispatch-v3";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DispatchEvidence {
    policy: String,
    profile_binding: crate::Digest,
    configuration_binding: crate::Digest,
    npc: crate::NpcId,
    world: crate::WorldId,
    dispatch_tick: u64,
    dispatch: m::PathfinderDispatch,
    request_binding: crate::Digest,
}

/// Live runtime event plus exact replayable metadata. Clones share one constructor
/// claim; deserialization restores metadata only, never a fresh live claim. A
/// provider must pair the event with its already loaded source map-instance query.
/// ```compile_fail
/// use benilla_world_runtime::native::PathfinderDispatchReceipt;
/// let receipt = PathfinderDispatchReceipt { live: None };
/// ```
#[derive(Clone, Debug, Serialize)]
#[serde(transparent)]
pub struct PathfinderDispatchReceipt {
    evidence: DispatchEvidence,
    #[serde(skip)]
    live: Option<Arc<AtomicBool>>,
}
impl PartialEq for PathfinderDispatchReceipt {
    fn eq(&self, other: &Self) -> bool {
        // Replay compares every serialized field, not process-local allocation.
        self.evidence == other.evidence
    }
}
impl Eq for PathfinderDispatchReceipt {}
impl<'de> Deserialize<'de> for PathfinderDispatchReceipt {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self {
            evidence: DispatchEvidence::deserialize(deserializer)?,
            live: None,
        })
    }
}
impl PathfinderDispatchReceipt {
    pub(super) fn mint(
        profile: &Profile,
        world: crate::WorldId,
        dispatch_tick: u64,
        npc: crate::NpcId,
        owner: &CreatureCapabilityWitness,
        request: &m::RouteRequest,
    ) -> Result<Option<Self>, NativeError> {
        request.validate_pathfinder_dispatch()?;
        let Some(dispatch) = request.pathfinder_dispatch else {
            return Ok(None);
        };
        let Some(source) = profile.pathfinder_configuration.as_ref() else {
            return Ok(None);
        };
        owner.validate_for(profile, npc)?;
        let configuration = source.constructor_binding(profile, npc.spawn)?;
        let identity = request.binding.identity;
        if identity.creature != npc.spawn
            || identity.map != profile.map_id
            || identity.content.0 != profile.content_digest
            || identity.geometry.0 != profile.geometry_revision
        {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(Some(Self {
            evidence: DispatchEvidence {
                policy: PATHFINDER_DISPATCH_POLICY.into(),
                profile_binding: profile.digest(),
                configuration_binding: configuration.source_binding(),
                npc,
                world,
                dispatch_tick,
                dispatch,
                request_binding: crate::digest(
                    b"benilla-pathfinder-dispatch-request-v1\0",
                    request,
                ),
            },
            live: Some(Arc::new(AtomicBool::new(false))),
        }))
    }
    pub fn world(&self) -> crate::WorldId {
        self.evidence.world
    }
    pub fn dispatch_tick(&self) -> u64 {
        self.evidence.dispatch_tick
    }
    pub fn npc(&self) -> crate::NpcId {
        self.evidence.npc
    }
    pub fn constructor_kind(&self) -> m::PathfinderConstructorKind {
        self.evidence.dispatch.constructor_kind()
    }
    pub fn construction_binding(&self) -> m::QueryBinding {
        self.evidence.dispatch.construction_binding()
    }
    pub fn dispatch_binding(&self) -> m::QueryBinding {
        self.evidence.dispatch.dispatch_binding()
    }
    pub fn first_construction(&self) -> bool {
        self.evidence.dispatch.first_construction()
    }
    pub fn configuration_binding(&self) -> crate::Digest {
        self.evidence.configuration_binding
    }
    pub fn profile_binding(&self) -> crate::Digest {
        self.evidence.profile_binding
    }
    pub fn validate_for(&self, profile: &Profile, npc: crate::NpcId) -> Result<(), NativeError> {
        let source =
            profile
                .pathfinder_configuration
                .as_ref()
                .ok_or(NativeError::AuthorityUnavailable(
                    "selected pathfinder configuration",
                ))?;
        let configuration = source.constructor_binding(profile, npc.spawn)?;
        let binding = self.dispatch_binding();
        self.evidence.dispatch.validate_for(binding)?;
        if self.evidence.policy != PATHFINDER_DISPATCH_POLICY
            || self.evidence.profile_binding != profile.digest()
            || self.evidence.configuration_binding != configuration.source_binding()
            || self.evidence.npc != npc
            || binding.identity.creature != npc.spawn
            || binding.identity.map != profile.map_id
            || binding.identity.content.0 != profile.content_digest
            || binding.identity.geometry.0 != profile.geometry_revision
        {
            return Err(NativeError::AuthorityBinding);
        }
        if self.live.is_none() {
            return Err(NativeError::AuthorityUnavailable(
                "live pathfinder dispatch",
            ));
        }
        Ok(())
    }
    pub fn validate_for_world(
        &self,
        profile: &Profile,
        world: crate::WorldId,
        npc: crate::NpcId,
    ) -> Result<(), NativeError> {
        self.validate_for(profile, npc)?;
        if self.world() != world {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
    /// Validate the actual outer runtime callback too, so a same-profile world or
    /// tick cannot relabel a cloned live event before its first query claim.
    pub fn validate_for_context(
        &self,
        profile: &Profile,
        binding: &Binding,
    ) -> Result<(), NativeError> {
        self.validate_for_world(
            profile,
            binding.world,
            binding.npc.ok_or(NativeError::AuthorityBinding)?,
        )?;
        if binding.tick != self.dispatch_tick()
            || binding.profile != profile.digest()
            || binding.authority_policy != profile.authority_policy
            || binding.geometry != profile.geometry_revision
        {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
    // Called only immediately after CreatureRouteRequest validated this exact
    // receipt against the same immutable profile. No validation survives a yield.
    fn validate_context_after_request(
        &self,
        profile: &Profile,
        binding: &Binding,
    ) -> Result<(), NativeError> {
        if binding.npc != Some(self.npc()) {
            // A different/missing outer NPC must keep the full validator's error
            // order (including configuration lookup), not just a field mismatch.
            return self.validate_for_context(profile, binding);
        }
        if self.world() != binding.world
            || binding.tick != self.dispatch_tick()
            || binding.profile != self.evidence.profile_binding
            || binding.authority_policy != profile.authority_policy
            || binding.geometry != profile.geometry_revision
        {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
    /// Claim only after validating the request and the already loaded query. A
    /// second provider cannot consume a cloned first event for an equal-byte reload.
    pub fn claim_constructor_query(&self) -> Result<(), NativeError> {
        if !self.first_construction() {
            return Err(NativeError::AuthorityBinding);
        }
        self.live
            .as_ref()
            .ok_or(NativeError::AuthorityUnavailable(
                "live pathfinder dispatch",
            ))?
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ())
            .map_err(|_| NativeError::AuthorityBinding)
    }
    /// A provider may recognize its already consumed event. A regenerated first
    /// event after transaction rollback has a new seal and must be claimed too,
    /// while retaining the provider's original query allocation for that identity.
    pub fn same_live_dispatch(&self, other: &Self) -> bool {
        self.evidence == other.evidence
            && self
                .live
                .as_ref()
                .zip(other.live.as_ref())
                .is_some_and(|(a, b)| Arc::ptr_eq(a, b))
    }
}
impl CreatureRouteRequest {
    /// Live event validation only. The caller must separately retain the actual
    /// native constructor query; serialized snapshots/hashes cannot supply it.
    pub fn require_pathfinder_dispatch(
        &self,
        profile: &Profile,
    ) -> Result<&PathfinderDispatchReceipt, NativeError> {
        self.owner.validate_for(profile, self.owner.npc())?;
        self.request.validate_pathfinder_dispatch()?;
        let receipt =
            self.pathfinder_dispatch
                .as_ref()
                .ok_or(NativeError::AuthorityUnavailable(
                    "owned pathfinder constructor dispatch",
                ))?;
        receipt.validate_for(profile, self.owner.npc())?;
        if self.request.pathfinder_dispatch != Some(receipt.evidence.dispatch)
            || receipt.evidence.request_binding
                != crate::digest(b"benilla-pathfinder-dispatch-request-v1\0", &self.request)
            || self.owner_position != self.request.origin
        {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(receipt)
    }
}

impl AuthorityRequest {
    /// Full live dispatch check, including the outer world/tick callback binding.
    pub fn require_pathfinder_dispatch(
        &self,
        profile: &Profile,
    ) -> Result<&PathfinderDispatchReceipt, NativeError> {
        let Query::SourceMotion(MotionQuery::Route(request)) = &self.query else {
            return Err(NativeError::AuthorityBinding);
        };
        let receipt = request.require_pathfinder_dispatch(profile)?;
        receipt.validate_context_after_request(profile, &self.binding)?;
        Ok(receipt)
    }
}

#[cfg(test)]
#[path = "pathfinder_dispatch_tests.rs"]
mod tests;
