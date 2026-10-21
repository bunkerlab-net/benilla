// SPDX-License-Identifier: GPL-3.0-or-later
//! Component-only second retained Chase calculation from the actual cache2 owner.
//! Cold ancestry is historical; fresh prerequisites and cache3 never retain cache2's owner.
use super::*;

/// Own one real native successor and bounded ancestry metadata, not prior dependencies.
pub(crate) struct ResidentJoinedSecondRetainedDependency {
    root: ResidentRetainedRoot,
    origin: Rc<()>,
    previous_context: AuthorityRequest,
    previous_filter_ticket: u64,
    chase: ResidentChaseWaterDependency,
    observations: [PointNoWaterReply; 2],
    owner_ignore: OwnerIgnorePathfindingReceipt,
    tiles: LoadedSourceTilePresence,
    terminal: LoadedSourceSamePolygon,
}
pub(crate) type ResidentJoinedSecondRetainedObservation = ResidentJoinedRetainedObservation;

impl ResidentJoinedSecondRetainedDependency {
    pub(crate) fn deadline(&self, deadline: Instant) -> Instant {
        deadline.min(self.root.expires)
    }
    /// Historical equality only; a copied cache cannot create native ownership.
    pub(crate) fn check_historical_native_cache_for_test(
        &self,
        expected: &motion::PathFinderCache,
    ) -> Result<(), ResidentFilterError> {
        check_historical_native_cache(&self.terminal, &self.root.context, 3, expected)
    }
    /// Logical owned backing/trace bytes, excluding shared query/source allocations.
    /// The water prefix owns a second copy of the current request, not an old owner.
    pub(crate) fn storage_footprint_for_test(&self) -> usize {
        native_storage_footprint(&self.terminal)
            + 2 * context_cache_bytes(&self.root.context)
            + context_cache_bytes(&self.previous_context)
    }
}

impl ResidentFilterProvider<'_, '_, '_, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn observe_joined_second_retained(
        &mut self,
        origin: &ResidentJoinedColdDependency,
        prior: &ResidentJoinedRetainedDependency,
        context: &AuthorityRequest,
        player: PlayerId,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<ResidentJoinedSecondRetainedDependency, ResidentFilterError> {
        self.observe_joined_second_retained_observed(
            origin,
            prior,
            context,
            player,
            first_ticket,
            deadline,
            cancelled,
            |_| {},
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn observe_joined_second_retained_observed(
        &mut self,
        origin: &ResidentJoinedColdDependency,
        prior: &ResidentJoinedRetainedDependency,
        context: &AuthorityRequest,
        player: PlayerId,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        mut observe: impl FnMut(ResidentRetainedPhase),
    ) -> Result<ResidentJoinedSecondRetainedDependency, ResidentFilterError> {
        self.check_second_retained_predecessor(origin, prior, context, deadline, &cancelled)?;
        let previous_filter_ticket = prior.observations[1].request().ticket;
        if first_ticket == 0
            || first_ticket.checked_add(4).is_none()
            || first_ticket <= previous_filter_ticket
        {
            return Err(Failure::Binding.into());
        }
        let budget = second_retained_budget(&prior.terminal)?;
        let root = ResidentRetainedRoot {
            identity: Rc::new(()),
            provider: Rc::clone(&self.identity),
            predecessor: Rc::clone(&prior.root.identity),
            context: context.clone(),
            expires: deadline,
        };
        origin
            .retained_chase()
            .require_retained_model(self, context, player)?;
        prior.chase.require_retained_model(self, context, player)?;
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
        if !origin.retained_chase().same_retained_model(&prefix)
            || !prior.chase.same_retained_model(&prefix)
        {
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
        self.check_second_retained_predecessor(origin, prior, context, deadline, &cancelled)?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::UnsupportedQuery.into());
        };
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
        let retained_filter = prior.terminal.compatibility().filter();
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
        if filter != prior.terminal.compatibility().filter() {
            return Err(Failure::Binding.into());
        }
        // Only the actual ACK-selected cache2 terminal is a native predecessor.
        let terminal = selected
            .loaded_retained_same_polygon(
                &prior.terminal,
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
        let result = ResidentJoinedSecondRetainedDependency {
            root,
            origin: Rc::clone(
                &origin
                    .retained_cold()
                    .historical_observation()
                    .observation_identity,
            ),
            previous_context: prior.root.context.clone(),
            previous_filter_ticket,
            chase,
            observations,
            owner_ignore,
            tiles,
            terminal,
        };
        self.check_historical_second_retained_predecessor_for_test(
            &result, origin, prior, context,
        )?;
        self.second_retained_for(&result, origin, context, deadline, cancelled)?;
        Ok(result)
    }

    fn check_second_retained_predecessor(
        &self,
        origin: &ResidentJoinedColdDependency,
        prior: &ResidentJoinedRetainedDependency,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<(), ResidentFilterError> {
        self.require_two_retained_routes()?;
        self.check_context(context, deadline, &cancelled)?;
        second_retained_budget(&prior.terminal)?;
        let cache = self.retained_native_cache(&prior.terminal, &prior.root.context, 2)?;
        // Historical provenance deliberately does not reuse prior/cold deadlines.
        self.check_historical_retained_cache_for_test(prior, origin, &prior.root.context, &cache)?;
        self.check_second_retained_contexts(origin, &prior.root.context, context)?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::UnsupportedQuery.into());
        };
        if route.request.prior_cache.as_deref() != Some(&cache) {
            return Err(Failure::Binding.into());
        }
        self.height
            .verify_unchanged(&cancelled)
            .map_err(ResidentFilterError::Source)?;
        self.check_context(context, deadline, &cancelled)
    }

    fn check_second_retained_contexts(
        &self,
        origin: &ResidentJoinedColdDependency,
        previous: &AuthorityRequest,
        context: &AuthorityRequest,
    ) -> Result<(), ResidentFilterError> {
        self.require_two_retained_routes()?;
        let cold = origin.retained_cold().historical_observation();
        if !Rc::ptr_eq(&self.identity, &cold.identity) {
            return Err(Failure::Binding.into());
        }
        for request in [&cold.context, previous, context] {
            validate_route_context(&self.session, &self.profile, request)?;
        }
        let old = cold
            .context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let prior = previous
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let current = context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let Query::SourceMotion(MotionQuery::Route(old_route)) = &cold.context.query else {
            return Err(Failure::Binding.into());
        };
        let Query::SourceMotion(MotionQuery::Route(prior_route)) = &previous.query else {
            return Err(Failure::Binding.into());
        };
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::UnsupportedQuery.into());
        };
        if !old.first_construction()
            || prior.first_construction()
            || current.first_construction()
            || old.constructor_kind() != motion::PathfinderConstructorKind::ChaseLazy
            || prior.constructor_kind() != old.constructor_kind()
            || current.constructor_kind() != old.constructor_kind()
            || prior.construction_binding() != old.construction_binding()
            || current.construction_binding() != old.construction_binding()
            || prior.configuration_binding() != old.configuration_binding()
            || current.configuration_binding() != old.configuration_binding()
            || prior.npc() != old.npc()
            || current.npc() != old.npc()
            || prior_route.owner != old_route.owner
            || route.owner != old_route.owner
            || previous.binding.tick <= cold.context.binding.tick
            || context.binding.tick <= previous.binding.tick
            || prior_route.request.binding.query_id <= old_route.request.binding.query_id
            || route.request.binding.query_id <= prior_route.request.binding.query_id
            || prior_route.request.binding.now_ms <= old_route.request.binding.now_ms
            || route.request.binding.now_ms <= prior_route.request.binding.now_ms
            || prior_route.request.kind != motion::RouteKind::ChaseReplan
            || route.request.kind != motion::RouteKind::ChaseReplan
            || prior_route.request.point_limit != motion::PATHFINDER_CACHE_POINT_LIMIT
            || route.request.point_limit != motion::PATHFINDER_CACHE_POINT_LIMIT
            || old_route.request.prior_cache.is_some()
            || !origin.retained_cold().historical_calculate_ready()
        {
            return Err(Failure::Binding.into());
        }
        let native = origin.retained_cold().native_terminal().compatibility();
        let terminal = origin.retained_cold().native_terminal();
        let bits = |point: Point| [point.x.bits(), point.y.bits(), point.z.bits()];
        if native.is_retained()
            || native.storage().len() != usize::from(motion::PATHFINDER_CACHE_POINT_LIMIT) + 1
            || native.point_limit() != usize::from(motion::PATHFINDER_CACHE_POINT_LIMIT)
            || native.before_active_len() != 0
            || native.before_storage() != [0; motion::PATHFINDER_CACHE_POINT_LIMIT as usize]
            || native.initial_active_len() != 0
            || native.initial_storage() != [0; motion::PATHFINDER_CACHE_POINT_LIMIT as usize]
            || native.filter() != cold.filter
            || native.active_len() != 1
            || terminal.query_capacity() != 1024
            || terminal.path_type() != 1
            || terminal.points() != [bits(old_route.request.origin), bits(old_route.request.goal)]
            || terminal.actual_end() != bits(old_route.request.goal)
            || terminal.config()
                != (SourceSamePolygonConfig {
                    normalize_z: false,
                    ignore_normalization: false,
                })
        {
            return Err(Failure::Binding.into());
        }
        let cold_cache = self.retained_native_cache(terminal, &cold.context, 1)?;
        if prior_route.request.prior_cache.as_deref() != Some(&cold_cache) {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }

    /// Fresh source checks need no previous warm dependency after publication.
    pub(crate) fn second_retained_for(
        &self,
        dependency: &ResidentJoinedSecondRetainedDependency,
        origin: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<ResidentJoinedSecondRetainedObservation, ResidentFilterError> {
        let deadline = dependency
            .root
            .check_current(self, context, deadline, &cancelled)?;
        self.check_second_retained_native(dependency, origin, context)?;
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
        let bits = |point: Point| [point.x.bits(), point.y.bits(), point.z.bits()];
        if tickets[0] != water_ticket.checked_add(1).ok_or(Failure::Invalid)?
            || tickets[1] != tickets[0].checked_add(1).ok_or(Failure::Invalid)?
            || water_ticket
                <= dependency
                    .previous_filter_ticket
                    .checked_add(1)
                    .ok_or(Failure::Invalid)?
            || masks != [15, 10]
            || dependency.observations.iter().any(|reply| {
                reply.request().binding != self.height.binding()
                    || reply.request().position != bits(route.owner_position)
            })
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
            height: super::super::super::initialized_owner_height(&self.profile, route)?
                .ok_or(Failure::UnresolvedBody)?,
        };
        self.height
            .verify_unchanged(&cancelled)
            .map_err(ResidentFilterError::Source)?;
        self.check_context(context, deadline, &cancelled)?;
        let terminal = &dependency.terminal;
        let native = terminal.compatibility();
        Ok(ResidentJoinedSecondRetainedObservation {
            terminal: ResidentSamePolygonObservation {
                points: terminal.points().try_into().map_err(|_| Failure::Binding)?,
                liquid_tickets: tickets,
                liquid_masks: masks,
                actual_end: terminal.actual_end(),
                path_type: terminal.path_type(),
                active_len: native.active_len(),
                filter: native.filter(),
                query_capacity: terminal.query_capacity(),
                before_storage_zero: false,
                compatibility_id: native.compatibility_id(),
                compatibility_version: native.compatibility_version(),
            },
            body,
        })
    }
    pub(crate) fn second_retained_cache(
        &self,
        dependency: &ResidentJoinedSecondRetainedDependency,
        origin: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<motion::PathFinderCache, ResidentFilterError> {
        self.second_retained_for(dependency, origin, context, deadline, &cancelled)?;
        let revision = second_retained_revision(context)?;
        let cache = self.retained_native_cache(&dependency.terminal, context, revision)?;
        self.second_retained_for(dependency, origin, context, deadline, cancelled)?;
        Ok(cache)
    }
    /// Compare the actual historical predecessor, not an equal diagnostic copy.
    pub(crate) fn check_historical_second_retained_predecessor_for_test(
        &self,
        dependency: &ResidentJoinedSecondRetainedDependency,
        origin: &ResidentJoinedColdDependency,
        prior: &ResidentJoinedRetainedDependency,
        context: &AuthorityRequest,
    ) -> Result<(), ResidentFilterError> {
        self.check_second_retained_native(dependency, origin, context)?;
        let cache = self.retained_native_cache(&prior.terminal, &prior.root.context, 2)?;
        self.check_historical_retained_cache_for_test(prior, origin, &prior.root.context, &cache)?;
        let native = dependency.terminal.compatibility();
        let prior_native = prior.terminal.compatibility();
        if !Rc::ptr_eq(&dependency.root.predecessor, &prior.root.identity)
            || dependency.previous_context != prior.root.context
            || dependency.previous_filter_ticket != prior.observations[1].request().ticket
            || !prior.chase.same_water_model(&dependency.chase)
            || native.before_storage() != prior_native.storage()
            || native.before_active_len() != prior_native.active_len()
            || native.filter() != prior_native.filter()
            || native.initial_storage() != prior_native.initial_storage()
            || native.initial_active_len() != prior_native.initial_active_len()
            || dependency.terminal.slot() != prior.terminal.slot()
            || dependency.terminal.query_capacity() != prior.terminal.query_capacity()
            || dependency.terminal.config() != prior.terminal.config()
        {
            return Err(Failure::Binding.into());
        }
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::Binding.into());
        };
        if route.request.prior_cache.as_deref() != Some(&cache) {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }
    /// Whole native/current-cache diagnostic remains valid after dropping cache2's owner.
    pub(crate) fn check_historical_second_retained_cache_for_test(
        &self,
        dependency: &ResidentJoinedSecondRetainedDependency,
        origin: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
        expected: &motion::PathFinderCache,
    ) -> Result<(), ResidentFilterError> {
        self.check_second_retained_native(dependency, origin, context)?;
        dependency.check_historical_native_cache_for_test(expected)?;
        if self.retained_native_cache(
            &dependency.terminal,
            context,
            second_retained_revision(context)?,
        )? != *expected
        {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }

    fn check_second_retained_native(
        &self,
        dependency: &ResidentJoinedSecondRetainedDependency,
        origin: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
    ) -> Result<(), ResidentFilterError> {
        self.check_second_retained_contexts(origin, &dependency.previous_context, context)?;
        let cold = origin.retained_cold().historical_observation();
        if !Rc::ptr_eq(&dependency.root.provider, &self.identity)
            || !Rc::ptr_eq(&dependency.origin, &cold.observation_identity)
            || Rc::ptr_eq(&dependency.root.identity, &dependency.root.predecessor)
            || Rc::ptr_eq(&dependency.root.identity, &dependency.origin)
            || Rc::ptr_eq(&dependency.root.predecessor, &dependency.origin)
            || dependency.root.context != *context
            || !origin.retained_chase().same_water_model(&dependency.chase)
        {
            return Err(Failure::Binding.into());
        }
        let saved = dependency
            .root
            .context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let current = context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        if !saved.same_live_dispatch(current) {
            return Err(Failure::Binding.into());
        }
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::Binding.into());
        };
        let Query::SourceMotion(MotionQuery::Route(previous)) = &dependency.previous_context.query
        else {
            return Err(Failure::Binding.into());
        };
        let cache = route.request.prior_cache.as_ref().ok_or(Failure::Binding)?;
        cache
            .validate_for(route.request.binding.identity)
            .map_err(|_| Failure::Binding)?;
        let cold_cache = previous
            .request
            .prior_cache
            .as_ref()
            .ok_or(Failure::Binding)?;
        let terminal = &dependency.terminal;
        let native = terminal.compatibility();
        let cold_terminal = origin.retained_cold().native_terminal();
        let cold_native = cold_terminal.compatibility();
        let bits = |point: Point| [point.x.bits(), point.y.bits(), point.z.bits()];
        second_retained_lookup_budget(native.before_storage().len())?;
        second_retained_revision(context)?;
        if cache.binding != cold_cache.binding
            || cache.source != motion::RouteSource::OwnerPathFinder
            || cache.poly_refs != native.before_storage()
            || usize::from(cache.active_len) != native.before_active_len()
            || cache.active_len != 1
            || cache.point_limit != motion::PATHFINDER_CACHE_POINT_LIMIT
            || cache.path_type != 1
            || cache.requested_end != previous.request.goal
            || cache.include_flags != native.filter().include_flags
            || cache.exclude_flags != native.filter().exclude_flags
            || &dependency.owner_ignore
                != route
                    .require_owner_ignore_pathfinding(&self.profile)
                    .map_err(|_| Failure::Binding)?
            || dependency.owner_ignore.ignore_pathfinding()
            || !dependency.tiles.both_present()
            || !native.is_retained()
            || native.filter() != cold_native.filter()
            || native.initial_storage() != cold_native.initial_storage()
            || native.initial_active_len() != cold_native.initial_active_len()
            || terminal.slot() != cold_terminal.slot()
            || terminal.query_capacity() != cold_terminal.query_capacity()
            || terminal.query_capacity() != 1024
            || terminal.config() != cold_terminal.config()
            || terminal.points() != [bits(route.request.origin), bits(route.request.goal)]
            || terminal.actual_end() != bits(route.request.goal)
            || terminal.path_type() != 1
            || native.active_len() != 1
            || native.point_limit() != usize::from(motion::PATHFINDER_CACHE_POINT_LIMIT)
            || !(75..=motion::MAX_PATHFINDER_CACHE_REFS).contains(&native.storage().len())
        {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn second_retained_segment_los(
        &self,
        dependency: &ResidentJoinedSecondRetainedDependency,
        origin: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
        ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<RayReply, ResidentFilterError> {
        let deadline = dependency.deadline(deadline);
        let observed =
            self.second_retained_for(dependency, origin, context, deadline, &cancelled)?;
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
        self.second_retained_for(dependency, origin, context, deadline, cancelled)?;
        Ok(ray)
    }
}

fn second_retained_revision(context: &AuthorityRequest) -> Result<u64, ResidentFilterError> {
    let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
        return Err(Failure::Binding.into());
    };
    let cache = route.request.prior_cache.as_ref().ok_or(Failure::Binding)?;
    if cache.revision != 2 {
        return Err(Failure::Binding.into());
    }
    cache
        .revision
        .checked_add(1)
        .ok_or_else(|| Failure::Invalid.into())
}
fn second_retained_budget(
    terminal: &LoadedSourceSamePolygon,
) -> Result<SourceSamePolygonBudget, ResidentFilterError> {
    let native = terminal.compatibility();
    if native.active_len() != 1
        || native.point_limit() != usize::from(motion::PATHFINDER_CACHE_POINT_LIMIT)
    {
        return Err(Failure::UnsupportedQuery.into());
    }
    Ok(SourceSamePolygonBudget {
        lookup_primitives: second_retained_lookup_budget(native.storage().len())?,
        terminal_primitives: 1,
    })
}
fn second_retained_lookup_budget(length: usize) -> Result<usize, ResidentFilterError> {
    let limit = usize::from(motion::PATHFINDER_CACHE_POINT_LIMIT);
    if !(limit + 1..motion::MAX_PATHFINDER_CACHE_REFS).contains(&length) {
        return Err(Failure::UnsupportedQuery.into());
    }
    length
        .checked_sub(limit)
        .and_then(|tail| tail.checked_mul(2))
        .and_then(|work| work.checked_add(8))
        .filter(|work| *work <= 8050)
        .ok_or_else(|| Failure::UnsupportedQuery.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn second_retained_budget_tracks_actual_backing_and_closes_outside_domain() {
        assert_eq!(second_retained_lookup_budget(75).unwrap(), 10);
        assert_eq!(second_retained_lookup_budget(76).unwrap(), 12);
        assert_eq!(second_retained_lookup_budget(4095).unwrap(), 8050);
        for length in [0, 1, 73, 74, 4096, usize::MAX] {
            assert!(matches!(
                second_retained_lookup_budget(length),
                Err(ResidentFilterError::Context(Failure::UnsupportedQuery))
            ));
        }
    }
}
