// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only detail-triangle attribution for height calls in one genuine terminal.
//! This witnesses neither terrain support nor generator settings nor Route authority.
use super::LoadedSourceRetainedStraight;
use crate::{Error, SelectedSourceQuery, SourceMapQuery, SourceStraightHeight};
use std::{rc::Rc, time::Instant};

/// Reuses the bounded straight producer's 1026-point operational domain, not
/// source's equality-only limit 74. At most one clipped-hit plus one call per
/// interior sample fits under this conservative record cap.
pub const SOURCE_RETAINED_DETAIL_MAX_HEIGHT_RECORDS: usize = crate::SOURCE_STRAIGHT_PATH_MAX_POINTS;
pub const SOURCE_RETAINED_DETAIL_MAX_TRIANGLE_TESTS: usize = 4096;

/// A test is one dtClosestHeightPointTriangle invocation. Successful attribution
/// charges both its capture scan and the original query's repeated scan. Each
/// native call has at most 255 triangles; deadlines are checked on both sides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceRetainedDetailBudget {
    pub height_records: usize,
    pub triangle_tests: usize,
}
impl Default for SourceRetainedDetailBudget {
    fn default() -> Self {
        Self {
            height_records: SOURCE_RETAINED_DETAIL_MAX_HEIGHT_RECORDS,
            triangle_tests: SOURCE_RETAINED_DETAIL_MAX_TRIANGLE_TESTS,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceRetainedDetailUnsupported {
    RecordedStatus,
    OffMesh,
    OutsidePolygon,
    MissingDetail,
    EdgeFallback,
    Nonfinite,
    QueryStatus,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceRetainedDetailError {
    Boundary(Error),
    Unsupported {
        record: usize,
        reason: SourceRetainedDetailUnsupported,
    },
    HeightMismatch {
        record: usize,
    },
}
impl From<Error> for SourceRetainedDetailError {
    fn from(value: Error) -> Self {
        Self::Boundary(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceDetailVertexSource {
    Polygon,
    Detail,
}
/// Read-only observation, not a reconstructible loaded witness.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceDetailVertex {
    pub source: SourceDetailVertexSource,
    /// Index in this tile's verts or detailVerts array, according to source.
    pub tile_index: u32,
    /// Original Detour YZX coordinates, not world XYZ.
    pub detour_bits: [u32; 3],
}
/// One actual recorded call, matched to the first successful detail triangle in
/// the original order. Header bounds/agent metrics do not establish cs/ch, source
/// extraction identity, raw terrain, or the height of any unsampled point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceRetainedDetailHeight {
    pub recorded: SourceStraightHeight,
    pub query_status: u32,
    pub query_height_bits: u32,
    /// Actual loaded tile reference, including its salt/allocation-local slot.
    pub tile_reference: u64,
    pub tile_coordinates: [i32; 3],
    pub tile_bounds_min: [u32; 3],
    pub tile_bounds_max: [u32; 3],
    /// walkableHeight, walkableRadius, walkableClimb in that order.
    pub tile_agent_bits: [u32; 3],
    pub polygon_index: u32,
    pub polygon_vertex_count: u32,
    pub detail_triangle_count: u32,
    pub triangle_index: u32,
    /// Three local vertex indexes followed by original edge flags.
    pub triangle: [u8; 4],
    pub vertices: [SourceDetailVertex; 3],
}

/// Sealed attribution to an exact terminal and its retained query allocation.
/// Empty records mean that this terminal issued no height calls, not that its
/// points have any detail-height or continuous support proof. Only the query
/// and terminal token are retained; the terminal and its ancestors can drop.
///
/// ```compile_fail
/// use benilla_native_nav::{LoadedSourceRetainedStraightDetail, SourceRetainedDetailHeight};
/// fn forge(records: Vec<SourceRetainedDetailHeight>) -> LoadedSourceRetainedStraightDetail {
///     LoadedSourceRetainedStraightDetail::from(records)
/// }
/// ```
///
/// ```compile_fail
/// use benilla_native_nav::{LoadedSourceRetainedStraightDetail, LoadedSourceRetainedStraight};
/// fn authority(witness: LoadedSourceRetainedStraightDetail) -> LoadedSourceRetainedStraight {
///     witness.into()
/// }
/// ```
#[derive(Debug)]
pub struct LoadedSourceRetainedStraightDetail {
    query: SourceMapQuery,
    terminal: Rc<()>,
    from: [u32; 3],
    to: [u32; 3],
    records: Vec<SourceRetainedDetailHeight>,
    triangle_tests: usize,
}
impl LoadedSourceRetainedStraightDetail {
    pub fn check_terminal(&self, terminal: &LoadedSourceRetainedStraight) -> Result<(), Error> {
        if !Rc::ptr_eq(&self.terminal, &terminal.identity) {
            return Err(Error::Identity);
        }
        Ok(())
    }
    pub fn check_query(&self, selected: &SelectedSourceQuery<'_>) -> Result<(), Error> {
        selected.check_allocation(&self.query)?;
        if self.from != selected.from() || self.to != selected.to() {
            return Err(Error::Identity);
        }
        Ok(())
    }
    pub fn records(&self) -> &[SourceRetainedDetailHeight] {
        &self.records
    }
    pub fn triangle_tests(&self) -> usize {
        self.triangle_tests
    }
    pub fn source_revision(&self) -> &'static str {
        crate::SOURCE_COLD_DISTANCE_COMPATIBILITY_REVISION
    }
}

impl LoadedSourceRetainedStraight {
    /// Attribute only this terminal's already-recorded height calls. The caller
    /// cannot supply observations, references, status, or points as authority.
    pub fn detail_height_witness(
        &self,
        selected: &SelectedSourceQuery<'_>,
        budget: SourceRetainedDetailBudget,
        until: Instant,
    ) -> Result<LoadedSourceRetainedStraightDetail, SourceRetainedDetailError> {
        crate::deadline(until)?;
        self.check_query(selected)?;
        if budget.height_records > SOURCE_RETAINED_DETAIL_MAX_HEIGHT_RECORDS
            || budget.triangle_tests > SOURCE_RETAINED_DETAIL_MAX_TRIANGLE_TESTS
            || self.diagnostics().heights.len() > budget.height_records
            || self
                .diagnostics()
                .heights
                .len()
                .checked_mul(2)
                .ok_or(Error::Budget)?
                > budget.triangle_tests
        {
            return Err(Error::Budget.into());
        }
        #[cfg(benilla_native_detour)]
        {
            native::witness(self, selected, budget, until)
        }
        #[cfg(not(benilla_native_detour))]
        {
            Err(Error::Unsupported.into())
        }
    }
}

#[cfg(benilla_native_detour)]
mod native {
    use super::*;
    #[repr(C)]
    #[derive(Default)]
    struct Detail {
        status: u32,
        height: u32,
        triangle_index: u32,
        triangle_count: u32,
        triangle_tests: u32,
        polygon_index: u32,
        polygon_vertex_count: u32,
        triangle: [u8; 4],
        vertex_indices: [u32; 3],
        vertices: [[u32; 3]; 3],
        tile_coordinates: [i32; 3],
        bounds_min: [u32; 3],
        bounds_max: [u32; 3],
        agent: [u32; 3],
        tile_reference: u64,
    }
    const _: () = assert!(std::mem::size_of::<Detail>() == 136);
    unsafe extern "C" {
        fn benilla_nav_retained_detail_height(
            raw: *const std::ffi::c_void,
            polygon: u64,
            point: *const f32,
            max_triangle_tests: u32,
            out: *mut Detail,
        ) -> u32;
    }
    pub(super) fn witness(
        terminal: &LoadedSourceRetainedStraight,
        selected: &SelectedSourceQuery<'_>,
        budget: SourceRetainedDetailBudget,
        until: Instant,
    ) -> Result<LoadedSourceRetainedStraightDetail, SourceRetainedDetailError> {
        let mesh = selected.borrow_mesh()?;
        if mesh.query_profile() != crate::QueryProfile::SourceMapInstance1024
            || mesh.query_capacity() != 1024
        {
            return Err(Error::Identity.into());
        }
        let mut records = Vec::new();
        records
            .try_reserve_exact(terminal.diagnostics().heights.len())
            .map_err(|_| Error::Budget)?;
        let mut tests = 0;
        for (index, recorded) in terminal.diagnostics().heights.iter().copied().enumerate() {
            crate::deadline(until)?;
            if recorded.status != crate::SUCCESS {
                return Err(SourceRetainedDetailError::Unsupported {
                    record: index,
                    reason: SourceRetainedDetailUnsupported::RecordedStatus,
                });
            }
            if !matches!(recorded.endpoint, 3 | 4)
                || recorded.polygon == 0
                || recorded.before[..2] != recorded.after[..2]
            {
                return Err(Error::Identity.into());
            }
            let point = crate::detour(recorded.before)?;
            let mut detail = Detail::default();
            // SAFETY: immutable validated loaded mesh and genuine terminal record;
            // fixed ABI output and finite triangle budget. Native helper is read-only.
            let result = unsafe {
                benilla_nav_retained_detail_height(
                    mesh.raw.as_ptr(),
                    recorded.polygon,
                    point.as_ptr(),
                    (budget.triangle_tests - tests) as u32,
                    &mut detail,
                )
            };
            crate::deadline(until)?;
            let reason = match result {
                1 => None,
                0 => return Err(Error::Unavailable.into()),
                2 => return Err(Error::Format.into()),
                3 => Some(SourceRetainedDetailUnsupported::OffMesh),
                4 => Some(SourceRetainedDetailUnsupported::OutsidePolygon),
                5 => Some(SourceRetainedDetailUnsupported::MissingDetail),
                6 => Some(SourceRetainedDetailUnsupported::EdgeFallback),
                7 => return Err(Error::Budget.into()),
                8 => Some(SourceRetainedDetailUnsupported::QueryStatus),
                9 => Some(SourceRetainedDetailUnsupported::Nonfinite),
                10 => return Err(SourceRetainedDetailError::HeightMismatch { record: index }),
                _ => return Err(Error::Format.into()),
            };
            if let Some(reason) = reason {
                return Err(SourceRetainedDetailError::Unsupported {
                    record: index,
                    reason,
                });
            }
            if detail.status != recorded.status || detail.height != recorded.after[2] {
                return Err(SourceRetainedDetailError::HeightMismatch { record: index });
            }
            let charged = detail.triangle_tests as usize;
            let expected_tests = (detail.triangle_index as usize)
                .checked_add(1)
                .and_then(|count| count.checked_mul(2))
                .ok_or(Error::Budget)?;
            if charged != expected_tests
                || detail.triangle_index >= detail.triangle_count
                || charged > budget.triangle_tests - tests
            {
                return Err(Error::Format.into());
            }
            tests = tests.checked_add(charged).ok_or(Error::Budget)?;
            records.push(SourceRetainedDetailHeight {
                recorded,
                query_status: detail.status,
                query_height_bits: detail.height,
                tile_reference: detail.tile_reference,
                tile_coordinates: detail.tile_coordinates,
                tile_bounds_min: detail.bounds_min,
                tile_bounds_max: detail.bounds_max,
                tile_agent_bits: detail.agent,
                polygon_index: detail.polygon_index,
                polygon_vertex_count: detail.polygon_vertex_count,
                detail_triangle_count: detail.triangle_count,
                triangle_index: detail.triangle_index,
                triangle: detail.triangle,
                vertices: std::array::from_fn(|i| SourceDetailVertex {
                    source: if u32::from(detail.triangle[i]) < detail.polygon_vertex_count {
                        SourceDetailVertexSource::Polygon
                    } else {
                        SourceDetailVertexSource::Detail
                    },
                    tile_index: detail.vertex_indices[i],
                    detour_bits: detail.vertices[i],
                }),
            });
        }
        crate::deadline(until)?;
        Ok(LoadedSourceRetainedStraightDetail {
            query: selected.retain_allocation(),
            terminal: Rc::clone(&terminal.identity),
            from: terminal.from(),
            to: terminal.requested_end(),
            records,
            triangle_tests: tests,
        })
    }
}

#[cfg(all(test, benilla_native_detour))]
#[path = "source_retained_straight_detail_tests.rs"]
mod tests;
