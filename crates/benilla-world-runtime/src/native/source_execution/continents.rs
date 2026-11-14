// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! MapManager.cpp:50-110 construction, Map.cpp:137-194 spawn gates and independent preludes.
use super::*;
use benilla_sim::source_rng::{RawGenerator, SourceRandom, TraceEvent};
use std::collections::BTreeMap;

#[path = "world_scripts.rs"]
mod world_scripts;
pub use world_scripts::*;

/// Catalog operands only. No recorded map births, script results or selected pool members.
pub struct ContinentDefinitions {
    pub entries: [PersistentMapEntry; 2],
    pub temporary_starts: (u32, u32),
    pub script_ids: BTreeMap<u32, u32>,
    pub script_names: BTreeMap<u32, String>,
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
        let templates = catalog.execution_world_templates()?;
        let mut script_ids = BTreeMap::new();
        let mut script_names = BTreeMap::new();
        for (id, script, name) in templates {
            if maps.get(&id).is_some_and(|ty| !matches!(ty, 1..=3)) {
                script_ids.insert(id, script);
                script_names.insert(id, name);
            }
        }
        Ok(Self {
            entries,
            temporary_starts,
            script_ids,
            script_names,
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
    pub instance_data: Option<SourceWorldScript>,
    pub graveyards: SourceGraveyardManager,
    pub variables: BTreeMap<i32, i32>,
    pub spawn_updated: bool,
    pub dynamic_tree: SourceDynamicMap,
    grids_initialized: bool,
    loaded_grids: [u64; 64],
    terrain_referenced: bool,
    instance_data_complete: bool,
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
    /// Map.cpp:1662-1713. Constructor Initialize precedes the load query.
    pub fn create_instance_data<E: RawGenerator, S: FnMut(TraceEvent)>(
        &mut self,
        script_id: u32,
        script_name: &str,
        load: bool,
        random: &mut SourceRandom<'_, E, S>,
        storage: &mut impl WorldInstanceStorage,
    ) -> Result<()> {
        require(
            self.prelude_complete(),
            "execution.continents.prelude_missing",
        )?;
        if self.instance_data.is_some() {
            return Ok(());
        }
        self.script_id = script_id;
        if script_id == 0 {
            self.instance_data_complete = true;
            return Ok(());
        }
        self.instance_data = Some(SourceWorldScript::construct(
            self.identity,
            script_name,
            &mut self.variables,
            random,
        )?);
        if load {
            let query = format!("SELECT data FROM world WHERE map = '{}'", self.identity.map);
            match storage.query(self.identity.map, &query)? {
                WorldInstanceRow::Missing => storage.insert(self.identity.map)?,
                // Both derived scripts inherit InstanceData.h:64's empty Load.
                WorldInstanceRow::Null | WorldInstanceRow::Data(_) => {}
            }
        } else {
            self.instance_data
                .as_mut()
                .unwrap()
                .initialize(&mut self.variables, random)?;
        }
        self.instance_data_complete = true;
        Ok(())
    }
    /// Map.cpp:137-144. The null-info call replaces 0/0 with ObjectMgr spawn coordinates.
    pub fn can_spawn(
        &self,
        kind: SourceObjectKind,
        guid: u32,
        links: &SourceCreatureLinking,
        objects: &SourceObjects,
        state: &SourcePersistentState,
        owner: &mut impl LinkingGateOwner,
    ) -> Result<bool> {
        match kind {
            SourceObjectKind::Creature | SourceObjectKind::Pet => self
                .linking
                .can_spawn_guid(links, objects, state, owner, guid),
            SourceObjectKind::GameObject | SourceObjectKind::Transport => Ok(true),
            _ => Ok(false),
        }
    }
    /// Map.cpp:192-193. Bind the persistent state before any pool predicate runs.
    pub fn attach_persistent_state(
        &mut self,
        states: &mut SourceManagers,
        work: &mut impl PersistentEffects,
    ) -> Result<PersistentStateIdentity> {
        require(
            self.instance_data_complete,
            "execution.continents.instance_initialization_incomplete",
        )?;
        let state = states.create_state(self.entry, 0, 0, self.entry.dungeon(), false, 0, work)?;
        self.persistent = Some(state);
        states.bind_map(state, Some(self.identity), work)?;
        Ok(state)
    }
    /// Map.cpp:192-194, MapPersistentStateMgr.cpp:198-207.
    pub fn initialize_persistent_pools(
        &mut self,
        states: &mut SourceManagers,
        pools: &mut SourcePoolManager,
        events: &SourceGameEventManager,
        work: &mut (impl PoolWork + PersistentEffects),
    ) -> Result<()> {
        self.attach_persistent_state(states, work)?;
        self.initialize_pools(states, pools, events, work)
    }
    /// Map.cpp:194, after the map binding is installed.
    pub fn initialize_pools(
        &self,
        states: &mut SourceManagers,
        pools: &mut SourcePoolManager,
        events: &SourceGameEventManager,
        work: &mut impl PoolWork,
    ) -> Result<()> {
        let state = self
            .persistent
            .ok_or_else(|| refuse("execution.continents.persistent_state_owner"))?;
        require(
            states.state(state)?.map_binding() == Some(self.identity),
            "execution.continents.persistent_map_binding",
        )?;
        if !states.state(state)?.pools().initialized() {
            states.state_mut(state)?.pools_mut().mark_initialized();
            pools.initialize(states, state, work)?;
            events.initialize_state(states, state, pools, work)?;
        }
        Ok(())
    }
    /// Map.cpp:196-198. The world owner must have run at World.cpp:1158.
    pub fn initialize_after_pools(
        &mut self,
        states: &SourceManagers,
        world_graveyards: &SourceGraveyardManager,
    ) -> Result<()> {
        let state = self
            .persistent
            .ok_or_else(|| refuse("execution.continents.persistent_state_owner"))?;
        require(
            states.state(state)?.pools().initialized(),
            "execution.continents.pools_uninitialized",
        )?;
        self.graveyards.init(world_graveyards)?;
        // WorldStateVariableManager.cpp:30-32 first needs the loaded ObjectMgr
        // dungeon-encounter bounds. Do not skip to SpawnManager or navigation.
        Err(refuse(DUNGEON_ENCOUNTER_BOUNDS_OWNER))
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
                instance_data: None,
                graveyards: SourceGraveyardManager::default(),
                variables: BTreeMap::new(),
                spawn_updated: false, // SpawnManager.h:54, before Initialize at Map.cpp:200.
                dynamic_tree: SourceDynamicMap::new(identity),
                grids_initialized: false,
                loaded_grids: [0; 64],
                terrain_referenced: false,
                instance_data_complete: false,
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
