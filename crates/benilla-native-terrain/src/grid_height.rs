// SPDX-License-Identifier: GPL-3.0-or-later
//! CMaNGOS Classic 8ec338a1 GridMap height reader and TerrainInfo selector.
//! All arithmetic is binary32 with separate operations, never fused mul_add.
use std::ops::{Add, Sub};
use std::sync::Arc;

pub const INVALID_HEIGHT: f32 = -100_000.0;
pub const INVALID_HEIGHT_VALUE: f32 = -200_000.0;
pub const DEFAULT_HEIGHT_SEARCH: f32 = 10.0;
pub const GRID_SIZE: f32 = 533.333_3;
const V9: usize = 129 * 129;
#[cfg(test)]
const V8: usize = 128 * 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeightReadError {
    HeaderReadUndefined,
    Magic,
    InvalidHeaderFlags(u32),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeightQueryError {
    CoordinateConversionUndefined,
    FloatSampleReadUndefined,
    Uint8SampleReadUndefined,
    Uint16SampleReadUndefined,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HeightValue {
    Height(f32),
    Hole,
}
#[derive(Debug)]
enum Samples {
    Flat,
    Float,
    Uint8,
    Uint16,
}

/// Retains the original allocation, without copying V9/V8 or converting products.
#[derive(Debug)]
pub struct HeightData {
    pub base: f32,
    pub maximum: f32,
    multiplier: f32,
    samples: Samples,
    source: Option<(Arc<[u8]>, usize)>,
}
impl Default for HeightData {
    /// GridMap.cpp:51-56, including absent MHGT dispatch and empty holes.
    fn default() -> Self {
        Self {
            base: INVALID_HEIGHT_VALUE,
            maximum: INVALID_HEIGHT_VALUE,
            multiplier: 0.0,
            samples: Samples::Flat,
            source: None,
        }
    }
}
impl HeightData {
    /// GridMap.cpp:173-214 ignores the declared section size and reads V9 before V8.
    /// Known combined bits retain source NO_HEIGHT > INT16 > INT8 precedence.
    /// Unknown bits have no producer meaning and are explicitly outside this reader.
    pub fn decode(source: Arc<[u8]>, offset: usize) -> Result<Self, HeightReadError> {
        let bytes = source.get(offset..).unwrap_or_default();
        let header = bytes
            .get(..16)
            .ok_or(HeightReadError::HeaderReadUndefined)?;
        if &header[..4] != b"MHGT" {
            return Err(HeightReadError::Magic);
        }
        let word = |at| u32::from_le_bytes(header[at..at + 4].try_into().unwrap());
        let flags = word(4);
        if flags & !7 != 0 {
            return Err(HeightReadError::InvalidHeaderFlags(flags));
        }
        let base = f32::from_bits(word(8));
        let maximum = f32::from_bits(word(12));
        let (samples, multiplier, has_arrays) = if flags & 1 != 0 {
            (Samples::Flat, 0.0, false)
        } else if flags & 2 != 0 {
            // GridMap.cpp:184-191: subtraction rounds before division by 65535.
            (Samples::Uint16, (maximum - base) / 65535.0, true)
        } else if flags & 4 != 0 {
            // GridMap.cpp:193-200: subtraction rounds before division by 255.
            (Samples::Uint8, (maximum - base) / 255.0, true)
        } else {
            (Samples::Float, 0.0, true)
        };
        Ok(Self {
            base,
            maximum,
            multiplier,
            samples,
            source: has_arrays.then_some((source, offset + 16)),
        })
    }

    /// GridMap.cpp:273-275,292-306,377-388,445-456. Only float mode checks holes.
    pub fn get(&self, x: f32, y: f32, holes: &[u16; 256]) -> Result<HeightValue, HeightQueryError> {
        if matches!(self.samples, Samples::Flat) {
            return Ok(HeightValue::Height(self.base));
        }
        let (local, cell) = coordinates(x, y)?;
        if matches!(self.samples, Samples::Float) && is_hole(holes, cell) {
            return Ok(HeightValue::Hole);
        }
        let (source, offset) = self.source.as_ref().unwrap();
        let bytes = &source[*offset..];
        let index = cell[0] * 129 + cell[1];
        let center = V9 + cell[0] * 128 + cell[1];
        let height = match self.samples {
            Samples::Float => {
                let value = |i: usize| {
                    let raw = bytes
                        .get(i * 4..i * 4 + 4)
                        .ok_or(HeightQueryError::FloatSampleReadUndefined)?;
                    Ok(f32::from_le_bytes(raw.try_into().unwrap()))
                };
                let (a, b, c) =
                    coefficients(local, |i| value(index + i), || Ok(2.0 * value(center)?))?;
                // GridMap.cpp:372: left-associative, unfused f32 products and sums.
                a * local[0] + b * local[1] + c
            }
            Samples::Uint8 | Samples::Uint16 => {
                let value = |i: usize| match self.samples {
                    Samples::Uint8 => bytes
                        .get(i)
                        .copied()
                        .map(i32::from)
                        .ok_or(HeightQueryError::Uint8SampleReadUndefined),
                    _ => bytes
                        .get(i * 2..i * 2 + 2)
                        .map(|raw| i32::from(u16::from_le_bytes(raw.try_into().unwrap())))
                        .ok_or(HeightQueryError::Uint16SampleReadUndefined),
                };
                let (a, b, c) =
                    coefficients(local, |i| value(index + i), || Ok(2 * value(center)?))?;
                // GridMap.cpp:440,508: int32 coefficients convert at each float
                // operation, then multiply the interpolant before adding base.
                (a as f32 * local[0] + b as f32 * local[1] + c as f32) * self.multiplier + self.base
            }
            Samples::Flat => unreachable!(),
        };
        Ok(HeightValue::Height(height))
    }
}

/// GridMap.cpp:295-303,380-388,448-456. Truncate before masking, not floor.
pub fn coordinates(x: f32, y: f32) -> Result<([f32; 2], [usize; 2]), HeightQueryError> {
    fn axis(value: f32) -> Result<(f32, usize), HeightQueryError> {
        let value = 128.0 * (32.0 - value / GRID_SIZE);
        if !value.is_finite() || !(-2_147_483_648.0..2_147_483_648.0).contains(&value) {
            return Err(HeightQueryError::CoordinateConversionUndefined);
        }
        let integer = value as i32;
        Ok((value - integer as f32, (integer & 127) as usize))
    }
    let (x, row) = axis(x)?;
    let (y, col) = axis(y)?;
    Ok(([x, y], [row, col]))
}

/// GridMap.cpp:278-287 and holetab_h/v at 40-41. Rows and columns are not swapped.
pub fn is_hole(holes: &[u16; 256], [row, col]: [usize; 2]) -> bool {
    let mask = 1 << (row % 8 / 2 * 4 + col % 8 / 2);
    holes[row / 8 * 16 + col / 8] & mask != 0
}

/// Shared float kernel for the app's separately restricted float/flat reader.
pub fn float_height(v9: &[f32], v8: &[f32], local: [f32; 2], cell: [usize; 2]) -> f32 {
    let index = cell[0] * 129 + cell[1];
    let center = cell[0] * 128 + cell[1];
    let (a, b, c) = coefficients::<_, std::convert::Infallible>(
        local,
        |i| Ok(v9[index + i]),
        || Ok(2.0 * v8[center]),
    )
    .unwrap();
    // GridMap.cpp:372. Do not replace with bilinear interpolation or mul_add.
    a * local[0] + b * local[1] + c
}

fn coefficients<T, E>(
    [x, y]: [f32; 2],
    h: impl Fn(usize) -> Result<T, E>,
    center: impl FnOnce() -> Result<T, E>,
) -> Result<(T, T, T), E>
where
    T: Copy + Add<Output = T> + Sub<Output = T>,
{
    // GridMap.cpp:325-368,392-435,460-503: strict centre-fan predicates,
    // and the original subtraction/addition order in each of the four planes.
    if x + y < 1.0 {
        if x > y {
            let (h1, h2) = (h(0)?, h(129)?);
            let h5 = center()?;
            // GridMap.cpp:333-335,400-402,468-470.
            Ok((h2 - h1, h5 - h1 - h2, h1))
        } else {
            let (h1, h3) = (h(0)?, h(1)?);
            let h5 = center()?;
            // GridMap.cpp:343-345,410-412,478-480.
            Ok((h5 - h1 - h3, h3 - h1, h1))
        }
    } else if x > y {
        let (h2, h4) = (h(129)?, h(130)?);
        let h5 = center()?;
        // GridMap.cpp:356-358,423-425,491-493.
        Ok((h2 + h4 - h5, h4 - h2, h5 - h4))
    } else {
        let (h3, h4) = (h(1)?, h(130)?);
        let h5 = center()?;
        // GridMap.cpp:366-368,433-435,501-503.
        Ok((h4 - h3, h3 + h4 - h5, h5 - h4))
    }
}

/// GridMap.cpp:828-884. The caller has already performed full GetGrid, even
/// with VMaps disabled. Callback errors stop at the exact reached VMap call.
pub fn select_height<E>(
    map_height: f32,
    z: f32,
    use_vmaps: bool,
    height_enabled: bool,
    mut max_search: f32,
    mut query: impl FnMut(f32, f32) -> Result<f32, E>,
) -> Result<f32, E> {
    let mut vmap_height = INVALID_HEIGHT_VALUE;
    if use_vmaps && height_enabled {
        let z2 = z + 2.0; // GridMap.cpp:841.
        if map_height > INVALID_HEIGHT && z2 - map_height > max_search {
            max_search = z2 - map_height + 1.0; // GridMap.cpp:846-847.
        }
        vmap_height = query(z2, max_search)?; // GridMap.cpp:850.
        if vmap_height <= INVALID_HEIGHT {
            vmap_height = query(z2, 10_000.0)?; // GridMap.cpp:853-854.
        }
        if vmap_height <= INVALID_HEIGHT && map_height > z2 && (z2 - map_height).abs() > 30.0 {
            vmap_height = query(z2, -max_search)?; // GridMap.cpp:857-858.
        }
        if vmap_height <= INVALID_HEIGHT && map_height > INVALID_HEIGHT && z2 < map_height {
            vmap_height = query(map_height + 2.0, DEFAULT_HEIGHT_SEARCH)?; // GridMap.cpp:861-862.
        }
    }
    // GridMap.cpp:868-884. Equality selects terrain, including its signed zero.
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
mod tests;
