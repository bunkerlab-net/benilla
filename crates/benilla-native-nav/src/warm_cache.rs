// SPDX-License-Identifier: GPL-3.0-or-later
//! Immutable loaded warm polygon-cache proposals, before BuildPointPath and owner admission.
//! A seed is a bounded loaded dependency, never a source initial-owner cache producer.
use crate::{deadline, Error, Mesh, SourceOwnerFilter};
use std::time::Instant;

pub const SOURCE_WARM_CACHE_POLICY: u32 = 1;
pub const SOURCE_WARM_CACHE_POINT_LIMIT: usize = 74;
pub const SOURCE_WARM_CACHE_MAX_STORAGE: usize = 4096;
pub const SOURCE_WARM_CACHE_MAX_LOOKUP_PRIMITIVES: usize = 16388;
pub const SOURCE_WARM_CACHE_MAX_CORRIDOR_EXPANSIONS: usize = 4096;

/// Opaque authority for one immutable loaded mesh instance. It cannot be constructed from
/// public refs, hashes, snapshots or serialized state. Read-only refs confer no authority.
/// The complete retained storage includes inactive slots and the searchable tail.
///
/// ```compile_fail
/// use benilla_native_nav::{SourceOwnerFilter, WarmCache};
/// let forged = WarmCache { mesh_id: 1, storage: vec![42; 74], active_len: 1,
///     filter: SourceOwnerFilter { include_flags: 1, exclude_flags: 0 } };
/// ```
///
/// ```compile_fail
/// use benilla_native_nav::{Mesh, WarmCache};
/// fn restore(mesh: &Mesh, serialized_refs: &[u64]) {
///     let _ = WarmCache::authored_fixture(mesh, serialized_refs, 1);
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WarmCache {
    mesh_id: u64,
    filter: SourceOwnerFilter,
    storage: Vec<u64>,
    active_len: usize,
}
impl WarmCache {
    pub fn storage(&self) -> &[u64] {
        &self.storage
    }
    pub fn active_len(&self) -> usize {
        self.active_len
    }
    pub fn point_limit(&self) -> usize {
        SOURCE_WARM_CACHE_POINT_LIMIT
    }
    pub fn filter(&self) -> SourceOwnerFilter {
        self.filter
    }

    #[cfg(all(test, benilla_native_detour))]
    pub(crate) fn authored_fixture(
        mesh: &Mesh,
        storage: &[u64],
        active_len: usize,
    ) -> Result<Self, Error> {
        if !(SOURCE_WARM_CACHE_POINT_LIMIT..=SOURCE_WARM_CACHE_MAX_STORAGE).contains(&storage.len())
            || active_len > SOURCE_WARM_CACHE_POINT_LIMIT
        {
            return Err(Error::Format);
        }
        for &reference in storage {
            if reference != 0 && !native::valid_reference(mesh, reference) {
                return Err(Error::Identity);
            }
        }
        Ok(Self {
            mesh_id: mesh.instance_id,
            filter: SourceOwnerFilter {
                include_flags: 1,
                exclude_flags: 0,
            },
            storage: copy_storage(storage)?,
            active_len,
        })
    }
}
#[cfg(benilla_native_detour)]
fn copy_storage(storage: &[u64]) -> Result<Vec<u64>, Error> {
    let mut copied = Vec::new();
    copied
        .try_reserve_exact(storage.len())
        .map_err(|_| Error::Budget)?;
    copied.extend_from_slice(storage);
    Ok(copied)
}
/// Only LoadedSourceCorridor calls this with its private original polygon storage.
pub(crate) fn seed(
    mesh: &Mesh,
    polygons: &[u64],
    filter: SourceOwnerFilter,
    point_limit: usize,
    until: Instant,
) -> Result<WarmCache, Error> {
    deadline(until)?;
    if point_limit != SOURCE_WARM_CACHE_POINT_LIMIT
        || filter
            != (SourceOwnerFilter {
                include_flags: 1,
                exclude_flags: 0,
            })
    {
        return Err(Error::Unsupported);
    }
    if polygons.is_empty() || polygons.len() > point_limit {
        return Err(Error::Format);
    }
    let mut storage = Vec::new();
    storage
        .try_reserve_exact(point_limit)
        .map_err(|_| Error::Budget)?;
    storage.extend_from_slice(polygons);
    storage.resize(point_limit, 0);
    deadline(until)?;
    Ok(WarmCache {
        mesh_id: mesh.instance_id,
        filter,
        storage,
        active_len: polygons.len(),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceWarmCacheBudget {
    pub lookup_primitives: usize,
    pub corridor_expansions: usize,
}
impl Default for SourceWarmCacheBudget {
    fn default() -> Self {
        Self {
            lookup_primitives: SOURCE_WARM_CACHE_MAX_LOOKUP_PRIMITIVES,
            corridor_expansions: SOURCE_WARM_CACHE_MAX_CORRIDOR_EXPANSIONS,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceWarmCacheError {
    Boundary(Error),
    /// Neither a nonempty active scan nor an eligible tail can initialize source distance.
    ColdDistanceUndefined,
    MissingPolygon,
    /// Source needs owner water/flying state and possible end projection above seven units.
    FarFromPolygonUnsupported,
    /// Raw direct partial/truncated/out-of-nodes domain is explicitly excluded, not repaired.
    UnsupportedDirectStatus(u32),
}
impl From<Error> for SourceWarmCacheError {
    fn from(value: Error) -> Self {
        Self::Boundary(value)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceWarmLookupOrigin {
    Active,
    Tail,
    Nearest5,
    Nearest10,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceWarmLookup {
    pub polygon: u64,
    pub distance_bits: u32,
    pub origin: SourceWarmLookupOrigin,
    pub appended: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceWarmPolyPhase {
    SamePolygon,
    Cut,
    Replacement,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceWarmPrimitive {
    Active,
    Tail,
    Nearest5,
    Nearest10,
    CutScan,
    DirectCreate,
    DirectExpansion,
    DirectFinish,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceWarmTrace {
    /// Zero is start lookup, one is end lookup, two is polygon transition.
    pub endpoint: u8,
    pub primitive: SourceWarmPrimitive,
    pub polygon: u64,
    pub status: u32,
    /// Running sqrt(minimum) for scans, actual distance for nearest hits, otherwise zero.
    pub distance_bits: u32,
}
/// Successful polygon-phase dependency only. Its after handle is a proposal; neither this
/// result nor its refs are a Route fact, owner path_type or a completed BuildPointPath.
#[derive(Debug, PartialEq, Eq)]
pub struct LoadedSourceWarmCache {
    from: [u32; 3],
    to: [u32; 3],
    after: WarmCache,
    start: SourceWarmLookup,
    end: SourceWarmLookup,
    phase: SourceWarmPolyPhase,
    direct_status: Option<u32>,
    trace: Vec<SourceWarmTrace>,
}
impl LoadedSourceWarmCache {
    pub fn from(&self) -> [u32; 3] {
        self.from
    }
    pub fn to(&self) -> [u32; 3] {
        self.to
    }
    pub fn after(&self) -> &WarmCache {
        &self.after
    }
    pub fn into_after(self) -> WarmCache {
        self.after
    }
    pub fn start(&self) -> SourceWarmLookup {
        self.start
    }
    pub fn end(&self) -> SourceWarmLookup {
        self.end
    }
    pub fn phase(&self) -> SourceWarmPolyPhase {
        self.phase
    }
    pub fn direct_status(&self) -> Option<u32> {
        self.direct_status
    }
    pub fn trace(&self) -> &[SourceWarmTrace] {
        &self.trace
    }
}
impl Mesh {
    /// Run source ordered active/tail/nearest lookups sequentially on a private candidate,
    /// then source same/cut or selected-reference direct replacement. Retained inactive slots
    /// and tail survive. The prior handle is unchanged on every success or rejection.
    /// Fixed non-dungeon limit 74; no cold-owner repair, smoothing or owner classification.
    pub fn loaded_source_warm_cache(
        &mut self,
        before: &WarmCache,
        from: [u32; 3],
        to: [u32; 3],
        budget: SourceWarmCacheBudget,
        until: Instant,
    ) -> Result<LoadedSourceWarmCache, SourceWarmCacheError> {
        #[cfg(benilla_native_detour)]
        {
            native::query(self, before, from, to, budget, until)
        }
        #[cfg(not(benilla_native_detour))]
        {
            let _ = (before, from, to, budget, until);
            Err(Error::Unsupported.into())
        }
    }
}

#[cfg(benilla_native_detour)]
mod native {
    use super::*;
    use crate::{detour, validate, PROGRESS, SUCCESS};
    use std::{ffi::c_void, ptr::NonNull};
    #[repr(C)]
    #[derive(Default)]
    struct Scan {
        reference: u64,
        minimum_squared: f32,
        distance: f32,
    }
    unsafe extern "C" {
        fn benilla_nav_warm_scan_reset(scan: *mut Scan);
        fn benilla_nav_warm_scan(
            raw: *mut c_void,
            reference: u64,
            point: *const f32,
            scan: *mut Scan,
        ) -> u32;
        fn benilla_nav_warm_nearest(
            raw: *mut c_void,
            point: *const f32,
            extent: f32,
            include: u16,
            exclude: u16,
            reference: *mut u64,
            distance: *mut f32,
        ) -> u32;
        #[cfg(test)]
        fn benilla_nav_warm_valid_ref(raw: *mut c_void, reference: u64) -> u8;
        fn benilla_nav_source_direct_create(
            raw: *mut c_void,
            a: u64,
            b: u64,
            start: *const f32,
            end: *const f32,
            include: u16,
            exclude: u16,
            status: *mut u32,
        ) -> *mut c_void;
        fn benilla_source_direct_step(state: *mut c_void, done: *mut i32) -> u32;
        fn benilla_source_direct_finish(
            state: *mut c_void,
            path: *mut u64,
            count: *mut i32,
            limit: i32,
        ) -> u32;
        fn benilla_source_direct_free(state: *mut c_void);
    }
    #[cfg(test)]
    pub(super) fn valid_reference(mesh: &Mesh, reference: u64) -> bool {
        // SAFETY: read-only validation against the live immutable loaded mesh.
        unsafe { benilla_nav_warm_valid_ref(mesh.raw.as_ptr(), reference) == 1 }
    }
    struct Work {
        budget: SourceWarmCacheBudget,
        until: Instant,
        lookups: usize,
        expansions: usize,
        trace: Vec<SourceWarmTrace>,
    }
    impl Work {
        fn lookup(&mut self) -> Result<(), Error> {
            deadline(self.until)?;
            if self.lookups >= self.budget.lookup_primitives {
                return Err(Error::Budget);
            }
            self.lookups += 1;
            Ok(())
        }
        fn expansion(&mut self) -> Result<(), Error> {
            deadline(self.until)?;
            if self.expansions >= self.budget.corridor_expansions {
                return Err(Error::Budget);
            }
            self.expansions += 1;
            Ok(())
        }
        fn record(
            &mut self,
            endpoint: u8,
            primitive: SourceWarmPrimitive,
            polygon: u64,
            status: u32,
            distance_bits: u32,
        ) -> Result<(), Error> {
            deadline(self.until)?;
            self.trace.try_reserve(1).map_err(|_| Error::Budget)?;
            self.trace.push(SourceWarmTrace {
                endpoint,
                primitive,
                polygon,
                status,
                distance_bits,
            });
            Ok(())
        }
    }
    fn scan(
        mesh: &Mesh,
        refs: &[u64],
        point: &[f32; 3],
        endpoint: u8,
        primitive: SourceWarmPrimitive,
        work: &mut Work,
    ) -> Result<Scan, Error> {
        debug_assert!(!refs.is_empty());
        let mut scan = Scan::default();
        deadline(work.until)?;
        // SAFETY: initialized C-layout scan, no pointers retained by the native primitive.
        unsafe { benilla_nav_warm_scan_reset(&mut scan) };
        for &reference in refs {
            work.lookup()?;
            // SAFETY: refs originated in this loaded mesh; zero refs have defined source failure.
            // Point is validated finite; exactly one closestPointOnPoly call per budget check.
            let status = unsafe {
                benilla_nav_warm_scan(mesh.raw.as_ptr(), reference, point.as_ptr(), &mut scan)
            };
            work.record(
                endpoint,
                primitive,
                reference,
                status,
                scan.distance.to_bits(),
            )?;
            if scan.minimum_squared < 1.0 {
                break;
            }
        }
        deadline(work.until)?;
        Ok(scan)
    }
    fn nearest_bounds(mesh: &Mesh, point: &[f32; 3], extent: f32) -> Result<(), Error> {
        let mut cells = 1usize;
        for axis in [0, 2] {
            let low = ((point[axis] - extent - mesh.origin[axis]) / validate::GRID).floor();
            let high = ((point[axis] + extent - mesh.origin[axis]) / validate::GRID).floor();
            if !(-1024. ..=1024.).contains(&low) || !(-1024. ..=1024.).contains(&high) || high < low
            {
                return Err(Error::Budget);
            }
            cells = cells
                .checked_mul((high - low) as usize + 1)
                .ok_or(Error::Budget)?;
        }
        if cells > 16 {
            return Err(Error::Budget);
        }
        Ok(())
    }
    fn lookup(
        mesh: &Mesh,
        candidate: &mut WarmCache,
        point: &[f32; 3],
        endpoint: u8,
        work: &mut Work,
    ) -> Result<SourceWarmLookup, SourceWarmCacheError> {
        let mut distance = None;
        if candidate.active_len != 0 {
            let found = scan(
                mesh,
                &candidate.storage[..candidate.active_len],
                point,
                endpoint,
                SourceWarmPrimitive::Active,
                work,
            )?;
            distance = Some(found.distance);
            if found.minimum_squared <= 3.0 && found.reference != 0 {
                return Ok(SourceWarmLookup {
                    polygon: found.reference,
                    distance_bits: found.distance.to_bits(),
                    origin: SourceWarmLookupOrigin::Active,
                    appended: false,
                });
            }
        }
        // Source starts the tail at point_limit, never at active_len. This scan replaces,
        // rather than minimizes with, the active scan's comparison distance.
        if candidate.storage.len() > candidate.active_len.max(SOURCE_WARM_CACHE_POINT_LIMIT) {
            let found = scan(
                mesh,
                &candidate.storage[SOURCE_WARM_CACHE_POINT_LIMIT..],
                point,
                endpoint,
                SourceWarmPrimitive::Tail,
                work,
            )?;
            distance = Some(found.distance);
            if found.minimum_squared <= 7.0 && found.reference != 0 {
                return Ok(SourceWarmLookup {
                    polygon: found.reference,
                    distance_bits: found.distance.to_bits(),
                    origin: SourceWarmLookupOrigin::Tail,
                    appended: false,
                });
            }
        }
        // BuildPolyPath's distances have no initializer. Reject before a successful nearest
        // could read that unwritten value; no invented zero initializer or smaller limit.
        let distance = distance.ok_or(SourceWarmCacheError::ColdDistanceUndefined)?;
        for (extent, primitive, origin) in [
            (
                5.0,
                SourceWarmPrimitive::Nearest5,
                SourceWarmLookupOrigin::Nearest5,
            ),
            (
                10.0,
                SourceWarmPrimitive::Nearest10,
                SourceWarmLookupOrigin::Nearest10,
            ),
        ] {
            nearest_bounds(mesh, point, extent)?;
            work.lookup()?;
            let mut polygon = 0;
            let mut new_distance = 0.0;
            // SAFETY: bounded finite query rectangle, live mesh and initialized outputs.
            let status = unsafe {
                benilla_nav_warm_nearest(
                    mesh.raw.as_ptr(),
                    point.as_ptr(),
                    extent,
                    candidate.filter.include_flags,
                    candidate.filter.exclude_flags,
                    &mut polygon,
                    &mut new_distance,
                )
            };
            work.record(endpoint, primitive, polygon, status, new_distance.to_bits())?;
            if status & SUCCESS != 0 && polygon != 0 {
                if !new_distance.is_finite() || new_distance < 0.0 {
                    return Err(Error::Format.into());
                }
                // Source's zero disjunct is retained, though finite warm misses cannot reach it.
                let appended = distance == 0.0 || new_distance < distance;
                if appended {
                    if candidate.storage.len() >= SOURCE_WARM_CACHE_MAX_STORAGE {
                        return Err(Error::Budget.into());
                    }
                    candidate
                        .storage
                        .try_reserve(1)
                        .map_err(|_| Error::Budget)?;
                    candidate.storage.push(polygon);
                }
                return Ok(SourceWarmLookup {
                    polygon,
                    distance_bits: new_distance.to_bits(),
                    origin,
                    appended,
                });
            }
        }
        Err(SourceWarmCacheError::MissingPolygon)
    }
    struct DirectState(NonNull<c_void>);
    impl Drop for DirectState {
        fn drop(&mut self) {
            // SAFETY: sole owner; direct state is freed before the mutable mesh borrow ends.
            unsafe { benilla_source_direct_free(self.0.as_ptr()) }
        }
    }
    fn replace(
        mesh: &Mesh,
        candidate: &mut WarmCache,
        start: SourceWarmLookup,
        end: SourceWarmLookup,
        a: &[f32; 3],
        b: &[f32; 3],
        work: &mut Work,
    ) -> Result<u32, SourceWarmCacheError> {
        candidate.active_len = 0; // Source clear retains backing storage, including appended tail.
        work.lookup()?;
        let mut status = 0;
        // SAFETY: selected refs come from the same mesh. This intentionally does not redo
        // nearest lookup; original positions and copied filter feed the private 1024-node loop.
        let raw = unsafe {
            benilla_nav_source_direct_create(
                mesh.raw.as_ptr(),
                start.polygon,
                end.polygon,
                a.as_ptr(),
                b.as_ptr(),
                candidate.filter.include_flags,
                candidate.filter.exclude_flags,
                &mut status,
            )
        };
        let state = NonNull::new(raw).map(DirectState);
        // Own the state before any fallible work, including deadline checks or trace allocation.
        work.record(
            2,
            SourceWarmPrimitive::DirectCreate,
            start.polygon,
            status,
            0,
        )?;
        let state = state.ok_or(Error::Native(status))?;
        while status == PROGRESS {
            work.expansion()?;
            let mut done = 0;
            // SAFETY: one source outer expansion of the privately owned initialized state.
            status = unsafe { benilla_source_direct_step(state.0.as_ptr(), &mut done) };
            work.record(2, SourceWarmPrimitive::DirectExpansion, 0, status, 0)?;
            if done != 1 {
                return Err(Error::Format.into());
            }
        }
        if status != SUCCESS {
            return Err(SourceWarmCacheError::UnsupportedDirectStatus(status));
        }
        work.lookup()?;
        let mut path = [0u64; SOURCE_WARM_CACHE_POINT_LIMIT];
        let mut count = 0;
        // SAFETY: completed private state and output capacity matching fixed source limit 74.
        let status = unsafe {
            benilla_source_direct_finish(
                state.0.as_ptr(),
                path.as_mut_ptr(),
                &mut count,
                SOURCE_WARM_CACHE_POINT_LIMIT as i32,
            )
        };
        work.record(2, SourceWarmPrimitive::DirectFinish, 0, status, 0)?;
        if status != SUCCESS {
            return Err(SourceWarmCacheError::UnsupportedDirectStatus(status));
        }
        if !(1..=SOURCE_WARM_CACHE_POINT_LIMIT as i32).contains(&count)
            || path[0] != start.polygon
            || path[count as usize - 1] != end.polygon
        {
            return Err(Error::Format.into());
        }
        candidate.active_len = count as usize;
        candidate.storage[..count as usize].copy_from_slice(&path[..count as usize]);
        deadline(work.until)?;
        Ok(status)
    }
    pub(super) fn query(
        mesh: &mut Mesh,
        before: &WarmCache,
        from: [u32; 3],
        to: [u32; 3],
        budget: SourceWarmCacheBudget,
        until: Instant,
    ) -> Result<LoadedSourceWarmCache, SourceWarmCacheError> {
        deadline(until)?;
        if before.mesh_id != mesh.instance_id {
            return Err(Error::Identity.into());
        }
        if budget.lookup_primitives > SOURCE_WARM_CACHE_MAX_LOOKUP_PRIMITIVES
            || budget.corridor_expansions > SOURCE_WARM_CACHE_MAX_CORRIDOR_EXPANSIONS
        {
            return Err(Error::Budget.into());
        }
        let a = detour(from)?;
        let b = detour(to)?;
        if (0..3).map(|i| (b[i] - a[i]) * (b[i] - a[i])).sum::<f32>() > 4096. * 4096. {
            return Err(Error::Budget.into());
        }
        let mut candidate = WarmCache {
            mesh_id: before.mesh_id,
            filter: before.filter,
            storage: copy_storage(&before.storage)?,
            active_len: before.active_len,
        };
        let mut work = Work {
            budget,
            until,
            lookups: 0,
            expansions: 0,
            trace: Vec::new(),
        };
        let start = lookup(mesh, &mut candidate, &a, 0, &mut work)?;
        // Start appends are visible here. Running these independently is not source-compatible.
        let end = lookup(mesh, &mut candidate, &b, 1, &mut work)?;
        if f32::from_bits(start.distance_bits) > 7.0 || f32::from_bits(end.distance_bits) > 7.0 {
            return Err(SourceWarmCacheError::FarFromPolygonUnsupported);
        }
        let (phase, direct_status) = if start.polygon == end.polygon {
            candidate.storage[0] = start.polygon;
            candidate.active_len = 1;
            (SourceWarmPolyPhase::SamePolygon, None)
        } else {
            let mut path_start = 0;
            let mut start_found = false;
            while path_start < candidate.active_len {
                work.lookup()?;
                let reference = candidate.storage[path_start];
                work.record(2, SourceWarmPrimitive::CutScan, reference, SUCCESS, 0)?;
                if reference == 0 {
                    break;
                }
                if reference == start.polygon {
                    start_found = true;
                    break;
                }
                path_start += 1;
            }
            let mut path_end = candidate.active_len.saturating_sub(1);
            let mut end_found = false;
            while path_end > path_start {
                work.lookup()?;
                let reference = candidate.storage[path_end];
                work.record(2, SourceWarmPrimitive::CutScan, reference, SUCCESS, 0)?;
                if reference == end.polygon {
                    end_found = true;
                    break;
                }
                path_end -= 1;
            }
            if start_found && end_found {
                candidate.active_len = path_end - path_start + 1;
                candidate.storage.copy_within(path_start..=path_end, 0);
                (SourceWarmPolyPhase::Cut, None)
            } else {
                let status = replace(mesh, &mut candidate, start, end, &a, &b, &mut work)?;
                (SourceWarmPolyPhase::Replacement, Some(status))
            }
        };
        deadline(until)?;
        Ok(LoadedSourceWarmCache {
            from,
            to,
            after: candidate,
            start,
            end,
            phase,
            direct_status,
            trace: work.trace,
        })
    }
}
