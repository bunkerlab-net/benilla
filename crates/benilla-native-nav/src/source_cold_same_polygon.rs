// SPDX-License-Identifier: GPL-3.0-or-later
//! Selected source1024 query and immutable same-polygon lineage from the approved cold origin.
//! This dependency grants no owner constructor/configuration/dispatch or Route authority.
use crate::{
    Error, LoadedSourceColdDistanceCompatibility, SelectedSourceQuery, SourceMapQuery,
    SourceMapQuerySlot, SourceOwnerFilter, SourceWarmCacheError,
};
use std::{rc::Rc, time::Instant};

#[path = "source_chase_fixed_cache.rs"]
mod chase_fixed_cache;
pub use chase_fixed_cache::*;

#[path = "source_retained_straight.rs"]
mod retained_straight;
pub use retained_straight::*;

/// Explicit NormalizePath state. There is deliberately no server-default constructor.
/// Only false/false is supported, without a terrain callback or ignore-state bypass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceSamePolygonConfig {
    pub normalize_z: bool,
    pub ignore_normalization: bool,
}

/// Cold start has two nearest attempts; end has at most one appended tail scan and
/// two nearest attempts. No active scan or direct search exists in this domain.
pub const SOURCE_COLD_SAME_POLYGON_MAX_LOOKUP_PRIMITIVES: usize = 5;

/// Each retained endpoint has one active slot, at most the full searchable tail,
/// and two nearest attempts. This bounds work, not the source point limit or cache.
pub const SOURCE_RETAINED_SAME_POLYGON_MAX_LOOKUP_PRIMITIVES: usize =
    2 * (crate::SOURCE_WARM_CACHE_MAX_STORAGE - crate::SOURCE_WARM_CACHE_POINT_LIMIT + 3);

/// Operational work limits, independent of the source's 74-point constructor limit.
/// A terminal primitive comprises the finite two-point BuildShortcut plus same-poly
/// cache/type assignment. Native lookups and terminal work check deadlines on both sides;
/// there is no wall-clock preemption inside one bounded primitive. The default lookup
/// budget covers every cold case and the common retained active-hit case. Retained
/// callers can explicitly raise it to SOURCE_RETAINED_SAME_POLYGON_MAX_LOOKUP_PRIMITIVES.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceSamePolygonBudget {
    pub lookup_primitives: usize,
    pub terminal_primitives: usize,
}
impl Default for SourceSamePolygonBudget {
    fn default() -> Self {
        Self {
            lookup_primitives: SOURCE_COLD_SAME_POLYGON_MAX_LOOKUP_PRIMITIVES,
            terminal_primitives: 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceSamePolygonError {
    Boundary(Error),
    Lookup(SourceWarmCacheError),
    NormalizationConfigurationUnsupported,
    NonSamePolygonUnsupported,
    ConfigurationIdentity,
}
impl From<Error> for SourceSamePolygonError {
    fn from(value: Error) -> Self {
        Self::Boundary(value)
    }
}
impl From<SourceWarmCacheError> for SourceSamePolygonError {
    fn from(value: SourceWarmCacheError) -> Self {
        Self::Lookup(value)
    }
}

/// One opaque initial-or-retained terminal proposal on the genuine source1024 allocation.
/// The approved cold-origin policy and zero constructor storage remain bound through
/// repeated original-warm calculations, with exact latest before/after observations.
/// A result is an immutable proposal, never evidence that its owner accepted it.
/// No serialization, original-warm relabel or public after-cache extraction is available.
/// Read-only polygon/point observations are not owner or gameplay Route evidence.
///
/// ```compile_fail
/// use benilla_native_nav::{LoadedSourceSamePolygon, WarmCache};
/// fn erase_policy(result: LoadedSourceSamePolygon) -> WarmCache {
///     result.into_after()
/// }
/// ```
///
/// ```compile_fail
/// use benilla_native_nav::{LoadedSourceSamePolygon, LoadedSourceWarmPointPath};
/// fn erase_policy(result: LoadedSourceSamePolygon) -> LoadedSourceWarmPointPath {
///     result.into()
/// }
/// ```
///
/// ```compile_fail
/// use benilla_native_nav::{LoadedSourceSamePolygon, WarmCache};
/// fn seed(cache: WarmCache) -> LoadedSourceSamePolygon { cache.into() }
/// ```
///
/// ```compile_fail
/// use benilla_native_nav::{LoadedSourceSamePolygon, LoadedSourceColdSamePolygon};
/// fn relabel(result: LoadedSourceSamePolygon) -> LoadedSourceColdSamePolygon {
///     LoadedSourceColdSamePolygon::from_retained_storage(result.compatibility().storage())
/// }
/// ```
///
/// ```compile_fail
/// use benilla_native_nav::{LoadedSourceSamePolygon, SourceMapQuery};
/// fn restore(query: SourceMapQuery, refs: &[u64]) -> LoadedSourceSamePolygon {
///     LoadedSourceSamePolygon::restore(query, refs)
/// }
/// ```
#[derive(Debug)]
pub struct LoadedSourceSamePolygon {
    query: SourceMapQuery,
    identity: Rc<()>,
    predecessor: Option<Rc<()>>,
    compatibility: LoadedSourceColdDistanceCompatibility,
    config: SourceSamePolygonConfig,
    terminal: crate::warm_point_path::SamePolygonTerminal,
}
/// Compatibility names for callers of the original cold-only entry point; these are
/// aliases of the shared lineage and configuration, never conversions or new proofs.
pub type LoadedSourceColdSamePolygon = LoadedSourceSamePolygon;
pub type SourceColdSamePolygonConfig = SourceSamePolygonConfig;
pub type SourceColdSamePolygonBudget = SourceSamePolygonBudget;
pub type SourceColdSamePolygonError = SourceSamePolygonError;

impl LoadedSourceSamePolygon {
    /// Original allocation and calculate endpoints must both match; source slot or byte
    /// equality is insufficient. The proposal itself keeps that allocation alive.
    pub fn check_query(&self, selected: &SelectedSourceQuery<'_>) -> Result<(), Error> {
        selected.check_allocation(&self.query)?;
        if selected.from() != self.compatibility.from() || selected.to() != self.compatibility.to()
        {
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
    pub fn compatibility(&self) -> &LoadedSourceColdDistanceCompatibility {
        &self.compatibility
    }
    pub fn config(&self) -> SourceSamePolygonConfig {
        self.config
    }
    /// Source owner XYZ bits, with explicitly disabled NormalizePath.
    pub fn points(&self) -> &[[u32; 3]] {
        &self.terminal.points
    }
    pub fn actual_end(&self) -> [u32; 3] {
        self.terminal.actual_end
    }
    /// Raw source PATHFIND_NORMAL only, never complete-route authority.
    pub fn path_type(&self) -> u32 {
        self.terminal.path_type
    }
    pub fn terminal_primitives(&self) -> usize {
        1
    }
}
impl SelectedSourceQuery<'_> {
    /// Propose a retained same-polygon calculation from the actual prior opaque result.
    /// Endpoints may change, but its query allocation and explicit configuration cannot.
    /// The nonempty active scan defines both original source distance locals; this does
    /// not apply the cold repair again. Complete prior storage survives every failure.
    /// Source straight calculation reaches the same-polygon terminal without invoking
    /// raycast, BuildPointPath, smoothing, or any owner/Route admission mechanism.
    pub fn loaded_retained_same_polygon(
        &self,
        prior: &LoadedSourceSamePolygon,
        config: SourceSamePolygonConfig,
        budget: SourceSamePolygonBudget,
        until: Instant,
    ) -> Result<LoadedSourceSamePolygon, SourceSamePolygonError> {
        crate::deadline(until)?;
        self.check_allocation(&prior.query)?;
        if config != prior.config {
            return Err(SourceSamePolygonError::ConfigurationIdentity);
        }
        if config.normalize_z || config.ignore_normalization {
            return Err(SourceSamePolygonError::NormalizationConfigurationUnsupported);
        }
        if budget.lookup_primitives > SOURCE_RETAINED_SAME_POLYGON_MAX_LOOKUP_PRIMITIVES
            || budget.terminal_primitives != 1
        {
            return Err(Error::Budget.into());
        }
        #[cfg(benilla_native_detour)]
        {
            let mut mesh = self.borrow_mesh()?;
            if mesh.query_profile() != crate::QueryProfile::SourceMapInstance1024
                || mesh.query_capacity() != 1024
            {
                return Err(Error::Identity.into());
            }
            let compatibility = prior
                .compatibility
                .retained_same_polygon(
                    &mut mesh,
                    self.from(),
                    self.to(),
                    budget.lookup_primitives,
                    until,
                )?
                .ok_or(SourceSamePolygonError::NonSamePolygonUnsupported)?;
            crate::deadline(until)?;
            let terminal = crate::warm_point_path::same_polygon_terminal(self.from(), self.to());
            crate::deadline(until)?;
            Ok(LoadedSourceSamePolygon {
                query: self.retain_allocation(),
                identity: Rc::new(()),
                predecessor: Some(Rc::clone(&prior.identity)),
                compatibility,
                config,
                terminal,
            })
        }
        #[cfg(not(benilla_native_detour))]
        {
            Err(Error::Unsupported.into())
        }
    }
    /// Execute the approved cold lookup and same-polygon branch on this actual selected
    /// query. No direct query allocation, warm seed, smoothing or alternate branch runs.
    /// Every failure discards the private candidate; no retained path cache is changed
    /// or candidate published. Native lookup scratch may change and is safe to retry.
    /// Constructor/config/runtime dispatch provenance remains the application's
    /// responsibility; this call supplies none of those prerequisites.
    pub fn loaded_cold_same_polygon(
        &self,
        compatibility_version: u32,
        config: SourceSamePolygonConfig,
        budget: SourceSamePolygonBudget,
        until: Instant,
    ) -> Result<LoadedSourceSamePolygon, SourceSamePolygonError> {
        self.loaded_cold_same_polygon_with_filter(
            compatibility_version,
            SourceOwnerFilter::GROUND,
            config,
            budget,
            until,
        )
    }

    /// Preserve the caller's source-derived 1/0 or 13/0 filter in every initial and
    /// retained native query. The mesh still contains only validated ground polygons;
    /// callers must separately prove the exact owner's capabilities and dry branch.
    pub fn loaded_cold_same_polygon_with_filter(
        &self,
        compatibility_version: u32,
        filter: SourceOwnerFilter,
        config: SourceSamePolygonConfig,
        budget: SourceSamePolygonBudget,
        until: Instant,
    ) -> Result<LoadedSourceSamePolygon, SourceSamePolygonError> {
        crate::deadline(until)?;
        if !filter.is_supported_dry()
            || compatibility_version != crate::SOURCE_COLD_DISTANCE_COMPATIBILITY_VERSION
        {
            return Err(Error::Unsupported.into());
        }
        if config.normalize_z || config.ignore_normalization {
            return Err(SourceSamePolygonError::NormalizationConfigurationUnsupported);
        }
        if budget.lookup_primitives > SOURCE_COLD_SAME_POLYGON_MAX_LOOKUP_PRIMITIVES
            || budget.terminal_primitives != 1
        {
            return Err(Error::Budget.into());
        }
        #[cfg(benilla_native_detour)]
        {
            let mut mesh = self.borrow_mesh()?;
            if mesh.query_profile() != crate::QueryProfile::SourceMapInstance1024
                || mesh.query_capacity() != 1024
            {
                return Err(Error::Identity.into());
            }
            let compatibility = LoadedSourceColdDistanceCompatibility::same_polygon(
                &mut mesh,
                self.from(),
                self.to(),
                filter,
                budget.lookup_primitives,
                until,
            )?
            .ok_or(SourceSamePolygonError::NonSamePolygonUnsupported)?;
            // PathFinder.cpp:508–519: BuildShortcut clears active length, installs the
            // exact start/actual-end points, then restores poly[0]/active=1 and NORMAL.
            // The private same-polygon selection already preserves that exact retained
            // polygon storage. NormalizePath:990 returns for explicit normalize_z=false.
            crate::deadline(until)?;
            let terminal = crate::warm_point_path::same_polygon_terminal(self.from(), self.to());
            crate::deadline(until)?;
            Ok(LoadedSourceSamePolygon {
                query: self.retain_allocation(),
                identity: Rc::new(()),
                predecessor: None,
                compatibility,
                config,
                terminal,
            })
        }
        #[cfg(not(benilla_native_detour))]
        {
            Err(Error::Unsupported.into())
        }
    }
}

#[cfg(all(test, benilla_native_detour))]
#[path = "source_cold_same_polygon_tests.rs"]
mod tests;

#[cfg(all(test, benilla_native_detour))]
#[path = "source_retained_same_polygon_tests.rs"]
mod retained_tests;

#[cfg(all(test, benilla_native_detour))]
#[path = "source_chase_fixed_cache_tests.rs"]
mod chase_fixed_cache_tests;
