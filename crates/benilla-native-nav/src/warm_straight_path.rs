// SPDX-License-Identifier: GPL-3.0-or-later
//! A bounded warm straight calculation, never Chase dispatch, a cold producer or Route.
//! Collision width is an explicit scalar dependency; its owner provenance is external.
use crate::{
    Error, Mesh, SourcePointPathConfig, SourceWarmCacheError, SourceWarmLookup,
    SourceWarmPolyPhase, SourceWarmTrace, WarmCache, SOURCE_WARM_CACHE_MAX_LOOKUP_PRIMITIVES,
};
use std::time::Instant;

pub const SOURCE_WARM_STRAIGHT_PATH_POLICY: u32 = 1;
/// Operational domain, independent of source's equality-only point limit 74.
pub const SOURCE_STRAIGHT_PATH_MAX_POINTS: usize = 1026;
pub const SOURCE_STRAIGHT_PATH_MAX_RAY_POLYGONS: usize = 4096;
pub const SOURCE_STRAIGHT_PATH_MAX_PRIMITIVES: usize = 4096;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceStraightPathBudget {
    pub lookup_primitives: usize,
    pub raycast_polygons: usize,
    pub sampled_points: usize,
    pub native_primitives: usize,
}
impl Default for SourceStraightPathBudget {
    fn default() -> Self {
        Self {
            lookup_primitives: SOURCE_WARM_CACHE_MAX_LOOKUP_PRIMITIVES,
            raycast_polygons: SOURCE_STRAIGHT_PATH_MAX_RAY_POLYGONS,
            sampled_points: SOURCE_STRAIGHT_PATH_MAX_POINTS,
            native_primitives: SOURCE_STRAIGHT_PATH_MAX_PRIMITIVES,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceStraightPathError {
    Boundary(Error),
    WarmCache(SourceWarmCacheError),
    ActiveNormalizationUnsupported,
    InvalidPreviousPathType,
    InvalidCollisionWidth,
}
impl From<Error> for SourceStraightPathError {
    fn from(value: Error) -> Self {
        Self::Boundary(value)
    }
}
impl From<SourceWarmCacheError> for SourceStraightPathError {
    fn from(value: SourceWarmCacheError) -> Self {
        Self::WarmCache(value)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceStraightRay {
    pub status: u32,
    pub hit_bits: u32,
    /// Exact Detour YZX output; unlike path points, this is not permuted to world XYZ.
    pub normal_bits: [u32; 3],
    pub polygons: Vec<u64>,
    pub adjusted_hit_bits: Option<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceStraightHeight {
    /// Three is the clipped ray hit; four is an accumulated straight sample.
    pub endpoint: u8,
    pub polygon: u64,
    pub before: [u32; 3],
    pub after: [u32; 3],
    pub status: u32,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceStraightPathDiagnostics {
    pub raycast_polygons: usize,
    pub native_primitives: usize,
    pub sampled_points: usize,
    pub heights: Vec<SourceStraightHeight>,
}
/// Opaque loaded terminal proposal. Read-only diagnostics cannot mint another one.
/// `end` can be source-clipped; `requested_end` always retains the original query.
/// `optimize_path` is bound in the configuration but source ignores it on this branch.
///
/// ```compile_fail
/// use benilla_native_nav::{WarmCache, LoadedSourceWarmStraightPath};
/// fn forge(cache: WarmCache) -> LoadedSourceWarmStraightPath { cache.into() }
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct LoadedSourceWarmStraightPath {
    from: [u32; 3],
    requested_end: [u32; 3],
    end: [u32; 3],
    actual_end: [u32; 3],
    collision_width_bits: u32,
    config: SourcePointPathConfig,
    start_lookup: SourceWarmLookup,
    end_lookup: SourceWarmLookup,
    phase: SourceWarmPolyPhase,
    after: WarmCache,
    points: Vec<[u32; 3]>,
    path_type: u32,
    ray: Option<SourceStraightRay>,
    trace: Vec<SourceWarmTrace>,
    diagnostics: SourceStraightPathDiagnostics,
}
impl LoadedSourceWarmStraightPath {
    pub fn from(&self) -> [u32; 3] {
        self.from
    }
    pub fn requested_end(&self) -> [u32; 3] {
        self.requested_end
    }
    pub fn end(&self) -> [u32; 3] {
        self.end
    }
    pub fn actual_end(&self) -> [u32; 3] {
        self.actual_end
    }
    pub fn collision_width_bits(&self) -> u32 {
        self.collision_width_bits
    }
    pub fn config(&self) -> SourcePointPathConfig {
        self.config
    }
    pub fn start_lookup(&self) -> SourceWarmLookup {
        self.start_lookup
    }
    pub fn end_lookup(&self) -> SourceWarmLookup {
        self.end_lookup
    }
    pub fn phase(&self) -> SourceWarmPolyPhase {
        self.phase
    }
    pub fn after(&self) -> &WarmCache {
        &self.after
    }
    pub fn into_after(self) -> WarmCache {
        self.after
    }
    pub fn points(&self) -> &[[u32; 3]] {
        &self.points
    }
    pub fn path_type(&self) -> u32 {
        self.path_type
    }
    pub fn ray(&self) -> Option<&SourceStraightRay> {
        self.ray.as_ref()
    }
    pub fn trace(&self) -> &[SourceWarmTrace] {
        &self.trace
    }
    pub fn diagnostics(&self) -> &SourceStraightPathDiagnostics {
        &self.diagnostics
    }
}
impl Mesh {
    /// Sequential source selection, optional raycast and straight sampling on one private
    /// candidate. SamePolygon/Cut bypass raycast. No findPath, smoothing, normalization,
    /// cold-distance repair, transport or gameplay admission occurs. Supported original
    /// query length is <=4096; sample count <=1026 is a work bound, not a source clamp.
    #[allow(clippy::too_many_arguments)]
    pub fn loaded_source_warm_straight_path(
        &mut self,
        before: &WarmCache,
        from: [u32; 3],
        to: [u32; 3],
        collision_width_bits: u32,
        config: SourcePointPathConfig,
        budget: SourceStraightPathBudget,
        until: Instant,
    ) -> Result<LoadedSourceWarmStraightPath, SourceStraightPathError> {
        #[cfg(benilla_native_detour)]
        {
            native::query(
                self,
                before,
                from,
                to,
                collision_width_bits,
                config,
                budget,
                until,
            )
        }
        #[cfg(not(benilla_native_detour))]
        {
            let _ = (
                before,
                from,
                to,
                collision_width_bits,
                config,
                budget,
                until,
            );
            Err(Error::Unsupported.into())
        }
    }
}
#[cfg(benilla_native_detour)]
mod native {
    use super::*;
    use crate::{
        deadline, detour, warm_cache::native as warm, world, SourceWarmCacheBudget, SUCCESS,
    };
    use std::{ffi::c_void, ptr::NonNull};
    #[repr(C)]
    struct RayResult {
        path: [u64; 74],
        hit: f32,
        normal: [f32; 3],
        count: u32,
        status: u32,
    }
    unsafe extern "C" {
        fn benilla_nav_source_ray_create(
            mesh: *mut c_void,
            start: u64,
            from: *const f32,
            to: *const f32,
            include: u16,
            exclude: u16,
        ) -> *mut c_void;
        fn benilla_source_ray_free(state: *mut c_void);
        fn benilla_source_ray_step(state: *mut c_void) -> u32;
        fn benilla_source_ray_result(state: *const c_void, out: *mut RayResult);
        fn benilla_nav_straight_height(
            mesh: *mut c_void,
            polygon: u64,
            point: *const f32,
            height: *mut f32,
        ) -> u32;
        fn benilla_source_straight_plan(
            from: *const f32,
            to: *const f32,
            max_points: u32,
            steps: *mut u32,
            diff: *mut f32,
        ) -> u32;
        fn benilla_source_straight_hit(
            from: *const f32,
            to: *const f32,
            hit: f32,
            width: f32,
            adjusted: *mut f32,
            point: *mut f32,
        ) -> u32;
        fn benilla_source_straight_hit_distance(from: *const f32, hit: *const f32) -> u32;
        fn benilla_source_straight_advance(previous: *mut f32, diff: *const f32);
        fn benilla_source_straight_terminal(
            from: *const f32,
            end: *const f32,
            count: u32,
            path_type: u32,
            force: u32,
            last: *mut f32,
            actual: *mut f32,
            shortcut: *mut u32,
        ) -> u32;
    }
    struct Ray(NonNull<c_void>);
    impl Drop for Ray {
        fn drop(&mut self) {
            // SAFETY: sole owner, all exits release before loaded mesh borrow ends.
            unsafe { benilla_source_ray_free(self.0.as_ptr()) }
        }
    }
    struct Work {
        budget: SourceStraightPathBudget,
        until: Instant,
        diagnostics: SourceStraightPathDiagnostics,
    }
    impl Work {
        fn primitive(&mut self) -> Result<(), Error> {
            deadline(self.until)?;
            if self.diagnostics.native_primitives >= self.budget.native_primitives {
                return Err(Error::Budget);
            }
            self.diagnostics.native_primitives += 1;
            Ok(())
        }
        fn ray_polygon(&mut self) -> Result<(), Error> {
            deadline(self.until)?;
            if self.diagnostics.raycast_polygons >= self.budget.raycast_polygons {
                return Err(Error::Budget);
            }
            self.diagnostics.raycast_polygons += 1;
            Ok(())
        }
    }
    fn checked(point: [f32; 3]) -> Result<[u32; 3], Error> {
        let bits = point.map(f32::to_bits);
        detour(bits)?;
        Ok(bits)
    }
    fn height(
        mesh: &Mesh,
        polygon: u64,
        point: &mut [f32; 3],
        endpoint: u8,
        work: &mut Work,
    ) -> Result<(), Error> {
        work.primitive()?;
        let before = world(*point);
        let mut z = point[1];
        // SAFETY: reference selected from this mesh; input finite and output initialized.
        // Ground INVALID_PARAM returns without writing, exactly preserving source height.
        let status = unsafe {
            benilla_nav_straight_height(mesh.raw.as_ptr(), polygon, point.as_ptr(), &mut z)
        };
        deadline(work.until)?;
        if status & SUCCESS == 0 && status != (1 << 31 | 1 << 3) {
            return Err(Error::Native(status));
        }
        point[1] = z;
        let after = checked([point[2], point[0], point[1]])?;
        work.diagnostics
            .heights
            .try_reserve(1)
            .map_err(|_| Error::Budget)?;
        work.diagnostics.heights.push(SourceStraightHeight {
            endpoint,
            polygon,
            before,
            after,
            status,
        });
        Ok(())
    }
    fn optional_lookup(
        mesh: &Mesh,
        candidate: &mut WarmCache,
        point: &[f32; 3],
        endpoint: u8,
        work: &mut warm::Work,
    ) -> Result<Option<SourceWarmLookup>, SourceStraightPathError> {
        match warm::lookup(mesh, candidate, point, endpoint, work) {
            Ok(found) => Ok(Some(found)),
            Err(SourceWarmCacheError::MissingPolygon) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn query(
        mesh: &mut Mesh,
        before: &WarmCache,
        from: [u32; 3],
        to: [u32; 3],
        collision_width_bits: u32,
        config: SourcePointPathConfig,
        budget: SourceStraightPathBudget,
        until: Instant,
    ) -> Result<LoadedSourceWarmStraightPath, SourceStraightPathError> {
        deadline(until)?;
        before.check_mesh(mesh)?;
        if before.storage()[..before.active_len()].contains(&0) {
            return Err(Error::Format.into());
        }
        if config.normalize_z && !config.ignore_normalization {
            return Err(SourceStraightPathError::ActiveNormalizationUnsupported);
        }
        if config.previous_path_type & !0x3f != 0 {
            return Err(SourceStraightPathError::InvalidPreviousPathType);
        }
        let width = f32::from_bits(collision_width_bits);
        if !width.is_finite() || width < 0.0 {
            return Err(SourceStraightPathError::InvalidCollisionWidth);
        }
        if budget.lookup_primitives > SOURCE_WARM_CACHE_MAX_LOOKUP_PRIMITIVES
            || budget.raycast_polygons > SOURCE_STRAIGHT_PATH_MAX_RAY_POLYGONS
            || budget.sampled_points > SOURCE_STRAIGHT_PATH_MAX_POINTS
            || budget.native_primitives > SOURCE_STRAIGHT_PATH_MAX_PRIMITIVES
        {
            return Err(Error::Budget.into());
        }
        let (mut selected, mut lookups) = warm::prepare(
            mesh,
            before,
            from,
            to,
            SourceWarmCacheBudget {
                lookup_primitives: budget.lookup_primitives,
                corridor_expansions: 0,
            },
            until,
        )?;
        let mut work = Work {
            budget,
            until,
            diagnostics: SourceStraightPathDiagnostics::default(),
        };
        let start = from.map(f32::from_bits);
        let a = detour(from)?;
        let b = detour(to)?;
        let mut end = to.map(f32::from_bits);
        let mut actual_end = end;
        let mut path_type = config.previous_path_type;
        let mut points = Vec::new();
        let mut ray = None;
        let mut clipped = false;
        let mut build_points = true;
        if selected.phase() == SourceWarmPolyPhase::SamePolygon {
            work.primitive()?;
            points.try_reserve_exact(2).map_err(|_| Error::Budget)?;
            let terminal = crate::warm_point_path::same_polygon_terminal(from, to);
            points.extend_from_slice(&terminal.points);
            actual_end = terminal.actual_end.map(f32::from_bits);
            path_type = terminal.path_type;
            build_points = false;
        } else if selected.phase() == SourceWarmPolyPhase::Replacement {
            work.primitive()?;
            let filter = selected.after.filter();
            // SAFETY: selected nonzero ref and finite positions, immutable validated mesh.
            // Native creation validates again before touching legacy ray output semantics.
            let raw = unsafe {
                benilla_nav_source_ray_create(
                    mesh.raw.as_ptr(),
                    selected.start().polygon,
                    a.as_ptr(),
                    b.as_ptr(),
                    filter.include_flags,
                    filter.exclude_flags,
                )
            };
            let state = NonNull::new(raw).map(Ray);
            deadline(until)?;
            let state = state.ok_or(Error::Budget)?;
            loop {
                work.ray_polygon()?;
                // SAFETY: one original polygon iteration, state remains privately owned.
                let done = unsafe { benilla_source_ray_step(state.0.as_ptr()) };
                deadline(until)?;
                if done == 1 {
                    break;
                }
                if done != 0 {
                    return Err(Error::Format.into());
                }
            }
            work.primitive()?;
            let mut raw = RayResult {
                path: [0; 74],
                hit: 0.0,
                normal: [0.0; 3],
                count: 0,
                status: 0,
            };
            // SAFETY: matching initialized fixed-layout result and completed private state.
            unsafe { benilla_source_ray_result(state.0.as_ptr(), &mut raw) };
            deadline(until)?;
            if raw.count > 74 || !raw.hit.is_finite() || raw.normal.iter().any(|v| !v.is_finite()) {
                return Err(Error::Format.into());
            }
            selected
                .after
                .install_prefix(&raw.path[..raw.count as usize])?;
            let mut polygons = Vec::new();
            polygons
                .try_reserve_exact(raw.count as usize)
                .map_err(|_| Error::Budget)?;
            polygons.extend_from_slice(&raw.path[..raw.count as usize]);
            let mut observed = SourceStraightRay {
                status: raw.status,
                hit_bits: raw.hit.to_bits(),
                normal_bits: raw.normal.map(f32::to_bits),
                polygons,
                adjusted_hit_bits: None,
            };
            if raw.hit != f32::MAX {
                work.primitive()?;
                let mut adjusted = 0.0;
                let mut hit = [0.0; 3];
                // SAFETY: finite inputs and initialized output arrays; native source order.
                let proceed = unsafe {
                    benilla_source_straight_hit(
                        a.as_ptr(),
                        b.as_ptr(),
                        raw.hit,
                        width,
                        &mut adjusted,
                        hit.as_mut_ptr(),
                    )
                };
                deadline(until)?;
                observed.adjusted_hit_bits = Some(adjusted.to_bits());
                path_type = 8;
                build_points = false;
                if proceed == 1 {
                    checked([hit[2], hit[0], hit[1]])?;
                    if let Some(found) =
                        optional_lookup(mesh, &mut selected.after, &hit, 3, &mut lookups)?
                    {
                        height(mesh, found.polygon, &mut hit, 3, &mut work)?;
                        work.primitive()?;
                        // SAFETY: finite initialized vectors, comparison is source double 0.01.
                        let moved = unsafe {
                            benilla_source_straight_hit_distance(start.as_ptr(), hit.as_ptr())
                        };
                        deadline(until)?;
                        if moved == 1 {
                            end = [hit[2], hit[0], hit[1]];
                            actual_end = end;
                            path_type = config.previous_path_type;
                            clipped = true;
                            build_points = true;
                        }
                    }
                }
            } else if selected.after.active_len() == 0 || raw.status & (1 << 31) != 0 {
                points.try_reserve_exact(2).map_err(|_| Error::Budget)?;
                points.extend_from_slice(&[from, to]);
                selected.after.clear_active();
                path_type = 8;
                build_points = false;
            }
            ray = Some(observed);
        }
        if build_points {
            if !clipped {
                path_type = if selected.after.storage()[selected.after.active_len() - 1]
                    == selected.end().polygon
                    && config.previous_path_type & 4 == 0
                {
                    1
                } else {
                    4
                };
            }
            work.primitive()?;
            let mut steps = 0;
            let mut diff = [0.0; 3];
            // SAFETY: finite endpoints; native checks finite magnitude and safe uint cast.
            let plan = unsafe {
                benilla_source_straight_plan(
                    start.as_ptr(),
                    end.as_ptr(),
                    SOURCE_STRAIGHT_PATH_MAX_POINTS as u32,
                    &mut steps,
                    diff.as_mut_ptr(),
                )
            };
            deadline(until)?;
            match plan {
                1 => {
                    let count = steps as usize + 2;
                    if count > budget.sampled_points {
                        return Err(Error::Budget.into());
                    }
                    points.try_reserve_exact(count).map_err(|_| Error::Budget)?;
                    points.push(from);
                    let mut previous = start;
                    for _ in 0..steps {
                        work.primitive()?;
                        // SAFETY: three finite components, prior height is intentionally accumulated.
                        unsafe {
                            benilla_source_straight_advance(previous.as_mut_ptr(), diff.as_ptr())
                        };
                        deadline(until)?;
                        let bits = checked(previous)?;
                        let mut converted = detour(bits)?;
                        if let Some(found) =
                            optional_lookup(mesh, &mut selected.after, &converted, 4, &mut lookups)?
                        {
                            height(mesh, found.polygon, &mut converted, 4, &mut work)?;
                            previous[2] = converted[1];
                        }
                        points.push(checked(previous)?);
                    }
                    points.push(checked(end)?);
                    work.diagnostics.sampled_points = count;
                }
                2 => (),
                _ => return Err(Error::Budget.into()),
            }
            work.primitive()?;
            let mut last = points.last().copied().unwrap_or(to).map(f32::from_bits);
            let mut shortcut = 0;
            // SAFETY: fixed initialized vectors, count is bounded by the supported plan.
            path_type = unsafe {
                benilla_source_straight_terminal(
                    start.as_ptr(),
                    end.as_ptr(),
                    points.len() as u32,
                    path_type,
                    u32::from(config.force_destination),
                    last.as_mut_ptr(),
                    actual_end.as_mut_ptr(),
                    &mut shortcut,
                )
            };
            deadline(until)?;
            if shortcut == 1 {
                selected.after.clear_active();
                points.clear();
                points.try_reserve(2).map_err(|_| Error::Budget)?;
                points.extend_from_slice(&[from, checked(actual_end)?]);
            } else if let Some(point) = points.last_mut() {
                *point = checked(last)?;
            }
            if clipped {
                path_type = 4;
            }
        }
        warm::finish_trace(&mut selected, lookups);
        let mut trace = Vec::new();
        trace
            .try_reserve_exact(selected.trace().len())
            .map_err(|_| Error::Budget)?;
        trace.extend_from_slice(selected.trace());
        let result = LoadedSourceWarmStraightPath {
            from,
            requested_end: to,
            end: checked(end)?,
            actual_end: checked(actual_end)?,
            collision_width_bits,
            config,
            start_lookup: selected.start(),
            end_lookup: selected.end(),
            phase: selected.phase(),
            after: selected.into_after(),
            points,
            path_type,
            ray,
            trace,
            diagnostics: work.diagnostics,
        };
        deadline(until)?;
        Ok(result)
    }
}
