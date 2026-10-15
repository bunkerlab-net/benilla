// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded GridMap liquid queries and the separate exact dry TerrainInfo proof.
//! Not player environment state, accessibility, source authenticity or session admission.
use super::{terrain::GRID_SIZE, Error, Hash, InventoriedProducts, ProductId, TileId};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, time::Instant};

const POLICY: &[u8] = b"cmangos-8ec338a1-complete-dry-native-liquid-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SnapshotBinding {
    pub map: u32,
    pub content: Hash,
    pub source_epoch: Hash,
    pub inventory: Hash,
    pub policy: Hash,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LiquidRequest {
    pub snapshot: SnapshotBinding,
    pub ticket: u64,
    /// Exact world-coordinate binary32 bits, echoed even when two poses share a tile.
    pub position: [u32; 3],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LiquidStatus {
    NoWater,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LiquidReply {
    pub request: LiquidRequest,
    pub status: LiquidStatus,
}

/// Immutable proof over this exact decoded product inventory. No caller-authored dry
/// flag or list of tiles can construct it. There is intentionally no runtime authority
/// conversion: Player::IsInWater reads ordered cached environment state, not this query.
#[derive(Debug)]
pub(crate) struct DryLiquidSnapshot {
    binding: SnapshotBinding,
    tiles: BTreeSet<TileId>,
}
impl InventoriedProducts {
    pub(crate) fn load_dry_liquid(&self) -> Result<DryLiquidSnapshot, Error> {
        // The real VMap decoder rejects LIQU payloads, malformed/unknown models and
        // non-tiled/worldspawn domains. Require every tree slot, not only a ray's visits.
        let vmap = self.load_static_vmap()?;
        vmap.require_complete_dry_map()?;
        let mut tiles = BTreeSet::new();
        for (id, product) in &self.products {
            if let ProductId::Terrain(tile) = id {
                // A decoded wet tile is not a proof of absent water, even when its liquid
                // rectangle does not cover this query. Keep the stronger dry contract.
                if product.terrain.as_ref().ok_or(Error::Missing)?.has_liquid() {
                    return Err(Error::Unsupported);
                }
                tiles.insert(*tile);
            }
        }
        if tiles.is_empty() {
            return Err(Error::Missing);
        }
        let source = vmap.binding();
        let mut digest = Sha256::new();
        digest.update(POLICY);
        digest.update(source.policy);
        Ok(DryLiquidSnapshot {
            binding: SnapshotBinding {
                map: source.map,
                content: source.content,
                source_epoch: source.source_epoch,
                inventory: source.inventory,
                policy: digest.finalize().into(),
            },
            tiles,
        })
    }
}
impl DryLiquidSnapshot {
    pub(crate) fn binding(&self) -> SnapshotBinding {
        self.binding
    }
    pub(crate) fn query(
        &self,
        request: LiquidRequest,
        deadline: Instant,
    ) -> Result<LiquidReply, Error> {
        if request.snapshot != self.binding || request.ticket == 0 {
            return Err(Error::Identity);
        }
        if Instant::now() >= deadline {
            return Err(Error::Budget);
        }
        let p = request.position.map(f32::from_bits);
        if p.iter().any(|v| !v.is_finite() || v.abs() > 100_000.0) {
            return Err(Error::Format);
        }
        let tile = world_tile(p[0], p[1])?;
        if !self.tiles.contains(&tile) {
            return Err(Error::Missing);
        }
        // TerrainInfo::getLiquidStatus: no GroupModel has iLiquid, hence GetLiquidLevel
        // is false; GridMap's absent m_liquidFlags/m_liquidGlobalFlags returns NO_WATER
        // before height, holes, area/DBC overrides or collision-height arithmetic.
        // This is a source branch proof, not "no visible water" or clear navigation.
        if Instant::now() >= deadline {
            return Err(Error::Budget);
        }
        Ok(LiquidReply {
            request,
            status: LiquidStatus::NoWater,
        })
    }
}
pub(super) fn world_tile(x: f32, y: f32) -> Result<TileId, Error> {
    // TerrainInfo::GetGrid, GridMap.cpp:1224–1226: source binary32 division and
    // subtraction, then truncation. Reject out-of-grid coordinates before conversion; this is not player map-coordinate validation.
    let axis = |v| {
        let grid = 32.0f32 - v / GRID_SIZE;
        if !(0.0..64.0).contains(&grid) {
            return Err(Error::OutsideTile);
        }
        Ok(grid as u8)
    };
    Ok(TileId {
        x: axis(x)?,
        y: axis(y)?,
    })
}

/// A raw GridMap result, not TerrainInfo's VMap/static-ground selection or cached
/// Player environment state. Every numeric input, including collision height/mask,
/// is echoed and bound; no caller boolean can provide a liquid observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GridLiquidRequest {
    pub query: LiquidRequest,
    pub required_type: u8,
    pub collision_height: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GridLiquidReply {
    pub request: GridLiquidRequest,
    pub result: super::terrain::GridLiquidResult,
}
#[derive(Debug)]
pub(crate) struct GridLiquidSnapshot<'a> {
    binding: SnapshotBinding,
    products: &'a InventoriedProducts,
    types: super::terrain::LiquidTypes,
    areas: Option<super::terrain::AreaTable>,
}
impl InventoriedProducts {
    /// LiquidType.dbc is supplied as complete actual WDBC bytes, never a caller-made
    /// row resolver. Its hash is part of this snapshot policy. This checks consistency,
    /// not authenticity or a live source lease; it cannot create runtime admission.
    pub(crate) fn load_grid_liquid(
        &self,
        liquid_type_dbc: &[u8],
    ) -> Result<GridLiquidSnapshot<'_>, Error> {
        self.grid_liquid_snapshot(super::terrain::LiquidTypes::decode(liquid_type_dbc)?, None)
    }

    /// Optional complete AreaTable bytes enable the exact GridMap override branch.
    /// Absence of this argument is unsupported when needed, not an empty table.
    pub(crate) fn load_grid_liquid_with_areas(
        &self,
        liquid_type_dbc: &[u8],
        area_table_dbc: &[u8],
    ) -> Result<GridLiquidSnapshot<'_>, Error> {
        self.grid_liquid_snapshot(
            super::terrain::LiquidTypes::decode(liquid_type_dbc)?,
            Some(super::terrain::AreaTable::decode(area_table_dbc)?),
        )
    }

    fn grid_liquid_snapshot(
        &self,
        types: super::terrain::LiquidTypes,
        areas: Option<super::terrain::AreaTable>,
    ) -> Result<GridLiquidSnapshot<'_>, Error> {
        // Keep unresolved slots and WMO LIQU outside this bounded slice. This does
        // not turn a GridMap answer into a composite TerrainInfo result.
        let vmap = self.load_static_vmap()?;
        vmap.require_complete_dry_map()?;
        if !self
            .products
            .keys()
            .any(|id| matches!(id, ProductId::Terrain(_)))
        {
            return Err(Error::Missing);
        }
        let source = vmap.binding();
        let mut digest = Sha256::new();
        if let Some(areas) = &areas {
            digest.update(b"cmangos-8ec338a1-gridmap-area-liquid-v1");
            digest.update(areas.digest());
        } else {
            // Preserve the no-area profile binding and its unsupported dependency.
            digest.update(b"cmangos-8ec338a1-gridmap-liquid-v1");
        }
        digest.update(source.policy);
        digest.update(types.digest());
        Ok(GridLiquidSnapshot {
            binding: SnapshotBinding {
                map: source.map,
                content: source.content,
                source_epoch: source.source_epoch,
                inventory: source.inventory,
                policy: digest.finalize().into(),
            },
            products: self,
            types,
            areas,
        })
    }
}
impl GridLiquidSnapshot<'_> {
    pub(crate) fn binding(&self) -> SnapshotBinding {
        self.binding
    }
    pub(crate) fn query(
        &self,
        request: GridLiquidRequest,
        deadline: Instant,
    ) -> Result<GridLiquidReply, Error> {
        if request.query.snapshot != self.binding || request.query.ticket == 0 {
            return Err(Error::Identity);
        }
        if Instant::now() >= deadline {
            return Err(Error::Budget);
        }
        let p = request.query.position.map(f32::from_bits);
        if p.iter().any(|v| !v.is_finite() || v.abs() > 100_000.0) {
            return Err(Error::Format);
        }
        let tile = self
            .products
            .terrain(self.binding.map, world_tile(p[0], p[1])?)?;
        let result = tile.grid_liquid_status_with_stores(
            p[0],
            p[1],
            p[2],
            request.required_type,
            f32::from_bits(request.collision_height),
            super::terrain::LiquidStores {
                types: &self.types,
                areas: self.areas.as_ref(),
            },
        )?;
        if Instant::now() >= deadline {
            return Err(Error::Budget);
        }
        Ok(GridLiquidReply { request, result })
    }
}

#[cfg(test)]
mod tests;
