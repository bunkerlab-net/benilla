// SPDX-License-Identifier: GPL-3.0-or-later
//! Product-backed ground and physically dry swimming-capable allowed-Z subqueries.
//! Initial candidates enforce target Z; the empty-selector capability derives
//! source-order probes and the final carried Z. No ground reply supplies LOS
//! selection, a complete movement fact or admission.
use super::{
    core::{Failure, NearPointJob, Session},
    products::ground_height::{
        allowed_ground_z, MapHeightReply, MapHeightRequest, MapHeightSnapshot, SnapshotBinding,
        SwimmingAllowedZReply, SwimmingAllowedZRequest,
    },
};
use benilla_sim::creature_motion::NearPointAngle;
use benilla_world_runtime::native::{
    AllowedPositionZBranch, AuthorityRequest, MotionQuery, Profile, Query, SOLO_UNIT,
};
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Instant};

#[path = "source_ground_height/empty_selector.rs"]
mod empty_selector;
#[path = "source_ground_height/initial_candidate.rs"]
mod initial_candidate;
pub(crate) use empty_selector::{
    EmptySelectorGroundSequence, SequentialGroundPhase, SequentialGroundReply,
};

pub(crate) struct SourceGroundHeightProvider<'a> {
    session: Session,
    profile: Profile,
    snapshot: Arc<MapHeightSnapshot<'a>>,
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
    pub source: AllowedHeightObservation,
    /// Raw Map::GetHeight bits; an invalid source height is not an error or support.
    pub ground_height: u32,
    /// Original candidate Z bits are retained on equality or invalid source height.
    pub allowed_z: u32,
}
/// Explicit source branch and complete observations; equal dry answers do not
/// turn a swimming-capable call into the ground-only path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AllowedHeightObservation {
    Ground(MapHeightReply),
    Swimming(Box<SwimmingAllowedZReply>),
}
pub(crate) struct BoundGroundHeightQuery<'a, 'products> {
    provider: &'a SourceGroundHeightProvider<'products>,
    context: AuthorityRequest,
    branch: AllowedPositionZBranch,
    collision_height: u32,
}
impl<'products> SourceGroundHeightProvider<'products> {
    pub(crate) fn bind(
        session: &Session,
        profile: &Profile,
        snapshot: MapHeightSnapshot<'products>,
    ) -> Result<Self, Failure> {
        let expected = geometry_revision(snapshot.binding());
        Self::bind_revision(session, profile, snapshot, expected)
    }
    pub(super) fn bind_with_primary(
        session: &Session,
        profile: &Profile,
        snapshot: MapHeightSnapshot<'products>,
        config: super::source_near_point::SourceNearPointConfiguration,
    ) -> Result<Self, Failure> {
        let expected = super::source_near_point::geometry_revision(&snapshot, config)?;
        Self::bind_revision(session, profile, snapshot, expected)
    }
    pub(super) fn bind_with_empty_selector(
        session: &Session,
        profile: &Profile,
        snapshot: MapHeightSnapshot<'products>,
        config: super::source_near_point::SourceNearPointConfiguration,
    ) -> Result<Self, Failure> {
        let expected = super::source_near_point_empty::geometry_revision(&snapshot, config)?;
        Self::bind_revision(session, profile, snapshot, expected)
    }
    pub(super) fn bind_with_same_polygon(
        session: &Session,
        profile: &Profile,
        snapshot: Arc<MapHeightSnapshot<'products>>,
        config: super::source_near_point::SourceNearPointConfiguration,
        binding: super::source_route_same_polygon::SamePolygonBinding,
    ) -> Result<Self, Failure> {
        binding.check_near_point(&snapshot, config)?;
        Self::bind_revision_shared(session, profile, snapshot, binding.geometry_revision()?)
    }
    fn bind_revision(
        session: &Session,
        profile: &Profile,
        snapshot: MapHeightSnapshot<'products>,
        expected: [u8; 32],
    ) -> Result<Self, Failure> {
        Self::bind_revision_shared(session, profile, Arc::new(snapshot), expected)
    }
    fn bind_revision_shared(
        session: &Session,
        profile: &Profile,
        snapshot: Arc<MapHeightSnapshot<'products>>,
        expected: [u8; 32],
    ) -> Result<Self, Failure> {
        profile.validate().map_err(|_| Failure::Binding)?;
        let b = snapshot.binding();
        if session.world == [0; 16]
            || session.profile != profile.digest()
            || session.policy != profile.authority_policy
            || session.map != profile.map_id
            || session.content != profile.content_digest
            || session.geometry != profile.geometry_revision
            || session.geometry != expected
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
    pub(super) fn static_vmap(&self) -> &super::products::vmap::VmapSnapshot {
        self.snapshot.static_vmap()
    }
    /// Freeze one actual authority request before querying any candidate. The runtime
    /// constructs and checks its witness from staged owned state during resolution;
    /// this adapter validates immutable source consistency, not committed-state liveness.
    pub(crate) fn begin(
        &self,
        session: &Session,
        context: &AuthorityRequest,
    ) -> Result<BoundGroundHeightQuery<'_, 'products>, Failure> {
        self.begin_with_searcher_validation(session, context, || {
            let Query::SourceMotion(MotionQuery::NearPoint(near)) = &context.query else {
                return Err(Failure::UnsupportedQuery);
            };
            near.searcher
                .validate_context(&self.profile, &context.binding)
                .map_err(|_| Failure::Binding)
        })
    }

    /// Consume completed searcher validation at the same point in the check order.
    pub(super) fn begin_validated(
        &self,
        job: &NearPointJob<'_>,
    ) -> Result<BoundGroundHeightQuery<'_, 'products>, Failure> {
        self.begin_with_searcher_validation(job.session(), job.request(), || {
            job.require_profile(&self.profile)
        })
    }

    fn begin_with_searcher_validation(
        &self,
        session: &Session,
        context: &AuthorityRequest,
        validate_searcher: impl FnOnce() -> Result<(), Failure>,
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
            || session.primary_near_point != self.session.primary_near_point
            || session.empty_selector_near_point != self.session.empty_selector_near_point
            || session.player_environment != self.session.player_environment
            || session.source_location != self.session.source_location
            || session.same_polygon_route != self.session.same_polygon_route
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
        validate_searcher()?;
        let branch = near
            .searcher
            .allowed_position_z_branch()
            .map_err(|_| Failure::Binding)?;
        if branch == AllowedPositionZBranch::Flying {
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
        let npc_motion = motion.npc(npc.spawn).map_err(|_| Failure::UnresolvedBody)?;
        let body = session
            .bodies
            .get(&npc.spawn)
            .ok_or(Failure::UnresolvedBody)?;
        let player = session.player.ok_or(Failure::UnresolvedBody)?;
        if body.radius != npc_profile.ai.bounding_radius
            || body.height != npc_motion.npc_policy.settings.collision_height
            || q.owner_bounding_radius != body.radius
            || player.body.radius != self.profile.player.bounding_radius
            || player.body.height != motion.player_collision_height
            || q.target.bounding_radius != player.body.radius
            || q.target.collision_height != player.body.height
            || q.target.combat_reach != self.profile.player.combat_reach
        {
            return Err(Failure::UnresolvedBody);
        }
        let collision_height = if branch == AllowedPositionZBranch::Swimming {
            // The capability witness retains the selected model for this exact life.
            // A respawn may select a height different from the initial session body.
            let initialization = near.searcher.initialization().ok_or(Failure::Binding)?;
            if initialization.request.snapshot
                != super::source_creature_initialization::snapshot_binding(&self.snapshot)
            {
                return Err(Failure::Binding);
            }
            initialization.request.collision_height.bits()
        } else {
            body.height.as_f32().to_bits()
        };
        Ok(BoundGroundHeightQuery {
            provider: self,
            context: context.clone(),
            branch,
            collision_height,
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
        self.query_initial_candidate_with_budget(
            request,
            &mut super::products::vmap::QueryBudget::new(deadline),
        )
    }
    pub(super) fn query_initial_candidate_with_budget(
        &self,
        request: GroundHeightRequest,
        budget: &mut super::products::vmap::QueryBudget,
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
        self.query_ground_with_budget(request, budget)
    }
    /// Only this module's sequential capability may supply a carried candidate Z.
    /// The initial-candidate entry point above retains its exact target-Z guard.
    fn query_ground_with_budget(
        &self,
        request: GroundHeightRequest,
        budget: &mut super::products::vmap::QueryBudget,
    ) -> Result<GroundHeightReply, Failure> {
        if request.context != self.context
            || request.snapshot != self.provider.binding()
            || request.ticket == 0
        {
            return Err(Failure::Binding);
        }
        let query = MapHeightRequest {
            snapshot: request.snapshot,
            ticket: request.ticket,
            position: request.position,
            swim: self.branch == AllowedPositionZBranch::Swimming,
        };
        let (source, ground_height, allowed_z) = match self.branch {
            AllowedPositionZBranch::Ground => {
                let reply = self
                    .provider
                    .snapshot
                    .query_with_budget(query, budget)
                    .map_err(Failure::product)?;
                if reply.request != query {
                    return Err(Failure::Binding);
                }
                let allowed_z = allowed_ground_z(
                    f32::from_bits(request.position[2]),
                    f32::from_bits(reply.height),
                )
                .to_bits();
                (
                    AllowedHeightObservation::Ground(reply),
                    reply.height,
                    allowed_z,
                )
            }
            AllowedPositionZBranch::Swimming => {
                let swimming = SwimmingAllowedZRequest {
                    height: query,
                    collision_height: self.collision_height,
                };
                let reply = self
                    .provider
                    .snapshot
                    .query_swimming_allowed_z_with_budget(swimming, budget)
                    .map_err(Failure::product)?;
                if reply.request != swimming {
                    return Err(Failure::Binding);
                }
                (
                    AllowedHeightObservation::Swimming(Box::new(reply)),
                    reply.ground.height,
                    reply.allowed_z,
                )
            }
            AllowedPositionZBranch::Flying => return Err(Failure::UnsupportedQuery),
        };
        Ok(GroundHeightReply {
            request,
            source,
            ground_height,
            allowed_z,
        })
    }
}

/// A distinct spatial revision; a complete future provider must compose all its
/// query policies rather than treating this scalar dependency as full navigation.
pub(crate) fn geometry_revision(b: SnapshotBinding) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"benilla-source-ground-or-dry-swimming-initial-candidate-subquery-v3");
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
