// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded actual-byte VMapManager2::GetLiquidLevel, including its floor/type writes.
//! No TerrainInfo wet status, DBC area override, environment state or admission proof.
use super::super::{envelopes::liquid_dimensions, VmapLiquidProducts};
use super::{
    allocate, apply, scale, sub, to_internal, Cursor, Error, LocationFloor, QueryBudget,
    SnapshotBinding, Vector, VmapSnapshot,
};
use sha2::{Digest, Sha256};
use std::time::Instant;

const POLICY: &[u8] = b"cmangos-8ec338a1-static-vmap-liquid-v1-glibc-2.41-defined-u32-domain";
const TILE_SIZE: f32 = 533.333_f32 / 128.0_f32;

/// Bound separately from all dry rays/heights/locations and terrain liquid status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LiquidRequest {
    pub snapshot: SnapshotBinding,
    pub ticket: u64,
    pub position: [u32; 3],
    pub required_mask: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LiquidResult {
    /// Source leaves caller floor/type/level unchanged when no location is selected.
    NoLocation,
    Location {
        location: LocationFloor,
        /// Raw uint32 GetLiquidType, including zero and unknown IDs; never a DBC row.
        liquid_type: u32,
        /// Source writes level only on success. None retains selected floor/type,
        /// including mask rejection, dry groups and disabled/outside liquid cells.
        level: Option<u32>,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LiquidReply {
    pub request: LiquidRequest,
    pub result: LiquidResult,
}

#[derive(Debug)]
pub(crate) struct LiquidSnapshot {
    binding: SnapshotBinding,
    // Never expose this wet geometry as a conservative VmapSnapshot.
    geometry: VmapSnapshot,
}
impl VmapLiquidProducts {
    pub(crate) fn load_vmap_liquid(&self) -> Result<LiquidSnapshot, Error> {
        let geometry = VmapSnapshot::load_profile(&self.inventory, true)?;
        if geometry.spawns.iter().any(Option::is_none) {
            return Err(Error::Missing);
        }
        let mut hash = Sha256::new();
        hash.update(POLICY);
        hash.update(geometry.binding.inventory);
        hash.update(geometry.binding.policy);
        hash.update(geometry.location_binding().policy);
        Ok(LiquidSnapshot {
            binding: SnapshotBinding {
                policy: hash.finalize().into(),
                ..geometry.binding
            },
            geometry,
        })
    }
}
impl LiquidSnapshot {
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
        let mut budget = QueryBudget::new(deadline);
        let result = self.liquid(
            request.position.map(f32::from_bits),
            request.required_mask,
            &mut budget,
        )?;
        budget.complete()?;
        Ok(LiquidReply { request, result })
    }

    fn liquid(
        &self,
        position: Vector,
        required_mask: u8,
        budget: &mut QueryBudget,
    ) -> Result<LiquidResult, Error> {
        let Some(selected) = self.geometry.select_location(position, budget)? else {
            return Ok(LiquidResult::NoLocation);
        };
        budget.take()?;
        let spawn = self.geometry.spawns[selected.spawn]
            .as_ref()
            .ok_or(Error::Missing)?;
        let group = &self.geometry.models[spawn.model].groups[selected.group];
        let liquid_type = group.liquid.as_ref().map_or(0, |liquid| liquid.liquid_type);
        let mut level = None;
        if required_mask == 0 || liquid_mask(liquid_type) & required_mask != 0 {
            if let Some(liquid) = &group.liquid {
                let position = to_internal(position);
                let local = scale(
                    apply(spawn.inverse, sub(position, spawn.position)),
                    spawn.inverse_scale,
                );
                if let Some(local_level) = liquid.height(local, budget)? {
                    // ModelInstance deliberately uses local z, even for tilted models.
                    let world_level = (local_level - local[2]) * spawn.scale + position[2];
                    if !world_level.is_finite() {
                        return Err(Error::Format);
                    }
                    level = Some(world_level.to_bits());
                }
            }
        }
        budget.complete()?;
        Ok(LiquidResult::Location {
            location: selected.floor,
            liquid_type,
            level,
        })
    }
}

fn liquid_mask(liquid_type: u32) -> u8 {
    match liquid_type {
        // Maps/GridMapDefines.h, Classic bit order (not later client enums).
        1 | 41 | 61 => 8,
        2 => 2,
        3 => 1,
        4 | 21 => 4,
        _ => 0,
    }
}

#[derive(Debug)]
pub(super) struct WmoLiquid {
    width: usize,
    height: usize,
    corner: Vector,
    liquid_type: u32,
    heights: Vec<f32>,
    flags: Vec<u8>,
}
impl WmoLiquid {
    pub(super) fn decode(c: &mut Cursor<'_>, declared: usize) -> Result<Self, Error> {
        let width = c.word()?;
        let height = c.word()?;
        let (heights, flags) = liquid_dimensions(width, height, declared)?;
        // Checked dimensions precede allocation; the source's declared length is
        // four bytes shorter than the actual payload because it omits iType.
        c.require(16 + heights * 4 + flags)?;
        let corner = c.vector()?;
        let liquid_type = c.word()?;
        let mut values = allocate(heights)?;
        for _ in 0..heights {
            values.push(c.float()?);
        }
        let mut cells = allocate(flags)?;
        cells.extend_from_slice(c.take(flags)?);
        Ok(Self {
            width: width as usize,
            height: height as usize,
            corner,
            liquid_type,
            heights: values,
            flags: cells,
        })
    }

    fn height(&self, position: Vector, budget: &mut QueryBudget) -> Result<Option<f32>, Error> {
        budget.take()?;
        let tx_f = (position[0] - self.corner[0]) / TILE_SIZE;
        let tx = source_index(tx_f)?;
        if tx_f < 0.0 || tx >= self.width {
            return Ok(None);
        }
        // Preserve X early exits: source never computes or casts Y in those cases.
        let ty_f = (position[1] - self.corner[1]) / TILE_SIZE;
        let ty = source_index(ty_f)?;
        if ty_f < 0.0 || ty >= self.height {
            return Ok(None);
        }
        if self.flags[tx + ty * self.width] & 0x0f == 0x0f {
            return Ok(None);
        }
        let dx = tx_f - tx as f32;
        let dy = ty_f - ty as f32;
        let row = self.width + 1;
        let (sx, sy) = if dx > dy {
            (
                self.heights[tx + 1 + ty * row] - self.heights[tx + ty * row],
                self.heights[tx + 1 + (ty + 1) * row] - self.heights[tx + 1 + ty * row],
            )
        } else {
            (
                self.heights[tx + 1 + (ty + 1) * row] - self.heights[tx + (ty + 1) * row],
                self.heights[tx + (ty + 1) * row] - self.heights[tx + ty * row],
            )
        };
        let level = self.heights[tx + ty * row] + dx * sx + dy * sy;
        if !level.is_finite() {
            return Err(Error::Format);
        }
        Ok(Some(level))
    }
}

/// C++ casts before checking negatives. Only truncations representable in uint32
/// are defined; reject other coordinates rather than converting UB into absence.
fn source_index(value: f32) -> Result<usize, Error> {
    if !value.is_finite() {
        return Err(Error::Format);
    }
    if value <= -1.0 || value >= 4_294_967_296.0_f32 {
        return Err(Error::Unsupported);
    }
    Ok(value as u32 as usize)
}

#[cfg(test)]
pub(super) mod tests;
