// SPDX-License-Identifier: GPL-3.0-or-later
//! In-memory source map/tile framing; no publication or consumer admission.
use crate::{AssembledSpawnBounds, DoodadProjectionProfile, PreparedDiscovery};
use std::{fmt, ops::Range};
const MAX_BYTES: usize = 256 * 1024 * 1024;
const GLOBAL_TILE: u32 = (65 << 16) | 65;
#[derive(Debug)]
pub enum MapProductError {
    Cancelled,
    Limit,
    Allocation,
    Record,
    MixedWorldTile,
    Bounds(crate::SpawnBoundsError),
    Index(crate::MapSpawnIndexError),
    Bih(benilla_native_vmap::BihBuildError),
    Source(crate::SourceError),
}
impl fmt::Display for MapProductError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Cancelled => "vmap.map_cancelled",
            Self::Limit => "vmap.map_limit",
            Self::Allocation => "vmap.map_allocation",
            Self::Record => "vmap.map_record",
            Self::MixedWorldTile => "vmap.map_mixed_world_tile",
            Self::Bounds(_) => "vmap.map_bounds",
            Self::Index(_) => "vmap.map_index",
            Self::Bih(_) => "vmap.map_bih",
            Self::Source(_) => "vmap.map_source",
        })
    }
}
impl std::error::Error for MapProductError {}
struct Spawn {
    id: u32,
    flags: u32,
    bytes: Range<usize>,
    name: Range<usize>,
}
pub struct MapTileProduct {
    key: u32,
    bytes: Vec<u8>,
}
impl MapTileProduct {
    pub fn coordinates(&self) -> (u32, u32) {
        (self.key >> 16, self.key & 0xff)
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
pub struct MapProducts {
    binding: [u8; 32],
    map: u32,
    tree: Vec<u8>,
    tiles: Vec<MapTileProduct>,
    spawn_bytes: Vec<u8>,
    spawns: Vec<Spawn>,
}
impl fmt::Debug for MapProducts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MapProducts")
            .field("map", &self.map)
            .field("tiles", &self.tiles.len())
            .field("spawns", &self.spawns.len())
            .finish_non_exhaustive()
    }
}
impl MapProducts {
    /// Captured preprocessing identity only; not a live source lease.
    pub fn source_binding(&self) -> &[u8; 32] {
        &self.binding
    }
    pub fn map_id(&self) -> u32 {
        self.map
    }
    pub fn tree_bytes(&self) -> &[u8] {
        &self.tree
    }
    pub fn tiles(&self) -> &[MapTileProduct] {
        &self.tiles
    }
    /// Full source name bytes, one per ascending first-wins spawn. Duplicates and
    /// trailing NULs are retained; these are not validated filesystem paths.
    pub fn spawn_model_names(&self) -> impl Iterator<Item = &[u8]> {
        self.spawns
            .iter()
            .map(|s| &self.spawn_bytes[s.name.clone()])
    }
}
impl PreparedDiscovery {
    pub fn map_products(
        &self,
        profile: DoodadProjectionProfile,
        cancelled: impl Fn() -> bool,
    ) -> Result<Option<MapProducts>, MapProductError> {
        let bounds = self
            .assembled_spawn_bounds(profile, &cancelled)
            .map_err(MapProductError::Bounds)?;
        let products = bounds.map_products(&cancelled)?;
        self.source
            .verify_unchanged(&cancelled)
            .map_err(MapProductError::Source)?;
        check(&cancelled)?;
        Ok(products)
    }
}
impl AssembledSpawnBounds {
    /// Frame source output bytes from captured numeric preparation. Empty selected
    /// streams produce no map file, matching readMapSpawns/convertWorld2.
    pub fn map_products(
        &self,
        cancelled: impl Fn() -> bool,
    ) -> Result<Option<MapProducts>, MapProductError> {
        self.map_products_with_limits(cancelled, MAX_BYTES, 4097)
    }
    fn map_products_with_limits(
        &self,
        cancelled: impl Fn() -> bool,
        byte_limit: usize,
        tile_limit: usize,
    ) -> Result<Option<MapProducts>, MapProductError> {
        check(&cancelled)?;
        let records = self.records();
        let index = records
            .map_spawn_index(&cancelled)
            .map_err(MapProductError::Index)?;
        if index.unique_count() == 0 {
            return Ok(None);
        }
        if index.unique_count() != self.bounds().len() {
            return Err(MapProductError::Record);
        }
        let map = word(records.bytes(), 0)?;
        let mut budget = Budget {
            used: 0,
            limit: byte_limit,
        };
        let mut spawn_bytes = Vec::new();
        let mut spawns = Vec::new();
        spawns
            .try_reserve_exact(index.unique_count())
            .map_err(|_| MapProductError::Allocation)?;
        for ((id, record), bound) in index.unique_records().zip(self.bounds()) {
            check(&cancelled)?;
            if id != bound.generated_id || bound.flags & 4 == 0 {
                return Err(MapProductError::Record);
            }
            let old_flags = word(record, 12)?;
            let name_at = if old_flags & 4 != 0 { 74 } else { 50 };
            let length = word(record, name_at)? as usize;
            if length > 500 || name_at + 4 + length != record.len() {
                return Err(MapProductError::Record);
            }
            let start = spawn_bytes.len();
            budget.add(&mut spawn_bytes, &bound.flags.to_le_bytes())?;
            budget.add(&mut spawn_bytes, &record[16..50])?;
            for v in bound.bounds.into_iter().flatten() {
                budget.add(&mut spawn_bytes, &v.to_le_bytes())?;
            }
            budget.add(&mut spawn_bytes, &record[name_at..name_at + 4])?;
            let name_start = spawn_bytes.len();
            budget.add(&mut spawn_bytes, &record[name_at + 4..])?;
            spawns.push(Spawn {
                id,
                flags: bound.flags,
                bytes: start..spawn_bytes.len(),
                name: name_start..spawn_bytes.len(),
            });
        }
        let lookup = |id| {
            spawns
                .binary_search_by_key(&id, |s| s.id)
                .map_err(|_| MapProductError::Record)
        };
        let mut tree = Vec::new();
        budget.add(&mut tree, b"VMAP_7.0")?;
        let mut globals = 0usize;
        for (i, (key, _)) in index.tile_references().enumerate() {
            if i % 1024 == 0 {
                check(&cancelled)?;
            }
            if key == GLOBAL_TILE {
                globals += 1;
            }
        }
        budget.add(&mut tree, &[u8::from(globals == 0)])?;
        budget.add(&mut tree, b"NODE")?;
        let bih = self.map_bih(&cancelled).map_err(MapProductError::Bih)?;
        let bytes = bih.serialize(&cancelled).map_err(MapProductError::Bih)?;
        budget.add(&mut tree, &bytes)?;
        drop(bytes);
        drop(bih);
        budget.add(&mut tree, b"GOBJ")?;
        let mut tiles = Vec::new();
        let mut refs = index.tile_references().peekable();
        while let Some((key, id)) = refs.next() {
            check(&cancelled)?;
            let mut tile = Vec::new();
            budget.add(&mut tile, b"VMAP_7.0\0\0\0\0")?;
            let mut count = 0u32;
            let mut world = None;
            let mut append = |id: u32| -> Result<(), MapProductError> {
                check(&cancelled)?;
                let slot = lookup(id)?;
                let spawn = &spawns[slot];
                let is_world = spawn.flags & 2 != 0;
                if world == Some(true) && !is_world {
                    return Err(MapProductError::MixedWorldTile);
                }
                if world.is_none() {
                    world = Some(is_world);
                }
                if key == GLOBAL_TILE {
                    budget.add(&mut tree, &spawn_bytes[spawn.bytes.clone()])?;
                    budget.add(&mut tree, &(slot as u32).to_le_bytes())?;
                }
                if world == Some(false) {
                    budget.add(&mut tile, &spawn_bytes[spawn.bytes.clone()])?;
                    budget.add(&mut tile, &(slot as u32).to_le_bytes())?;
                }
                count = count.checked_add(1).ok_or(MapProductError::Limit)?;
                Ok(())
            };
            append(id)?;
            while refs.peek().is_some_and(|(next, _)| *next == key) {
                append(refs.next().unwrap().1)?;
            }
            if world == Some(false) {
                tile[8..12].copy_from_slice(&count.to_le_bytes());
                if tiles.len() == tile_limit {
                    return Err(MapProductError::Limit);
                }
                tiles
                    .try_reserve(1)
                    .map_err(|_| MapProductError::Allocation)?;
                tiles.push(MapTileProduct { key, bytes: tile });
            }
        }
        check(&cancelled)?;
        Ok(Some(MapProducts {
            binding: *records.preprocessing_binding(),
            map,
            tree,
            tiles,
            spawn_bytes,
            spawns,
        }))
    }
}
fn word(bytes: &[u8], at: usize) -> Result<u32, MapProductError> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or(MapProductError::Record)?
            .try_into()
            .unwrap(),
    ))
}
fn check(cancelled: &impl Fn() -> bool) -> Result<(), MapProductError> {
    if cancelled() {
        Err(MapProductError::Cancelled)
    } else {
        Ok(())
    }
}
struct Budget {
    used: usize,
    limit: usize,
}
impl Budget {
    fn add(&mut self, out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), MapProductError> {
        self.used = self
            .used
            .checked_add(bytes.len())
            .filter(|n| *n <= self.limit)
            .ok_or(MapProductError::Limit)?;
        let end = out
            .len()
            .checked_add(bytes.len())
            .filter(|n| *n <= self.limit)
            .ok_or(MapProductError::Limit)?;
        if end > out.capacity() {
            let capacity = end.max(out.capacity().saturating_mul(2)).min(self.limit);
            out.try_reserve_exact(capacity - out.len())
                .map_err(|_| MapProductError::Allocation)?;
            if out.capacity() > capacity {
                return Err(MapProductError::Allocation);
            }
        }
        out.extend_from_slice(bytes);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record(id: u32, key: u32, flags: u32) -> Vec<u8> {
        let mut r = Vec::new();
        for v in [0, key >> 16, key & 0xffff, flags] {
            r.extend_from_slice(&v.to_le_bytes());
        }
        r.extend_from_slice(&0u16.to_le_bytes());
        r.extend_from_slice(&id.to_le_bytes());
        for v in [0f32, 0., 0., 0., 0., 0., 1., -1., -1., -1., 1., 1., 1.] {
            r.extend_from_slice(&v.to_le_bytes());
        }
        r.extend_from_slice(&5u32.to_le_bytes());
        r.extend_from_slice(b"a.wmo");
        r
    }
    fn fixture(bytes: Vec<u8>, n: usize) -> AssembledSpawnBounds {
        AssembledSpawnBounds::fixture(crate::PlacementRecords::fixture(bytes, n))
    }
    #[test]
    fn original_complete_map_products_match() {
        fn unhex(s: &str) -> Vec<u8> {
            s.as_bytes()
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
                .collect()
        }
        let mut cases = 0;
        for line in include_str!("../tools/fixtures/map-products.tsv")
            .lines()
            .filter(|s| !s.starts_with('#'))
        {
            let f: Vec<_> = line.split('\t').collect();
            let raw = unhex(f[1]);
            let mut position = 0;
            let mut count = 0;
            while position < raw.len() {
                let name_at = position
                    + if word(&raw, position + 12).unwrap() & 4 != 0 {
                        74
                    } else {
                        50
                    };
                position = name_at + 4 + word(&raw, name_at).unwrap() as usize;
                count += 1;
            }
            let assembled = fixture(raw, count);
            let product = assembled.map_products(|| false).unwrap();
            let expected: std::collections::BTreeMap<String, String> =
                serde_json::from_str(f[3]).unwrap();
            let raw_models: std::collections::BTreeMap<String, String> =
                serde_json::from_str(f[2]).unwrap();
            let mut actual = std::collections::BTreeMap::new();
            if let Some(p) = product {
                actual.insert(format!("{:03}.vmtree", p.map_id()), p.tree_bytes().to_vec());
                for t in p.tiles() {
                    let (x, y) = t.coordinates();
                    actual.insert(
                        format!("{:03}_{x:02}_{y:02}.vmtile", p.map_id()),
                        t.bytes().to_vec(),
                    );
                }
                let names: std::collections::BTreeSet<_> = p.spawn_model_names().collect();
                for name in names {
                    let prefix = name.split(|b| *b == 0).next().unwrap();
                    let source_name = std::str::from_utf8(prefix).unwrap();
                    let output_name = if name.contains(&0) {
                        source_name.to_string()
                    } else {
                        format!("{source_name}.vmo")
                    };
                    let model = benilla_native_vmap::assemble_raw_model(
                        &unhex(&raw_models[source_name]),
                        || false,
                    )
                    .unwrap();
                    actual.insert(output_name, model);
                }
            }
            let expected: std::collections::BTreeMap<_, _> =
                expected.into_iter().map(|(k, v)| (k, unhex(&v))).collect();
            assert_eq!(actual, expected, "{}", f[0]);
            cases += 1;
        }
        assert_eq!(cases, 12);
    }
    #[test]
    fn empty_map_and_mixed_world_tile_policy() {
        assert!(fixture(vec![], 0).map_products(|| false).unwrap().is_none());
        let mut bad = record(1, 0, 6);
        bad.extend(record(2, 0, 4));
        assert!(matches!(
            fixture(bad, 2).map_products(|| false),
            Err(MapProductError::MixedWorldTile)
        ));
        let mut good = record(1, 0, 4);
        good.extend(record(2, 0, 6));
        let products = fixture(good, 2).map_products(|| false).unwrap().unwrap();
        assert_eq!(products.tiles.len(), 1);
        assert_eq!(word(&products.tiles[0].bytes, 8).unwrap(), 2);
        let mut omitted = record(1, 0, 6);
        omitted.extend(record(2, 0, 6));
        let omitted = fixture(omitted, 2).map_products(|| false).unwrap().unwrap();
        assert!(omitted.tiles().is_empty());
        assert_eq!(omitted.tree_bytes()[8], 1);
        let mut global = record(1, GLOBAL_TILE, 4);
        global.extend(record(1, GLOBAL_TILE, 4));
        let products = fixture(global, 2).map_products(|| false).unwrap().unwrap();
        assert_eq!(products.tree[8], 0);
        assert_eq!(products.tiles.len(), 1);
        assert_eq!(word(&products.tiles[0].bytes, 8).unwrap(), 2);
    }
    #[test]
    fn cancellation_sweep_and_byte_budget() {
        let mut records = record(2, 0, 4);
        records.extend(record(1, GLOBAL_TILE, 6));
        records.extend(record(2, 1, 4));
        let assembled = fixture(records, 3);
        assert!(matches!(
            assembled.map_products_with_limits(|| false, 32, 4097),
            Err(MapProductError::Limit)
        ));
        assert!(matches!(
            assembled.map_products_with_limits(|| false, MAX_BYTES, 0),
            Err(MapProductError::Limit)
        ));
        let n = std::cell::Cell::new(0usize);
        assembled
            .map_products(|| {
                n.set(n.get() + 1);
                false
            })
            .unwrap();
        for cutoff in 0..n.get() {
            let p = std::cell::Cell::new(0usize);
            let result = assembled.map_products(|| {
                let v = p.get();
                p.set(v + 1);
                v >= cutoff
            });
            assert!(matches!(
                result,
                Err(MapProductError::Cancelled)
                    | Err(MapProductError::Index(crate::MapSpawnIndexError::Cancelled))
                    | Err(MapProductError::Bih(
                        benilla_native_vmap::BihBuildError::Cancelled
                    ))
            ));
        }
        let mut budget = Budget {
            used: MAX_BYTES,
            limit: MAX_BYTES,
        };
        assert!(matches!(
            budget.add(&mut vec![], &[0]),
            Err(MapProductError::Limit)
        ));
        let mut budget = Budget {
            used: usize::MAX,
            limit: MAX_BYTES,
        };
        assert!(matches!(
            budget.add(&mut vec![], &[0]),
            Err(MapProductError::Limit)
        ));
    }
}
