// SPDX-License-Identifier: GPL-3.0-or-later
// Authored integration enters real F3 construction and F4.07 state; PoolManager.cpp:442-470.
use super::*;
struct PoolHost {
    host: Host,
    world: World,
    spawns: SourceSpawnManager,
    life: Option<SourceCreatureLifecycle>,
}
impl PersistentEffects for PoolHost {
    fn sql(&mut self, op: PersistentSql) -> Result<()> {
        self.host.sql(op)
    }
    fn query_respawns(&mut self, kind: GridSpawnKind) -> Result<CharacterQueryStatus> {
        self.host.query_respawns(kind)
    }
    fn observe(&mut self, op: PersistentOperation) -> Result<()> {
        self.host.observe(op)
    }
    fn dungeon_reset(&mut self, entry: PersistentMapEntry) -> Result<i64> {
        self.host.dungeon_reset(entry)
    }
}
impl PoolLeafOwner for PoolHost {
    fn pool_spawn(
        &mut self,
        kind: GridSpawnKind,
        guid: u32,
    ) -> Result<Option<PoolSpawnDefinition>> {
        assert_eq!((kind, guid), (GridSpawnKind::Creature, 40));
        Ok(Some(PoolSpawnDefinition {
            map: 0,
            cell: cell(0.0),
            entry: 7,
            respawn: [100, 100],
            gameobject_min: 0,
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
    fn pool_random_respawn(&mut self, bounds: [u32; 2]) -> Result<u32> {
        self.host
            .factory
            .random
            .creature_urand(bounds[0], bounds[1])
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
            .contains(PoolMember::Creature(40)));
        assert!(states
            .state_mut(request.state)?
            .cell_guids(cell(0.0))
            .creatures
            .contains(&40));
        self.host.factory.saved_time = states
            .state(request.state)?
            .respawn(GridSpawnKind::Creature, 40);
        let pair = cell(0.0);
        let attempt = GridLoadAttempt {
            grid: self
                .world
                .cells
                .grids()
                .identity(pair.grid_x(), pair.grid_y())
                .unwrap(),
            cell: pair,
            kind: GridSpawnKind::Creature,
            origin: GridSpawnOrigin::Persistent,
            guid: 40,
            catalog_ordinal: 40,
        };
        let outcome = request.load_creature(
            &mut self.host.factory,
            attempt,
            spawn(40, 0.0),
            &mut self.world.context(),
        )?;
        if let GridCandidateOutcome::Loaded(object) = outcome {
            self.life = Some(
                self.host
                    .factory
                    .states
                    .remove(&object)
                    .unwrap()
                    .into_lifecycle(
                        CreatureLifecycleSettings {
                            save_immediately: true,
                            world_boss: false,
                            forever_corpse: false,
                            corpse_acceleration_ms: 120_000,
                            pool: Some(1),
                        },
                        &self.world.objects,
                    )?,
            );
        }
        Ok(outcome)
    }
    fn pool_lookup(&mut self, map_id: MapIdentity, key: SpawnKey) -> Result<Option<ObjectHandle>> {
        assert_eq!(key.kind, GridSpawnKind::Creature);
        Ok(self
            .world
            .objects
            .lookup_database(map_id, DatabaseLookupKind::Creature, key.guid))
    }
    fn pool_live(&mut self, request: PoolLiveRequest, states: &mut SourceManagers) -> Result<()> {
        request.creature(
            self.life.as_mut().unwrap(),
            &mut CreatureLifecycleContext {
                grid: &mut self.world.context(),
                persistent: states,
                spawns: &mut self.spawns,
            },
            &mut self.host,
        )
    }
    fn pool_save_immediately(&mut self) -> Result<bool> {
        Ok(true)
    }
    fn pool_creature_world_boss(&mut self, _: ObjectHandle) -> Result<bool> {
        Ok(false)
    }
    fn pool_gameobject_dynamic(&mut self, _: u32) -> Result<Option<bool>> {
        panic!("creature only")
    }
    fn pool_schedule(&mut self, _: PoolScheduleRequest, _: &mut SourceManagers) -> Result<()> {
        panic!("creature only")
    }
}
impl PoolWork for PoolHost {
    fn engine_access(&mut self) -> Result<()> {
        Ok(())
    }
    fn chance(&mut self) -> Result<f64> {
        let random = &mut self.host.factory.random;
        SourceRandom::new(1, &mut random.mt, &mut random.cursor, &mut |event| {
            random.events.push(event)
        })
        .unwrap()
        .rand_chance()
        .map_err(|_| refuse("authored.chance"))
    }
    fn shuffle(&mut self, values: &mut [usize]) -> Result<()> {
        let random = &mut self.host.factory.random;
        SourceRandom::new(1, &mut random.mt, &mut random.cursor, &mut |event| {
            random.events.push(event)
        })
        .unwrap()
        .shuffle(values)
        .map_err(|_| refuse("authored.shuffle"))
    }
    fn can_spawn(
        &mut self,
        _: &mut SourcePoolManager,
        _: &mut SourceManagers,
        _: PersistentStateIdentity,
        _: PoolKind,
        _: u32,
    ) -> Result<bool> {
        Ok(false)
    }
    fn leaf_effect(
        &mut self,
        _: &mut SourcePoolManager,
        states: &mut SourceManagers,
        effect: PoolLeafEffect,
    ) -> Result<()> {
        execute_pool_leaf(states, effect, self)
    }
    fn observe(&mut self, _: PoolOperation) -> Result<()> {
        Ok(())
    }
}
#[test]
fn loaded_and_unloaded_pool_replacements_use_one_stream_and_actual_retained_state() {
    for loaded in [false, true] {
        let mut host = Host::new();
        host.wall = 100;
        host.factory.now = 100;
        let mut world = World::new(vec![]);
        if loaded {
            world
                .context()
                .load_grid(cell(0.0), GridLoadTrigger::Enter, &mut host.factory)
                .unwrap();
        }
        let mut persistent = SourceManagers::default();
        let id = persistent
            .create_state(
                PersistentMapEntry { id: 0, map_type: 0 },
                0,
                0,
                false,
                true,
                0,
                &mut host,
            )
            .unwrap();
        persistent.bind_map(id, Some(map()), &mut host).unwrap();
        let mut spawns = SourceSpawnManager::new(map(), id).unwrap();
        spawns.initialize(&mut persistent, &mut host).unwrap();
        let mut owner = PoolHost {
            host,
            world,
            spawns,
            life: None,
        };
        let mut pools = SourcePoolManager::default();
        struct Definitions;
        impl PoolLoadOwner for Definitions {
            fn spawn_map(&mut self, _: PoolKind, _: u32) -> Result<Option<u32>> {
                Ok(Some(0))
            }
            fn map_exists(&mut self, _: u32) -> Result<bool> {
                Ok(true)
            }
        }
        struct Rows(u8);
        impl PoolRows for Rows {
            fn maximum(&mut self) -> Result<Option<u16>> {
                Ok(Some(1))
            }
            fn template(&mut self) -> Result<Option<PoolTemplateRow>> {
                if self.0 == 0 {
                    self.0 = 1;
                    Ok(Some(PoolTemplateRow {
                        ordinal: 0,
                        id: 1,
                        limit: 1,
                        description: vec![],
                    }))
                } else {
                    Ok(None)
                }
            }
            fn member(&mut self, slot: usize) -> Result<Option<PoolMembershipRow>> {
                if slot == 2 && self.0 == 1 {
                    self.0 = 2;
                    Ok(Some(PoolMembershipRow {
                        ordinal: 0,
                        guid: 40,
                        pool: 1,
                        chance: 0.0,
                    }))
                } else {
                    Ok(None)
                }
            }
        }
        pools.load(&mut Rows(0), &mut Definitions).unwrap();
        pools
            .spawn_pool(&mut persistent, id, 1, false, &mut owner)
            .unwrap();
        assert_eq!(
            owner.host.factory.random.cursor.raw,
            if loaded { 3 } else { 1 }
        );
        assert_eq!(
            persistent
                .state(id)
                .unwrap()
                .respawn(GridSpawnKind::Creature, 40),
            200
        );
        if loaded {
            let life = owner.life.as_ref().unwrap();
            assert_eq!(life.construction.respawn_time, 200);
            assert!(life.respawn_overridden);
            let object = life.object();
            assert_eq!(
                owner
                    .world
                    .objects
                    .get(object)
                    .unwrap()
                    .guid
                    .unwrap()
                    .counter(),
                40
            );
            let before = owner.host.factory.random.cursor.raw;
            pools
                .update_pool(&mut persistent, id, 1, PoolKind::Creature, 40, &mut owner)
                .unwrap();
            assert_eq!(owner.host.factory.random.cursor.raw, before);
            assert_eq!(owner.life.as_ref().unwrap().object(), object);
            pools
                .despawn_pool(&mut persistent, id, 1, &mut owner)
                .unwrap();
            assert_eq!(owner.world.cells.removals().len(), 1);
        } else {
            assert!(owner.life.is_none());
        }
    }
}
