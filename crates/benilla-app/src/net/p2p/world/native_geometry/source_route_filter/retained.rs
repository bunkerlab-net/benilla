// SPDX-License-Identifier: GPL-3.0-or-later
//! Component-only one-retained Chase calculation from an opaque cold predecessor.
//! Current clearance is fresh; historical filter/native state supplies ancestry only.
use super::super::super::{
    core::Body,
    products::vmap::{RayReply, StaticRay},
};
use super::{
    chase_los::{water::ResidentChaseWaterDependency, ResidentChaseLosDecision},
    cold::ResidentSamePolygonObservation,
    joined::ResidentJoinedColdDependency,
    *,
};
use benilla_native_nav::{
    LoadedSourceSamePolygon, LoadedSourceTilePresence, SourceSamePolygonBudget,
    SourceSamePolygonConfig,
};
use benilla_world_runtime::{native::OwnerIgnorePathfindingReceipt, PlayerId};

#[path = "second_retained.rs"]
pub(crate) mod second_retained;

/// Read-only stage markers; an observer cannot supply evidence or bypass a refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResidentRetainedPhase {
    Model,
    ChaseRay,
    ChaseRayComplete,
    WaterGate,
    OwnerWater,
    TargetWater,
    Selected,
    OwnerIgnore,
    Tiles,
    FilterAllLiquids,
    FilterWaterOrOcean,
    Terminal,
}

/// Unlike ResidentFilterDependency, this contains no purported current point reply.
/// The fresh gate identity cannot be confused with the historical calculate identity.
pub(super) struct ResidentRetainedRoot {
    identity: Rc<()>,
    provider: Rc<()>,
    predecessor: Rc<()>,
    context: AuthorityRequest,
    expires: Instant,
}
impl ResidentRetainedRoot {
    pub(super) fn identity(&self) -> &Rc<()> {
        &self.identity
    }
    pub(super) fn check_current(
        &self,
        provider: &ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<Instant, ResidentFilterError> {
        if !Rc::ptr_eq(&self.provider, &provider.identity) || self.context != *context {
            return Err(Failure::Binding.into());
        }
        let saved = self
            .context
            .require_pathfinder_dispatch(&provider.profile)
            .map_err(|_| Failure::Binding)?;
        let current = context
            .require_pathfinder_dispatch(&provider.profile)
            .map_err(|_| Failure::Binding)?;
        if !saved.same_live_dispatch(current) {
            return Err(Failure::Binding.into());
        }
        let deadline = deadline.min(self.expires);
        provider.check_context(context, deadline, &cancelled)?;
        provider
            .height
            .verify_unchanged(&cancelled)
            .map_err(ResidentFilterError::Source)?;
        provider.check_context(context, deadline, &cancelled)?;
        Ok(deadline)
    }
}

/// Own fresh prerequisites and the real native successor, never a reconstructed cache.
/// The concrete candidate separately owns its ACK-selected cold predecessor by Rc.
pub(crate) struct ResidentJoinedRetainedDependency {
    root: ResidentRetainedRoot,
    chase: ResidentChaseWaterDependency,
    observations: [PointNoWaterReply; 2],
    owner_ignore: OwnerIgnorePathfindingReceipt,
    tiles: LoadedSourceTilePresence,
    terminal: LoadedSourceSamePolygon,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResidentJoinedRetainedObservation {
    pub(crate) terminal: ResidentSamePolygonObservation,
    pub(crate) body: Body,
}
impl ResidentJoinedRetainedDependency {
    pub(crate) fn deadline(&self, deadline: Instant) -> Instant {
        deadline.min(self.root.expires)
    }
    /// Historical native equality only, without renewing any source observation.
    pub(crate) fn check_historical_native_cache_for_test(
        &self,
        expected: &motion::PathFinderCache,
    ) -> Result<(), ResidentFilterError> {
        check_historical_native_cache(&self.terminal, &self.root.context, 2, expected)
    }
    /// Logical owned backing/trace bytes, excluding shared query/source allocations.
    pub(crate) fn storage_footprint_for_test(&self) -> usize {
        native_storage_footprint(&self.terminal) + 2 * context_cache_bytes(&self.root.context)
    }
}

impl ResidentFilterProvider<'_, '_, '_, '_> {
    /// No constructor/filter source work precedes the current straight Chase gate.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn observe_joined_retained(
        &mut self,
        prior: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
        player: PlayerId,
        first_ticket: u64,
        budget: SourceSamePolygonBudget,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<ResidentJoinedRetainedDependency, ResidentFilterError> {
        self.observe_joined_retained_observed(
            prior,
            context,
            player,
            first_ticket,
            budget,
            deadline,
            cancelled,
            |_| {},
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn observe_joined_retained_observed(
        &mut self,
        prior: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
        player: PlayerId,
        first_ticket: u64,
        budget: SourceSamePolygonBudget,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        mut observe: impl FnMut(ResidentRetainedPhase),
    ) -> Result<ResidentJoinedRetainedDependency, ResidentFilterError> {
        self.check_retained_predecessor(prior, context, deadline, &cancelled)?;
        if first_ticket == 0
            || first_ticket.checked_add(4).is_none()
            || first_ticket
                <= prior.retained_cold().historical_observation().observations[1]
                    .request()
                    .ticket
        {
            return Err(Failure::Binding.into());
        }
        let root = ResidentRetainedRoot {
            identity: Rc::new(()),
            provider: Rc::clone(&self.identity),
            predecessor: Rc::clone(
                &prior
                    .retained_cold()
                    .historical_observation()
                    .observation_identity,
            ),
            context: context.clone(),
            expires: deadline,
        };
        prior
            .retained_chase()
            .require_retained_model(self, context, player)?;
        let prefix = self.observe_retained_chase_los(
            &root,
            context,
            player,
            first_ticket,
            deadline,
            &cancelled,
            |phase| {
                observe(match phase {
                    0 => ResidentRetainedPhase::Model,
                    1 => ResidentRetainedPhase::ChaseRay,
                    _ => ResidentRetainedPhase::ChaseRayComplete,
                })
            },
        )?;
        if !prior.retained_chase().same_retained_model(&prefix) {
            return Err(Failure::Binding.into());
        }
        if self
            .los_for(&prefix, context, deadline, &cancelled)?
            .decision
            != ResidentChaseLosDecision::ClearPrefix
        {
            return Err(Failure::UnsupportedQuery.into());
        }
        let chase = self.observe_chase_water_observed(
            &prefix,
            context,
            first_ticket + 1,
            deadline,
            &cancelled,
            |phase| {
                observe(match phase {
                    0 => ResidentRetainedPhase::WaterGate,
                    1 => ResidentRetainedPhase::OwnerWater,
                    _ => ResidentRetainedPhase::TargetWater,
                })
            },
        )?;
        let (deadline, water_ticket) =
            chase.retained_prerequisites(self, &root, context, deadline, &cancelled)?;
        if water_ticket != first_ticket + 1 {
            return Err(Failure::Binding.into());
        }
        self.check_retained_predecessor(prior, context, deadline, &cancelled)?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::UnsupportedQuery.into());
        };
        // The actual registry retains the original allocation, selecting this live
        // request's origin/goal without rewriting launch_origin on a replan.
        let mut constructor = self
            .query
            .retain_constructor(context)
            .map_err(ResidentFilterError::Query)?;
        let selected = constructor
            .select_for_calculate(deadline)
            .map_err(ResidentFilterError::Query)?;
        observe(ResidentRetainedPhase::Selected);
        check_time(deadline, &cancelled)?;
        let owner_ignore = route
            .require_owner_ignore_pathfinding(&self.profile)
            .map_err(|_| Failure::Binding)?
            .clone();
        if owner_ignore.ignore_pathfinding() {
            return Err(Failure::UnsupportedQuery.into());
        }
        observe(ResidentRetainedPhase::OwnerIgnore);
        check_time(deadline, &cancelled)?;
        let tiles = selected.tile_presence(deadline).map_err(native_error)?;
        selected.check_tile_presence(&tiles).map_err(native_error)?;
        if !tiles.both_present() {
            return Err(Failure::MissingGeometry.into());
        }
        observe(ResidentRetainedPhase::Tiles);
        check_time(deadline, &cancelled)?;
        let retained_filter = prior.retained_cold().historical_observation().filter;
        let (observations, filter) = point_observations(
            self.height,
            context,
            first_ticket + 2,
            deadline,
            &cancelled,
            Some(retained_filter),
            &mut |phase| {
                observe(if phase == 0 {
                    ResidentRetainedPhase::FilterAllLiquids
                } else {
                    ResidentRetainedPhase::FilterWaterOrOcean
                })
            },
        )?;
        check_time(deadline, &cancelled)?;
        // This native API cannot replace the filter carried by its predecessor.
        if filter
            != prior
                .retained_cold()
                .native_terminal()
                .compatibility()
                .filter()
        {
            return Err(Failure::Binding.into());
        }
        let terminal = selected
            .loaded_retained_same_polygon(
                prior.retained_cold().native_terminal(),
                SourceSamePolygonConfig {
                    normalize_z: false,
                    ignore_normalization: false,
                },
                budget,
                deadline,
            )
            .map_err(ResidentFilterError::Terminal)?;
        terminal.check_query(&selected).map_err(native_error)?;
        observe(ResidentRetainedPhase::Terminal);
        check_time(deadline, &cancelled)?;
        let result = ResidentJoinedRetainedDependency {
            root,
            chase,
            observations,
            owner_ignore,
            tiles,
            terminal,
        };
        self.retained_for(&result, prior, context, deadline, cancelled)?;
        Ok(result)
    }

    /// Historical points/deadlines never answer a current query. Only immutable
    /// original-owner/native/filter provenance is used to select this predecessor.
    fn check_retained_predecessor(
        &self,
        prior: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<(), ResidentFilterError> {
        self.require_one_retained_route()?;
        self.check_context(context, deadline, &cancelled)?;
        let previous = prior.retained_cold().historical_observation();
        if !Rc::ptr_eq(&self.identity, &previous.identity) {
            return Err(Failure::Binding.into());
        }
        self.check_context(&previous.context, deadline, &cancelled)?;
        let old = previous
            .context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let current = context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let Query::SourceMotion(MotionQuery::Route(old_route)) = &previous.context.query else {
            return Err(Failure::Binding.into());
        };
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::UnsupportedQuery.into());
        };
        if !old.first_construction()
            || current.first_construction()
            || old.constructor_kind() != motion::PathfinderConstructorKind::ChaseLazy
            || current.constructor_kind() != old.constructor_kind()
            || current.construction_binding() != old.construction_binding()
            || current.configuration_binding() != old.configuration_binding()
            || current.npc() != old.npc()
            || route.owner != old_route.owner
            || route.request.binding.query_id <= old_route.request.binding.query_id
            || route.request.binding.now_ms <= old_route.request.binding.now_ms
            || route.request.kind != motion::RouteKind::ChaseReplan
            || route.request.point_limit != motion::PATHFINDER_CACHE_POINT_LIMIT
            || old_route.request.prior_cache.is_some()
            || !prior.retained_cold().historical_calculate_ready()
        {
            return Err(Failure::Binding.into());
        }
        let native = prior.retained_cold().native_terminal().compatibility();
        if native.is_retained()
            || native.storage().len() != usize::from(motion::PATHFINDER_CACHE_POINT_LIMIT) + 1
            || native.point_limit() != usize::from(motion::PATHFINDER_CACHE_POINT_LIMIT)
            || prior.retained_cold().native_terminal().query_capacity() != 1024
            || native.before_active_len() != 0
            || native.before_storage() != [0; motion::PATHFINDER_CACHE_POINT_LIMIT as usize]
            || native.filter() != previous.filter
            || prior.retained_cold().native_terminal().path_type() != 1
            || native.active_len() != 1
            || prior.retained_cold().native_terminal().config()
                != (SourceSamePolygonConfig {
                    normalize_z: false,
                    ignore_normalization: false,
                })
        {
            return Err(Failure::Binding.into());
        }
        let cache = self.retained_native_cache(
            prior.retained_cold().native_terminal(),
            &previous.context,
            1,
        )?;
        if route.request.prior_cache.as_deref() != Some(&cache) {
            return Err(Failure::Binding.into());
        }
        self.height
            .verify_unchanged(&cancelled)
            .map_err(ResidentFilterError::Source)?;
        self.check_context(context, deadline, &cancelled)
    }

    /// Only diagnostics escape; the actual native terminal stays in the dependency.
    pub(crate) fn retained_for(
        &self,
        dependency: &ResidentJoinedRetainedDependency,
        prior: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<ResidentJoinedRetainedObservation, ResidentFilterError> {
        let deadline = dependency
            .root
            .check_current(self, context, deadline, &cancelled)?;
        self.check_retained_predecessor(prior, context, deadline, &cancelled)?;
        if !Rc::ptr_eq(
            &dependency.root.predecessor,
            &prior
                .retained_cold()
                .historical_observation()
                .observation_identity,
        ) || !prior.retained_chase().same_water_model(&dependency.chase)
        {
            return Err(Failure::Binding.into());
        }
        let (_, water_ticket) = dependency.chase.retained_prerequisites(
            self,
            &dependency.root,
            context,
            deadline,
            &cancelled,
        )?;
        let tickets = dependency
            .observations
            .each_ref()
            .map(|reply| reply.request().ticket);
        let masks = dependency
            .observations
            .each_ref()
            .map(|reply| reply.request().mask);
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::UnsupportedQuery.into());
        };
        let terminal = &dependency.terminal;
        let native = terminal.compatibility();
        let prior_native = prior.retained_cold().native_terminal().compatibility();
        let bits = |point: Point| [point.x.bits(), point.y.bits(), point.z.bits()];
        if &dependency.owner_ignore
            != route
                .require_owner_ignore_pathfinding(&self.profile)
                .map_err(|_| Failure::Binding)?
            || dependency.owner_ignore.ignore_pathfinding()
            || !dependency.tiles.both_present()
            || tickets[0] != water_ticket.checked_add(1).ok_or(Failure::Invalid)?
            || tickets[1] != tickets[0].checked_add(1).ok_or(Failure::Invalid)?
            || masks != [15, 10]
            || dependency.observations.iter().any(|reply| {
                reply.request().binding != self.height.binding()
                    || reply.request().position != bits(route.owner_position)
            })
            || !native.is_retained()
            || native.before_storage() != prior_native.storage()
            || native.before_active_len() != prior_native.active_len()
            || native.filter() != prior_native.filter()
            || native.initial_storage() != prior_native.initial_storage()
            || native.initial_active_len() != prior_native.initial_active_len()
            || terminal.slot() != prior.retained_cold().native_terminal().slot()
            || terminal.query_capacity() != prior.retained_cold().native_terminal().query_capacity()
            || terminal.query_capacity() != 1024
            || terminal.config() != prior.retained_cold().native_terminal().config()
            || terminal.points() != [bits(route.request.origin), bits(route.request.goal)]
            || terminal.actual_end() != bits(route.request.goal)
            || terminal.path_type() != 1
            || native.active_len() != 1
            || native.point_limit() != usize::from(motion::PATHFINDER_CACHE_POINT_LIMIT)
        {
            return Err(Failure::Binding.into());
        }
        let npc = context.binding.npc.ok_or(Failure::Binding)?;
        let body = Body {
            radius: self
                .session
                .bodies
                .get(&npc.spawn)
                .ok_or(Failure::UnresolvedBody)?
                .radius,
            height: super::super::initialized_owner_height(&self.profile, route)?
                .ok_or(Failure::UnresolvedBody)?,
        };
        self.height
            .verify_unchanged(&cancelled)
            .map_err(ResidentFilterError::Source)?;
        self.check_context(context, deadline, &cancelled)?;
        Ok(ResidentJoinedRetainedObservation {
            terminal: ResidentSamePolygonObservation {
                points: terminal.points().try_into().map_err(|_| Failure::Binding)?,
                liquid_tickets: tickets,
                liquid_masks: masks,
                actual_end: terminal.actual_end(),
                path_type: terminal.path_type(),
                active_len: native.active_len(),
                filter: native.filter(),
                query_capacity: terminal.query_capacity(),
                before_storage_zero: native.before_active_len() == 0
                    && native.before_storage().len()
                        == usize::from(motion::PATHFINDER_CACHE_POINT_LIMIT)
                    && native.before_storage().iter().all(|id| *id == 0),
                compatibility_id: native.compatibility_id(),
                compatibility_version: native.compatibility_version(),
            },
            body,
        })
    }
    pub(crate) fn retained_cache(
        &self,
        dependency: &ResidentJoinedRetainedDependency,
        prior: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<motion::PathFinderCache, ResidentFilterError> {
        self.retained_for(dependency, prior, context, deadline, &cancelled)?;
        let cache = self.retained_native_cache(&dependency.terminal, context, 2)?;
        self.retained_for(dependency, prior, context, deadline, cancelled)?;
        Ok(cache)
    }
    /// Compare only owned historical ancestry/cache; neither renews source evidence
    /// nor authorizes another calculation. `context` is the retained calculation.
    pub(crate) fn check_historical_retained_cache_for_test(
        &self,
        dependency: &ResidentJoinedRetainedDependency,
        prior: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
        expected: &motion::PathFinderCache,
    ) -> Result<(), ResidentFilterError> {
        self.require_one_retained_route()?;
        let previous = prior.retained_cold().historical_observation();
        if !Rc::ptr_eq(&dependency.root.provider, &self.identity)
            || !Rc::ptr_eq(&previous.identity, &self.identity)
            || dependency.root.context != *context
            || !Rc::ptr_eq(&dependency.root.predecessor, &previous.observation_identity)
            || !prior.retained_chase().same_water_model(&dependency.chase)
        {
            return Err(Failure::Binding.into());
        }
        validate_route_context(&self.session, &self.profile, context)?;
        validate_route_context(&self.session, &self.profile, &previous.context)?;
        let saved = dependency
            .root
            .context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let current = context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let old = previous
            .context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::Binding.into());
        };
        let Query::SourceMotion(MotionQuery::Route(old_route)) = &previous.context.query else {
            return Err(Failure::Binding.into());
        };
        if !saved.same_live_dispatch(current)
            || !old.first_construction()
            || current.first_construction()
            || old.constructor_kind() != motion::PathfinderConstructorKind::ChaseLazy
            || current.constructor_kind() != old.constructor_kind()
            || current.construction_binding() != old.construction_binding()
            || current.configuration_binding() != old.configuration_binding()
            || current.npc() != old.npc()
            || route.owner != old_route.owner
            || route.request.binding.query_id <= old_route.request.binding.query_id
            || route.request.binding.now_ms <= old_route.request.binding.now_ms
            || route.request.kind != motion::RouteKind::ChaseReplan
            || route.request.point_limit != motion::PATHFINDER_CACHE_POINT_LIMIT
            || old_route.request.prior_cache.is_some()
            || !prior.retained_cold().historical_calculate_ready()
        {
            return Err(Failure::Binding.into());
        }
        let terminal = &dependency.terminal;
        let native = terminal.compatibility();
        let prior_terminal = prior.retained_cold().native_terminal();
        let prior_native = prior_terminal.compatibility();
        let bits = |point: Point| [point.x.bits(), point.y.bits(), point.z.bits()];
        if prior_native.is_retained()
            || prior_native.storage().len() != usize::from(motion::PATHFINDER_CACHE_POINT_LIMIT) + 1
            || prior_native.point_limit() != usize::from(motion::PATHFINDER_CACHE_POINT_LIMIT)
            || prior_native.before_active_len() != 0
            || prior_native.before_storage() != [0; motion::PATHFINDER_CACHE_POINT_LIMIT as usize]
            || prior_native.filter() != previous.filter
            || prior_terminal.path_type() != 1
            || prior_native.active_len() != 1
            || prior_terminal.config()
                != (SourceSamePolygonConfig {
                    normalize_z: false,
                    ignore_normalization: false,
                })
            || prior_terminal.points()
                != [bits(old_route.request.origin), bits(old_route.request.goal)]
            || prior_terminal.actual_end() != bits(old_route.request.goal)
            || &dependency.owner_ignore
                != route
                    .require_owner_ignore_pathfinding(&self.profile)
                    .map_err(|_| Failure::Binding)?
            || dependency.owner_ignore.ignore_pathfinding()
            || !dependency.tiles.both_present()
            || !native.is_retained()
            || native.before_storage() != prior_native.storage()
            || native.before_active_len() != prior_native.active_len()
            || native.filter() != prior_native.filter()
            || native.initial_storage() != prior_native.initial_storage()
            || native.initial_active_len() != prior_native.initial_active_len()
            || terminal.slot() != prior_terminal.slot()
            || terminal.query_capacity() != prior_terminal.query_capacity()
            || terminal.query_capacity() != 1024
            || terminal.config() != prior_terminal.config()
            || terminal.points() != [bits(route.request.origin), bits(route.request.goal)]
            || terminal.actual_end() != bits(route.request.goal)
            || terminal.path_type() != 1
            || native.active_len() != 1
            || native.point_limit() != usize::from(motion::PATHFINDER_CACHE_POINT_LIMIT)
        {
            return Err(Failure::Binding.into());
        }
        let prior_cache = self.retained_native_cache(prior_terminal, &previous.context, 1)?;
        if route.request.prior_cache.as_deref() != Some(&prior_cache)
            || self.retained_native_cache(terminal, context, 2)? != *expected
        {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }
    fn retained_native_cache(
        &self,
        terminal: &LoadedSourceSamePolygon,
        context: &AuthorityRequest,
        revision: u64,
    ) -> Result<motion::PathFinderCache, ResidentFilterError> {
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::Binding.into());
        };
        let native = terminal.compatibility();
        let binding = self.query.binding();
        let cache = motion::PathFinderCache {
            binding: motion::PathFinderCacheBinding {
                identity: route.request.binding.identity,
                owner_generation: route.owner.npc().generation,
                inventory: binding.inventory,
                source_epoch: binding.source_epoch,
                navigation_policy: self.session.geometry,
            },
            revision,
            source: motion::RouteSource::OwnerPathFinder,
            poly_refs: native.storage().to_vec(),
            active_len: u16::try_from(native.active_len()).map_err(|_| Failure::Invalid)?,
            point_limit: motion::PATHFINDER_CACHE_POINT_LIMIT,
            path_type: terminal.path_type(),
            requested_end: super::super::super::source_near_point::exact_point(
                terminal.actual_end(),
            )?,
            include_flags: native.filter().include_flags,
            exclude_flags: native.filter().exclude_flags,
        };
        cache
            .validate_for(route.request.binding.identity)
            .map_err(|_| Failure::Binding)?;
        Ok(cache)
    }
    /// Independent M2-inclusive source ray at OWNER height on both actual endpoints.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn retained_segment_los(
        &self,
        dependency: &ResidentJoinedRetainedDependency,
        prior: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
        ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<RayReply, ResidentFilterError> {
        let deadline = dependency.deadline(deadline);
        let observed = self.retained_for(dependency, prior, context, deadline, &cancelled)?;
        if ticket == 0 || ticket <= observed.terminal.liquid_tickets[1] {
            return Err(Failure::Binding.into());
        }
        let eye = |p: [u32; 3]| {
            [
                p[0],
                p[1],
                (f32::from_bits(p[2]) + observed.body.height.as_f32()).to_bits(),
            ]
        };
        let ray = self
            .height
            .chase_ray(
                ticket,
                eye(observed.terminal.points[0]),
                eye(observed.terminal.points[1]),
                deadline,
                &cancelled,
            )
            .map_err(ResidentFilterError::Source)?;
        if ray.request.ignore_m2 || !matches!(ray.result, StaticRay::Miss) {
            return Err(Failure::Obstacle.into());
        }
        self.retained_for(dependency, prior, context, deadline, cancelled)?;
        Ok(ray)
    }
}

fn native_error(error: benilla_native_nav::Error) -> ResidentFilterError {
    ResidentFilterError::Query(
        super::super::super::source_pathfinder_query::ConstructorQueryError::Native(error),
    )
}
fn check_time(deadline: Instant, cancelled: &impl Fn() -> bool) -> Result<(), ResidentFilterError> {
    if cancelled() {
        return Err(ResidentFilterError::Source(
            source_height::CompositionError::Cancelled,
        ));
    }
    if Instant::now() >= deadline {
        return Err(Failure::Deadline.into());
    }
    Ok(())
}

/// Compare an owned result to a diagnostic cache; this never constructs a dependency.
fn check_historical_native_cache(
    terminal: &LoadedSourceSamePolygon,
    context: &AuthorityRequest,
    revision: u64,
    expected: &motion::PathFinderCache,
) -> Result<(), ResidentFilterError> {
    let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
        return Err(Failure::Binding.into());
    };
    let prior = route.request.prior_cache.as_ref().ok_or(Failure::Binding)?;
    let native = terminal.compatibility();
    let bits = |point: Point| [point.x.bits(), point.y.bits(), point.z.bits()];
    expected
        .validate_for(route.request.binding.identity)
        .map_err(|_| Failure::Binding)?;
    if expected.binding != prior.binding
        || expected.revision != revision
        || prior.revision.checked_add(1) != Some(revision)
        || expected.source != motion::RouteSource::OwnerPathFinder
        || expected.poly_refs != native.storage()
        || usize::from(expected.active_len) != native.active_len()
        || usize::from(expected.point_limit) != native.point_limit()
        || expected.path_type != terminal.path_type()
        || bits(expected.requested_end) != terminal.actual_end()
        || expected.include_flags != native.filter().include_flags
        || expected.exclude_flags != native.filter().exclude_flags
        || native.before_storage() != prior.poly_refs
        || native.before_active_len() != usize::from(prior.active_len)
        || terminal.points() != [bits(route.request.origin), bits(route.request.goal)]
        || terminal.actual_end() != bits(route.request.goal)
        || !native.is_retained()
    {
        return Err(Failure::Binding.into());
    }
    Ok(())
}
fn context_cache_bytes(context: &AuthorityRequest) -> usize {
    let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
        return 0;
    };
    route
        .request
        .prior_cache
        .as_ref()
        .map_or(0, |cache| std::mem::size_of_val(cache.poly_refs.as_slice()))
}
fn native_storage_footprint(terminal: &LoadedSourceSamePolygon) -> usize {
    let native = terminal.compatibility();
    std::mem::size_of_val(native.initial_storage())
        + std::mem::size_of_val(native.before_storage())
        + std::mem::size_of_val(native.storage())
        + std::mem::size_of_val(native.trace())
        + std::mem::size_of_val(terminal.points())
}

/// Conservative logical cold backing/trace bytes; no native handle leaves its owner.
pub(crate) fn cold_storage_footprint_for_test(origin: &ResidentJoinedColdDependency) -> usize {
    native_storage_footprint(origin.retained_cold().native_terminal())
        + 3 * context_cache_bytes(&origin.retained_cold().historical_observation().context)
}
