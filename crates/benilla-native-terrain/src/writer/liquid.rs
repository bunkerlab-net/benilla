// SPDX-License-Identifier: GPL-3.0-or-later
//! Pinned ConvertADT MCLQ scan/MLIQ packing and exact boundary-history projection.
//! Unknown serialized values reject unless a private raw-input proof resolves them.
use super::{word, BoundaryHistory, Cell, CELLS};
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use std::collections::BTreeMap;

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

struct PreparedLiquid {
    entries: [u16; CELLS],
    flags: [u8; CELLS],
    heights: Vec<Option<f32>>,
    full_type: bool,
    min_x: usize,
    min_y: usize,
    width: usize,
    height: usize,
    min_height: f32,
    flat: bool,
}

fn visit_cell_heights(
    slot: usize,
    data: &[u8],
    mut write: impl FnMut(usize, usize, f32),
) -> Result<()> {
    ensure!(data.len() == 804, "terrain.mclq_length");
    let (row, col) = (slot / 16, slot % 16);
    for y in 0..=8 {
        for x in 0..=8 {
            let height = f32::from_bits(word(data, 12 + (y * 9 + x) * 8)?);
            ensure!(height.is_finite(), "terrain.liquid_nonfinite");
            write(col * 8 + x, row * 8 + y, height);
        }
    }
    Ok(())
}

/// Masking and packing never alter row/column 128. Only the last MCIN row or
/// column (31 cells) can write them; all active MCLQ heights are still validated.
/// Hidden, untyped, and no-output cells participate before source output logic.
pub(super) fn direct_boundary_writes(cells: &[Cell<'_>]) -> Result<BTreeMap<usize, f32>> {
    let mut writes = BTreeMap::new();
    for (slot, cell) in cells.iter().enumerate() {
        if let Some(data) = cell.liquid {
            visit_cell_heights(slot, data, |x, y, value| {
                if x == 128 || y == 128 {
                    // Forward MCIN traversal preserves the last seam/corner writer.
                    writes.insert(y * 129 + x, value);
                }
            })?;
        }
    }
    Ok(writes)
}

pub(super) fn missing_boundary(cells: &[Cell<'_>]) -> Result<BTreeMap<usize, Option<f32>>> {
    let mut missing = BTreeMap::new();
    if let Some(prepared) = prepare(cells)? {
        for at in prepared.missing() {
            ensure!(
                at % 129 == 128 || at / 129 == 128,
                "terrain.liquid_history_interior"
            );
            missing.insert(at, None);
        }
    }
    Ok(missing)
}

pub(super) fn produce_with_history(
    cells: &[Cell<'_>],
    history: Option<&BoundaryHistory>,
) -> Result<Vec<u8>> {
    match prepare(cells)? {
        Some(prepared) => prepared.pack(history),
        None => Ok(Vec::new()),
    }
}

pub(super) fn produce_observed(
    cells: &[Cell<'_>],
    observation: &mut Option<(LiquidRectangle, MissingLiquidHeights)>,
) -> Result<Vec<u8>> {
    let Some(prepared) = prepare(cells)? else {
        return Ok(Vec::new());
    };
    let mut missing = prepared.missing();
    let first = missing.next();
    *observation = Some((
        LiquidRectangle {
            x: prepared.min_x as u8,
            y: prepared.min_y as u8,
            width: prepared.width as u8,
            height: prepared.height as u8,
        },
        MissingLiquidHeights {
            count: usize::from(first.is_some()) + missing.count(),
            first_x: first.map(|at| (at % 129) as u8),
            first_y: first.map(|at| (at / 129) as u8),
        },
    ));
    prepared.pack(None)
}

fn prepare(cells: &[Cell<'_>]) -> Result<Option<PreparedLiquid>> {
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
        // Even hidden/untyped active cells overwrite shared seams in MCIN order.
        visit_cell_heights(slot, data, |x, y, value| heights[y * 129 + x] = Some(value))?;
    }
    let full_type = entries
        .iter()
        .zip(&flags)
        .any(|(&entry, &flag)| entry != entries[0] || flag != flags[0]);
    if flags[0] == 0 && !full_type {
        return Ok(None);
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
    Ok(Some(PreparedLiquid {
        entries,
        flags,
        heights,
        full_type,
        min_x,
        min_y,
        width: max_x - min_x + 2,
        height: max_y - min_y + 2,
        min_height,
        // Exact -f 0; hidden cells never raise maxHeight.
        flat: max_height == min_height,
    }))
}

impl PreparedLiquid {
    fn missing(&self) -> impl Iterator<Item = usize> + '_ {
        (self.min_y..self.min_y + self.height)
            .flat_map(|y| (self.min_x..self.min_x + self.width).map(move |x| y * 129 + x))
            .filter(|&at| !self.flat && self.heights[at].is_none())
    }

    fn pack(self, history: Option<&BoundaryHistory>) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(16 + 768 + self.width * self.height * 4);
        out.extend_from_slice(b"MLIQ");
        out.push(u8::from(!self.full_type) | (u8::from(self.flat) << 1));
        out.push(if self.full_type { 0 } else { self.flags[0] });
        out.extend_from_slice(&(if self.full_type { 0 } else { self.entries[0] }).to_le_bytes());
        out.extend_from_slice(&[
            self.min_x as u8,
            self.min_y as u8,
            self.width as u8,
            self.height as u8,
        ]);
        out.extend_from_slice(&self.min_height.to_le_bytes());
        if self.full_type {
            for entry in self.entries {
                out.extend_from_slice(&entry.to_le_bytes());
            }
            out.extend_from_slice(&self.flags);
        }
        if !self.flat {
            for y in self.min_y..self.min_y + self.height {
                for x in self.min_x..self.min_x + self.width {
                    let at = y * 129 + x;
                    let value = self.heights[at]
                        .or_else(|| history.and_then(|h| h.heights.get(&at).copied().flatten()))
                        .context("terrain.liquid_history")?;
                    out.extend_from_slice(&value.to_le_bytes());
                }
            }
        }
        Ok(out)
    }
}
