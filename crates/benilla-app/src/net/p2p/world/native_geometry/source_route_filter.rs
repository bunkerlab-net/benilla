// SPDX-License-Identifier: GPL-3.0-or-later
//! Loaded-product PathFinder createFilter/updateFilter dependency for an owned
//! dry ground or ground/water-capable creature. This supplies no route or admission.
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
use std::{sync::Arc, time::Instant};

#[path = "source_route_filter/resident.rs"]
pub(crate) mod resident;

pub(crate) struct SourceRouteFilterProvider {
    identity: Arc<()>,
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

/// Exact dry updateFilter evidence: retained filter bits plus fresh ordered queries.
/// It cannot be substituted for a source target-water gate or a newly created filter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RouteFilterUpdateDependency {
    request: RouteFilterRequest,
    owner_position: [u32; 3],
    liquid: [RouteLiquidObservation; 2],
    retained: benilla_native_nav::SourceOwnerFilter,
    continued_from_ticket: Option<u64>,
}
impl RouteFilterUpdateDependency {
    pub(crate) fn request(&self) -> &RouteFilterRequest {
        &self.request
    }
    pub(crate) fn owner_position(&self) -> [u32; 3] {
        self.owner_position
    }
    pub(crate) fn liquid_observations(&self) -> &[RouteLiquidObservation; 2] {
        &self.liquid
    }
    pub(crate) fn filter(&self) -> benilla_native_nav::SourceOwnerFilter {
        self.retained
    }
    pub(crate) fn continued_from_ticket(&self) -> Option<u64> {
        self.continued_from_ticket
    }
}

/// Successful immutable context validation for one exact provider allocation.
/// Only an original live Route job may retain this between bounded stages.
pub(super) struct OwnedRouteFilterContext {
    provider: Arc<()>,
    context: AuthorityRequest,
}

impl OwnedRouteFilterContext {
    pub(super) fn matches_context(&self, context: &AuthorityRequest) -> bool {
        self.context == *context
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

    /// The terminal smooth-calculation policy is explicit and typed, never a supplied hash.
    pub(super) fn bind_with_point_path(
        session: &Session,
        profile: &Profile,
        snapshot: DryLiquidSnapshot,
        navigation: super::products::navigation::SnapshotBinding,
        configuration: super::source_route_point_path::PointPathConfiguration,
    ) -> Result<Self, Failure> {
        let expected = super::source_route_point_path::geometry_revision(
            snapshot.binding(),
            navigation,
            configuration,
        )?;
        Self::bind_revision(session, profile, snapshot, expected)
    }

    pub(super) fn bind_with_straight_path(
        session: &Session,
        profile: &Profile,
        snapshot: DryLiquidSnapshot,
        navigation: super::products::navigation::SnapshotBinding,
        configuration: super::source_route_straight_path::StraightPathConfiguration,
    ) -> Result<Self, Failure> {
        let expected = super::source_route_straight_path::geometry_revision(
            snapshot.binding(),
            navigation,
            configuration,
        )?;
        Self::bind_revision(session, profile, snapshot, expected)
    }

    /// Exact Chase composition, still restricted to the existing ground/dry owner.
    pub(super) fn bind_with_chase_gate(
        session: &Session,
        profile: &Profile,
        snapshot: DryLiquidSnapshot,
        navigation: super::products::navigation::SnapshotBinding,
        vmap: super::products::vmap::SnapshotBinding,
        configuration: super::source_route_chase_gate::ChaseGateConfiguration,
    ) -> Result<Self, Failure> {
        let expected = super::source_route_chase_gate::geometry_revision(
            snapshot.binding(),
            navigation,
            vmap,
            configuration,
        )?;
        Self::bind_revision(session, profile, snapshot, expected)
    }

    pub(super) fn bind_with_same_polygon(
        session: &Session,
        profile: &Profile,
        snapshot: DryLiquidSnapshot,
        binding: super::source_route_same_polygon::SamePolygonBinding,
    ) -> Result<Self, Failure> {
        if snapshot.binding() != binding.liquid {
            return Err(Failure::Binding);
        }
        Self::bind_revision(session, profile, snapshot, binding.geometry_revision()?)
    }

    pub(super) fn dry_liquid(&self) -> &DryLiquidSnapshot {
        &self.snapshot
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
            identity: Arc::new(()),
            session: session.clone(),
            profile: profile.clone(),
            snapshot,
        })
    }

    pub(crate) fn binding(&self) -> SnapshotBinding {
        self.snapshot.binding()
    }

    pub(super) fn prepare_context(
        &self,
        session: &Session,
        context: &AuthorityRequest,
    ) -> Result<OwnedRouteFilterContext, Failure> {
        self.begin(session, context)?;
        Ok(OwnedRouteFilterContext {
            provider: Arc::clone(&self.identity),
            context: context.clone(),
        })
    }

    /// The original Work already completed the shared route-context checks.
    /// Adoption still requires this provider's exact frozen session and profile.
    pub(super) fn prepare_job_context(
        &self,
        job: &super::core::RouteJob<'_>,
    ) -> Result<OwnedRouteFilterContext, Failure> {
        job.require_profile(&self.profile)?;
        if job.session() != &self.session {
            return Err(Failure::Binding);
        }
        Ok(OwnedRouteFilterContext {
            provider: Arc::clone(&self.identity),
            context: job.request().clone(),
        })
    }

    /// No replacement input can be paired with retained validation. The enclosing
    /// original job independently checks attachment, session, request and expiry.
    pub(super) fn reborrow(
        &self,
        context: &OwnedRouteFilterContext,
    ) -> Result<BoundRouteFilterQuery<'_>, Failure> {
        if !Arc::ptr_eq(&self.identity, &context.provider) {
            return Err(Failure::Binding);
        }
        Ok(BoundRouteFilterQuery {
            provider: self,
            context: context.context.clone(),
        })
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
            || session.player_environment != self.session.player_environment
            || session.source_location != self.session.source_location
            || session.same_polygon_route != self.session.same_polygon_route
        {
            return Err(Failure::Binding);
        }
        validate_route_context(session, &self.profile, context)?;
        Ok(BoundRouteFilterQuery {
            provider: self,
            context: context.clone(),
        })
    }
}

/// Shared request/body guards; this does not provide any loaded geometry evidence.
pub(super) fn validate_route_context(
    session: &Session,
    profile: &Profile,
    context: &AuthorityRequest,
) -> Result<(), Failure> {
    #[cfg(test)]
    let timing = super::products::source_height::freshness_timings::Timer::start(
        super::products::source_height::freshness_timings::Phase::ContextValidation,
    );
    validate_route_context_prefix(session, profile, context)?;
    let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
        return Err(Failure::UnsupportedQuery);
    };
    let height = initialized_owner_height(profile, route)?;
    #[cfg(test)]
    {
        let result = validate_route_context_body(session, profile, context, height);
        timing.finish(result.is_ok());
        result
    }
    #[cfg(not(test))]
    validate_route_context_body(session, profile, context, height)
}

pub(super) fn validate_route_context_prefix(
    session: &Session,
    profile: &Profile,
    context: &AuthorityRequest,
) -> Result<(), Failure> {
    validate_route_context_prefix_with(session, profile, context, |route, binding| {
        route
            .owner
            .validate_context(profile, binding)
            .map_err(|_| Failure::Binding)
    })
}

pub(super) fn validate_initialized_route_context(
    session: &Session,
    profile: &benilla_world_runtime::native::ValidatedRouteModelProfile,
    context: &AuthorityRequest,
) -> Result<Option<InitializedOwnerValidation>, Failure> {
    validate_route_context_prefix_with(session, profile.profile(), context, |route, binding| {
        if route.owner.initialization().is_none() {
            route
                .owner
                .validate_context(profile.profile(), binding)
                .map_err(|_| Failure::Binding)?;
            return Ok(None);
        }
        let owner = profile
            .validate_owner_context(Arc::new(route.clone()), binding)
            .map_err(|_| Failure::Binding)?;
        Ok(InitializedOwnerValidation::begin(owner))
    })
}

fn validate_route_context_prefix_with<R>(
    session: &Session,
    profile: &Profile,
    context: &AuthorityRequest,
    validate_owner: impl FnOnce(
        &benilla_world_runtime::native::CreatureRouteRequest,
        &benilla_world_runtime::native::Binding,
    ) -> Result<R, Failure>,
) -> Result<R, Failure> {
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
    let owner_validation = validate_owner(route, b)?;
    let q = &route.request;
    let now = b
        .tick
        .checked_mul(u64::from(profile.tick_ms))
        .and_then(|elapsed| profile.clock_origin_ms.checked_add(elapsed))
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
    let expected_flags =
        route_filter::initial_include_flags(capabilities).ok_or(Failure::UnsupportedQuery)?;
    if q.prior_cache
        .as_ref()
        .is_some_and(|cache| cache.include_flags != expected_flags || cache.exclude_flags != 0)
    {
        return Err(Failure::Binding);
    }
    let source = profile
        .source_motion
        .as_ref()
        .ok_or(Failure::UnresolvedBody)?;
    profile.npc(npc.spawn).ok_or(Failure::Binding)?;
    source.npc(npc.spawn).map_err(|_| Failure::UnresolvedBody)?;
    session
        .bodies
        .get(&npc.spawn)
        .ok_or(Failure::UnresolvedBody)?;
    Ok(owner_validation)
}

pub(super) fn validate_route_context_body(
    session: &Session,
    profile: &Profile,
    context: &AuthorityRequest,
    initialized_height: Option<benilla_sim::creature_ai::Real>,
) -> Result<(), Failure> {
    let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
        return Err(Failure::UnsupportedQuery);
    };
    let npc = context.binding.npc.ok_or(Failure::Binding)?;
    let q = &route.request;
    let source = profile
        .source_motion
        .as_ref()
        .ok_or(Failure::UnresolvedBody)?;
    let npc_profile = profile.npc(npc.spawn).ok_or(Failure::Binding)?;
    let npc_motion = source.npc(npc.spawn).map_err(|_| Failure::UnresolvedBody)?;
    let body = session
        .bodies
        .get(&npc.spawn)
        .ok_or(Failure::UnresolvedBody)?;
    let owner_height = initialized_height.unwrap_or(body.height);
    if body.radius != npc_profile.ai.bounding_radius
        || body.height != npc_motion.npc_policy.settings.collision_height
        || q.owner_collision_height != owner_height
    {
        return Err(Failure::UnresolvedBody);
    }
    if let Some(target) = q.target {
        let player = session.player.ok_or(Failure::UnresolvedBody)?;
        if target.id != SOLO_UNIT {
            return Err(Failure::Binding);
        }
        target.pose.validate().map_err(|_| Failure::Invalid)?;
        if player.body.radius != profile.player.bounding_radius
            || player.body.height != source.player_collision_height
            || target.bounding_radius != player.body.radius
            || target.collision_height != player.body.height
            || target.combat_reach != profile.player.combat_reach
        {
            return Err(Failure::UnresolvedBody);
        }
    }
    Ok(())
}

/// A dry initialization retains the selected model for this life. The frozen
/// roster still describes the initial profile; it cannot replace a later model's
/// height when deriving the concrete query body. Callers first validate context.
pub(super) fn initialized_owner_height(
    profile: &Profile,
    route: &benilla_world_runtime::native::CreatureRouteRequest,
) -> Result<Option<benilla_sim::creature_ai::Real>, Failure> {
    let Some(initialization) = route.owner.initialization() else {
        return Ok(None);
    };
    let selected = route
        .require_collision_width(profile)
        .map_err(|_| Failure::Binding)?
        .validated_model_selection(profile, route.owner.npc(), Some(initialization))
        .map_err(|_| Failure::Binding)?;
    if selected.bounding_radius().bits()
        != profile
            .npc(route.owner.npc().spawn)
            .ok_or(Failure::Binding)?
            .ai
            .bounding_radius
            .bits()
    {
        return Err(Failure::UnresolvedBody);
    }
    // Initialization validates the immutable raw model source; this method
    // validates the generation-owned width/height digest, a distinct domain.
    let height = route
        .require_collision_height(profile)
        .map_err(|_| Failure::Binding)?;
    initialized_height_for(profile, route, height).map(Some)
}

fn initialized_height_for(
    profile: &Profile,
    route: &benilla_world_runtime::native::CreatureRouteRequest,
    height: &benilla_world_runtime::native::CreatureCollisionHeightWitness,
) -> Result<benilla_sim::creature_ai::Real, Failure> {
    let initial = &route
        .owner
        .initialization()
        .ok_or(Failure::Binding)?
        .request;
    let raw_model_source = profile
        .creature_collision_heights
        .as_ref()
        .and_then(|catalog| catalog.source(route.owner.npc().spawn))
        .ok_or(Failure::Binding)?
        .width_source_binding();
    if route.owner.inhabit_type() != 3
        || initial.model_source != raw_model_source
        || initial.binding.npc != Some(height.npc())
        || initial.native_display_id != height.native_display_id()
        || initial.model_id != height.model_id()
        || initial.object_scale != height.object_scale()
        || initial.raw_height != height.raw_height()
        || initial.collision_height != height.height()
    {
        return Err(Failure::Binding);
    }
    benilla_sim::creature_ai::Real::from_bits(height.height().bits())
        .map_err(|_| Failure::UnresolvedBody)
}

/// Immutable initialization checks retained only by the original mailbox Work.
/// A completed receipt binds the exact profile and route, not a digest-only claim.
#[derive(Clone)]
pub(super) struct InitializedOwnerValidation {
    owner: benilla_world_runtime::native::ValidatedRouteModelOwner,
    width: Option<benilla_world_runtime::native::ValidatedRouteModelWidth>,
    selection: Option<benilla_sim::creature_init::ModelSelectionReceipt>,
    height: Option<benilla_world_runtime::native::CreatureCollisionHeightWitness>,
}
pub(super) struct ValidatedInitializedOwner<'a> {
    pub selection: &'a benilla_sim::creature_init::ModelSelectionReceipt,
    pub width: &'a benilla_world_runtime::native::CreatureCollisionWidthWitness,
    pub height: &'a benilla_world_runtime::native::CreatureCollisionHeightWitness,
}
impl InitializedOwnerValidation {
    fn begin(owner: benilla_world_runtime::native::ValidatedRouteModelOwner) -> Option<Self> {
        owner.route().owner.initialization()?;
        Some(Self {
            owner,
            width: None,
            selection: None,
            height: None,
        })
    }
    pub(super) fn validate_width(&self) -> Result<Self, Failure> {
        if self.width.is_some() || self.selection.is_some() || self.height.is_some() {
            return Err(Failure::Binding);
        }
        let width = self.owner.validate_width().map_err(|_| Failure::Binding)?;
        let mut next = self.clone();
        next.width = Some(width);
        Ok(next)
    }
    pub(super) fn validate_selection(&self) -> Result<Self, Failure> {
        if self.selection.is_some() || self.height.is_some() {
            return Err(Failure::Binding);
        }
        let selected = self
            .width
            .as_ref()
            .ok_or(Failure::Binding)?
            .width()
            .validated_model_selection(
                self.owner.profile(),
                self.owner.route().owner.npc(),
                self.owner.route().owner.initialization(),
            )
            .map_err(|_| Failure::Binding)?;
        if selected.bounding_radius().bits()
            != self
                .owner
                .profile()
                .npc(self.owner.route().owner.npc().spawn)
                .ok_or(Failure::Binding)?
                .ai
                .bounding_radius
                .bits()
        {
            return Err(Failure::UnresolvedBody);
        }
        let mut next = self.clone();
        next.selection = Some(selected);
        Ok(next)
    }
    pub(super) fn validate_height(&self) -> Result<Self, Failure> {
        if self.selection.is_none() || self.height.is_some() {
            return Err(Failure::Binding);
        }
        let height = self
            .width
            .as_ref()
            .ok_or(Failure::Binding)?
            .require_height()
            .map_err(|_| Failure::Binding)?;
        initialized_height_for(self.owner.profile(), self.owner.route(), height)?;
        let mut next = self.clone();
        next.height = Some(height.clone());
        Ok(next)
    }
    pub(super) fn for_route<'a>(
        &'a self,
        profile: &Profile,
        route: &benilla_world_runtime::native::CreatureRouteRequest,
    ) -> Result<ValidatedInitializedOwner<'a>, Failure> {
        if !self.owner.matches_route(profile, route) {
            return Err(Failure::Binding);
        }
        Ok(ValidatedInitializedOwner {
            selection: self.selection.as_ref().ok_or(Failure::Binding)?,
            width: self.width.as_ref().ok_or(Failure::Binding)?.width(),
            height: self.height.as_ref().ok_or(Failure::Binding)?,
        })
    }
    pub(super) fn target_height_for(
        &self,
        profile: &Profile,
        route: &benilla_world_runtime::native::CreatureRouteRequest,
        world: [u8; 16],
        player: benilla_world_runtime::PlayerId,
    ) -> Result<
        &benilla_world_runtime::native::PlayerCollisionHeightWitness,
        benilla_world_runtime::native::NativeError,
    > {
        self.for_route(profile, route)
            .map_err(|_| benilla_world_runtime::native::NativeError::AuthorityBinding)?;
        self.owner.require_target_collision_height(world, player)
    }
    pub(super) fn target_water_for(
        &self,
        profile: &Profile,
        route: &benilla_world_runtime::native::CreatureRouteRequest,
        world: [u8; 16],
        player: benilla_world_runtime::PlayerId,
    ) -> Result<
        &benilla_world_runtime::native::PlayerEnvironmentTarget,
        benilla_world_runtime::native::NativeError,
    > {
        self.for_route(profile, route)
            .map_err(|_| benilla_world_runtime::native::NativeError::AuthorityBinding)?;
        self.owner.require_target_water(world, player)
    }
    pub(super) fn owner_ignore_for(
        &self,
        profile: &Profile,
        route: &benilla_world_runtime::native::CreatureRouteRequest,
    ) -> Result<&benilla_world_runtime::native::OwnerIgnorePathfindingReceipt, Failure> {
        self.for_route(profile, route)?;
        self.owner
            .require_owner_ignore_pathfinding()
            .map_err(|_| Failure::Binding)
    }
    /// Reuse only the completed immutable owner/profile checks for this exact
    /// live request. Dispatch metadata, configuration, request hash and outer
    /// binding retain their full ordered validation before any native query.
    pub(super) fn dispatch_for(
        &self,
        profile: &Profile,
        route: &benilla_world_runtime::native::CreatureRouteRequest,
        binding: &benilla_world_runtime::native::Binding,
    ) -> Result<&benilla_world_runtime::native::PathfinderDispatchReceipt, Failure> {
        self.for_route(profile, route)?;
        self.owner
            .require_pathfinder_dispatch(binding)
            .map_err(|_| Failure::Binding)
    }
    pub(super) fn height_for(
        &self,
        profile: &Profile,
        route: &benilla_world_runtime::native::CreatureRouteRequest,
    ) -> Result<benilla_sim::creature_ai::Real, Failure> {
        benilla_sim::creature_ai::Real::from_bits(
            self.for_route(profile, route)?.height.height().bits(),
        )
        .map_err(|_| Failure::UnresolvedBody)
    }
}

impl BoundRouteFilterQuery<'_> {
    /// PathFinder.cpp:1092-1104: in the loaded ground/dry domain both predicates
    /// are false, so updateFilter does not write or recreate the retained filter.
    pub(super) fn query_retained_update(
        &self,
        request: RouteFilterRequest,
        retained: benilla_native_nav::SourceOwnerFilter,
        previous: Option<&RouteFilterUpdateDependency>,
        deadline: Instant,
    ) -> Result<RouteFilterUpdateDependency, Failure> {
        let Query::SourceMotion(MotionQuery::Route(route)) = &self.context.query else {
            return Err(Failure::UnsupportedQuery);
        };
        let expected_flags = route_filter::initial_include_flags(
            route.owner.capabilities().map_err(|_| Failure::Binding)?,
        )
        .ok_or(Failure::UnsupportedQuery)?;
        if request.context != self.context
            || request.snapshot != self.provider.binding()
            || request.first_ticket == 0
            || retained.include_flags != expected_flags
            || retained.exclude_flags != 0
        {
            return Err(Failure::Binding);
        }
        let second_ticket = request
            .first_ticket
            .checked_add(1)
            .ok_or(Failure::Invalid)?;
        if let Some(previous) = previous {
            if previous.request.context != request.context
                || previous.request.snapshot != request.snapshot
                || previous.request.first_ticket.checked_add(4) != Some(request.first_ticket)
                || previous.retained != retained
            {
                return Err(Failure::Binding);
            }
        }
        let point = route.owner_position;
        let position = [point.x.bits(), point.y.bits(), point.z.bits()];
        let mut observations = Vec::with_capacity(2);
        for (mask, ticket) in [
            (route_filter::LiquidMask::AllLiquids, request.first_ticket),
            (route_filter::LiquidMask::WaterOrOcean, second_ticket),
        ] {
            if Instant::now() >= deadline {
                return Err(Failure::Deadline);
            }
            let query = LiquidRequest {
                snapshot: request.snapshot,
                ticket,
                position,
            };
            let reply = self
                .provider
                .snapshot
                .query(query, deadline)
                .map_err(Failure::product)?;
            if reply.request != query {
                return Err(Failure::Binding);
            }
            // Actual loaded dry GridMap provides CanCheckLiquidLevel and NO_WATER
            // for each mask. No getNavTerrain call or include-flags assignment occurs.
            match reply.status {
                LiquidStatus::NoWater => (),
            }
            observations.push(RouteLiquidObservation { mask, reply });
        }
        if Instant::now() >= deadline {
            return Err(Failure::Deadline);
        }
        Ok(RouteFilterUpdateDependency {
            request,
            owner_position: position,
            liquid: observations.try_into().map_err(|_| Failure::Binding)?,
            retained,
            continued_from_ticket: previous.map(|p| p.request.first_ticket),
        })
    }

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
            return Err(Failure::Deadline);
        }
        let Query::SourceMotion(MotionQuery::Route(route)) = &self.context.query else {
            return Err(Failure::UnsupportedQuery);
        };
        let point = route.owner_position;
        let position = [point.x, point.y, point.z].map(|v| v.as_f32().to_bits());
        let capabilities = route.owner.capabilities().map_err(|_| Failure::Binding)?;
        let mut liquid = [None, None];
        let mut next = 0;
        let filter = route_filter::create_dry_filter(capabilities, |mask| {
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
                .map_err(Failure::product)?;
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
            return Err(Failure::Deadline);
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
    hash.update(b"benilla-source-route-owner-dry-filter-dependency-v2");
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
