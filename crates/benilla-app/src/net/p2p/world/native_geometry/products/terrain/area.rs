// SPDX-License-Identifier: GPL-3.0-or-later
//! Pinned GridMap AREA keys and complete AreaTable.dbc rows for liquid overrides.
//! Source: CMaNGOS 8ec338a1 GridMap.cpp, DBCStructure.h and DBCfmt.h.
use super::super::Hash;
use super::{u16_at, u32_at, Error, GRID_SIZE};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const MAX_ROWS: usize = 65_536; // Local resource limits, not DBC or game rules.
const MAX_STRINGS: usize = 1024 * 1024;
const FIELDS: usize = 25;
const STRIDE: usize = FIELDS * 4;

#[derive(Debug)]
pub(super) struct GridAreas {
    global: u16,
    cells: Option<Box<[u16; 256]>>,
}

impl GridAreas {
    pub(super) fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.get(..4) != Some(b"AREA") {
            return Err(Error::Format);
        }
        let flags = u16_at(bytes, 4)?;
        let cells = match flags {
            0 if bytes.len() == 520 => {
                let mut cells = Box::new([0; 256]);
                for (i, cell) in cells.iter_mut().enumerate() {
                    *cell = u16_at(bytes, 8 + i * 2)?;
                }
                Some(cells)
            }
            1 if bytes.len() == 8 => None,
            0 | 1 => return Err(Error::Framing),
            _ => return Err(Error::Unsupported),
        };
        Ok(Self {
            global: u16_at(bytes, 6)?,
            cells,
        })
    }

    /// Caller has already bounded the world position to its TerrainTile.
    pub(super) fn get(&self, x: f32, y: f32) -> u16 {
        let Some(cells) = &self.cells else {
            return self.global;
        };
        // GridMap::getArea computes at resolution 16, independently of liquid's 128.
        let x = 16.0 * (32.0 - x / GRID_SIZE);
        let y = 16.0 * (32.0 - y / GRID_SIZE);
        let lx = x as usize & 15;
        let ly = y as usize & 15;
        cells[lx * 16 + ly]
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct AreaEntry {
    pub(super) zone: u32,
    pub(super) liquid_override: u32,
}

/// Complete bounded niiiixxxxxissssssssxixxxi data, keyed by the raw u32 ID.
/// A missing row is absent from these exact bytes, never an authored closure guess.
/// Loading/hashing the file establishes no provenance or gameplay authority.
#[derive(Debug)]
pub(crate) struct AreaTable {
    pub(super) entries: BTreeMap<u32, AreaEntry>,
    digest: Hash,
}

impl AreaTable {
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.get(..4) != Some(b"WDBC") {
            return Err(Error::Format);
        }
        let rows = u32_at(bytes, 4)? as usize;
        let fields = u32_at(bytes, 8)? as usize;
        let stride = u32_at(bytes, 12)? as usize;
        let strings = u32_at(bytes, 16)? as usize;
        if rows > MAX_ROWS || strings > MAX_STRINGS {
            return Err(Error::Budget);
        }
        if fields != FIELDS || stride != STRIDE {
            return Err(Error::Unsupported);
        }
        let strings_at = rows
            .checked_mul(STRIDE)
            .and_then(|n| n.checked_add(20))
            .ok_or(Error::Framing)?;
        if strings == 0
            || strings_at.checked_add(strings) != Some(bytes.len())
            || bytes.get(strings_at) != Some(&0)
            || bytes.last() != Some(&0)
        {
            return Err(Error::Framing);
        }
        let mut entries = BTreeMap::new();
        for row in 0..rows {
            let at = 20 + row * STRIDE;
            // All eight locale offsets must point into the complete string block.
            // Its validated final NUL terminates every in-bounds string without scans.
            for field in 11..=18 {
                if u32_at(bytes, at + field * 4)? as usize >= strings {
                    return Err(Error::Framing);
                }
            }
            let id = u32_at(bytes, at)?;
            // DBCFileLoader::AutoProduceData increments its u32 maximum ID to size
            // the index; UINT32_MAX would wrap and is outside its valid store domain.
            if id == u32::MAX {
                return Err(Error::Unsupported);
            }
            let entry = AreaEntry {
                zone: u32_at(bytes, at + 2 * 4)?,
                liquid_override: u32_at(bytes, at + 24 * 4)?,
            };
            if entries.insert(id, entry).is_some() {
                return Err(Error::Duplicate);
            }
        }
        Ok(Self {
            entries,
            digest: Sha256::digest(bytes).into(),
        })
    }

    pub(crate) fn digest(&self) -> Hash {
        self.digest
    }
}

#[cfg(test)]
mod tests;
