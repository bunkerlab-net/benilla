// SPDX-License-Identifier: GPL-3.0-or-later
//! PathFinder.cpp:563-792,855-877,1226-1348. Status details remain source inputs.
use super::*;

pub struct PathQuery<'a> {
    raw: *mut c_void,
    _mesh: &'a MapMesh,
}
#[derive(Clone, Copy, Debug)]
pub struct PolyResult {
    pub status: u32,
    pub hit: f32,
}
impl PathCache {
    /// PathFinder.cpp:401-408 retains the previous limit only at distance <= 100.
    pub fn dungeon_limit(&mut self, distance: f32) {
        let limit = if distance > 300. {
            296
        } else if distance > 200. {
            222
        } else if distance > 100. {
            148
        } else {
            self.point_limit
        };
        self.point_limit = limit;
        if self.storage.len() < limit {
            self.storage.resize(limit, 0);
        }
    }
    pub fn active(&self) -> &[u64] {
        &self.storage[..self.active]
    }
    /// PathFinder.cpp:529-573 searches the end backwards strictly after the start.
    pub fn reuse_subrange(&mut self, start: u64, end: u64) -> bool {
        let Some(first) = self.active().iter().position(|&r| r == 0 || r == start) else {
            return false;
        };
        if self.storage[first] != start {
            return false;
        }
        let Some(last) = (first + 1..self.active)
            .rev()
            .find(|&i| self.storage[i] == end)
        else {
            return false;
        };
        self.active = last - first + 1;
        self.storage.copy_within(first..last + 1, 0);
        true
    }
    pub fn same_poly(&mut self, reference: u64) {
        self.storage[0] = reference;
        self.active = 1;
    }
}
impl MapMesh {
    pub fn pathfinder_query(&self, handle: &QueryHandle) -> Result<PathQuery<'_>, LiveError> {
        numeric()?;
        Ok(PathQuery {
            raw: self.checked_query(handle)?.raw.as_ptr(),
            _mesh: self,
        })
    }
}
impl PathQuery<'_> {
    pub fn have_tile(&self, point: [f32; 3]) -> Result<bool, LiveError> {
        // DetourNavMesh.cpp:1191-1195 uses only horizontal X/Z in calcTileLoc.
        if [point[0], point[2]]
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 32768.)
        {
            return Err(Error::Budget.into());
        }
        #[cfg(benilla_native_detour)]
        // SAFETY: live query and bounded finite point; calcTileLoc fits int32.
        unsafe {
            Ok(ffi_path::benilla_live_have_tile(self.raw, point.as_ptr()) != 0)
        }
        #[cfg(not(benilla_native_detour))]
        {
            Err(Error::Unsupported.into())
        }
    }
    fn point(&self, point: [f32; 3]) -> Result<(), LiveError> {
        if point.iter().any(|v| !v.is_finite() || v.abs() > 32768.) {
            return Err(Error::Budget.into());
        }
        Ok(())
    }
    pub fn closest(&self, reference: u64, point: [f32; 3]) -> Result<(u32, [f32; 3]), LiveError> {
        self.point(point)?;
        let mut out = point;
        #[cfg(benilla_native_detour)]
        // SAFETY: Detour checks the reference before accessing its polygon.
        let status = unsafe {
            ffi_path::benilla_live_closest(self.raw, reference, point.as_ptr(), out.as_mut_ptr())
        };
        #[cfg(not(benilla_native_detour))]
        let status = {
            let _ = reference;
            return Err(Error::Unsupported.into());
        };
        self.point(out)?;
        Ok((status, out))
    }
    pub fn height(
        &self,
        reference: u64,
        point: [f32; 3],
        height: &mut f32,
    ) -> Result<u32, LiveError> {
        self.point(point)?;
        #[cfg(benilla_native_detour)]
        // SAFETY: height is initialized, including the source's failure-preserved value.
        let status =
            unsafe { ffi_path::benilla_live_height(self.raw, reference, point.as_ptr(), height) };
        #[cfg(not(benilla_native_detour))]
        let status = {
            let _ = (reference, height);
            return Err(Error::Unsupported.into());
        };
        Ok(status)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn poly_path(
        &self,
        cache: &mut PathCache,
        filter: SourceOwnerFilter,
        ends: [u64; 2],
        start: [f32; 3],
        end: [f32; 3],
        ray: bool,
    ) -> Result<PolyResult, LiveError> {
        self.point(start)?;
        self.point(end)?;
        let mut count = 0;
        let mut hit = 0.;
        #[cfg(benilla_native_detour)]
        // SAFETY: cache storage covers the exact source point limit; query validates refs.
        let status = unsafe {
            ffi_path::benilla_live_find_path(
                self.raw,
                ends[0],
                ends[1],
                start.as_ptr(),
                end.as_ptr(),
                filter.include_flags,
                filter.exclude_flags,
                cache.storage.as_mut_ptr(),
                &mut count,
                cache.point_limit as i32,
                u32::from(ray),
                &mut hit,
            )
        };
        #[cfg(not(benilla_native_detour))]
        let status = {
            let _ = (filter, ends, ray);
            return Err(Error::Unsupported.into());
        };
        if count < 0 || count as usize > cache.point_limit {
            return Err(Error::Format.into());
        }
        cache.active = count as usize;
        Ok(PolyResult { status, hit })
    }
    /// PathFinder.cpp:880-939 retains old output slots skipped by zero sentinels.
    pub fn convert_points(
        &self,
        yzx: &[f32],
        count: usize,
        optimize: bool,
        xyz: &mut Vec<[f32; 3]>,
    ) -> Result<usize, LiveError> {
        if count > 296 || yzx.len() < count * 3 {
            return Err(Error::Budget.into());
        }
        xyz.resize(count, [0.; 3]);
        let mut final_count = 0;
        #[cfg(benilla_native_detour)]
        // SAFETY: exact validated input length and initialized prior-output storage.
        let result = unsafe {
            ffi_path::benilla_live_convert_points(
                yzx.as_ptr(),
                count as u32,
                u32::from(optimize),
                xyz.as_mut_ptr().cast(),
                &mut final_count,
            )
        };
        #[cfg(not(benilla_native_detour))]
        let result = {
            let _ = optimize;
            return Err(Error::Unsupported.into());
        };
        if result == 2 {
            return Err(LiveError::PathOperation(924));
        }
        if result != 1 || final_count as usize > count {
            return Err(Error::Format.into());
        }
        xyz.truncate(final_count as usize);
        Ok(final_count as usize)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn points(
        &self,
        cache: &PathCache,
        filter: SourceOwnerFilter,
        start: [f32; 3],
        end: [f32; 3],
        straight: bool,
        points: &mut Vec<f32>,
        scratch: &mut Vec<u64>,
    ) -> Result<(u32, usize), LiveError> {
        self.point(start)?;
        self.point(end)?;
        if points.len() < cache.point_limit * 3 {
            points.resize(cache.point_limit * 3, 0.);
        }
        #[cfg(benilla_native_detour)]
        {
            if straight {
                let mut count = 0;
                // SAFETY: native validates refs; output capacity is the source point limit.
                let status = unsafe {
                    ffi_path::benilla_live_straight(
                        self.raw,
                        start.as_ptr(),
                        end.as_ptr(),
                        cache.storage.as_ptr(),
                        cache.active as i32,
                        points.as_mut_ptr(),
                        &mut count,
                        cache.point_limit as i32,
                    )
                };
                if count < 0 || count as usize > cache.point_limit {
                    return Err(Error::Format.into());
                }
                return Ok((status, count as usize));
            }
            if scratch.len() < cache.point_limit {
                scratch.resize(cache.point_limit, 0);
            }
            // SAFETY: source-owned prefix and all buffers have the exact declared capacity.
            let raw = unsafe {
                ffi_path::benilla_live_smooth_create(
                    self.raw,
                    start.as_ptr(),
                    end.as_ptr(),
                    cache.storage.as_ptr(),
                    cache.active as u32,
                    cache.point_limit as u32,
                    filter.include_flags,
                    filter.exclude_flags,
                    scratch.as_mut_ptr(),
                    points.as_mut_ptr(),
                )
            };
            let raw = NonNull::new(raw).ok_or(LiveError::Allocation)?;
            struct Guard(NonNull<c_void>);
            impl Drop for Guard {
                fn drop(&mut self) {
                    unsafe { ffi_path::benilla_source_free(self.0.as_ptr()) }
                }
            }
            let state = Guard(raw);
            let mut step = ffi_path::Step::default();
            // Each iteration advances one source primitive; the semantic point limit bounds this loop.
            loop {
                let result = unsafe { ffi_path::benilla_source_step(state.0.as_ptr(), &mut step) };
                if result == 2 {
                    return Err(LiveError::PathOperation(step.kind));
                }
                if result == 1 {
                    break;
                }
            }
            let mut scratch_count = 0;
            unsafe {
                ffi_path::benilla_source_copy_live(
                    state.0.as_ptr(),
                    points.as_mut_ptr(),
                    scratch.as_mut_ptr(),
                    &mut scratch_count,
                )
            };
            if step.point_count as usize > cache.point_limit
                || scratch_count as usize > cache.point_limit
            {
                return Err(Error::Format.into());
            }
            Ok((step.source_status, step.point_count as usize))
        }
        #[cfg(not(benilla_native_detour))]
        {
            let _ = (filter, straight);
            Err(Error::Unsupported.into())
        }
    }
}
#[cfg(benilla_native_detour)]
mod ffi_path {
    use super::*;
    #[repr(C)]
    #[derive(Default)]
    pub struct Step {
        pub kind: u32,
        pub status: u32,
        pub iterations: u32,
        pub stop: u32,
        pub source_status: u32,
        pub point_count: u32,
    }
    unsafe extern "C" {
        pub fn benilla_live_have_tile(raw: *mut c_void, p: *const f32) -> u32;
        pub fn benilla_live_find_path(
            raw: *mut c_void,
            a: u64,
            b: u64,
            start: *const f32,
            end: *const f32,
            include: u16,
            exclude: u16,
            path: *mut u64,
            count: *mut i32,
            limit: i32,
            ray: u32,
            hit: *mut f32,
        ) -> u32;
        pub fn benilla_live_closest(
            raw: *mut c_void,
            reference: u64,
            p: *const f32,
            out: *mut f32,
        ) -> u32;
        pub fn benilla_live_height(
            raw: *mut c_void,
            reference: u64,
            p: *const f32,
            out: *mut f32,
        ) -> u32;
        pub fn benilla_live_straight(
            raw: *mut c_void,
            start: *const f32,
            end: *const f32,
            path: *const u64,
            count: i32,
            points: *mut f32,
            n: *mut i32,
            limit: i32,
        ) -> u32;
        pub fn benilla_live_smooth_create(
            raw: *mut c_void,
            start: *const f32,
            end: *const f32,
            path: *const u64,
            count: u32,
            limit: u32,
            include: u16,
            exclude: u16,
            scratch: *mut u64,
            points: *mut f32,
        ) -> *mut c_void;
        pub fn benilla_source_step(raw: *mut c_void, step: *mut Step) -> u32;
        pub fn benilla_source_copy_live(
            raw: *mut c_void,
            points: *mut f32,
            path: *mut u64,
            count: *mut u32,
        );
        pub fn benilla_source_free(raw: *mut c_void);
        pub fn benilla_live_convert_points(
            yzx: *const f32,
            count: u32,
            optimize: u32,
            xyz: *mut f32,
            final_count: *mut u32,
        ) -> u32;
    }
}

#[cfg(all(test, benilla_native_detour))]
#[path = "live_path_tests.rs"]
mod tests;
