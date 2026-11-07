// SPDX-License-Identifier: GPL-3.0-or-later
//! Pinned TileAssembler::readMapSpawns container semantics, before geometry assembly.
use crate::PlacementRecords;
use std::{fmt, ops::Range};

pub const MAX_INDEX_RECORDS: usize = 1_048_576;
pub const MAP_SPAWN_INDEX_POLICY: &str = "cmangos-first-wins-tile-multimap-v1";

#[derive(Debug, PartialEq, Eq)]
pub enum MapSpawnIndexError {
    Cancelled,
    Limit,
    Allocation,
    Record,
}
impl fmt::Display for MapSpawnIndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Cancelled => "vmap.index_cancelled",
            Self::Limit => "vmap.index_limit",
            Self::Allocation => "vmap.index_allocation",
            Self::Record => "vmap.index_record",
        })
    }
}
impl std::error::Error for MapSpawnIndexError {}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct Entry {
    key: u32,
    id: u32,
    start: usize,
    end: usize,
}

/// Borrows a captured stream. Unique records are ascending by generated ID;
/// tile references retain duplicates and source encounter order within a tile.
/// No bounds, BIH primitives, or collision admission have been calculated.
pub struct MapSpawnIndex<'a> {
    records: &'a PlacementRecords,
    unique: Vec<Entry>,
    tiles: Vec<Entry>,
}
impl fmt::Debug for MapSpawnIndex<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MapSpawnIndex")
            .field("unique", &self.unique.len())
            .field("tile_references", &self.tiles.len())
            .finish_non_exhaustive()
    }
}
impl MapSpawnIndex<'_> {
    pub fn policy_id(&self) -> &'static str {
        MAP_SPAWN_INDEX_POLICY
    }
    pub fn unique_count(&self) -> usize {
        self.unique.len()
    }
    pub fn tile_reference_count(&self) -> usize {
        self.tiles.len()
    }
    /// Raw extractor record including map/tile header, without transformed bounds.
    pub fn unique_records(&self) -> impl Iterator<Item = (u32, &[u8])> {
        self.unique
            .iter()
            .map(|e| (e.id, &self.records.bytes()[e.start..e.end]))
    }
    /// Packed tile key and generated ID; every encounter is retained.
    pub fn tile_references(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.tiles.iter().map(|e| (e.key, e.id))
    }
    pub fn source_binding(&self) -> &[u8; 32] {
        self.records.preprocessing_binding()
    }
}
impl PlacementRecords {
    pub fn map_spawn_index(
        &self,
        cancelled: impl Fn() -> bool,
    ) -> Result<MapSpawnIndex<'_>, MapSpawnIndexError> {
        check(&cancelled)?;
        let count = self.record_count();
        if count > MAX_INDEX_RECORDS {
            return Err(MapSpawnIndexError::Limit);
        }
        let mut unique = allocate(count)?;
        let mut tiles = allocate(count)?;
        let mut scratch = allocate(count)?;
        let mut position = 0usize;
        let mut map = None;
        for _ in 0..count {
            check(&cancelled)?;
            let start = position;
            let bytes = self.bytes();
            let map_id = word(bytes, start)?;
            if map
                .replace(map_id)
                .is_some_and(|previous| previous != map_id)
            {
                return Err(MapSpawnIndexError::Record);
            }
            let x = word(bytes, start + 4)?;
            let y = word(bytes, start + 8)?;
            let flags = word(bytes, start + 12)?;
            let id = word(bytes, start + 18)?;
            let name_at = start
                .checked_add(if flags & 4 != 0 { 74 } else { 50 })
                .ok_or(MapSpawnIndexError::Record)?;
            position = name_at
                .checked_add(4)
                .and_then(|n| n.checked_add(word(bytes, name_at).ok()? as usize))
                .filter(|n| *n <= bytes.len())
                .ok_or(MapSpawnIndexError::Record)?;
            unique.push(Entry {
                key: id,
                id,
                start,
                end: position,
            });
            tiles.push(Entry {
                key: (x << 16) | y,
                id,
                start,
                end: position,
            });
        }
        if position != self.bytes().len() {
            return Err(MapSpawnIndexError::Record);
        }
        stable_radix(&mut unique, &mut scratch, &cancelled)?;
        // Stable ID sorting preserves std::map::insert first-wins semantics.
        let mut retained = 0;
        for read in 0..unique.len() {
            if read % 1024 == 0 {
                check(&cancelled)?;
            }
            if retained == 0 || unique[retained - 1].id != unique[read].id {
                unique[retained] = unique[read];
                retained += 1;
            }
        }
        unique.truncate(retained);
        stable_radix(&mut tiles, &mut scratch, &cancelled)?;
        check(&cancelled)?;
        Ok(MapSpawnIndex {
            records: self,
            unique,
            tiles,
        })
    }
}
fn allocate(count: usize) -> Result<Vec<Entry>, MapSpawnIndexError> {
    if count > MAX_INDEX_RECORDS {
        return Err(MapSpawnIndexError::Limit);
    }
    let mut v = Vec::new();
    v.try_reserve_exact(count)
        .map_err(|_| MapSpawnIndexError::Allocation)?;
    Ok(v)
}
fn word(bytes: &[u8], at: usize) -> Result<u32, MapSpawnIndexError> {
    let r: Range<usize> = at..at.checked_add(4).ok_or(MapSpawnIndexError::Record)?;
    Ok(u32::from_le_bytes(
        bytes
            .get(r)
            .ok_or(MapSpawnIndexError::Record)?
            .try_into()
            .map_err(|_| MapSpawnIndexError::Record)?,
    ))
}
fn check(cancelled: &dyn Fn() -> bool) -> Result<(), MapSpawnIndexError> {
    if cancelled() {
        Err(MapSpawnIndexError::Cancelled)
    } else {
        Ok(())
    }
}
// Four stable byte passes avoid platform container dependencies, recursion,
// hidden sorting allocations, and long uninterruptible comparison sorts.
fn stable_radix(
    entries: &mut Vec<Entry>,
    scratch: &mut Vec<Entry>,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), MapSpawnIndexError> {
    scratch.truncate(entries.len());
    while scratch.len() < entries.len() {
        check(cancelled)?;
        scratch.resize((scratch.len() + 1024).min(entries.len()), Entry::default());
    }
    for shift in [0, 8, 16, 24] {
        let mut offsets = [0usize; 256];
        for (i, e) in entries.iter().enumerate() {
            if i % 1024 == 0 {
                check(cancelled)?;
            }
            offsets[((e.key >> shift) & 255) as usize] += 1;
        }
        let mut sum = 0;
        for n in &mut offsets {
            let count = *n;
            *n = sum;
            sum += count;
        }
        for (i, e) in entries.iter().enumerate() {
            if i % 1024 == 0 {
                check(cancelled)?;
            }
            let at = &mut offsets[((e.key >> shift) & 255) as usize];
            scratch[*at] = *e;
            *at += 1;
        }
        std::mem::swap(entries, scratch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn allocation_budget_rejects_excess_before_reserving() {
        assert_eq!(
            allocate(MAX_INDEX_RECORDS + 1).unwrap_err(),
            MapSpawnIndexError::Limit
        );
        assert!(allocate(0).unwrap().is_empty());
    }
    #[test]
    fn radix_is_unsigned_stable_and_keeps_equal_key_payloads() {
        let keys = [u32::MAX, 65536, 0, 255, 256, 65536, 1, u32::MAX, 0];
        let mut entries: Vec<_> = keys
            .iter()
            .enumerate()
            .map(|(i, &key)| Entry {
                key,
                id: i as u32,
                ..Entry::default()
            })
            .collect();
        let mut scratch = allocate(entries.len()).unwrap();
        stable_radix(&mut entries, &mut scratch, &|| false).unwrap();
        assert_eq!(
            entries.iter().map(|e| e.id).collect::<Vec<_>>(),
            [2, 8, 6, 3, 4, 1, 5, 0, 7]
        );
    }
    #[test]
    fn radix_cancellation_is_checked_in_each_pass() {
        let values: Vec<_> = (0..4096)
            .rev()
            .map(|key| Entry {
                key,
                ..Entry::default()
            })
            .collect();
        let polls = std::cell::Cell::new(0);
        stable_radix(
            &mut values.clone(),
            &mut allocate(values.len()).unwrap(),
            &|| {
                polls.set(polls.get() + 1);
                false
            },
        )
        .unwrap();
        for cutoff in 0..polls.get() {
            let count = std::cell::Cell::new(0);
            assert_eq!(
                stable_radix(
                    &mut values.clone(),
                    &mut allocate(values.len()).unwrap(),
                    &|| {
                        let n = count.get();
                        count.set(n + 1);
                        n >= cutoff
                    }
                ),
                Err(MapSpawnIndexError::Cancelled)
            );
        }
    }
}
