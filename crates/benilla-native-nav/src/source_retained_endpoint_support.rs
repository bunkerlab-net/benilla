// SPDX-License-Identifier: GPL-3.0-or-later
//! Supplemental current-query endpoint attribution, not original path height calls.
use super::*;

/// This separate probe domain never consumes or invents path height records. Its
/// triangle ceiling is shared by both endpoints, not renewed for the second one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceRetainedEndpointBudget {
    pub endpoints: usize,
    pub triangle_tests: usize,
}
impl Default for SourceRetainedEndpointBudget {
    fn default() -> Self {
        Self {
            endpoints: 2,
            triangle_tests: SOURCE_RETAINED_DETAIL_MAX_TRIANGLE_TESTS,
        }
    }
}

/// A supplemental read-only getPolyHeight result. `point` preserves the actual
/// native endpoint Z; query_height_bits is independent and never replaces it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceRetainedEndpointHeight {
    pub point: [u32; 3],
    pub polygon: u64,
    pub query_status: u32,
    pub query_height_bits: u32,
    pub tile_reference: u64,
    pub tile_coordinates: [i32; 3],
    pub tile_bounds_min: [u32; 3],
    pub tile_bounds_max: [u32; 3],
    pub tile_agent_bits: [u32; 3],
    pub polygon_index: u32,
    pub polygon_vertex_count: u32,
    pub detail_triangle_count: u32,
    pub triangle_index: u32,
    pub triangle: [u8; 4],
    pub vertices: [SourceDetailVertex; 3],
}

/// Current terminal/query identity, retained without predecessor chains. Probes
/// establish endpoint attribution only, never continuous terrain/body support.
///
/// ```compile_fail
/// use benilla_native_nav::{LoadedSourceRetainedEndpointSupport, SourceRetainedEndpointHeight};
/// fn forge(probes: [SourceRetainedEndpointHeight; 2]) -> LoadedSourceRetainedEndpointSupport {
///     LoadedSourceRetainedEndpointSupport::from(probes)
/// }
/// ```
///
/// ```compile_fail
/// use benilla_native_nav::{LoadedSourceRetainedEndpointSupport, LoadedSourceRetainedStraightDetail};
/// fn original(witness: LoadedSourceRetainedEndpointSupport) -> LoadedSourceRetainedStraightDetail {
///     witness.into()
/// }
/// ```
#[derive(Debug)]
pub struct LoadedSourceRetainedEndpointSupport {
    pub(super) query: SourceMapQuery,
    pub(super) terminal: Rc<()>,
    pub(super) from: [u32; 3],
    pub(super) to: [u32; 3],
    pub(super) probes: [SourceRetainedEndpointHeight; 2],
    pub(super) triangle_tests: usize,
}
impl LoadedSourceRetainedEndpointSupport {
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
    pub fn probes(&self) -> &[SourceRetainedEndpointHeight; 2] {
        &self.probes
    }
    pub fn triangle_tests(&self) -> usize {
        self.triangle_tests
    }
    pub fn source_revision(&self) -> &'static str {
        crate::SOURCE_COLD_DISTANCE_COMPATIBILITY_REVISION
    }
}

impl LoadedSourceRetainedStraight {
    /// Probe the exact two original endpoints without changing the path, cache,
    /// or its empty original-height diagnostics. No caller-supplied coordinates
    /// or polygon references can become authority through this constructor.
    pub fn endpoint_support_witness(
        &self,
        selected: &SelectedSourceQuery<'_>,
        budget: SourceRetainedEndpointBudget,
        until: Instant,
    ) -> Result<LoadedSourceRetainedEndpointSupport, SourceRetainedDetailError> {
        crate::deadline(until)?;
        self.check_query(selected)?;
        if budget.endpoints != 2
            || !(4..=SOURCE_RETAINED_DETAIL_MAX_TRIANGLE_TESTS).contains(&budget.triangle_tests)
        {
            return Err(Error::Budget.into());
        }
        let polygon = self.start_lookup().polygon;
        if self.phase() != crate::SourceWarmPolyPhase::SamePolygon
            || self.points() != [self.from(), self.requested_end()]
            || self.end() != self.requested_end()
            || self.actual_end() != self.requested_end()
            || self.path_type() != 1
            || self.active_len() != 1
            || polygon == 0
            || self.end_lookup().polygon != polygon
            || self.storage().first() != Some(&polygon)
            || self.ray().is_some()
            || !self.diagnostics().heights.is_empty()
            || self.diagnostics().sampled_points != 0
        {
            return Err(Error::Unsupported.into());
        }
        #[cfg(benilla_native_detour)]
        {
            super::native::endpoint_witness(self, selected, budget, until)
        }
        #[cfg(not(benilla_native_detour))]
        {
            Err(Error::Unsupported.into())
        }
    }
}

#[cfg(all(test, benilla_native_detour))]
#[path = "source_retained_endpoint_support_tests.rs"]
mod tests;
