// SPDX-License-Identifier: GPL-3.0-or-later
//! Complete bounded WDBC inputs for pinned TerrainInfo::GetAreaFlag and final lookup.
//! DBCfmt.h: niiixxxxxiissssssssx (20), nxixssssssssxxxxxxxixxxxxxxxxxxxxxxxxxixxx (42).
use super::super::u32_at;
use super::{Error, Hash};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, ops::RangeInclusive};

const MAX_ROWS: usize = 65_536;
const MAX_STRINGS: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug)]
pub(super) struct WmoAreaEntry {
    pub triple: [i32; 3],
    pub area_id: u32,
}

#[derive(Debug)]
pub(super) struct WmoAreaTable {
    entries: BTreeMap<u32, WmoAreaEntry>,
    digest: Hash,
}
impl WmoAreaTable {
    pub(super) fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let rows = records(bytes, 20, 11..=18)?;
        let mut entries = BTreeMap::new();
        for row in rows {
            let id = indexed_id(row)?;
            let entry = WmoAreaEntry {
                triple: [
                    u32_at(row, 4)? as i32,
                    u32_at(row, 8)? as i32,
                    u32_at(row, 12)? as i32,
                ],
                area_id: u32_at(row, 10 * 4)?,
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

    pub(super) fn digest(&self) -> Hash {
        self.digest
    }

    pub(super) fn ordered_entries(&self) -> impl Iterator<Item = WmoAreaEntry> + '_ {
        self.entries.values().copied()
    }
}

#[derive(Debug)]
pub(super) struct MapTable {
    entries: BTreeMap<u32, u32>,
    digest: Hash,
}
impl MapTable {
    pub(super) fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let rows = records(bytes, 42, 4..=11)?;
        let mut entries = BTreeMap::new();
        for row in rows {
            let id = indexed_id(row)?;
            if entries.insert(id, u32_at(row, 19 * 4)?).is_some() {
                return Err(Error::Duplicate);
            }
        }
        Ok(Self {
            entries,
            digest: Sha256::digest(bytes).into(),
        })
    }

    pub(super) fn digest(&self) -> Hash {
        self.digest
    }

    pub(super) fn linked_zone(&self, id: u32) -> Option<u32> {
        self.entries.get(&id).copied()
    }
}

fn indexed_id(row: &[u8]) -> Result<u32, Error> {
    let id = u32_at(row, 0)?;
    // AutoProduceData increments the maximum u32 ID to size its indexed store.
    if id == u32::MAX {
        return Err(Error::Unsupported);
    }
    Ok(id)
}

fn records(
    bytes: &[u8],
    fields: usize,
    locale_fields: RangeInclusive<usize>,
) -> Result<std::slice::ChunksExact<'_, u8>, Error> {
    if bytes.get(..4) != Some(b"WDBC") {
        return Err(Error::Format);
    }
    let rows = u32_at(bytes, 4)? as usize;
    let actual_fields = u32_at(bytes, 8)? as usize;
    let stride = u32_at(bytes, 12)? as usize;
    let strings = u32_at(bytes, 16)? as usize;
    if rows > MAX_ROWS || strings > MAX_STRINGS {
        return Err(Error::Budget);
    }
    if actual_fields != fields || stride != fields * 4 {
        return Err(Error::Unsupported);
    }
    let strings_at = rows
        .checked_mul(stride)
        .and_then(|size| size.checked_add(20))
        .ok_or(Error::Framing)?;
    if strings == 0
        || strings_at.checked_add(strings) != Some(bytes.len())
        || bytes.get(strings_at) != Some(&0)
        || bytes.last() != Some(&0)
    {
        return Err(Error::Framing);
    }
    let records = bytes[20..strings_at].chunks_exact(stride);
    for record in records.clone() {
        // Only the eight actual 's' fields are strings in the pinned DBCfmt profile.
        // A final NUL terminates every in-bounds offset without unbounded scans.
        for field in locale_fields.clone() {
            if u32_at(record, field * 4)? as usize >= strings {
                return Err(Error::Framing);
            }
        }
    }
    Ok(records)
}
