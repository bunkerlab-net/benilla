// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, PoolManager.cpp:305-335,442-552.
use super::*;
use benilla_sim::resident_map::CellPair;

#[derive(Clone, Copy, Debug)]
pub struct PoolSpawnDefinition {
    pub map: u32,
    pub cell: CellPair,
    pub entry: u32,
    pub respawn: [u32; 2],
    pub gameobject_min: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoolLeafClock {
    UnloadedCreature,
    UnloadedGameObject,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoolLoadRequest {
    pub state: PersistentStateIdentity,
    pub map: MapIdentity,
    pub key: SpawnKey,
}
impl PoolLoadRequest {
    fn validate<D: GridDefinitions, P: GridProductOwner>(
        self,
        attempt: GridLoadAttempt,
        context: &GridFactoryContext<'_, D, P>,
    ) -> Result<()> {
        require(
            self.map == context.cells.map()
                && self.map == attempt.grid.map
                && self.key
                    == SpawnKey {
                        kind: attempt.kind,
                        guid: attempt.guid,
                    },
            "execution.pool.constructor_binding",
        )
    }
    pub fn load_creature<D, P, F>(
        self,
        owner: &mut F,
        attempt: GridLoadAttempt,
        row: D::Row,
        context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<GridCandidateOutcome>
    where
        D: GridDefinitions,
        P: GridProductOwner,
        F: CreatureFactoryOwner<D, P>,
    {
        self.validate(attempt, context)?;
        load_pool_source_creature(owner, attempt, row, context)
    }
    pub fn load_gameobject<D, P, F>(
        self,
        owner: &mut F,
        attempt: GridLoadAttempt,
        row: D::Row,
        context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<GridCandidateOutcome>
    where
        D: GridDefinitions,
        P: GridProductOwner,
        F: GameObjectFactoryOwner<D, P>,
    {
        self.validate(attempt, context)?;
        load_pool_source_gameobject(owner, attempt, row, context)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoolLiveAction {
    SetCreatureDelay(u32),
    SetRespawnTime(u32),
    SaveRespawn,
    MapAdd,
    Remove,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoolLiveRequest {
    pub load: PoolLoadRequest,
    pub object: ObjectHandle,
    pub action: PoolLiveAction,
}
impl PoolLiveRequest {
    fn validate(&self, objects: &SourceObjects) -> Result<()> {
        let object = objects.get(self.object)?;
        require(
            object.map == Some(self.load.map)
                && object.database_guid == self.load.key.guid
                && object.kind
                    == match self.load.key.kind {
                        GridSpawnKind::Creature => SourceObjectKind::Creature,
                        GridSpawnKind::GameObject => SourceObjectKind::GameObject,
                    },
            "execution.pool.live_binding",
        )
    }
    pub fn creature<D, P, F>(
        self,
        creature: &mut SourceCreatureLifecycle,
        context: &mut CreatureLifecycleContext<'_, '_, D, P>,
        owner: &mut F,
    ) -> Result<()>
    where
        D: GridDefinitions,
        P: ResidencyProductOwner,
        F: CreatureLifecycleOwner<D, P>,
    {
        self.validate(context.grid.objects)?;
        require(
            creature.object() == self.object && context.spawns.persistent() == self.load.state,
            "execution.pool.live_binding",
        )?;
        match self.action {
            PoolLiveAction::SetCreatureDelay(delay) => {
                creature.set_respawn_delay(delay, false);
                Ok(())
            }
            PoolLiveAction::SetRespawnTime(delay) => {
                creature.set_respawn_time::<D, P>(delay, owner)
            }
            PoolLiveAction::SaveRespawn => creature.save_respawn(context, owner),
            PoolLiveAction::MapAdd => {
                owner.lifecycle_phase(CreatureLifecyclePhase::MapAdd, creature, context)
            }
            PoolLiveAction::Remove => context.grid.add_to_remove(self.object, owner).map(|_| ()),
        }
    }
    pub fn gameobject<D, P, F>(
        self,
        context: &mut GameObjectLifecycleContext<'_, '_, D, P>,
        owner: &mut F,
    ) -> Result<()>
    where
        D: GridDefinitions,
        P: ResidencyProductOwner,
        F: GameObjectLifecycleOwner<D, P>,
    {
        self.validate(context.grid.objects)?;
        require(
            context.spawns.persistent() == self.load.state,
            "execution.pool.live_binding",
        )?;
        match self.action {
            PoolLiveAction::SetCreatureDelay(_) => Err(refuse("execution.pool.live_kind")),
            PoolLiveAction::SetRespawnTime(delay) => {
                set_source_gameobject_respawn_time(owner, self.object, i64::from(delay), context)
            }
            PoolLiveAction::SaveRespawn => {
                save_source_gameobject_respawn(owner, self.object, context)
            }
            PoolLiveAction::MapAdd => {
                gameobjects::add_source_gameobject(owner, self.object, context.grid)
            }
            PoolLiveAction::Remove => context.grid.add_to_remove(self.object, owner).map(|_| ()),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoolScheduleRequest {
    pub state: PersistentStateIdentity,
    pub map: MapIdentity,
    pub guid: u32,
    pub delay: u32,
}
impl PoolScheduleRequest {
    pub fn execute(
        self,
        manager: &mut SourceSpawnManager,
        states: &mut SourceManagers,
        owner: &mut impl SpawnWork,
    ) -> Result<()> {
        require(
            manager.persistent() == self.state
                && states.state(self.state)?.map_binding() == Some(self.map),
            "execution.pool.scheduler_binding",
        )?;
        manager.respawn(
            states,
            SpawnKey {
                kind: GridSpawnKind::GameObject,
                guid: self.guid,
            },
            self.delay,
            owner,
        )
    }
}
/// Actual ObjectMgr/map/constructor/lifecycle owners; no successful default leaf effects.
pub trait PoolLeafOwner: PersistentEffects {
    fn pool_spawn(&mut self, kind: GridSpawnKind, guid: u32)
        -> Result<Option<PoolSpawnDefinition>>;
    fn pool_grid_loaded(&mut self, map: MapIdentity, cell: CellPair) -> Result<bool>;
    fn pool_random_respawn(&mut self, bounds: [u32; 2]) -> Result<u32>;
    fn pool_wall(&mut self, site: PoolLeafClock) -> Result<ClockSample>;
    fn pool_world_seconds(&mut self) -> Result<i64>;
    fn pool_load(
        &mut self,
        request: PoolLoadRequest,
        states: &mut SourceManagers,
    ) -> Result<GridCandidateOutcome>;
    fn pool_lookup(&mut self, map: MapIdentity, key: SpawnKey) -> Result<Option<ObjectHandle>>;
    fn pool_live(&mut self, request: PoolLiveRequest, states: &mut SourceManagers) -> Result<()>;
    fn pool_save_immediately(&mut self) -> Result<bool>;
    fn pool_creature_world_boss(&mut self, object: ObjectHandle) -> Result<bool>;
    fn pool_gameobject_dynamic(&mut self, entry: u32) -> Result<Option<bool>>;
    fn pool_schedule(
        &mut self,
        request: PoolScheduleRequest,
        states: &mut SourceManagers,
    ) -> Result<()>;
}
fn wall(owner: &mut impl PoolLeafOwner, site: PoolLeafClock) -> Result<i64> {
    let sample = owner.pool_wall(site)?;
    require(
        sample.kind() == ClockKind::WallSeconds,
        "execution.pool.wall_clock_kind",
    )?;
    Ok(sample.value())
}
fn deadline(now: i64, delay: u32) -> Result<i64> {
    now.checked_add(i64::from(delay))
        .ok_or_else(|| refuse("execution.pool.time_add_undefined"))
}
fn random_delay(owner: &mut impl PoolLeafOwner, bounds: [u32; 2]) -> Result<u32> {
    // Inverted unsigned bounds violate the distribution precondition; Util.cpp:74-77.
    require(
        bounds[0] <= bounds[1],
        "execution.pool.respawn_bounds_undefined",
    )?;
    owner.pool_random_respawn(bounds)
}
/// Executes the selected leaf at its source location; PoolManager.cpp:305-335,442-552.
pub fn execute_pool_leaf(
    states: &mut SourceManagers,
    effect: PoolLeafEffect,
    owner: &mut impl PoolLeafOwner,
) -> Result<()> {
    let kind = match effect.kind {
        PoolKind::Creature => GridSpawnKind::Creature,
        PoolKind::GameObject => GridSpawnKind::GameObject,
        PoolKind::Child => return Err(refuse("execution.pool.child_leaf")),
    };
    let Some(data) = owner.pool_spawn(kind, effect.guid)? else {
        return Ok(());
    };
    let key = SpawnKey {
        kind,
        guid: effect.guid,
    };
    if !matches!(effect.action, PoolEffectAction::Spawn { .. }) {
        // Despawn/re-add resolve the leaf's map, unlike Spawn1Object; PoolManager.cpp:310,328,535,549.
        let target = if effect.state.map == data.map {
            Some(effect.state)
        } else {
            states.lookup_state(data.map, 0)
        };
        let Some(target) = target else { return Ok(()) };
        if effect.action == PoolEffectAction::Despawn {
            states.overlay(target, data.cell, kind, effect.guid, false, owner)?;
        }
        if let Some(map) = states.state(target)?.map_binding() {
            if let Some(object) = owner.pool_lookup(map, key)? {
                owner.pool_live(
                    PoolLiveRequest {
                        load: PoolLoadRequest {
                            state: target,
                            map,
                            key,
                        },
                        object,
                        action: if effect.action == PoolEffectAction::Despawn {
                            PoolLiveAction::Remove
                        } else {
                            PoolLiveAction::MapAdd
                        },
                    },
                    states,
                )?;
            }
        }
        return Ok(());
    }
    let PoolEffectAction::Spawn { instantly } = effect.action else {
        unreachable!()
    };
    states.overlay(effect.state, data.cell, kind, effect.guid, true, owner)?;
    let map = states.state(effect.state)?.map_binding();
    if let Some(map) = map {
        if owner.pool_grid_loaded(map, data.cell)? {
            let load = PoolLoadRequest {
                state: effect.state,
                map,
                key,
            };
            let result = owner.pool_load(load, states)?;
            if let GridCandidateOutcome::Loaded(object) = result {
                if !instantly {
                    let delay = random_delay(owner, data.respawn)?;
                    if kind == GridSpawnKind::Creature {
                        owner.pool_live(
                            PoolLiveRequest {
                                load,
                                object,
                                action: PoolLiveAction::SetCreatureDelay(delay),
                            },
                            states,
                        )?;
                    }
                    owner.pool_live(
                        PoolLiveRequest {
                            load,
                            object,
                            action: PoolLiveAction::SetRespawnTime(delay),
                        },
                        states,
                    )?;
                    if owner.pool_save_immediately()?
                        || (kind == GridSpawnKind::Creature
                            && owner.pool_creature_world_boss(object)?)
                    {
                        owner.pool_live(
                            PoolLiveRequest {
                                load,
                                object,
                                action: PoolLiveAction::SaveRespawn,
                            },
                            states,
                        )?;
                    }
                }
            }
            return Ok(());
        }
    }
    if instantly {
        return Ok(());
    }
    if kind == GridSpawnKind::Creature {
        let now = wall(owner, PoolLeafClock::UnloadedCreature)?;
        let delay = random_delay(owner, data.respawn)?;
        let until = deadline(now, delay)?;
        let world = owner.pool_world_seconds()?;
        states.save_respawn(effect.state, kind, effect.guid, until, world, owner)?;
    } else if data.gameobject_min >= 0 {
        let delay = random_delay(owner, data.respawn)?;
        let now = wall(owner, PoolLeafClock::UnloadedGameObject)?;
        let until = deadline(now, delay)?;
        let world = owner.pool_world_seconds()?;
        states.save_respawn(effect.state, kind, effect.guid, until, world, owner)?;
        if let Some(map) = map {
            if owner
                .pool_gameobject_dynamic(data.entry)?
                .is_none_or(|dynamic| dynamic)
            {
                owner.pool_schedule(
                    PoolScheduleRequest {
                        state: effect.state,
                        map,
                        guid: effect.guid,
                        delay,
                    },
                    states,
                )?;
            }
        }
    }
    Ok(())
}
