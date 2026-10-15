// SPDX-License-Identifier: GPL-3.0-or-later
//! Loaded-product PathFinder createFilter/updateFilter dependency for an owned
//! ground-only creature. This does not supply a route, movement fact or admission.
use super::{
    core::{Failure, Session},
    products::liquid::{
        DryLiquidSnapshot, LiquidReply, LiquidRequest, LiquidStatus, SnapshotBinding,
    },
};
use benilla_sim::{creature_ai::Point, creature_motion as motion, route_filter};
use benilla_world_runtime::native::{
    AuthorityRequest, MotionQuery, Profile, Query, CREATURE_CAPABILITY_POLICY,
    CREATURE_CAPABILITY_RUNTIME_SOURCE_SHA256, SOLO_UNIT,
};
use sha2::{Digest, Sha256};
use std::time::Instant;

pub(crate) struct SourceRouteFilterProvider {
    session: Session,
    profile: Profile,
    snapshot: DryLiquidSnapshot,
}

/// Freeze the complete runtime request, including the source capability/catalog,
/// current owner pose, outer generation, clock and route endpoints. Tickets name
/// the two liquid observations, never an invented navigation path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RouteFilterRequest {
    pub snapshot: SnapshotBinding,
    pub context: AuthorityRequest,
    pub first_ticket: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RouteLiquidObservation {
    pub mask: route_filter::LiquidMask,
    pub reply: LiquidReply,
}

/// Private construction preserves the distinction between a filter prerequisite
/// and a caller-authored pair of liquid statuses. There is no Facts conversion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RouteFilterDependency {
    request: RouteFilterRequest,
    owner_position: [u32; 3],
    liquid: [RouteLiquidObservation; 2],
    filter: route_filter::Filter,
}

impl RouteFilterDependency {
    pub(crate) fn request(&self) -> &RouteFilterRequest {
        &self.request
    }
    pub(crate) fn owner_position(&self) -> [u32; 3] {
        self.owner_position
    }
    pub(crate) fn liquid_observations(&self) -> &[RouteLiquidObservation; 2] {
        &self.liquid
    }
    pub(crate) fn filter(&self) -> route_filter::Filter {
        self.filter
    }
}

pub(crate) struct BoundRouteFilterQuery<'a> {
    provider: &'a SourceRouteFilterProvider,
    context: AuthorityRequest,
}

impl SourceRouteFilterProvider {
    pub(crate) fn bind(
        session: &Session,
        profile: &Profile,
        snapshot: DryLiquidSnapshot,
    ) -> Result<Self, Failure> {
        let expected = geometry_revision(snapshot.binding());
        Self::bind_revision(session, profile, snapshot, expected)
    }

    /// Only this typed source-stage composition may select the combined policy.
    /// Callers cannot substitute an arbitrary expected geometry hash.
    pub(super) fn bind_with_smoothing(
        session: &Session,
        profile: &Profile,
        snapshot: DryLiquidSnapshot,
        navigation: super::products::navigation::SnapshotBinding,
        configuration: super::source_route_smoothing::SourceRouteSmoothingConfiguration,
    ) -> Result<Self, Failure> {
        let expected = super::source_route_smoothing::geometry_revision(
            snapshot.binding(),
            navigation,
            configuration,
        )?;
        Self::bind_revision(session, profile, snapshot, expected)
    }

    /// Typed warm-cache composition retains every original owner/participant guard.
    pub(super) fn bind_with_warm_cache(
        session: &Session,
        profile: &Profile,
        snapshot: DryLiquidSnapshot,
        navigation: super::products::navigation::SnapshotBinding,
        configuration: super::source_route_warm_cache::WarmCacheConfiguration,
    ) -> Result<Self, Failure> {
        let expected = super::source_route_warm_cache::geometry_revision(
            snapshot.binding(),
            navigation,
            configuration,
        )?;
        Self::bind_revision(session, profile, snapshot, expected)
    }

    fn bind_revision(
        session: &Session,
        profile: &Profile,
        snapshot: DryLiquidSnapshot,
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
        require_participants(session)?;
        Ok(Self {
            session: session.clone(),
            profile: profile.clone(),
            snapshot,
        })
    }

    pub(crate) fn binding(&self) -> SnapshotBinding {
        self.snapshot.binding()
    }

    /// Validate source consistency against the selected immutable profile. The
    /// runtime owns staged liveness; a deserialized request cannot establish it.
    pub(crate) fn begin(
        &self,
        session: &Session,
        context: &AuthorityRequest,
    ) -> Result<BoundRouteFilterQuery<'_>, Failure> {
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
        {
            return Err(Failure::Binding);
        }
        require_participants(session)?;
        let b = &context.binding;
        if b.world != session.world
            || b.profile != session.profile
            || b.authority_policy != session.policy
            || b.geometry != session.geometry
            || b.tick == 0
        {
            return Err(Failure::Binding);
        }
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::UnsupportedQuery);
        };
        let npc = b.npc.ok_or(Failure::Binding)?;
        route
            .owner
            .validate_for(&self.profile, npc)
            .map_err(|_| Failure::Binding)?;
        let q = &route.request;
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
            || route.owner_position != q.origin
        {
            return Err(Failure::Binding);
        }
        for point in [route.owner_position, q.origin, q.launch_origin, q.goal] {
            Point::new(point.x.as_f32(), point.y.as_f32(), point.z.as_f32())
                .map_err(|_| Failure::Invalid)?;
        }
        if !(2..=motion::MAX_PATH_POINTS).contains(&usize::from(q.point_limit)) {
            return Err(Failure::Invalid);
        }
        let capabilities = route.owner.capabilities().map_err(|_| Failure::Binding)?;
        if !capabilities.can_walk() || capabilities.can_swim() || capabilities.can_fly() {
            return Err(Failure::UnsupportedQuery);
        }
        let source = self
            .profile
            .source_motion
            .as_ref()
            .ok_or(Failure::UnresolvedBody)?;
        let npc_profile = self.profile.npc(npc.spawn).ok_or(Failure::Binding)?;
        let body = session
            .bodies
            .get(&npc.spawn)
            .ok_or(Failure::UnresolvedBody)?;
        if body.radius != npc_profile.ai.bounding_radius
            || body.height != source.npc_policy.settings.collision_height
            || q.owner_collision_height != body.height
        {
            return Err(Failure::UnresolvedBody);
        }
        if let Some(target) = q.target {
            let player = session.player.ok_or(Failure::UnresolvedBody)?;
            if target.id != SOLO_UNIT {
                return Err(Failure::Binding);
            }
            target.pose.validate().map_err(|_| Failure::Invalid)?;
            if player.body.radius != self.profile.player.bounding_radius
                || player.body.height != source.player_collision_height
                || target.bounding_radius != player.body.radius
                || target.collision_height != player.body.height
                || target.combat_reach != self.profile.player.combat_reach
            {
                return Err(Failure::UnresolvedBody);
            }
        }
        Ok(BoundRouteFilterQuery {
            provider: self,
            context: context.clone(),
        })
    }
}

impl BoundRouteFilterQuery<'_> {
    pub(crate) fn query(
        &self,
        request: RouteFilterRequest,
        deadline: Instant,
    ) -> Result<RouteFilterDependency, Failure> {
        self.query_observed(request, deadline, |_| {})
    }

    /// Observers inspect actual loaded queries only; they cannot replace answers.
    fn query_observed(
        &self,
        request: RouteFilterRequest,
        deadline: Instant,
        mut observe: impl FnMut(&RouteLiquidObservation),
    ) -> Result<RouteFilterDependency, Failure> {
        if request.context != self.context
            || request.snapshot != self.provider.binding()
            || request.first_ticket == 0
        {
            return Err(Failure::Binding);
        }
        let second_ticket = request
            .first_ticket
            .checked_add(1)
            .ok_or(Failure::Invalid)?;
        if Instant::now() >= deadline {
            return Err(Failure::MissingGeometry);
        }
        let Query::SourceMotion(MotionQuery::Route(route)) = &self.context.query else {
            return Err(Failure::UnsupportedQuery);
        };
        let point = route.owner_position;
        let position = [point.x, point.y, point.z].map(|v| v.as_f32().to_bits());
        let capabilities = route.owner.capabilities().map_err(|_| Failure::Binding)?;
        let mut liquid = [None, None];
        let mut next = 0;
        let filter = route_filter::create_ground_filter(capabilities, |mask| {
            let expected = match next {
                0 => (route_filter::LiquidMask::AllLiquids, request.first_ticket),
                1 => (route_filter::LiquidMask::WaterOrOcean, second_ticket),
                _ => return Err(Failure::Binding),
            };
            if mask != expected.0 {
                return Err(Failure::Binding);
            }
            let query = LiquidRequest {
                snapshot: request.snapshot,
                ticket: expected.1,
                position,
            };
            let reply = self
                .provider
                .snapshot
                .query(query, deadline)
                .map_err(|_| Failure::MissingGeometry)?;
            if reply.request != query {
                return Err(Failure::Binding);
            }
            // DryLiquidSnapshot proves the actual loaded GridMap exists here, so
            // GridMap.cpp:797 CanCheckLiquidLevel is true even without height mode.
            // The source NO_WATER branch is invariant under these two masks, but
            // Unit.cpp:4467/4477 still performs both queries in short-circuit order.
            let status = match reply.status {
                LiquidStatus::NoWater => 0,
            };
            let dry = route_filter::NoWater::try_from_status(status).map_err(|_| Failure::Water)?;
            let observation = RouteLiquidObservation { mask, reply };
            observe(&observation);
            liquid[next] = Some(observation);
            next += 1;
            Ok(dry)
        })
        .map_err(|error| match error {
            route_filter::Error::UnsupportedCapabilities => Failure::UnsupportedQuery,
            route_filter::Error::Liquid(error) => error,
        })?;
        if next != 2 {
            return Err(Failure::Binding);
        }
        if Instant::now() >= deadline {
            return Err(Failure::MissingGeometry);
        }
        let [first, second] = liquid;
        Ok(RouteFilterDependency {
            request,
            owner_position: position,
            liquid: [
                first.ok_or(Failure::Binding)?,
                second.ok_or(Failure::Binding)?,
            ],
            filter,
        })
    }
}

fn require_participants(session: &Session) -> Result<(), Failure> {
    if session
        .participants
        .as_ref()
        .is_some_and(|proof| proof.matches(session.content, session.map))
    {
        Ok(())
    } else {
        Err(Failure::UnprovenParticipants)
    }
}

/// This filter-only revision cannot masquerade as ground, VMap or Detour policy.
/// Inventory hashes include the recorded effective config and exact product bytes;
/// consistency is not an authenticated configuration or live source-epoch lease.
pub(crate) fn geometry_revision(b: SnapshotBinding) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"benilla-source-route-owner-dry-filter-dependency-v1");
    for policy in [route_filter::POLICY, CREATURE_CAPABILITY_POLICY] {
        hash.update((policy.len() as u64).to_le_bytes());
        hash.update(policy.as_bytes());
    }
    for source in route_filter::SOURCE_SHA256
        .iter()
        .chain(CREATURE_CAPABILITY_RUNTIME_SOURCE_SHA256.iter())
    {
        hash.update(source.as_bytes());
    }
    hash.update(b.map.to_le_bytes());
    hash.update(b.content);
    hash.update(b.source_epoch);
    hash.update(b.inventory);
    hash.update(b.policy);
    hash.finalize().into()
}

#[cfg(test)]
#[path = "source_route_filter/tests.rs"]
mod tests;
