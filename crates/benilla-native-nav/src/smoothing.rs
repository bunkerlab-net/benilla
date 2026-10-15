// SPDX-License-Identifier: GPL-3.0-or-later
//! Loaded ground-mesh dependency only: original source smoothing before optimization,
//! normalization or owner PathFinder admission. Public polygon references are never inputs.
use super::{Error, Mesh, Nearest};
use std::time::Instant;

pub const SOURCE_SMOOTHING_MAX_POINTS: usize = 74;
pub const SOURCE_SMOOTHING_POLICY: u32 = 2;
pub const SOURCE_SMOOTHING_MAX_CORRIDOR_EXPANSIONS: usize = 4096;
pub const SOURCE_SMOOTHING_MAX_PRIMITIVES: usize = 256;
pub const SOURCE_PATHFINDER_CPP_SHA256: &str =
    "0fc2684275621af341e104d7593c7f66e5baa6251e088548deb5c547625294ef";
pub const SOURCE_PATHFINDER_H_SHA256: &str =
    "5c98dfd9a7d28a5faf3a86866baf056cd6df40c06f4816f64070a92d4a89773b";
/// A copied owner-derived filter; only the dry ground 1/0 subset is admitted here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceOwnerFilter {
    pub include_flags: u16,
    pub exclude_flags: u16,
}
/// Operational work caps, distinct from the source's semantic point limit.
/// Zero is allowed to exercise cancellation. Maximum values are 4096 and 256.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceSmoothingBudget {
    pub corridor_expansions: usize,
    pub smoothing_primitives: usize,
}
impl Default for SourceSmoothingBudget {
    fn default() -> Self {
        Self {
            corridor_expansions: SOURCE_SMOOTHING_MAX_CORRIDOR_EXPANSIONS,
            smoothing_primitives: SOURCE_SMOOTHING_MAX_PRIMITIVES,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceSmoothingDiagnostics {
    pub boundary_start_status: Option<u32>,
    pub boundary_end_status: Option<u32>,
    pub steer_status: Option<u32>,
    pub move_status: Option<u32>,
    pub height_status: Option<u32>,
    pub primitive_calls: usize,
    pub iterations: usize,
    pub steer_buffer_too_small: usize,
    pub move_buffer_too_small: usize,
    pub height_failures: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceSmoothingError {
    pub error: Error,
    pub diagnostics: SourceSmoothingDiagnostics,
}
impl From<Error> for SourceSmoothingError {
    fn from(error: Error) -> Self {
        Self {
            error,
            diagnostics: SourceSmoothingDiagnostics::default(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceSmoothStop {
    TargetReached,
    NoSteerTarget,
    PointLimit,
    BoundaryFailure,
}
/// A completed raw source function result, not a complete or admitted movement route.
/// `status` may be DT_FAILURE at the source limit, including when the target was reached.
/// NoSteerTarget may be DT_SUCCESS with only one point. Points precede OptimizePath and Z
/// normalization; intermediate heights have +0.5, while the final target keeps source bits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadedSourceSmooth {
    pub from: [u32; 3],
    pub to: [u32; 3],
    pub filter: SourceOwnerFilter,
    pub point_limit: usize,
    pub start: Nearest,
    pub end: Nearest,
    pub polygons: Vec<u64>,
    pub final_polygons: Vec<u64>,
    pub points: Vec<[u32; 3]>,
    pub status: u32,
    pub stop: SourceSmoothStop,
    pub diagnostics: SourceSmoothingDiagnostics,
}
/// Fresh bounded corridor bound to one exclusive mesh borrow, copied filter, original
/// endpoints and absolute deadline. It cannot be assembled from public polygon references.
///
/// ```compile_fail
/// # use benilla_native_nav::{Mesh, SourceOwnerFilter, SourceSmoothingBudget};
/// # use std::time::Instant;
/// # fn example(mesh: &mut Mesh, until: Instant) {
/// let corridor = mesh.loaded_source_corridor([0; 3], [0; 3],
///     SourceOwnerFilter { include_flags: 1, exclude_flags: 0 }, 74,
///     SourceSmoothingBudget::default(), until).unwrap();
/// let _other = mesh.nearest([0; 3], 5.0, until);
/// let _result = corridor.smooth();
/// # }
/// ```
pub struct LoadedSourceCorridor<'a> {
    mesh: &'a mut Mesh,
    from: [u32; 3],
    to: [u32; 3],
    filter: SourceOwnerFilter,
    point_limit: usize,
    budget: SourceSmoothingBudget,
    until: Instant,
    start: Nearest,
    end: Nearest,
    polygons: Vec<u64>,
}
impl std::fmt::Debug for LoadedSourceCorridor<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedSourceCorridor")
            .field("from", &self.from)
            .field("to", &self.to)
            .field("filter", &self.filter)
            .field("point_limit", &self.point_limit)
            .field("polygons", &self.polygons)
            .finish_non_exhaustive()
    }
}
impl LoadedSourceCorridor<'_> {
    pub fn original_from(&self) -> [u32; 3] {
        self.from
    }
    pub fn original_to(&self) -> [u32; 3] {
        self.to
    }
    pub fn filter(&self) -> SourceOwnerFilter {
        self.filter
    }
    pub fn point_limit(&self) -> usize {
        self.point_limit
    }
    pub fn polygons(&self) -> &[u64] {
        &self.polygons
    }
    pub fn start(&self) -> Nearest {
        self.start
    }
    pub fn end(&self) -> Nearest {
        self.end
    }
    /// Mint a bounded warm dependency from this handle's original, unsmoothed corridor.
    /// This does not establish a source owner initial-cache producer or a Route fact.
    pub fn warm_cache_seed(&self) -> Result<super::WarmCache, Error> {
        super::warm_cache::seed(
            self.mesh,
            &self.polygons,
            self.filter,
            self.point_limit,
            self.until,
        )
    }
    pub fn smooth(self) -> Result<LoadedSourceSmooth, SourceSmoothingError> {
        #[cfg(benilla_native_detour)]
        {
            native::smooth(self)
        }
        #[cfg(not(benilla_native_detour))]
        {
            let _ = (self.mesh, self.budget, self.until);
            Err(Error::Unsupported.into())
        }
    }
}
impl Mesh {
    /// Bounded nearest-poly lookup and the original direct findPath loop with ORIGINAL
    /// endpoint positions, a separately owned 1024-node pool and one expansion per check.
    /// Nearest 5/10 lookup deliberately bypasses getPolyByLocation and its cold-distance UB;
    /// this implements neither cached owner corridor reuse nor complete owner PathFinder.
    /// Incomplete direct statuses reject, including exact source truncation and exhaustion.
    pub fn loaded_source_corridor(
        &mut self,
        from: [u32; 3],
        to: [u32; 3],
        filter: SourceOwnerFilter,
        point_limit: usize,
        budget: SourceSmoothingBudget,
        until: Instant,
    ) -> Result<LoadedSourceCorridor<'_>, SourceSmoothingError> {
        #[cfg(benilla_native_detour)]
        {
            native::corridor(self, from, to, filter, point_limit, budget, until)
        }
        #[cfg(not(benilla_native_detour))]
        {
            let _ = (from, to, filter, point_limit, budget, until);
            Err(Error::Unsupported.into())
        }
    }
    /// Convenience composition of the separate bounded loaded corridor and raw smoother.
    /// Budgets/deadlines reject without publishing a partial dependency. A completed source
    /// failure is retained in the result's raw status rather than promoted to a route.
    pub fn loaded_source_smooth(
        &mut self,
        from: [u32; 3],
        to: [u32; 3],
        filter: SourceOwnerFilter,
        point_limit: usize,
        budget: SourceSmoothingBudget,
        until: Instant,
    ) -> Result<LoadedSourceSmooth, SourceSmoothingError> {
        self.loaded_source_corridor(from, to, filter, point_limit, budget, until)?
            .smooth()
    }
}

#[cfg(benilla_native_detour)]
mod native {
    use super::*;
    use crate::{deadline, detour, success, validate, world, PROGRESS};
    use std::{ffi::c_void, ptr::NonNull};
    #[repr(C)]
    #[derive(Default)]
    struct Step {
        kind: u32,
        status: u32,
        iterations: u32,
        stop: u32,
        source_status: u32,
        point_count: u32,
    }
    unsafe extern "C" {
        fn benilla_nav_source_nearest(
            raw: *mut c_void,
            p: *const f32,
            e: *const f32,
            include: u16,
            exclude: u16,
            r: *mut u64,
            n: *mut f32,
            over: *mut u8,
        ) -> u32;
        fn benilla_nav_source_direct_create(
            raw: *mut c_void,
            a: u64,
            b: u64,
            s: *const f32,
            e: *const f32,
            include: u16,
            exclude: u16,
            status: *mut u32,
        ) -> *mut c_void;
        fn benilla_source_direct_free(state: *mut c_void);
        fn benilla_source_direct_step(state: *mut c_void, done: *mut i32) -> u32;
        fn benilla_source_direct_finish(
            state: *mut c_void,
            path: *mut u64,
            count: *mut i32,
            limit: i32,
        ) -> u32;
        fn benilla_nav_source_create(
            raw: *mut c_void,
            s: *const f32,
            e: *const f32,
            path: *const u64,
            count: u32,
            limit: u32,
            include: u16,
            exclude: u16,
        ) -> *mut c_void;
        fn benilla_source_free(state: *mut c_void);
        fn benilla_source_step(state: *mut c_void, result: *mut Step) -> u32;
        fn benilla_source_copy(
            state: *mut c_void,
            points: *mut f32,
            path: *mut u64,
            count: *mut u32,
        );
    }
    fn nearest(
        mesh: &mut Mesh,
        p: [f32; 3],
        extent: f32,
        filter: SourceOwnerFilter,
        until: Instant,
    ) -> Result<Nearest, Error> {
        deadline(until)?;
        let mut cells = 1usize;
        for axis in [0, 2] {
            let low = ((p[axis] - extent - mesh.origin[axis]) / validate::GRID).floor();
            let high = ((p[axis] + extent - mesh.origin[axis]) / validate::GRID).floor();
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
        let mut polygon = 0;
        let mut point = [0.; 3];
        let mut over = 0;
        // SAFETY: immutable validated mesh, bounded finite search, initialized sized outputs;
        // copied filter is consumed synchronously and never points into caller storage.
        let status = unsafe {
            benilla_nav_source_nearest(
                mesh.raw.as_ptr(),
                p.as_ptr(),
                [extent; 3].as_ptr(),
                filter.include_flags,
                filter.exclude_flags,
                &mut polygon,
                point.as_mut_ptr(),
                &mut over,
            )
        };
        deadline(until)?;
        success(status)?;
        if polygon == 0 {
            return Err(Error::Unavailable);
        }
        if point.iter().any(|v| !v.is_finite() || v.abs() > 100_000.) {
            return Err(Error::Format);
        }
        Ok(Nearest {
            polygon,
            point: world(point),
            over: over == 1,
        })
    }
    pub(super) fn corridor(
        mesh: &mut Mesh,
        from: [u32; 3],
        to: [u32; 3],
        filter: SourceOwnerFilter,
        point_limit: usize,
        budget: SourceSmoothingBudget,
        until: Instant,
    ) -> Result<LoadedSourceCorridor<'_>, SourceSmoothingError> {
        if filter.include_flags != 1 || filter.exclude_flags != 0 {
            return Err(Error::Unsupported.into());
        }
        if !(2..=74).contains(&point_limit) {
            return Err(Error::Format.into());
        }
        if budget.corridor_expansions > SOURCE_SMOOTHING_MAX_CORRIDOR_EXPANSIONS
            || budget.smoothing_primitives > SOURCE_SMOOTHING_MAX_PRIMITIVES
        {
            return Err(Error::Budget.into());
        }
        let a = detour(from)?;
        let b = detour(to)?;
        if (0..3).map(|i| (b[i] - a[i]) * (b[i] - a[i])).sum::<f32>() > 4096. * 4096. {
            return Err(Error::Budget.into());
        }
        let mut lookup = |p| match nearest(mesh, p, 5., filter, until) {
            Err(Error::Unavailable) => nearest(mesh, p, 10., filter, until),
            r => r,
        };
        let start = lookup(a)?;
        let end = lookup(b)?;
        deadline(until)?;
        let mut status = 0;
        // SAFETY: refs come from this immutable mesh; original positions are finite.
        // The separately owned native state copies endpoints/filter and owns its pools.
        let raw = unsafe {
            benilla_nav_source_direct_create(
                mesh.raw.as_ptr(),
                start.polygon,
                end.polygon,
                a.as_ptr(),
                b.as_ptr(),
                filter.include_flags,
                filter.exclude_flags,
                &mut status,
            )
        };
        let state = NonNull::new(raw).map(DirectState);
        // Install the destructor before checking the deadline or any native status.
        deadline(until)?;
        let state = state.ok_or(Error::Native(status))?;
        let mut work = 0;
        while status == PROGRESS {
            deadline(until)?;
            if work >= budget.corridor_expansions {
                return Err(Error::Budget.into());
            }
            let mut done = 0;
            // SAFETY: private live state; exactly one original direct outer expansion.
            status = unsafe { benilla_source_direct_step(state.0.as_ptr(), &mut done) };
            deadline(until)?;
            if done != 1 {
                return Err(Error::Format.into());
            }
            work += 1;
        }
        success(status)?;
        let mut path = [0u64; SOURCE_SMOOTHING_MAX_POINTS];
        let mut count = 0;
        deadline(until)?;
        // SAFETY: fixed 74-slot output, requested 2..74 capacity, completed private state.
        // The original direct reconstruction succeeds at exact capacity and reports only
        // actual truncation as BUFFER_TOO_SMALL. Every incomplete status rejects.
        let status = unsafe {
            benilla_source_direct_finish(
                state.0.as_ptr(),
                path.as_mut_ptr(),
                &mut count,
                point_limit as i32,
            )
        };
        deadline(until)?;
        success(status)?;
        if !(1..=point_limit as i32).contains(&count)
            || path[0] != start.polygon
            || path[count as usize - 1] != end.polygon
        {
            return Err(Error::Unavailable.into());
        }
        let mut polygons = Vec::new();
        polygons
            .try_reserve_exact(count as usize)
            .map_err(|_| Error::Budget)?;
        polygons.extend_from_slice(&path[..count as usize]);
        drop(state);
        deadline(until)?;
        Ok(LoadedSourceCorridor {
            mesh,
            from,
            to,
            filter,
            point_limit,
            budget,
            until,
            start,
            end,
            polygons,
        })
    }
    struct DirectState(NonNull<c_void>);
    impl Drop for DirectState {
        fn drop(&mut self) {
            // SAFETY: sole owner; this state borrows the immutable mesh only until drop.
            // Cancellation discards private state without finalizing or publishing a path.
            unsafe { benilla_source_direct_free(self.0.as_ptr()) }
        }
    }
    struct State(NonNull<c_void>);
    impl Drop for State {
        fn drop(&mut self) {
            // SAFETY: sole native state owner; mesh lifetime is retained by the corridor.
            unsafe { benilla_source_free(self.0.as_ptr()) }
        }
    }
    pub(super) fn smooth(
        c: LoadedSourceCorridor<'_>,
    ) -> Result<LoadedSourceSmooth, SourceSmoothingError> {
        let mut diagnostics = SourceSmoothingDiagnostics::default();
        let run = |diagnostics: &mut SourceSmoothingDiagnostics| -> Result<_, Error> {
            deadline(c.until)?;
            let a = detour(c.from)?;
            let b = detour(c.to)?;
            // SAFETY: corridor private, generated by this same exclusively borrowed mesh;
            // source state copies refs, original positions and filter before returning.
            let state = State(
                NonNull::new(unsafe {
                    benilla_nav_source_create(
                        c.mesh.raw.as_ptr(),
                        a.as_ptr(),
                        b.as_ptr(),
                        c.polygons.as_ptr(),
                        c.polygons.len() as u32,
                        c.point_limit as u32,
                        c.filter.include_flags,
                        c.filter.exclude_flags,
                    )
                })
                .ok_or(Error::Budget)?,
            );
            deadline(c.until)?;
            let terminal = loop {
                deadline(c.until)?;
                if diagnostics.primitive_calls >= c.budget.smoothing_primitives {
                    return Err(Error::Budget);
                }
                let mut step = Step::default();
                // SAFETY: private live state, one primitive per call and initialized output.
                let outcome = unsafe { benilla_source_step(state.0.as_ptr(), &mut step) };
                diagnostics.primitive_calls += 1;
                diagnostics.iterations = step.iterations as usize;
                match step.kind {
                    0 => diagnostics.boundary_start_status = Some(step.status),
                    1 => diagnostics.boundary_end_status = Some(step.status),
                    2 => {
                        diagnostics.steer_status = Some(step.status);
                        if step.status & (1 << 4) != 0 {
                            diagnostics.steer_buffer_too_small += 1;
                        }
                    }
                    3 => {
                        diagnostics.move_status = Some(step.status);
                        if step.status & (1 << 4) != 0 {
                            diagnostics.move_buffer_too_small += 1;
                        }
                    }
                    4 => {
                        diagnostics.height_status = Some(step.status);
                        if step.status & (1 << 31) != 0 {
                            diagnostics.height_failures += 1;
                        }
                    }
                    _ => return Err(Error::Format),
                }
                deadline(c.until)?;
                match outcome {
                    0 => (),
                    1 => break step,
                    2 => return Err(Error::Native(step.status)),
                    _ => return Err(Error::Format),
                }
            };
            let stop = match terminal.stop {
                1 => SourceSmoothStop::TargetReached,
                2 => SourceSmoothStop::NoSteerTarget,
                3 => SourceSmoothStop::PointLimit,
                4 => SourceSmoothStop::BoundaryFailure,
                _ => return Err(Error::Format),
            };
            if terminal.point_count as usize > c.point_limit {
                return Err(Error::Format);
            }
            let mut raw_points = [[0f32; 3]; 74];
            let mut raw_path = [0u64; 74];
            let mut final_count = 0;
            deadline(c.until)?;
            // SAFETY: matching fixed native snapshot capacities; no refs are accepted back.
            unsafe {
                benilla_source_copy(
                    state.0.as_ptr(),
                    raw_points.as_mut_ptr().cast(),
                    raw_path.as_mut_ptr(),
                    &mut final_count,
                )
            };
            deadline(c.until)?;
            if !(1..=74).contains(&final_count) {
                return Err(Error::Format);
            }
            let mut points = Vec::new();
            points
                .try_reserve_exact(terminal.point_count as usize)
                .map_err(|_| Error::Budget)?;
            for p in &raw_points[..terminal.point_count as usize] {
                if p.iter().any(|v| !v.is_finite() || v.abs() > 100_000.) {
                    return Err(Error::Format);
                }
                points.push(world(*p));
            }
            let mut final_polygons = Vec::new();
            final_polygons
                .try_reserve_exact(final_count as usize)
                .map_err(|_| Error::Budget)?;
            final_polygons.extend_from_slice(&raw_path[..final_count as usize]);
            deadline(c.until)?;
            Ok((points, final_polygons, terminal.source_status, stop))
        };
        let (points, final_polygons, status, stop) =
            run(&mut diagnostics).map_err(|error| SourceSmoothingError {
                error,
                diagnostics: diagnostics.clone(),
            })?;
        // One final publication check, still using the original absolute request deadline.
        deadline(c.until).map_err(|error| SourceSmoothingError {
            error,
            diagnostics: diagnostics.clone(),
        })?;
        Ok(LoadedSourceSmooth {
            from: c.from,
            to: c.to,
            filter: c.filter,
            point_limit: c.point_limit,
            start: c.start,
            end: c.end,
            polygons: c.polygons,
            final_polygons,
            points,
            status,
            stop,
            diagnostics,
        })
    }
}
