// SPDX-License-Identifier: GPL-3.0-or-later
//! Component-only finite Chase loop from the actual ACK-selected native owner.
//! Each successor owns fresh evidence and bounded metadata, never a previous warm owner.
use super::*;

pub(crate) const MAX_SOURCE_REVISION: u64 = 8;

/// Borrowed authority projected only from an existing sealed native owner.
/// Neither a diagnostic cache nor raw terminal parts can construct this view.
pub(crate) struct ResidentCurrentNative<'a> {
    owner: CurrentNativeOwner<'a>,
}
enum CurrentNativeOwner<'a> {
    First(&'a ResidentJoinedRetainedDependency),
    Repeated(&'a ResidentJoinedRepeatedRetainedDependency),
}
impl ResidentJoinedRetainedDependency {
    pub(crate) fn current_native(&self) -> ResidentCurrentNative<'_> {
        ResidentCurrentNative {
            owner: CurrentNativeOwner::First(self),
        }
    }
}
impl ResidentCurrentNative<'_> {
    pub(crate) fn source_revision(&self) -> u64 {
        match &self.owner {
            CurrentNativeOwner::First(_) => 2,
            CurrentNativeOwner::Repeated(owner) => owner.source_revision,
        }
    }
    fn root(&self) -> &ResidentRetainedRoot {
        match &self.owner {
            CurrentNativeOwner::First(owner) => &owner.root,
            CurrentNativeOwner::Repeated(owner) => &owner.root,
        }
    }
    fn origin_identity(&self) -> &Rc<()> {
        match &self.owner {
            CurrentNativeOwner::First(owner) => &owner.root.predecessor,
            CurrentNativeOwner::Repeated(owner) => &owner.origin,
        }
    }
    fn chase(&self) -> &ResidentChaseWaterDependency {
        match &self.owner {
            CurrentNativeOwner::First(owner) => &owner.chase,
            CurrentNativeOwner::Repeated(owner) => &owner.chase,
        }
    }
    fn terminal(&self) -> &LoadedSourceSamePolygon {
        match &self.owner {
            CurrentNativeOwner::First(owner) => &owner.terminal,
            CurrentNativeOwner::Repeated(owner) => &owner.terminal,
        }
    }
    fn filter_ticket(&self) -> u64 {
        match &self.owner {
            CurrentNativeOwner::First(owner) => owner.observations[1].request().ticket,
            CurrentNativeOwner::Repeated(owner) => owner.observations[1].request().ticket,
        }
    }
}

/// Own one real native successor and bounded ancestry metadata, not prior dependencies.
pub(crate) struct ResidentJoinedRepeatedRetainedDependency {
    root: ResidentRetainedRoot,
    origin: Rc<()>,
    previous_context: AuthorityRequest,
    previous_filter_ticket: u64,
    source_revision: u64,
    chase: ResidentChaseWaterDependency,
    observations: [PointNoWaterReply; 2],
    owner_ignore: OwnerIgnorePathfindingReceipt,
    tiles: LoadedSourceTilePresence,
    terminal: LoadedSourceSamePolygon,
}
pub(crate) type ResidentJoinedRepeatedRetainedObservation = ResidentJoinedRetainedObservation;

impl ResidentJoinedRepeatedRetainedDependency {
    pub(crate) fn current_native(&self) -> ResidentCurrentNative<'_> {
        ResidentCurrentNative {
            owner: CurrentNativeOwner::Repeated(self),
        }
    }
    pub(crate) fn source_revision(&self) -> u64 {
        self.source_revision
    }
    /// Source-free ACK check against the actual retained owner, not a copied cache.
    pub(crate) fn check_historical_native_predecessor_for_test(
        &self,
        prior: &ResidentCurrentNative<'_>,
    ) -> Result<(), ResidentFilterError> {
        let native = self.terminal.compatibility();
        let prior_native = prior.terminal().compatibility();
        if !Rc::ptr_eq(&self.root.provider, &prior.root().provider)
            || !Rc::ptr_eq(&self.origin, prior.origin_identity())
            || !Rc::ptr_eq(&self.root.predecessor, &prior.root().identity)
            || self.previous_context != prior.root().context
            || !same_historical_dispatch(&self.previous_context, &prior.root().context)
            || self.previous_filter_ticket != prior.filter_ticket()
            || repeated_revision(prior.source_revision(), &self.root.context)?
                != self.source_revision
            || !prior.chase().same_water_model(&self.chase)
            || !native.is_retained()
            || native.before_storage() != prior_native.storage()
            || native.before_active_len() != prior_native.active_len()
            || native.filter() != prior_native.filter()
            || native.initial_storage() != prior_native.initial_storage()
            || native.initial_active_len() != prior_native.initial_active_len()
            || self.terminal.slot() != prior.terminal().slot()
            || self.terminal.query_capacity() != prior.terminal().query_capacity()
            || self.terminal.config() != prior.terminal().config()
        {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }
    pub(crate) fn deadline(&self, deadline: Instant) -> Instant {
        deadline.min(self.root.expires)
    }
    /// Historical equality only; a copied cache cannot create native ownership.
    pub(crate) fn check_historical_native_cache_for_test(
        &self,
        expected: &motion::PathFinderCache,
    ) -> Result<(), ResidentFilterError> {
        check_historical_native_cache(
            &self.terminal,
            &self.root.context,
            self.source_revision,
            expected,
        )
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
    /// Historical provenance only; source freshness is established for the new request.
    fn current_native_cache(
        &self,
        origin: &ResidentJoinedColdDependency,
        prior: &ResidentCurrentNative<'_>,
    ) -> Result<motion::PathFinderCache, ResidentFilterError> {
        let cache = self.retained_native_cache(
            prior.terminal(),
            &prior.root().context,
            prior.source_revision(),
        )?;
        match &prior.owner {
            CurrentNativeOwner::First(owner) => self.check_historical_retained_cache_for_test(
                owner,
                origin,
                &prior.root().context,
                &cache,
            )?,
            CurrentNativeOwner::Repeated(owner) => self.check_historical_repeated_cache_for_test(
                owner,
                origin,
                &prior.root().context,
                &cache,
            )?,
        }
        Ok(cache)
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn observe_joined_repeated(
        &mut self,
        origin: &ResidentJoinedColdDependency,
        prior: &ResidentCurrentNative<'_>,
        context: &AuthorityRequest,
        player: PlayerId,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<ResidentJoinedRepeatedRetainedDependency, ResidentFilterError> {
        self.observe_joined_repeated_observed(
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
    pub(crate) fn observe_joined_repeated_observed(
        &mut self,
        origin: &ResidentJoinedColdDependency,
        prior: &ResidentCurrentNative<'_>,
        context: &AuthorityRequest,
        player: PlayerId,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        mut observe: impl FnMut(ResidentRetainedPhase),
    ) -> Result<ResidentJoinedRepeatedRetainedDependency, ResidentFilterError> {
        self.check_repeated_predecessor(origin, prior, context, deadline, &cancelled)?;
        let source_revision = repeated_revision(prior.source_revision(), context)?;
        let previous_filter_ticket = prior.filter_ticket();
        if first_ticket == 0
            || first_ticket.checked_add(4).is_none()
            || first_ticket <= previous_filter_ticket
        {
            return Err(Failure::Binding.into());
        }
        let budget = repeated_budget(prior.terminal())?;
        let root = ResidentRetainedRoot {
            identity: Rc::new(()),
            provider: Rc::clone(&self.identity),
            predecessor: Rc::clone(&prior.root().identity),
            context: context.clone(),
            expires: deadline,
        };
        origin
            .retained_chase()
            .require_retained_model(self, context, player)?;
        prior
            .chase()
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
        if !origin.retained_chase().same_retained_model(&prefix)
            || !prior.chase().same_retained_model(&prefix)
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
        self.check_repeated_predecessor(origin, prior, context, deadline, &cancelled)?;
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
        let retained_filter = prior.terminal().compatibility().filter();
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
        if filter != prior.terminal().compatibility().filter() {
            return Err(Failure::Binding.into());
        }
        // Only the actual ACK-selected current native terminal supplies calculation input.
        let terminal = selected
            .loaded_retained_same_polygon(
                prior.terminal(),
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
        let result = ResidentJoinedRepeatedRetainedDependency {
            root,
            origin: Rc::clone(
                &origin
                    .retained_cold()
                    .historical_observation()
                    .observation_identity,
            ),
            previous_context: prior.root().context.clone(),
            previous_filter_ticket,
            source_revision,
            chase,
            observations,
            owner_ignore,
            tiles,
            terminal,
        };
        self.check_historical_repeated_predecessor_for_test(&result, origin, prior, context)?;
        self.repeated_for(&result, origin, context, deadline, cancelled)?;
        Ok(result)
    }

    fn check_repeated_predecessor(
        &self,
        origin: &ResidentJoinedColdDependency,
        prior: &ResidentCurrentNative<'_>,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<(), ResidentFilterError> {
        self.require_finite_retained_routes()?;
        self.check_context(context, deadline, &cancelled)?;
        repeated_budget(prior.terminal())?;
        let cache = self.current_native_cache(origin, prior)?;
        // Historical provenance deliberately does not reuse prior/cold deadlines.
        repeated_revision(prior.source_revision(), context)?;
        self.check_repeated_contexts(origin, &prior.root().context, context)?;
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

    fn check_repeated_contexts(
        &self,
        origin: &ResidentJoinedColdDependency,
        previous: &AuthorityRequest,
        context: &AuthorityRequest,
    ) -> Result<(), ResidentFilterError> {
        self.require_finite_retained_routes()?;
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
        Ok(())
    }

    /// Fresh source checks need no previous warm dependency after publication.
    pub(crate) fn repeated_for(
        &self,
        dependency: &ResidentJoinedRepeatedRetainedDependency,
        origin: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<ResidentJoinedRepeatedRetainedObservation, ResidentFilterError> {
        let deadline = dependency
            .root
            .check_current(self, context, deadline, &cancelled)?;
        self.check_repeated_native(dependency, origin, context)?;
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
        Ok(ResidentJoinedRepeatedRetainedObservation {
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
    pub(crate) fn repeated_cache(
        &self,
        dependency: &ResidentJoinedRepeatedRetainedDependency,
        origin: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<motion::PathFinderCache, ResidentFilterError> {
        self.repeated_for(dependency, origin, context, deadline, &cancelled)?;
        let revision = dependency.source_revision;
        let cache = self.retained_native_cache(&dependency.terminal, context, revision)?;
        self.repeated_for(dependency, origin, context, deadline, cancelled)?;
        Ok(cache)
    }
    /// Compare the actual historical predecessor, not an equal diagnostic copy.
    pub(crate) fn check_historical_repeated_predecessor_for_test(
        &self,
        dependency: &ResidentJoinedRepeatedRetainedDependency,
        origin: &ResidentJoinedColdDependency,
        prior: &ResidentCurrentNative<'_>,
        context: &AuthorityRequest,
    ) -> Result<(), ResidentFilterError> {
        self.check_repeated_native(dependency, origin, context)?;
        let cache = self.current_native_cache(origin, prior)?;
        dependency.check_historical_native_predecessor_for_test(prior)?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::Binding.into());
        };
        if route.request.prior_cache.as_deref() != Some(&cache) {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }
    /// Whole native/current-cache diagnostic stays valid after dropping the previous warm owner.
    pub(crate) fn check_historical_repeated_cache_for_test(
        &self,
        dependency: &ResidentJoinedRepeatedRetainedDependency,
        origin: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
        expected: &motion::PathFinderCache,
    ) -> Result<(), ResidentFilterError> {
        self.check_repeated_native(dependency, origin, context)?;
        dependency.check_historical_native_cache_for_test(expected)?;
        if self.retained_native_cache(&dependency.terminal, context, dependency.source_revision)?
            != *expected
        {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }

    fn check_repeated_native(
        &self,
        dependency: &ResidentJoinedRepeatedRetainedDependency,
        origin: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
    ) -> Result<(), ResidentFilterError> {
        self.check_repeated_contexts(origin, &dependency.previous_context, context)?;
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
        let previous_cache = previous
            .request
            .prior_cache
            .as_ref()
            .ok_or(Failure::Binding)?;
        let terminal = &dependency.terminal;
        let native = terminal.compatibility();
        let cold_terminal = origin.retained_cold().native_terminal();
        let cold_native = cold_terminal.compatibility();
        let bits = |point: Point| [point.x.bits(), point.y.bits(), point.z.bits()];
        second_retained::second_retained_lookup_budget(native.before_storage().len())?;
        let prior_revision = cache.revision;
        if repeated_revision(prior_revision, context)? != dependency.source_revision {
            return Err(Failure::Binding.into());
        }
        previous_cache
            .validate_for(previous.request.binding.identity)
            .map_err(|_| Failure::Binding)?;
        if cache.binding != previous_cache.binding
            || previous_cache.revision.checked_add(1) != Some(cache.revision)
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
    pub(crate) fn repeated_segment_los(
        &self,
        dependency: &ResidentJoinedRepeatedRetainedDependency,
        origin: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
        ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<RayReply, ResidentFilterError> {
        let deadline = dependency.deadline(deadline);
        let observed = self.repeated_for(dependency, origin, context, deadline, &cancelled)?;
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
        self.repeated_for(dependency, origin, context, deadline, cancelled)?;
        Ok(ray)
    }
}

fn repeated_revision(
    previous_revision: u64,
    context: &AuthorityRequest,
) -> Result<u64, ResidentFilterError> {
    let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
        return Err(Failure::Binding.into());
    };
    let cache = route.request.prior_cache.as_ref().ok_or(Failure::Binding)?;
    if cache.revision != previous_revision {
        return Err(Failure::Binding.into());
    }
    next_source_revision(previous_revision)
}
fn next_source_revision(previous_revision: u64) -> Result<u64, ResidentFilterError> {
    previous_revision
        .checked_add(1)
        .filter(|revision| (3..=MAX_SOURCE_REVISION).contains(revision))
        .ok_or_else(|| Failure::Binding.into())
}
fn repeated_budget(
    terminal: &LoadedSourceSamePolygon,
) -> Result<SourceSamePolygonBudget, ResidentFilterError> {
    let native = terminal.compatibility();
    if native.active_len() != 1
        || native.point_limit() != usize::from(motion::PATHFINDER_CACHE_POINT_LIMIT)
    {
        return Err(Failure::UnsupportedQuery.into());
    }
    Ok(SourceSamePolygonBudget {
        lookup_primitives: second_retained::second_retained_lookup_budget(native.storage().len())?,
        terminal_primitives: 1,
    })
}

fn same_historical_dispatch(a: &AuthorityRequest, b: &AuthorityRequest) -> bool {
    let Query::SourceMotion(MotionQuery::Route(a)) = &a.query else {
        return false;
    };
    let Query::SourceMotion(MotionQuery::Route(b)) = &b.query else {
        return false;
    };
    a.pathfinder_dispatch
        .as_ref()
        .zip(b.pathfinder_dispatch.as_ref())
        .is_some_and(|(a, b)| a.same_live_dispatch(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_source_revision_is_finite_checked_and_never_authorizes_a_ninth() {
        for revision in 2..MAX_SOURCE_REVISION {
            assert_eq!(next_source_revision(revision).unwrap(), revision + 1);
        }
        for revision in [0, 1, MAX_SOURCE_REVISION, u64::MAX] {
            assert!(matches!(
                next_source_revision(revision),
                Err(ResidentFilterError::Context(Failure::Binding))
            ));
        }
    }
}
