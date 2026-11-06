// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, GridMap.cpp:693-795,805-825.
//! TerrainInfo tile references and independent minute cleanup, with real F6 owners.
use super::*;
use benilla_sim::source_rng::{RawGenerator, SourceRandom, TraceEvent};
use benilla_sim::source_time::IntervalTimer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainTileIdentity {
    pub map: u32,
    pub terrain_generation: u64,
    pub x: u16,
    pub y: u16,
    pub generation: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainCleanupPhase {
    UnloadData,
    DeleteGridMap,
    UnloadVMap,
    UnloadMMap,
}
#[derive(Clone, Copy, Debug)]
pub struct TerrainQueryLoad {
    pub present: bool,
    pub fully_loaded: bool,
}
pub trait TerrainResidencyOwner: GridProductOwner {
    fn terrain_cleanup(
        &mut self,
        tile: TerrainTileIdentity,
        phase: TerrainCleanupPhase,
    ) -> Result<()>;
    fn load_query_tile(
        &mut self,
        _map: MapIdentity,
        _coordinates: ProductCoordinates,
        _map_only: bool,
    ) -> Result<TerrainQueryLoad> {
        Err(refuse("execution.residency.terrain_query_geometry_owner"))
    }
}
#[derive(Default)]
struct TerrainTile {
    references: i16,
    present: Option<TerrainTileIdentity>,
    load_attempted: bool,
    fully_loaded: bool,
}
pub struct SourceTerrainResidency {
    map: u32,
    generation: u64,
    next_tile: u64,
    timer: IntervalTimer,
    tiles: BTreeMap<(u16, u16), TerrainTile>,
    // Map::m_bLoadedGrids belongs to each map instance, not TerrainInfo.
    maps: BTreeSet<(MapIdentity, u16, u16)>,
}
impl SourceTerrainResidency {
    pub fn new<E: RawGenerator, S: FnMut(TraceEvent)>(
        map: u32,
        generation: u64,
        random: &mut SourceRandom<'_, E, S>,
    ) -> Result<Self> {
        require(generation > 0, "execution.residency.terrain_generation")?;
        let start = random
            .urand(20, 40)
            .map_err(|_| refuse("execution.residency.terrain_random_owner"))?;
        let mut timer = IntervalTimer::default();
        timer.set_interval(60_000);
        timer.set_current(i64::from(start) * 1000);
        Ok(Self {
            map,
            generation,
            next_tile: 1,
            timer,
            tiles: BTreeMap::new(),
            maps: BTreeSet::new(),
        })
    }
    fn coordinates(&self, x: u16, y: u16) -> Result<()> {
        require(x < 64 && y < 64, "execution.residency.terrain_coordinates")
    }
    pub fn reference_count(&self, x: u16, y: u16) -> Result<i16> {
        self.coordinates(x, y)?;
        Ok(self.tiles.get(&(x, y)).map_or(0, |tile| tile.references))
    }
    pub fn tile(&self, x: u16, y: u16) -> Result<Option<TerrainTileIdentity>> {
        self.coordinates(x, y)?;
        Ok(self.tiles.get(&(x, y)).and_then(|tile| tile.present))
    }
    pub fn load_attempted(&self, x: u16, y: u16) -> Result<bool> {
        self.coordinates(x, y)?;
        Ok(self
            .tiles
            .get(&(x, y))
            .is_some_and(|tile| tile.load_attempted))
    }
    pub fn cleanup_current(&self) -> i64 {
        self.timer.get_current()
    }
    // GetGrid can create unreferenced tiles before any NGrid; GridMap.cpp:1222-1238.
    pub fn query_at<O: TerrainResidencyOwner>(
        &mut self,
        map: MapIdentity,
        position: [u32; 2],
        map_only: bool,
        owner: &mut O,
    ) -> Result<Option<TerrainTileIdentity>> {
        require(
            map.map == self.map && map.creation_generation > 0,
            "execution.residency.terrain_map",
        )?;
        let coordinate = |bits| -> Result<u16> {
            let value = 32.0_f32 - f32::from_bits(bits) / f32::from_bits(0x4405_5555);
            require(
                value.is_finite() && (0.0..64.0).contains(&value),
                "execution.residency.terrain_query_coordinates",
            )?;
            Ok(value as u16)
        };
        let c = ProductCoordinates {
            x: coordinate(position[0])?,
            y: coordinate(position[1])?,
        };
        let tile = self.tiles.entry((c.x, c.y)).or_default();
        if tile.present.is_none() && tile.load_attempted {
            return Ok(None);
        }
        if tile.present.is_none() || (!tile.fully_loaded && !map_only) {
            let outcome = owner.load_query_tile(map, c, map_only)?;
            require(
                (outcome.present || tile.present.is_none())
                    && (!outcome.fully_loaded || outcome.present),
                "execution.residency.terrain_query_outcome",
            )?;
            tile.load_attempted = true;
            if outcome.present && tile.present.is_none() {
                tile.present = Some(new_tile(self.map, self.generation, &mut self.next_tile, c)?);
            }
            tile.fully_loaded = outcome.fully_loaded;
        }
        Ok(tile.present)
    }
    fn load<O: TerrainResidencyOwner>(
        &mut self,
        map: MapIdentity,
        coordinates: ProductCoordinates,
        owner: &mut O,
    ) -> Result<bool> {
        require(
            map.map == self.map && map.creation_generation > 0,
            "execution.residency.terrain_map",
        )?;
        self.coordinates(coordinates.x, coordinates.y)?;
        let key = (map, coordinates.x, coordinates.y);
        if self.maps.contains(&key) {
            return Ok(true);
        }
        let tile = self
            .tiles
            .entry((coordinates.x, coordinates.y))
            .or_default();
        // Pinned signed-short narrowing on increment, guarded decrement on Unload.
        tile.references = tile.references.wrapping_add(1);
        if tile.present.is_none() {
            let present = owner.load(map, coordinates)?;
            tile.load_attempted = true;
            if present {
                tile.present = Some(new_tile(
                    self.map,
                    self.generation,
                    &mut self.next_tile,
                    coordinates,
                )?);
                tile.fully_loaded = true;
            }
        }
        let present = tile.present.is_some();
        if present {
            self.maps.insert(key);
        }
        Ok(present)
    }
    fn release(&mut self, map: MapIdentity, coordinates: ProductCoordinates) -> Result<()> {
        self.coordinates(coordinates.x, coordinates.y)?;
        require(
            map.map == self.map && self.maps.remove(&(map, coordinates.x, coordinates.y)),
            "execution.residency.terrain_reference_owner",
        )?;
        let tile = self
            .tiles
            .get_mut(&(coordinates.x, coordinates.y))
            .ok_or_else(|| refuse("execution.residency.terrain_tile_absent"))?;
        if tile.present.is_some() {
            if tile.references > 0 {
                tile.references -= 1;
            }
            // UnrefGrid returns zero for nonpositive counts even when the stored count is negative.
            if tile.references <= 0 {
                tile.load_attempted = false;
            }
        }
        Ok(())
    }
    pub fn cleanup<O: TerrainResidencyOwner>(&mut self, diff: u32, owner: &mut O) -> Result<()> {
        self.timer
            .update(i64::from(diff))
            .map_err(|_| refuse("execution.residency.terrain_timer_overflow"))?;
        if !self.timer.passed() {
            return Ok(());
        }
        // TerrainInfo differs from object-grid x/y order: y outside x.
        for y in 0..64 {
            for x in 0..64 {
                let Some(tile) = self.tiles.get_mut(&(x, y)) else {
                    continue;
                };
                if tile.references != 0 {
                    continue;
                }
                let Some(identity) = tile.present.take() else {
                    continue;
                };
                tile.fully_loaded = false;
                tile.load_attempted = false;
                for phase in [
                    TerrainCleanupPhase::UnloadData,
                    TerrainCleanupPhase::DeleteGridMap,
                    TerrainCleanupPhase::UnloadVMap,
                    TerrainCleanupPhase::UnloadMMap,
                ] {
                    owner.terrain_cleanup(identity, phase)?;
                }
            }
        }
        self.timer
            .reset()
            .map_err(|_| refuse("execution.residency.terrain_timer_overflow"))
    }
}
/// The existing real loader calls this adapter. Missing geometry never becomes
/// a successful fake product receipt; TerrainInfo owns the shared tile lifetime.
pub struct SourceTerrainProducts<'a, O> {
    pub terrain: &'a mut SourceTerrainResidency,
    pub owner: &'a mut O,
}
impl<O: TerrainResidencyOwner> GridProductOwner for SourceTerrainProducts<'_, O> {
    fn load(&mut self, map: MapIdentity, coordinates: ProductCoordinates) -> Result<bool> {
        self.terrain.load(map, coordinates, self.owner)
    }
}
impl<O: TerrainResidencyOwner> ResidencyProductOwner for SourceTerrainProducts<'_, O> {
    fn release_grid_products(
        &mut self,
        map: MapIdentity,
        coordinates: ProductCoordinates,
    ) -> Result<()> {
        self.terrain.release(map, coordinates)
    }
}
fn new_tile(
    map: u32,
    terrain_generation: u64,
    next: &mut u64,
    c: ProductCoordinates,
) -> Result<TerrainTileIdentity> {
    let generation = *next;
    *next = generation
        .checked_add(1)
        .ok_or_else(|| refuse("execution.residency.tile_generation_exhausted"))?;
    Ok(TerrainTileIdentity {
        map,
        terrain_generation,
        x: c.x,
        y: c.y,
        generation,
    })
}
