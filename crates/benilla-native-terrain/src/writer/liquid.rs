// SPDX-License-Identifier: GPL-3.0-or-later
//! Pinned ConvertADT MCLQ scan/MLIQ packing, with explicit selected-tile height ownership.
//! The source retains global edge heights between tiles. Unknown serialized values reject;
//! no initial zero or caller-supplied prior product substitutes for that source history.
use super::{word, Cell, CELLS};
use anyhow::{ensure, Context, Result};
use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct LiquidRectangle {
    x: u8,
    y: u8,
    width: u8,
    height: u8,
}

#[derive(Serialize)]
pub(crate) struct MissingLiquidHeights {
    count: usize,
    first_x: Option<u8>,
    first_y: Option<u8>,
}

pub(super) fn produce(cells: &[Cell<'_>]) -> Result<Vec<u8>> {
    produce_observed(cells, &mut None)
}

pub(super) fn produce_observed(
    cells: &[Cell<'_>],
    observation: &mut Option<(LiquidRectangle, MissingLiquidHeights)>,
) -> Result<Vec<u8>> {
    let mut entries = [0_u16; CELLS];
    let mut flags = [0_u8; CELLS];
    let mut shown = vec![false; 128 * 128];
    let mut heights = vec![None; 129 * 129];
    for (slot, cell) in cells.iter().enumerate() {
        let Some(data) = cell.liquid else { continue };
        ensure!(data.len() == 804, "terrain.mclq_length");
        let (row, col) = (slot / 16, slot % 16);
        for y in 0..8 {
            for x in 0..8 {
                let value = data[656 + y * 8 + x];
                if value != 0x0f {
                    shown[(row * 8 + y) * 128 + col * 8 + x] = true;
                    if value & 0x80 != 0 {
                        flags[slot] |= 0x10;
                    }
                }
            }
        }
        // Independent source conditions: class flags accumulate; the last entry wins.
        for (mask, entry, kind) in [(0x4, 1, 0x8), (0x8, 2, 0x2), (0x10, 3, 0x1)] {
            if cell.flags & mask != 0 {
                entries[slot] = entry;
                flags[slot] |= kind;
            }
        }
        for y in 0..=8 {
            for x in 0..=8 {
                let height = f32::from_bits(word(data, 12 + (y * 9 + x) * 8)?);
                ensure!(height.is_finite(), "terrain.liquid_nonfinite");
                // Even hidden/untyped active cells overwrite shared seams in MCIN order.
                heights[(row * 8 + y) * 129 + col * 8 + x] = Some(height);
            }
        }
    }
    let full_type = entries
        .iter()
        .zip(&flags)
        .any(|(&entry, &flag)| entry != entries[0] || flag != flags[0]);
    if flags[0] == 0 && !full_type {
        return Ok(Vec::new());
    }
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (255, 255, 0, 0);
    let (mut min_height, mut max_height) = (20000.0_f32, -20000.0_f32);
    for y in 0..128 {
        for x in 0..128 {
            let at = y * 129 + x;
            if shown[y * 128 + x] {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
                let height = heights[at].context("terrain.liquid_history")?;
                if max_height < height {
                    max_height = height;
                }
                if min_height > height {
                    min_height = height;
                }
            } else {
                heights[at] = Some(-500.0);
                if min_height > -500.0 {
                    min_height = -500.0;
                }
            }
        }
    }
    // Source's typed/no-visible branch would serialize an invalid, out-of-bounds rectangle.
    ensure!(
        min_x <= max_x && min_y <= max_y,
        "terrain.liquid_empty_bounds"
    );
    let (width, height) = (max_x - min_x + 2, max_y - min_y + 2);
    // Exact -f 0; hidden cells never raise maxHeight.
    let flat = max_height == min_height;
    // Observe exactly the serialized rectangle after hidden interior cells have
    // been assigned -500. Flat output never consumes this height payload.
    let mut missing = MissingLiquidHeights {
        count: 0,
        first_x: None,
        first_y: None,
    };
    if !flat {
        for y in min_y..min_y + height {
            for x in min_x..min_x + width {
                if heights[y * 129 + x].is_none() {
                    missing.count += 1;
                    if missing.first_x.is_none() {
                        missing.first_x = Some(x as u8);
                        missing.first_y = Some(y as u8);
                    }
                }
            }
        }
    }
    *observation = Some((
        LiquidRectangle {
            x: min_x as u8,
            y: min_y as u8,
            width: width as u8,
            height: height as u8,
        },
        missing,
    ));
    let mut out = Vec::with_capacity(16 + 768 + width * height * 4);
    out.extend_from_slice(b"MLIQ");
    out.push(u8::from(!full_type) | (u8::from(flat) << 1));
    out.push(if full_type { 0 } else { flags[0] });
    out.extend_from_slice(&(if full_type { 0 } else { entries[0] }).to_le_bytes());
    out.extend_from_slice(&[min_x as u8, min_y as u8, width as u8, height as u8]);
    out.extend_from_slice(&min_height.to_le_bytes());
    if full_type {
        for entry in entries {
            out.extend_from_slice(&entry.to_le_bytes());
        }
        out.extend_from_slice(&flags);
    }
    if !flat {
        for y in min_y..min_y + height {
            for x in min_x..min_x + width {
                let value = heights[y * 129 + x].context("terrain.liquid_history")?;
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
    }
    Ok(out)
}
