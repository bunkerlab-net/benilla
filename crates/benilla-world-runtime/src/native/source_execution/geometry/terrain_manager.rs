// SPDX-License-Identifier: GPL-3.0-or-later
//! TerrainManager and cold map-only TerrainInfo execution, not geometry admission.
use super::identity_table::IdentityTable;
use super::*;
use benilla_native_terrain::grid_area::{AreaData, AreaReadError, FlagSearch};
use benilla_sim::source_rng::{RawGenerator, SourceRandom, TraceEvent};
use std::collections::BTreeMap;
use std::sync::Arc;

type TileKey = (u32, u16, u16);

/// Immutable acquired bytes or an observed source fopen absence. An index miss is unknown.
#[derive(Default)]
pub struct SourceTerrainFiles {
    files: BTreeMap<TileKey, Option<Arc<[u8]>>>,
}
impl SourceTerrainFiles {
    pub fn retain_present(
        &mut self,
        map: u32,
        x: u16,
        y: u16,
        path: &Path,
        expected: &Sha256Id,
    ) -> Result<()> {
        let file = ExecutionFile::open(path, 16 * 1024 * 1024)?;
        let mut bytes = vec![0; file.len() as usize];
        file.read_into(0, &mut bytes)?;
        file.check()?;
        require(
            Sha256Id::of(&bytes) == *expected,
            "execution.terrain.product_identity",
        )?;
        self.files.insert((map, x, y), Some(bytes.into()));
        Ok(())
    }
    /// Only a source absence observation may call this, never an acquisition miss.
    /// This component input does not certify source lineage or admit gameplay.
    pub fn retain_source_absence(&mut self, map: u32, x: u16, y: u16) {
        self.files.insert((map, x, y), None);
    }
    fn read(&self, key: TileKey) -> Result<Option<Arc<[u8]>>> {
        self.files
            .get(&key)
            .cloned()
            .ok_or_else(|| refuse("execution.terrain.map_product_unavailable"))
    }
    #[cfg(test)]
    pub(super) fn bytes(&mut self, map: u32, x: u16, y: u16, bytes: Vec<u8>) {
        self.files.insert((map, x, y), Some(bytes.into()));
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SourceAreaEntry {
    pub id: u32,
    pub map: u32,
    pub zone: u32,
    pub flag: u32,
}
#[derive(Default)]
pub struct SourceTerrainDbc {
    pub areas: BTreeMap<u32, SourceAreaEntry>,
    pub linked_zones: BTreeMap<u32, u32>,
}
impl SourceTerrainDbc {
    pub fn catalog(
        catalog: &mut super::super::production_compiler::VerifiedCatalog,
    ) -> Result<Self> {
        let areas = catalog
            .execution_terrain_dbc("AreaTable.dbc")?
            .into_iter()
            .map(|r| {
                (
                    r[0],
                    SourceAreaEntry {
                        id: r[0],
                        map: r[1],
                        zone: r[2],
                        flag: r[3],
                    },
                )
            })
            .collect();
        let linked_zones = catalog
            .execution_terrain_dbc("Map.dbc")?
            .into_iter()
            .map(|r| (r[0], r[1]))
            .collect();
        Ok(Self {
            areas,
            linked_zones,
        })
    }
    /// DBCStores.cpp:676-704 and GridMap.cpp:1406-1412. No parent lookup.
    pub fn zone(&self, map: u32, flag: u16) -> u32 {
        self.location(map, flag)[0]
    }
    pub fn location(&self, map: u32, flag: u16) -> [u32; 2] {
        let mut search = FlagSearch::new(flag, map);
        for area in self.areas.values().copied() {
            if let Some(area) = search.visit(area, area.flag, area.map) {
                return [if area.zone != 0 { area.zone } else { area.id }, area.id];
            }
        }
        let area = search.wrong_map().or_else(|| {
            self.linked_zones
                .get(&map)
                .and_then(|id| self.areas.get(id).copied())
        });
        area.map_or([0, 0], |a| [if a.zone != 0 { a.zone } else { a.id }, a.id])
    }
}

/// VMapManager2.cpp:36-38,76-83,258-269. Construction installs no tree.
pub struct SourceVMapManager {
    created: bool,
    thread_safe: bool,
    los_enabled: bool,
    height_enabled: bool,
    trees: BTreeMap<u32, Option<u64>>,
    pub configuration: Option<(Sha256Id, [usize; 2])>,
}
impl Default for SourceVMapManager {
    fn default() -> Self {
        Self {
            created: false,
            thread_safe: true,
            los_enabled: true,
            height_enabled: true,
            trees: BTreeMap::new(),
            configuration: None,
        }
    }
}
impl SourceVMapManager {
    pub fn created(&self) -> bool {
        self.created
    }
    pub fn loaded_trees(&self) -> usize {
        self.trees.values().filter(|tree| tree.is_some()).count()
    }
    pub fn thread_safe(&self) -> bool {
        self.thread_safe
    }
    pub fn map_slots(&self) -> usize {
        self.trees.len()
    }
    pub fn settings(&self) -> [bool; 2] {
        [self.los_enabled, self.height_enabled]
    }
    fn has_tree(&self, map: u32) -> bool {
        self.trees.get(&map).is_some_and(Option::is_some)
    }
    /// World.cpp:817-824,912-919; seal16-full-01/mangosd.conf:265-266.
    pub fn initialize_recorded_world(
        &mut self,
        maps: impl IntoIterator<Item = u32>,
        bytes: &[u8],
        approved: &Sha256Id,
    ) -> Result<()> {
        require(
            Sha256Id::of(bytes) == *approved,
            "execution.terrain.vmap_config_identity",
        )?;
        let (los_line, los) = super::spawn_loader_config::assignment(bytes, "vmap.enableLOS")?;
        let (height_line, height) =
            super::spawn_loader_config::assignment(bytes, "vmap.enableHeight")?;
        let enabled = |value: &str| {
            value == "1" || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("yes")
        };
        self.created = true;
        self.los_enabled = enabled(&los);
        self.height_enabled = enabled(&height);
        self.configuration = Some((approved.clone(), [los_line, height_line]));
        for map in maps {
            self.trees.entry(map).or_insert(None);
        }
        self.thread_safe = false;
        Ok(())
    }
    fn area_info(&self, map: u32) -> Result<()> {
        require(
            !self.has_tree(map),
            "execution.terrain.live_vmap_area_owner",
        )
    }
    #[cfg(test)]
    pub(super) fn install_tree(&mut self, map: u32, generation: u64) {
        self.trees.insert(map, Some(generation));
    }
}

/// GridMap.cpp:43-69. Default storage is real even when fopen fails.
#[derive(Debug)]
pub struct SourceGridMap {
    pub bytes: Option<Arc<[u8]>>,
    pub load_success: bool,
    areas: AreaData,
    pub flat_height: f32,
    pub holes: [u16; 256],
    pub liquid_entry: u16,
    pub liquid_flags: u8,
    pub liquid_offset: [u8; 2],
    pub liquid_size: [u8; 2],
    pub liquid_level: f32,
}
impl Default for SourceGridMap {
    fn default() -> Self {
        Self {
            bytes: None,
            load_success: true,
            areas: AreaData::default(),
            flat_height: -200_000.0,
            holes: [0; 256],
            liquid_entry: 0,
            liquid_flags: 0,
            liquid_offset: [0; 2],
            liquid_size: [0; 2],
            liquid_level: -200_000.0,
        }
    }
}
fn word(bytes: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or_else(|| refuse("execution.terrain.map_header_read_undefined"))?
            .try_into()
            .unwrap(),
    ))
}
impl SourceGridMap {
    fn load(bytes: Option<Arc<[u8]>>) -> Result<Self> {
        let mut grid = Self {
            bytes,
            ..Self::default()
        };
        let Some(bytes) = &grid.bytes else {
            return Ok(grid);
        };
        require(
            bytes.len() >= 40,
            "execution.terrain.map_header_read_undefined",
        )?;
        if &bytes[..8] != b"MAPSz1.4" {
            grid.load_success = false;
            return Ok(grid);
        }
        let area = word(bytes, 8)? as usize;
        if area != 0 {
            grid.areas = match AreaData::decode(bytes.get(area..).unwrap_or_default()) {
                Ok(area) => area,
                Err(AreaReadError::Magic) => {
                    grid.load_success = false;
                    return Ok(grid);
                }
                Err(AreaReadError::Truncated) => {
                    return Err(refuse("execution.terrain.area_partial_read_owner"))
                }
            };
        }
        // Retain partial-loader state and source order, GridMap.cpp:104-126.
        let holes = word(bytes, 32)? as usize;
        if holes != 0 {
            let raw = bytes
                .get(holes..holes + 512)
                .ok_or_else(|| refuse("execution.terrain.holes_partial_read_owner"))?;
            for (hole, pair) in grid.holes.iter_mut().zip(raw.as_chunks::<2>().0) {
                *hole = u16::from_le_bytes(*pair);
            }
        }
        let height = word(bytes, 16)? as usize;
        if height != 0 {
            let header = bytes
                .get(height..height + 16)
                .ok_or_else(|| refuse("execution.terrain.height_header_read_undefined"))?;
            if &header[..4] != b"MHGT" {
                grid.load_success = false;
                return Ok(grid);
            }
            grid.flat_height = f32::from_bits(word(header, 8)?);
            // Nonflat arrays remain retained in bytes, never interpreted as flat height.
        }
        let liquid = word(bytes, 24)? as usize;
        if liquid != 0 {
            let header = bytes
                .get(liquid..liquid + 16)
                .ok_or_else(|| refuse("execution.terrain.liquid_header_read_undefined"))?;
            if &header[..4] != b"MLIQ" {
                grid.load_success = false;
                return Ok(grid);
            }
            grid.liquid_entry = u16::from_le_bytes([header[6], header[7]]);
            grid.liquid_flags = header[5];
            grid.liquid_offset = [header[8], header[9]];
            grid.liquid_size = [header[10], header[11]];
            grid.liquid_level = f32::from_bits(word(header, 12)?);
        }
        Ok(grid)
    }
    pub fn area(&self, x: f32, y: f32) -> Result<u16> {
        self.areas
            .get(x, y)
            .ok_or_else(|| refuse("execution.terrain.area_coordinate_conversion_undefined"))
    }
}

pub struct SourceTerrainInfo {
    pub residency: SourceTerrainResidency,
    // GridMap.h:122-140 uses atomic_long, with defined LP64 atomic wrapping.
    references: i64,
    grids: BTreeMap<(u16, u16), SourceGridMap>,
}
impl SourceTerrainInfo {
    pub fn references(&self) -> i64 {
        self.references
    }
    pub fn grid(&self, x: u16, y: u16) -> Option<&SourceGridMap> {
        self.grids.get(&(x, y))
    }
    fn query(
        &mut self,
        map: u32,
        pose: [f32; 3],
        map_only: bool,
        files: &SourceTerrainFiles,
    ) -> Result<TerrainTileIdentity> {
        let grids = &mut self.grids;
        self.residency
            .query_terrain_at([pose[0].to_bits(), pose[1].to_bits()], map_only, |c| {
                if let std::collections::btree_map::Entry::Vacant(entry) = grids.entry((c.x, c.y)) {
                    entry.insert(SourceGridMap::load(files.read((map, c.x, c.y))?)?);
                }
                // Actual missing owner, after map allocation, before VMap/MMap loading.
                require(map_only, "execution.terrain.vmap_tile_load_owner")?;
                Ok(TerrainQueryLoad {
                    present: true,
                    fully_loaded: false,
                })
            })?
            .ok_or_else(|| refuse("execution.terrain.grid_allocation_owner"))
    }
}

#[derive(Default)]
pub struct SourceTerrainManager {
    terrains: IdentityTable<SourceTerrainInfo>,
    next_generation: u64,
    pub vmap: SourceVMapManager,
    mmap_created: bool,
    pub files: SourceTerrainFiles,
    pub dbc: SourceTerrainDbc,
}
impl SourceTerrainManager {
    pub fn new(files: SourceTerrainFiles, dbc: SourceTerrainDbc) -> Self {
        Self {
            files,
            dbc,
            ..Self::default()
        }
    }
    pub fn len(&self) -> usize {
        self.terrains.len()
    }
    pub fn bucket_count(&self) -> usize {
        self.terrains.bucket_count()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn terrain(&self, map: u32) -> Option<&SourceTerrainInfo> {
        self.terrains.get(u64::from(map))
    }
    /// GridMap.cpp:1348-1359. No sorted-map eager construction.
    pub fn load_terrain<E: RawGenerator, S: FnMut(TraceEvent)>(
        &mut self,
        map: u32,
        random: &mut SourceRandom<'_, E, S>,
    ) -> Result<&mut SourceTerrainInfo> {
        let key = u64::from(map);
        if self.terrains.get(key).is_none() {
            let generation = self
                .next_generation
                .checked_add(1)
                .ok_or_else(|| refuse("execution.terrain.generation_overflow"))?;
            let residency = SourceTerrainResidency::new(map, generation, random)?;
            // The singleton is obtained only after the successful draw and timer assignments.
            self.vmap.created = true;
            self.terrains.insert(
                key,
                SourceTerrainInfo {
                    residency,
                    references: 0,
                    grids: BTreeMap::new(),
                },
                65_536,
            )?;
            self.next_generation = generation;
        }
        Ok(self.terrains.get_mut(key).unwrap())
    }
    pub fn original_zone<E: RawGenerator, S: FnMut(TraceEvent)>(
        &mut self,
        map: u32,
        pose: [f32; 3],
        random: &mut SourceRandom<'_, E, S>,
    ) -> Result<u32> {
        self.load_terrain(map, random)?;
        Ok(self.zone_and_area(map, pose)?[0])
    }
    pub fn zone_and_area(&mut self, map: u32, pose: [f32; 3]) -> Result<[u32; 2]> {
        require(self.terrain(map).is_some(), "execution.terrain.info_owner")?;
        self.vmap.area_info(map)?;
        let tile = self.query_grid(map, pose, true)?;
        let grid = self.terrain(map).unwrap().grid(tile.x, tile.y).unwrap();
        Ok(self.dbc.location(map, grid.area(pose[0], pose[1])?))
    }
    pub fn query_grid(
        &mut self,
        map: u32,
        pose: [f32; 3],
        map_only: bool,
    ) -> Result<TerrainTileIdentity> {
        self.terrains
            .get_mut(u64::from(map))
            .ok_or_else(|| refuse("execution.terrain.info_owner"))?
            .query(map, pose, map_only, &self.files)
    }
    pub fn add_ref(&mut self, map: u32) -> Result<()> {
        let info = self
            .terrains
            .get_mut(u64::from(map))
            .ok_or_else(|| refuse("execution.terrain.info_owner"))?;
        info.references = info.references.wrapping_add(1);
        Ok(())
    }
    pub fn release(&mut self, map: u32) -> Result<bool> {
        let info = self
            .terrains
            .get_mut(u64::from(map))
            .ok_or_else(|| refuse("execution.terrain.info_owner"))?;
        info.references = info.references.wrapping_sub(1);
        Ok(info.references < 1)
    }
    fn destroy(&mut self, map: u32) -> Result<()> {
        require(
            !self.vmap.has_tree(map),
            "execution.terrain.vmap_map_unload_owner",
        )?;
        self.terrains.erase(u64::from(map));
        // GridMap.cpp:716-724 obtains the still-empty MMap singleton after VMap unload.
        self.mmap_created = true;
        Ok(())
    }
    pub fn unload_terrain(&mut self, map: u32, grid_unload: bool) -> Result<()> {
        if grid_unload && self.terrain(map).is_some_and(|info| info.references <= 0) {
            self.destroy(map)?;
        }
        Ok(())
    }
    pub fn unload_all(&mut self) -> Result<()> {
        while let Some(map) = self.terrains.first_key() {
            self.destroy(map as u32)?;
        }
        Ok(())
    }
    /// World.cpp:1701, GridMap.cpp:1382-1387. Preserve source hash iteration.
    pub fn update(&mut self, diff: u32) -> Result<()> {
        let mut key = self.terrains.first_key();
        while let Some(map) = key {
            key = self.terrains.next_key(map);
            let info = self.terrains.get_mut(map).unwrap();
            let mut cleanup = ColdCleanup {
                grids: &mut info.grids,
                vmap: &self.vmap,
                mmap_created: &mut self.mmap_created,
            };
            info.residency.cleanup(diff, &mut cleanup)?;
        }
        Ok(())
    }
}
struct ColdCleanup<'a> {
    grids: &'a mut BTreeMap<(u16, u16), SourceGridMap>,
    vmap: &'a SourceVMapManager,
    mmap_created: &'a mut bool,
}
impl GridProductOwner for ColdCleanup<'_> {
    fn load(&mut self, _: MapIdentity, _: ProductCoordinates) -> Result<bool> {
        Err(refuse("execution.terrain.full_grid_load_owner"))
    }
}
impl TerrainResidencyOwner for ColdCleanup<'_> {
    fn terrain_cleanup(
        &mut self,
        tile: TerrainTileIdentity,
        phase: TerrainCleanupPhase,
    ) -> Result<()> {
        match phase {
            TerrainCleanupPhase::UnloadData => {
                self.grids
                    .get_mut(&(tile.x, tile.y))
                    .ok_or_else(|| refuse("execution.terrain.grid_allocation_owner"))?
                    .areas
                    .unload_cells();
            }
            TerrainCleanupPhase::DeleteGridMap => {
                self.grids.remove(&(tile.x, tile.y));
            }
            TerrainCleanupPhase::UnloadVMap => require(
                !self.vmap.has_tree(tile.map),
                "execution.terrain.vmap_tile_unload_owner",
            )?,
            TerrainCleanupPhase::UnloadMMap => *self.mmap_created = true,
        }
        Ok(())
    }
}
