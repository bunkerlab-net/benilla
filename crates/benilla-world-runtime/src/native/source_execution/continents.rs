// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! MapManager.cpp:50-110 construction, Map.cpp:137-194 spawn gates and independent preludes.
use super::*;
use benilla_sim::source_rng::{RawGenerator, SourceRandom, TraceEvent};
use std::collections::BTreeMap;

#[path = "world_scripts.rs"]
mod world_scripts;
pub use world_scripts::*;

/// Map.cpp:2966-2975.
pub fn source_map_forces_dynamic_guids(map: u32) -> bool {
    matches!(map, 533 | 543)
}

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

pub struct MapUpdaterConfig(i32);

impl MapUpdaterConfig {
    // World.cpp:625,2395-2402 defaults a missing or negative MapUpdate.Threads to 3.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        const DEFAULT_MAP_THREADS: i32 = 3;
        let value = match spawn_loader_config::assignment(bytes, "MapUpdate.Threads") {
            Ok((_, raw)) => {
                // Config.cpp:127-132 uses std::stoi and accepts a decimal prefix.
                let raw = raw.trim_ascii_start();
                let digits = raw.strip_prefix(['+', '-']).unwrap_or(raw);
                let end = raw.len() - digits.len()
                    + digits.bytes().take_while(u8::is_ascii_digit).count();
                raw[..end]
                    .parse::<i32>()
                    .map_err(|_| refuse("execution.continents.config_integer"))?
            }
            Err(e) if e.id == "execution.spawn_loader.config_missing.MapUpdate.Threads" => {
                DEFAULT_MAP_THREADS
            }
            Err(e) => return Err(e),
        };
        Ok(Self(if value < 0 {
            DEFAULT_MAP_THREADS
        } else {
            value
        }))
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
    /// Live WorldObject::m_transport bindings, initialized by Object.cpp:1170.
    pub passengers: SourcePassengers,
    pub weather: SourceMapWeather,
    pub guids: Option<MapGuidAllocators>,
    pub persistent: Option<PersistentStateIdentity>,
    pub script_id: u32,
    pub instance_data: Option<SourceWorldScript>,
    pub graveyards: SourceGraveyardManager,
    pub creature_spells: Option<CreatureSpellMapData>,
    pub variables: SourceWorldStateVariables,
    pub spawn_updated: bool,
    pub dynamic_tree: SourceDynamicMap,
    pub transports: SourceMoTransports,
    pub objects: SourceObjects,
    pub gameobjects: SourceGameObjects,
    pub units: SourceUnits,
    // Map.h:344-345,498. This live per-entry registry is not saved.
    pub(super) temporary_creatures: BTreeMap<u32, u32>,
    pub motions: BTreeMap<ObjectHandle, SourceMotionMaster>,
    pub ais: SourceCreatureAis,
    pub event_ais: BTreeMap<ObjectHandle, SourceEventAi>,
    pub threats: SourceThreats,
    pub combat: SourceCombat,
    // Map.h:448,501 owns the map's messager and spawn manager.
    pub messages: SourceMessager,
    pub spawns: Option<SourceSpawnManager>,
    grids_initialized: bool,
    loaded_grids: [u64; 64],
    terrain_referenced: bool,
    instance_data_complete: bool,
}
impl SourceContinent {
    // Map.cpp:2978-2986 calls instance data before walking live subscribers.
    pub fn on_event_happened(
        &mut self,
        event: u16,
        activate: bool,
        resume: bool,
        order: EventNotificationOrder,
    ) -> Result<()> {
        if let Some(instance) = &mut self.instance_data {
            instance.on_event_happened(&self.objects, event, activate)?;
        }
        self.objects
            .for_each_event_subscriber(self.identity, order, |handle| {
                let object = self.objects.get(handle)?;
                if !object.in_world {
                    return Ok(());
                }
                match object.kind {
                    // GameObject.h:868 dispatches through the object's AI.
                    SourceObjectKind::GameObject | SourceObjectKind::Transport => self
                        .gameobjects
                        .get(handle)?
                        .ai
                        .as_ref()
                        .ok_or_else(|| refuse("execution.gameobject.GameObject_868.ai_owner"))?
                        .on_event_happened(event, activate, resume),
                    // Creature.h:863 dispatches to the installed UnitAI, including on pets.
                    SourceObjectKind::Creature | SourceObjectKind::Pet => {
                        self.ais
                            .get(&self.objects, handle)?
                            .on_event_happened(event, activate, resume);
                        Ok(())
                    }
                    // Object.h:1088 provides the empty callback for other WorldObjects.
                    _ => Ok(()),
                }
            })
    }

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
        encounters: Option<&LoadedInstanceEncounters>,
    ) -> Result<()> {
        let state = self
            .persistent
            .ok_or_else(|| refuse("execution.continents.persistent_state_owner"))?;
        require(
            states.state(state)?.pools().initialized(),
            "execution.continents.pools_uninitialized",
        )?;
        self.graveyards.init(world_graveyards)?;
        self.variables.initialize(
            self.identity.map,
            states.state(state)?.completed_encounters,
            encounters,
        )?;
        Ok(())
    }
}

pub struct SourceContinents {
    pub grid_states: [NGridPhase; 4],
    pub max_instance_id: u32,
    pub maps: [SourceContinent; 2],
    updater: SourceScheduler,
}

pub(super) const WAR_EFFORT_GAMEOBJECT_FACTORY: &str =
    "execution.world.Object_2182.war_effort_gameobject_factory_owner";

/// Binds live factory inputs to `DynamicSpawnRequest::spawn_gameobject`.
pub type WorldGameObjectFactory<'a> =
    dyn FnMut(&mut SourceContinent, DynamicSpawnRequest, &EffectiveSpawnData) -> Result<bool> + 'a;
pub struct ReadySourceContinents {
    continents: SourceContinents,
    conditions: Option<SourceConditions>,
    loot: Option<SourceLootStores>,
    battlegrounds: SourceBattleGroundMgr,
}
impl ReadySourceContinents {
    pub fn continents(&self) -> &SourceContinents {
        &self.continents
    }

    // MapManager.cpp:396-407 visits the loaded map registry in (map, instance) order.
    pub fn next_map(&self, map_id: Option<u32>, after: Option<MapIdentity>) -> Option<MapIdentity> {
        self.continents
            .maps
            .iter()
            .map(|map| map.identity)
            .find(|&map| {
                map_id.is_none_or(|id| id == map.map)
                    && after.is_none_or(|prev| (map.map, map.instance) > (prev.map, prev.instance))
            })
    }

    pub fn map(&self, identity: MapIdentity) -> Result<&SourceContinent> {
        self.continents
            .maps
            .iter()
            .find(|map| map.identity == identity)
            .ok_or_else(|| refuse("execution.continents.map_identity"))
    }

    pub fn map_mut(&mut self, identity: MapIdentity) -> Result<&mut SourceContinent> {
        self.continents
            .maps
            .iter_mut()
            .find(|map| map.identity == identity)
            .ok_or_else(|| refuse("execution.continents.map_identity"))
    }

    // Object.cpp:2173-2188 looks up the row before calling the dynamic constructor.
    pub fn spawn_world_gameobject(
        &mut self,
        map: MapIdentity,
        thread: u64,
        guid: u32,
        spawns: &SourceEffectiveSpawns,
        factory: Option<&mut WorldGameObjectFactory<'_>>,
    ) -> Result<()> {
        let key = SpawnKey {
            kind: GridSpawnKind::GameObject,
            guid,
        };
        let Some(row) = spawns.complete_data(key)? else {
            return Ok(());
        };
        // Map.cpp:141-142 allows gameobjects even when the row has a spawn mask.
        let map = self.map_mut(map)?;
        let construct = factory.ok_or_else(|| refuse(WAR_EFFORT_GAMEOBJECT_FACTORY))?;
        construct(
            map,
            DynamicSpawnRequest {
                map: map.identity,
                thread,
                key,
            },
            row,
        )?;
        Ok(())
    }

    pub fn bind_conditions(&mut self, conditions: SourceConditions) {
        self.conditions = Some(conditions);
    }

    pub fn conditions(&self) -> Option<&SourceConditions> {
        self.conditions.as_ref()
    }

    pub fn bind_loot(&mut self, stores: SourceLootStores) {
        self.loot = Some(stores);
    }

    pub fn loot(&self) -> Option<&SourceLootStores> {
        self.loot.as_ref()
    }

    // MapManager.cpp:56-58 reads the signed thread count after the continent join.
    pub fn activate_map_updater(
        &mut self,
        config: &MapUpdaterConfig,
        worker_births: &[ThreadBirth],
    ) -> Result<()> {
        if config.0 > 0 {
            self.activate(config.0 as usize, worker_births)?;
        }
        Ok(())
    }

    // MapUpdater.cpp:28-35,61-64 derives activation from the presence of workers.
    fn activate(&mut self, workers: usize, worker_births: &[ThreadBirth]) -> Result<()> {
        if self.continents.updater.worker_count() != 0 {
            return Ok(());
        }
        require(
            workers == worker_births.len(),
            "execution.continents.map_updater_birth_count",
        )?;
        // Deviation: Use logical workers because replay follows recorded thread identities.
        for &birth in worker_births {
            self.continents
                .updater
                .birth(birth, ThreadRole::MapWorker)?;
        }
        Ok(())
    }

    pub fn map_updater(&self) -> &SourceScheduler {
        &self.continents.updater
    }

    // MapManager.cpp:203 queues an update when the updater is active.
    pub fn schedule_map_update(&mut self, _job: MapJob) -> Result<()> {
        Err(refuse(
            "execution.continents.MapManager_203.schedule_update_owner",
        ))
    }

    // World.cpp:1394 calls BattleGroundMgr after MapManager::Initialize returns.
    pub fn create_initial_battlegrounds(
        &mut self,
        inputs: &mut impl BattleGroundInputs,
    ) -> Result<usize> {
        self.battlegrounds
            .create_initial_battlegrounds(self.loot.as_ref(), inputs)
    }

    pub fn battlegrounds(&self) -> &SourceBattleGroundMgr {
        &self.battlegrounds
    }

    // World.cpp:1395 audits all loaded loot stores after battleground templates bind their references.
    pub fn check_reference_loot_templates(
        &mut self,
        notice: impl FnMut(LootReferenceNotice),
    ) -> Result<()> {
        self.loot
            .as_mut()
            .ok_or_else(|| refuse("execution.world.World_1395.loot_stores_owner"))?
            .check_reference_templates(self.battlegrounds.used_refloot(), notice)
    }
}

impl SourceContinents {
    // MapManager.cpp:92-111 waits for both initialization jobs before returning.
    pub fn join_initialization(self, outcomes: [Result<()>; 2]) -> Result<ReadySourceContinents> {
        // Deviation: Report map 0's refusal first because startup needs a deterministic error.
        let [map0, map1] = outcomes;
        map0?;
        map1?;
        Ok(ReadySourceContinents {
            continents: self,
            conditions: None,
            loot: None,
            battlegrounds: SourceBattleGroundMgr::default(),
        })
    }

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
                passengers: SourcePassengers::default(),
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
                creature_spells: None,
                variables: SourceWorldStateVariables::default(),
                spawn_updated: false, // SpawnManager.h:54, before Initialize at Map.cpp:200.
                dynamic_tree: SourceDynamicMap::new(identity),
                transports: SourceMoTransports::default(), // Map.cpp:161; Map.h:495.
                objects: SourceObjects::default(),
                gameobjects: SourceGameObjects::default(),
                units: SourceUnits::default(),
                temporary_creatures: BTreeMap::new(),
                motions: BTreeMap::new(),
                ais: SourceCreatureAis::default(),
                event_ais: BTreeMap::new(),
                threats: SourceThreats::default(),
                combat: SourceCombat::default(),
                messages: SourceMessager::new(MessageOwner::Map(identity)),
                spawns: None,
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
            updater: SourceScheduler::default(), // MapUpdater.h:36,49-56.
        })
    }
}

#[cfg(test)]
#[path = "continents_tests.rs"]
mod tests;
