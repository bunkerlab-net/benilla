// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded TerrainInfo::getLiquidStatus with complete dry static VMaps enabled.
//! Source selection over loaded bytes, not cached player water state or admission.
use super::{
    height,
    liquid::world_tile,
    terrain::{AreaTable, GridLiquidResult, GridLiquidStatus, LiquidStores, LiquidTypes},
    vmap::{QueryBudget, StaticLocation, VmapSnapshot},
    Error, Hash, InventoriedProducts,
};
use sha2::{Digest, Sha256};
use std::time::Instant;

const DEFAULT_WATER_SEARCH: f32 = 50.0;
const POLICY: &[u8] = b"cmangos-8ec338a1-terrain-liquid-complete-dry-vmap-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SnapshotBinding {
    pub map: u32,
    pub content: Hash,
    pub source_epoch: Hash,
    pub inventory: Hash,
    pub policy: Hash,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TerrainLiquidRequest {
    pub snapshot: SnapshotBinding,
    pub ticket: u64,
    /// Exact source-world binary32 pose; never a caller-authored spatial predicate.
    pub position: [u32; 3],
    pub required_type: u8,
    pub collision_height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TerrainLiquidReply {
    pub request: TerrainLiquidRequest,
    /// Surviving raw GridMap status/data, including its original depth_level bits.
    /// NoWater has no data: the source leaves the caller's output untouched.
    pub result: GridLiquidResult,
}

#[derive(Debug)]
pub(crate) struct TerrainLiquidSnapshot<'a> {
    binding: SnapshotBinding,
    products: &'a InventoriedProducts,
    vmap: VmapSnapshot,
    types: LiquidTypes,
    areas: Option<AreaTable>,
}

impl InventoriedProducts {
    /// Complete actual DBC bytes are decoded and hash-bound. Missing AreaTable is
    /// unsupported only when the raw GridMap branch needs that store.
    pub(crate) fn load_terrain_liquid(
        &self,
        liquid_type_dbc: &[u8],
    ) -> Result<TerrainLiquidSnapshot<'_>, Error> {
        self.terrain_liquid_snapshot(LiquidTypes::decode(liquid_type_dbc)?, None)
    }

    pub(crate) fn load_terrain_liquid_with_areas(
        &self,
        liquid_type_dbc: &[u8],
        area_table_dbc: &[u8],
    ) -> Result<TerrainLiquidSnapshot<'_>, Error> {
        self.terrain_liquid_snapshot(
            LiquidTypes::decode(liquid_type_dbc)?,
            Some(AreaTable::decode(area_table_dbc)?),
        )
    }

    fn terrain_liquid_snapshot(
        &self,
        types: LiquidTypes,
        areas: Option<AreaTable>,
    ) -> Result<TerrainLiquidSnapshot<'_>, Error> {
        let vmap = self.load_static_vmap()?;
        vmap.require_complete_dry_map()?;
        let source = vmap.binding();
        let mut policy = Sha256::new();
        policy.update(POLICY);
        policy.update(height::POLICY);
        policy.update(source.policy);
        policy.update(vmap.location_binding().policy);
        policy.update(types.digest());
        policy.update([u8::from(areas.is_some())]);
        if let Some(areas) = &areas {
            policy.update(areas.digest());
        }
        Ok(TerrainLiquidSnapshot {
            binding: SnapshotBinding {
                map: source.map,
                content: source.content,
                source_epoch: source.source_epoch,
                inventory: source.inventory,
                policy: policy.finalize().into(),
            },
            products: self,
            vmap,
            types,
            areas,
        })
    }
}

impl TerrainLiquidSnapshot<'_> {
    pub(crate) fn binding(&self) -> SnapshotBinding {
        self.binding
    }

    pub(crate) fn query(
        &self,
        request: TerrainLiquidRequest,
        deadline: Instant,
    ) -> Result<TerrainLiquidReply, Error> {
        self.query_with_budget(request, &mut QueryBudget::new(deadline))
    }

    fn query_with_budget(
        &self,
        request: TerrainLiquidRequest,
        budget: &mut QueryBudget,
    ) -> Result<TerrainLiquidReply, Error> {
        if request.snapshot != self.binding || request.ticket == 0 {
            return Err(Error::Identity);
        }
        budget.take()?;
        let p = request.position.map(f32::from_bits);
        let collision_height = f32::from_bits(request.collision_height);
        if !collision_height.is_finite() {
            return Err(Error::Format);
        }
        // GridMap.cpp:1059 runs this before even a dry GridMap's early return.
        let mut ground =
            height::height(self.products, &self.vmap, p, DEFAULT_WATER_SEARCH, budget)?;
        // VMapManager2::GetLiquidLevel mutates floor before its mask/dry failure.
        // Location uses original z, whereas static height starts its rays at z+2.
        if let StaticLocation::Floor(location) = self.vmap.location(p, budget)? {
            ground = f32::from_bits(location.floor);
        }
        // Every loaded group is dry, so GetLiquidLevel is false and the source
        // takes the GridMap branch. Do not shortcut its DBC/area dependencies.
        budget.take()?;
        let tile = self
            .products
            .terrain(self.binding.map, world_tile(p[0], p[1])?)?;
        let map_result = tile.grid_liquid_status_with_stores(
            p[0],
            p[1],
            p[2],
            request.required_type,
            collision_height,
            LiquidStores {
                types: &self.types,
                areas: self.areas.as_ref(),
            },
        )?;
        budget.take()?;
        let result = if map_result.status != GridLiquidStatus::NoWater
            && f32::from_bits(map_result.data.ok_or(Error::Format)?.level) > ground
        {
            map_result
        } else {
            GridLiquidResult {
                status: GridLiquidStatus::NoWater,
                data: None,
            }
        };
        budget.complete()?;
        Ok(TerrainLiquidReply { request, result })
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod oracle_tests;
