// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual loaded map-instance query allocation and query-only retention lifecycle.
//! This is a constructor dependency, not evidence that an owner dispatched or that
//! PathFinder::createFilter, calculate, or BuildPolyPath ran. No Route is minted.
use crate::{
    source_map_coord_pair, Error, LoadedSourceTilePresence, Mesh, SourceMapCoordError, TileInput,
};
use std::{cell::RefCell, rc::Rc, time::Instant};

pub const SOURCE_MAP_QUERY_POLICY: &str = "cmangos-8ec338a1-loaded-map-instance-query1024-v1";

/// Allocation policy stored by the loader and checked against the native node pool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryProfile {
    Bounded512,
    SourceMapInstance1024,
}
impl QueryProfile {
    pub const fn node_capacity(self) -> u32 {
        match self {
            Self::Bounded512 => 512,
            Self::SourceMapInstance1024 => 1024,
        }
    }
}

/// Source MMapManager key. instance_id is the actual source slot, never the local
/// Mesh allocation identity. Application configuration must establish these values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceMapQuerySlot {
    pub map_id: u32,
    pub instance_id: u32,
}

/// Owns the immutable loaded inventory and its genuine 1024-node query. There is
/// no conversion from/to the bounded Mesh, replacement, unload or hash restoration.
/// Retained dependencies share its allocation, which stays alive until the last
/// dependency is dropped. This is not a manager unload or replacement mechanism.
///
/// ```compile_fail
/// use benilla_native_nav::{Mesh, SourceMapQuery};
/// fn relabel(bounded: Mesh) -> SourceMapQuery { bounded.into() }
/// ```
pub struct SourceMapQuery {
    inner: Rc<SourceMapQueryInner>,
}
struct SourceMapQueryInner {
    mesh: RefCell<Mesh>,
    slot: SourceMapQuerySlot,
}
impl SourceMapQuery {
    /// Map loading precedes retaining the default query at the owner constructor.
    /// This creates an allocation; it does not prove a source owner construction.
    pub fn load(
        slot: SourceMapQuerySlot,
        params: &[u8],
        tiles: &[TileInput<'_>],
    ) -> Result<Self, Error> {
        let mesh = Mesh::load_profile(params, tiles, QueryProfile::SourceMapInstance1024)?;
        Ok(Self {
            inner: Rc::new(SourceMapQueryInner {
                mesh: RefCell::new(mesh),
                slot,
            }),
        })
    }
    pub fn slot(&self) -> SourceMapQuerySlot {
        self.inner.slot
    }
    pub fn query_profile(&self) -> QueryProfile {
        self.inner.mesh.borrow().query_profile()
    }
    pub fn query_capacity(&self) -> u32 {
        self.inner.mesh.borrow().query_capacity()
    }
    pub fn agent_dimensions(&self) -> [f32; 3] {
        self.inner.mesh.borrow().agent_dimensions()
    }
    #[cfg(benilla_native_detour)]
    pub(crate) fn wander_constructor_storage(
        &self,
        filter: crate::SourceOwnerFilter,
        until: Instant,
    ) -> Result<crate::WarmCache, Error> {
        let mesh = self
            .inner
            .mesh
            .try_borrow()
            .map_err(|_| Error::Unavailable)?;
        if mesh.query_profile() != QueryProfile::SourceMapInstance1024
            || mesh.query_capacity() != 1024
        {
            return Err(Error::Identity);
        }
        crate::warm_cache::cold_constructor_storage(&mesh, filter, until)
    }
    /// Retain the existing default query, initially without a current query.
    /// Enabled-at-construction, owner/generator identity and source dispatch timing
    /// must be supplied by a separate owner boundary; this lease grants none of them.
    pub fn retain_default(&self) -> RetainedSourceQuery {
        RetainedSourceQuery {
            default: Rc::clone(&self.inner),
            selected: false,
        }
    }
}
impl std::fmt::Debug for SourceMapQuery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceMapQuery")
            .field("slot", &self.slot())
            .field("query_profile", &self.query_profile())
            .finish_non_exhaustive()
    }
}

/// Query fields of the supported fixed-map/nontransport constructor dependency.
/// No Clone/Deserialize: a new lease is a new local query-only lifecycle, not an
/// existing owner's constructor or an independently restorable current query.
///
/// ```compile_fail
/// use benilla_native_nav::{RetainedSourceQuery, SourceMapQuery};
/// fn forge(query: SourceMapQuery) -> RetainedSourceQuery {
///     RetainedSourceQuery { default: query, selected: true }
/// }
/// ```
pub struct RetainedSourceQuery {
    default: Rc<SourceMapQueryInner>,
    selected: bool,
}
impl std::fmt::Debug for RetainedSourceQuery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RetainedSourceQuery")
            .field("slot", &self.slot())
            .field("current_selected", &self.selected)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceQuerySelectionError {
    Coordinates(SourceMapCoordError),
    Identity,
}
impl RetainedSourceQuery {
    pub fn current_selected(&self) -> bool {
        self.selected
    }
    pub fn slot(&self) -> SourceMapQuerySlot {
        self.default.slot
    }
    /// Actual allocation identity is checked by reference, never byte/hash equality.
    pub fn check_query(&self, query: &SourceMapQuery) -> Result<(), Error> {
        if !Rc::ptr_eq(&self.default, &query.inner) {
            return Err(Error::Identity);
        }
        Ok(())
    }
    #[cfg(benilla_native_detour)]
    pub(crate) fn retain_allocation(&self) -> SourceMapQuery {
        SourceMapQuery {
            inner: Rc::clone(&self.default),
        }
    }
    /// Actual SetCurrentNavMesh selection after clearing and updateFilter, before draws.
    pub fn select_wander_prefix(
        &mut self,
        prefix: &mut crate::LoadedSourceWanderPrefix,
        until: Instant,
    ) -> Result<(), Error> {
        crate::deadline(until)?;
        self.check_query(&prefix.query)?;
        if prefix.selected {
            return Err(Error::Identity);
        }
        self.selected = true;
        prefix.selected = true;
        Ok(())
    }
    /// Reborrow the selected allocation after samples; no second selection occurs.
    pub fn continue_wander_prefix(
        &self,
        prefix: &crate::LoadedSourceWanderPrefix,
        from: [u32; 3],
        raw_end: [u32; 3],
    ) -> Result<SelectedSourceQuery<'_>, Error> {
        self.check_query(&prefix.query)?;
        if !self.selected || !prefix.selected {
            return Err(Error::Identity);
        }
        Ok(SelectedSourceQuery {
            query: &self.default,
            from,
            to: raw_end,
        })
    }
    /// ComputePathToRandomPoint selects after updateFilter, before its samples.
    /// Unlike calculate, this does not claim IsValidMapCoord calls. The supplied
    /// endpoints bind later observations; their numeric domain is checked there.
    #[cfg(test)]
    pub fn select_for_wander(
        &mut self,
        slot: SourceMapQuerySlot,
        from: [u32; 3],
        raw_end: [u32; 3],
    ) -> Result<SelectedSourceQuery<'_>, SourceQuerySelectionError> {
        if slot != self.default.slot {
            return Err(SourceQuerySelectionError::Identity);
        }
        self.selected = true;
        Ok(SelectedSourceQuery {
            query: &self.default,
            from,
            to: raw_end,
        })
    }
    /// Only the query selection prefix: destination/start checks precede selection.
    /// A valid coordinate outside later geometry budgets still selects the query;
    /// tile lookup may then fail. No tile lookup, filter or polygon state is implied.
    pub fn select_for_calculate(
        &mut self,
        slot: SourceMapQuerySlot,
        from: [u32; 3],
        to: [u32; 3],
    ) -> Result<SelectedSourceQuery<'_>, SourceQuerySelectionError> {
        source_map_coord_pair(from, to).map_err(SourceQuerySelectionError::Coordinates)?;
        if slot != self.default.slot {
            return Err(SourceQuerySelectionError::Identity);
        }
        self.selected = true;
        Ok(SelectedSourceQuery {
            query: &self.default,
            from,
            to,
        })
    }
}

/// Live selected default query and original calculate endpoints. The borrowed
/// allocation cannot be replaced by identical bytes or a different source slot.
pub struct SelectedSourceQuery<'a> {
    query: &'a Rc<SourceMapQueryInner>,
    from: [u32; 3],
    to: [u32; 3],
}
impl std::fmt::Debug for SelectedSourceQuery<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SelectedSourceQuery")
            .field("slot", &self.slot())
            .field("from", &self.from)
            .field("to", &self.to)
            .finish_non_exhaustive()
    }
}
impl SelectedSourceQuery<'_> {
    #[cfg(benilla_native_detour)]
    pub(crate) fn retain_allocation(&self) -> SourceMapQuery {
        SourceMapQuery {
            inner: Rc::clone(self.query),
        }
    }
    pub(crate) fn check_allocation(&self, query: &SourceMapQuery) -> Result<(), Error> {
        if !Rc::ptr_eq(self.query, &query.inner) {
            return Err(Error::Identity);
        }
        Ok(())
    }
    #[cfg(benilla_native_detour)]
    pub(crate) fn borrow_mesh(&self) -> Result<std::cell::RefMut<'_, Mesh>, Error> {
        self.query
            .mesh
            .try_borrow_mut()
            .map_err(|_| Error::Unavailable)
    }
    pub fn slot(&self) -> SourceMapQuerySlot {
        self.query.slot
    }
    pub fn query_profile(&self) -> QueryProfile {
        self.query.mesh.borrow().query_profile()
    }
    pub fn query_capacity(&self) -> u32 {
        self.query.mesh.borrow().query_capacity()
    }
    pub fn from(&self) -> [u32; 3] {
        self.from
    }
    pub fn to(&self) -> [u32; 3] {
        self.to
    }
    /// Bounded HaveTile only, using the same allocation retained before selection.
    pub fn tile_presence(&self, until: Instant) -> Result<LoadedSourceTilePresence, Error> {
        self.query
            .mesh
            .try_borrow()
            .map_err(|_| Error::Unavailable)?
            .source_tile_presence_pair(self.from, self.to, until)
    }
    pub fn check_tile_presence(&self, presence: &LoadedSourceTilePresence) -> Result<(), Error> {
        if presence.from() != self.from || presence.to() != self.to {
            return Err(Error::Identity);
        }
        let mesh = self
            .query
            .mesh
            .try_borrow()
            .map_err(|_| Error::Unavailable)?;
        presence.check_mesh(&mesh)
    }
}

#[cfg(all(test, benilla_native_detour))]
#[path = "source_query_tests.rs"]
mod tests;
