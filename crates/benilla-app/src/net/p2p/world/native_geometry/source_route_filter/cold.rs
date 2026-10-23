// SPDX-License-Identifier: GPL-3.0-or-later
//! Conditional native same-polygon result; no Chase gate, Route or cache authority.
use super::*;
use benilla_native_nav::{
    LoadedSourceSamePolygon, LoadedSourceTilePresence, SourceOwnerFilter, SourceSamePolygonBudget,
    SourceSamePolygonConfig, SOURCE_COLD_DISTANCE_COMPATIBILITY_VERSION,
};
use benilla_world_runtime::native::OwnerIgnorePathfindingReceipt;

/// The native allocation/cache-bearing terminal never leaves this private type.
pub(crate) struct ResidentSamePolygonDependency {
    observation: ResidentFilterDependency,
    owner_ignore: OwnerIgnorePathfindingReceipt,
    tiles: LoadedSourceTilePresence,
    terminal: LoadedSourceSamePolygon,
}
#[cfg(all(test, not(benilla_headless_geometry)))]
impl ResidentSamePolygonDependency {
    pub(super) fn historical_observation(&self) -> &ResidentFilterDependency {
        &self.observation
    }
    pub(super) fn native_terminal(&self) -> &LoadedSourceSamePolygon {
        &self.terminal
    }
    pub(super) fn historical_calculate_ready(&self) -> bool {
        !self.owner_ignore.ignore_pathfinding() && self.tiles.both_present()
    }
}
/// Copied diagnostic observations, with no native query or cache reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResidentSamePolygonObservation {
    pub(crate) points: [[u32; 3]; 2],
    pub(crate) liquid_tickets: [u64; 2],
    pub(crate) liquid_masks: [u8; 2],
    pub(crate) actual_end: [u32; 3],
    pub(crate) path_type: u32,
    pub(crate) active_len: usize,
    pub(crate) filter: SourceOwnerFilter,
    pub(crate) query_capacity: u32,
    pub(crate) before_storage_zero: bool,
    pub(crate) compatibility_id: &'static str,
    pub(crate) compatibility_version: u32,
}
impl ResidentFilterProvider<'_, '_, '_, '_> {
    #[cfg(test)]
    pub(crate) fn retained_constructor_count(&self) -> usize {
        self.query.constructor_count()
    }

    // Explicit operational budget/deadline and source configuration are separate
    // from the provenance dependency and must not be inferred from each other.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn observe_cold_same_polygon(
        &mut self,
        prior: &ResidentFilterDependency,
        context: &AuthorityRequest,
        first_ticket: u64,
        config: SourceSamePolygonConfig,
        budget: SourceSamePolygonBudget,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<ResidentSamePolygonDependency, ResidentFilterError> {
        self.cold_inner(
            prior,
            context,
            first_ticket,
            config,
            budget,
            deadline,
            cancelled,
            |_| {},
        )
    }
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn observe_cold_same_polygon_observed(
        &mut self,
        prior: &ResidentFilterDependency,
        context: &AuthorityRequest,
        first_ticket: u64,
        config: SourceSamePolygonConfig,
        budget: SourceSamePolygonBudget,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        observe: impl FnMut(u8),
    ) -> Result<ResidentSamePolygonDependency, ResidentFilterError> {
        self.cold_inner(
            prior,
            context,
            first_ticket,
            config,
            budget,
            deadline,
            cancelled,
            observe,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn cold_inner(
        &mut self,
        prior: &ResidentFilterDependency,
        context: &AuthorityRequest,
        first_ticket: u64,
        config: SourceSamePolygonConfig,
        budget: SourceSamePolygonBudget,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        mut observe: impl FnMut(u8),
    ) -> Result<ResidentSamePolygonDependency, ResidentFilterError> {
        let deadline = deadline.min(prior.expires);
        let retained = self.filter_for(prior, context, deadline, &cancelled)?;
        if first_ticket == 0
            || first_ticket.checked_add(1).is_none()
            || first_ticket <= prior.observations[1].request().ticket
        {
            return Err(Failure::Binding.into());
        }
        let event = context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::UnsupportedQuery.into());
        };
        if !event.first_construction()
            || event.constructor_kind() != motion::PathfinderConstructorKind::ChaseLazy
            || route.request.prior_cache.is_some()
            || !matches!(
                route.request.kind,
                motion::RouteKind::ChaseInitial | motion::RouteKind::ChaseReplan
            )
            || route.request.launch_origin != route.request.origin
            || route.request.point_limit != motion::PATHFINDER_CACHE_POINT_LIMIT
        {
            return Err(Failure::UnsupportedQuery.into());
        }
        // select_for_calculate preserves original destination/start coordinate
        // checks and the retained source1024 allocation.
        let state = &mut *self.state;
        let mut constructor = state
            .query
            .retain_constructor(context)
            .map_err(ResidentFilterError::Query)?;
        let selected = constructor
            .select_for_calculate(deadline)
            .map_err(ResidentFilterError::Query)?;
        observe(0);
        check_time(deadline, &cancelled)?;
        let owner_ignore = route
            .require_owner_ignore_pathfinding(&state.profile)
            .map_err(|_| Failure::Binding)?
            .clone();
        if owner_ignore.ignore_pathfinding() {
            return Err(Failure::UnsupportedQuery.into());
        }
        observe(1);
        check_time(deadline, &cancelled)?;
        let tiles = selected.tile_presence(deadline).map_err(native_error)?;
        selected.check_tile_presence(&tiles).map_err(native_error)?;
        if !tiles.both_present() {
            return Err(Failure::MissingGeometry.into());
        }
        observe(2);
        check_time(deadline, &cancelled)?;
        // Fresh calculate updateFilter observations, never the constructor pair.
        // Disjoint field borrowing preserves the selected query throughout.
        let (observations, filter) = point_observations(
            self.height,
            context,
            first_ticket,
            deadline,
            &cancelled,
            Some(retained),
            &mut |phase| observe(phase + 3),
        )?;
        check_time(deadline, &cancelled)?;
        let terminal = selected
            .loaded_cold_same_polygon_with_filter(
                SOURCE_COLD_DISTANCE_COMPATIBILITY_VERSION,
                filter,
                config,
                budget,
                deadline,
            )
            .map_err(ResidentFilterError::Terminal)?;
        terminal.check_query(&selected).map_err(native_error)?;
        observe(5);
        let bits = |p: Point| [p.x.bits(), p.y.bits(), p.z.bits()];
        let compatibility = terminal.compatibility();
        if terminal.points() != [bits(route.request.origin), bits(route.request.goal)]
            || terminal.actual_end() != bits(route.request.goal)
            || terminal.path_type() != 1
            || compatibility.active_len() != 1
            || compatibility.filter() != filter
            || terminal.config() != config
            || compatibility.is_retained()
            || compatibility.before_active_len() != 0
            || compatibility.before_storage().len()
                != usize::from(motion::PATHFINDER_CACHE_POINT_LIMIT)
            || compatibility.before_storage().iter().any(|id| *id != 0)
        {
            return Err(Failure::Binding.into());
        }
        self.check_context(context, deadline, &cancelled)?;
        self.height
            .verify_unchanged(&cancelled)
            .map_err(ResidentFilterError::Source)?;
        self.check_context(context, deadline, &cancelled)?;
        Ok(ResidentSamePolygonDependency {
            observation: ResidentFilterDependency {
                identity: Rc::clone(&self.identity),
                observation_identity: Rc::new(()),
                context: context.clone(),
                expires: deadline,
                observations,
                filter,
            },
            owner_ignore,
            tiles,
            terminal,
        })
    }
    /// Cache state comes from the owned native terminal, never its diagnostic copy.
    #[cfg(not(benilla_headless_geometry))]
    pub(super) fn cold_cache(
        &self,
        dependency: &ResidentSamePolygonDependency,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<motion::PathFinderCache, ResidentFilterError> {
        self.terminal_for(dependency, context, deadline, cancelled)?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::Binding.into());
        };
        let terminal = &dependency.terminal;
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
            revision: 1,
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
    /// Revalidate exact live context, expiry and consumed sources before copying
    /// observations. No terminal, native storage, query or route evidence escapes.
    pub(crate) fn terminal_for(
        &self,
        dependency: &ResidentSamePolygonDependency,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<ResidentSamePolygonObservation, ResidentFilterError> {
        let filter = self.filter_for(&dependency.observation, context, deadline, cancelled)?;
        if dependency.owner_ignore.ignore_pathfinding() || !dependency.tiles.both_present() {
            return Err(Failure::Binding.into());
        }
        let terminal = &dependency.terminal;
        let compatibility = terminal.compatibility();
        Ok(ResidentSamePolygonObservation {
            liquid_tickets: dependency
                .observation
                .observations
                .each_ref()
                .map(|reply| reply.request().ticket),
            liquid_masks: dependency
                .observation
                .observations
                .each_ref()
                .map(|reply| reply.request().mask),
            points: terminal.points().try_into().map_err(|_| Failure::Binding)?,
            actual_end: terminal.actual_end(),
            path_type: terminal.path_type(),
            active_len: compatibility.active_len(),
            filter,
            query_capacity: terminal.query_capacity(),
            before_storage_zero: compatibility.before_active_len() == 0
                && compatibility.before_storage().len()
                    == usize::from(motion::PATHFINDER_CACHE_POINT_LIMIT)
                && compatibility.before_storage().iter().all(|id| *id == 0),
            compatibility_id: compatibility.compatibility_id(),
            compatibility_version: compatibility.compatibility_version(),
        })
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
