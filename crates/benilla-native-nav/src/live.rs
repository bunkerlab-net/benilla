// SPDX-License-Identifier: GPL-3.0-or-later
//! Mutable MoveMap mesh. No sliced search, route, or PathFinder filter is constructed.
use crate::{validate, Error, TileInput};
use std::{collections::BTreeMap, ffi::c_void, ptr::NonNull, rc::Rc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveError {
    NumericBinding,
    OffMeshNumericBinding,
    Validation(Error),
    Allocation,
    StaleQuery,
    StaleTile,
    Native(u32),
}
impl From<Error> for LiveError {
    fn from(value: Error) -> Self {
        Self::Validation(value)
    }
}

/// Identity is a live allocation, not a saved counter or content hash.
#[derive(Clone, Debug)]
pub struct QueryHandle {
    mesh: Rc<()>,
    query: Rc<()>,
    instance: u32,
}
impl QueryHandle {
    pub fn same_mesh(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.mesh, &other.mesh)
    }
    pub fn same_query(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.query, &other.query)
    }
    pub fn instance(&self) -> u32 {
        self.instance
    }
}
struct Query {
    raw: NonNull<c_void>,
    identity: Rc<()>,
}
impl Drop for Query {
    fn drop(&mut self) {
        // SAFETY: exactly one owner, destroyed before its attached mesh.
        unsafe { ffi::benilla_live_query_free(self.raw.as_ptr()) }
    }
}
struct Tile {
    reference: u64,
    validation: validate::LiveTileValidation,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotState {
    pub salt: u32,
    pub occupied: bool,
    /// Hash chain for an occupied slot, free-list chain otherwise.
    pub next_slot: Option<u32>,
}

pub struct MapMesh {
    // Fields drop in declaration order; queries must precede the mesh allocation.
    queries: BTreeMap<u32, Query>,
    raw: NonNull<c_void>,
    identity: Rc<()>,
    params: [u8; 28],
    tiles: BTreeMap<[u8; 2], Tile>,
}
impl MapMesh {
    pub fn new(params: &[u8]) -> Result<Self, LiveError> {
        numeric()?;
        validate::live_params(params)?;
        // SAFETY: validated 28-byte ABI, copied into aligned native storage.
        let raw = NonNull::new(unsafe { ffi::benilla_live_create(params.as_ptr()) })
            .ok_or(LiveError::Allocation)?;
        Ok(Self {
            queries: BTreeMap::new(),
            raw,
            identity: Rc::new(()),
            params: params.try_into().unwrap(),
            tiles: BTreeMap::new(),
        })
    }
    /// MoveMap.cpp:356-379, a repeated instance does not allocate again.
    pub fn load_instance(&mut self, instance: u32) -> Result<(), LiveError> {
        if self.queries.contains_key(&instance) {
            return Ok(());
        }
        // SAFETY: mesh is live and the query never escapes this owner.
        let raw = NonNull::new(unsafe { ffi::benilla_live_query(self.raw.as_ptr()) })
            .ok_or(LiveError::Allocation)?;
        let query = Query {
            raw,
            identity: Rc::new(()),
        };
        // SAFETY: newly allocated query is attached to this still-live mesh.
        if unsafe { ffi::benilla_live_query_capacity(self.raw.as_ptr(), raw.as_ptr()) }
            != crate::QueryProfile::SourceMapInstance1024.node_capacity()
        {
            return Err(LiveError::Validation(Error::Identity));
        }
        self.queries.insert(instance, query);
        Ok(())
    }
    pub fn unload_instance(&mut self, instance: u32) -> bool {
        self.queries.remove(&instance).is_some()
    }
    /// MoveMap.cpp:499-509 is lookup only, never lazy allocation.
    pub fn query(&self, instance: u32) -> Option<QueryHandle> {
        self.queries.get(&instance).map(|query| QueryHandle {
            mesh: self.identity.clone(),
            query: query.identity.clone(),
            instance,
        })
    }
    pub fn query_capacity(&self, handle: &QueryHandle) -> Result<u32, LiveError> {
        let query = self
            .queries
            .get(&handle.instance)
            .ok_or(LiveError::StaleQuery)?;
        if !Rc::ptr_eq(&self.identity, &handle.mesh) || !Rc::ptr_eq(&query.identity, &handle.query)
        {
            return Err(LiveError::StaleQuery);
        }
        // SAFETY: both allocations remain owned and the handle was authenticated above.
        Ok(unsafe { ffi::benilla_live_query_capacity(self.raw.as_ptr(), query.raw.as_ptr()) })
    }
    /// DT64 references encode salt:12, slot:21, polygon:31, DetourNavMesh.h:43-45.
    pub fn tile_reference(&self, grid: [u8; 2]) -> Option<u64> {
        self.tiles.get(&grid).map(|tile| tile.reference)
    }
    pub fn tiles(&self) -> impl Iterator<Item = ([u8; 2], u64)> + '_ {
        self.tiles
            .iter()
            .map(|(&grid, tile)| (grid, tile.reference))
    }
    pub fn instances(&self) -> impl Iterator<Item = u32> + '_ {
        self.queries.keys().copied()
    }
    pub fn slots(&self) -> impl Iterator<Item = SlotState> + '_ {
        let count = u32::from_le_bytes(self.params[20..24].try_into().unwrap());
        (0..count).map(|index| {
            let mut words = [0; 3];
            // SAFETY: index is bounded by the immutable validated maxTiles.
            unsafe { ffi::benilla_live_slot(self.raw.as_ptr(), index, words.as_mut_ptr()) };
            SlotState {
                salt: words[0],
                occupied: words[1] != 0,
                next_slot: (words[2] != u32::MAX).then_some(words[2]),
            }
        })
    }
    /// Source duplicate/full-slot failures leave the free list, salts and links unchanged.
    pub fn add_tile(&mut self, input: TileInput<'_>) -> Result<u64, LiveError> {
        numeric()?;
        if self.tiles.contains_key(&input.grid) {
            return Err(LiveError::Native((1 << 31) | (1 << 7)));
        }
        let bytes = input.bytes;
        if bytes.len() >= 28 {
            let word = |i| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
            if word(20) != 0x444e4156 {
                return Err(LiveError::Native(0x8000_0001));
            }
            if word(24) != 7 {
                return Err(LiveError::Native(0x8000_0002));
            }
            if bytes.len() >= 76 && word(72) != 0 {
                // DetourNavMesh.cpp:561-610 reaches closestPointOnPoly and sqrt.
                return Err(LiveError::OffMeshNumericBinding);
            }
        }
        let grid = input.grid;
        let validation = validate::live_tile(&self.params, input)?;
        let xy = validation.coordinates();
        validation.connecting(self.tiles.values().filter_map(|tile| {
            let other = tile.validation.coordinates();
            ((other[0] - xy[0]).abs() <= 1 && (other[1] - xy[1]).abs() <= 1)
                .then_some(&tile.validation)
        }))?;
        let mut reference = 0;
        // SAFETY: complete existing tile validation plus all new cross-tile checks.
        // No offmesh polygon is admitted. Portal roundf has finite inputs in [0,255].
        let status = unsafe {
            ffi::benilla_live_add(
                self.raw.as_ptr(),
                bytes[20..].as_ptr(),
                (bytes.len() - 20) as i32,
                &mut reference,
            )
        };
        native(status)?;
        self.tiles.insert(
            grid,
            Tile {
                reference,
                validation,
            },
        );
        Ok(reference)
    }
    pub fn remove_tile(&mut self, grid: [u8; 2]) -> Result<bool, LiveError> {
        let Some(tile) = self.tiles.get(&grid) else {
            return Ok(false);
        };
        // SAFETY: source ref comes only from this live mesh's successful addTile.
        native(unsafe { ffi::benilla_live_remove(self.raw.as_ptr(), tile.reference) })?;
        self.tiles.remove(&grid);
        Ok(true)
    }
    /// Read the actual source link-list order. The caller owns output storage.
    pub fn links(&self, reference: u64, polygon: u32, out: &mut [u64]) -> Result<usize, LiveError> {
        if polygon >= 1 << 31 || !self.tiles.values().any(|tile| tile.reference == reference) {
            return Err(LiveError::StaleTile);
        }
        let capacity =
            u32::try_from(out.len()).map_err(|_| LiveError::Validation(Error::Budget))?;
        // SAFETY: installed ref, checked output capacity; native validates the polygon.
        let count = unsafe {
            ffi::benilla_live_links(
                self.raw.as_ptr(),
                reference,
                polygon,
                out.as_mut_ptr(),
                capacity,
            )
        };
        if count == u32::MAX {
            return Err(LiveError::StaleTile);
        }
        Ok(count as usize)
    }
}
impl Drop for MapMesh {
    fn drop(&mut self) {
        self.queries.clear();
        // SAFETY: queries are gone; all aligned tile buffers belong to this allocation.
        unsafe { ffi::benilla_live_free(self.raw.as_ptr()) }
    }
}
fn numeric() -> Result<(), LiveError> {
    // SAFETY: read-only FP control observation. Never changes the caller's environment.
    if unsafe { ffi::benilla_live_numeric() } == 1 {
        Ok(())
    } else {
        Err(LiveError::NumericBinding)
    }
}
fn native(status: u32) -> Result<(), LiveError> {
    if status & (1 << 31) != 0 {
        Err(LiveError::Native(status))
    } else {
        Ok(())
    }
}
#[cfg(benilla_native_detour)]
mod ffi {
    use std::ffi::c_void;
    unsafe extern "C" {
        pub fn benilla_live_numeric() -> u32;
        pub fn benilla_live_create(params: *const u8) -> *mut c_void;
        pub fn benilla_live_free(raw: *mut c_void);
        pub fn benilla_live_query(raw: *mut c_void) -> *mut c_void;
        pub fn benilla_live_query_free(raw: *mut c_void);
        pub fn benilla_live_query_capacity(mesh: *const c_void, query: *const c_void) -> u32;
        pub fn benilla_live_add(
            raw: *mut c_void,
            bytes: *const u8,
            size: i32,
            reference: *mut u64,
        ) -> u32;
        pub fn benilla_live_remove(raw: *mut c_void, reference: u64) -> u32;
        pub fn benilla_live_links(
            raw: *const c_void,
            reference: u64,
            polygon: u32,
            out: *mut u64,
            capacity: u32,
        ) -> u32;
        pub fn benilla_live_slot(raw: *const c_void, index: u32, out: *mut u32);
    }
}
#[cfg(not(benilla_native_detour))]
mod ffi {
    use std::ffi::c_void;
    pub unsafe fn benilla_live_numeric() -> u32 {
        0
    }
    pub unsafe fn benilla_live_create(_: *const u8) -> *mut c_void {
        std::ptr::null_mut()
    }
    pub unsafe fn benilla_live_free(_: *mut c_void) {}
    pub unsafe fn benilla_live_query(_: *mut c_void) -> *mut c_void {
        std::ptr::null_mut()
    }
    pub unsafe fn benilla_live_query_free(_: *mut c_void) {}
    pub unsafe fn benilla_live_query_capacity(_: *const c_void, _: *const c_void) -> u32 {
        0
    }
    pub unsafe fn benilla_live_add(_: *mut c_void, _: *const u8, _: i32, _: *mut u64) -> u32 {
        1 << 31
    }
    pub unsafe fn benilla_live_remove(_: *mut c_void, _: u64) -> u32 {
        1 << 31
    }
    pub unsafe fn benilla_live_links(_: *const c_void, _: u64, _: u32, _: *mut u64, _: u32) -> u32 {
        u32::MAX
    }
    pub unsafe fn benilla_live_slot(_: *const c_void, _: u32, _: *mut u32) {}
}

#[cfg(all(test, benilla_native_detour))]
#[path = "live_tests.rs"]
mod tests;
