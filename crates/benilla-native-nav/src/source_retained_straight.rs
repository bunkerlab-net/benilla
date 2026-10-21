// SPDX-License-Identifier: GPL-3.0-or-later
//! Original retained straight calculation after a genuine selected cold-origin terminal.
//! PathFinder.cpp:522-792,802-850,942-983; no Chase dispatch, fallback or Route authority.
use super::{LoadedSourceSamePolygon, SourceSamePolygonConfig};
#[cfg(benilla_native_detour)]
use crate::SourcePointPathConfig;
use crate::{
    Error, LoadedSourceWarmStraightPath, SelectedSourceQuery, SourceMapQuery, SourceMapQuerySlot,
    SourceOwnerFilter, SourceStraightPathBudget, SourceStraightPathDiagnostics,
    SourceStraightPathError, SourceStraightRay, SourceWarmLookup, SourceWarmPolyPhase,
    SourceWarmTrace, WarmCache,
};
use std::{rc::Rc, time::Instant};

/// Explicit straight-calculation configuration. Source ignores optimize_path here;
/// it is nevertheless bound across this lineage. Force destination is always false.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceRetainedStraightConfig {
    pub normalization: SourceSamePolygonConfig,
    pub optimize_path: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceRetainedStraightError {
    Boundary(Error),
    Straight(SourceStraightPathError),
    NormalizationConfigurationUnsupported,
    ConfigurationIdentity,
    CollisionWidthIdentity,
}
impl From<Error> for SourceRetainedStraightError {
    fn from(value: Error) -> Self {
        Self::Boundary(value)
    }
}
impl From<SourceStraightPathError> for SourceRetainedStraightError {
    fn from(value: SourceStraightPathError) -> Self {
        Self::Straight(value)
    }
}

/// Immutable proposal retaining the actual source1024 allocation and cold-origin policy.
/// No owner acceptance, full Chase calculation, fallback, save/restore or Route is implied.
/// Initial/before/after storage is bounded and independent; predecessor tokens do not own
/// earlier result objects. Public observations cannot recreate or erase loaded authority.
///
/// ```compile_fail
/// use benilla_native_nav::{LoadedSourceRetainedStraight, WarmCache};
/// fn erase_policy(result: LoadedSourceRetainedStraight) -> WarmCache {
///     result.into_after()
/// }
/// ```
///
/// ```compile_fail
/// use benilla_native_nav::{LoadedSourceRetainedStraight, LoadedSourceWarmStraightPath};
/// fn relabel(result: LoadedSourceRetainedStraight) -> LoadedSourceWarmStraightPath {
///     result.into()
/// }
/// ```
///
/// ```compile_fail
/// use benilla_native_nav::{LoadedSourceRetainedStraight, SourceMapQuery};
/// fn restore(query: SourceMapQuery, refs: &[u64]) -> LoadedSourceRetainedStraight {
///     LoadedSourceRetainedStraight::restore(query, refs)
/// }
/// ```
///
/// ```compile_fail
/// use benilla_native_nav::{LoadedSourceRetainedStraight, LoadedSourceWarmStraightPath};
/// fn extract(result: LoadedSourceRetainedStraight) -> LoadedSourceWarmStraightPath {
///     result.terminal
/// }
/// ```
///
/// ```compile_fail
/// use benilla_native_nav::{LoadedSourceRetainedStraight, WarmCache};
/// fn forge(cache: WarmCache) -> LoadedSourceRetainedStraight { cache.into() }
/// ```
#[derive(Debug)]
pub struct LoadedSourceRetainedStraight {
    query: SourceMapQuery,
    identity: Rc<()>,
    predecessor: Rc<()>,
    compatibility_version: u32,
    initial: WarmCache,
    before: WarmCache,
    config: SourceRetainedStraightConfig,
    terminal: LoadedSourceWarmStraightPath,
}
impl LoadedSourceRetainedStraight {
    pub fn check_query(&self, selected: &SelectedSourceQuery<'_>) -> Result<(), Error> {
        selected.check_allocation(&self.query)?;
        if selected.from() != self.from() || selected.to() != self.requested_end() {
            return Err(Error::Identity);
        }
        Ok(())
    }
    /// Exact immediate predecessor, not equal bytes or another same-query proposal.
    pub fn check_same_polygon_predecessor(
        &self,
        prior: &LoadedSourceSamePolygon,
    ) -> Result<(), Error> {
        if Rc::ptr_eq(&self.predecessor, &prior.identity) {
            Ok(())
        } else {
            Err(Error::Identity)
        }
    }
    pub fn check_predecessor(&self, prior: &Self) -> Result<(), Error> {
        if Rc::ptr_eq(&self.predecessor, &prior.identity) {
            Ok(())
        } else {
            Err(Error::Identity)
        }
    }
    pub fn slot(&self) -> SourceMapQuerySlot {
        self.query.slot()
    }
    pub fn query_capacity(&self) -> u32 {
        self.query.query_capacity()
    }
    pub fn query_policy(&self) -> &'static str {
        crate::SOURCE_MAP_QUERY_POLICY
    }
    pub fn compatibility_id(&self) -> &'static str {
        crate::SOURCE_COLD_DISTANCE_COMPATIBILITY_ID
    }
    pub fn compatibility_version(&self) -> u32 {
        self.compatibility_version
    }
    pub fn source_revision(&self) -> &'static str {
        crate::SOURCE_COLD_DISTANCE_COMPATIBILITY_REVISION
    }
    pub fn initial_storage(&self) -> &[u64] {
        self.initial.storage()
    }
    pub fn initial_active_len(&self) -> usize {
        self.initial.active_len()
    }
    pub fn before_storage(&self) -> &[u64] {
        self.before.storage()
    }
    pub fn before_active_len(&self) -> usize {
        self.before.active_len()
    }
    pub fn storage(&self) -> &[u64] {
        self.terminal.after().storage()
    }
    pub fn active_len(&self) -> usize {
        self.terminal.after().active_len()
    }
    pub fn point_limit(&self) -> usize {
        self.initial.point_limit()
    }
    pub fn filter(&self) -> SourceOwnerFilter {
        self.initial.filter()
    }
    pub fn config(&self) -> SourceRetainedStraightConfig {
        self.config
    }
    pub fn previous_path_type(&self) -> u32 {
        self.terminal.config().previous_path_type
    }
    pub fn collision_width_bits(&self) -> u32 {
        self.terminal.collision_width_bits()
    }
    pub fn from(&self) -> [u32; 3] {
        self.terminal.from()
    }
    pub fn requested_end(&self) -> [u32; 3] {
        self.terminal.requested_end()
    }
    pub fn end(&self) -> [u32; 3] {
        self.terminal.end()
    }
    pub fn actual_end(&self) -> [u32; 3] {
        self.terminal.actual_end()
    }
    pub fn points(&self) -> &[[u32; 3]] {
        self.terminal.points()
    }
    /// Raw straight result. NOPATH/INCOMPLETE does not execute Chase's smooth retry.
    pub fn path_type(&self) -> u32 {
        self.terminal.path_type()
    }
    pub fn start_lookup(&self) -> SourceWarmLookup {
        self.terminal.start_lookup()
    }
    pub fn end_lookup(&self) -> SourceWarmLookup {
        self.terminal.end_lookup()
    }
    pub fn phase(&self) -> SourceWarmPolyPhase {
        self.terminal.phase()
    }
    pub fn ray(&self) -> Option<&SourceStraightRay> {
        self.terminal.ray()
    }
    pub fn trace(&self) -> &[SourceWarmTrace] {
        self.terminal.trace()
    }
    pub fn diagnostics(&self) -> &SourceStraightPathDiagnostics {
        self.terminal.diagnostics()
    }
}

impl SelectedSourceQuery<'_> {
    /// Start a straight continuation from the genuine initial or retained same-polygon
    /// result. Original warm lookup runs once on its private cache, without cold repair.
    pub fn loaded_retained_straight_from_same_polygon(
        &self,
        prior: &LoadedSourceSamePolygon,
        config: SourceRetainedStraightConfig,
        collision_width_bits: u32,
        budget: SourceStraightPathBudget,
        until: Instant,
    ) -> Result<LoadedSourceRetainedStraight, SourceRetainedStraightError> {
        crate::deadline(until)?;
        self.check_allocation(&prior.query)?;
        if config.normalization != prior.config() {
            return Err(SourceRetainedStraightError::ConfigurationIdentity);
        }
        validate_config(config)?;
        #[cfg(benilla_native_detour)]
        {
            self.retained_straight(
                &prior.identity,
                prior.compatibility().compatibility_version(),
                prior.compatibility().initial_cache(),
                prior.compatibility().retained_cache(),
                prior.path_type(),
                config,
                collision_width_bits,
                budget,
                until,
            )
        }
        #[cfg(not(benilla_native_detour))]
        {
            let _ = (collision_width_bits, budget);
            Err(Error::Unsupported.into())
        }
    }

    /// Continue from this exact prior straight terminal, including its raw type and full
    /// mutated cache. Configuration and full-width bits remain bound. This is another
    /// explicit straight calculation, not Chase dispatch or a change of owner dimensions.
    pub fn loaded_retained_straight(
        &self,
        prior: &LoadedSourceRetainedStraight,
        config: SourceRetainedStraightConfig,
        collision_width_bits: u32,
        budget: SourceStraightPathBudget,
        until: Instant,
    ) -> Result<LoadedSourceRetainedStraight, SourceRetainedStraightError> {
        crate::deadline(until)?;
        self.check_allocation(&prior.query)?;
        if config != prior.config {
            return Err(SourceRetainedStraightError::ConfigurationIdentity);
        }
        if collision_width_bits != prior.collision_width_bits() {
            return Err(SourceRetainedStraightError::CollisionWidthIdentity);
        }
        validate_config(config)?;
        #[cfg(benilla_native_detour)]
        {
            self.retained_straight(
                &prior.identity,
                prior.compatibility_version,
                &prior.initial,
                prior.terminal.after(),
                prior.path_type(),
                config,
                collision_width_bits,
                budget,
                until,
            )
        }
        #[cfg(not(benilla_native_detour))]
        {
            let _ = (collision_width_bits, budget);
            Err(Error::Unsupported.into())
        }
    }

    #[cfg(benilla_native_detour)]
    #[allow(clippy::too_many_arguments)]
    fn retained_straight(
        &self,
        predecessor: &Rc<()>,
        compatibility_version: u32,
        initial: &WarmCache,
        before: &WarmCache,
        previous_path_type: u32,
        config: SourceRetainedStraightConfig,
        collision_width_bits: u32,
        budget: SourceStraightPathBudget,
        until: Instant,
    ) -> Result<LoadedSourceRetainedStraight, SourceRetainedStraightError> {
        let mut mesh = self.borrow_mesh()?;
        if mesh.query_profile() != crate::QueryProfile::SourceMapInstance1024
            || mesh.query_capacity() != 1024
        {
            return Err(Error::Identity.into());
        }
        initial.check_mesh(&mesh)?;
        before.check_mesh(&mesh)?;
        if initial.filter() != before.filter() {
            return Err(Error::Identity.into());
        }
        let initial = initial.try_copy()?;
        let before = before.try_copy()?;
        crate::deadline(until)?;
        let terminal = mesh.loaded_source_warm_straight_path(
            &before,
            self.from(),
            self.to(),
            collision_width_bits,
            SourcePointPathConfig {
                optimize_path: config.optimize_path,
                normalize_z: config.normalization.normalize_z,
                ignore_normalization: config.normalization.ignore_normalization,
                force_destination: false,
                previous_path_type,
            },
            budget,
            until,
        )?;
        terminal.after().check_mesh(&mesh)?;
        if terminal.after().filter() != initial.filter() {
            return Err(Error::Identity.into());
        }
        let result = LoadedSourceRetainedStraight {
            query: self.retain_allocation(),
            identity: Rc::new(()),
            predecessor: Rc::clone(predecessor),
            compatibility_version,
            initial,
            before,
            config,
            terminal,
        };
        crate::deadline(until)?;
        Ok(result)
    }
}

fn validate_config(
    config: SourceRetainedStraightConfig,
) -> Result<(), SourceRetainedStraightError> {
    if config.normalization.normalize_z || config.normalization.ignore_normalization {
        return Err(SourceRetainedStraightError::NormalizationConfigurationUnsupported);
    }
    Ok(())
}

#[cfg(all(test, benilla_native_detour))]
#[path = "source_retained_straight_tests.rs"]
mod tests;
