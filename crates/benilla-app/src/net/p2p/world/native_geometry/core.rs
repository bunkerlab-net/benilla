// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded local geometry transport, not peer authority or a navigation mesh. The only
//! movement solver is an unobstructed, continuously certified dry horizontal segment.
//! Separately selected static LOS does not imply walking support or accessibility.
use super::route_commit::{RouteCommitBoundary, RouteCommitNotice, RouteCommitSink};
use benilla_sim::creature_ai::{self as ai, movement::*};
use benilla_sim::creature_motion as motion;
use benilla_world_runtime::{
    native::{
        Authority, AuthorityReply, AuthorityRequest, CombatFacts, CombatPhase, CombatQuery, Facts,
        MotionFacts, MotionQuery, NativeError, OwnerIgnorePathfindingReceipt,
        PathfinderDispatchReceipt, Profile, Query, SourceSpellFacts, SourceSpellPhase,
        SourceSpellQuery, SOLO_UNIT,
    },
    Digest, PlayerId, WorldId,
};
use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock,
    },
    time::{Duration, Instant},
};

pub(super) const QUEUE_CAPACITY: usize = 16;
pub(super) const PER_FRAME: usize = 8;
const ROUTE_COMMIT_CAPACITY: usize = 1;
const WAIT: Duration = Duration::from_millis(500);
const MAX_SEGMENT: f32 = 8.0;

/// Resolved by a future source-audited compiler, never supplied by a network packet.
/// Collision height is distinct from combat reach; it cannot be inferred from it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Body {
    pub radius: ai::Real,
    pub height: ai::Real,
}
impl Body {
    fn validate(self) -> bool {
        (0.0..=4.0).contains(&self.radius.as_f32())
            && self.radius.as_f32() > 0.0
            && (0.0..=16.0).contains(&self.height.as_f32())
            && self.height.as_f32() > 0.0
    }
}

/// Trusted source-resolved player body and maximum speed for this bounded movement state.
/// Enabled native slow profiles supply their current source-derived ground speed in the
/// authority request. Sprint/mount/swim/flight, transports and jump/fall remain excluded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PlayerMotion {
    pub body: Body,
    pub speed: ai::Real,
}
impl PlayerMotion {
    fn validate(self) -> bool {
        self.body.validate()
            && (0.0..=MAX_SEGMENT).contains(&self.speed.as_f32())
            && self.speed.as_f32() > 0.0
    }
}

/// Closed compiler/controller proof that this content/map has no non-map collision
/// participants: game-object/door/transport spawns, runtime creation and scripted colliders
/// are excluded, and the controller cannot create them while this token is installed.
/// There is deliberately no production constructor until that complete input audit exists.
/// Empty ADT references, empty entity queries and a digest alone cannot construct this token.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct NonRenderParticipants {
    content: Digest,
    map: u32,
}
impl NonRenderParticipants {
    #[cfg(test)]
    pub(crate) fn authored_fixture(content: Digest, map: u32) -> Self {
        Self { content, map }
    }
    pub(super) fn matches(&self, content: Digest, map: u32) -> bool {
        self.content == content && self.map == map
    }
}

/// Explicit LOS-enabled policy with frozen heights from the runtime's source-motion
/// owner. This is not native admission: world construction remains capability gated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StaticLosPolicy {
    profile: Digest,
    map: u32,
    content: Digest,
    geometry: Digest,
    authority: Digest,
    player_height: ai::Real,
    npc: u64,
    npc_height: ai::Real,
}
impl StaticLosPolicy {
    #[allow(
        dead_code,
        reason = "used by app/controller integration, not the standalone geometry harness"
    )]
    pub(crate) fn from_profile(
        profile: &benilla_world_runtime::native::Profile,
        line_of_sight_enabled: bool,
    ) -> Result<Self, Failure> {
        if !line_of_sight_enabled || profile.npcs.len() != 1 {
            return Err(Failure::UnsupportedQuery);
        }
        let motion = profile
            .source_motion
            .as_ref()
            .ok_or(Failure::UnresolvedBody)?;
        let npc = profile.npcs[0].ai.creature_id;
        let npc_motion = motion.npc(npc).map_err(|_| Failure::UnresolvedBody)?;
        Ok(Self {
            profile: profile.digest(),
            map: profile.map_id,
            content: profile.content_digest,
            geometry: profile.geometry_revision,
            authority: profile.authority_policy,
            player_height: motion.player_collision_height,
            npc,
            npc_height: npc_motion.npc_policy.settings.collision_height,
        })
    }
    fn check(self, session: &Session) -> Result<(), Failure> {
        if self.profile != session.profile
            || self.map != session.map
            || self.content != session.content
            || self.geometry != session.geometry
            || self.authority != session.policy
        {
            return Err(Failure::Binding);
        }
        if session
            .player
            .is_none_or(|p| p.body.height != self.player_height)
            || session.bodies.len() != 1
            || session
                .bodies
                .get(&self.npc)
                .is_none_or(|b| b.height != self.npc_height)
        {
            return Err(Failure::UnresolvedBody);
        }
        Ok(())
    }
}

/// Explicit primary-candidate mailbox route bound to the selected source owners.
/// It grants no geometry fact: the retained product provider still validates the
/// complete source witness, candidate, ground and LOS for each staged request.
#[derive(Clone, Debug, Eq)]
pub(crate) struct PrimaryNearPointPolicy {
    profile: Arc<Profile>,
    profile_digest: Digest,
    npc_bodies: BTreeMap<u64, Body>,
    player_body: Body,
}
impl PartialEq for PrimaryNearPointPolicy {
    fn eq(&self, other: &Self) -> bool {
        self.profile_digest == other.profile_digest
            && self.npc_bodies == other.npc_bodies
            && self.player_body == other.player_body
            && (Arc::ptr_eq(&self.profile, &other.profile) || self.profile == other.profile)
    }
}
impl PrimaryNearPointPolicy {
    pub(crate) fn from_profile(profile: &Profile) -> Result<Self, Failure> {
        // Profile validation requires exact motion/capability catalog coverage and
        // the explicitly selected visitor catalog for the two-owner domain.
        profile.validate().map_err(|_| Failure::Binding)?;
        let source = profile
            .source_motion
            .as_ref()
            .ok_or(Failure::UnsupportedQuery)?;
        if profile.creature_capabilities.is_none() || !matches!(profile.npcs.len(), 1 | 2) {
            return Err(Failure::UnsupportedQuery);
        }
        let npc_bodies = profile
            .npcs
            .iter()
            .map(|npc| {
                let npc_motion = source
                    .npc(npc.ai.creature_id)
                    .map_err(|_| Failure::UnresolvedBody)?;
                let body = Body {
                    radius: npc.ai.bounding_radius,
                    height: npc_motion.npc_policy.settings.collision_height,
                };
                if !body.validate() {
                    return Err(Failure::UnresolvedBody);
                }
                Ok((npc.ai.creature_id, body))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let player_body = Body {
            radius: profile.player.bounding_radius,
            height: source.player_collision_height,
        };
        if !player_body.validate() {
            return Err(Failure::UnresolvedBody);
        }
        Ok(Self {
            profile: Arc::new(profile.clone()),
            profile_digest: profile.digest(),
            npc_bodies,
            player_body,
        })
    }
    fn check(&self, session: &Session) -> Result<(), Failure> {
        let profile = &self.profile;
        if self.profile_digest != session.profile
            || profile.map_id != session.map
            || profile.content_digest != session.content
            || profile.geometry_revision != session.geometry
            || profile.authority_policy != session.policy
        {
            return Err(Failure::Binding);
        }
        // Validate the whole frozen roster, including owners other than this query.
        if session.player.is_none_or(|p| p.body != self.player_body)
            || session.bodies != self.npc_bodies
        {
            return Err(Failure::UnresolvedBody);
        }
        Ok(())
    }
    fn body(&self, session: &Session, request: &AuthorityRequest) -> Result<Body, Failure> {
        self.check(session)?;
        let Query::SourceMotion(MotionQuery::NearPoint(near)) = &request.query else {
            return Err(Failure::UnsupportedQuery);
        };
        let npc = request.binding.npc.ok_or(Failure::Binding)?;
        let npc_body = self.npc_bodies.get(&npc.spawn).ok_or(Failure::Binding)?;
        // The selected source catalog and exact generation are part of the receipt;
        // matching a stable spawn or equal body dimensions cannot substitute it.
        near.searcher
            .validate_context(&self.profile, &request.binding)
            .map_err(|_| Failure::Binding)?;
        let q = &near.request;
        let profile = &self.profile;
        let now = request
            .binding
            .tick
            .checked_mul(u64::from(profile.tick_ms))
            .and_then(|elapsed| profile.clock_origin_ms.checked_add(elapsed))
            .ok_or(Failure::Invalid)?;
        if q.binding.identity.creature != npc.spawn
            || q.binding.identity.map != profile.map_id
            || q.binding.identity.content.0 != profile.content_digest
            || q.binding.identity.geometry.0 != profile.geometry_revision
            || q.binding.query_id == 0
            || q.binding.now_ms != now
            || q.target.id != SOLO_UNIT
        {
            return Err(Failure::Binding);
        }
        if q.owner_bounding_radius != npc_body.radius
            || q.target.bounding_radius != self.player_body.radius
            || q.target.collision_height != self.player_body.height
            || q.target.combat_reach != profile.player.combat_reach
        {
            return Err(Failure::UnresolvedBody);
        }
        q.owner.validate().map_err(|_| Failure::Invalid)?;
        q.target.pose.validate().map_err(|_| Failure::Invalid)?;
        if q.distance.as_f32() < 0.0 {
            return Err(Failure::Invalid);
        }
        Ok(*npc_body)
    }
}

/// Distinct empty-selector mailbox route with the same frozen source-owner contract.
/// Selecting it does not authorize an occupied selector or establish geometry parity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EmptySelectorNearPointPolicy(PrimaryNearPointPolicy);
impl EmptySelectorNearPointPolicy {
    pub(crate) fn from_profile(profile: &Profile) -> Result<Self, Failure> {
        // Fallback source-order certification is still the separate singleton slice.
        if profile.npcs.len() != 1 {
            return Err(Failure::UnsupportedQuery);
        }
        PrimaryNearPointPolicy::from_profile(profile).map(Self)
    }
    fn check(&self, session: &Session) -> Result<(), Failure> {
        self.0.check(session)
    }
    fn body(&self, session: &Session, request: &AuthorityRequest) -> Result<Body, Failure> {
        self.0.body(session, request)
    }
}

/// Explicit environment selection bound to the actual initialized model owner.
/// This equality policy cannot authenticate loaded spatial products by itself.
#[derive(Clone, Debug, Eq)]
pub(crate) struct PlayerEnvironmentPolicy {
    profile: Arc<Profile>,
    profile_digest: Digest,
    player: PlayerId,
    body: Body,
}
impl PartialEq for PlayerEnvironmentPolicy {
    fn eq(&self, other: &Self) -> bool {
        self.profile_digest == other.profile_digest
            && self.player == other.player
            && self.body == other.body
            && (Arc::ptr_eq(&self.profile, &other.profile) || self.profile == other.profile)
    }
}
impl PlayerEnvironmentPolicy {
    pub(crate) fn from_profile(profile: &Profile, player: PlayerId) -> Result<Self, Failure> {
        profile.validate().map_err(|_| Failure::Binding)?;
        profile
            .player_environment
            .as_ref()
            .ok_or(Failure::UnsupportedQuery)?;
        let height = profile
            .player_collision_height
            .as_ref()
            .ok_or(Failure::UnresolvedBody)?;
        let body = Body {
            radius: profile.player.bounding_radius,
            height: ai::Real::from_bits(height.initial_height().height().bits())
                .map_err(|_| Failure::UnresolvedBody)?,
        };
        if !body.validate() || player.owner == [0; 16] || player.character == [0; 16] {
            return Err(Failure::UnresolvedBody);
        }
        Ok(Self {
            profile: Arc::new(profile.clone()),
            profile_digest: profile.digest(),
            player,
            body,
        })
    }
    pub(super) fn check(&self, session: &Session) -> Result<(), Failure> {
        if self.profile_digest != session.profile
            || self.profile.map_id != session.map
            || self.profile.content_digest != session.content
            || self.profile.geometry_revision != session.geometry
            || self.profile.authority_policy != session.policy
        {
            return Err(Failure::Binding);
        }
        if session.player.is_none_or(|p| p.body != self.body) {
            return Err(Failure::UnresolvedBody);
        }
        Ok(())
    }
    fn body(&self, session: &Session, request: &AuthorityRequest) -> Result<Body, Failure> {
        self.check(session)?;
        let Query::PlayerEnvironment(query) = &request.query else {
            return Err(Failure::UnsupportedQuery);
        };
        if request.binding.npc.is_some() {
            return Err(Failure::Binding);
        }
        query
            .validate_for(&self.profile, session.world, self.player)
            .map_err(|_| Failure::Binding)?;
        if query.height_witness().height().bits() != self.body.height.bits() {
            return Err(Failure::UnresolvedBody);
        }
        Ok(self.body)
    }
}

/// Exact player/source selection for independent zone and area observations.
/// A selected catalog does not certify a pose; the owned provider must query it.
#[derive(Clone, Debug, Eq)]
pub(crate) struct SourceLocationPolicy {
    profile: Arc<Profile>,
    profile_digest: Digest,
    player: PlayerId,
    body: Body,
}
impl PartialEq for SourceLocationPolicy {
    fn eq(&self, other: &Self) -> bool {
        self.profile_digest == other.profile_digest
            && self.player == other.player
            && self.body == other.body
            && (Arc::ptr_eq(&self.profile, &other.profile) || self.profile == other.profile)
    }
}
impl SourceLocationPolicy {
    pub(crate) fn from_profile(profile: &Profile, player: PlayerId) -> Result<Self, Failure> {
        profile.validate().map_err(|_| Failure::Binding)?;
        profile.source_location().ok_or(Failure::UnsupportedQuery)?;
        let body = Body {
            radius: profile.player.bounding_radius,
            height: profile
                .source_motion
                .as_ref()
                .ok_or(Failure::UnresolvedBody)?
                .player_collision_height,
        };
        if !body.validate() || player.owner == [0; 16] || player.character == [0; 16] {
            return Err(Failure::UnresolvedBody);
        }
        Ok(Self {
            profile: Arc::new(profile.clone()),
            profile_digest: profile.digest(),
            player,
            body,
        })
    }
    pub(super) fn check(&self, session: &Session) -> Result<(), Failure> {
        if self.profile_digest != session.profile
            || self.profile.map_id != session.map
            || self.profile.content_digest != session.content
            || self.profile.geometry_revision != session.geometry
            || self.profile.authority_policy != session.policy
        {
            return Err(Failure::Binding);
        }
        if session.player.is_none_or(|player| player.body != self.body) {
            return Err(Failure::UnresolvedBody);
        }
        Ok(())
    }
    fn body(&self, session: &Session, request: &AuthorityRequest) -> Result<Body, Failure> {
        self.check(session)?;
        let Query::SourceLocation(query) = &request.query else {
            return Err(Failure::UnsupportedQuery);
        };
        if request.binding.npc.is_some() || request.binding.tick != query.tick() {
            return Err(Failure::Binding);
        }
        query
            .validate_for(&self.profile, session.world, self.player)
            .map_err(|_| Failure::Binding)?;
        Ok(self.body)
    }
}

/// Explicit transport capability for the bounded loaded first/retained Chase path.
/// This validates request ownership and domains, never raw geometry or native caches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SamePolygonRoutePolicy {
    environment: PlayerEnvironmentPolicy,
    binding: super::source_route_same_polygon::SamePolygonBinding,
}
impl SamePolygonRoutePolicy {
    pub(crate) fn from_profile(
        profile: &Profile,
        player: PlayerId,
        binding: super::source_route_same_polygon::SamePolygonBinding,
    ) -> Result<Self, Failure> {
        if binding.geometry_revision()? != profile.geometry_revision {
            return Err(Failure::Binding);
        }
        Ok(Self {
            environment: PlayerEnvironmentPolicy::from_profile(profile, player)?,
            binding,
        })
    }
    fn check(&self, session: &Session) -> Result<(), Failure> {
        self.environment.check(session)?;
        if session.player_environment.as_ref() != Some(&self.environment)
            || session.geometry != self.binding.geometry_revision()?
        {
            return Err(Failure::Binding);
        }
        Ok(())
    }
    fn body(&self, session: &Session, context: &AuthorityRequest) -> Result<Body, Failure> {
        self.validate_context(session, context)?;
        self.validate_model(session, context)?;
        self.resolve_owner_body(session, context)
    }
    fn validate_context(
        &self,
        session: &Session,
        context: &AuthorityRequest,
    ) -> Result<(), Failure> {
        self.check(session)?;
        let profile = &self.environment.profile;
        super::source_route_filter::validate_route_context(session, profile, context)?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::UnsupportedQuery);
        };
        let q = &route.request;
        if !matches!(
            q.kind,
            motion::RouteKind::ChaseInitial | motion::RouteKind::ChaseReplan
        ) || q.launch_origin != q.origin
            || q.point_limit != motion::PATHFINDER_CACHE_POINT_LIMIT
            || q.target.is_none()
        {
            return Err(Failure::UnsupportedQuery);
        }
        let dispatch = context
            .require_pathfinder_dispatch(profile)
            .map_err(|_| Failure::Binding)?;
        match q.prior_cache.as_deref() {
            None if dispatch.first_construction() => (),
            Some(prior) if !dispatch.first_construction() => {
                prior
                    .validate_for(q.binding.identity)
                    .map_err(|_| Failure::Binding)?;
                if prior.source != motion::RouteSource::OwnerPathFinder
                    || prior.binding.owner_generation != route.owner.npc().generation
                    || prior.binding.inventory != self.binding.navigation.inventory
                    || prior.binding.source_epoch != self.binding.navigation.source_epoch
                    || prior.binding.navigation_policy != session.geometry
                {
                    return Err(Failure::Binding);
                }
            }
            _ => return Err(Failure::Binding),
        }
        Ok(())
    }
    fn validate_model(
        &self,
        session: &Session,
        context: &AuthorityRequest,
    ) -> Result<super::source_route_chase_gate::ChaseGateModel, Failure> {
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::UnsupportedQuery);
        };
        super::source_route_chase_gate::gate_model(
            &self.environment.profile,
            session.world,
            self.environment.player,
            route,
        )
    }
    fn resolve_owner_body(
        &self,
        session: &Session,
        context: &AuthorityRequest,
    ) -> Result<Body, Failure> {
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::UnsupportedQuery);
        };
        let profile = &self.environment.profile;
        if route
            .require_owner_ignore_pathfinding(profile)
            .map_err(|_| Failure::Binding)?
            .ignore_pathfinding()
        {
            return Err(Failure::UnsupportedQuery);
        }
        let npc = context.binding.npc.ok_or(Failure::Binding)?;
        let mut body = session
            .bodies
            .get(&npc.spawn)
            .copied()
            .ok_or(Failure::UnresolvedBody)?;
        if let Some(height) = super::source_route_filter::initialized_owner_height(profile, route)?
        {
            body.height = height;
        }
        if !body.validate() {
            return Err(Failure::UnresolvedBody);
        }
        Ok(body)
    }
}

/// Local session identity. Digests bind inputs; they do not attest that client collision
/// equals CMaNGOS VMap data. Production admission must separately verify asset identity.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Session {
    pub world: WorldId,
    pub profile: Digest,
    pub policy: Digest,
    pub content: Digest,
    pub geometry: Digest,
    pub map: u32,
    pub bodies: BTreeMap<u64, Body>,
    pub player: Option<PlayerMotion>,
    pub participants: Option<NonRenderParticipants>,
    pub static_los: Option<StaticLosPolicy>,
    pub primary_near_point: Option<PrimaryNearPointPolicy>,
    pub empty_selector_near_point: Option<EmptySelectorNearPointPolicy>,
    pub player_environment: Option<PlayerEnvironmentPolicy>,
    pub source_location: Option<SourceLocationPolicy>,
    pub same_polygon_route: Option<SamePolygonRoutePolicy>,
}
/// One successful immutable body validation for these exact borrowed objects.
/// No geometry, dispatch liveness or result is certified by this attempt-local value.
pub(super) struct ValidatedRequest<'attempt> {
    session: &'attempt Session,
    request: &'attempt AuthorityRequest,
    body: Body,
}
impl<'attempt> ValidatedRequest<'attempt> {
    pub(super) fn session(&self) -> &'attempt Session {
        self.session
    }
    pub(super) fn request(&self) -> &'attempt AuthorityRequest {
        self.request
    }
    pub(super) fn body(&self) -> Body {
        self.body
    }
}
impl Session {
    pub(super) fn with_validated_request<R>(
        &self,
        request: &AuthorityRequest,
        consume: impl for<'attempt> FnOnce(ValidatedRequest<'attempt>) -> R,
    ) -> Result<R, Failure> {
        Ok(consume(self.validated_request(request)?))
    }
    fn validated_request<'attempt>(
        &'attempt self,
        request: &'attempt AuthorityRequest,
    ) -> Result<ValidatedRequest<'attempt>, Failure> {
        let body = self.body(request)?;
        Ok(ValidatedRequest {
            session: self,
            request,
            body,
        })
    }
    fn check_request_binding(&self, request: &AuthorityRequest) -> Result<(), Failure> {
        self.check_participants()?;
        let b = &request.binding;
        if b.world != self.world
            || b.profile != self.profile
            || b.authority_policy != self.policy
            || b.geometry != self.geometry
            || b.tick == 0
        {
            return Err(Failure::Binding);
        }
        Ok(())
    }
    pub(super) fn body(&self, request: &AuthorityRequest) -> Result<Body, Failure> {
        self.check_request_binding(request)?;
        let b = &request.binding;
        let m = match &request.query {
            Query::Movement(m) => m,
            Query::SourceMotion(MotionQuery::Route(_)) => {
                return self
                    .same_polygon_route
                    .as_ref()
                    .ok_or(Failure::UnsupportedQuery)?
                    .body(self, request);
            }
            Query::SourceLocation(_) => {
                return self
                    .source_location
                    .as_ref()
                    .ok_or(Failure::UnsupportedQuery)?
                    .body(self, request);
            }
            Query::PlayerEnvironment(_) => {
                return self
                    .player_environment
                    .as_ref()
                    .ok_or(Failure::UnsupportedQuery)?
                    .body(self, request);
            }
            Query::PlayerMovement {
                accepted_path,
                elapsed_ms,
                ..
            } => {
                if accepted_path.len() >= benilla_world_runtime::native::MAX_INPUTS_PER_TICK
                    || *elapsed_ms == 0
                    || *elapsed_ms > 1000
                {
                    return Err(Failure::Invalid);
                }
                if b.npc.is_some() {
                    return Err(Failure::Binding);
                }
                return self.player.map(|p| p.body).ok_or(Failure::UnresolvedBody);
            }
            Query::SourceMotion(MotionQuery::NearPoint(_)) => {
                return match (
                    self.primary_near_point.as_ref(),
                    self.empty_selector_near_point.as_ref(),
                ) {
                    (Some(policy), None) => policy.body(self, request),
                    (None, Some(policy)) => policy.body(self, request),
                    (None, None) => Err(Failure::UnsupportedQuery),
                    (Some(_), Some(_)) => Err(Failure::Invalid),
                };
            }
            Query::SourceSpell(SourceSpellQuery::LineOfSight(_))
            | Query::SourceMotion(MotionQuery::Combat(CombatQuery::NpcLineOfSight(_))) => {
                let supported = match &request.query {
                    Query::SourceSpell(SourceSpellQuery::LineOfSight(q)) => matches!(
                        q.phase,
                        SourceSpellPhase::Prepare | SourceSpellPhase::ReleaseCheck
                    ),
                    Query::SourceMotion(MotionQuery::Combat(CombatQuery::NpcLineOfSight(q))) => {
                        q.phase == CombatPhase::Sight
                    }
                    _ => false,
                };
                if !supported {
                    return Err(Failure::UnsupportedQuery);
                }
                self.static_los
                    .ok_or(Failure::UnsupportedQuery)?
                    .check(self)?;
                self.player.ok_or(Failure::UnresolvedBody)?;
                let npc = b.npc.ok_or(Failure::Binding)?;
                return self
                    .bodies
                    .get(&npc.spawn)
                    .copied()
                    .ok_or(Failure::UnresolvedBody);
            }
            _ => return Err(Failure::UnsupportedQuery),
        };
        if m.map_id != self.map
            || m.content_revision.0 != self.content
            || m.geometry_revision.0 != self.geometry
            || b.npc.is_none_or(|id| id.spawn != m.creature_id)
        {
            return Err(Failure::Binding);
        }
        self.bodies
            .get(&m.creature_id)
            .copied()
            .ok_or(Failure::UnresolvedBody)
    }
    pub(super) fn check_participants(&self) -> Result<(), Failure> {
        if self
            .participants
            .as_ref()
            .is_some_and(|proof| proof.matches(self.content, self.map))
        {
            Ok(())
        } else {
            Err(Failure::UnprovenParticipants)
        }
    }
    fn validate(&self) -> bool {
        [self.profile, self.policy, self.content, self.geometry]
            .iter()
            .all(|d| *d != [0; 32])
            && self.world != [0; 16]
            && (!self.bodies.is_empty() || self.player.is_some())
            && self.player.is_none_or(PlayerMotion::validate)
            && self.bodies.len() <= 64
            && self.bodies.iter().all(|(id, b)| *id != 0 && b.validate())
            && !(self.primary_near_point.is_some() && self.empty_selector_near_point.is_some())
            && self
                .player_environment
                .as_ref()
                .is_none_or(|p| p.check(self).is_ok())
            && self
                .source_location
                .as_ref()
                .is_none_or(|p| p.check(self).is_ok())
            && self
                .same_polygon_route
                .as_ref()
                .is_none_or(|p| p.check(self).is_ok())
            && self
                .primary_near_point
                .as_ref()
                .is_none_or(|p| p.check(self).is_ok())
            && self
                .empty_selector_near_point
                .as_ref()
                .is_none_or(|p| p.check(self).is_ok())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Failure {
    Binding,
    Invalid,
    UnresolvedBody,
    UnprovenParticipants,
    UnsupportedQuery,
    UnsupportedNavigation,
    UnsupportedNearPointFallback,
    UnsupportedPointRepresentation,
    MissingGeometry,
    /// The supplied execution deadline expired, not missing or unsupported geometry.
    Deadline,
    /// A concurrent enqueue filled the bounded FIFO before this job could yield.
    QueueCapacity,
    Obstacle,
    Water,
    UnsupportedGround,
}
impl Failure {
    pub(super) fn product(error: super::products::Error) -> Self {
        match error {
            super::products::Error::Deadline => Self::Deadline,
            _ => Self::MissingGeometry,
        }
    }
    pub(super) fn navigation(error: benilla_native_nav::Error) -> Self {
        match error {
            benilla_native_nav::Error::Deadline => Self::Deadline,
            _ => Self::MissingGeometry,
        }
    }
    fn native(self) -> NativeError {
        match self {
            Self::UnprovenParticipants => NativeError::AuthorityUnavailable(
                "non-render collision participants have not been excluded",
            ),
            Self::Binding => NativeError::AuthorityBinding,
            Self::Invalid => NativeError::InvalidInput,
            Self::UnsupportedQuery => {
                NativeError::AuthorityUnavailable("local geometry query or policy unsupported")
            }
            Self::UnresolvedBody => {
                NativeError::AuthorityUnavailable("native collision dimensions unresolved")
            }
            Self::Deadline => {
                NativeError::AuthorityUnavailable("local geometry execution deadline exceeded")
            }
            Self::QueueCapacity => NativeError::AuthorityUnavailable("local geometry queue full"),
            Self::MissingGeometry => {
                NativeError::AuthorityUnavailable("local geometry coverage incomplete")
            }
            Self::UnsupportedNearPointFallback => NativeError::AuthorityUnavailable(
                "source near-point fallback is required but not implemented",
            ),
            Self::UnsupportedPointRepresentation => NativeError::AuthorityUnavailable(
                "source near-point coordinates cannot be represented exactly",
            ),
            Self::UnsupportedNavigation => {
                NativeError::AuthorityUnavailable("outside local planar navigation domain")
            }
            Self::UnsupportedGround => {
                NativeError::AuthorityUnavailable("continuous dry planar support unproven")
            }
            Self::Water => NativeError::AuthorityUnavailable("liquid navigation unsupported"),
            Self::Obstacle => NativeError::Navigation(ai::AiError::Movement(BlockReason::Obstacle)),
        }
    }
}

/// The sole owner stays in the cross-thread Work. Read-only leases never extend
/// its lifetime, replace its attachment, or renew its original absolute expiry.
struct JobIdentity {
    ticket: u64,
    request: AuthorityRequest,
    deadline: Instant,
    attachment: Arc<AtomicBool>,
    owner_alive: AtomicBool,
}
struct JobOwner(Arc<JobIdentity>);
impl Drop for JobOwner {
    fn drop(&mut self) {
        self.0.owner_alive.store(false, Ordering::Release);
    }
}
#[derive(Clone)]
pub(super) struct JobLease(Arc<JobIdentity>);
impl JobLease {
    pub(super) fn same_job(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub(super) fn is_active(&self) -> bool {
        self.0.owner_alive.load(Ordering::Acquire)
            && self.0.attachment.load(Ordering::Acquire)
            && Instant::now() < self.0.deadline
    }
    pub(super) fn request(&self) -> &AuthorityRequest {
        &self.0.request
    }
    pub(super) fn ticket(&self) -> u64 {
        self.0.ticket
    }
    pub(super) fn deadline(&self) -> Instant {
        self.0.deadline
    }
}
struct OwnedRouteValidation {
    model: super::source_route_chase_gate::ChaseGateModel,
    session: Session,
    request: AuthorityRequest,
    body: Body,
    lease: JobLease,
    selected_provider: OnceLock<Arc<()>>,
    #[cfg(test)]
    phase: std::cell::Cell<&'static str>,
}
impl OwnedRouteValidation {
    fn check(&self, session: &Session, work: &Work) -> Result<(), Failure> {
        check_owned_route(&self.session, &self.request, &self.lease, session, work)
    }
}
fn check_owned_route(
    saved_session: &Session,
    saved_request: &AuthorityRequest,
    lease: &JobLease,
    session: &Session,
    work: &Work,
) -> Result<(), Failure> {
    if saved_session != session || saved_request != &work.request || !lease.same_job(&work.lease())
    {
        return Err(Failure::Binding);
    }
    // Equal transcript bytes cannot replace the original live dispatch allocation.
    let (
        Query::SourceMotion(MotionQuery::Route(saved)),
        Query::SourceMotion(MotionQuery::Route(current)),
    ) = (&saved_request.query, &work.request.query)
    else {
        return Err(Failure::Binding);
    };
    if !saved
        .pathfinder_dispatch
        .as_ref()
        .zip(current.pathfinder_dispatch.as_ref())
        .is_some_and(|(a, b)| a.same_live_dispatch(b))
    {
        return Err(Failure::Binding);
    }
    if !lease.is_active() {
        return Err(Failure::Deadline);
    }
    Ok(())
}

/// Completed immutable subparts remain owned by this original queued Work. No
/// provider, geometry fact, constructor or native cache can use a partial body.
struct OwnedRouteBodyProgress {
    model: Option<super::source_route_chase_gate::ChaseGateModel>,
    session: Session,
    request: AuthorityRequest,
    lease: JobLease,
    next: RouteBodyPhase,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RouteBodyPhase {
    Context,
    Model,
    OwnerState,
}
impl RouteBodyPhase {
    #[cfg(test)]
    fn name(self) -> &'static str {
        match self {
            Self::Context => "body_context_dispatch",
            Self::Model => "body_model",
            Self::OwnerState => "body_owner_state",
        }
    }
}
/// An invocation-local view of the core's completed immutable Route validation.
/// Providers can retain the read-only lease, never this borrowed receipt.
pub(super) struct RouteJob<'call> {
    validation: &'call OwnedRouteValidation,
}
impl<'call> RouteJob<'call> {
    pub(super) fn session(&self) -> &'call Session {
        &self.validation.session
    }
    pub(super) fn request(&self) -> &'call AuthorityRequest {
        &self.validation.request
    }
    pub(super) fn body(&self) -> Body {
        self.validation.body
    }
    /// Completed immutable checks belong to this original Work and its exact
    /// frozen profile, never a digest-only or deserialized validation claim.
    pub(super) fn require_profile(&self, profile: &Profile) -> Result<(), Failure> {
        if !self.validation.lease.is_active() {
            return Err(Failure::Deadline);
        }
        let policy = self
            .session()
            .same_polygon_route
            .as_ref()
            .ok_or(Failure::Binding)?;
        if *policy.environment.profile != *profile {
            return Err(Failure::Binding);
        }
        Ok(())
    }
    pub(super) fn validated_model(
        &self,
        profile: &Profile,
        player: PlayerId,
        binding: super::source_route_same_polygon::SamePolygonBinding,
    ) -> Result<&'call super::source_route_chase_gate::ChaseGateModel, Failure> {
        self.require_profile(profile)?;
        let policy = self
            .session()
            .same_polygon_route
            .as_ref()
            .ok_or(Failure::Binding)?;
        if policy.environment.player != player || policy.binding != binding {
            return Err(Failure::Binding);
        }
        Ok(&self.validation.model)
    }
    pub(super) fn validated_dispatch(
        &self,
        profile: &Profile,
    ) -> Result<&'call PathfinderDispatchReceipt, Failure> {
        self.require_profile(profile)?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &self.request().query else {
            return Err(Failure::Binding);
        };
        route.pathfinder_dispatch.as_ref().ok_or(Failure::Binding)
    }
    pub(super) fn validated_owner_ignore(
        &self,
        profile: &Profile,
    ) -> Result<&'call OwnerIgnorePathfindingReceipt, Failure> {
        self.require_profile(profile)?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &self.request().query else {
            return Err(Failure::Binding);
        };
        route
            .owner_ignore_pathfinding
            .as_ref()
            .ok_or(Failure::Binding)
    }

    pub(super) fn lease(&self) -> JobLease {
        self.validation.lease.clone()
    }
    /// Pin the selected provider allocation to this original Work, including
    /// retries before the provider has installed its first pending phase.
    pub(super) fn bind_provider(&self, identity: &Arc<()>) -> Result<(), Failure> {
        if !self.validation.lease.is_active() {
            return Err(Failure::Deadline);
        }
        let selected = self
            .validation
            .selected_provider
            .get_or_init(|| Arc::clone(identity));
        if Arc::ptr_eq(selected, identity) {
            Ok(())
        } else {
            Err(Failure::Binding)
        }
    }
    /// Validate only facts for this original owned Route receipt. Replacement
    /// session, request or body inputs cannot enter staged certification.
    pub(super) fn validate_facts(&self, facts: &Facts) -> Result<(), Failure> {
        if !self.validation.lease.is_active() {
            return Err(Failure::Deadline);
        }
        let (
            Query::SourceMotion(MotionQuery::Route(route)),
            Facts::SourceMotion(MotionFacts::Spatial(reply)),
        ) = (&self.validation.request.query, facts)
        else {
            return Err(Failure::Binding);
        };
        validate_route_facts(
            &self.validation.session,
            &self.validation.request,
            route,
            reply,
        )?;
        if !self.validation.lease.is_active() {
            return Err(Failure::Deadline);
        }
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn record_phase(&self, phase: &'static str) {
        self.validation.phase.set(phase);
    }
}
pub(super) enum ServiceQuery<'call> {
    Immediate(ValidatedRequest<'call>),
    Route(RouteJob<'call>),
}
pub(super) enum ServiceStep {
    Pending,
    Complete(Box<Facts>),
}
struct Work {
    owner: JobOwner,
    route_body: Option<OwnedRouteBodyProgress>,
    route_validation: Option<OwnedRouteValidation>,
    reply: Sender<Reply>,
}
impl Work {
    fn new(
        ticket: u64,
        request: AuthorityRequest,
        deadline: Instant,
        reply: Sender<Reply>,
        attachment: Arc<AtomicBool>,
    ) -> Self {
        Self {
            owner: JobOwner(Arc::new(JobIdentity {
                ticket,
                request,
                deadline,
                attachment,
                owner_alive: AtomicBool::new(true),
            })),
            route_body: None,
            route_validation: None,
            reply,
        }
    }
    fn lease(&self) -> JobLease {
        JobLease(self.owner.0.clone())
    }
    fn active_for(&self, attachment: &Arc<AtomicBool>) -> bool {
        Arc::ptr_eq(&self.owner.0.attachment, attachment) && self.lease().is_active()
    }
}
impl std::ops::Deref for Work {
    type Target = JobIdentity;
    fn deref(&self) -> &Self::Target {
        &self.owner.0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QueryDisposition {
    Deferred,
    Delivered,
    Failed,
    Retired,
}
struct Reply {
    ticket: u64,
    request: AuthorityRequest,
    result: Result<Facts, Failure>,
}
#[cfg(test)]
pub(super) struct ServiceTestReply(Receiver<Reply>);
#[cfg(test)]
impl ServiceTestReply {
    pub(super) fn try_result(
        &self,
    ) -> Result<Result<Facts, Failure>, crossbeam_channel::TryRecvError> {
        self.0.try_recv().map(|reply| reply.result)
    }
}

/// Control data for the same installed loaded owner. It never carries native objects
/// or grants a proposal the authority of an actual committed controller transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RouteCommitCommand {
    Initialize(RouteCommitBoundary),
    Commit(RouteCommitNotice),
}
impl RouteCommitCommand {
    fn boundary(&self) -> &RouteCommitBoundary {
        match self {
            Self::Initialize(boundary) => boundary,
            Self::Commit(notice) => notice.boundary(),
        }
    }
}
struct RouteCommitWork {
    ticket: u64,
    command: RouteCommitCommand,
    deadline: Instant,
    reply: Sender<RouteCommitReply>,
}
struct RouteCommitReply {
    ticket: u64,
    command: RouteCommitCommand,
    result: Result<(), Failure>,
}

/// The controller owns this separately from its proposal-only Authority. Its unique
/// transport and cancellation fence belong to exactly one mailbox installation.
pub(crate) struct MailboxRouteCommitSink {
    session: Session,
    tx: Sender<RouteCommitWork>,
    ticket: u64,
    active: Arc<AtomicBool>,
    cancelled: Receiver<()>,
    ready: Arc<AtomicBool>,
    initial: Option<RouteCommitBoundary>,
}

/// Owned exclusively by the off-thread native driver. The main-thread service never waits.
/// A timeout leaves NativeWorld::step uncommitted; retry the same input/tick or pause explicitly.
/// Do not call resolve from a Bevy system: it deliberately waits for the main-thread service.
pub(crate) struct LocalAuthority {
    session: Session,
    tx: Sender<Work>,
    ticket: u64,
    active: Arc<AtomicBool>,
    cancelled: Receiver<()>,
    route_ready: Arc<AtomicBool>,
    route_commit: Option<MailboxRouteCommitSink>,
}
pub(crate) struct Mailbox {
    session: Session,
    rx: Receiver<Work>,
    route_rx: Receiver<RouteCommitWork>,
    route_initial: RefCell<Option<RouteCommitBoundary>>,
    /// Retry at the same bounded FIFO tail; no second queue or renewed job lifetime.
    retry: Sender<Work>,
    active: Arc<AtomicBool>,
    _cancellation: Sender<()>,
}
impl Drop for Mailbox {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
        // Crossbeam keeps queued values alive while a sender exists. Release old
        // reply endpoints now; the separate cancellation channel also covers a
        // producer racing this bounded drain before receiver destruction.
        for _ in 0..QUEUE_CAPACITY {
            if self.rx.try_recv().is_err() {
                break;
            }
        }
        for _ in 0..ROUTE_COMMIT_CAPACITY {
            if self.route_rx.try_recv().is_err() {
                break;
            }
        }
    }
}
pub(crate) fn mailbox(session: Session) -> Result<(LocalAuthority, Mailbox), Failure> {
    session.check_participants()?;
    if !session.validate() {
        return Err(Failure::Invalid);
    }
    let (tx, rx) = bounded(QUEUE_CAPACITY);
    let (route_tx, route_rx) = bounded(ROUTE_COMMIT_CAPACITY);
    let active = Arc::new(AtomicBool::new(true));
    let (cancellation, cancelled) = bounded(0);
    let route_selected = session.same_polygon_route.is_some();
    let route_ready = Arc::new(AtomicBool::new(!route_selected));
    let route_commit = route_selected.then(|| MailboxRouteCommitSink {
        session: session.clone(),
        tx: route_tx,
        ticket: 0,
        active: Arc::clone(&active),
        cancelled: cancelled.clone(),
        ready: Arc::clone(&route_ready),
        initial: None,
    });
    Ok((
        LocalAuthority {
            session: session.clone(),
            tx: tx.clone(),
            ticket: 0,
            active: active.clone(),
            cancelled,
            route_ready,
            route_commit,
        },
        Mailbox {
            session,
            rx,
            route_rx,
            route_initial: RefCell::new(None),
            retry: tx,
            active,
            _cancellation: cancellation,
        },
    ))
}
impl Authority for LocalAuthority {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        self.check_active()?;
        if !self.route_ready.load(Ordering::Acquire) {
            return Err(NativeError::AuthorityUnavailable(
                "local route commit session is not initialized",
            ));
        }
        self.session.body(request).map_err(Failure::native)?;
        self.ticket = self.ticket.checked_add(1).ok_or(NativeError::Limit)?;
        let (tx, rx) = bounded(1);
        let deadline = Instant::now() + WAIT;
        self.tx
            .try_send(Work::new(
                self.ticket,
                request.clone(),
                deadline,
                tx,
                self.active.clone(),
            ))
            .map_err(|e| match e {
                TrySendError::Full(_) => {
                    NativeError::AuthorityUnavailable("local geometry queue full")
                }
                TrySendError::Disconnected(_) => {
                    NativeError::AuthorityUnavailable("local geometry service closed")
                }
            })?;
        let reply = receive_reply_until(&rx, &self.cancelled, deadline)?;
        self.finish_reply(request, reply)
    }
}
impl LocalAuthority {
    /// A selected owned-route mailbox cannot resolve until this exact sink's
    /// initialization response has been accepted by its waiting controller.
    pub(crate) fn take_route_commit_sink(&mut self) -> Option<MailboxRouteCommitSink> {
        self.route_commit.take()
    }
    fn check_active(&self) -> Result<(), NativeError> {
        if self.active.load(Ordering::Acquire) {
            Ok(())
        } else {
            Err(NativeError::AuthorityUnavailable(
                "local geometry snapshot replaced",
            ))
        }
    }
    fn finish_reply(
        &self,
        request: &AuthorityRequest,
        reply: Reply,
    ) -> Result<AuthorityReply, NativeError> {
        self.check_active()?;
        if reply.ticket != self.ticket || reply.request != *request {
            return Err(NativeError::AuthorityBinding);
        }
        let facts = reply.result.map_err(Failure::native)?;
        validate_facts(&self.session, request, &facts).map_err(Failure::native)?;
        self.check_active()?;
        Ok(AuthorityReply {
            request: request.clone(),
            facts,
        })
    }
}

fn check_route_boundary(session: &Session, boundary: &RouteCommitBoundary) -> Result<(), Failure> {
    let policy = session
        .same_polygon_route
        .as_ref()
        .ok_or(Failure::UnsupportedQuery)?;
    if boundary.world() != session.world
        || boundary.player() != policy.environment.player
        || boundary.profile() != session.profile
        || boundary.geometry() != session.geometry
        || boundary.authority() != session.policy
    {
        return Err(Failure::Binding);
    }
    Ok(())
}

fn same_route_attachment(initial: &RouteCommitBoundary, next: &RouteCommitBoundary) -> bool {
    initial.session() == next.session()
        && initial.world() == next.world()
        && initial.player() == next.player()
        && initial.profile() == next.profile()
        && initial.geometry() == next.geometry()
        && initial.authority() == next.authority()
}

impl MailboxRouteCommitSink {
    fn check_active(&self) -> Result<(), NativeError> {
        if self.active.load(Ordering::Acquire) {
            Ok(())
        } else {
            Err(NativeError::AuthorityUnavailable(
                "local route commit snapshot replaced",
            ))
        }
    }
    fn send(&mut self, command: RouteCommitCommand) -> Result<(), NativeError> {
        self.check_active()?;
        check_route_boundary(&self.session, command.boundary()).map_err(Failure::native)?;
        self.ticket = self.ticket.checked_add(1).ok_or(NativeError::Limit)?;
        let (reply, rx) = bounded(1);
        let deadline = Instant::now() + WAIT;
        self.tx
            .try_send(RouteCommitWork {
                ticket: self.ticket,
                command: command.clone(),
                deadline,
                reply,
            })
            .map_err(|error| match error {
                TrySendError::Full(_) => {
                    NativeError::AuthorityUnavailable("local route commit queue full")
                }
                TrySendError::Disconnected(_) => {
                    NativeError::AuthorityUnavailable("local route commit service closed")
                }
            })?;
        let reply = receive_reply_until(&rx, &self.cancelled, deadline)?;
        self.finish_reply(&command, reply)
    }
    fn finish_reply(
        &self,
        command: &RouteCommitCommand,
        reply: RouteCommitReply,
    ) -> Result<(), NativeError> {
        self.check_active()?;
        if reply.ticket != self.ticket || reply.command != *command {
            return Err(NativeError::AuthorityBinding);
        }
        reply.result.map_err(Failure::native)?;
        self.check_active()
    }
}
impl RouteCommitSink for MailboxRouteCommitSink {
    fn initialize(&mut self, boundary: &RouteCommitBoundary) -> Result<(), NativeError> {
        self.check_active()?;
        check_route_boundary(&self.session, boundary).map_err(Failure::native)?;
        match &self.initial {
            Some(initial) if initial != boundary => return Err(NativeError::AuthorityBinding),
            Some(_) => (),
            None => self.initial = Some(boundary.clone()),
        }
        // Reserve the original attachment before sending. A lost reply may have
        // applied it; only the exact same boundary may be retried afterward.
        self.send(RouteCommitCommand::Initialize(boundary.clone()))?;
        self.ready.store(true, Ordering::Release);
        Ok(())
    }
    fn deliver(&mut self, notice: &RouteCommitNotice) -> Result<(), NativeError> {
        self.check_active()?;
        if !self.ready.load(Ordering::Acquire) {
            return Err(NativeError::AuthorityUnavailable(
                "local route commit session is not initialized",
            ));
        }
        if self
            .initial
            .as_ref()
            .is_none_or(|initial| !same_route_attachment(initial, notice.boundary()))
        {
            return Err(NativeError::AuthorityBinding);
        }
        self.send(RouteCommitCommand::Commit(notice.clone()))
    }
}

fn receive_reply_until<T>(
    rx: &Receiver<T>,
    cancelled: &Receiver<()>,
    deadline: Instant,
) -> Result<T, NativeError> {
    let unavailable = || NativeError::AuthorityUnavailable("local geometry reply unavailable");
    let expired = || NativeError::AuthorityUnavailable("local geometry request deadline exceeded");
    let timeout = crossbeam_channel::at(deadline);
    let reply = crossbeam_channel::select_biased! {
        recv(cancelled) -> _ => return Err(unavailable()),
        recv(rx) -> result => result.map_err(|_| unavailable())?,
        recv(timeout) -> _ => return Err(expired()),
    };
    // Channel receives may return an already-buffered reply after their deadline. A driver
    // descheduled after enqueue must not renew the request's authority window when it resumes.
    if Instant::now() >= deadline {
        return Err(expired());
    }
    Ok(reply)
}

impl Mailbox {
    /// Controls consume the same explicit frame budget as queries, but require no
    /// current terrain/WorldLive view. Applying one is not proof its reply arrived:
    /// the provider must safely accept the exact last notice again after reply loss.
    pub(crate) fn serve_route_commits(
        &self,
        limit: usize,
        mut commit: impl FnMut(&Session, &RouteCommitCommand) -> Result<(), Failure>,
    ) -> usize {
        let deadline = Instant::now() + Duration::from_millis(2);
        let mut count = 0;
        while count < limit.min(PER_FRAME) {
            if count > 0 && Instant::now() >= deadline {
                break;
            }
            let Ok(work) = self.route_rx.try_recv() else {
                break;
            };
            count += 1;
            if !self.active.load(Ordering::Acquire) || Instant::now() >= work.deadline {
                continue;
            }
            let result = self.check_route_command(&work.command).and_then(|()| {
                commit(&self.session, &work.command)?;
                if let RouteCommitCommand::Initialize(boundary) = &work.command {
                    *self.route_initial.borrow_mut() = Some(boundary.clone());
                }
                Ok(())
            });
            if !self.active.load(Ordering::Acquire) || Instant::now() >= work.deadline {
                continue;
            }
            let _ = work.reply.try_send(RouteCommitReply {
                ticket: work.ticket,
                command: work.command,
                result,
            });
        }
        count
    }
    fn check_route_command(&self, command: &RouteCommitCommand) -> Result<(), Failure> {
        check_route_boundary(&self.session, command.boundary())?;
        let initial = self.route_initial.borrow();
        match (initial.as_ref(), command) {
            (None, RouteCommitCommand::Initialize(_)) => Ok(()),
            (Some(initial), RouteCommitCommand::Initialize(boundary)) if initial == boundary => {
                Ok(())
            }
            (Some(initial), RouteCommitCommand::Commit(notice))
                if same_route_attachment(initial, notice.boundary()) =>
            {
                Ok(())
            }
            _ => Err(Failure::Binding),
        }
    }
    /// The count includes expired/rejected jobs, so cancellation cannot bypass the frame bound.
    /// A caller provides a fresh read-only geometry view for each service invocation.
    #[cfg(test)]
    pub(super) fn serve(
        &self,
        limit: usize,
        mut geometry: impl FnMut(&MovementRequest, Body) -> Result<MovementOutcome, Failure>,
    ) -> usize {
        self.serve_queries(limit, |_, request, body| {
            let Query::Movement(m) = &request.query else {
                return Err(Failure::UnsupportedQuery);
            };
            geometry(m, body).map(Facts::Movement)
        })
    }
    pub(super) fn serve_queries(
        &self,
        limit: usize,
        mut geometry: impl FnMut(&Session, &AuthorityRequest, Body) -> Result<Facts, Failure>,
    ) -> usize {
        self.serve_queries_until(limit, |session, request, body, _| {
            geometry(session, request, body)
        })
    }
    /// A slice expiry retries the exact work in a later invocation. The original
    /// job expiry remains absolute; repeatedly oversized work eventually times out.
    pub(crate) fn serve_queries_until(
        &self,
        limit: usize,
        mut geometry: impl FnMut(&Session, &AuthorityRequest, Body, Instant) -> Result<Facts, Failure>,
    ) -> usize {
        let deadline = Instant::now() + Duration::from_millis(2);
        self.serve_query_slice(limit, deadline, &mut geometry)
    }
    /// The higher-ranked callback cannot retain validation across attempts.
    pub(super) fn serve_validated_queries_until(
        &self,
        limit: usize,
        geometry: impl for<'attempt> FnMut(
            ValidatedRequest<'attempt>,
            Instant,
        ) -> Result<Facts, Failure>,
    ) -> usize {
        let deadline = Instant::now() + Duration::from_millis(2);
        self.serve_validated_query_slice(limit, deadline, geometry)
    }
    /// Selected Route jobs retain only completed owned validation between calls.
    /// Every yield spends one frame attempt and returns to the same bounded FIFO.
    pub(super) fn serve_service_queries_until(
        &self,
        limit: usize,
        stage_routes: bool,
        geometry: impl for<'call> FnMut(ServiceQuery<'call>, Instant) -> Result<ServiceStep, Failure>,
    ) -> usize {
        self.serve_service_query_slice(
            limit,
            stage_routes,
            Instant::now() + Duration::from_millis(2),
            geometry,
        )
    }
    #[cfg(test)]
    pub(super) fn enqueue_service_test_request(
        &self,
        ticket: u64,
        request: AuthorityRequest,
    ) -> Result<(JobLease, ServiceTestReply), Failure> {
        self.session.body(&request)?;
        if !self.active.load(Ordering::Acquire) {
            return Err(Failure::Binding);
        }
        let (reply, receive) = bounded(1);
        let work = Work::new(
            ticket,
            request,
            Instant::now() + WAIT,
            reply,
            self.active.clone(),
        );
        let lease = work.lease();
        self.retry
            .try_send(work)
            .map_err(|_| Failure::QueueCapacity)?;
        Ok((lease, ServiceTestReply(receive)))
    }
    #[cfg(test)]
    pub(super) fn serve_service_test_slice(
        &self,
        limit: usize,
        stage_routes: bool,
        deadline: Instant,
        geometry: impl for<'call> FnMut(ServiceQuery<'call>, Instant) -> Result<ServiceStep, Failure>,
    ) -> usize {
        self.serve_service_query_slice(limit, stage_routes, deadline, geometry)
    }
    fn serve_service_query_slice(
        &self,
        limit: usize,
        stage_routes: bool,
        deadline: Instant,
        mut geometry: impl for<'call> FnMut(
            ServiceQuery<'call>,
            Instant,
        ) -> Result<ServiceStep, Failure>,
    ) -> usize {
        let mut count = 0;
        while count < limit.min(PER_FRAME) {
            if count > 0 && Instant::now() >= deadline {
                break;
            }
            let Ok(mut work) = self.rx.try_recv() else {
                break;
            };
            count += 1;
            if !work.active_for(&self.active) {
                continue;
            }
            let staged = stage_routes
                && matches!(
                    &work.request.query,
                    Query::SourceMotion(MotionQuery::Route(_))
                );
            #[cfg(test)]
            let timing = (
                Instant::now(),
                work.ticket,
                work.request.binding.tick,
                if !staged {
                    "immediate"
                } else if work.route_validation.is_none() {
                    "body"
                } else {
                    "provider"
                },
            );
            #[cfg(test)]
            if let Some(validation) = &work.route_validation {
                validation.phase.set("resume_guard");
            }
            let result = if staged {
                self.advance_route_query(&mut work, deadline, &mut geometry)
            } else {
                self.session
                    .validated_request(&work.request)
                    .and_then(|validated| {
                        match geometry(
                            ServiceQuery::Immediate(validated),
                            work.deadline.min(deadline),
                        )? {
                            ServiceStep::Pending => Err(Failure::UnsupportedQuery),
                            ServiceStep::Complete(facts) => {
                                validate_facts(&self.session, &work.request, &facts)?;
                                Ok(ServiceStep::Complete(facts))
                            }
                        }
                    })
            };
            #[cfg(test)]
            let phase = work.route_validation.as_ref().map_or_else(
                || {
                    work.route_body
                        .as_ref()
                        .map_or(timing.3, |body| body.next.name())
                },
                |validation| validation.phase.get(),
            );
            let pending = matches!(&result, Ok(ServiceStep::Pending));
            let disposition = match result {
                Ok(ServiceStep::Pending) => self.defer_route_query(work),
                Ok(ServiceStep::Complete(facts)) => {
                    self.finish_query_result(work, Ok(*facts), deadline, staged)
                }
                Err(error) => self.finish_query_result(work, Err(error), deadline, staged),
            };
            // Keep observation I/O outside all deadline-sensitive checks and publication.
            #[cfg(test)]
            if staged {
                eprintln!(
                    "native geometry Route job timing: tick={} ticket={} phase={} disposition={:?} elapsed={:?}",
                    timing.2,
                    timing.1,
                    phase,
                    disposition,
                    timing.0.elapsed(),
                );
            }
            // Pending always ends this invocation, including a full tail.
            if pending || disposition == QueryDisposition::Deferred {
                break;
            }
        }
        count
    }
    fn advance_route_query(
        &self,
        work: &mut Work,
        slice: Instant,
        geometry: &mut impl for<'call> FnMut(
            ServiceQuery<'call>,
            Instant,
        ) -> Result<ServiceStep, Failure>,
    ) -> Result<ServiceStep, Failure> {
        if !work.active_for(&self.active) || Instant::now() >= slice {
            return Err(Failure::Deadline);
        }
        if work.route_validation.is_none() {
            self.advance_route_body(work, slice, Instant::now, |_, _| {})?;
            return Ok(ServiceStep::Pending);
        }
        let validation = work.route_validation.as_ref().ok_or(Failure::Binding)?;
        validation.check(&self.session, work)?;
        let step = geometry(
            ServiceQuery::Route(RouteJob { validation }),
            work.deadline.min(slice),
        )?;
        // Provider failure provenance above is preserved even if time also expired.
        validation.check(&self.session, work)?;
        if let ServiceStep::Complete(facts) = &step {
            RouteJob { validation }.validate_facts(facts)?;
        }
        if !work.active_for(&self.active) || Instant::now() >= slice {
            // The provider retains Terminal, not these facts. The next attempt must
            // certify current physics again before it may publish a Complete result.
            return Err(Failure::Deadline);
        }
        Ok(step)
    }
    fn advance_route_body(
        &self,
        work: &mut Work,
        slice: Instant,
        mut now: impl FnMut() -> Instant,
        mut observed: impl FnMut(RouteBodyPhase, bool),
    ) -> Result<(), Failure> {
        if work.route_body.is_none() {
            work.route_body = Some(OwnedRouteBodyProgress {
                model: None,
                session: self.session.clone(),
                request: work.request.clone(),
                lease: work.lease(),
                next: RouteBodyPhase::Context,
            });
        }
        loop {
            let progress = work.route_body.as_ref().ok_or(Failure::Binding)?;
            check_owned_route(
                &progress.session,
                &progress.request,
                &progress.lease,
                &self.session,
                work,
            )?;
            if !work.active_for(&self.active) || now() >= slice {
                return Err(Failure::Deadline);
            }
            let policy = progress
                .session
                .same_polygon_route
                .as_ref()
                .ok_or(Failure::UnsupportedQuery)?;
            let phase = progress.next;
            let (body, model) = match phase {
                RouteBodyPhase::Context => {
                    progress.session.check_request_binding(&progress.request)?;
                    policy.validate_context(&progress.session, &progress.request)?;
                    (None, None)
                }
                RouteBodyPhase::Model => (
                    None,
                    Some(policy.validate_model(&progress.session, &progress.request)?),
                ),
                RouteBodyPhase::OwnerState => (
                    Some(policy.resolve_owner_body(&progress.session, &progress.request)?),
                    None,
                ),
            };
            observed(phase, false);
            // A real validation failure above keeps its provenance even if time expired.
            // Only a complete, timely subpart is retained; its original input is immutable.
            check_owned_route(
                &progress.session,
                &progress.request,
                &progress.lease,
                &self.session,
                work,
            )?;
            if !work.active_for(&self.active) || now() >= slice {
                return Err(Failure::Deadline);
            }
            if let Some(body) = body {
                let progress = work.route_body.take().ok_or(Failure::Binding)?;
                work.route_validation = Some(OwnedRouteValidation {
                    model: progress.model.ok_or(Failure::Binding)?,
                    session: progress.session,
                    request: progress.request,
                    body,
                    lease: progress.lease,
                    selected_provider: OnceLock::new(),
                    #[cfg(test)]
                    phase: std::cell::Cell::new("body_complete"),
                });
                observed(phase, true);
                return Ok(());
            }
            let progress = work.route_body.as_mut().ok_or(Failure::Binding)?;
            if let Some(model) = model {
                progress.model = Some(model);
            }
            progress.next = match phase {
                RouteBodyPhase::Context => RouteBodyPhase::Model,
                RouteBodyPhase::Model => RouteBodyPhase::OwnerState,
                RouteBodyPhase::OwnerState => unreachable!(),
            };
            observed(phase, true);
        }
    }
    fn defer_route_query(&self, work: Work) -> QueryDisposition {
        if !work.active_for(&self.active) {
            return QueryDisposition::Retired;
        }
        match self.retry.try_send(work) {
            Ok(()) => QueryDisposition::Deferred,
            Err(error) => self.finish_query_result(
                error.into_inner(),
                Err(Failure::QueueCapacity),
                Instant::now(),
                false,
            ),
        }
    }
    fn serve_query_slice(
        &self,
        limit: usize,
        deadline: Instant,
        mut geometry: impl FnMut(&Session, &AuthorityRequest, Body, Instant) -> Result<Facts, Failure>,
    ) -> usize {
        self.serve_validated_query_slice(limit, deadline, |validated, until| {
            geometry(
                validated.session(),
                validated.request(),
                validated.body(),
                until,
            )
        })
    }
    fn serve_validated_query_slice(
        &self,
        limit: usize,
        deadline: Instant,
        mut geometry: impl for<'attempt> FnMut(
            ValidatedRequest<'attempt>,
            Instant,
        ) -> Result<Facts, Failure>,
    ) -> usize {
        let mut count = 0;
        while count < limit.min(PER_FRAME) {
            if count > 0 && Instant::now() >= deadline {
                break;
            }
            let Ok(work) = self.rx.try_recv() else { break };
            count += 1;
            if !work.active_for(&self.active) {
                continue;
            }
            #[cfg(test)]
            let started = Instant::now();
            #[cfg(test)]
            let mut stage = "session.body";
            let validated = self.session.validated_request(&work.request);
            #[cfg(test)]
            let body_finished = Instant::now();
            #[cfg(test)]
            let mut provider_finished = body_finished;
            let result = validated.and_then(|validated| {
                #[cfg(test)]
                {
                    stage = "provider callback";
                }
                let facts = geometry(validated, work.deadline.min(deadline));
                #[cfg(test)]
                {
                    provider_finished = Instant::now();
                }
                let facts = facts?;
                #[cfg(test)]
                {
                    stage = "validate_facts";
                }
                validate_facts(&self.session, &work.request, &facts)?;
                Ok(facts)
            });
            #[cfg(test)]
            if matches!(
                &work.request.query,
                Query::SourceMotion(MotionQuery::Route(_))
            ) {
                let ended = Instant::now();
                eprintln!(
                    "native geometry Route timing: tick={} ticket={} success={} stage={stage} \
                     body={:?} provider={:?} reply_validation={:?} total={:?} \
                     frame_remaining={:?} job_remaining={:?}",
                    work.request.binding.tick,
                    work.ticket,
                    result.is_ok(),
                    body_finished.duration_since(started),
                    provider_finished.saturating_duration_since(body_finished),
                    ended.duration_since(provider_finished),
                    ended.duration_since(started),
                    deadline.saturating_duration_since(ended),
                    work.deadline.saturating_duration_since(ended),
                );
            }
            #[cfg(test)]
            if let Err(error) = &result {
                let ended = Instant::now();
                eprintln!(
                    "native geometry mailbox failure: tick={} ticket={} query_kind={} stage={stage} \
                     error={error:?} elapsed={:?} job_expired_before={} job_expired_after={} \
                     frame_expired_before={} frame_expired_after={} \
                     job_remaining_before={:?} frame_remaining_before={:?} \
                     job_remaining_after={:?} frame_remaining_after={:?}",
                    work.request.binding.tick,
                    work.ticket,
                    query_kind(&work.request.query),
                    ended.duration_since(started),
                    started >= work.deadline,
                    ended >= work.deadline,
                    started >= deadline,
                    ended >= deadline,
                    work.deadline.saturating_duration_since(started),
                    deadline.saturating_duration_since(started),
                    work.deadline.saturating_duration_since(ended),
                    deadline.saturating_duration_since(ended),
                );
            }
            if self.finish_query(work, result, deadline) {
                break;
            }
        }
        count
    }
    /// True ends this invocation after deferring one still-live request. Real
    /// failures retain their meaning even when the clock expired during them.
    fn finish_query(&self, work: Work, result: Result<Facts, Failure>, slice: Instant) -> bool {
        self.finish_query_result(work, result, slice, false) == QueryDisposition::Deferred
    }
    fn finish_query_result(
        &self,
        mut work: Work,
        mut result: Result<Facts, Failure>,
        slice: Instant,
        staged: bool,
    ) -> QueryDisposition {
        // Clone the transport echo before the last time guards, not after them.
        let request = work.request.clone();
        if !work.active_for(&self.active) {
            return QueryDisposition::Retired;
        }
        let now = Instant::now();
        if staged && result.is_ok() && now >= slice {
            result = Err(Failure::Deadline);
        }
        if matches!(result, Err(Failure::Deadline)) && now >= slice && slice < work.deadline {
            match self.retry.try_send(work) {
                Ok(()) => return QueryDisposition::Deferred,
                // A concurrent enqueue may consume the vacated slot. Do not
                // exceed capacity, drop another job, or renew this one's expiry.
                Err(error) => {
                    work = error.into_inner();
                    if staged {
                        result = Err(Failure::QueueCapacity);
                    }
                }
            }
        }
        if !work.active_for(&self.active) {
            return QueryDisposition::Retired;
        }
        let disposition = if result.is_ok() {
            QueryDisposition::Delivered
        } else {
            QueryDisposition::Failed
        };
        match work.reply.try_send(Reply {
            ticket: work.ticket,
            request,
            result,
        }) {
            Ok(()) => disposition,
            Err(_) => QueryDisposition::Retired,
        }
    }
}

#[cfg(test)]
fn query_kind(query: &Query) -> &'static str {
    match query {
        Query::Movement(_) => "Movement",
        Query::PlayerMovement { .. } => "PlayerMovement",
        Query::PlayerEnvironment(_) => "PlayerEnvironment",
        Query::SourceLocation(_) => "SourceLocation",
        Query::SourceMotion(MotionQuery::Route(_)) => "Route",
        Query::SourceMotion(MotionQuery::NearPoint(_)) => "NearPoint",
        Query::SourceMotion(MotionQuery::Combat(_)) => "Combat",
        Query::SourceMotion(_) => "SourceMotion",
        Query::SourceSpell(_) => "SourceSpell",
        _ => "Other",
    }
}

pub(super) fn validate_facts(
    session: &Session,
    request: &AuthorityRequest,
    facts: &Facts,
) -> Result<(), Failure> {
    match (&request.query, facts) {
        (
            Query::SourceMotion(MotionQuery::Route(route)),
            Facts::SourceMotion(MotionFacts::Spatial(reply)),
        ) => {
            session.body(request)?;
            validate_route_facts(session, request, route, reply)
        }
        (Query::Movement(m), Facts::Movement(outcome)) => validate_outcome(m, outcome),
        (Query::SourceLocation(query), Facts::SourceLocation(facts)) => {
            session.body(request)?;
            let catalog = session
                .source_location
                .as_ref()
                .and_then(|policy| policy.profile.source_location())
                .ok_or(Failure::Binding)?
                .catalog();
            facts
                .validate_with_catalog(query, catalog)
                .map_err(|_| Failure::Binding)
        }
        (Query::PlayerEnvironment(query), Facts::PlayerEnvironment(facts)) => {
            session.body(request)?;
            facts.validate_for(query).map_err(|_| Failure::Binding)
        }
        (Query::PlayerMovement { .. }, Facts::PlayerMovement { accepted }) => {
            let player = session.player.ok_or(Failure::UnresolvedBody)?;
            let within_budget = player_path(&request.query, player)?.is_some();
            if *accepted && !within_budget {
                Err(Failure::Binding)
            } else {
                Ok(())
            }
        }
        (
            Query::SourceSpell(SourceSpellQuery::LineOfSight(_)),
            Facts::SourceSpell(SourceSpellFacts::LineOfSight(Check::Passed | Check::Failed)),
        )
        | (
            Query::SourceMotion(MotionQuery::Combat(CombatQuery::NpcLineOfSight(_))),
            Facts::SourceMotion(MotionFacts::Combat(CombatFacts::NpcLineOfSight(
                Check::Passed | Check::Failed,
            ))),
        ) => session.body(request).map(|_| ()),
        (
            Query::SourceMotion(MotionQuery::NearPoint(near)),
            Facts::SourceMotion(MotionFacts::Spatial(reply)),
        ) => {
            session.body(request)?;
            if reply.request != motion::Query::NearPoint(near.request.clone()) {
                return Err(Failure::Binding);
            }
            let motion::Fact::NearPoint(point) = reply.fact else {
                return Err(Failure::Binding);
            };
            point
                .distance_squared(point)
                .map(|_| ())
                .map_err(|_| Failure::Binding)
        }
        _ => Err(Failure::Binding),
    }
}

/// Shared exact Route result checks. Only the owned staged receipt may skip
/// repeating Session::body; raw consumers and LocalAuthority still perform it.
fn validate_route_facts(
    session: &Session,
    request: &AuthorityRequest,
    route: &benilla_world_runtime::native::CreatureRouteRequest,
    reply: &motion::Reply,
) -> Result<(), Failure> {
    if reply.request != motion::Query::Route(route.request.clone()) {
        return Err(Failure::Binding);
    }
    let motion::Fact::RouteCalculation(calculation) = &reply.fact else {
        return Err(Failure::Binding);
    };
    calculation
        .validate_for(&route.request)
        .map_err(|_| Failure::Binding)?;
    let motion::RouteOutcome::Complete(path) = &calculation.outcome else {
        return Err(Failure::Binding);
    };
    let passed = SegmentChecks {
        loaded_coverage: Check::Passed,
        line_of_sight: Check::Passed,
        body_sweep: Check::Passed,
        continuous_ground_support: Check::Passed,
        dry_ground: Check::Passed,
    };
    let policy = session
        .same_polygon_route
        .as_ref()
        .ok_or(Failure::UnsupportedQuery)?;
    let include_flags = benilla_sim::route_filter::initial_include_flags(
        route.owner.capabilities().map_err(|_| Failure::Binding)?,
    )
    .ok_or(Failure::UnsupportedQuery)?;
    if calculation.source != motion::RouteSource::OwnerPathFinder
        || path.source != motion::RouteSource::OwnerPathFinder
        || path.path_identity == [0; 32]
        || path.points != [route.request.origin, route.request.goal]
        || path.segments != [passed]
        || calculation.after_cache.binding.owner_generation
            != request.binding.npc.ok_or(Failure::Binding)?.generation
        || calculation.after_cache.binding.navigation_policy != session.geometry
        || calculation.after_cache.binding.inventory != policy.binding.navigation.inventory
        || calculation.after_cache.binding.source_epoch != policy.binding.navigation.source_epoch
        || calculation.after_cache.active_len != 1
        || calculation.after_cache.include_flags != include_flags
        || calculation.after_cache.exclude_flags != 0
    {
        return Err(Failure::Binding);
    }
    Ok(())
}

/// Each accepted packet extends the same immutable path; even returning to the origin spends
/// distance. This is a local conservative policy, not CMaNGOS client anti-cheat parity.
/// Every segment length is rounded upward to binary32, then added in binary64 with an upward
/// guard when addition is inexact. Budget arithmetic starts from the source-resolved f32 speed.
fn player_path(query: &Query, player: PlayerMotion) -> Result<Option<Vec<[f32; 3]>>, Failure> {
    let Query::PlayerMovement {
        frame_origin,
        accepted_path,
        destination,
        elapsed_ms,
        speed,
    } = query
    else {
        return Err(Failure::UnsupportedQuery);
    };
    if !player.validate()
        || *elapsed_ms == 0
        || *elapsed_ms > 1000
        || accepted_path.len() >= benilla_world_runtime::native::MAX_INPUTS_PER_TICK
    {
        return Err(Failure::Invalid);
    }
    let points: Vec<_> = std::iter::once(*frame_origin)
        .chain(accepted_path.iter().copied())
        .chain(std::iter::once(*destination))
        .map(point)
        .collect();
    if points
        .iter()
        .flatten()
        .any(|v| !v.is_finite() || v.abs() > 32760.0)
    {
        return Err(Failure::Invalid);
    }
    // Only the trusted runtime constructs this request; the speed is echoed and replay-bound.
    // Legacy profiles retain the session speed. Enabled profiles carry the live movement mode.
    let speed = speed.unwrap_or(player.speed).as_f32();
    if !(0.0..=MAX_SEGMENT).contains(&speed) {
        return Err(Failure::Invalid);
    }
    let numerator = f64::from(speed) * f64::from(*elapsed_ms);
    let nearest_budget = numerator / 1000.0;
    // Fused residual retains the sign of division error; multiplying the quotient back
    // normally can round away an upward error (e.g. .001 * 1000 == 1).
    let budget = if (-nearest_budget).mul_add(1000.0, numerator) < 0.0 {
        f64::from_bits(nearest_budget.to_bits() - 1)
    } else {
        nearest_budget
    };
    if budget > f64::from(MAX_SEGMENT) {
        return Err(Failure::UnsupportedNavigation);
    }
    let mut distance = 0.0_f64;
    for pair in points.windows(2) {
        if pair[0][2] != pair[1][2] {
            return Err(Failure::UnsupportedGround);
        }
        let dx = f64::from(pair[1][0]) - f64::from(pair[0][0]);
        let dy = f64::from(pair[1][1]) - f64::from(pair[0][1]);
        // Non-axis-aligned segments use outward-rounded binary64 operations before
        // conversion; this can reject a borderline diagonal, never grant extra distance.
        let up = |v: f64| {
            if v == 0.0 {
                v
            } else {
                f64::from_bits(v.to_bits() + 1)
            }
        };
        let abs_difference = |a: f32, b: f32, difference: f64| {
            let a = f64::from(a);
            let b = -f64::from(b);
            let v = difference - a;
            let error = (a - (difference - v)) + (b - v);
            if error.signum() == difference.signum() && error != 0.0 {
                up(difference.abs())
            } else {
                difference.abs()
            }
        };
        let dx = abs_difference(pair[1][0], pair[0][0], dx);
        let dy = abs_difference(pair[1][1], pair[0][1], dy);
        let upper64 = if dx == 0.0 {
            dy
        } else if dy == 0.0 {
            dx
        } else {
            up(up(up(dx * dx) + up(dy * dy)).sqrt())
        };
        let mut upper = upper64 as f32;
        if f64::from(upper) < upper64 {
            upper = f32::from_bits(upper.to_bits() + 1);
        }
        let next = distance + f64::from(upper);
        // Error-free TwoSum detects a rounded-down accumulation; never let a very small
        // movement disappear beside a large one. Zero movement does not spend an ULP.
        let v = next - distance;
        let error = (distance - (next - v)) + (f64::from(upper) - v);
        distance = if error > 0.0 {
            f64::from_bits(next.to_bits() + 1)
        } else {
            next
        };
        if distance > budget {
            return Ok(None);
        }
    }
    Ok(Some(points))
}

pub(crate) fn solve_player(
    query: &Query,
    player: PlayerMotion,
    geometry: &mut impl Corridor,
) -> Result<Facts, Failure> {
    let Some(points) = player_path(query, player)? else {
        return Ok(Facts::PlayerMovement { accepted: false });
    };
    for pair in points.windows(2) {
        match geometry.certify(pair[0], pair[1], player.body) {
            Ok(()) => {}
            Err(Failure::Obstacle) => return Ok(Facts::PlayerMovement { accepted: false }),
            Err(failure) => return Err(failure),
        }
    }
    Ok(Facts::PlayerMovement { accepted: true })
}

fn validate_outcome(request: &MovementRequest, outcome: &MovementOutcome) -> Result<(), Failure> {
    struct Answer(MovementOutcome);
    impl GeometryAuthority for Answer {
        fn resolve(&mut self, _: &MovementRequest) -> Result<MovementOutcome, ai::AiError> {
            Ok(self.0.clone())
        }
    }
    resolve_movement(&mut Answer(outcome.clone()), request.clone())
        .map(|_| ())
        .map_err(|_| Failure::Binding)
}

/// Geometry providers must prove coverage first, then the entire swept body, elevated LOS,
/// continuous support and absence of liquid. No-hit alone is never this certificate.
pub(crate) trait Corridor {
    fn certify(&mut self, from: [f32; 3], to: [f32; 3], body: Body) -> Result<(), Failure>;
}

pub(super) fn solve(
    request: &MovementRequest,
    body: Body,
    geometry: &mut impl Corridor,
) -> Result<MovementOutcome, Failure> {
    if !body.validate()
        || request.elapsed_ms == 0
        || request.elapsed_ms > 1000
        || request.request_id == 0
        || request.command_id == 0
        || request.creature_id == 0
        || request.speed.as_f32() <= 0.0
    {
        return Err(Failure::Invalid);
    }
    let from = point(request.origin);
    let goal = point(request.goal);
    if from
        .iter()
        .chain(&goal)
        .any(|x| !x.is_finite() || x.abs() > 32760.0)
    {
        return Err(Failure::Invalid);
    }
    if from[2] != goal[2] {
        return Err(Failure::UnsupportedGround);
    }
    // Client-local FPU geometry, explicitly outside the deterministic simulation kernel.
    // The independent software-binary32 movement validator remains the final budget gate.
    let budget = request.speed.as_f32() * request.elapsed_ms as f32 / 1000.0;
    if !budget.is_finite() || budget <= 0.0 || budget > MAX_SEGMENT {
        return Err(Failure::UnsupportedNavigation);
    }
    let dx = f64::from(goal[0]) - f64::from(from[0]);
    let dy = f64::from(goal[1]) - f64::from(from[1]);
    let distance = dx.hypot(dy);
    let mut to = goal;
    if distance > f64::from(budget) {
        // Leave several coordinate ULPs of headroom; rounding the position must not create
        // more movement than the software arithmetic allows. Extremely tiny progress refuses.
        let coordinate_ulp = from[..2]
            .iter()
            .map(|v| {
                let v = v.abs();
                f64::from(f32::from_bits(v.to_bits() + 1) - v)
            })
            .fold(0.0_f64, f64::max);
        let permitted = f64::from(budget) - coordinate_ulp * 4.0 - f64::from(budget) * 1e-6;
        if permitted <= 0.0 {
            return Err(Failure::UnsupportedNavigation);
        }
        let fraction = permitted / distance;
        to[0] = (f64::from(from[0]) + dx * fraction) as f32;
        to[1] = (f64::from(from[1]) + dy * fraction) as f32;
    }
    let actual =
        (f64::from(to[0]) - f64::from(from[0])).hypot(f64::from(to[1]) - f64::from(from[1]));
    if actual == 0.0 && from != goal {
        return Err(Failure::UnsupportedNavigation);
    }
    let mut travelled = actual as f32;
    // Round total distance upward, including diagonal displacement; never report straight-line
    // progress with a travelled value smaller than its actual displacement.
    if f64::from(travelled) < actual {
        travelled = f32::from_bits(travelled.to_bits() + 1);
    }
    match geometry.certify(from, to, body) {
        Ok(()) => {}
        Err(Failure::Obstacle) => {
            return Ok(MovementOutcome {
                request_id: request.request_id,
                map_id: request.map_id,
                content_revision: request.content_revision,
                geometry_revision: request.geometry_revision,
                result: MovementResult::Blocked(BlockReason::Obstacle),
            });
        }
        Err(error) => return Err(error),
    }
    let outcome = MovementOutcome {
        request_id: request.request_id,
        map_id: request.map_id,
        content_revision: request.content_revision,
        geometry_revision: request.geometry_revision,
        result: MovementResult::Progress {
            position: ai::Point::new(to[0], to[1], to[2]).map_err(|_| Failure::Invalid)?,
            travelled: ai::Real::new(travelled).map_err(|_| Failure::Invalid)?,
            arrived: to == goal,
            checks: SegmentChecks {
                loaded_coverage: Check::Passed,
                line_of_sight: Check::Passed,
                body_sweep: Check::Passed,
                continuous_ground_support: Check::Passed,
                dry_ground: Check::Passed,
            },
        },
    };
    validate_outcome(request, &outcome)?;
    Ok(outcome)
}
fn point(p: ai::Point) -> [f32; 3] {
    [p.x.as_f32(), p.y.as_f32(), p.z.as_f32()]
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
#[path = "near_point_mailbox_tests.rs"]
mod near_point_mailbox_tests;

#[cfg(test)]
#[path = "empty_selector_policy_tests.rs"]
mod empty_selector_policy_tests;

/// Render-frame accounting, separate from fixed ticks (which may run several times per frame).
#[derive(Default)]
pub(super) struct FrameBudget {
    frame: Option<u32>,
    used: usize,
}
impl FrameBudget {
    pub(super) fn remaining(&mut self, frame: u32) -> usize {
        if self.frame != Some(frame) {
            self.frame = Some(frame);
            self.used = 0;
        }
        PER_FRAME.saturating_sub(self.used)
    }
    pub(super) fn spend(&mut self, used: usize) {
        self.used = self.used.saturating_add(used);
    }
}

/// Exact current raw-source identity. Tile order is canonical (row-major) at the provider.
/// The policy names client-local geometry, never deterministic VMap/MMap parity.
pub(super) fn geometry_revision(
    map: u32,
    wdt: [u8; 32],
    tiles: &[((i32, i32), [u8; 32]); 9],
    models: &[(u32, [u8; 32])],
    inventory: [u8; 32],
) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(b"benilla-local-dry-flat-cell-static-los-corridor-v6\0");
    hash.update(map.to_le_bytes());
    hash.update(wdt);
    hash.update(inventory);
    for ((x, y), digest) in tiles {
        hash.update(x.to_le_bytes());
        hash.update(y.to_le_bytes());
        hash.update(digest);
    }
    hash.update((models.len() as u64).to_le_bytes());
    for (uid, source) in models {
        hash.update(uid.to_le_bytes());
        hash.update(source);
    }
    hash.finalize().into()
}

#[cfg(test)]
#[path = "player_environment_mailbox_tests.rs"]
mod player_environment_mailbox_tests;

#[cfg(test)]
#[path = "route_commit_mailbox_tests.rs"]
mod route_commit_mailbox_tests;

#[cfg(test)]
#[path = "staged_job_tests.rs"]
mod staged_job_tests;

#[cfg(test)]
#[path = "route_body_tests.rs"]
mod route_body_tests;

#[cfg(test)]
#[path = "source_location_mailbox_tests.rs"]
mod source_location_mailbox_tests;
