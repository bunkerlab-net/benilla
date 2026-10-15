// SPDX-License-Identifier: GPL-3.0-or-later
//! Pinned TerrainInfo::GetHeightStatic with height calculation and VMaps enabled.
//! Actual decoded terrain and static geometry only; not composite liquid or support.
use super::{liquid::world_tile, vmap, Error, Hash, InventoriedProducts, PointHeight};
use sha2::{Digest, Sha256};
use std::time::Instant;

const INVALID_HEIGHT: f32 = -100_000.0;
const INVALID_HEIGHT_VALUE: f32 = -200_000.0;
const DEFAULT_HEIGHT_SEARCH: f32 = 10.0;
const POLICY: &[u8] = b"cmangos-8ec338a1-static-height-enabled-complete-dry-vmap-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SnapshotBinding {
    pub map: u32,
    pub content: Hash,
    pub source_epoch: Hash,
    pub inventory: Hash,
    pub policy: Hash,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HeightRequest {
    pub snapshot: SnapshotBinding,
    pub ticket: u64,
    pub position: [u32; 3],
    pub max_search: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HeightReply {
    pub request: HeightRequest,
    /// Exact raw height bits, including source invalid-height values. Never support.
    pub height: u32,
}
#[derive(Debug)]
pub(crate) struct StaticHeightSnapshot<'a> {
    binding: SnapshotBinding,
    products: &'a InventoriedProducts,
    vmap: vmap::VmapSnapshot,
}
impl InventoriedProducts {
    /// Bounded query profile, not a claim about the effective running source config.
    /// Source configuration/provenance and non-render participant admission are separate.
    pub(crate) fn load_static_height(&self) -> Result<StaticHeightSnapshot<'_>, Error> {
        let vmap = self.load_static_vmap()?;
        vmap.require_complete_dry_map()?;
        let source = vmap.binding();
        let mut policy = Sha256::new();
        policy.update(POLICY);
        policy.update(source.policy);
        Ok(StaticHeightSnapshot {
            binding: SnapshotBinding {
                map: source.map,
                content: source.content,
                source_epoch: source.source_epoch,
                inventory: source.inventory,
                policy: policy.finalize().into(),
            },
            products: self,
            vmap,
        })
    }
}
impl StaticHeightSnapshot<'_> {
    pub(crate) fn binding(&self) -> SnapshotBinding {
        self.binding
    }
    pub(crate) fn query(
        &self,
        request: HeightRequest,
        deadline: Instant,
    ) -> Result<HeightReply, Error> {
        if request.snapshot != self.binding || request.ticket == 0 {
            return Err(Error::Identity);
        }
        let mut budget = vmap::QueryBudget::new(deadline);
        budget.complete()?;
        let p = request.position.map(f32::from_bits);
        let search = f32::from_bits(request.max_search);
        if p.iter().any(|v| !v.is_finite()) || !search.is_finite() {
            return Err(Error::Format);
        }
        if p.iter().any(|v| v.abs() > 100_000.0) || search.abs() > 10_000.0 {
            return Err(Error::Unsupported);
        }
        // Missing GridMap coverage remains unknown, not the source hole sentinel.
        let tile = self
            .products
            .terrain(self.binding.map, world_tile(p[0], p[1])?)?;
        let map_height = match tile.point_height(p[0], p[1])? {
            PointHeight::Height(value) => value,
            PointHeight::Hole => INVALID_HEIGHT_VALUE,
        };
        let height = select_height(map_height, p[2], search, |z, distance| {
            self.vmap.height([p[0], p[1], z], distance, &mut budget)
        })?;
        budget.complete()?;
        Ok(HeightReply {
            request,
            height: height.to_bits(),
        })
    }
}

/// Exact source decision order and binary32 arithmetic. The only production caller
/// closes over this snapshot's real VMap; the function has no authority constructor.
fn select_height(
    map_height: f32,
    z: f32,
    mut max_search: f32,
    mut query: impl FnMut(f32, f32) -> Result<f32, Error>,
) -> Result<f32, Error> {
    let z2 = z + 2.0;
    if map_height > INVALID_HEIGHT && z2 - map_height > max_search {
        max_search = z2 - map_height + 1.0;
    }
    let mut vmap_height = query(z2, max_search)?;
    if vmap_height <= INVALID_HEIGHT {
        vmap_height = query(z2, 10_000.0)?;
    }
    if vmap_height <= INVALID_HEIGHT && map_height > z2 && (z2 - map_height).abs() > 30.0 {
        vmap_height = query(z2, -max_search)?;
    }
    if vmap_height <= INVALID_HEIGHT && map_height > INVALID_HEIGHT && z2 < map_height {
        vmap_height = query(map_height + 2.0, DEFAULT_HEIGHT_SEARCH)?;
    }
    if vmap_height > INVALID_HEIGHT {
        if map_height > INVALID_HEIGHT {
            if z < map_height || vmap_height > map_height {
                return Ok(vmap_height);
            }
            return Ok(map_height);
        }
        return Ok(vmap_height);
    }
    Ok(map_height)
}

#[cfg(test)]
mod oracle_tests;
#[cfg(test)]
mod tests;
