// SPDX-License-Identifier: GPL-3.0-or-later
//! Loaded owners for SpawnManager.cpp:50-109, before Map.cpp:202.
use super::*;
use benilla_sim::source_spawn_groups::GroupRandom;
use std::sync::Arc;

pub(super) struct Initialize<'a> {
    pub thread: u64,
    pub clock: Option<(u64, i64, usize)>,
    pub spawns: &'a SourceEffectiveSpawns,
    pub effects: &'a SourceEventEffects,
    pub map: &'a SourceContinent,
    pub pools: &'a SourcePoolManager,
    pub links: &'a SourceCreatureLinking,
    pub objects: &'a SourceObjects,
    pub groups: &'a mut SourceSpawnGroups,
    pub formations: &'a mut SourceFormations,
    pub operations: Vec<SpawnOperation>,
    pub persistent: Vec<PersistentOperation>,
    pub visits: [usize; 2],
}
impl PersistentEffects for Initialize<'_> {
    fn sql(&mut self, _: PersistentSql) -> Result<()> {
        Err(refuse("execution.spawn.initialize.sql_owner"))
    }
    fn query_respawns(&mut self, _: GridSpawnKind) -> Result<CharacterQueryStatus> {
        Err(refuse("execution.spawn.initialize.query_owner"))
    }
    fn observe(&mut self, operation: PersistentOperation) -> Result<()> {
        self.persistent.push(operation);
        Ok(())
    }
    fn dungeon_reset(&mut self, _: PersistentMapEntry) -> Result<i64> {
        Err(refuse("execution.spawn.initialize.dungeon_reset_owner"))
    }
}
impl SpawnWork for Initialize<'_> {
    fn thread(&self) -> u64 {
        self.thread
    }
    fn wall_seconds(&mut self, call: SpawnClockCall) -> Result<ClockSample> {
        require(
            call == SpawnClockCall::Initialize,
            "execution.spawn.initialize.clock_call",
        )?;
        let (_, seconds, _) = self
            .clock
            .take()
            .ok_or_else(|| refuse(SPAWN_INITIALIZE_CLOCK_OWNER))?;
        Ok(ClockSample::authored(ClockKind::WallSeconds, seconds))
    }
    fn world_clock_millis(&mut self) -> Result<i64> {
        Err(refuse("execution.spawn.initialize.world_clock_owner"))
    }
    fn world_game_seconds(&mut self) -> Result<i64> {
        Err(refuse("execution.spawn.initialize.world_game_time_owner"))
    }
    fn dynamic_guids(&mut self, map: u32, kind: GridSpawnKind) -> Result<Arc<[u32]>> {
        require(
            map == self.map.identity.map,
            "execution.spawn.initialize.dynamic_map",
        )?;
        let guids = self.effects.dynamic_guids(map, kind);
        self.visits[usize::from(kind == GridSpawnKind::GameObject)] = guids.len();
        Ok(Arc::from(guids))
    }
    fn definition(&mut self, key: SpawnKey) -> Result<Option<DynamicSpawnDefinition>> {
        self.spawns.complete_data(key)?;
        Ok(self
            .spawns
            .pre_terrain_group_definition(key)?
            .map(|data| DynamicSpawnDefinition {
                map: data.map,
                cell: data.cell,
                spawn_mask: data.spawn_mask,
            }))
    }
    fn linking_can_spawn(&mut self, state: &SourcePersistentState, guid: u32) -> Result<bool> {
        self.map.linking.can_spawn_guid(
            self.links,
            self.objects,
            state,
            &mut MapLinkingGate {
                spawns: self.spawns,
                pools: self.pools,
                script: self.map.instance_data.as_ref(),
            },
            guid,
        )
    }
    fn groups(&mut self) -> Result<Arc<BTreeMap<u32, DynamicSpawnGroup>>> {
        self.groups.dynamic_groups()
    }
    fn create_group(
        &mut self,
        states: &mut SourceManagers,
        group: DynamicSpawnGroup,
    ) -> Result<SpawnGroupHandle> {
        let persistent = self
            .map
            .persistent
            .ok_or_else(|| refuse("execution.continents.persistent_state_owner"))?;
        let mut work = CreateGroup {
            thread: self.thread,
            formations: self.formations,
        };
        Ok(self
            .groups
            .create(self.map.identity, persistent, group.id, states, &mut work)?
            .handle)
    }
    fn update_group(
        &mut self,
        _: &mut SourceSpawnManager,
        _: &mut SourceManagers,
        _: SpawnGroupHandle,
    ) -> Result<()> {
        Err(refuse("execution.spawn.initialize.group_update_owner"))
    }
    fn respawn_group_in_vicinity(
        &mut self,
        _: &mut SourceSpawnManager,
        _: &mut SourceManagers,
        _: SpawnGroupHandle,
        _: [u32; 4],
        _: u32,
    ) -> Result<()> {
        Err(refuse("execution.spawn.initialize.group_vicinity_owner"))
    }
    fn construct(
        &mut self,
        _: &mut SourceSpawnManager,
        _: &mut SourceManagers,
        _: DynamicSpawnRequest,
    ) -> Result<bool> {
        Err(refuse("execution.spawn.initialize.construct_owner"))
    }
    fn spawn_operation(&mut self, map: MapIdentity, operation: SpawnOperation) -> Result<()> {
        require(
            map == self.map.identity,
            "execution.spawn.initialize.observation_map",
        )?;
        self.operations.push(operation);
        Ok(())
    }
}

struct CreateGroup<'a> {
    thread: u64,
    formations: &'a mut SourceFormations,
}
impl PersistentEffects for CreateGroup<'_> {
    fn sql(&mut self, _: PersistentSql) -> Result<()> {
        Err(refuse("execution.group.create.sql_owner"))
    }
    fn query_respawns(&mut self, _: GridSpawnKind) -> Result<CharacterQueryStatus> {
        Err(refuse("execution.group.create.query_owner"))
    }
    fn observe(&mut self, _: PersistentOperation) -> Result<()> {
        Err(refuse("execution.group.create.persistent_operation_owner"))
    }
    fn dungeon_reset(&mut self, _: PersistentMapEntry) -> Result<i64> {
        Err(refuse("execution.group.create.reset_owner"))
    }
}
impl GroupRandom for CreateGroup<'_> {
    type Error = Refusal;
    fn urand(&mut self, _: u32, _: u32) -> Result<u32> {
        Err(refuse("execution.group.create.urand_owner"))
    }
    fn shuffle(&mut self, _: &mut [usize]) -> Result<()> {
        Err(refuse("execution.group.create.shuffle_owner"))
    }
}
impl GroupWork for CreateGroup<'_> {
    fn thread(&self) -> u64 {
        self.thread
    }
    fn wall_seconds(&mut self, _: GroupClockCall) -> Result<ClockSample> {
        Err(refuse("execution.group.create.wall_owner"))
    }
    fn world_clock_millis(&mut self) -> Result<i64> {
        Err(refuse("execution.group.create.clock_owner"))
    }
    fn world_game_seconds(&mut self) -> Result<i64> {
        Err(refuse("execution.group.create.game_time_owner"))
    }
    fn condition(
        &mut self,
        _: &mut SourceSpawnGroups,
        _: &mut SourceManagers,
        _: GroupIdentity,
        _: i32,
        _: bool,
    ) -> Result<bool> {
        Err(refuse("execution.group.create.condition_owner"))
    }
    fn roll_chance_i(&mut self, _: i32) -> Result<bool> {
        Err(refuse("execution.group.create.chance_owner"))
    }
    fn random_entry(&mut self, _: SpawnKey) -> Result<u32> {
        Err(refuse("execution.group.create.random_entry_owner"))
    }
    fn spawn_definition(&mut self, _: SpawnKey) -> Result<Option<GroupSpawnDefinition>> {
        Err(refuse("execution.group.create.spawn_definition_owner"))
    }
    fn is_loaded(&mut self, _: MapIdentity, _: [u32; 3]) -> Result<bool> {
        Err(refuse("execution.group.create.loaded_owner"))
    }
    fn construct(
        &mut self,
        _: &mut SourceSpawnGroups,
        _: &mut SourceManagers,
        _: GroupSpawnRequest,
    ) -> Result<bool> {
        Err(refuse("execution.group.create.construct_owner"))
    }
    fn live_object(&mut self, _: MapIdentity, _: SpawnKey) -> Result<Option<ObjectHandle>> {
        Err(refuse("execution.group.create.live_object_owner"))
    }
    fn removed_object(&mut self, _: ObjectHandle) -> Result<GroupRemovedObject> {
        Err(refuse("execution.group.create.removed_object_owner"))
    }
    fn has_instance(&mut self, _: ObjectHandle) -> Result<bool> {
        Err(refuse("execution.group.create.instance_owner"))
    }
    fn callback(
        &mut self,
        groups: &mut SourceSpawnGroups,
        _: &mut SourceManagers,
        group: GroupIdentity,
        callback: GroupCallback,
    ) -> Result<()> {
        require(
            matches!(callback, GroupCallback::FormationCreate),
            "execution.group.create.callback_owner",
        )?;
        groups.state(group)?;
        let (formation, count) = groups.formation_definition(group)?;
        self.formations
            .create(group, formation.clone(), count, false)
    }
    fn group_operation(&mut self, operation: GroupOperation) -> Result<()> {
        require(
            matches!(operation, GroupOperation::Created(_)),
            "execution.group.create.operation_owner",
        )
    }
}

pub(super) fn bind_mmap_world(
    recording: &Path,
    config: &[u8],
    manager: &mut SourceMMapManager,
) -> Result<()> {
    // World.cpp:829-836 precedes either Map.cpp:202; reuse F6.07a's frozen bindings.
    manager.initialize_recorded_world(
        config,
        &Sha256Id::parse("321f2dc486e38f77037366bf84fb0ce8d09ed823ffa4524082ab01a8b0b79244")
            .unwrap(),
    )?;
    let root = recording
        .ancestors()
        .find(|path| path.ends_with("benilla-config/full-server/f1"))
        .ok_or_else(|| refuse("execution.startup.recorded_f1_path_component_missing"))?;
    let bytes = std::fs::read(root.join("products-manifest.json"))
        .map_err(|_| refuse("execution.mmap.products_manifest_missing"))?;
    require(
        Sha256Id::of(&bytes)
            == Sha256Id::parse("5d681785ca7cbccd26b453464e5d86a178290ed41caac9bac4c890053f69559f")
                .unwrap(),
        "execution.mmap.products_manifest_identity",
    )?;
    let manifest: Vec<serde_json::Value> = serde_json::from_slice(&bytes)
        .map_err(|_| refuse("execution.mmap.products_manifest_shape"))?;
    for row in manifest {
        let name = row["path"]
            .as_str()
            .ok_or_else(|| refuse("execution.mmap.product_path"))?;
        let Some(map) = name
            .strip_prefix("mmaps/")
            .and_then(|name| name.strip_suffix(".mmap"))
        else {
            continue;
        };
        let map: u32 = map
            .parse()
            .map_err(|_| refuse("execution.mmap.product_map"))?;
        let hash = Sha256Id::parse(
            row["sha256"]
                .as_str()
                .ok_or_else(|| refuse("execution.mmap.product_hash"))?,
        )
        .map_err(|_| refuse("execution.mmap.product_hash"))?;
        manager
            .files
            .retain_map(map, &root.join("products").join(name), hash)?;
    }
    Ok(())
}
