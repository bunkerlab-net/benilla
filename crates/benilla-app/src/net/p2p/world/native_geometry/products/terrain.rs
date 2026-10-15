// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded translation of pinned CMaNGOS GridMap's float/flat height and liquid readers.
//! This is not TerrainInfo/Map height selection, support, collision, or movement.
//! Upstream authors, grants and exact source hashes: PROVENANCE.md.
use super::{u16_at, u32_at, Error, PointHeight, TileId};
mod liquid;
#[cfg(test)]
pub(crate) use liquid::{GridLiquidData, GridLiquidStatus};
pub(crate) use liquid::{GridLiquidResult, LiquidTypes};

const HEADER: usize = 40;
const V9: usize = 129 * 129;
const V8: usize = 128 * 128;
pub(super) const GRID_SIZE: f32 = 533.333_3; // SIZE_OF_GRIDS = 533.33333f, same binary32.
const MAX_HEIGHT: f32 = 1_000_000.0; // Local finite arithmetic bound, not a source game rule.

#[derive(Debug)]
enum Heights {
    Flat(f32),
    Float { v9: Box<[f32]>, v8: Box<[f32]> },
}

/// Owns decoded numbers; cannot be retargeted, mutated, or loaded lazily from disk.
#[derive(Debug)]
pub(crate) struct TerrainTile {
    id: TileId,
    heights: Heights,
    holes: [u16; 256],
    liquid: Option<liquid::GridLiquid>,
}

impl TerrainTile {
    pub(super) fn decode(id: TileId, bytes: &[u8]) -> Result<Self, Error> {
        id.validate()?;
        if bytes.get(..8) != Some(b"MAPSz1.4") {
            return Err(Error::Format);
        }
        // The pinned -f 0 writer emits consecutive AREA, MHGT, optional MLIQ, holes.
        // No offset gaps/aliases, absent area/heights/holes, or trailing data.
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
        let liquid_end = height_end.checked_add(liquid_len).ok_or(Error::Framing)?;
        let file_end = holes_offset.checked_add(holes_len).ok_or(Error::Framing)?;
        if area_offset != HEADER
            || !matches!(area_len, 8 | 520)
            || height_offset != HEADER + area_len
            || !matches!(height_len, 16 | 132116)
            || !matches!((liquid_offset, liquid_len), (0, 0))
                && (liquid_offset != height_end || liquid_len == 0)
            || holes_offset != liquid_end
            || holes_len != 512
            || bytes.len() != file_end
        {
            return Err(Error::Framing);
        }
        let area = &bytes[area_offset..height_offset];
        if &area[..4] != b"AREA" {
            return Err(Error::Format);
        }
        match u16_at(area, 4)? {
            0 if area_len == 520 => {}
            1 if area_len == 8 => {}
            0 | 1 => return Err(Error::Framing),
            _ => return Err(Error::Unsupported),
        }
        let height = &bytes[height_offset..height_end];
        if &height[..4] != b"MHGT" {
            return Err(Error::Format);
        }
        let low = height_number(height, 8)?;
        let high = height_number(height, 12)?;
        if low > high {
            return Err(Error::Format);
        }
        let heights = match u32_at(height, 4)? {
            1 if height_len == 16 && low == high => Heights::Flat(low),
            0 if height_len == 16 + (V9 + V8) * 4 => {
                // Exact count/framing checked before either allocation. The inventory's
                // 64-tile limit separately bounds decoded storage to 64 * 132100 bytes.
                let values = |start, count| {
                    (start..start + count)
                        .map(|i| {
                            let value = height_number(height, 16 + i * 4)?;
                            if value < low || value > high {
                                return Err(Error::Format);
                            }
                            Ok(value)
                        })
                        .collect::<Result<Vec<_>, _>>()
                        .map(Vec::into_boxed_slice)
                };
                Heights::Float {
                    v9: values(0, V9)?,
                    v8: values(V9, V8)?,
                }
            }
            0 | 1 => return Err(Error::Framing),
            _ => return Err(Error::Unsupported),
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
            holes,
            liquid,
        })
    }

    /// Section presence, not a claim that every cell contains water.
    pub(crate) fn has_liquid(&self) -> bool {
        self.liquid.is_some()
    }

    /// Exact pinned float-operation order, guarded against wrong tile/NaN/overflow.
    /// Pinned flat-height dispatch bypasses holes; this is deliberately NOT support proof.
    pub(crate) fn point_height(&self, world_x: f32, world_y: f32) -> Result<PointHeight, Error> {
        let (x, y, ix, iy) = self.coordinates(world_x, world_y)?;
        let Heights::Float { v9, v8 } = &self.heights else {
            let Heights::Flat(height) = self.heights else {
                unreachable!()
            };
            return Ok(PointHeight::Height(height));
        };
        if self.is_hole(ix, iy) {
            return Ok(PointHeight::Hole);
        }
        // GridMap::getHeightFromFloat's four center-fan triangles, not bilinear interpolation.
        let (a, b, c) = if x + y < 1.0 {
            if x > y {
                let h1 = v9[ix * 129 + iy];
                let h2 = v9[(ix + 1) * 129 + iy];
                let h5 = 2.0 * v8[ix * 128 + iy];
                (h2 - h1, h5 - h1 - h2, h1)
            } else {
                let h1 = v9[ix * 129 + iy];
                let h3 = v9[ix * 129 + iy + 1];
                let h5 = 2.0 * v8[ix * 128 + iy];
                (h5 - h1 - h3, h3 - h1, h1)
            }
        } else if x > y {
            let h2 = v9[(ix + 1) * 129 + iy];
            let h4 = v9[(ix + 1) * 129 + iy + 1];
            let h5 = 2.0 * v8[ix * 128 + iy];
            (h2 + h4 - h5, h4 - h2, h5 - h4)
        } else {
            let h3 = v9[ix * 129 + iy + 1];
            let h4 = v9[(ix + 1) * 129 + iy + 1];
            let h5 = 2.0 * v8[ix * 128 + iy];
            (h4 - h3, h3 + h4 - h5, h5 - h4)
        };
        Ok(PointHeight::Height(a * x + b * y + c))
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

    fn is_hole(&self, row: usize, col: usize) -> bool {
        let mask = 1 << (row % 8 / 2 * 4 + col % 8 / 2);
        self.holes[row / 8 * 16 + col / 8] & mask != 0
    }
}

fn height_number(bytes: &[u8], at: usize) -> Result<f32, Error> {
    let value = f32::from_bits(u32_at(bytes, at)?);
    if !value.is_finite() || value.abs() > MAX_HEIGHT {
        return Err(Error::Format);
    }
    Ok(value)
}
