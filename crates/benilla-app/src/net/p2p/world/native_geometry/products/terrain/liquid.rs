// SPDX-License-Identifier: GPL-3.0-or-later
//! Pinned GridMap::loadGridMapLiquidData/getLiquidStatus, not TerrainInfo selection.
//! Source: CMaNGOS 8ec338a1 GridMap.cpp and DBCStructure.h/DBCfmt.h (niii).
use super::super::Hash;
use super::{
    height_number, u16_at, u32_at, AreaTable, Error, PointHeight, TerrainTile, MAX_HEIGHT,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const NO_TYPE: u8 = 1;
const NO_HEIGHT: u8 = 2;
const DEEP_WATER: u8 = 0x10;
const KNOWN_TYPES: u8 = 0x3f;
const INVALID_HEIGHT_VALUE: f32 = -200_000.0;
const MAX_ROWS: usize = 65_536; // Local resource limits, not DBC or game rules.
const MAX_STRINGS: usize = 1024 * 1024;

/// A complete, bounded LiquidType.dbc table. Missing IDs are absent from these exact
/// bytes, not guessed from an authored subset; loading establishes no source provenance.
#[derive(Debug)]
pub(crate) struct LiquidTypes {
    entries: BTreeMap<u32, u32>,
    digest: Hash,
}
impl LiquidTypes {
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, Error> {
        Self::decode_profile(bytes, false)
    }

    /// Opt-in TerrainInfo store: source IDs and Type are uint32. Unsupported
    /// shifts are rejected only if the selected query branch executes them.
    pub(in super::super) fn decode_complete(bytes: &[u8]) -> Result<Self, Error> {
        Self::decode_profile(bytes, true)
    }

    fn decode_profile(bytes: &[u8], complete: bool) -> Result<Self, Error> {
        if bytes.get(..4) != Some(b"WDBC") {
            return Err(Error::Format);
        }
        let rows = u32_at(bytes, 4)? as usize;
        let fields = u32_at(bytes, 8)?;
        let stride = u32_at(bytes, 12)?;
        let strings = u32_at(bytes, 16)? as usize;
        if rows > MAX_ROWS || strings > MAX_STRINGS {
            return Err(Error::Budget);
        }
        if fields != 4 || stride != 16 {
            return Err(Error::Unsupported);
        }
        let strings_at = rows
            .checked_mul(16)
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
        for i in 0..rows {
            let at = 20 + i * 16;
            let id = u32_at(bytes, at)?;
            // niii: Id, LiquidId, Type, SpellId. Only Id and Type affect this query.
            let kind = u32_at(bytes, at + 8)?;
            if id == u32::MAX || (!complete && (id > u16::MAX as u32 || kind > 3)) {
                return Err(Error::Unsupported);
            }
            if entries.insert(id, kind).is_some() {
                return Err(Error::Duplicate);
            }
        }
        Ok(Self {
            entries,
            digest: Sha256::digest(bytes).into(),
        })
    }

    pub(in super::super) fn kind(&self, id: u32) -> Option<u32> {
        self.entries.get(&id).copied()
    }

    pub(crate) fn digest(&self) -> Hash {
        self.digest
    }
}

/// Complete decoded stores needed by the bounded GridMap query, never admission proof.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LiquidStores<'a> {
    pub types: &'a LiquidTypes,
    pub areas: Option<&'a AreaTable>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub(crate) enum GridLiquidStatus {
    NoWater = 0,
    AboveWater = 1,
    WaterWalk = 2,
    InWater = 4,
    UnderWater = 8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GridLiquidData {
    pub entry: u32,
    pub type_flags: u32,
    /// Exact source binary32 bits, including the hole sentinel for depth_level.
    pub level: u32,
    pub depth_level: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GridLiquidResult {
    pub status: GridLiquidStatus,
    pub data: Option<GridLiquidData>,
}
impl GridLiquidResult {
    pub(in super::super) fn no_water() -> Self {
        Self {
            status: GridLiquidStatus::NoWater,
            data: None,
        }
    }
}

#[derive(Debug)]
struct CellTypes {
    entries: [u16; 256],
    flags: [u8; 256],
}
#[derive(Debug)]
pub(super) struct GridLiquid {
    entry: u16,
    flags: u8,
    offset_x: u8,
    offset_y: u8,
    width: u8,
    height: u8,
    level: f32,
    cell_types: Option<Box<CellTypes>>,
    levels: Option<Box<[f32]>>,
}
impl GridLiquid {
    pub(super) fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.get(..4) != Some(b"MLIQ") {
            return Err(Error::Format);
        }
        let header = bytes.get(..16).ok_or(Error::Framing)?;
        let flags = header[4];
        if flags & !(NO_TYPE | NO_HEIGHT) != 0 || header[5] & !KNOWN_TYPES != 0 {
            return Err(Error::Unsupported);
        }
        let offset_x = header[8];
        let offset_y = header[9];
        let width = header[10];
        let height = header[11];
        // Writer bounds span visible cells plus the final vertex: 129 is valid.
        if offset_x >= 128
            || offset_y >= 128
            || width == 0
            || height == 0
            || u16::from(offset_x) + u16::from(width) > 129
            || u16::from(offset_y) + u16::from(height) > 129
        {
            return Err(Error::Format);
        }
        let count = usize::from(width)
            .checked_mul(usize::from(height))
            .ok_or(Error::Framing)?;
        let heights_at = if flags & NO_TYPE == 0 {
            16usize + 768
        } else {
            16
        };
        let height_bytes = if flags & NO_HEIGHT == 0 {
            count.checked_mul(4).ok_or(Error::Framing)?
        } else {
            0
        };
        if heights_at.checked_add(height_bytes) != Some(bytes.len()) {
            return Err(Error::Framing);
        }
        let level = height_number(bytes, 12)?;
        let cell_types = if flags & NO_TYPE == 0 {
            let mut entries = [0; 256];
            let mut cell_flags = [0; 256];
            for (i, entry) in entries.iter_mut().enumerate() {
                *entry = u16_at(bytes, 16 + i * 2)?;
                let value = bytes[16 + 512 + i];
                if value & !KNOWN_TYPES != 0 {
                    return Err(Error::Unsupported);
                }
                cell_flags[i] = value;
            }
            Some(Box::new(CellTypes {
                entries,
                flags: cell_flags,
            }))
        } else {
            None
        };
        let levels = if flags & NO_HEIGHT == 0 {
            Some(
                (0..count)
                    .map(|i| height_number(bytes, heights_at + i * 4))
                    .collect::<Result<Vec<_>, _>>()?
                    .into_boxed_slice(),
            )
        } else {
            None
        };
        Ok(Self {
            entry: u16_at(bytes, 6)?,
            flags: header[5],
            offset_x,
            offset_y,
            width,
            height,
            level,
            cell_types,
            levels,
        })
    }
}

impl TerrainTile {
    /// Compatibility entry point without AreaTable data; matched entries <21 need it.
    pub(crate) fn grid_liquid_status(
        &self,
        x: f32,
        y: f32,
        z: f32,
        required_mask: u8,
        collision_height: f32,
        types: &LiquidTypes,
    ) -> Result<GridLiquidResult, Error> {
        self.grid_liquid_status_with_stores(
            x,
            y,
            z,
            required_mask,
            collision_height,
            LiquidStores { types, areas: None },
        )
    }

    /// Pinned GridMap query only, including exact area/one-parent liquid overrides.
    /// No VMap selection, cached player water state, or gameplay authority.
    pub(crate) fn grid_liquid_status_with_stores(
        &self,
        x: f32,
        y: f32,
        z: f32,
        required_mask: u8,
        collision_height: f32,
        stores: LiquidStores<'_>,
    ) -> Result<GridLiquidResult, Error> {
        self.grid_liquid_status_impl(
            [x, y, z],
            required_mask,
            collision_height,
            stores,
            || Ok(()),
        )
    }

    /// Composite-only entry shares the caller's absolute deadline and work allowance.
    pub(in super::super) fn grid_liquid_status_with_budget(
        &self,
        position: [f32; 3],
        required_mask: u8,
        collision_height: f32,
        stores: LiquidStores<'_>,
        budget: &mut super::super::vmap::QueryBudget,
    ) -> Result<GridLiquidResult, Error> {
        self.grid_liquid_status_impl(position, required_mask, collision_height, stores, || {
            budget.take()
        })
    }

    fn grid_liquid_status_impl(
        &self,
        position: [f32; 3],
        required_mask: u8,
        collision_height: f32,
        stores: LiquidStores<'_>,
        mut take: impl FnMut() -> Result<(), Error>,
    ) -> Result<GridLiquidResult, Error> {
        take()?;
        let [x, y, z] = position;
        // Local input guards are independent of the source branch ordering below.
        if !z.is_finite()
            || z.abs() > MAX_HEIGHT
            || !collision_height.is_finite()
            || collision_height.abs() > MAX_HEIGHT
        {
            return Err(Error::Format);
        }
        let (_, _, x_int, y_int) = self.coordinates(x, y)?;
        let Some(liquid) = &self.liquid else {
            return Ok(GridLiquidResult::no_water());
        };
        if liquid.cell_types.is_none() && liquid.flags == 0 {
            return Ok(GridLiquidResult::no_water());
        }
        let idx = (x_int >> 3) * 16 + (y_int >> 3);
        let (mut entry, mut kind) = match &liquid.cell_types {
            Some(cells) => (u32::from(cells.entries[idx]), cells.flags[idx]),
            None => (u32::from(liquid.entry), liquid.flags),
        };
        take()?;
        if let Some(source_type) = stores.types.kind(entry) {
            kind &= DEEP_WATER;
            let mut liq_type_idx = source_type;
            // The source uses getArea's raw key as an AreaTable ID, not exploreFlag.
            // Its area lookup precedes zero/mask/rectangle exits and follows one parent.
            if entry < 21 {
                take()?;
                let areas = stores.areas.ok_or(Error::Unsupported)?;
                if let Some(area) = areas.entries.get(&u32::from(self.areas.get(x, y))) {
                    let mut override_liquid = area.liquid_override;
                    if override_liquid == 0 && area.zone != 0 {
                        take()?;
                        if let Some(parent) = areas.entries.get(&area.zone) {
                            override_liquid = parent.liquid_override;
                        }
                    }
                    // Zero and full uint32 overrides are real lookups; the old
                    // conservative decoder still excludes IDs above uint16.
                    take()?;
                    if let Some(override_type) = stores.types.kind(override_liquid) {
                        entry = override_liquid;
                        liq_type_idx = override_type;
                    }
                }
            }
            kind |= source_type_flag(liq_type_idx)? as u8;
        }
        if kind == 0 || (required_mask != 0 && required_mask & kind == 0) {
            return Ok(GridLiquidResult::no_water());
        }
        // Deliberate source axis order: x subtracts offY; y subtracts offX.
        let Some(lx) = x_int.checked_sub(usize::from(liquid.offset_y)) else {
            return Ok(GridLiquidResult::no_water());
        };
        let Some(ly) = y_int.checked_sub(usize::from(liquid.offset_x)) else {
            return Ok(GridLiquidResult::no_water());
        };
        if lx >= usize::from(liquid.height) || ly >= usize::from(liquid.width) {
            return Ok(GridLiquidResult::no_water());
        }
        take()?;
        let liquid_level = match &liquid.levels {
            Some(levels) => levels[lx * usize::from(liquid.width) + ly],
            None => liquid.level,
        };
        let ground_level = match self.point_height(x, y)? {
            PointHeight::Height(height) => height,
            PointHeight::Hole => INVALID_HEIGHT_VALUE,
        };
        if liquid_level < ground_level || z < ground_level - 2.0 {
            return Ok(GridLiquidResult::no_water());
        }
        let data = GridLiquidData {
            entry,
            type_flags: u32::from(kind),
            level: liquid_level.to_bits(),
            depth_level: ground_level.to_bits(),
        };
        take()?;
        let status = liquid_status(liquid_level, z, collision_height);
        Ok(GridLiquidResult {
            status,
            data: Some(data),
        })
    }
}

/// C++17 signed 1 << 31 has the defined uint32 result 0x80000000; >=32 is UB.
pub(in super::super) fn source_type_flag(kind: u32) -> Result<u32, Error> {
    1u32.checked_shl(kind).ok_or(Error::Unsupported)
}

pub(in super::super) fn liquid_status(
    level: f32,
    z: f32,
    collision_height: f32,
) -> GridLiquidStatus {
    let delta = level - z;
    if delta > collision_height {
        GridLiquidStatus::UnderWater
    } else if delta > 0.0 {
        GridLiquidStatus::InWater
    } else if delta > -1.0 {
        GridLiquidStatus::WaterWalk
    } else {
        GridLiquidStatus::AboveWater
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod oracle_tests;

#[cfg(test)]
mod area_oracle_tests;
