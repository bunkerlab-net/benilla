// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded translation of pinned CMaNGOS GridMap's height and liquid readers.
//! This is not TerrainInfo/Map height selection, support, collision, or movement.
//! Upstream authors, grants and exact source hashes: PROVENANCE.md.
use super::{u16_at, u32_at, Error, PointHeight, TileId};
use benilla_native_terrain::grid_height::{HeightData, HeightValue};
use std::sync::Arc;
mod area;
mod liquid;
pub(super) use area::AreaEntry;
pub(crate) use area::AreaTable;
pub(crate) use liquid::GridLiquidData;
pub(super) use liquid::{liquid_status, source_type_flag};
pub(crate) use liquid::{GridLiquidResult, GridLiquidStatus, LiquidStores, LiquidTypes};

const HEADER: usize = 40;
const V9: usize = 129 * 129;
const V8: usize = 128 * 128;
pub(super) const GRID_SIZE: f32 = 533.333_3; // SIZE_OF_GRIDS = 533.33333f, same binary32.
const MAX_HEIGHT: f32 = 1_000_000.0; // Local finite arithmetic bound, not a source game rule.

pub(super) const HEIGHT_POLICY: &[u8] = b"cmangos-gridmap-float-flat-absent-u8-u16-v2";

/// Owns immutable decoded products; never loads lazily from disk.
#[derive(Debug)]
pub(crate) struct TerrainTile {
    id: TileId,
    heights: HeightData,
    areas: area::GridAreas,
    holes: [u16; 256],
    liquid: Option<liquid::GridLiquid>,
}

impl TerrainTile {
    pub(super) fn decode(id: TileId, bytes: &[u8]) -> Result<Self, Error> {
        id.validate()?;
        if bytes.get(..8) != Some(b"MAPSz1.4") {
            return Err(Error::Format);
        }
        // The pinned writer emits consecutive AREA, optional MHGT/MLIQ, holes.
        // No offset gaps/aliases, absent area/holes, or trailing data.
        let area_offset = u32_at(bytes, 8)? as usize;
        let area_len = u32_at(bytes, 12)? as usize;
        let height_offset = u32_at(bytes, 16)? as usize;
        let height_len = u32_at(bytes, 20)? as usize;
        let liquid_offset = u32_at(bytes, 24)? as usize;
        let liquid_len = u32_at(bytes, 28)? as usize;
        let holes_offset = u32_at(bytes, 32)? as usize;
        let holes_len = u32_at(bytes, 36)? as usize;
        let height_end = height_offset
            .checked_add(height_len)
            .ok_or(Error::Framing)?;
        let height_end = if height_len == 0 {
            HEADER + area_len
        } else {
            height_end
        };
        let liquid_end = height_end.checked_add(liquid_len).ok_or(Error::Framing)?;
        let file_end = holes_offset.checked_add(holes_len).ok_or(Error::Framing)?;
        if area_offset != HEADER
            || !matches!(area_len, 8 | 520)
            || (height_len == 0 && height_offset != 0)
            || (height_len != 0 && height_offset != HEADER + area_len)
            || !matches!((liquid_offset, liquid_len), (0, 0))
                && (liquid_offset != height_end || liquid_len == 0)
            || holes_offset != liquid_end
            || holes_len != 512
            || bytes.len() != file_end
        {
            return Err(Error::Framing);
        }
        let areas = area::GridAreas::decode(
            bytes
                .get(area_offset..HEADER + area_len)
                .ok_or(Error::Framing)?,
        )?;
        let heights = if height_len == 0 {
            HeightData::default()
        } else {
            let height = bytes
                .get(height_offset..height_end)
                .ok_or(Error::TerrainHeightSamples)?;
            let flags = validate_height_section(height)?;
            if flags == 1 {
                HeightData::flat(height_number(height, 8)?, height_number(height, 12)?)
            } else {
                HeightData::decode(Arc::from(height), 0).map_err(|_| Error::TerrainHeightHeader)?
            }
        };
        let mut holes = [0; 256];
        for (i, hole) in holes.iter_mut().enumerate() {
            *hole = u16_at(bytes, holes_offset + i * 2)?;
        }
        let liquid = if liquid_len == 0 {
            None
        } else {
            Some(liquid::GridLiquid::decode(
                &bytes[liquid_offset..liquid_end],
            )?)
        };
        Ok(Self {
            id,
            heights,
            areas,
            holes,
            liquid,
        })
    }

    /// Section presence, not a claim that every cell contains water.
    pub(crate) fn has_liquid(&self) -> bool {
        self.liquid.is_some()
    }

    /// Raw GridMap AREA flag, distinct from the AreaTable ID lookup used by liquid.
    pub(super) fn area_flag(&self, world_x: f32, world_y: f32) -> Result<u16, Error> {
        self.coordinates(world_x, world_y)?;
        Ok(self.areas.get(world_x, world_y))
    }

    /// Exact pinned float-operation order, guarded against wrong tile/NaN/overflow.
    /// Pinned flat-height dispatch bypasses holes; this is deliberately NOT support proof.
    pub(crate) fn point_height(&self, world_x: f32, world_y: f32) -> Result<PointHeight, Error> {
        self.coordinates(world_x, world_y)?;
        match self
            .heights
            .get(world_x, world_y, &self.holes)
            .map_err(|_| Error::TerrainHeightSamples)?
        {
            HeightValue::Height(height) => Ok(PointHeight::Height(height)),
            HeightValue::Hole => Ok(PointHeight::Hole),
        }
    }

    fn coordinates(&self, x: f32, y: f32) -> Result<(f32, f32, usize, usize), Error> {
        let x = 128.0 * (32.0 - x / GRID_SIZE);
        let y = 128.0 * (32.0 - y / GRID_SIZE);
        let inside = |v: f32, tile: u8| {
            let start = f32::from(tile) * 128.0;
            v.is_finite() && v >= start && v < start + 128.0
        };
        if !inside(x, self.id.x) || !inside(y, self.id.y) {
            return Err(Error::OutsideTile);
        }
        let ix = x as usize;
        let iy = y as usize;
        Ok((x - ix as f32, y - iy as f32, ix & 127, iy & 127))
    }
}

/// Bounded producer admission, before shared GridMap arithmetic reads any samples.
pub(super) fn validate_height_section(height: &[u8]) -> Result<u32, Error> {
    if height.len() < 16 || height.get(..4) != Some(b"MHGT") {
        return Err(Error::TerrainHeightHeader);
    }
    let flags = u32_at(height, 4)?;
    if !matches!(flags, 0 | 1 | 2 | 4) {
        return Err(Error::TerrainHeightFlags(flags));
    }
    let low = height_number(height, 8)?;
    let high = height_number(height, 12)?;
    if low > high {
        return Err(Error::Format);
    }
    let width = match flags {
        1 => 0,
        2 => 2,
        4 => 1,
        _ => 4,
    };
    if height.len() != 16 + (V9 + V8) * width {
        return Err(Error::TerrainHeightSamples);
    }
    if flags == 0 {
        for at in (16..height.len()).step_by(4) {
            let value = height_number(height, at)?;
            if value < low || value > high {
                return Err(Error::Format);
            }
        }
    }
    Ok(flags)
}

fn height_number(bytes: &[u8], at: usize) -> Result<f32, Error> {
    let value = f32::from_bits(u32_at(bytes, at)?);
    if !value.is_finite() || value.abs() > MAX_HEIGHT {
        return Err(Error::Format);
    }
    Ok(value)
}

/// GridMap.cpp:252-255 retains finite liquid samples, including FLT_MAX boundary data.
pub(super) fn liquid_height_number(bytes: &[u8], at: usize) -> Result<f32, Error> {
    let value = f32::from_bits(u32_at(bytes, at)?);
    if !value.is_finite() {
        return Err(Error::Format);
    }
    Ok(value)
}
