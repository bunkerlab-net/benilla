// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual-byte TerrainInfo::getLiquidStatus with wet and dry static VMaps enabled.
//! This opt-in profile models a non-null GridMapLiquidData output only. Status-only
//! calls skip wet DBC/area work in the source and are deliberately not represented.
//! No cached player environment, source provenance lease or gameplay admission.
use super::{
    height,
    liquid::world_tile,
    terrain::{
        liquid_status, source_type_flag, GridLiquidData, GridLiquidResult, GridLiquidStatus,
        LiquidStores, LiquidTypes,
    },
    terrain_area::TerrainAreaSnapshot,
    vmap::{
        liquid::{LiquidResult, LiquidSnapshot},
        QueryBudget,
    },
    Error, Hash, VmapLiquidProducts,
};
use sha2::{Digest, Sha256};
use std::time::Instant;

mod environment;

const DEFAULT_WATER_SEARCH: f32 = 50.0;
const POLICY: &[u8] = b"cmangos-8ec338a1-terrain-wet-liquid-complete-dbc-data-present-u32-v1";

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
    /// Exact original source-world binary32 pose; no caller-authored area predicate.
    pub position: [u32; 3],
    pub required_type: u8,
    pub collision_height: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TerrainLiquidReply {
    pub request: TerrainLiquidRequest,
    /// NoWater leaves caller output untouched; represented by data=None. Wet entry
    /// and type_flags are uint32. Fallback keeps the raw GridMap depth unmodified.
    pub result: GridLiquidResult,
}
#[derive(Debug)]
pub(crate) struct TerrainLiquidSnapshot<'a> {
    binding: SnapshotBinding,
    products: &'a VmapLiquidProducts,
    liquid: LiquidSnapshot,
    area: TerrainAreaSnapshot<'a>,
    types: LiquidTypes,
}

impl VmapLiquidProducts {
    /// Requires all four complete actual WDBC stores, even for a query that will
    /// not consult them. Empty valid stores are distinct from missing bytes.
    pub(crate) fn load_terrain_wet_liquid(
        &self,
        liquid_type_dbc: &[u8],
        area_table_dbc: &[u8],
        wmo_area_table_dbc: &[u8],
        map_dbc: &[u8],
    ) -> Result<TerrainLiquidSnapshot<'_>, Error> {
        let types = LiquidTypes::decode_complete(liquid_type_dbc)?;
        let liquid = self.load_vmap_liquid()?;
        let area = self.load_terrain_area(area_table_dbc, wmo_area_table_dbc, map_dbc)?;
        let source = liquid.binding();
        let area_source = area.binding();
        if source.map != area_source.map
            || source.content != area_source.content
            || source.source_epoch != area_source.source_epoch
            || source.inventory != area_source.inventory
        {
            return Err(Error::Identity);
        }
        let mut policy = Sha256::new();
        policy.update(POLICY);
        policy.update(height::POLICY);
        policy.update(source.policy);
        policy.update(area_source.policy);
        policy.update(types.digest());
        Ok(TerrainLiquidSnapshot {
            binding: SnapshotBinding {
                map: source.map,
                content: source.content,
                source_epoch: source.source_epoch,
                inventory: source.inventory,
                policy: policy.finalize().into(),
            },
            products: self,
            liquid,
            area,
            types,
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
        let position = request.position.map(f32::from_bits);
        let collision = f32::from_bits(request.collision_height);
        if !collision.is_finite() {
            return Err(Error::Format);
        }
        // Static height runs first, even if a later wet result overwrites its floor.
        let mut ground = self.liquid.static_height(
            &self.products.inventory,
            position,
            DEFAULT_WATER_SEARCH,
            budget,
        )?;
        let raw = self
            .liquid
            .liquid(position, request.required_type, budget)?;
        let mut result = GridLiquidResult::no_water();
        if let LiquidResult::Location {
            location,
            liquid_type,
            level: Some(level),
        } = raw
        {
            ground = f32::from_bits(location.floor);
            let level = f32::from_bits(level);
            budget.take()?;
            // Source success suppresses fallback even when these strict guards fail.
            if level > ground && position[2] > ground - 2.0 {
                let mut entry = liquid_type;
                budget.take()?;
                let mut kind = self.types.kind(entry).unwrap_or(0);
                // The wet area branch does not depend on a base LiquidType match.
                if entry != 0 && entry < 21 {
                    if let Some(area) = self.area.liquid_area(position, budget)? {
                        let mut override_liquid = area.liquid_override;
                        if override_liquid == 0 && area.zone != 0 {
                            budget.take()?;
                            if let Some(parent) = self.area.area_by_id(area.zone) {
                                override_liquid = parent.liquid_override;
                            }
                        }
                        budget.take()?;
                        // Override zero is a real lookup; required_type is NOT rechecked.
                        if let Some(override_kind) = self.types.kind(override_liquid) {
                            entry = override_liquid;
                            kind = override_kind;
                        }
                    }
                }
                let type_flags = source_type_flag(kind)?;
                result = GridLiquidResult {
                    status: liquid_status(level, position[2], collision),
                    data: Some(GridLiquidData {
                        entry,
                        type_flags,
                        level: level.to_bits(),
                        depth_level: ground.to_bits(),
                    }),
                };
            }
        } else {
            // GetLiquidLevel writes floor/type before mask/dry/hole/outside failure.
            if let LiquidResult::Location { location, .. } = raw {
                ground = f32::from_bits(location.floor);
            }
            budget.take()?;
            let tile = self
                .products
                .inventory
                .terrain(self.binding.map, world_tile(position[0], position[1])?)?;
            let map_result = tile.grid_liquid_status_with_budget(
                position,
                request.required_type,
                collision,
                LiquidStores {
                    types: &self.types,
                    areas: Some(self.area.areas()),
                },
                budget,
            )?;
            budget.take()?;
            if map_result.status != GridLiquidStatus::NoWater
                && f32::from_bits(map_result.data.ok_or(Error::Format)?.level) > ground
            {
                result = map_result;
            }
        }
        budget.complete()?;
        Ok(TerrainLiquidReply { request, result })
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod oracle_tests;
