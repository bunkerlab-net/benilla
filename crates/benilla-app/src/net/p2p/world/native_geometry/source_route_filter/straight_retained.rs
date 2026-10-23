// SPDX-License-Identifier: GPL-3.0-or-later
//! Component-only finite straight Chase continuation from the actual ACK-selected owner.
//! Pinned TargetedMovementGenerator.cpp:502-504 selects calculate(false, true) only
//! after the ordered distance/Z/LOS/dry gates. No fallback or normalization is admitted.
use super::super::super::super::products::vmap::QueryBudget;
use super::*;
use benilla_native_nav::{
    LoadedSourceRetainedStraight, SourceOwnerFilter, SourceRetainedStraightConfig,
    SourceStraightHeight, SourceStraightPathBudget, SourceWarmTrace,
};

#[path = "straight_retained/detail_provenance.rs"]
mod detail_provenance;
#[path = "straight_retained/generated_flat_support.rs"]
pub(crate) mod generated_flat_support;

pub(crate) const MAX_SOURCE_REVISION: u64 = 8;
/// Operational refusal, not a change to the original sampling algorithm or point limit 74.
pub(crate) const STRAIGHT_MAX_POINTS: usize = 32;
const LOOKUP_LIMIT: usize = 8050;
const STORAGE_LIMIT: usize = 4096;
const POST_LOOKUPS: usize = STRAIGHT_MAX_POINTS - 1; // thirty samples plus one clipped hit.
const CONFIG: SourceRetainedStraightConfig = SourceRetainedStraightConfig {
    normalization: SourceSamePolygonConfig {
        normalize_z: false,
        ignore_normalization: false,
    },
    optimize_path: false,
};

/// Borrowed sealed authority; no serialized cache or public parts can construct this view.
pub(crate) struct ResidentCurrentStraightNative<'a> {
    owner: CurrentStraightOwner<'a>,
}
enum CurrentStraightOwner<'a> {
    Cold(&'a ResidentJoinedColdDependency),
    Straight(&'a ResidentJoinedStraightDependency),
}
impl ResidentJoinedColdDependency {
    pub(crate) fn current_straight_native(&self) -> ResidentCurrentStraightNative<'_> {
        ResidentCurrentStraightNative {
            owner: CurrentStraightOwner::Cold(self),
        }
    }
}
impl ResidentCurrentStraightNative<'_> {
    pub(crate) fn source_revision(&self) -> u64 {
        match self.owner {
            CurrentStraightOwner::Cold(_) => 1,
            CurrentStraightOwner::Straight(owner) => owner.source_revision,
        }
    }
    pub(crate) fn budget(&self) -> Result<SourceStraightPathBudget, ResidentFilterError> {
        straight_budget(self.active_len(), self.storage().len())
    }
    fn context(&self) -> &AuthorityRequest {
        match self.owner {
            CurrentStraightOwner::Cold(owner) => {
                &owner.retained_cold().historical_observation().context
            }
            CurrentStraightOwner::Straight(owner) => &owner.root.context,
        }
    }
    fn identity(&self) -> &Rc<()> {
        match self.owner {
            CurrentStraightOwner::Cold(owner) => {
                &owner
                    .retained_cold()
                    .historical_observation()
                    .observation_identity
            }
            CurrentStraightOwner::Straight(owner) => &owner.root.identity,
        }
    }
    fn provider(&self) -> &Rc<()> {
        match self.owner {
            CurrentStraightOwner::Cold(owner) => {
                &owner.retained_cold().historical_observation().identity
            }
            CurrentStraightOwner::Straight(owner) => &owner.root.provider,
        }
    }
    fn origin_identity(&self) -> &Rc<()> {
        match self.owner {
            CurrentStraightOwner::Cold(_) => self.identity(),
            CurrentStraightOwner::Straight(owner) => &owner.origin,
        }
    }
    fn chase(&self) -> &ResidentChaseWaterDependency {
        match self.owner {
            CurrentStraightOwner::Cold(owner) => owner.retained_chase(),
            CurrentStraightOwner::Straight(owner) => &owner.chase,
        }
    }
    fn filter_ticket(&self) -> u64 {
        match self.owner {
            CurrentStraightOwner::Cold(owner) => {
                owner.retained_cold().historical_observation().observations[1]
                    .request()
                    .ticket
            }
            CurrentStraightOwner::Straight(owner) => owner.observations[1].request().ticket,
        }
    }
    fn storage(&self) -> &[u64] {
        match self.owner {
            CurrentStraightOwner::Cold(owner) => owner
                .retained_cold()
                .native_terminal()
                .compatibility()
                .storage(),
            CurrentStraightOwner::Straight(owner) => owner.terminal.storage(),
        }
    }
    fn active_len(&self) -> usize {
        match self.owner {
            CurrentStraightOwner::Cold(owner) => owner
                .retained_cold()
                .native_terminal()
                .compatibility()
                .active_len(),
            CurrentStraightOwner::Straight(owner) => owner.terminal.active_len(),
        }
    }
    fn filter(&self) -> SourceOwnerFilter {
        match self.owner {
            CurrentStraightOwner::Cold(owner) => owner
                .retained_cold()
                .native_terminal()
                .compatibility()
                .filter(),
            CurrentStraightOwner::Straight(owner) => owner.terminal.filter(),
        }
    }
    fn path_type(&self) -> u32 {
        match self.owner {
            CurrentStraightOwner::Cold(owner) => {
                owner.retained_cold().native_terminal().path_type()
            }
            CurrentStraightOwner::Straight(owner) => owner.terminal.path_type(),
        }
    }
}

/// One current terminal plus bounded tokens/copies, never a previous warm object chain.
pub(crate) struct ResidentJoinedStraightDependency {
    root: ResidentRetainedRoot,
    origin: Rc<()>,
    previous_context: AuthorityRequest,
    previous_filter_ticket: u64,
    source_revision: u64,
    chase: ResidentChaseWaterDependency,
    observations: [PointNoWaterReply; 2],
    owner_ignore: OwnerIgnorePathfindingReceipt,
    tiles: LoadedSourceTilePresence,
    terminal: LoadedSourceRetainedStraight,
}
pub(crate) struct ResidentJoinedStraightObservation<'a> {
    pub(crate) points: &'a [[u32; 3]],
    pub(crate) trace: &'a [SourceWarmTrace],
    pub(crate) body: Body,
}
impl ResidentJoinedStraightDependency {
    pub(crate) fn current_straight_native(&self) -> ResidentCurrentStraightNative<'_> {
        ResidentCurrentStraightNative {
            owner: CurrentStraightOwner::Straight(self),
        }
    }
    pub(crate) fn source_revision(&self) -> u64 {
        self.source_revision
    }
    pub(crate) fn deadline(&self, deadline: Instant) -> Instant {
        deadline.min(self.root.expires)
    }
    pub(crate) fn check_historical_native_predecessor_for_test(
        &self,
        prior: &ResidentCurrentStraightNative<'_>,
    ) -> Result<(), ResidentFilterError> {
        if !Rc::ptr_eq(&self.root.provider, prior.provider())
            || !Rc::ptr_eq(&self.origin, prior.origin_identity())
            || !Rc::ptr_eq(&self.root.predecessor, prior.identity())
            || self.previous_context != *prior.context()
            || !same_dispatch(&self.previous_context, prior.context())
            || self.previous_filter_ticket != prior.filter_ticket()
            || next_revision(prior.source_revision())? != self.source_revision
            || !prior.chase().same_water_model(&self.chase)
            || self.terminal.before_storage() != prior.storage()
            || self.terminal.before_active_len() != prior.active_len()
            || self.terminal.filter() != prior.filter()
            || self.terminal.previous_path_type() != prior.path_type()
        {
            return Err(Failure::Binding.into());
        }
        match prior.owner {
            CurrentStraightOwner::Cold(owner) => self
                .terminal
                .check_same_polygon_predecessor(owner.retained_cold().native_terminal()),
            CurrentStraightOwner::Straight(owner) => {
                self.terminal.check_predecessor(&owner.terminal)
            }
        }
        .map_err(native_error)
    }
    pub(crate) fn check_historical_native_cache_for_test(
        &self,
        expected: &motion::PathFinderCache,
    ) -> Result<(), ResidentFilterError> {
        let Query::SourceMotion(MotionQuery::Route(route)) = &self.root.context.query else {
            return Err(Failure::Binding.into());
        };
        let prior = route.request.prior_cache.as_ref().ok_or(Failure::Binding)?;
        expected
            .validate_for(route.request.binding.identity)
            .map_err(|_| Failure::Binding)?;
        let native = &self.terminal;
        if expected.binding != prior.binding
            || expected.revision != self.source_revision
            || prior.revision.checked_add(1) != Some(self.source_revision)
            || expected.source != motion::RouteSource::OwnerPathFinder
            || expected.poly_refs != native.storage()
            || usize::from(expected.active_len) != native.active_len()
            || usize::from(expected.point_limit) != native.point_limit()
            || expected.path_type != native.path_type()
            || bits(expected.requested_end) != native.requested_end()
            || expected.include_flags != native.filter().include_flags
            || expected.exclude_flags != native.filter().exclude_flags
            || native.before_storage() != prior.poly_refs
            || native.before_active_len() != usize::from(prior.active_len)
        {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }
    pub(crate) fn check_historical_native_points_for_test(
        &self,
        points: &[Point],
    ) -> Result<(), ResidentFilterError> {
        if points.len() != self.terminal.points().len()
            || points
                .iter()
                .copied()
                .map(bits)
                .ne(self.terminal.points().iter().copied())
        {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }
    /// Logical owned buffers only. Shared source/query allocations and allocator overhead excluded.
    pub(crate) fn storage_footprint_for_test(&self) -> usize {
        let n = &self.terminal;
        std::mem::size_of_val(n.initial_storage())
            + std::mem::size_of_val(n.before_storage())
            + std::mem::size_of_val(n.storage())
            + std::mem::size_of_val(n.trace())
            + std::mem::size_of_val(n.points())
            + n.ray()
                .map_or(0, |ray| std::mem::size_of_val(ray.polygons.as_slice()))
            + std::mem::size_of_val(n.diagnostics().heights.as_slice())
            + 2 * context_cache_bytes(&self.root.context)
            + context_cache_bytes(&self.previous_context)
    }
}
pub(crate) fn straight_storage_upper_bound_for_test() -> usize {
    6 * STORAGE_LIMIT * std::mem::size_of::<u64>()
        + LOOKUP_LIMIT * std::mem::size_of::<SourceWarmTrace>()
        + 74 * std::mem::size_of::<u64>()
        + POST_LOOKUPS * std::mem::size_of::<SourceStraightHeight>()
        + STRAIGHT_MAX_POINTS * std::mem::size_of::<[u32; 3]>()
}

impl ResidentFilterProvider<'_, '_, '_, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn observe_joined_straight(
        &mut self,
        origin: &ResidentJoinedColdDependency,
        prior: &ResidentCurrentStraightNative<'_>,
        context: &AuthorityRequest,
        player: PlayerId,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<ResidentJoinedStraightDependency, ResidentFilterError> {
        self.observe_joined_straight_observed(
            origin,
            prior,
            context,
            player,
            first_ticket,
            prior.budget()?,
            deadline,
            cancelled,
            |_| {},
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn observe_joined_straight_observed(
        &mut self,
        origin: &ResidentJoinedColdDependency,
        prior: &ResidentCurrentStraightNative<'_>,
        context: &AuthorityRequest,
        player: PlayerId,
        first_ticket: u64,
        budget: SourceStraightPathBudget,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        mut observe: impl FnMut(ResidentRetainedPhase),
    ) -> Result<ResidentJoinedStraightDependency, ResidentFilterError> {
        self.check_straight_predecessor(origin, prior, context, deadline, &cancelled)?;
        check_budget(budget, prior.budget()?)?;
        let source_revision = next_revision(prior.source_revision())?;
        let previous_filter_ticket = prior.filter_ticket();
        if first_ticket == 0
            || first_ticket
                .checked_add(STRAIGHT_MAX_POINTS as u64 + 2)
                .is_none()
            || first_ticket <= previous_filter_ticket
        {
            return Err(Failure::Binding.into());
        }
        let root = ResidentRetainedRoot {
            identity: Rc::new(()),
            provider: Rc::clone(&self.identity),
            predecessor: Rc::clone(prior.identity()),
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
        // This separate component policy selects the original calculate(false, true)
        // call only after its genuine clear/dry Chase prefix. The live constructor
        // receipt does not independently claim straight/force flags.
        self.check_straight_predecessor(origin, prior, context, deadline, &cancelled)?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::UnsupportedQuery.into());
        };
        let width = route
            .require_collision_width(&self.profile)
            .map_err(|_| Failure::Binding)?
            .width()
            .bits();
        let state = &mut *self.state;
        let mut constructor = state
            .query
            .retain_constructor(context)
            .map_err(ResidentFilterError::Query)?;
        let selected = constructor
            .select_for_calculate(deadline)
            .map_err(ResidentFilterError::Query)?;
        observe(ResidentRetainedPhase::Selected);
        check_time(deadline, &cancelled)?;
        let owner_ignore = route
            .require_owner_ignore_pathfinding(&state.profile)
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
        let (observations, filter) = point_observations(
            self.height,
            context,
            first_ticket + 2,
            deadline,
            &cancelled,
            Some(prior.filter()),
            &mut |phase| {
                observe(if phase == 0 {
                    ResidentRetainedPhase::FilterAllLiquids
                } else {
                    ResidentRetainedPhase::FilterWaterOrOcean
                })
            },
        )?;
        check_time(deadline, &cancelled)?;
        if filter != prior.filter() {
            return Err(Failure::Binding.into());
        }
        let terminal = match prior.owner {
            CurrentStraightOwner::Cold(owner) => selected
                .loaded_retained_straight_from_same_polygon(
                    owner.retained_cold().native_terminal(),
                    CONFIG,
                    width,
                    budget,
                    deadline,
                ),
            CurrentStraightOwner::Straight(owner) => {
                selected.loaded_retained_straight(&owner.terminal, CONFIG, width, budget, deadline)
            }
        }
        .map_err(ResidentFilterError::StraightTerminal)?;
        terminal.check_query(&selected).map_err(native_error)?;
        observe(ResidentRetainedPhase::Terminal);
        check_time(deadline, &cancelled)?;
        require_supported_terminal(&terminal)?;
        let result = ResidentJoinedStraightDependency {
            root,
            origin: Rc::clone(prior.origin_identity()),
            previous_context: prior.context().clone(),
            previous_filter_ticket,
            source_revision,
            chase,
            observations,
            owner_ignore,
            tiles,
            terminal,
        };
        self.check_historical_straight_predecessor_for_test(&result, origin, prior, context)?;
        self.straight_for(&result, origin, context, deadline, cancelled)?;
        Ok(result)
    }
    fn check_straight_predecessor(
        &self,
        origin: &ResidentJoinedColdDependency,
        prior: &ResidentCurrentStraightNative<'_>,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<(), ResidentFilterError> {
        self.require_finite_straight_routes()?;
        self.check_context(context, deadline, &cancelled)?;
        prior.budget()?;
        next_revision(prior.source_revision())?;
        let cache = self.current_straight_cache(origin, prior)?;
        self.check_straight_contexts(origin, prior.context(), prior.source_revision(), context)?;
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
    fn check_straight_cold(
        &self,
        origin: &ResidentJoinedColdDependency,
    ) -> Result<(), ResidentFilterError> {
        self.require_finite_straight_routes()?;
        let cold = origin.retained_cold().historical_observation();
        if !Rc::ptr_eq(&self.identity, &cold.identity) {
            return Err(Failure::Binding.into());
        }
        validate_route_context(&self.session, &self.profile, &cold.context)?;
        let dispatch = cold
            .context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &cold.context.query else {
            return Err(Failure::Binding.into());
        };
        route
            .require_collision_width(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let terminal = origin.retained_cold().native_terminal();
        let native = terminal.compatibility();
        if !dispatch.first_construction()
            || dispatch.constructor_kind() != motion::PathfinderConstructorKind::ChaseLazy
            || route.request.prior_cache.is_some()
            || !origin.retained_cold().historical_calculate_ready()
            || native.is_retained()
            || native.storage().len() != 75
            || native.point_limit() != 74
            || native.before_active_len() != 0
            || native.before_storage() != [0; 74]
            || native.initial_active_len() != 0
            || native.initial_storage() != [0; 74]
            || native.filter() != cold.filter
            || native.active_len() != 1
            || terminal.query_capacity() != 1024
            || terminal.path_type() != 1
            || terminal.points() != [bits(route.request.origin), bits(route.request.goal)]
            || terminal.actual_end() != bits(route.request.goal)
            || terminal.config() != CONFIG.normalization
        {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }
    fn check_straight_contexts(
        &self,
        origin: &ResidentJoinedColdDependency,
        previous: &AuthorityRequest,
        prior_revision: u64,
        context: &AuthorityRequest,
    ) -> Result<(), ResidentFilterError> {
        self.check_straight_cold(origin)?;
        next_revision(prior_revision)?;
        let cold = origin.retained_cold().historical_observation();
        for request in [previous, context] {
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
            return Err(Failure::Binding.into());
        };
        for candidate in [prior_route, route] {
            if candidate.owner != old_route.owner
                || candidate
                    .require_collision_width(&self.profile)
                    .map_err(|_| Failure::Binding)?
                    != old_route
                        .require_collision_width(&self.profile)
                        .map_err(|_| Failure::Binding)?
                || candidate.collision_height != old_route.collision_height
            {
                return Err(Failure::Binding.into());
            }
        }
        if current.first_construction()
            || prior.first_construction() != (prior_revision == 1)
            || prior.constructor_kind() != old.constructor_kind()
            || current.constructor_kind() != old.constructor_kind()
            || prior.construction_binding() != old.construction_binding()
            || current.construction_binding() != old.construction_binding()
            || prior.configuration_binding() != old.configuration_binding()
            || current.configuration_binding() != old.configuration_binding()
            || prior.npc() != old.npc()
            || current.npc() != old.npc()
            || context.binding.tick <= previous.binding.tick
            || route.request.binding.query_id <= prior_route.request.binding.query_id
            || route.request.binding.now_ms <= prior_route.request.binding.now_ms
            || route.request.kind != motion::RouteKind::ChaseReplan
            || route.request.point_limit != motion::PATHFINDER_CACHE_POINT_LIMIT
            || prior_route.request.point_limit != motion::PATHFINDER_CACHE_POINT_LIMIT
            || (prior_revision == 1
                && (previous != &cold.context || !same_dispatch(previous, &cold.context)))
            || (prior_revision != 1
                && (previous.binding.tick <= cold.context.binding.tick
                    || prior_route.request.binding.query_id <= old_route.request.binding.query_id
                    || prior_route.request.binding.now_ms <= old_route.request.binding.now_ms
                    || prior_route.request.kind != motion::RouteKind::ChaseReplan))
        {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }
    fn current_straight_cache(
        &self,
        origin: &ResidentJoinedColdDependency,
        prior: &ResidentCurrentStraightNative<'_>,
    ) -> Result<motion::PathFinderCache, ResidentFilterError> {
        self.check_straight_cold(origin)?;
        if !Rc::ptr_eq(prior.provider(), &self.identity)
            || !Rc::ptr_eq(
                prior.origin_identity(),
                &origin
                    .retained_cold()
                    .historical_observation()
                    .observation_identity,
            )
        {
            return Err(Failure::Binding.into());
        }
        match prior.owner {
            CurrentStraightOwner::Cold(owner) => {
                if !std::ptr::eq(owner, origin) {
                    return Err(Failure::Binding.into());
                }
                self.retained_native_cache(
                    owner.retained_cold().native_terminal(),
                    prior.context(),
                    1,
                )
            }
            CurrentStraightOwner::Straight(owner) => {
                self.check_straight_native(owner, origin, prior.context())?;
                self.straight_native_cache(owner, prior.context())
            }
        }
    }
    pub(crate) fn check_historical_straight_current_for_test(
        &self,
        origin: &ResidentJoinedColdDependency,
        prior: &ResidentCurrentStraightNative<'_>,
        expected: &motion::PathFinderCache,
    ) -> Result<(), ResidentFilterError> {
        if self.current_straight_cache(origin, prior)? != *expected {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }
    pub(crate) fn check_historical_straight_predecessor_for_test(
        &self,
        dependency: &ResidentJoinedStraightDependency,
        origin: &ResidentJoinedColdDependency,
        prior: &ResidentCurrentStraightNative<'_>,
        context: &AuthorityRequest,
    ) -> Result<(), ResidentFilterError> {
        self.check_straight_native(dependency, origin, context)?;
        let cache = self.current_straight_cache(origin, prior)?;
        dependency.check_historical_native_predecessor_for_test(prior)?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::Binding.into());
        };
        if route.request.prior_cache.as_deref() != Some(&cache) {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }
    pub(crate) fn check_historical_straight_cache_for_test(
        &self,
        dependency: &ResidentJoinedStraightDependency,
        origin: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
        expected: &motion::PathFinderCache,
    ) -> Result<(), ResidentFilterError> {
        self.check_straight_native(dependency, origin, context)?;
        dependency.check_historical_native_cache_for_test(expected)?;
        if self.straight_native_cache(dependency, context)? != *expected {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }
    fn check_straight_native(
        &self,
        dependency: &ResidentJoinedStraightDependency,
        origin: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
    ) -> Result<(), ResidentFilterError> {
        let revision = dependency.source_revision;
        let previous_revision = revision.checked_sub(1).ok_or(Failure::Binding)?;
        self.check_straight_contexts(
            origin,
            &dependency.previous_context,
            previous_revision,
            context,
        )?;
        let cold = origin.retained_cold().historical_observation();
        if !Rc::ptr_eq(&dependency.root.provider, &self.identity)
            || !Rc::ptr_eq(&dependency.origin, &cold.observation_identity)
            || Rc::ptr_eq(&dependency.root.identity, &dependency.root.predecessor)
            || Rc::ptr_eq(&dependency.root.identity, &dependency.origin)
            || (Rc::ptr_eq(&dependency.root.predecessor, &dependency.origin)
                != (previous_revision == 1))
            || dependency.root.context != *context
            || !same_dispatch(&dependency.root.context, context)
            || !origin.retained_chase().same_water_model(&dependency.chase)
        {
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
        if previous_revision == 1 {
            if **cache
                != self.retained_native_cache(
                    origin.retained_cold().native_terminal(),
                    &cold.context,
                    1,
                )?
            {
                return Err(Failure::Binding.into());
            }
        } else {
            let prior = previous
                .request
                .prior_cache
                .as_ref()
                .ok_or(Failure::Binding)?;
            prior
                .validate_for(previous.request.binding.identity)
                .map_err(|_| Failure::Binding)?;
            if cache.binding != prior.binding
                || prior.revision.checked_add(1) != Some(cache.revision)
            {
                return Err(Failure::Binding.into());
            }
        }
        let native = &dependency.terminal;
        let cold_terminal = origin.retained_cold().native_terminal();
        let cold_native = cold_terminal.compatibility();
        straight_budget(native.before_active_len(), native.before_storage().len())?;
        let width = route
            .require_collision_width(&self.profile)
            .map_err(|_| Failure::Binding)?;
        if cache.revision != previous_revision
            || next_revision(cache.revision)? != revision
            || cache.source != motion::RouteSource::OwnerPathFinder
            || cache.poly_refs != native.before_storage()
            || usize::from(cache.active_len) != native.before_active_len()
            || cache.point_limit != motion::PATHFINDER_CACHE_POINT_LIMIT
            || cache.path_type != native.previous_path_type()
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
            || native.initial_storage() != cold_native.initial_storage()
            || native.initial_active_len() != cold_native.initial_active_len()
            || native.filter() != cold_native.filter()
            || native.compatibility_id() != cold_native.compatibility_id()
            || native.compatibility_version() != cold_native.compatibility_version()
            || native.slot() != cold_terminal.slot()
            || native.query_capacity() != 1024
            || native.query_capacity() != cold_terminal.query_capacity()
            || native.config() != CONFIG
            || native.collision_width_bits() != width.width().bits()
            || native.from() != bits(route.request.origin)
            || native.requested_end() != bits(route.request.goal)
            || native.end() != bits(route.request.goal)
            || native.actual_end() != bits(route.request.goal)
            || native.path_type() != 1
            || !(1..=74).contains(&native.active_len())
            || native.point_limit() != 74
            || !(75..=STORAGE_LIMIT).contains(&native.storage().len())
            || !(2..=STRAIGHT_MAX_POINTS).contains(&native.points().len())
            || native.points().first() != Some(&bits(route.request.origin))
            || native.points().last() != Some(&bits(route.request.goal))
            || native.trace().len() > LOOKUP_LIMIT
            || native.diagnostics().heights.len() > POST_LOOKUPS
        {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }
    pub(crate) fn straight_for<'a>(
        &self,
        dependency: &'a ResidentJoinedStraightDependency,
        origin: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<ResidentJoinedStraightObservation<'a>, ResidentFilterError> {
        let deadline = dependency
            .root
            .check_current(self, context, deadline, &cancelled)?;
        self.check_straight_native(dependency, origin, context)?;
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
            return Err(Failure::Binding.into());
        };
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
        let native = &dependency.terminal;
        Ok(ResidentJoinedStraightObservation {
            points: native.points(),
            trace: native.trace(),
            body,
        })
    }
    fn straight_native_cache(
        &self,
        dependency: &ResidentJoinedStraightDependency,
        context: &AuthorityRequest,
    ) -> Result<motion::PathFinderCache, ResidentFilterError> {
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::Binding.into());
        };
        let native = &dependency.terminal;
        let binding = self.query.binding();
        let cache = motion::PathFinderCache {
            binding: motion::PathFinderCacheBinding {
                identity: route.request.binding.identity,
                owner_generation: route.owner.npc().generation,
                inventory: binding.inventory,
                source_epoch: binding.source_epoch,
                navigation_policy: self.session.geometry,
            },
            revision: dependency.source_revision,
            source: motion::RouteSource::OwnerPathFinder,
            poly_refs: native.storage().to_vec(),
            active_len: u16::try_from(native.active_len()).map_err(|_| Failure::Invalid)?,
            point_limit: motion::PATHFINDER_CACHE_POINT_LIMIT,
            path_type: native.path_type(),
            requested_end: super::super::super::super::source_near_point::exact_point(
                native.requested_end(),
            )?,
            include_flags: native.filter().include_flags,
            exclude_flags: native.filter().exclude_flags,
        };
        cache
            .validate_for(route.request.binding.identity)
            .map_err(|_| Failure::Binding)?;
        Ok(cache)
    }
    pub(crate) fn straight_cache_for(
        &self,
        dependency: &ResidentJoinedStraightDependency,
        origin: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<motion::PathFinderCache, ResidentFilterError> {
        self.straight_for(dependency, origin, context, deadline, &cancelled)?;
        let cache = self.straight_native_cache(dependency, context)?;
        self.straight_for(dependency, origin, context, deadline, cancelled)?;
        Ok(cache)
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn straight_segment_los(
        &self,
        dependency: &ResidentJoinedStraightDependency,
        origin: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
        segment_index: usize,
        ticket: u64,
        budget: &mut QueryBudget,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<RayReply, ResidentFilterError> {
        let deadline = dependency.deadline(deadline);
        let observed = self.straight_for(dependency, origin, context, deadline, &cancelled)?;
        let expected = dependency.observations[1]
            .request()
            .ticket
            .checked_add(1)
            .and_then(|first| first.checked_add(u64::try_from(segment_index).ok()?))
            .ok_or(Failure::Invalid)?;
        let end_index = segment_index.checked_add(1).ok_or(Failure::Invalid)?;
        if ticket == 0 || ticket != expected || end_index >= observed.points.len() {
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
            .source_ray_with_budget(
                ticket,
                eye(observed.points[segment_index]),
                eye(observed.points[end_index]),
                budget,
                &cancelled,
            )
            .map_err(ResidentFilterError::Source)?;
        if ray.request.ignore_m2 || !matches!(ray.result, StaticRay::Miss) {
            return Err(Failure::Obstacle.into());
        }
        self.straight_for(dependency, origin, context, deadline, cancelled)?;
        Ok(ray)
    }
}
/// Valid source outputs outside this component's complete-route domain are not
/// corrupt provenance. In particular INCOMPLETE/NOPATH never trigger a fake fallback.
fn require_supported_terminal(
    native: &LoadedSourceRetainedStraight,
) -> Result<(), ResidentFilterError> {
    if native.path_type() != 1
        || native.end() != native.requested_end()
        || native.actual_end() != native.requested_end()
        || !(1..=74).contains(&native.active_len())
        || !(2..=STRAIGHT_MAX_POINTS).contains(&native.points().len())
        || native.points().first() != Some(&native.from())
        || native.points().last() != Some(&native.requested_end())
    {
        return Err(Failure::UnsupportedQuery.into());
    }
    // DetourStatus.h DT_SUCCESS is exactly 1<<30. Source may return detail flags
    // or a clipped ray; this component refuses them even if the raw type is NORMAL.
    // SamePolygon/Cut legitimately have no ray and retain their separate native proof.
    if let Some(ray) = native.ray() {
        if ray.status != (1 << 30)
            || ray.hit_bits != f32::MAX.to_bits()
            || ray.adjusted_hit_bits.is_some()
            || !(1..=74).contains(&ray.polygons.len())
            || ray.polygons.as_slice() != &native.storage()[..native.active_len()]
        {
            return Err(Failure::UnsupportedQuery.into());
        }
    }
    Ok(())
}
fn bits(point: Point) -> [u32; 3] {
    [point.x.bits(), point.y.bits(), point.z.bits()]
}
fn same_dispatch(a: &AuthorityRequest, b: &AuthorityRequest) -> bool {
    let (Query::SourceMotion(MotionQuery::Route(a)), Query::SourceMotion(MotionQuery::Route(b))) =
        (&a.query, &b.query)
    else {
        return false;
    };
    match (&a.pathfinder_dispatch, &b.pathfinder_dispatch) {
        (Some(a), Some(b)) => a.same_live_dispatch(b),
        _ => false,
    }
}
fn next_revision(previous: u64) -> Result<u64, ResidentFilterError> {
    if !(1..MAX_SOURCE_REVISION).contains(&previous) {
        return Err(Failure::UnsupportedQuery.into());
    }
    previous
        .checked_add(1)
        .ok_or_else(|| Failure::Invalid.into())
}
/// Count both original Cut scans: forward [0,p] and reverse (p,A) are disjoint,
/// including forward exhaustion. Start/end <=A+T+2 and A+T+3; together with Cut
/// <=3A+2T+5. At most 31 later lookups see active<=74 and tail<=T+2+(j-1),
/// hence each <=T+j+77 and total <=3A+33T+2888. Every lookup appends at most once.
fn straight_budget(
    active: usize,
    length: usize,
) -> Result<SourceStraightPathBudget, ResidentFilterError> {
    if !(1..=74).contains(&active) || !(75..=STORAGE_LIMIT).contains(&length) {
        return Err(native_error(benilla_native_nav::Error::Budget));
    }
    let tail = length
        .checked_sub(74)
        .ok_or_else(|| native_error(benilla_native_nav::Error::Budget))?;
    let bound = active
        .checked_mul(3)
        .and_then(|a| tail.checked_mul(33).and_then(|t| a.checked_add(t)))
        .and_then(|n| n.checked_add(2888))
        .ok_or_else(|| native_error(benilla_native_nav::Error::Budget))?;
    let storage = length
        .checked_add(2 + POST_LOOKUPS)
        .ok_or_else(|| native_error(benilla_native_nav::Error::Budget))?;
    if bound > LOOKUP_LIMIT || storage > STORAGE_LIMIT {
        return Err(native_error(benilla_native_nav::Error::Budget));
    }
    Ok(SourceStraightPathBudget {
        lookup_primitives: bound,
        raycast_polygons: 4096,
        sampled_points: STRAIGHT_MAX_POINTS,
        native_primitives: 4096,
    })
}
fn check_budget(
    budget: SourceStraightPathBudget,
    admitted: SourceStraightPathBudget,
) -> Result<(), ResidentFilterError> {
    if budget.lookup_primitives > admitted.lookup_primitives
        || budget.raycast_polygons > admitted.raycast_polygons
        || !(2..=admitted.sampled_points).contains(&budget.sampled_points)
        || budget.native_primitives > admitted.native_primitives
    {
        return Err(native_error(benilla_native_nav::Error::Budget));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_straight_budget_counts_selection_cut_hit_and_samples() {
        for (active, length, expected) in [
            (1, 75, 2924),
            (74, 223, 8027),
            (1, 224, 7841),
            (1, 230, 8039),
        ] {
            let budget = straight_budget(active, length).unwrap();
            assert_eq!(budget.lookup_primitives, expected);
            assert_eq!(budget.sampled_points, 32);
            assert_eq!(budget.raycast_polygons, 4096);
            assert_eq!(budget.native_primitives, 4096);
            let tail = length - 74;
            let endpoints = (active + tail + 2) + (active + tail + 3);
            let later: usize = (1..=31).map(|j| tail + j + 77).sum();
            assert_eq!(expected, endpoints + active + later);
        }
        for (a, n) in [
            (74, 224),
            (1, 231),
            (0, 75),
            (75, 75),
            (1, 74),
            (1, 4096),
            (1, usize::MAX),
        ] {
            assert!(straight_budget(a, n).is_err(), "A={a} N={n}");
        }
    }
    #[test]
    fn cut_scans_are_both_charged_without_overlap() {
        for active in 1usize..=74 {
            for stop in 0..=active {
                let forward = (stop + 1).min(active);
                let reverse = active.saturating_sub(stop + 1);
                assert!(forward + reverse <= active);
            }
        }
    }
    #[test]
    fn operational_budgets_refuse_widening_without_clamping() {
        let admitted = straight_budget(1, 75).unwrap();
        for enlarged in [
            SourceStraightPathBudget {
                lookup_primitives: admitted.lookup_primitives + 1,
                ..admitted
            },
            SourceStraightPathBudget {
                raycast_polygons: 4097,
                ..admitted
            },
            SourceStraightPathBudget {
                sampled_points: 33,
                ..admitted
            },
            SourceStraightPathBudget {
                sampled_points: 1,
                ..admitted
            },
            SourceStraightPathBudget {
                native_primitives: 4097,
                ..admitted
            },
        ] {
            assert!(check_budget(enlarged, admitted).is_err());
        }
        assert!(check_budget(
            SourceStraightPathBudget {
                lookup_primitives: 0,
                ..admitted
            },
            admitted
        )
        .is_ok());
        assert!(next_revision(1).is_ok());
        assert!(next_revision(7).is_ok());
        assert!(next_revision(0).is_err());
        assert!(next_revision(8).is_err());
        assert!(next_revision(u64::MAX).is_err());
    }
}
