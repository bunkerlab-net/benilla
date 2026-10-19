// SPDX-License-Identifier: GPL-3.0-or-later
//! Private, bounded MMap/Detour reader. Queries describe only the loaded ground mesh;
//! they never authorize body movement, sloped support, source epochs or live sessions.
mod cold_distance;
mod smoothing;
mod source_cold_same_polygon;
mod source_home_same_polygon;
mod source_query;
mod source_wander_same_polygon;
mod tile_presence;
mod validate;
mod warm_cache;
mod warm_point_path;
mod warm_straight_path;
pub use cold_distance::*;
pub use smoothing::*;
pub use source_cold_same_polygon::*;
pub use source_home_same_polygon::*;
pub use source_query::*;
pub use source_wander_same_polygon::*;
pub use tile_presence::*;
pub use warm_cache::*;
pub use warm_point_path::*;
pub use warm_straight_path::*;
/// Selected ABI builds; source differential evidence is still platform-specific.
pub const SUPPORTED_ABI: bool = cfg!(benilla_native_detour);
use std::{marker::PhantomData, ptr::NonNull, rc::Rc, time::Instant};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Format,
    Framing,
    Unsupported,
    Identity,
    Budget,
    Deadline,
    Unavailable,
    Native(u32),
}
pub struct TileInput<'a> {
    pub grid: [u8; 2],
    pub bytes: &'a [u8],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Nearest {
    pub polygon: u64,
    pub point: [u32; 3],
    pub over: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Corner {
    pub point: [u32; 3],
    pub flags: u8,
    pub polygon: u64,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Route {
    pub start: Nearest,
    pub end: Nearest,
    pub polygons: Vec<u64>,
    pub corners: Vec<Corner>,
}
/// Never Send/Sync: pinned query state is mutable and sliced searches retain a filter pointer.
pub struct Mesh {
    raw: NonNull<std::ffi::c_void>,
    // Allocation addresses can be reused; retained dependencies require a never-reused ID.
    instance_id: u64,
    query_profile: QueryProfile,
    origin: [f32; 3],
    agent: [f32; 3],
    _single_thread: PhantomData<Rc<()>>,
}
const SUCCESS: u32 = 1 << 30;
const PROGRESS: u32 = 1 << 29;
// Early deadline/budget/native failure must reset source sliced state before any
// subsequent non-sliced query. Finalization is bounded by the selected node pool.
struct SearchGuard {
    raw: NonNull<std::ffi::c_void>,
    active: bool,
}
impl Drop for SearchGuard {
    fn drop(&mut self) {
        if self.active {
            let mut path = [0u64; 256];
            let mut count = 0;
            // SAFETY: guard owns the active search, output matches bridge's fixed limit.
            unsafe {
                ffi::benilla_nav_finish(self.raw.as_ptr(), path.as_mut_ptr(), &mut count);
            }
        }
    }
}

#[cfg(benilla_native_detour)]
mod ffi {
    use std::ffi::c_void;
    unsafe extern "C" {
        pub fn benilla_nav_create(params: *const u8) -> *mut c_void;
        pub fn benilla_nav_create_source1024(params: *const u8) -> *mut c_void;
        pub fn benilla_nav_query_capacity(mesh: *const c_void) -> u32;
        pub fn benilla_nav_free(mesh: *mut c_void);
        pub fn benilla_nav_add(mesh: *mut c_void, bytes: *const u8, size: i32) -> u32;
        pub fn benilla_nav_nearest(
            mesh: *mut c_void,
            p: *const f32,
            e: *const f32,
            r: *mut u64,
            n: *mut f32,
            o: *mut u8,
        ) -> u32;
        pub fn benilla_nav_begin(
            mesh: *mut c_void,
            a: u64,
            b: u64,
            s: *const f32,
            e: *const f32,
        ) -> u32;
        pub fn benilla_nav_step(mesh: *mut c_void, done: *mut i32) -> u32;
        pub fn benilla_nav_finish(mesh: *mut c_void, path: *mut u64, count: *mut i32) -> u32;
        pub fn benilla_nav_straight(
            mesh: *mut c_void,
            s: *const f32,
            e: *const f32,
            path: *const u64,
            count: i32,
            points: *mut f32,
            flags: *mut u8,
            refs: *mut u64,
            n: *mut i32,
        ) -> u32;
    }
}
// Other targets build the surrounding client but cannot instantiate the selected native ABI.
#[cfg(not(benilla_native_detour))]
#[allow(unused_variables)]
mod ffi {
    use std::ffi::c_void;
    pub unsafe fn benilla_nav_create(params: *const u8) -> *mut c_void {
        std::ptr::null_mut()
    }
    pub unsafe fn benilla_nav_create_source1024(params: *const u8) -> *mut c_void {
        std::ptr::null_mut()
    }
    pub unsafe fn benilla_nav_query_capacity(mesh: *const c_void) -> u32 {
        0
    }
    pub unsafe fn benilla_nav_free(mesh: *mut c_void) {}
    pub unsafe fn benilla_nav_add(mesh: *mut c_void, bytes: *const u8, size: i32) -> u32 {
        0
    }
    pub unsafe fn benilla_nav_nearest(
        mesh: *mut c_void,
        p: *const f32,
        e: *const f32,
        r: *mut u64,
        n: *mut f32,
        o: *mut u8,
    ) -> u32 {
        0
    }
    pub unsafe fn benilla_nav_begin(
        mesh: *mut c_void,
        a: u64,
        b: u64,
        s: *const f32,
        e: *const f32,
    ) -> u32 {
        0
    }
    pub unsafe fn benilla_nav_step(mesh: *mut c_void, done: *mut i32) -> u32 {
        0
    }
    pub unsafe fn benilla_nav_finish(mesh: *mut c_void, path: *mut u64, count: *mut i32) -> u32 {
        0
    }
    pub unsafe fn benilla_nav_straight(
        mesh: *mut c_void,
        s: *const f32,
        e: *const f32,
        path: *const u64,
        count: i32,
        points: *mut f32,
        flags: *mut u8,
        refs: *mut u64,
        n: *mut i32,
    ) -> u32 {
        0
    }
}
fn deadline(until: Instant) -> Result<(), Error> {
    if Instant::now() >= until {
        Err(Error::Deadline)
    } else {
        Ok(())
    }
}
fn success(s: u32) -> Result<(), Error> {
    if s == SUCCESS {
        Ok(())
    } else {
        Err(Error::Native(s))
    }
}
fn detour(p: [u32; 3]) -> Result<[f32; 3], Error> {
    let p = p.map(f32::from_bits);
    if p.iter().any(|v| !v.is_finite() || v.abs() > 100_000.) {
        return Err(Error::Format);
    }
    Ok([p[1], p[2], p[0]]) // PathFinder input {y,z,x}; no sign change or float roundtrip
}
fn world(p: [f32; 3]) -> [u32; 3] {
    [p[2], p[0], p[1]].map(f32::to_bits)
}
impl Mesh {
    pub fn load(params: &[u8], tiles: &[TileInput<'_>]) -> Result<Self, Error> {
        Self::load_profile(params, tiles, QueryProfile::Bounded512)
    }
    fn load_profile(
        params: &[u8],
        tiles: &[TileInput<'_>],
        query_profile: QueryProfile,
    ) -> Result<Self, Error> {
        if !cfg!(benilla_native_detour) {
            return Err(Error::Unsupported);
        }
        // This performs the complete validation of every tile before even native init.
        let valid = validate::Validated::decode(params, tiles)?;
        static NEXT_MESH_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let instance_id = NEXT_MESH_ID
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |id| id.checked_add(1),
            )
            .map_err(|_| Error::Budget)?;
        // SAFETY: exactly 28 validated bytes, copied into an aligned native params object.
        let raw = NonNull::new(unsafe {
            match query_profile {
                QueryProfile::Bounded512 => ffi::benilla_nav_create(valid.params.as_ptr()),
                QueryProfile::SourceMapInstance1024 => {
                    ffi::benilla_nav_create_source1024(valid.params.as_ptr())
                }
            }
        })
        .ok_or(Error::Budget)?;
        let mesh = Self {
            raw,
            instance_id,
            query_profile,
            origin: valid.origin,
            agent: valid.agent,
            _single_thread: PhantomData,
        };
        if mesh.query_capacity() != query_profile.node_capacity() {
            return Err(Error::Identity);
        }
        for bytes in valid.tiles {
            // SAFETY: validated exact layout/topology, all counts and conversions bounded.
            // C++ makes its own correctly aligned copy; flags=0, owner frees after mesh.
            success(unsafe {
                ffi::benilla_nav_add(mesh.raw.as_ptr(), bytes.as_ptr(), bytes.len() as i32)
            })?;
        }
        Ok(mesh)
    }
    /// The native allocation profile, not a caller-selected policy/hash label.
    pub fn query_profile(&self) -> QueryProfile {
        self.query_profile
    }
    /// Read back the actual attached query's node pool, not the requested capacity.
    pub fn query_capacity(&self) -> u32 {
        // SAFETY: this immutable observation uses the live query owned by self.
        unsafe { ffi::benilla_nav_query_capacity(self.raw.as_ptr()) }
    }
    pub fn agent_dimensions(&self) -> [f32; 3] {
        self.agent
    }
    fn nearest_native(
        &mut self,
        p: [f32; 3],
        extent: f32,
        until: Instant,
    ) -> Result<Nearest, Error> {
        deadline(until)?;
        // queryPolygons visits grid cells, even absent tiles. Bound its float-to-int casts
        // and rectangle independently of decoded tile counts. Binary32 order matches source.
        let mut cells = 1usize;
        for axis in [0, 2] {
            let low = ((p[axis] - extent - self.origin[axis]) / validate::GRID).floor();
            let high = ((p[axis] + extent - self.origin[axis]) / validate::GRID).floor();
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
        let mut reference = 0;
        let mut point = [0.; 3];
        let mut over = 0;
        // SAFETY: exclusive mesh, validated finite inputs, initialized sized output buffers.
        let status = unsafe {
            ffi::benilla_nav_nearest(
                self.raw.as_ptr(),
                p.as_ptr(),
                [extent; 3].as_ptr(),
                &mut reference,
                point.as_mut_ptr(),
                &mut over,
            )
        };
        deadline(until)?;
        success(status)?;
        if reference == 0 {
            return Err(Error::Unavailable);
        }
        if point.iter().any(|v| !v.is_finite() || v.abs() > 100_000.) {
            return Err(Error::Format);
        }
        Ok(Nearest {
            polygon: reference,
            point: world(point),
            over: over == 1,
        })
    }
    /// Source nearest-poly primitive with explicit bounded search extent, not owner PathFinder.
    pub fn nearest(
        &mut self,
        point: [u32; 3],
        extent: f32,
        until: Instant,
    ) -> Result<Nearest, Error> {
        if !extent.is_finite() || extent <= 0. || extent > 10. {
            return Err(Error::Format);
        }
        self.nearest_native(detour(point)?, extent, until)
    }
    /// Complete ground-only corridor plus source funnel. No partial/no-node/buffer-truncated
    /// result succeeds. Each sliced expansion checks the original absolute deadline.
    pub fn route(&mut self, from: [u32; 3], to: [u32; 3], until: Instant) -> Result<Route, Error> {
        self.route_with_limit(from, to, until, 4096)
    }
    fn route_with_limit(
        &mut self,
        from: [u32; 3],
        to: [u32; 3],
        until: Instant,
        steps: usize,
    ) -> Result<Route, Error> {
        let a = detour(from)?;
        let b = detour(to)?;
        if (0..3).map(|i| (b[i] - a[i]) * (b[i] - a[i])).sum::<f32>() > 4096. * 4096. {
            return Err(Error::Budget);
        }
        // Fresh-path near/far lookup follows PathFinder's 5/10 search boxes; owner-dependent
        // reuse, masks, shortcuts, smoothing and endpoint admission are intentionally absent.
        let mut lookup = |p| match self.nearest_native(p, 5., until) {
            Err(Error::Unavailable) => self.nearest_native(p, 10., until),
            r => r,
        };
        let start = lookup(a)?;
        let end = lookup(b)?;
        let a = detour(start.point)?;
        let b = detour(end.point)?;
        deadline(until)?;
        // SAFETY: both references came from this same unchanged mesh and filter is owned by it.
        let mut status = unsafe {
            ffi::benilla_nav_begin(
                self.raw.as_ptr(),
                start.polygon,
                end.polygon,
                a.as_ptr(),
                b.as_ptr(),
            )
        };
        let mut guard = SearchGuard {
            raw: self.raw,
            active: true,
        };
        let mut work = 0;
        while status == PROGRESS {
            deadline(until)?;
            if work >= steps {
                return Err(Error::Budget);
            }
            let mut done = 0;
            // SAFETY: this mesh exclusively owns the active sliced search; one expansion.
            status = unsafe { ffi::benilla_nav_step(self.raw.as_ptr(), &mut done) };
            if done != 1 && status == PROGRESS {
                return Err(Error::Format);
            }
            work += 1;
        }
        deadline(until)?;
        success(status)?;
        let mut path = [0u64; 256];
        let mut count = 0;
        // SAFETY: bridge uses exactly 256 as max output, matching this allocation.
        let status =
            unsafe { ffi::benilla_nav_finish(self.raw.as_ptr(), path.as_mut_ptr(), &mut count) };
        guard.active = false;
        deadline(until)?;
        success(status)?;
        if !(1..=256).contains(&count)
            || path[0] != start.polygon
            || path[count as usize - 1] != end.polygon
        {
            return Err(Error::Unavailable);
        }
        let mut points = [[0f32; 3]; 256];
        let mut flags = [0u8; 256];
        let mut refs = [0u64; 256];
        let mut n = 0;
        // SAFETY: valid source-produced corridor and fixed sized parallel output arrays.
        let status = unsafe {
            ffi::benilla_nav_straight(
                self.raw.as_ptr(),
                a.as_ptr(),
                b.as_ptr(),
                path.as_ptr(),
                count,
                points.as_mut_ptr().cast(),
                flags.as_mut_ptr(),
                refs.as_mut_ptr(),
                &mut n,
            )
        };
        deadline(until)?;
        success(status)?;
        if !(1..=256).contains(&n) || flags[n as usize - 1] & 2 == 0 {
            return Err(Error::Unavailable);
        }
        let mut corners = Vec::new();
        corners
            .try_reserve_exact(n as usize)
            .map_err(|_| Error::Budget)?;
        for i in 0..n as usize {
            if points[i]
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 100_000.)
                || flags[i] & !3 != 0
            {
                return Err(Error::Format);
            }
            corners.push(Corner {
                point: world(points[i]),
                flags: flags[i],
                polygon: refs[i],
            });
        }
        let mut polygons = Vec::new();
        polygons
            .try_reserve_exact(count as usize)
            .map_err(|_| Error::Budget)?;
        polygons.extend_from_slice(&path[..count as usize]);
        deadline(until)?;
        Ok(Route {
            start,
            end,
            polygons,
            corners,
        })
    }
}
impl Drop for Mesh {
    fn drop(&mut self) {
        // SAFETY: sole native owner, no outstanding borrows and destructor frees each buffer once.
        unsafe { ffi::benilla_nav_free(self.raw.as_ptr()) }
    }
}
#[cfg(all(test, benilla_native_detour))]
mod tests;

#[cfg(all(test, not(benilla_native_detour)))]
#[test]
fn unsupported_profile_never_constructs_a_native_mesh() {
    assert!(!SUPPORTED_ABI);
    assert!(matches!(Mesh::load(&[], &[]), Err(Error::Unsupported)));
}

#[cfg(all(test, benilla_native_detour))]
mod smoothing_tests;

#[cfg(all(test, benilla_native_detour))]
mod direct_corridor_tests;

#[cfg(all(test, benilla_native_detour))]
mod warm_cache_tests;

#[cfg(all(test, benilla_native_detour))]
mod warm_point_path_tests;

#[cfg(all(test, benilla_native_detour))]
mod warm_straight_path_tests;

#[cfg(all(test, benilla_native_detour))]
mod cold_distance_tests;

#[cfg(all(test, benilla_native_detour))]
mod source_wander_and_chase_reconstruction_tests;

#[cfg(all(test, benilla_native_detour))]
mod generated_tests;
