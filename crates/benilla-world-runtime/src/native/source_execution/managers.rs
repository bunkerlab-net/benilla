// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! F4.01 persistent-state projection. Later managers do not run here.
use super::*;
use benilla_sim::resident_map::CellPair;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum PersistentSql {
    Begin,
    Delete {
        kind: GridSpawnKind,
        guid: u32,
        instance: u32,
    },
    Insert {
        kind: GridSpawnKind,
        guid: u32,
        deadline: u64,
        instance: u32,
    },
    Commit,
    Cleanup {
        kind: GridSpawnKind,
        database_seconds: i64,
    },
    QueryRespawns {
        kind: GridSpawnKind,
    },
    UpdateInstanceReset {
        instance: u32,
        reset: u64,
    },
    CreateInstance {
        instance: u32,
        map: u32,
        reset: u64,
        encounters: u32,
    },
    /// Map.cpp:1706 submits a write; this is not a completion acknowledgement.
    CreateWorld {
        map: u32,
    },
}
/// No default SQL, reset scheduler, observation or successful callback implementation.
pub trait PersistentEffects {
    fn sql(&mut self, operation: PersistentSql) -> Result<()>;
    /// Executes the query and reports its own result. This is not a recorded answer input.
    fn query_respawns(&mut self, kind: GridSpawnKind) -> Result<CharacterQueryStatus>;
    fn observe(&mut self, operation: PersistentOperation) -> Result<()>;
    fn dungeon_reset(&mut self, entry: PersistentMapEntry) -> Result<i64>;
}
/// Must be the effective ObjectMgr set, not raw F2 rows or preparation dispositions.
pub trait PersistentDefinitions {
    fn map_entries(&mut self) -> Result<Vec<PersistentMapEntry>>;
    fn spawn_map(&mut self, kind: GridSpawnKind, guid: u32) -> Result<Option<u32>>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum PersistentBindKind {
    Player,
    Group,
}
#[derive(Default)]
pub struct SourceManagers {
    initialization_started: bool,
    states: HashMap<(bool, u32), SourcePersistentState>,
    next_generation: u64,
    stopped: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PersistentManagerInventory {
    pub version: u32,
    pub initialization_started: bool,
    pub next_generation: u64,
    pub stopped: bool,
    pub states: Vec<PersistentInventory>,
}
impl PersistentManagerInventory {
    pub fn validate(&self) -> Result<()> {
        require(self.version == 1, "execution.persistent.inventory_version")?;
        let mut keys = std::collections::BTreeSet::new();
        let mut generations = std::collections::BTreeSet::new();
        for state in &self.states {
            state.validate()?;
            require(
                state.identity.generation <= self.next_generation
                    && generations.insert(state.identity.generation)
                    && keys.insert(SourceManagers::key(
                        state.identity.map,
                        state.identity.instance,
                    )),
                "execution.persistent.inventory_state",
            )?;
        }
        Ok(())
    }
}
impl SourceManagers {
    // Source instance lookup ignores mapId when instanceId is nonzero; .cpp:683-691.
    fn key(map: u32, instance: u32) -> (bool, u32) {
        (instance != 0, if instance != 0 { instance } else { map })
    }
    pub fn inventory(&self) -> PersistentManagerInventory {
        PersistentManagerInventory {
            version: 1,
            initialization_started: self.initialization_started,
            next_generation: self.next_generation,
            stopped: self.stopped,
            states: self.inventories(),
        }
    }
    pub fn lookup_state(&self, map: u32, instance: u32) -> Option<PersistentStateIdentity> {
        self.states
            .get(&Self::key(map, instance))
            .map(|state| state.identity)
    }
    pub fn inventories(&self) -> Vec<PersistentInventory> {
        let mut states: Vec<_> = self
            .states
            .values()
            .map(SourcePersistentState::inventory)
            .collect();
        states.sort_by_key(|state| state.identity);
        states
    }
    pub fn state(&self, id: PersistentStateIdentity) -> Result<&SourcePersistentState> {
        self.states
            .get(&Self::key(id.map, id.instance))
            .filter(|s| s.identity == id)
            .ok_or_else(|| refuse("execution.persistent.stale_state"))
    }
    pub fn state_mut(&mut self, id: PersistentStateIdentity) -> Result<&mut SourcePersistentState> {
        require(!self.stopped, "execution.stopped")?;
        self.states
            .get_mut(&Self::key(id.map, id.instance))
            .filter(|s| s.identity == id)
            .ok_or_else(|| refuse("execution.persistent.stale_state"))
    }
    pub fn dungeon_bind(
        &mut self,
        id: PersistentStateIdentity,
        kind: PersistentBindKind,
        owner: u64,
        add: bool,
        effects: &mut impl PersistentEffects,
    ) -> Result<()> {
        require(!self.stopped, "execution.stopped")?;
        let result = (|| {
            let state = self.state_mut(id)?;
            require(
                state.entry.dungeon() && owner > 0,
                "execution.persistent.dungeon_bind_owner",
            )?;
            let binds = match kind {
                PersistentBindKind::Player => &mut state.player_binds,
                PersistentBindKind::Group => &mut state.group_binds,
            };
            let before = binds.len();
            // std::list::remove erases every matching pointer; MapPersistentStateMgr.h:186-190.
            if add {
                binds.push(owner);
            } else {
                binds.retain(|bound| *bound != owner);
            }
            effects.observe(PersistentOperation::DungeonBind {
                state: id,
                kind,
                owner,
                add,
                before,
                after: binds.len(),
            })?;
            if !add {
                self.unload_if_empty(id, effects)?;
            }
            Ok(())
        })();
        self.stopped |= result.is_err();
        result
    }
    #[allow(clippy::too_many_arguments)]
    pub fn create_state(
        &mut self,
        entry: PersistentMapEntry,
        instance: u32,
        mut reset: i64,
        can_reset: bool,
        load: bool,
        encounters: u32,
        effects: &mut impl PersistentEffects,
    ) -> Result<PersistentStateIdentity> {
        require(!self.stopped, "execution.stopped")?;
        let result = (|| {
            let key = Self::key(entry.id, instance);
            if let Some(old) = self.states.get(&key) {
                return Ok(old.identity);
            }
            require(
                entry.id <= u16::MAX as u32 && (entry.instanceable() || instance == 0),
                "execution.persistent.map_domain",
            )?;
            if entry.dungeon() && reset == 0 {
                reset = effects.dungeon_reset(entry)?;
            }
            self.next_generation = self
                .next_generation
                .checked_add(1)
                .ok_or_else(|| refuse("execution.persistent.generation_overflow"))?;
            // WorldPersistentState's constructor forces instance zero; .cpp:663-678, .h:153.
            let id = PersistentStateIdentity {
                map: entry.id as u16 as u32,
                instance: if entry.instanceable() { instance } else { 0 },
                generation: self.next_generation,
            };
            let state = SourcePersistentState::new(id, entry, reset, can_reset, encounters);
            if entry.dungeon() && !load {
                effects.sql(PersistentSql::CreateInstance {
                    instance,
                    map: entry.id,
                    reset: if entry.map_type == 2 { 0 } else { reset as u64 },
                    encounters,
                })?;
            }
            self.states.insert(key, state);
            effects.observe(PersistentOperation::Created { state: id })?;
            Ok(id)
        })();
        self.stopped |= result.is_err();
        result
    }
    pub fn bind_map(
        &mut self,
        id: PersistentStateIdentity,
        map: Option<MapIdentity>,
        effects: &mut impl PersistentEffects,
    ) -> Result<()> {
        require(!self.stopped, "execution.stopped")?;
        let result = (|| {
            require(
                map.is_none_or(|m| {
                    m.map == id.map && m.instance == id.instance && m.creation_generation > 0
                }),
                "execution.persistent.map_binding",
            )?;
            let state = self.state_mut(id)?;
            let before = state.used_by_map;
            state.used_by_map = map;
            effects.observe(PersistentOperation::Binding {
                state: id,
                before,
                after: map,
            })?;
            if map.is_none() {
                self.unload_if_empty(id, effects)?;
            }
            Ok(())
        })();
        self.stopped |= result.is_err();
        result
    }
    fn unload_if_empty(
        &mut self,
        id: PersistentStateIdentity,
        effects: &mut impl PersistentEffects,
    ) -> Result<bool> {
        let state = self.state(id)?;
        if !state.unloadable() {
            return Ok(false);
        }
        // Normal dungeon unload writes its reset before deletion; .cpp:708-723,294-300.
        if state.entry.map_type == 1 && state.reset_time != 0 {
            effects.sql(PersistentSql::UpdateInstanceReset {
                instance: id.instance,
                reset: state.reset_time as u64,
            })?;
        }
        self.states.remove(&Self::key(id.map, id.instance));
        effects.observe(PersistentOperation::Unloaded { state: id })?;
        Ok(true)
    }
    pub fn save_respawn(
        &mut self,
        id: PersistentStateIdentity,
        kind: GridSpawnKind,
        guid: u32,
        deadline: i64,
        world_seconds: i64,
        effects: &mut impl PersistentEffects,
    ) -> Result<()> {
        require(!self.stopped, "execution.stopped")?;
        let result = (|| {
            let bg = self.state(id)?.entry.battleground();
            self.set_respawn(id, kind, guid, deadline, world_seconds, effects)?;
            // Source Save continues on `this` after Set can delete it. Do not emulate a dangling read.
            require(
                self.state(id).is_ok(),
                "execution.persistent.save_after_unload_undefined",
            )?;
            if bg {
                return Ok(());
            }
            effects.sql(PersistentSql::Begin)?;
            effects.sql(PersistentSql::Delete {
                kind,
                guid,
                instance: id.instance,
            })?;
            if deadline > world_seconds {
                effects.sql(PersistentSql::Insert {
                    kind,
                    guid,
                    deadline: deadline as u64,
                    instance: id.instance,
                })?;
            }
            effects.sql(PersistentSql::Commit)
        })();
        self.stopped |= result.is_err();
        result
    }
    fn set_respawn(
        &mut self,
        id: PersistentStateIdentity,
        kind: GridSpawnKind,
        guid: u32,
        deadline: i64,
        world_seconds: i64,
        effects: &mut impl PersistentEffects,
    ) -> Result<()> {
        let state = self.state_mut(id)?;
        let before = state.respawn(kind, guid);
        let after = state.set_respawn(kind, guid, deadline, world_seconds);
        effects.observe(PersistentOperation::Deadline {
            state: id,
            kind,
            guid,
            requested: deadline,
            world_seconds,
            before,
            after,
        })?;
        if deadline <= world_seconds {
            self.unload_if_empty(id, effects)?;
        }
        Ok(())
    }
    pub fn overlay(
        &mut self,
        id: PersistentStateIdentity,
        cell: CellPair,
        kind: GridSpawnKind,
        guid: u32,
        insert: bool,
        effects: &mut impl PersistentEffects,
    ) -> Result<()> {
        require(!self.stopped, "execution.stopped")?;
        let result = (|| {
            require(cell.validate(), "execution.cell_coordinates")?;
            let changed = self.state_mut(id)?.overlay(cell, kind, guid, insert);
            effects.observe(PersistentOperation::Overlay {
                state: id,
                cell_key: persistent_cell_key(cell),
                kind,
                guid,
                insert,
                changed,
            })
        })();
        self.stopped |= result.is_err();
        result
    }
    pub fn require_durable_checkpoint(&self) -> Result<()> {
        Err(refuse("execution.managers.f7_continuation_owner"))
    }

    /// Only this row's startup projection: InitWorldMaps, then creature and GO respawns.
    pub fn initialize_persistent(
        &mut self,
        inputs: &ManagerInitialInputs,
        definitions: &mut impl PersistentDefinitions,
        effects: &mut impl PersistentEffects,
    ) -> Result<()> {
        require(!self.stopped, "execution.stopped")?;
        require(
            !self.initialization_started,
            "execution.persistent.initialization_repeated",
        )?;
        self.initialization_started = true;
        let result = self.initialize_inner(inputs, definitions, effects);
        self.stopped |= result.is_err();
        result
    }
    fn initialize_inner(
        &mut self,
        inputs: &ManagerInitialInputs,
        definitions: &mut impl PersistentDefinitions,
        effects: &mut impl PersistentEffects,
    ) -> Result<()> {
        inputs.check()?;
        let mut entries = definitions.map_entries()?;
        entries.sort_by_key(|m| m.id); // InitWorldMaps scans effective DBC ID order; .cpp:981-987.
        require(
            entries.windows(2).all(|w| w[0].id != w[1].id),
            "execution.persistent.duplicate_map_entry",
        )?;
        for &entry in &entries {
            if !entry.instanceable() {
                self.create_state(entry, 0, 0, false, false, 0, effects)?;
            }
        }
        for kind in [GridSpawnKind::Creature, GridSpawnKind::GameObject] {
            let query = inputs.query(kind);
            effects.sql(PersistentSql::Cleanup {
                kind,
                database_seconds: query.database_seconds,
            })?;
            let status = effects.query_respawns(kind)?;
            inputs.validate_query_result(kind, status)?;
            let mut loaded = 0;
            for &ordinal in &query.ordinals {
                let row = inputs.row(kind, ordinal)?;
                let disposition = if let Some(map_id) = definitions.spawn_map(kind, row.guid)? {
                    if let Some(&entry) = entries.iter().find(|entry| entry.id == map_id) {
                        let joined = inputs.instance(row.instance);
                        if row.instance != 0
                            && (joined.map_or(0, |i| i.map) != map_id || !entry.instanceable())
                        {
                            RespawnDisposition::InvalidInstance
                        } else if row.instance == 0 && entry.instanceable() {
                            RespawnDisposition::InstanceableWithoutInstance
                        } else {
                            let reset = joined.map_or(0, |i| i.reset_time as i64);
                            let encounters = joined.map_or(0, |i| i.encounters);
                            let id = self.create_state(
                                entry,
                                row.instance,
                                reset,
                                entry.dungeon(),
                                true,
                                encounters,
                                effects,
                            )?;
                            self.set_respawn(
                                id,
                                kind,
                                row.guid,
                                row.deadline as i64,
                                query.world_seconds,
                                effects,
                            )?;
                            loaded += 1;
                            RespawnDisposition::Loaded
                        }
                    } else {
                        RespawnDisposition::MissingMap
                    }
                } else {
                    RespawnDisposition::MissingSpawn
                };
                effects.observe(PersistentOperation::InitialRow {
                    kind,
                    ordinal,
                    disposition,
                })?;
            }
            effects.observe(PersistentOperation::InitialQuery {
                kind,
                status,
                database_seconds: query.database_seconds,
                world_seconds: query.world_seconds,
                loaded,
            })?;
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum RespawnDisposition {
    MissingSpawn,
    MissingMap,
    InvalidInstance,
    InstanceableWithoutInstance,
    Loaded,
}
