// SPDX-License-Identifier: GPL-3.0-or-later
//! MoveMap.cpp:102-278,356-509. Frozen products remain diagnostic, not F1 admission.
use super::*;
use benilla_native_nav::{
    live::{LiveError, MapMesh, QueryHandle},
    TileInput,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MMapForcing {
    Default,
    Forced,
    Ignored,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MMapLoad {
    Loaded,
    MapAbsent,
    TileAbsent,
    BadMagic,
    BadVersion,
    ReadFailed,
    AlreadyLoaded,
    AddFailed(u32),
}
impl MMapLoad {
    pub fn loaded(self) -> bool {
        self == Self::Loaded
    }
}

enum Product {
    Present {
        file: ExecutionFile,
        expected: Sha256Id,
    },
    SourceAbsent,
    #[cfg(test)]
    Authored(Vec<u8>),
}
impl Product {
    fn read(&self) -> Result<Option<Vec<u8>>> {
        match self {
            Self::SourceAbsent => Ok(None),
            Self::Present { file, expected } => {
                let mut bytes = vec![0; file.len() as usize];
                for (i, chunk) in bytes.chunks_mut(1024 * 1024).enumerate() {
                    file.read_into((i * 1024 * 1024) as u64, chunk)?;
                }
                file.check()?;
                require(
                    Sha256Id::of(&bytes) == *expected,
                    "execution.mmap.product_identity",
                )?;
                Ok(Some(bytes))
            }
            #[cfg(test)]
            Self::Authored(bytes) => Ok(Some(bytes.clone())),
        }
    }
}
#[derive(Default)]
pub struct SourceMMapFiles {
    maps: BTreeMap<u32, Product>,
    tiles: BTreeMap<(u32, u16, u16), Product>,
}
impl SourceMMapFiles {
    /// Retain the actual descriptor. A missing expected product is not source fopen failure.
    pub fn retain_map(&mut self, map: u32, path: &Path, expected: Sha256Id) -> Result<()> {
        require(
            !self.maps.contains_key(&map),
            "execution.mmap.product_replacement",
        )?;
        let file = ExecutionFile::open(path, 28)
            .map_err(|_| refuse("execution.mmap.expected_map_product_unavailable"))?;
        self.maps.insert(map, Product::Present { file, expected });
        Ok(())
    }
    pub fn retain_tile(
        &mut self,
        map: u32,
        x: u16,
        y: u16,
        path: &Path,
        expected: Sha256Id,
    ) -> Result<()> {
        require(
            !self.tiles.contains_key(&(map, x, y)),
            "execution.mmap.product_replacement",
        )?;
        let file = ExecutionFile::open(path, 16 * 1024 * 1024)
            .map_err(|_| refuse("execution.mmap.expected_tile_product_unavailable"))?;
        self.tiles
            .insert((map, x, y), Product::Present { file, expected });
        Ok(())
    }
    /// Only an observed source absence may be supplied, never an acquisition index miss.
    pub fn source_map_absent(&mut self, map: u32) -> Result<()> {
        require(
            !self.maps.contains_key(&map),
            "execution.mmap.product_replacement",
        )?;
        self.maps.insert(map, Product::SourceAbsent);
        Ok(())
    }
    pub fn source_tile_absent(&mut self, map: u32, x: u16, y: u16) -> Result<()> {
        require(
            !self.tiles.contains_key(&(map, x, y)),
            "execution.mmap.product_replacement",
        )?;
        self.tiles.insert((map, x, y), Product::SourceAbsent);
        Ok(())
    }
    #[cfg(test)]
    fn map_bytes(&mut self, map: u32, bytes: Vec<u8>) {
        self.maps.insert(map, Product::Authored(bytes));
    }
    #[cfg(test)]
    fn tile_bytes(&mut self, map: u32, x: u16, y: u16, bytes: Vec<u8>) {
        self.tiles.insert((map, x, y), Product::Authored(bytes));
    }
}

pub struct SourceMMapManager {
    created: bool,
    enabled: bool,
    preload: bool,
    ignored: BTreeSet<u32>,
    pub configuration: Option<(Sha256Id, [usize; 3])>,
    maps: BTreeMap<u32, MapMesh>,
    pub files: SourceMMapFiles,
}
impl Default for SourceMMapManager {
    fn default() -> Self {
        Self {
            created: false,
            enabled: true,
            preload: false,
            ignored: BTreeSet::new(),
            configuration: None,
            maps: BTreeMap::new(),
            files: SourceMMapFiles::default(),
        }
    }
}
impl SourceMMapManager {
    pub fn created(&self) -> bool {
        self.created
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    /// World.cpp:829-836; seal16-full-01/mangosd.conf:269-271.
    pub fn initialize_recorded_world(&mut self, bytes: &[u8], approved: &Sha256Id) -> Result<()> {
        require(
            Sha256Id::of(bytes) == *approved,
            "execution.mmap.config_identity",
        )?;
        let (enabled_line, enabled) = spawn_loader_config::assignment(bytes, "mmap.enabled")?;
        let (ignored_line, ignored) = spawn_loader_config::assignment(bytes, "mmap.ignoreMapIds")?;
        let (preload_line, preload) = spawn_loader_config::assignment(bytes, "mmap.preload")?;
        let boolean =
            |s: &str| s == "1" || s.eq_ignore_ascii_case("true") || s.eq_ignore_ascii_case("yes");
        let mut ids = BTreeSet::new();
        // strtok skips empty fields; atoi accepts whitespace, sign and a numeric prefix.
        for token in ignored.split(',').filter(|v| !v.is_empty()) {
            let token = token.trim_start_matches(|c: char| c.is_ascii_whitespace());
            let (negative, token) = if let Some(s) = token.strip_prefix('-') {
                (true, s)
            } else {
                (false, token.strip_prefix('+').unwrap_or(token))
            };
            let mut n = 0i64;
            for byte in token.bytes().take_while(u8::is_ascii_digit) {
                n = n
                    .checked_mul(10)
                    .and_then(|v| v.checked_add(i64::from(byte - b'0')))
                    .ok_or_else(|| refuse("execution.mmap.ignore_atoi_overflow"))?;
            }
            let n = if negative { -n } else { n };
            let n = i32::try_from(n).map_err(|_| refuse("execution.mmap.ignore_atoi_overflow"))?;
            ids.insert(n as u32);
        }
        self.created = true;
        self.enabled = boolean(&enabled);
        self.preload = boolean(&preload);
        self.ignored.extend(ids);
        self.configuration = Some((approved.clone(), [enabled_line, ignored_line, preload_line]));
        Ok(())
    }
    /// MoveMap.cpp:64-83. Forced does not override the global enabled switch.
    pub fn pathfinding_enabled(&self, map: u32, forcing: MMapForcing) -> Result<bool> {
        require(self.configuration.is_some(), "execution.mmap.config_owner")?;
        Ok(self.enabled
            && match forcing {
                MMapForcing::Forced => true,
                MMapForcing::Ignored => false,
                MMapForcing::Default => !self.ignored.contains(&map),
            })
    }
    /// Map.cpp:202-212 ignores a source load-false result, not a native refusal.
    pub fn initialize_map(&mut self, map: MapIdentity) -> Result<()> {
        require(self.configuration.is_some(), "execution.mmap.config_owner")?;
        self.created = true;
        if self.enabled {
            self.load_map_instance(map.map, map.instance)?;
            if self.preload {
                require(
                    self.maps.contains_key(&map.map),
                    "execution.mmap.preload_without_mesh",
                )?;
                // MoveMap.cpp:177. Manifest order cannot replace source directory order.
                return Err(refuse("execution.mmap.preload_directory_order_owner"));
            }
        }
        Ok(())
    }
    pub fn load_map_instance(&mut self, map: u32, instance: u32) -> Result<MMapLoad> {
        self.created = true;
        if !self.maps.contains_key(&map) {
            let bytes = self
                .files
                .maps
                .get(&map)
                .ok_or_else(|| refuse("execution.mmap.map_product_unavailable"))?
                .read()?;
            let Some(bytes) = bytes else {
                return Ok(MMapLoad::MapAbsent);
            };
            require(
                bytes.len() >= 28,
                "execution.mmap.undefined_short_params_read",
            )?;
            let mesh = MapMesh::new(&bytes[..28]).map_err(mmap_error)?;
            self.maps.insert(map, mesh);
        }
        self.maps
            .get_mut(&map)
            .unwrap()
            .load_instance(instance)
            .map_err(mmap_error)?;
        Ok(MMapLoad::Loaded)
    }
    pub fn mesh(&self, map: u32) -> Option<&MapMesh> {
        self.maps.get(&map)
    }
    pub fn mesh_count(&self) -> usize {
        self.maps.len()
    }
    pub fn tile_count(&self) -> usize {
        self.maps.values().map(|mesh| mesh.tiles().count()).sum()
    }
    pub fn query(&self, map: u32, instance: u32) -> Option<QueryHandle> {
        self.maps.get(&map)?.query(instance)
    }
    pub fn query_capacity(&self, map: u32, query: &QueryHandle) -> Result<u32> {
        self.maps
            .get(&map)
            .ok_or_else(|| refuse("execution.mmap.stale_query"))?
            .query_capacity(query)
            .map_err(mmap_error)
    }
    pub fn is_tile_loaded(&mut self, map: u32, x: u16, y: u16) -> bool {
        self.created = true;
        self.maps.get(&map).is_some_and(|mesh| {
            u8::try_from(x)
                .ok()
                .zip(u8::try_from(y).ok())
                .is_some_and(|(x, y)| mesh.tile_reference([x, y]).is_some())
        })
    }
    pub fn load_tile(&mut self, map: u32, x: u16, y: u16) -> Result<MMapLoad> {
        self.created = true;
        let mesh = self
            .maps
            .get_mut(&map)
            .ok_or_else(|| refuse("execution.mmap.load_map_without_mesh"))?;
        require(x < 64 && y < 64, "execution.mmap.tile_coordinates")?;
        let grid = [x as u8, y as u8];
        if mesh.tile_reference(grid).is_some() {
            return Ok(MMapLoad::AlreadyLoaded);
        }
        let bytes = self
            .files
            .tiles
            .get(&(map, x, y))
            .ok_or_else(|| refuse("execution.mmap.tile_product_unavailable"))?
            .read()?;
        let Some(bytes) = bytes else {
            return Ok(MMapLoad::TileAbsent);
        };
        // MoveMapSharedDefines.h:36-37 initializes the header before MoveMap.cpp:233.
        // MmapTileHeader's constructor survives a zero-byte read. A partial
        // element is indeterminate, C11 7.21.8.1p2; never imitate glibc's copy.
        require(
            bytes.is_empty() || bytes.len() >= 20,
            "execution.mmap.undefined_partial_header_read",
        )?;
        let defaults = [0x4d4d4150u32, 7, 8, 0, 0];
        let word = |i| {
            if bytes.is_empty() {
                defaults[i / 4]
            } else {
                u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap())
            }
        };
        if word(0) != 0x4d4d4150 {
            return Ok(MMapLoad::BadMagic);
        }
        if word(8) != 8 {
            return Ok(MMapLoad::BadVersion);
        }
        let size = word(12) as usize;
        if bytes.len() < 20 || size == 0 || size > bytes.len() - 20 {
            return Ok(MMapLoad::ReadFailed);
        }
        match mesh.add_tile(TileInput {
            grid,
            bytes: &bytes[..20 + size],
        }) {
            Ok(_) => Ok(MMapLoad::Loaded),
            Err(LiveError::Native(status)) => Ok(MMapLoad::AddFailed(status)),
            Err(error) => Err(mmap_error(error)),
        }
    }
    pub fn unload_tile(&mut self, map: u32, x: u16, y: u16) -> Result<bool> {
        self.created = true;
        let Some(mesh) = self.maps.get_mut(&map) else {
            return Ok(false);
        };
        let grid = [
            u8::try_from(x).map_err(|_| refuse("execution.mmap.tile_coordinates"))?,
            u8::try_from(y).map_err(|_| refuse("execution.mmap.tile_coordinates"))?,
        ];
        mesh.remove_tile(grid).map_err(mmap_error)
    }
    pub fn unload_map(&mut self, map: u32) -> Result<bool> {
        self.created = true;
        let Some(mesh) = self.maps.get_mut(&map) else {
            return Ok(false);
        };
        // A map's ref slots are independent from every other map. Queries die before mesh.
        loop {
            let next = mesh.tiles().next();
            let Some((grid, _)) = next else { break };
            mesh.remove_tile(grid).map_err(mmap_error)?;
        }
        self.maps.remove(&map);
        Ok(true)
    }
    pub fn unload_instance(&mut self, map: u32, instance: u32) -> bool {
        self.created = true;
        self.maps
            .get_mut(&map)
            .is_some_and(|mesh| mesh.unload_instance(instance))
    }
}
pub(super) fn mmap_error(error: LiveError) -> Refusal {
    use benilla_native_nav::Error;
    refuse(match error {
        LiveError::NumericBinding => "execution.mmap.reference_cpu_fp_binding",
        LiveError::OffMeshNumericBinding => "execution.mmap.offmesh_nearest_numeric_owner",
        LiveError::Validation(Error::Format) => "execution.mmap.tile_format",
        LiveError::Validation(Error::Framing) => "execution.mmap.tile_framing",
        LiveError::Validation(Error::Unsupported) => {
            "execution.mmap.tile_topology_or_numeric_owner"
        }
        LiveError::Validation(Error::Identity) => "execution.mmap.tile_identity",
        LiveError::Validation(_) => "execution.mmap.validation_budget",
        LiveError::Allocation => "execution.mmap.native_allocation",
        LiveError::StaleQuery => "execution.mmap.stale_query",
        LiveError::StaleTile => "execution.mmap.stale_tile",
        LiveError::Native(_) => "execution.mmap.remove_tile_assertion",
    })
}

#[cfg(test)]
#[path = "mmap_manager_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "mmap_census.rs"]
mod census;
