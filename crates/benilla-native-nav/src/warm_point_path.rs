// SPDX-License-Identifier: GPL-3.0-or-later
//! Terminal loaded smooth-calculation dependency, never a Route fact or owner dispatch.
//! Straight/funnel/raycast, playerbots, dungeon limits, transports and active normalization
//! are unrepresented. Chase's earlier straight-line attempt is not implemented here.
use crate::{
    Error, LoadedSourceWarmCache, Mesh, SourceSmoothStop, SourceSmoothingDiagnostics,
    SourceSmoothingError, WarmCache, SOURCE_SMOOTHING_MAX_PRIMITIVES,
};
use std::time::Instant;

/// Common BuildPolyPath same-polygon terminal, before straight/smooth dispatch.
/// Callers establish same polygon, both distances <=7 and inactive NormalizePath.
#[derive(Debug)]
pub(crate) struct SamePolygonTerminal {
    pub(crate) points: [[u32; 3]; 2],
    pub(crate) actual_end: [u32; 3],
    pub(crate) path_type: u32,
}
#[cfg(benilla_native_detour)]
pub(crate) fn same_polygon_terminal(from: [u32; 3], actual_end: [u32; 3]) -> SamePolygonTerminal {
    // PathFinder.cpp:508–519,1005–1020: BuildShortcut then NORMAL, no BuildPointPath.
    SamePolygonTerminal {
        points: [from, actual_end],
        actual_end,
        path_type: 1,
    }
}

pub const SOURCE_WARM_POINT_PATH_POLICY: u32 = 1;
pub const SOURCE_POINT_PATH_MAX_POSTPROCESS_PRIMITIVES: usize = 221;

/// Explicit source calculation state, never silently populated from server defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourcePointPathConfig {
    pub optimize_path: bool,
    pub normalize_z: bool,
    pub ignore_normalization: bool,
    pub force_destination: bool,
    pub previous_path_type: u32,
}
/// At most 74 conversions, 72 optimizer decisions, 74 compaction slots and one terminal
/// decision. These finite native loops run in one call, checked before and after; they do
/// not promise wall-clock preemption. Smoothing retains its per-native-primitive checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourcePointPathBudget {
    pub smoothing_primitives: usize,
    pub postprocess_primitives: usize,
}
impl Default for SourcePointPathBudget {
    fn default() -> Self {
        Self {
            smoothing_primitives: SOURCE_SMOOTHING_MAX_PRIMITIVES,
            postprocess_primitives: SOURCE_POINT_PATH_MAX_POSTPROCESS_PRIMITIVES,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourcePointPathDiagnostics {
    pub smoothing: SourceSmoothingDiagnostics,
    pub postprocess_primitives: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourcePointPathError {
    Boundary(Error),
    Smoothing(SourceSmoothingError),
    ActiveNormalizationUnsupported,
    /// Source skips original zero sentinels while resizing its retained point vector.
    /// That defined prior-vector dependence is absent from this cache, not undefined behavior.
    OriginalZeroPointOptimizationUnsupported,
    InvalidPreviousPathType,
}
impl From<Error> for SourcePointPathError {
    fn from(value: Error) -> Self {
        Self::Boundary(value)
    }
}
impl From<SourceSmoothingError> for SourcePointPathError {
    fn from(value: SourceSmoothingError) -> Self {
        Self::Smoothing(value)
    }
}

/// Immutable terminal proposal, bound to the exact loaded Mesh through opaque cache lineage.
/// It cannot be minted from a public raw smoother result or serialized polygon references.
/// `path_type` is only the source calculation's raw bit mask, never complete-route authority.
///
/// ```compile_fail
/// use benilla_native_nav::{LoadedSourceSmooth, LoadedSourceWarmPointPath};
/// fn promote(raw: LoadedSourceSmooth) -> LoadedSourceWarmPointPath { raw.into() }
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct LoadedSourceWarmPointPath {
    warm: LoadedSourceWarmCache,
    config: SourcePointPathConfig,
    after: WarmCache,
    points: Vec<[u32; 3]>,
    actual_end: [u32; 3],
    path_type: u32,
    smooth_points: Vec<[u32; 3]>,
    smooth_status: Option<u32>,
    smooth_stop: Option<SourceSmoothStop>,
    diagnostics: SourcePointPathDiagnostics,
}
impl LoadedSourceWarmPointPath {
    pub fn warm(&self) -> &LoadedSourceWarmCache {
        &self.warm
    }
    pub fn config(&self) -> SourcePointPathConfig {
        self.config
    }
    pub fn after(&self) -> &WarmCache {
        &self.after
    }
    pub fn into_after(self) -> WarmCache {
        self.after
    }
    /// Source owner XYZ coordinate bits, after optimization and terminal side effects.
    pub fn points(&self) -> &[[u32; 3]] {
        &self.points
    }
    /// Captured before source normalization, which is inactive in this supported domain.
    pub fn actual_end(&self) -> [u32; 3] {
        self.actual_end
    }
    pub fn path_type(&self) -> u32 {
        self.path_type
    }
    /// Raw source smoother XYZ diagnostic points. SamePolygon does not invoke smoothing.
    pub fn smooth_points(&self) -> &[[u32; 3]] {
        &self.smooth_points
    }
    pub fn smooth_status(&self) -> Option<u32> {
        self.smooth_status
    }
    pub fn smooth_stop(&self) -> Option<SourceSmoothStop> {
        self.smooth_stop
    }
    pub fn diagnostics(&self) -> &SourcePointPathDiagnostics {
        &self.diagnostics
    }
}
impl Mesh {
    /// Complete only the supported warm smooth-calculation branch using its selected refs.
    /// No lookup, direct search or cold-owner policy runs here. The prior warm proposal and
    /// mesh remain unchanged on all outcomes; the terminal cache is a separate proposal.
    /// This does not model Chase dispatch or a Home producer (Home creates a fresh finder).
    pub fn loaded_source_warm_point_path(
        &mut self,
        warm: &LoadedSourceWarmCache,
        config: SourcePointPathConfig,
        budget: SourcePointPathBudget,
        until: Instant,
    ) -> Result<LoadedSourceWarmPointPath, SourcePointPathError> {
        #[cfg(benilla_native_detour)]
        {
            native::query(self, warm, config, budget, until)
        }
        #[cfg(not(benilla_native_detour))]
        {
            let _ = (warm, config, budget, until);
            Err(Error::Unsupported.into())
        }
    }
}

#[cfg(benilla_native_detour)]
mod native {
    use super::*;
    use crate::{deadline, detour, smoothing, SourceWarmPolyPhase};

    #[repr(C)]
    struct NativeResult {
        points: [[f32; 3]; 74],
        actual_end: [f32; 3],
        point_count: u32,
        path_type: u32,
        clear_cache: u32,
    }
    unsafe extern "C" {
        fn benilla_source_point_path(
            yzx: *const f32,
            count: u32,
            limit: u32,
            status: u32,
            path_type: u32,
            from: *const f32,
            to: *const f32,
            optimize: u32,
            force: u32,
            result: *mut NativeResult,
        ) -> u32;
    }
    pub(super) fn query(
        mesh: &Mesh,
        warm: &LoadedSourceWarmCache,
        config: SourcePointPathConfig,
        budget: SourcePointPathBudget,
        until: Instant,
    ) -> Result<LoadedSourceWarmPointPath, SourcePointPathError> {
        deadline(until)?;
        warm.after().check_mesh(mesh)?;
        if config.normalize_z && !config.ignore_normalization {
            return Err(SourcePointPathError::ActiveNormalizationUnsupported);
        }
        if config.previous_path_type & !0x3f != 0 {
            return Err(SourcePointPathError::InvalidPreviousPathType);
        }
        if budget.smoothing_primitives > SOURCE_SMOOTHING_MAX_PRIMITIVES
            || budget.postprocess_primitives > SOURCE_POINT_PATH_MAX_POSTPROCESS_PRIMITIVES
        {
            return Err(Error::Budget.into());
        }
        // A proposal with this lineage was produced only by the bounded warm lookup path.
        // Fallible copies preserve the caller's proposal even after terminal clear().
        let mut after = warm.after().try_copy()?;
        let mut points = Vec::new();
        points.try_reserve_exact(74).map_err(|_| Error::Budget)?;
        deadline(until)?;
        let (actual_end, path_type, smooth_points, smooth_status, smooth_stop, diagnostics) =
            if warm.phase() == SourceWarmPolyPhase::SamePolygon {
                if budget.postprocess_primitives < 1 {
                    return Err(Error::Budget.into());
                }
                // BuildShortcut clears then reinstalls slot zero/active_len=1. Warm selection
                // already made that exact change, retaining all inactive storage and tail.
                let terminal = same_polygon_terminal(warm.from(), warm.to());
                points.extend_from_slice(&terminal.points);
                (
                    terminal.actual_end,
                    terminal.path_type,
                    Vec::new(),
                    None,
                    None,
                    SourcePointPathDiagnostics {
                        smoothing: SourceSmoothingDiagnostics::default(),
                        postprocess_primitives: 1,
                    },
                )
            } else {
                // Source BuildPolyPath 786-789 preserves prior INCOMPLETE even at endPoly.
                let classified = if after.storage()[after.active_len() - 1] == warm.end().polygon
                    && config.previous_path_type & 4 == 0
                {
                    1
                } else {
                    4
                };
                let raw = smoothing::smooth_warm(mesh, warm, budget.smoothing_primitives, until)?;
                let count = raw.points.len();
                let work = count
                    + 1
                    + if config.optimize_path && count > 2 {
                        count - 2 + count
                    } else {
                        0
                    };
                if work > budget.postprocess_primitives {
                    return Err(Error::Budget.into());
                }
                let mut yzx = [[0.0; 3]; 74];
                for (out, point) in yzx.iter_mut().zip(&raw.points) {
                    *out = detour(*point)?;
                }
                let mut terminal = NativeResult {
                    points: [[0.0; 3]; 74],
                    actual_end: [0.0; 3],
                    point_count: 0,
                    path_type: 0,
                    clear_cache: 0,
                };
                deadline(until)?;
                // SAFETY: private raw smoother produced <=74 finite points from this mesh;
                // initialized fixed arrays match the C ABI. All bounded loops are charged
                // before this call, and source arithmetic never retains caller pointers.
                let outcome = unsafe {
                    benilla_source_point_path(
                        yzx.as_ptr().cast(),
                        count as u32,
                        74,
                        raw.status,
                        classified,
                        warm.from().map(f32::from_bits).as_ptr(),
                        warm.to().map(f32::from_bits).as_ptr(),
                        u32::from(config.optimize_path),
                        u32::from(config.force_destination),
                        &mut terminal,
                    )
                };
                deadline(until)?;
                match outcome {
                    1 => (),
                    2 => {
                        return Err(SourcePointPathError::OriginalZeroPointOptimizationUnsupported)
                    }
                    _ => return Err(Error::Format.into()),
                }
                if !(2..=74).contains(&terminal.point_count)
                    || terminal.clear_cache > 1
                    || !matches!(terminal.path_type, 1 | 4 | 8 | 17 | 32)
                {
                    return Err(Error::Format.into());
                }
                for p in &terminal.points[..terminal.point_count as usize] {
                    points.push(checked_bits(*p)?);
                }
                if terminal.clear_cache == 1 {
                    after.clear_active();
                }
                (
                    checked_bits(terminal.actual_end)?,
                    terminal.path_type,
                    raw.points,
                    Some(raw.status),
                    Some(raw.stop),
                    SourcePointPathDiagnostics {
                        smoothing: raw.diagnostics,
                        postprocess_primitives: work,
                    },
                )
            };
        let warm = warm.try_copy()?;
        deadline(until)?;
        Ok(LoadedSourceWarmPointPath {
            warm,
            config,
            after,
            points,
            actual_end,
            path_type,
            smooth_points,
            smooth_status,
            smooth_stop,
            diagnostics,
        })
    }
    fn checked_bits(point: [f32; 3]) -> Result<[u32; 3], Error> {
        if point.iter().any(|v| !v.is_finite() || v.abs() > 100_000.) {
            return Err(Error::Format);
        }
        Ok(point.map(f32::to_bits))
    }
}
