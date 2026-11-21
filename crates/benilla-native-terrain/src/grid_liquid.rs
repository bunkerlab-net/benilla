// SPDX-License-Identifier: GPL-3.0-or-later
//! CMaNGOS 8ec338a1 GridMap.cpp liquid loading, selection and water wrappers.
use crate::grid_height::{self, INVALID_HEIGHT_VALUE};
use std::sync::Arc;

pub const DEFAULT_WATER_SEARCH: f32 = 50.0;
pub const DEFAULT_COLLISION_HEIGHT: f32 = 2.03128;
pub const ALL_LIQUIDS: u8 = 15;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    HeaderReadUndefined,
    Magic,
    InvalidHeaderFlags(u8),
    CoordinateConversionUndefined,
    EntryReadUndefined,
    FlagsReadUndefined,
    HeightReadUndefined,
    TypeShiftUndefined,
    LiquidTypeStoreUnavailable,
    AreaStoreUnavailable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Status {
    NoWater = 0,
    AboveWater = 1,
    WaterWalk = 2,
    InWater = 4,
    UnderWater = 8,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LiquidData {
    pub entry: u32,
    pub type_flags: u32,
    pub level: f32,
    pub depth_level: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct Area {
    pub zone: u32,
    pub liquid_override: u32,
}
/// Complete stores distinguish a missing row from an unbound store.
pub trait Lookup {
    type Error: From<Error>;
    fn liquid_type(&self, id: u32) -> Result<Option<u32>, Self::Error>;
    fn area(&self, id: u32) -> Result<Option<Area>, Self::Error>;
}
pub trait GridOwner: Lookup {
    fn area_key(&self, x: f32, y: f32) -> Result<u16, Self::Error>;
    fn height(&self, x: f32, y: f32) -> Result<f32, Self::Error>;
}

/// Retains source bytes, including short fread tails, without copying arrays.
#[derive(Debug)]
pub struct GridLiquid {
    pub entry: u16,
    pub flags: u8,
    pub offset: [u8; 2],
    pub size: [u8; 2],
    pub level: f32,
    source: Option<(Arc<[u8]>, usize)>,
    has_types: bool,
    has_heights: bool,
}
impl Default for GridLiquid {
    fn default() -> Self {
        Self {
            entry: 0,
            flags: 0,
            offset: [0; 2],
            size: [0; 2],
            level: INVALID_HEIGHT_VALUE,
            source: None,
            has_types: false,
            has_heights: false,
        }
    }
}
impl GridLiquid {
    /// GridMap.cpp:227-258. Section sizes are ignored; arrays follow the header.
    pub fn decode(source: Arc<[u8]>, offset: usize) -> Result<Self, Error> {
        let header = source
            .get(offset..)
            .unwrap_or_default()
            .get(..16)
            .ok_or(Error::HeaderReadUndefined)?;
        if &header[..4] != b"MLIQ" {
            return Err(Error::Magic);
        }
        if header[4] & !3 != 0 {
            return Err(Error::InvalidHeaderFlags(header[4]));
        }
        Ok(Self {
            entry: u16::from_le_bytes([header[6], header[7]]),
            flags: header[5],
            offset: [header[8], header[9]],
            size: [header[10], header[11]],
            level: f32::from_le_bytes(header[12..16].try_into().unwrap()),
            has_types: header[4] & 1 == 0,
            has_heights: header[4] & 2 == 0,
            source: Some((source, offset + 16)),
        })
    }
    fn bytes(&self) -> &[u8] {
        self.source
            .as_ref()
            .map_or(&[], |(s, o)| s.get(*o..).unwrap_or_default())
    }
    fn local(x: f32, y: f32) -> Result<[usize; 2], Error> {
        grid_height::coordinates(x, y)
            .map(|(_, c)| c)
            .map_err(|_| Error::CoordinateConversionUndefined)
    }
    fn rectangle(&self, cell: [usize; 2]) -> Option<usize> {
        // GridMap.cpp:519-528,598-607 deliberately subtracts swapped offsets.
        let row = cell[0].checked_sub(usize::from(self.offset[1]))?;
        if row >= usize::from(self.size[1]) {
            return None;
        }
        let col = cell[1].checked_sub(usize::from(self.offset[0]))?;
        if col >= usize::from(self.size[0]) {
            return None;
        }
        Some(row * usize::from(self.size[0]) + col)
    }
    fn sample(&self, index: usize) -> Result<f32, Error> {
        if !self.has_heights {
            return Ok(self.level);
        }
        let at = usize::from(self.has_types) * 768 + index * 4;
        self.bytes()
            .get(at..at + 4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .ok_or(Error::HeightReadUndefined)
    }
    /// GridMap.cpp:511-528. Flat levels return before coordinates or rectangle.
    pub fn level(&self, x: f32, y: f32) -> Result<f32, Error> {
        if !self.has_heights {
            return Ok(self.level);
        }
        self.rectangle(Self::local(x, y)?)
            .map_or(Ok(INVALID_HEIGHT_VALUE), |i| self.sample(i))
    }
    /// GridMap.cpp:531-540. This uses 16, not the status query's 128 resolution.
    pub fn terrain_type(&self, x: f32, y: f32) -> Result<u8, Error> {
        if !self.has_types {
            return Ok(self.flags);
        }
        let axis = |v: f32| {
            let v = 16.0 * (32.0 - v / grid_height::GRID_SIZE);
            if !v.is_finite() || !(-2147483648.0..2147483648.0).contains(&v) {
                return Err(Error::CoordinateConversionUndefined);
            }
            Ok((v as i32 & 15) as usize)
        };
        self.bytes()
            .get(512 + axis(x)? * 16 + axis(y)?)
            .copied()
            .ok_or(Error::FlagsReadUndefined)
    }
    /// GridMap.cpp:544-638. No early return writes the caller's data.
    pub fn status<O: GridOwner>(
        &self,
        p: [f32; 3],
        mask: u8,
        collision: f32,
        data: Option<&mut LiquidData>,
        owner: &O,
    ) -> Result<Status, O::Error> {
        if !self.has_types && self.flags == 0 {
            return Ok(Status::NoWater);
        }
        let [x, y, z] = p;
        let cell = Self::local(x, y)?;
        let idx = (cell[0] >> 3) * 16 + (cell[1] >> 3);
        // GridMap.cpp:559-560 reads flags before entries, even for a zero flag.
        let mut flags = if self.has_types {
            *self
                .bytes()
                .get(512 + idx)
                .ok_or(Error::FlagsReadUndefined)?
        } else {
            self.flags
        };
        let mut entry = if self.has_types {
            u32::from(u16::from_le_bytes(
                self.bytes()
                    .get(idx * 2..idx * 2 + 2)
                    .ok_or(Error::EntryReadUndefined)?
                    .try_into()
                    .unwrap(),
            ))
        } else {
            u32::from(self.entry)
        };
        if let Some(mut kind) = owner.liquid_type(entry)? {
            flags &= 0x10;
            if entry < 21 {
                if let Some(area) = owner.area(u32::from(owner.area_key(x, y)?))? {
                    if let Some((id, replacement)) = override_type(owner, area)? {
                        entry = id;
                        kind = replacement;
                    }
                }
            }
            flags |= type_flag(kind)? as u8;
        }
        if flags == 0 || (mask != 0 && mask & flags == 0) {
            return Ok(Status::NoWater);
        }
        let Some(index) = self.rectangle(cell) else {
            return Ok(Status::NoWater);
        };
        let level = self.sample(index)?;
        let ground = owner.height(x, y)?;
        if level < ground || z < ground - 2.0 {
            return Ok(Status::NoWater);
        }
        if let Some(out) = data {
            *out = LiquidData {
                entry,
                type_flags: u32::from(flags),
                level,
                depth_level: ground,
            };
        }
        Ok(classify(level, z, collision))
    }
}
fn override_type<O: Lookup>(owner: &O, area: Area) -> Result<Option<(u32, u32)>, O::Error> {
    let mut id = area.liquid_override;
    if id == 0 && area.zone != 0 {
        if let Some(parent) = owner.area(area.zone)? {
            id = parent.liquid_override;
        }
    }
    Ok(owner.liquid_type(id)?.map(|kind| (id, kind)))
}
/// GridMap.cpp:586,1098. C++17 defines 1<<31; shift counts >=32 are undefined.
pub fn type_flag(kind: u32) -> Result<u32, Error> {
    1u32.checked_shl(kind).ok_or(Error::TypeShiftUndefined)
}
/// GridMap.cpp:629-638,1105-1111. Each boundary is strictly greater.
pub fn classify(level: f32, z: f32, collision: f32) -> Status {
    let delta = level - z;
    if delta > collision {
        Status::UnderWater
    } else if delta > 0.0 {
        Status::InWater
    } else if delta > -1.0 {
        Status::WaterWalk
    } else {
        Status::AboveWater
    }
}

/// Actual source calls, not precomputed geometry outcomes. Errors retain call order.
pub trait TerrainOwner: Lookup {
    fn can_check(&mut self, p: [f32; 3]) -> Result<bool, Self::Error>;
    fn height_static(&mut self, p: [f32; 3]) -> Result<f32, Self::Error>;
    /// Must preserve VMap floor/type writes even when the query returns false.
    fn vmap_liquid(
        &mut self,
        p: [f32; 3],
        mask: u8,
        out: &mut LiquidData,
    ) -> Result<bool, Self::Error>;
    fn liquid_area(&mut self, p: [f32; 3]) -> Result<Option<Area>, Self::Error>;
    fn grid_liquid(
        &mut self,
        p: [f32; 3],
        mask: u8,
        collision: f32,
        out: &mut LiquidData,
    ) -> Result<Status, Self::Error>;
}
/// GridMap.cpp:1054-1127. Successful VMap queries suppress the grid else-if,
/// even when level<=floor or z<=floor-2 rejects that liquid at line 1065.
pub fn terrain_status<O: TerrainOwner>(
    owner: &mut O,
    p: [f32; 3],
    mask: u8,
    collision: f32,
    data: Option<&mut LiquidData>,
) -> Result<Status, O::Error> {
    let mut raw = LiquidData {
        entry: 0,
        type_flags: 0,
        level: INVALID_HEIGHT_VALUE,
        depth_level: owner.height_static(p)?,
    };
    if owner.vmap_liquid(p, mask, &mut raw)? {
        if raw.level > raw.depth_level && p[2] > raw.depth_level - 2.0 {
            if let Some(out) = data {
                let mut kind = owner.liquid_type(raw.entry)?.unwrap_or(0);
                if raw.entry != 0 && raw.entry < 21 {
                    if let Some(area) = owner.liquid_area(p)? {
                        if let Some((id, replacement)) = override_type(owner, area)? {
                            raw.entry = id;
                            kind = replacement;
                        }
                    }
                }
                // Source writes level/depth/entry before the possibly undefined shift.
                out.level = raw.level;
                out.depth_level = raw.depth_level;
                out.entry = raw.entry;
                out.type_flags = type_flag(kind)?;
            }
            return Ok(classify(raw.level, p[2], collision));
        }
    } else {
        let mut map = LiquidData::default();
        let status = owner.grid_liquid(p, mask, collision, &mut map)?;
        if status != Status::NoWater && map.level > raw.depth_level {
            if let Some(out) = data {
                *out = map;
            }
            return Ok(status);
        }
    }
    Ok(Status::NoWater)
}
/// GridMap.cpp:1130-1143 includes ABOVE_WATER as true.
pub fn is_in_water<O: TerrainOwner>(
    owner: &mut O,
    p: [f32; 3],
    data: Option<&mut LiquidData>,
) -> Result<bool, O::Error> {
    if !owner.can_check(p)? {
        return Ok(false);
    }
    let mut local = LiquidData::default();
    Ok(terrain_status(
        owner,
        p,
        ALL_LIQUIDS,
        DEFAULT_COLLISION_HEIGHT,
        Some(data.unwrap_or(&mut local)),
    )? != Status::NoWater)
}
/// GridMap.cpp:1147-1160 tests liquid depth, not the requested Z.
pub fn is_swimmable<O: TerrainOwner>(
    owner: &mut O,
    p: [f32; 3],
    radius: f32,
    data: Option<&mut LiquidData>,
) -> Result<bool, O::Error> {
    if !owner.can_check(p)? {
        return Ok(false);
    }
    let mut local = LiquidData::default();
    let out = data.unwrap_or(&mut local);
    Ok(
        terrain_status(owner, p, ALL_LIQUIDS, DEFAULT_COLLISION_HEIGHT, Some(out))?
            != Status::NoWater
            && out.level - out.depth_level > radius,
    )
}
/// GridMap.cpp:1169; GridMapDefines.h:68,70 define WATER|OCEAN as 0x0a.
/// GridMap.cpp:1163-1177 writes waterZ only for UNDER_WATER, water/ocean mask.
pub fn is_under_water<O: TerrainOwner>(
    owner: &mut O,
    p: [f32; 3],
    water_z: Option<&mut f32>,
) -> Result<bool, O::Error> {
    if !owner.can_check(p)? {
        return Ok(false);
    }
    let mut out = LiquidData::default();
    if terrain_status(owner, p, 10, DEFAULT_COLLISION_HEIGHT, Some(&mut out))? == Status::UnderWater
    {
        if let Some(z) = water_z {
            *z = out.level;
        }
        return Ok(true);
    }
    Ok(false)
}
/// GridMap.cpp:1194-1219 queries at groundZ, not the supplied Z.
pub fn water_or_ground<O: TerrainOwner>(
    owner: &mut O,
    mut p: [f32; 3],
    ground: f32,
    swim: bool,
    minimum: f32,
) -> Result<f32, O::Error> {
    if !owner.can_check(p)? {
        return Ok(INVALID_HEIGHT_VALUE);
    }
    p[2] = ground;
    let mut out = LiquidData::default();
    if terrain_status(
        owner,
        p,
        ALL_LIQUIDS,
        DEFAULT_COLLISION_HEIGHT,
        Some(&mut out),
    )? != Status::NoWater
    {
        if !swim {
            return Ok(out.level);
        }
        if out.level - ground > minimum {
            return Ok(out.level - minimum);
        }
    }
    Ok(ground)
}
/// GridMap.cpp:1311-1329 writes pGround before the second static-height call.
pub fn water_level<O: TerrainOwner>(
    owner: &mut O,
    mut p: [f32; 3],
    ground: Option<&mut f32>,
) -> Result<f32, O::Error> {
    if !owner.can_check(p)? {
        return Ok(INVALID_HEIGHT_VALUE);
    }
    p[2] = owner.height_static(p)?;
    if let Some(out) = ground {
        *out = p[2];
    }
    let mut out = LiquidData::default();
    if terrain_status(
        owner,
        p,
        ALL_LIQUIDS,
        DEFAULT_COLLISION_HEIGHT,
        Some(&mut out),
    )? == Status::NoWater
    {
        return Ok(INVALID_HEIGHT_VALUE);
    }
    Ok(out.level)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod oracle_tests;
