// SPDX-License-Identifier: GPL-3.0-or-later
//! ComputePathToRandomPoint endpoint lookup and same-polygon terminal on one real query.
//! This is neither the calculate cold-distance policy nor owner/terrain/Route authority.
use crate::{
    Error, LoadedSourceWarmCache, RetainedSourceQuery, SelectedSourceQuery, SourceMapQuery,
    SourceMapQuerySlot, SourceOwnerFilter, SourceSamePolygonConfig, SourceWarmCacheError,
    SourceWarmLookup, SourceWarmTrace, WarmCache,
};
use std::time::Instant;

/// Approved local initialization, only at ComputePathToRandomPoint's endpoint lookup.
pub const SOURCE_WANDER_ENDPOINT_COMPATIBILITY_ID: &str =
    "cmangos-computepathtorandompoint-zero-distance-v1";
pub const SOURCE_WANDER_ENDPOINT_COMPATIBILITY_VERSION: u32 = 1;
pub const SOURCE_WANDER_ORIGINAL_METHOD_SHA256: &str =
    "4a151a1cab7cf8cd9444d2c7edc09f3c5252fefaf5d537ad1b374775929840da";
pub const SOURCE_WANDER_PATCHED_METHOD_SHA256: &str =
    "1f553a9cdb97d3441b9f3108fcf69d8c1a7df6101a41fb5e0b849cb07fe9265a";
pub const SOURCE_WANDER_ENDPOINT_COMPATIBILITY_REVISION: &str =
    "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f";
pub const SOURCE_WANDER_MAX_ENDPOINT_LOOKUP_PRIMITIVES: usize =
    crate::SOURCE_WARM_CACHE_MAX_STORAGE - crate::SOURCE_WARM_CACHE_POINT_LIMIT + 2;
pub const SOURCE_WANDER_MAX_BUILD_LOOKUP_PRIMITIVES: usize =
    2 * SOURCE_WANDER_MAX_ENDPOINT_LOOKUP_PRIMITIVES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceWanderBudget {
    pub endpoint_lookup_primitives: usize,
    pub build_lookup_primitives: usize,
    /// One exact getPolyHeight call followed by the finite same-polygon terminal.
    pub terminal_primitives: usize,
}
impl Default for SourceWanderBudget {
    fn default() -> Self {
        Self {
            endpoint_lookup_primitives: 3,
            build_lookup_primitives: 6,
            terminal_primitives: 1,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceWanderError {
    Boundary(Error),
    Lookup(SourceWarmCacheError),
    ConfigurationIdentity,
    NormalizationConfigurationUnsupported,
    TooClose,
    NotTooClose,
    NonSamePolygonUnsupported,
}
impl From<Error> for SourceWanderError {
    fn from(value: Error) -> Self {
        Self::Boundary(value)
    }
}
impl From<SourceWarmCacheError> for SourceWanderError {
    fn from(value: SourceWarmCacheError) -> Self {
        Self::Lookup(value)
    }
}

/// Genuine immutable Wander lineage, including three defined source NOPATH branches.
/// No constructor accepts refs, cache DTOs, another path type or a Chase/cold result.
/// Failed operations never mutate the prior handle. Normal results retain ORIGINAL
/// BuildPolyPath tail lookups; NOPATH retains the exact reached cache/end mutations.
///
/// ```compile_fail
/// use benilla_native_nav::{LoadedSourceSamePolygon, LoadedSourceWanderTerminal};
/// fn relabel(cold: LoadedSourceSamePolygon) -> LoadedSourceWanderTerminal { cold.into() }
/// ```
#[derive(Debug)]
pub struct LoadedSourceWanderTerminal {
    query: SourceMapQuery,
    before: WarmCache,
    from: [u32; 3],
    raw_end: [u32; 3],
    config: SourceSamePolygonConfig,
    state: WanderTerminalState,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceWanderNoPathReason {
    MinimumDistance,
    EndpointMissing,
    EndpointHeight,
}
#[derive(Debug)]
enum WanderTerminalState {
    SamePolygon {
        endpoint: SourceWarmLookup,
        endpoint_trace: Vec<SourceWarmTrace>,
        height_status: u32,
        build: LoadedSourceWarmCache,
        terminal: crate::warm_point_path::SamePolygonTerminal,
    },
    NoPath {
        reason: SourceWanderNoPathReason,
        after: WarmCache,
        end: [u32; 3],
        endpoint: Option<SourceWarmLookup>,
        endpoint_trace: Vec<SourceWarmTrace>,
        height_status: Option<u32>,
    },
}
impl LoadedSourceWanderTerminal {
    pub fn no_path_reason(&self) -> Option<SourceWanderNoPathReason> {
        match &self.state {
            WanderTerminalState::NoPath { reason, .. } => Some(*reason),
            _ => None,
        }
    }
    pub fn tail_required(&self) -> bool {
        self.no_path_reason() != Some(SourceWanderNoPathReason::MinimumDistance)
    }

    fn after(&self) -> &WarmCache {
        match &self.state {
            WanderTerminalState::SamePolygon { build, .. } => build.after(),
            WanderTerminalState::NoPath { after, .. } => after,
        }
    }
    pub fn check_query(&self, selected: &SelectedSourceQuery<'_>) -> Result<(), Error> {
        selected.check_allocation(&self.query)?;
        if selected.from() != self.from || selected.to() != self.raw_end {
            return Err(Error::Identity);
        }
        Ok(())
    }
    pub fn slot(&self) -> SourceMapQuerySlot {
        self.query.slot()
    }
    pub fn query_capacity(&self) -> u32 {
        self.query.query_capacity()
    }
    pub fn config(&self) -> SourceSamePolygonConfig {
        self.config
    }
    pub fn raw_end(&self) -> [u32; 3] {
        self.raw_end
    }
    pub fn actual_end(&self) -> [u32; 3] {
        match &self.state {
            WanderTerminalState::SamePolygon { terminal, .. } => terminal.actual_end,
            WanderTerminalState::NoPath { end, .. } => *end,
        }
    }
    pub fn points(&self) -> &[[u32; 3]] {
        match &self.state {
            WanderTerminalState::SamePolygon { terminal, .. } => &terminal.points,
            WanderTerminalState::NoPath { .. } => &[],
        }
    }
    pub fn path_type(&self) -> u32 {
        match &self.state {
            WanderTerminalState::SamePolygon { terminal, .. } => terminal.path_type,
            WanderTerminalState::NoPath { .. } => 8,
        }
    }
    pub fn filter(&self) -> SourceOwnerFilter {
        self.after().filter()
    }
    pub fn storage(&self) -> &[u64] {
        self.after().storage()
    }
    pub fn active_len(&self) -> usize {
        self.after().active_len()
    }
    pub fn before_storage(&self) -> &[u64] {
        self.before.storage()
    }
    pub fn before_active_len(&self) -> usize {
        self.before.active_len()
    }
    pub fn endpoint_lookup(&self) -> Option<SourceWarmLookup> {
        match &self.state {
            WanderTerminalState::SamePolygon { endpoint, .. } => Some(*endpoint),
            WanderTerminalState::NoPath { endpoint, .. } => *endpoint,
        }
    }
    pub fn endpoint_trace(&self) -> &[SourceWarmTrace] {
        match &self.state {
            WanderTerminalState::SamePolygon { endpoint_trace, .. }
            | WanderTerminalState::NoPath { endpoint_trace, .. } => endpoint_trace,
        }
    }
    pub fn height_status(&self) -> Option<u32> {
        match &self.state {
            WanderTerminalState::SamePolygon { height_status, .. } => Some(*height_status),
            WanderTerminalState::NoPath { height_status, .. } => *height_status,
        }
    }
    pub fn height_before(&self) -> [u32; 3] {
        self.raw_end
    }
    pub fn height_after(&self) -> [u32; 3] {
        self.actual_end()
    }
    pub fn start_lookup(&self) -> Option<SourceWarmLookup> {
        match &self.state {
            WanderTerminalState::SamePolygon { build, .. } => Some(build.start()),
            _ => None,
        }
    }
    pub fn end_lookup(&self) -> Option<SourceWarmLookup> {
        match &self.state {
            WanderTerminalState::SamePolygon { build, .. } => Some(build.end()),
            _ => None,
        }
    }
    pub fn build_trace(&self) -> &[SourceWarmTrace] {
        match &self.state {
            WanderTerminalState::SamePolygon { build, .. } => build.trace(),
            _ => &[],
        }
    }
}
/// Clear/type/straight/force prefix retained across the runtime's pre-RNG authority call.
/// It can only be made from the actual retained query and an opaque prior Wander terminal.
/// No endpoint exists yet. Selection is a separate source-ordered operation after updateFilter.
#[derive(Debug)]
pub struct LoadedSourceWanderPrefix {
    pub(crate) query: SourceMapQuery,
    pub(crate) selected: bool,
    before: WarmCache,
    cleared: WarmCache,
    retained_end: [u32; 3],
    config: SourceSamePolygonConfig,
    budget: SourceWanderBudget,
}
impl LoadedSourceWanderPrefix {
    pub fn path_type(&self) -> u32 {
        8
    }
    pub fn straight_line(&self) -> bool {
        true
    }
    pub fn force_destination(&self) -> bool {
        false
    }
    pub fn points(&self) -> &[[u32; 3]] {
        &[]
    }

    pub fn before_storage(&self) -> &[u64] {
        self.before.storage()
    }
    pub fn before_active_len(&self) -> usize {
        self.before.active_len()
    }
    pub fn cleared_active_len(&self) -> usize {
        self.cleared.active_len()
    }
    pub fn filter(&self) -> SourceOwnerFilter {
        self.cleared.filter()
    }
    pub fn current_selected(&self) -> bool {
        self.selected
    }
}
impl RetainedSourceQuery {
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_wander_same_polygon(
        &self,
        prior: Option<&LoadedSourceWanderTerminal>,
        compatibility_version: u32,
        filter: SourceOwnerFilter,
        config: SourceSamePolygonConfig,
        budget: SourceWanderBudget,
        until: Instant,
    ) -> Result<LoadedSourceWanderPrefix, SourceWanderError> {
        crate::deadline(until)?;
        if compatibility_version != SOURCE_WANDER_ENDPOINT_COMPATIBILITY_VERSION
            || !filter.is_supported_dry()
        {
            return Err(Error::Unsupported.into());
        }
        if config.normalize_z || config.ignore_normalization {
            return Err(SourceWanderError::NormalizationConfigurationUnsupported);
        }
        if budget.endpoint_lookup_primitives > SOURCE_WANDER_MAX_ENDPOINT_LOOKUP_PRIMITIVES
            || budget.build_lookup_primitives > SOURCE_WANDER_MAX_BUILD_LOOKUP_PRIMITIVES
            || budget.terminal_primitives != 1
        {
            return Err(Error::Budget.into());
        }
        #[cfg(benilla_native_detour)]
        {
            if let Some(prior) = prior {
                self.check_query(&prior.query)?;
                if prior.config != config || prior.filter() != filter {
                    return Err(SourceWanderError::ConfigurationIdentity);
                }
            }
            let query = self.retain_allocation();
            let before = match prior {
                Some(prior) => prior.after().try_copy()?,
                None => query.wander_constructor_storage(filter, until)?,
            };
            let mut cleared = before.try_copy()?;
            cleared.clear_active();
            crate::deadline(until)?;
            Ok(LoadedSourceWanderPrefix {
                query,
                selected: false,
                before,
                cleared,
                retained_end: prior.map_or([0; 3], LoadedSourceWanderTerminal::actual_end),
                config,
                budget,
            })
        }
        #[cfg(not(benilla_native_detour))]
        {
            let _ = prior;
            Err(Error::Unsupported.into())
        }
    }
}
impl SelectedSourceQuery<'_> {
    /// Convenience dependency composition only; it does not establish pre-draw owner order.
    /// The live app uses the explicit prepare/select/continue methods instead.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub fn loaded_wander_same_polygon(
        &self,
        prior: Option<&LoadedSourceWanderTerminal>,
        compatibility_version: u32,
        filter: SourceOwnerFilter,
        config: SourceSamePolygonConfig,
        budget: SourceWanderBudget,
        until: Instant,
    ) -> Result<LoadedSourceWanderTerminal, SourceWanderError> {
        #[cfg(benilla_native_detour)]
        {
            let query = self.retain_allocation();
            let mut retained = query.retain_default();
            let mut prefix = retained.prepare_wander_same_polygon(
                prior,
                compatibility_version,
                filter,
                config,
                budget,
                until,
            )?;
            retained.select_wander_prefix(&mut prefix, until)?;
            self.loaded_prepared_wander_same_polygon(&prefix, until)
        }
        #[cfg(not(benilla_native_detour))]
        {
            let _ = (prior, compatibility_version, filter, config, budget, until);
            Err(Error::Unsupported.into())
        }
    }
    /// Defined early return before setStartPosition/setEndPosition, owner-ignore,
    /// HaveTile, endpoint lookup, height, or water/above-ground tail. No Route is made.
    pub fn loaded_prepared_wander_no_path(
        &self,
        prefix: &LoadedSourceWanderPrefix,
        until: Instant,
    ) -> Result<LoadedSourceWanderTerminal, SourceWanderError> {
        crate::deadline(until)?;
        self.check_allocation(&prefix.query)?;
        if !prefix.selected {
            return Err(Error::Identity.into());
        }
        #[cfg(benilla_native_detour)]
        {
            let a = self.from().map(f32::from_bits);
            let b = self.to().map(f32::from_bits);
            crate::detour(self.from())?;
            crate::detour(self.to())?;
            let [x, y, z] = std::array::from_fn(|i| a[i] - b[i]);
            if x * x + y * y + z * z >= 0.01f32 {
                return Err(SourceWanderError::NotTooClose);
            }
            let result = LoadedSourceWanderTerminal {
                query: self.retain_allocation(),
                before: prefix.before.try_copy()?,
                from: self.from(),
                raw_end: self.to(),
                config: prefix.config,
                state: WanderTerminalState::NoPath {
                    reason: SourceWanderNoPathReason::MinimumDistance,
                    after: prefix.cleared.try_copy()?,
                    end: prefix.retained_end,
                    endpoint: None,
                    endpoint_trace: Vec::new(),
                    height_status: None,
                },
            };
            crate::deadline(until)?;
            Ok(result)
        }
        #[cfg(not(benilla_native_detour))]
        {
            Err(Error::Unsupported.into())
        }
    }
    /// Endpoint lookup, exact height, then ORIGINAL BuildPolyPath on the cleared prefix.
    /// Endpoint ref0 and failed height are typed NOPATH terminals, not generic errors.
    /// The separate owner must afterward perform IsInWater/IsAboveGround for every result.
    pub fn loaded_prepared_wander_same_polygon(
        &self,
        prefix: &LoadedSourceWanderPrefix,
        until: Instant,
    ) -> Result<LoadedSourceWanderTerminal, SourceWanderError> {
        crate::deadline(until)?;
        self.check_allocation(&prefix.query)?;
        if !prefix.selected {
            return Err(Error::Identity.into());
        }
        #[cfg(benilla_native_detour)]
        {
            let mut mesh = self.borrow_mesh()?;
            if mesh.query_profile() != crate::QueryProfile::SourceMapInstance1024
                || mesh.query_capacity() != 1024
            {
                return Err(Error::Identity.into());
            }
            let from = self.from().map(f32::from_bits);
            let raw = self.to().map(f32::from_bits);
            crate::detour(self.from())?;
            crate::detour(self.to())?;
            let [x, y, z] = std::array::from_fn(|i| from[i] - raw[i]);
            if x * x + y * y + z * z < 0.01f32 {
                return Err(SourceWanderError::TooClose);
            }
            let (endpoint_cache, endpoint, endpoint_trace) =
                crate::warm_cache::native::wander_endpoint(
                    &mut mesh,
                    &prefix.cleared,
                    self.to(),
                    prefix.budget.endpoint_lookup_primitives,
                    until,
                )?;
            let Some(endpoint) = endpoint else {
                crate::deadline(until)?;
                let result = LoadedSourceWanderTerminal {
                    query: self.retain_allocation(),
                    before: prefix.before.try_copy()?,
                    from: self.from(),
                    raw_end: self.to(),
                    config: prefix.config,
                    state: WanderTerminalState::NoPath {
                        reason: SourceWanderNoPathReason::EndpointMissing,
                        after: endpoint_cache,
                        end: self.to(),
                        endpoint: None,
                        endpoint_trace,
                        height_status: None,
                    },
                };
                crate::deadline(until)?;
                return Ok(result);
            };
            let point = crate::detour(self.to())?;
            let mut height = point[1];
            crate::deadline(until)?;
            unsafe extern "C" {
                fn benilla_nav_straight_height(
                    mesh: *mut std::ffi::c_void,
                    polygon: u64,
                    point: *const f32,
                    height: *mut f32,
                ) -> u32;
            }
            // SAFETY: actual selected query/reference, finite YZX input and initialized output.
            let height_status = unsafe {
                benilla_nav_straight_height(
                    mesh.raw.as_ptr(),
                    endpoint.polygon,
                    point.as_ptr(),
                    &mut height,
                )
            };
            // Source sets end Z before testing exact DT_SUCCESS, not dtStatusSucceed.
            let end = [self.to()[0], self.to()[1], height.to_bits()];
            crate::deadline(until)?;
            if height_status != crate::SUCCESS {
                crate::detour(end)?;
                let result = LoadedSourceWanderTerminal {
                    query: self.retain_allocation(),
                    before: prefix.before.try_copy()?,
                    from: self.from(),
                    raw_end: self.to(),
                    config: prefix.config,
                    state: WanderTerminalState::NoPath {
                        reason: SourceWanderNoPathReason::EndpointHeight,
                        after: endpoint_cache,
                        end,
                        endpoint: Some(endpoint),
                        endpoint_trace,
                        height_status: Some(height_status),
                    },
                };
                crate::deadline(until)?;
                return Ok(result);
            }
            crate::detour(end)?;
            // Endpoint lookup already created/consulted the retained tail; no BuildPolyPath
            // cold-distance repair is selected or needed here, including on the first leg.
            let build = crate::warm_cache::native::retained_same_polygon(
                &mut mesh,
                &endpoint_cache,
                self.from(),
                end,
                crate::SourceWarmCacheBudget {
                    lookup_primitives: prefix.budget.build_lookup_primitives,
                    corridor_expansions: 0,
                },
                until,
            )?
            .ok_or(SourceWanderError::NonSamePolygonUnsupported)?;
            crate::deadline(until)?;
            let terminal = crate::warm_point_path::same_polygon_terminal(self.from(), end);
            crate::deadline(until)?;
            Ok(LoadedSourceWanderTerminal {
                query: self.retain_allocation(),
                before: prefix.before.try_copy()?,
                from: self.from(),
                raw_end: self.to(),
                config: prefix.config,
                state: WanderTerminalState::SamePolygon {
                    endpoint,
                    endpoint_trace,
                    height_status,
                    build,
                    terminal,
                },
            })
        }
        #[cfg(not(benilla_native_detour))]
        {
            Err(Error::Unsupported.into())
        }
    }
}

#[cfg(all(test, benilla_native_detour))]
#[path = "source_wander_same_polygon_tests.rs"]
mod tests;
