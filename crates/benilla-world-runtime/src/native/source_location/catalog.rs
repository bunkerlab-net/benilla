// SPDX-License-Identifier: GPL-3.0-or-later
//! Complete bounded AreaTable/WMOAreaTable/Map bytes for the pinned final area lookup.
//! Decoding complete bytes is not raw-source certification or native admission.
use super::*;
use sha2::{Digest as _, Sha256};
use std::ops::RangeInclusive;

const MAX_ROWS: usize = 65_536;
const MAX_STRINGS: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceLocationArea {
    pub id: u32,
    pub map_id: u32,
    pub zone: u32,
    pub explore_flag: u32,
    pub liquid_override: u32,
}

/// All bytes remain retained, including columns not used by this bounded lookup.
/// No Deserialize or row-list constructor can claim a complete indexed DBC store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLocationCatalog {
    bytes: [Vec<u8>; 3],
    areas: BTreeMap<u32, SourceLocationArea>,
    maps: BTreeMap<u32, u32>,
    digest: crate::Digest,
}
impl SourceLocationCatalog {
    pub fn decode(area: &[u8], wmo: &[u8], map: &[u8]) -> Result<Self, NativeError> {
        let mut areas = BTreeMap::new();
        for row in records(area, 25, 11..=18)? {
            let id = indexed_id(row)?;
            // GetAreaEntryByAreaFlagAndMap's inclusive indexed loop must not wrap.
            if id == u32::MAX - 1 {
                return Err(NativeError::UnsupportedMechanic(
                    "area indexed loop overflow",
                ));
            }
            let entry = SourceLocationArea {
                id,
                map_id: word(row, 1)?,
                zone: word(row, 2)?,
                explore_flag: word(row, 3)?,
                liquid_override: word(row, 24)?,
            };
            if areas.insert(id, entry).is_some() {
                return Err(NativeError::InvalidProfile);
            }
        }
        let mut wmo_ids = std::collections::BTreeSet::new();
        for row in records(wmo, 20, 11..=18)? {
            if !wmo_ids.insert(indexed_id(row)?) {
                return Err(NativeError::InvalidProfile);
            }
        }
        let mut maps = BTreeMap::new();
        for row in records(map, 42, 4..=11)? {
            if maps.insert(indexed_id(row)?, word(row, 19)?).is_some() {
                return Err(NativeError::InvalidProfile);
            }
        }
        let mut hash = Sha256::new();
        hash.update(b"cmangos-8ec338a1-source-location-dbc-v1\0");
        for bytes in [area, wmo, map] {
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
        Ok(Self {
            bytes: [area.to_vec(), wmo.to_vec(), map.to_vec()],
            areas,
            maps,
            digest: hash.finalize().into(),
        })
    }
    pub fn source_bytes(&self) -> [&[u8]; 3] {
        self.bytes.each_ref().map(Vec::as_slice)
    }
    pub fn digest(&self) -> crate::Digest {
        self.digest
    }
    pub(in crate::native) fn resolve(&self, map: u32, flag: u16) -> Option<SourceLocationArea> {
        let mut last_wrong_map = None;
        for area in self.areas.values().copied() {
            if flag != 0 && area.explore_flag == u32::from(flag) {
                if area.map_id == map {
                    return Some(area);
                }
                last_wrong_map = Some(area);
            }
        }
        last_wrong_map.or_else(|| self.areas.get(self.maps.get(&map)?).copied())
    }
}
fn word(bytes: &[u8], field: usize) -> Result<u32, NativeError> {
    let at = field.checked_mul(4).ok_or(NativeError::TooLarge)?;
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or(NativeError::InvalidProfile)?
            .try_into()
            .unwrap(),
    ))
}
fn indexed_id(row: &[u8]) -> Result<u32, NativeError> {
    let id = word(row, 0)?;
    if id == u32::MAX {
        return Err(NativeError::UnsupportedMechanic(
            "location indexed store overflow",
        ));
    }
    Ok(id)
}
fn records(
    bytes: &[u8],
    fields: usize,
    locales: RangeInclusive<usize>,
) -> Result<std::slice::ChunksExact<'_, u8>, NativeError> {
    if bytes.get(..4) != Some(b"WDBC") {
        return Err(NativeError::InvalidProfile);
    }
    let count = word(bytes, 1)? as usize;
    let strings = word(bytes, 4)? as usize;
    if count > MAX_ROWS || strings > MAX_STRINGS {
        return Err(NativeError::TooLarge);
    }
    if word(bytes, 2)? as usize != fields || word(bytes, 3)? as usize != fields * 4 {
        return Err(NativeError::UnsupportedMechanic("location DBC layout"));
    }
    let strings_at = count
        .checked_mul(fields * 4)
        .and_then(|n| n.checked_add(20))
        .ok_or(NativeError::TooLarge)?;
    if strings == 0
        || strings_at.checked_add(strings) != Some(bytes.len())
        || bytes.get(strings_at) != Some(&0)
        || bytes.last() != Some(&0)
    {
        return Err(NativeError::InvalidProfile);
    }
    let rows = bytes[20..strings_at].chunks_exact(fields * 4);
    for row in rows.clone() {
        for field in locales.clone() {
            if word(row, field)? as usize >= strings {
                return Err(NativeError::InvalidProfile);
            }
        }
    }
    Ok(rows)
}

#[cfg(test)]
mod tests;
