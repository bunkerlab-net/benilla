// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! MapManager.cpp:50-110 construction and independent Map.cpp:172-190 preludes.
use super::*;
use benilla_sim::source_rng::{RawGenerator, SourceRandom, TraceEvent};
use std::collections::BTreeMap;

/// Catalog operands only. No recorded map births, script results or selected pool members.
pub struct ContinentDefinitions {
    pub entries: [PersistentMapEntry; 2],
    pub temporary_starts: (u32, u32),
    pub script_ids: BTreeMap<u32, u32>,
}
impl ContinentDefinitions {
    pub fn load(
        catalog: &mut super::super::production_compiler::VerifiedCatalog,
        maps: &BTreeMap<u32, u32>,
        config: &[u8],
    ) -> Result<Self> {
        let entries = [0, 1].map(|id| {
            maps.get(&id)
                .copied()
                .map(|map_type| PersistentMapEntry { id, map_type })
                .ok_or_else(|| refuse("execution.continents.map_entry_missing"))
        });
        let [map0, map1] = entries;
        let entries = [map0?, map1?];
        let reserve = |key| -> Result<u32> {
            let value = spawn_loader_config::assignment(config, key)?
                .1
                .parse::<i32>()
                .map_err(|_| refuse("execution.continents.config_integer"))?;
            // World.cpp:790-793,2395-2402 uses 100 for a negative reserve.
            Ok(if value < 0 { 100 } else { value as u32 })
        };
        let temporary_starts = (
            catalog
                .execution_continent_guid_maximum("creature")?
                .wrapping_add(1)
                .wrapping_add(reserve("GuidReserveSize.Creature")?),
            catalog
                .execution_continent_guid_maximum("gameobject")?
                .wrapping_add(1)
                .wrapping_add(reserve("GuidReserveSize.GameObject")?),
        );
        let script_ids = catalog
            .execution_world_templates()?
            .into_iter()
            .filter(|(id, _)| maps.get(id).is_some_and(|ty| !matches!(ty, 1..=3)))
            .collect();
        Ok(Self {
            entries,
            temporary_starts,
            script_ids,
        })
    }
}

pub struct ContinentConfig {
    pub grid_expiry: u32,
    pub visibility: f32,
    pub weather_enabled: bool,
    pub weather_interval: u32,
}
impl ContinentConfig {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let value = |key| spawn_loader_config::assignment(bytes, key).map(|v| v.1);
        let unsigned = |key, default| -> Result<u32> {
            let parsed = value(key)?
                .parse::<i32>()
                .map_err(|_| refuse("execution.continents.config_integer"))?;
            // World.cpp:2395-2402 replaces negative integers with the setting's default.
            Ok(if parsed < 0 { default } else { parsed as u32 })
        };
        let boolean = |key| -> Result<bool> {
            let v = value(key)?;
            Ok(v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes"))
        };
        let float = |key| -> Result<f32> {
            value(key)?
                .parse()
                .map_err(|_| refuse("execution.continents.config_float"))
        };
        let aggro = float("Rate.Creature.Aggro")?;
        let aggro = if aggro < 0.0 { 1.0 } else { aggro };
        let mut visibility = float("Visibility.Distance.Continents")?;
        // World.cpp:747-756 is if/else-if, not a clamp. The lower bound can exceed the upper.
        if visibility < 45.0 * aggro {
            visibility = 45.0 * aggro;
        } else if visibility > 533.333_3 {
            visibility = 533.333_3;
        }
        Ok(Self {
            grid_expiry: unsigned("GridCleanUpDelay", 300_000)?.max(60_000),
            visibility,
            weather_enabled: boolean("ActivateWeather")?,
            weather_interval: unsigned("ChangeWeatherInterval", 600_000)?,
        })
    }
}

/// These are actual cold constructor fields, not initialized manager outcomes.
pub struct SourceContinent {
    pub identity: MapIdentity,
    pub entry: PersistentMapEntry,
    pub grid_expiry: u32,
    pub unload_timer: u32,
    pub visibility: f32,
    pub cells: SourceCells,
    pub linking: SourceCreatureLinkingHolder,
    pub weather: SourceMapWeather,
    pub guids: Option<MapGuidAllocators>,
    pub persistent: Option<PersistentStateIdentity>,
    pub script_id: u32,
    pub variables: BTreeMap<i32, i32>,
    pub spawn_updated: bool,
    /// DynamicTree.cpp:60-64. Cold fields only; this does not expose a collision query.
    pub dynamic_tree_rebalance_ms: i64,
    pub dynamic_tree_unbalanced: i32,
    grids_initialized: bool,
    loaded_grids: [u64; 64],
    terrain_referenced: bool,
}
impl SourceContinent {
    /// Map.cpp:172-189. Call independently for each async job; no RNG is requested here.
    pub fn initialize_prelude(
        &mut self,
        temporary_starts: (u32, u32),
        terrain: &mut SourceTerrainManager,
        config: &ContinentConfig,
    ) -> Result<()> {
        require(
            !self.terrain_referenced,
            "execution.continents.prelude_repeated",
        )?;
        self.guids = Some(MapGuidAllocators::from_temporary_starts(
            self.identity,
            temporary_starts,
        )?);
        // SourceCells' empty SourceGrids is the sparse equivalent of all 4096 null pointers.
        self.loaded_grids.fill(0);
        self.grids_initialized = true;
        self.visibility = config.visibility;
        terrain.add_ref(self.identity.map)?;
        self.terrain_referenced = true;
        Ok(())
    }
    pub fn prelude_complete(&self) -> bool {
        self.grids_initialized && self.terrain_referenced
    }
    pub fn grid_loaded(&self, x: u16, y: u16) -> Result<bool> {
        require(x < 64 && y < 64, "execution.grid_coordinates")?;
        require(
            self.grids_initialized,
            "execution.continents.grids_uninitialized",
        )?;
        Ok(self.loaded_grids[usize::from(y)] & (1 << x) != 0)
    }
    /// Map.cpp:1662-1679. An absent/zero script returns before the F5 call.
    pub fn create_instance_data(&mut self, script_id: u32) -> Result<()> {
        require(
            self.prelude_complete(),
            "execution.continents.prelude_missing",
        )?;
        self.script_id = script_id;
        if script_id == 0 {
            return Ok(());
        }
        Err(MissingManagerOwner::F5InstanceInitialization.refusal())
    }
}

pub struct SourceContinents {
    pub grid_states: [NGridPhase; 4],
    pub max_instance_id: u32,
    pub maps: [SourceContinent; 2],
}
impl SourceContinents {
    /// MapManager.cpp:52-54,96-106; only constructors have a world-thread order.
    pub fn construct<E: RawGenerator, S: FnMut(TraceEvent)>(
        entries: [PersistentMapEntry; 2],
        maximum_instance: GuidMaximum,
        terrain: &mut SourceTerrainManager,
        config: &ContinentConfig,
        random: &mut SourceRandom<'_, E, S>,
    ) -> Result<Self> {
        let construct = |entry: PersistentMapEntry,
                         terrain: &mut SourceTerrainManager,
                         random: &mut SourceRandom<'_, E, S>|
         -> Result<SourceContinent> {
            require(
                !entry.instanceable(),
                "execution.continents.world_map_entry",
            )?;
            let identity = MapIdentity {
                map: entry.id,
                instance: 0,
                creation_generation: 1,
            };
            terrain.load_terrain(entry.id, random)?;
            Ok(SourceContinent {
                identity,
                entry,
                grid_expiry: config.grid_expiry,
                unload_timer: 0,
                visibility: 100.0, // ObjectDefines.h:32-38; Map.cpp:158.
                cells: SourceCells::new(identity)?,
                linking: SourceCreatureLinkingHolder::new(identity)?,
                weather: SourceMapWeather::new(
                    identity,
                    config.weather_enabled,
                    config.weather_interval,
                )?,
                guids: None,
                persistent: None,
                script_id: 0,
                variables: BTreeMap::new(),
                spawn_updated: false, // SpawnManager.h:54, before Initialize at Map.cpp:200.
                dynamic_tree_rebalance_ms: 200,
                dynamic_tree_unbalanced: 0,
                grids_initialized: false,
                loaded_grids: [0; 64],
                terrain_referenced: false,
            })
        };
        require(
            entries[0].id == 0 && entries[1].id == 1,
            "execution.continents.map_entries",
        )?;
        // InitStateMachine and InitMaxInstanceId precede either constructor.
        let grid_states = [
            NGridPhase::Invalid,
            NGridPhase::Active,
            NGridPhase::Idle,
            NGridPhase::Removal,
        ];
        let max_instance_id = match maximum_instance {
            GuidMaximum::NoResult => 0,
            GuidMaximum::Value(value) => value,
        };
        let map0 = construct(entries[0], terrain, random)?;
        let map1 = construct(entries[1], terrain, random)?;
        Ok(Self {
            grid_states,
            max_instance_id,
            maps: [map0, map1],
        })
    }
}

#[cfg(test)]
#[path = "continents_tests.rs"]
mod tests;
