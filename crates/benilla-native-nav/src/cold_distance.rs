// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit BuildPolyPath zero-distance compatibility dependency, never owner/Route proof.
use crate::{
    Error, LoadedSourceWarmCache, Mesh, SourceOwnerFilter, SourceWarmCacheBudget,
    SourceWarmCacheError, SourceWarmLookup, SourceWarmPolyPhase, SourceWarmTrace, WarmCache,
};
use std::time::Instant;

/// Deliberate deviation from the undefined original cold read, not historical parity.
pub const SOURCE_COLD_DISTANCE_COMPATIBILITY_ID: &str = "cmangos-buildpolypath-zero-distance-v1";
pub const SOURCE_COLD_DISTANCE_COMPATIBILITY_VERSION: u32 = 1;
pub const SOURCE_COLD_DISTANCE_COMPATIBILITY_REVISION: &str =
    "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f";

/// Opaque loaded calculation result retaining mesh identity, selected policy, and the
/// actual constructor-equivalent empty storage. A private retained continuation keeps
/// that origin while using original warm lookup semantics. It cannot become a warm seed,
/// owner receipt, point path, or Route. Public references are read-only observations.
///
/// ```compile_fail
/// use benilla_native_nav::{LoadedSourceColdDistanceCompatibility, WarmCache};
/// fn erase_policy(result: LoadedSourceColdDistanceCompatibility) -> WarmCache {
///     result.into_after()
/// }
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct LoadedSourceColdDistanceCompatibility {
    version: u32,
    initial: WarmCache,
    previous: Option<WarmCache>,
    observed: LoadedSourceWarmCache,
}
impl LoadedSourceColdDistanceCompatibility {
    /// Private same-query composition keeps the selected compatibility policy attached.
    /// No direct replacement is invoked or public original-warm handle returned.
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
        let Some(observed) = crate::warm_cache::native::cold_same_polygon(
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
            version: SOURCE_COLD_DISTANCE_COMPATIBILITY_VERSION,
            initial,
            previous: None,
            observed,
        }))
    }
    /// Continue the real retained cache under original, defined warm lookup semantics.
    /// The initial compatibility policy remains attached; no new repair is selected.
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
        let Some(observed) = crate::warm_cache::native::retained_same_polygon(
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
        SOURCE_COLD_DISTANCE_COMPATIBILITY_ID
    }
    pub fn compatibility_version(&self) -> u32 {
        self.version
    }
    pub fn source_revision(&self) -> &'static str {
        SOURCE_COLD_DISTANCE_COMPATIBILITY_REVISION
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
    /// Execute the selected +0.0f-local repair with real loaded Detour queries. Fixed
    /// non-dungeon 74 slots, active=0, ground-only filter; budgets do not change that limit.
    /// Lookup uses the bounded loader's 512-node query, not a source manager allocation;
    /// direct replacement uses the existing separately bounded 1024-node calculation.
    /// No owner/filter/config admission, SetCurrentNavMesh, HaveTile, or BuildPointPath.
    pub fn loaded_source_cold_distance_compatibility(
        &mut self,
        compatibility_version: u32,
        from: [u32; 3],
        to: [u32; 3],
        budget: SourceWarmCacheBudget,
        until: Instant,
    ) -> Result<LoadedSourceColdDistanceCompatibility, SourceWarmCacheError> {
        if compatibility_version != SOURCE_COLD_DISTANCE_COMPATIBILITY_VERSION {
            return Err(Error::Unsupported.into());
        }
        #[cfg(benilla_native_detour)]
        {
            let initial = crate::warm_cache::cold_constructor_storage(
                self,
                SourceOwnerFilter::GROUND,
                until,
            )?;
            let observed = crate::warm_cache::native::query_with_initialization(
                self,
                &initial,
                from,
                to,
                budget,
                until,
                crate::warm_cache::DistanceInitialization::CompatibilityZeroV1,
            )?;
            Ok(LoadedSourceColdDistanceCompatibility {
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
