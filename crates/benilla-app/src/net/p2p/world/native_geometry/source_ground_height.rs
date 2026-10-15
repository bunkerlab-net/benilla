// SPDX-License-Identifier: GPL-3.0-or-later
//! Product-backed ground allowed-Z subquery for a future NearPoint provider.
//! The scalar query accepts explicit XY; the initial-candidate query derives XY
//! under a pinned arithmetic policy. Neither supplies LOS selection, a complete
//! movement fact or admission.
use super::{
    core::{Failure, Session},
    products::ground_height::{
        allowed_ground_z, MapHeightRequest, MapHeightSnapshot, SnapshotBinding,
    },
};
use benilla_sim::creature_motion::NearPointAngle;
use benilla_world_runtime::native::{
    AllowedPositionZBranch, AuthorityRequest, MotionQuery, Profile, Query, SOLO_UNIT,
};
use sha2::{Digest, Sha256};
use std::time::Instant;

#[path = "source_ground_height/initial_candidate.rs"]
mod initial_candidate;

pub(crate) struct SourceGroundHeightProvider<'a> {
    session: Session,
    profile: Profile,
    snapshot: MapHeightSnapshot<'a>,
}
/// The full originating request is retained, including both poses, source witness,
/// authority identities, generation, clock and query ID. A ticket names one probe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GroundHeightRequest {
    pub snapshot: SnapshotBinding,
    pub context: AuthorityRequest,
    pub ticket: u64,
    pub position: [u32; 3],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GroundHeightReply {
    pub request: GroundHeightRequest,
    /// Raw Map::GetHeight bits; an invalid source height is not an error or support.
    pub ground_height: u32,
    /// Original candidate Z bits are retained on equality or invalid source height.
    pub allowed_z: u32,
}
pub(crate) struct BoundGroundHeightQuery<'a, 'products> {
    provider: &'a SourceGroundHeightProvider<'products>,
    context: AuthorityRequest,
}
impl<'products> SourceGroundHeightProvider<'products> {
    pub(crate) fn bind(
        session: &Session,
        profile: &Profile,
        snapshot: MapHeightSnapshot<'products>,
    ) -> Result<Self, Failure> {
        profile.validate().map_err(|_| Failure::Binding)?;
        let b = snapshot.binding();
        if session.world == [0; 16]
            || session.profile != profile.digest()
            || session.policy != profile.authority_policy
            || session.map != profile.map_id
            || session.content != profile.content_digest
            || session.geometry != profile.geometry_revision
            || session.geometry != geometry_revision(b)
            || b.map != session.map
            || b.content != session.content
        {
            return Err(Failure::Binding);
        }
        if profile.creature_capabilities.is_none() || profile.source_motion.is_none() {
            return Err(Failure::UnresolvedBody);
        }
        if !session
            .participants
            .as_ref()
            .is_some_and(|p| p.matches(b.content, b.map))
        {
            return Err(Failure::UnprovenParticipants);
        }
        Ok(Self {
            session: session.clone(),
            profile: profile.clone(),
            snapshot,
        })
    }
    pub(crate) fn binding(&self) -> SnapshotBinding {
        self.snapshot.binding()
    }
    /// Freeze one actual authority request before querying any candidate. The runtime
    /// constructs and checks its witness from staged owned state during resolution;
    /// this adapter validates immutable source consistency, not committed-state liveness.
    pub(crate) fn begin(
        &self,
        session: &Session,
        context: &AuthorityRequest,
    ) -> Result<BoundGroundHeightQuery<'_, 'products>, Failure> {
        if session.world != self.session.world
            || session.profile != self.session.profile
            || session.policy != self.session.policy
            || session.content != self.session.content
            || session.geometry != self.session.geometry
            || session.map != self.session.map
            || session.bodies != self.session.bodies
            || session.player != self.session.player
            || session.static_los != self.session.static_los
        {
            return Err(Failure::Binding);
        }
        if !session
            .participants
            .as_ref()
            .is_some_and(|p| p.matches(session.content, session.map))
        {
            return Err(Failure::UnprovenParticipants);
        }
        let b = &context.binding;
        if b.world != session.world
            || b.profile != session.profile
            || b.authority_policy != session.policy
            || b.geometry != session.geometry
            || b.tick == 0
        {
            return Err(Failure::Binding);
        }
        let Query::SourceMotion(MotionQuery::NearPoint(near)) = &context.query else {
            return Err(Failure::UnsupportedQuery);
        };
        let npc = b.npc.ok_or(Failure::Binding)?;
        near.searcher
            .validate_for(&self.profile, npc)
            .map_err(|_| Failure::Binding)?;
        if near
            .searcher
            .allowed_position_z_branch()
            .map_err(|_| Failure::Binding)?
            != AllowedPositionZBranch::Ground
        {
            return Err(Failure::UnsupportedQuery);
        }
        let q = &near.request;
        let now = b
            .tick
            .checked_mul(u64::from(self.profile.tick_ms))
            .and_then(|elapsed| self.profile.clock_origin_ms.checked_add(elapsed))
            .ok_or(Failure::Invalid)?;
        if q.binding.identity.creature != npc.spawn
            || q.binding.identity.map != session.map
            || q.binding.identity.content.0 != session.content
            || q.binding.identity.geometry.0 != session.geometry
            || q.binding.query_id == 0
            || q.binding.now_ms != now
            || q.target.id != SOLO_UNIT
        {
            return Err(Failure::Binding);
        }
        q.owner.validate().map_err(|_| Failure::Invalid)?;
        q.target.pose.validate().map_err(|_| Failure::Invalid)?;
        if !q.distance.as_f32().is_finite() || q.distance.as_f32() < 0.0 {
            return Err(Failure::Invalid);
        }
        if let NearPointAngle::Absolute(angle) = q.angle {
            if !angle.as_f32().is_finite() {
                return Err(Failure::Invalid);
            }
        }
        let npc_profile = self.profile.npc(npc.spawn).ok_or(Failure::Binding)?;
        let motion = self
            .profile
            .source_motion
            .as_ref()
            .ok_or(Failure::UnresolvedBody)?;
        let body = session
            .bodies
            .get(&npc.spawn)
            .ok_or(Failure::UnresolvedBody)?;
        let player = session.player.ok_or(Failure::UnresolvedBody)?;
        if body.radius != npc_profile.ai.bounding_radius
            || body.height != motion.npc_policy.settings.collision_height
            || q.owner_bounding_radius != body.radius
            || player.body.radius != self.profile.player.bounding_radius
            || player.body.height != motion.player_collision_height
            || q.target.bounding_radius != player.body.radius
            || q.target.collision_height != player.body.height
            || q.target.combat_reach != self.profile.player.combat_reach
        {
            return Err(Failure::UnresolvedBody);
        }
        Ok(BoundGroundHeightQuery {
            provider: self,
            context: context.clone(),
        })
    }
}
impl BoundGroundHeightQuery<'_, '_> {
    /// First/reset candidate only: GetNearPointAt initializes Z from target posZ.
    /// Its later fallback can reuse a previous adjusted Z and needs a separate
    /// sequential proof; an arbitrary candidate Z cannot masquerade as this phase.
    pub(crate) fn query_initial_candidate(
        &self,
        request: GroundHeightRequest,
        deadline: Instant,
    ) -> Result<GroundHeightReply, Failure> {
        if request.context != self.context
            || request.snapshot != self.provider.binding()
            || request.ticket == 0
        {
            return Err(Failure::Binding);
        }
        let Query::SourceMotion(MotionQuery::NearPoint(near)) = &self.context.query else {
            return Err(Failure::UnsupportedQuery);
        };
        if request.position[2] != near.request.target.pose.position.z.as_f32().to_bits() {
            return Err(Failure::Binding);
        }
        let query = MapHeightRequest {
            snapshot: request.snapshot,
            ticket: request.ticket,
            position: request.position,
            swim: false,
        };
        let reply = self
            .provider
            .snapshot
            .query(query, deadline)
            .map_err(|_| Failure::MissingGeometry)?;
        if reply.request != query {
            return Err(Failure::Binding);
        }
        let allowed_z = allowed_ground_z(
            f32::from_bits(request.position[2]),
            f32::from_bits(reply.height),
        )
        .to_bits();
        Ok(GroundHeightReply {
            request,
            ground_height: reply.height,
            allowed_z,
        })
    }
}

/// A distinct spatial revision; a complete future provider must compose all its
/// query policies rather than treating this scalar dependency as full navigation.
pub(crate) fn geometry_revision(b: SnapshotBinding) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"benilla-source-ground-initial-candidate-subquery-v2");
    h.update(initial_candidate::policy_digest());
    h.update(b.map.to_le_bytes());
    h.update(b.content);
    h.update(b.source_epoch);
    h.update(b.inventory);
    h.update(b.policy);
    h.finalize().into()
}

#[cfg(test)]
#[path = "source_ground_height/tests.rs"]
mod tests;
