// SPDX-License-Identifier: GPL-3.0-or-later
//! A single exact NO_WATER observation, never a dry tile/map/segment capability.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PointNoWaterRequest {
    pub binding: Binding,
    pub ticket: u64,
    pub position: [u32; 3],
    /// Supported source owner predicates: ALL_LIQUIDS=15, WATER|OCEAN=10.
    pub mask: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PointNoWaterReply {
    request: PointNoWaterRequest,
    ground_height: u32,
    location: Option<vmap::LocationFloor>,
}
impl PointNoWaterReply {
    pub(crate) fn request(&self) -> PointNoWaterRequest {
        self.request
    }
    pub(crate) fn initial_static_height(&self) -> u32 {
        self.ground_height
    }
    pub(crate) fn effective_floor(&self) -> u32 {
        self.location
            .map_or(self.ground_height, |location| location.floor)
    }
    pub(crate) fn location(&self) -> Option<vmap::LocationFloor> {
        self.location
    }
}
impl PreparedSourceHeight<'_, '_, '_> {
    /// Sequential freshness checks surround a source-ordered point observation.
    /// Synchronous native decode/source reads are not an atomic live source lease.
    /// VMap height calculation is enabled in this narrowly supported configuration.
    pub(crate) fn point_no_water(
        &self,
        request: PointNoWaterRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<PointNoWaterReply, CompositionError> {
        self.point_no_water_inner(request, deadline, cancelled, |_| {})
    }
    #[cfg(test)]
    pub(crate) fn point_no_water_observed(
        &self,
        request: PointNoWaterRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        observe: impl FnMut(u8),
    ) -> Result<PointNoWaterReply, CompositionError> {
        self.point_no_water_inner(request, deadline, cancelled, observe)
    }
    fn point_no_water_inner(
        &self,
        request: PointNoWaterRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        mut observe: impl FnMut(u8),
    ) -> Result<PointNoWaterReply, CompositionError> {
        let poll = || -> Result<(), CompositionError> {
            check(&cancelled)?;
            if Instant::now() >= deadline {
                return Err(Error::Deadline.into());
            }
            Ok(())
        };
        poll()?;
        if request.binding != self.binding || request.ticket == 0 {
            return Err(Error::Identity.into());
        }
        if !matches!(request.mask, 15 | 10) {
            return Err(Error::Unsupported.into());
        }
        self.verify_unchanged(&cancelled)?;
        poll()?;
        let mut budget = vmap::QueryBudget::new(deadline);
        // TerrainInfo::getLiquidStatus always computes GetHeightStatic first.
        observe(0);
        let height = self.query_with_budget(
            HeightRequest {
                binding: request.binding,
                ticket: request.ticket,
                position: request.position,
                max_search: 50.0f32.to_bits(),
            },
            &mut budget,
        )?;
        poll()?;
        observe(1);
        let p = request.position.map(f32::from_bits);
        let location = self.collision.no_liquid_with_budget(p, &mut budget)?;
        poll()?;
        // False GetLiquidLevel falls through to GridMap. A liquid-bearing tile is
        // deliberately outside this proof, even if its rectangle misses this point.
        observe(2);
        let tile = self
            .tiles
            .get(&world_tile(p[0], p[1])?)
            .ok_or(Error::Missing)?;
        if tile.has_liquid() {
            return Err(Error::Unsupported.into());
        }
        budget.complete()?;
        self.verify_unchanged(&cancelled)?;
        poll()?;
        Ok(PointNoWaterReply {
            request,
            ground_height: height.height,
            location,
        })
    }
}
