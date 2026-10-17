// SPDX-License-Identifier: GPL-3.0-or-later
//! Loaded, enabled VMapManager2::getHeight; not composite terrain or body support.
use super::{
    to_internal, Error, HeightReply, HeightRequest, QueryBudget, Ray, StaticRay, VmapSnapshot,
};
use std::time::Instant;

const INVALID_HEIGHT: f32 = -200_000.0; // IVMapManager.h: VMAP_INVALID_HEIGHT_VALUE
const MAX_SEARCH: f32 = 10_000.0; // Includes TerrainInfo::GetHeightStatic's fallback.

impl VmapSnapshot {
    pub(crate) fn query_height(
        &self,
        request: HeightRequest,
        deadline: Instant,
    ) -> Result<HeightReply, Error> {
        if request.snapshot != self.binding || request.ticket == 0 {
            return Err(Error::Identity);
        }
        let mut budget = QueryBudget::new(deadline);
        let height = self.height(
            request.position.map(f32::from_bits),
            f32::from_bits(request.max_search),
            &mut budget,
        )?;
        budget.complete()?;
        Ok(HeightReply {
            request,
            height: height.to_bits(),
        })
    }

    /// A caller composing multiple height probes must reuse this work/deadline budget.
    /// Out-of-profile or incomplete queries are errors, never the no-height sentinel.
    pub(in super::super) fn height(
        &self,
        position: [f32; 3],
        max_search: f32,
        budget: &mut QueryBudget,
    ) -> Result<f32, Error> {
        if position.iter().any(|v| !v.is_finite()) || !max_search.is_finite() {
            return Err(Error::Format);
        }
        if position.iter().any(|v| v.abs() > 100_000.0) || max_search.abs() > MAX_SEARCH {
            return Err(Error::Unsupported);
        }
        budget.take()?;
        let origin = to_internal(position);
        // StaticMapTree::getHeight constructs an exact unit vertical ray. In
        // particular -0 searches down; neither normalization nor segment caps apply.
        let down = max_search >= 0.0;
        let ray = Ray {
            origin,
            direction: [0.0, 0.0, if down { -1.0 } else { 1.0 }],
        };
        let mut distance = max_search.abs();
        // Source getIntersectionTime defaults to nearest hit and ignoreM2Model=false.
        let result = self.intersect_internal(ray, &mut distance, false, budget)?;
        let height = match result {
            StaticRay::Hit { .. } if down => origin[2] - distance,
            StaticRay::Hit { .. } => origin[2] + distance,
            StaticRay::Miss => f32::INFINITY,
        };
        // VMapManager2 maps the tree's positive-infinity/NaN result to its sentinel.
        let height = if height < f32::INFINITY {
            height
        } else {
            INVALID_HEIGHT
        };
        budget.complete()?;
        Ok(height)
    }
}

#[cfg(test)]
mod tests;
