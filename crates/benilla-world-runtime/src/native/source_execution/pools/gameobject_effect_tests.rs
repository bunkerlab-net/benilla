// SPDX-License-Identifier: GPL-3.0-or-later
// Authored integration reuses GO constructors, lifecycle and scheduler; PoolManager.cpp:475-519.
use super::*;
struct PoolHost {
    factory: Factory,
    world: World,
    spawns: SourceSpawnManager,
    row: GameObjectSpawn,
}
impl PersistentEffects for PoolHost {
    fn sql(&mut self, op: PersistentSql) -> Result<()> {
        self.factory.sql(op)
    }
    fn query_respawns(&mut self, kind: GridSpawnKind) -> Result<CharacterQueryStatus> {
        self.factory.query_respawns(kind)
    }
    fn observe(&mut self, op: PersistentOperation) -> Result<()> {
        self.factory.observe(op)
    }
    fn dungeon_reset(&mut self, entry: PersistentMapEntry) -> Result<i64> {
        self.factory.dungeon_reset(entry)
    }
}
impl PoolLeafOwner for PoolHost {
    fn pool_spawn(
        &mut self,
        kind: GridSpawnKind,
        guid: u32,
    ) -> Result<Option<PoolSpawnDefinition>> {
        assert_eq!((kind, guid), (GridSpawnKind::GameObject, 7));
        Ok(Some(PoolSpawnDefinition {
            map: 0,
            cell: compute_cell_pair(0, 0).unwrap(),
            entry: self.row.entry,
            respawn: [self.row.respawn_min as u32, self.row.respawn_max as u32],
            gameobject_min: self.row.respawn_min,
        }))
    }
    fn pool_grid_loaded(&mut self, map_id: MapIdentity, pair: CellPair) -> Result<bool> {
        assert_eq!(map_id, map());
        Ok(self
            .world
            .cells
            .grids()
            .object_data_loaded(pair.grid_x(), pair.grid_y()))
    }
    fn pool_random_respawn(&mut self, b: [u32; 2]) -> Result<u32> {
        self.factory.gameobject_urand(b[0], b[1])
    }
    fn pool_wall(&mut self, _: PoolLeafClock) -> Result<ClockSample> {
        Ok(ClockSample::authored(ClockKind::WallSeconds, 100))
    }
    fn pool_world_seconds(&mut self) -> Result<i64> {
        Ok(100)
    }
    fn pool_load(
        &mut self,
        request: PoolLoadRequest,
        states: &mut SourceManagers,
    ) -> Result<GridCandidateOutcome> {
        assert!(states
            .state(request.state)?
            .pools()
            .contains(PoolMember::GameObject(7)));
        assert!(states
            .state_mut(request.state)?
            .cell_guids(compute_cell_pair(0, 0).unwrap())
            .gameobjects
            .contains(&7));
        self.factory.saved = states
            .state(request.state)?
            .respawn(GridSpawnKind::GameObject, 7);
        let pair = compute_cell_pair(0, 0).unwrap();
        let attempt = GridLoadAttempt {
            grid: self
                .world
                .cells
                .grids()
                .identity(pair.grid_x(), pair.grid_y())
                .unwrap(),
            cell: pair,
            kind: GridSpawnKind::GameObject,
            origin: GridSpawnOrigin::Persistent,
            guid: 7,
            catalog_ordinal: 0,
        };
        request.load_gameobject(
            &mut self.factory,
            attempt,
            self.row.clone(),
            &mut self.world.context(),
        )
    }
    fn pool_lookup(&mut self, map_id: MapIdentity, key: SpawnKey) -> Result<Option<ObjectHandle>> {
        assert_eq!(key.kind, GridSpawnKind::GameObject);
        Ok(self
            .world
            .objects
            .lookup_database(map_id, DatabaseLookupKind::GameObject, key.guid))
    }
    fn pool_live(&mut self, request: PoolLiveRequest, states: &mut SourceManagers) -> Result<()> {
        request.gameobject(
            &mut GameObjectLifecycleContext {
                grid: &mut self.world.context(),
                persistent: states,
                spawns: &mut self.spawns,
            },
            &mut self.factory,
        )
    }
    fn pool_save_immediately(&mut self) -> Result<bool> {
        self.factory.go_save_immediately()
    }
    fn pool_creature_world_boss(&mut self, _: ObjectHandle) -> Result<bool> {
        panic!("GO only")
    }
    fn pool_gameobject_dynamic(&mut self, entry: u32) -> Result<Option<bool>> {
        Ok(self
            .factory
            .gameobject_template(entry)?
            .map(|template| template.extra_flags & 2 != 0))
    }
    fn pool_schedule(
        &mut self,
        request: PoolScheduleRequest,
        states: &mut SourceManagers,
    ) -> Result<()> {
        request.execute(&mut self.spawns, states, &mut self.factory)
    }
}
#[test]
fn loaded_unloaded_and_negative_pool_go_effects_use_actual_lifecycle_and_scheduling() {
    for loaded in [false, true] {
        for delay in [-25, 0, 25] {
            let mut factory = Factory::new();
            factory.lifecycle = Some(HostState::default());
            factory.templates.get_mut(&100).unwrap().extra_flags = 2;
            let mut world = World::new();
            if loaded {
                world
                    .context()
                    .load_grid(
                        compute_cell_pair(0, 0).unwrap(),
                        GridLoadTrigger::Enter,
                        &mut factory,
                    )
                    .unwrap();
            }
            let mut states = SourceManagers::default();
            let id = states
                .create_state(
                    PersistentMapEntry { id: 0, map_type: 0 },
                    0,
                    0,
                    false,
                    true,
                    0,
                    &mut factory,
                )
                .unwrap();
            states.bind_map(id, Some(map()), &mut factory).unwrap();
            let mut spawns = SourceSpawnManager::new(map(), id).unwrap();
            spawns.initialize(&mut states, &mut factory).unwrap();
            states
                .state_mut(id)
                .unwrap()
                .pools_mut()
                .add(PoolMember::GameObject(7), 1);
            let mut row = spawn();
            row.respawn_min = delay;
            row.respawn_max = delay;
            let mut owner = PoolHost {
                factory,
                world,
                spawns,
                row,
            };
            execute_pool_leaf(
                &mut states,
                PoolLeafEffect {
                    state: id,
                    kind: PoolKind::GameObject,
                    guid: 7,
                    action: PoolEffectAction::Spawn { instantly: false },
                },
                &mut owner,
            )
            .unwrap();
            assert_eq!(
                owner.factory.cursor.raw,
                if loaded {
                    if delay < 0 {
                        1
                    } else {
                        2
                    }
                } else if delay < 0 {
                    0
                } else {
                    1
                }
            );
            let persisted = states
                .state(id)
                .unwrap()
                .respawn(GridSpawnKind::GameObject, 7);
            if loaded {
                let object = owner
                    .world
                    .objects
                    .lookup_database(map(), DatabaseLookupKind::GameObject, 7)
                    .unwrap();
                let go = owner.factory.objects.get(object).unwrap();
                assert_eq!(
                    go.respawn_time,
                    if delay == 0 {
                        0
                    } else {
                        101 + i64::from(delay as u32)
                    }
                );
                assert_eq!(go.respawn_delay, delay as u32);
                assert_eq!(go.spawned_by_default, delay >= 0);
                assert_eq!(persisted, if delay > 0 { 126 } else { 0 });
                assert!(owner.spawns.inventory().active.is_empty());
                let before = owner.factory.cursor.raw;
                execute_pool_leaf(
                    &mut states,
                    PoolLeafEffect {
                        state: id,
                        kind: PoolKind::GameObject,
                        guid: 7,
                        action: PoolEffectAction::Respawn,
                    },
                    &mut owner,
                )
                .unwrap();
                assert_eq!(owner.factory.cursor.raw, before);
                execute_pool_leaf(
                    &mut states,
                    PoolLeafEffect {
                        state: id,
                        kind: PoolKind::GameObject,
                        guid: 7,
                        action: PoolEffectAction::Despawn,
                    },
                    &mut owner,
                )
                .unwrap();
                assert_eq!(owner.world.cells.removals().len(), 1);
            } else if delay >= 0 {
                assert_eq!(persisted, 101 + i64::from(delay));
                assert_eq!(owner.spawns.inventory().active.len(), 1);
                assert_eq!(
                    owner.spawns.inventory().active[0].deadline_millis,
                    101_000 + i64::from(delay) * 1000
                );
                let insert_deadlines: Vec<_> = owner
                    .factory
                    .life()
                    .events
                    .iter()
                    .filter_map(|event| {
                        if let Event::Sql(PersistentSql::Insert { deadline, .. }) = event {
                            Some(*deadline)
                        } else {
                            None
                        }
                    })
                    .collect();
                assert_eq!(
                    insert_deadlines,
                    if delay == 0 {
                        vec![101]
                    } else {
                        vec![125, 126]
                    }
                );
            } else {
                assert_eq!(persisted, 0);
                assert!(owner.spawns.inventory().active.is_empty());
            }
        }
    }
}
