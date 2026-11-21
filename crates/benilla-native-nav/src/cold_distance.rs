// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-intent prior-distance policy for loaded PathFinder calculations, never Route proof.
use crate::{
    Error, LoadedSourceWarmCache, Mesh, SourceOwnerFilter, SourceWarmCacheBudget,
    SourceWarmCacheError, SourceWarmLookup, SourceWarmPolyPhase, SourceWarmTrace, WarmCache,
};
use std::time::Instant;

/// Owner-approved absent-prior-distance intent, not undefined reference-byte parity.
pub const SOURCE_PATH_DISTANCE_INTENT_ID: &str = "cmangos-pathfinder-distance-intent-v1";
pub const SOURCE_PATH_DISTANCE_INTENT_VERSION: u32 = 1;
pub const SOURCE_PATH_DISTANCE_INTENT_REVISION: &str = "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f";

/// Opaque loaded calculation result retaining mesh identity, selected policy, and the
/// actual constructor-equivalent empty storage. A private retained continuation keeps
/// that origin while using original warm lookup semantics. It cannot become a warm seed,
/// owner receipt, point path, or Route. Public references are read-only observations.
///
/// ```compile_fail
/// use benilla_native_nav::{LoadedSourcePathDistanceIntent, WarmCache};
/// fn erase_policy(result: LoadedSourcePathDistanceIntent) -> WarmCache {
///     result.into_after()
/// }
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct LoadedSourcePathDistanceIntent {
    version: u32,
    initial: WarmCache,
    previous: Option<WarmCache>,
    observed: LoadedSourceWarmCache,
}
impl LoadedSourcePathDistanceIntent {
    /// Private loaded composition only; observations cannot recreate these handles.
    #[cfg(benilla_native_detour)]
    pub(crate) fn initial_cache(&self) -> &WarmCache {
        &self.initial
    }
    #[cfg(benilla_native_detour)]
    pub(crate) fn retained_cache(&self) -> &WarmCache {
        self.observed.after()
    }
    /// Retain the source-intent policy with the same-query composition (PathFinder.cpp:410-415).
    #[cfg(benilla_native_detour)]
    pub(crate) fn same_polygon(
        mesh: &mut Mesh,
        from: [u32; 3],
        to: [u32; 3],
        filter: SourceOwnerFilter,
        lookup_primitives: usize,
        until: Instant,
    ) -> Result<Option<Self>, SourceWarmCacheError> {
        let initial = crate::warm_cache::cold_constructor_storage(mesh, filter, until)?;
        let Some(observed) = crate::warm_cache::native::same_polygon(
            mesh,
            &initial,
            from,
            to,
            SourceWarmCacheBudget {
                lookup_primitives,
                corridor_expansions: 0,
            },
            until,
        )?
        else {
            return Ok(None);
        };
        Ok(Some(Self {
            version: SOURCE_PATH_DISTANCE_INTENT_VERSION,
            initial,
            previous: None,
            observed,
        }))
    }
    /// Continue the retained cache with defined warm predicates (PathFinder.cpp:342-349).
    #[cfg(benilla_native_detour)]
    pub(crate) fn retained_same_polygon(
        &self,
        mesh: &mut Mesh,
        from: [u32; 3],
        to: [u32; 3],
        lookup_primitives: usize,
        until: Instant,
    ) -> Result<Option<Self>, SourceWarmCacheError> {
        self.check_mesh(mesh)?;
        if self.observed.after().active_len() != 1 {
            return Err(Error::Format.into());
        }
        let Some(observed) = crate::warm_cache::native::same_polygon(
            mesh,
            self.observed.after(),
            from,
            to,
            SourceWarmCacheBudget {
                lookup_primitives,
                corridor_expansions: 0,
            },
            until,
        )?
        else {
            return Ok(None);
        };
        let result = Self {
            version: self.version,
            initial: self.initial.try_copy()?,
            previous: Some(self.observed.after().try_copy()?),
            observed,
        };
        crate::deadline(until)?;
        Ok(Some(result))
    }
    /// Whether this observation continues an actual prior terminal cache.
    pub fn is_retained(&self) -> bool {
        self.previous.is_some()
    }
    /// Complete storage before this calculation; the origin stays in initial_storage.
    pub fn before_storage(&self) -> &[u64] {
        self.previous.as_ref().unwrap_or(&self.initial).storage()
    }
    pub fn before_active_len(&self) -> usize {
        self.previous.as_ref().unwrap_or(&self.initial).active_len()
    }
    pub fn check_mesh(&self, mesh: &Mesh) -> Result<(), Error> {
        self.initial.check_mesh(mesh)?;
        self.observed.after().check_mesh(mesh)
    }
    pub fn compatibility_id(&self) -> &'static str {
        SOURCE_PATH_DISTANCE_INTENT_ID
    }
    pub fn compatibility_version(&self) -> u32 {
        self.version
    }
    pub fn source_revision(&self) -> &'static str {
        SOURCE_PATH_DISTANCE_INTENT_REVISION
    }
    pub fn initial_storage(&self) -> &[u64] {
        self.initial.storage()
    }
    pub fn initial_active_len(&self) -> usize {
        self.initial.active_len()
    }
    pub fn point_limit(&self) -> usize {
        self.initial.point_limit()
    }
    pub fn filter(&self) -> SourceOwnerFilter {
        self.initial.filter()
    }
    pub fn storage(&self) -> &[u64] {
        self.observed.after().storage()
    }
    pub fn active_len(&self) -> usize {
        self.observed.after().active_len()
    }
    pub fn from(&self) -> [u32; 3] {
        self.observed.from()
    }
    pub fn to(&self) -> [u32; 3] {
        self.observed.to()
    }
    pub fn start(&self) -> SourceWarmLookup {
        self.observed.start()
    }
    pub fn end(&self) -> SourceWarmLookup {
        self.observed.end()
    }
    pub fn phase(&self) -> SourceWarmPolyPhase {
        self.observed.phase()
    }
    pub fn direct_status(&self) -> Option<u32> {
        self.observed.direct_status()
    }
    pub fn trace(&self) -> &[SourceWarmTrace] {
        self.observed.trace()
    }
}
impl Mesh {
    /// Execute the absent-prior-distance intent with real loaded Detour queries.
    /// Fixed non-dungeon 74 slots, active=0, ground-only filter; budgets do not change that limit.
    /// Lookup uses the bounded loader's 512-node query, not a source manager allocation;
    /// direct replacement uses the existing separately bounded 1024-node calculation.
    /// No owner/filter/config admission, SetCurrentNavMesh, HaveTile, or BuildPointPath.
    pub fn loaded_source_path_distance_intent(
        &mut self,
        compatibility_version: u32,
        from: [u32; 3],
        to: [u32; 3],
        budget: SourceWarmCacheBudget,
        until: Instant,
    ) -> Result<LoadedSourcePathDistanceIntent, SourceWarmCacheError> {
        if compatibility_version != SOURCE_PATH_DISTANCE_INTENT_VERSION {
            return Err(Error::Unsupported.into());
        }
        #[cfg(benilla_native_detour)]
        {
            let initial = crate::warm_cache::cold_constructor_storage(
                self,
                SourceOwnerFilter::GROUND,
                until,
            )?;
            let observed =
                crate::warm_cache::native::query(self, &initial, from, to, budget, until)?;
            Ok(LoadedSourcePathDistanceIntent {
                version: compatibility_version,
                initial,
                previous: None,
                observed,
            })
        }
        #[cfg(not(benilla_native_detour))]
        {
            let _ = (from, to, budget, until);
            Err(Error::Unsupported.into())
        }
    }
}
