// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, PoolManager.cpp:1168-1241, MapPersistentStateMgr.h:374-395.
use super::*;

/// Supplies actual Map.dbc and the source unordered instance-container successor.
pub trait PoolStateOwner: PoolWork {
    fn pool_map_entry(&mut self, map: u32) -> Result<Option<PersistentMapEntry>>;
    fn pool_next_instance_state(
        &mut self,
        states: &SourceManagers,
        map: u32,
        after: Option<PersistentStateIdentity>,
    ) -> Result<Option<PersistentStateIdentity>>;
}
#[derive(Clone, Copy)]
enum MapAction {
    Spawn(bool),
    Despawn,
    Update(PoolKind, u32),
}
impl SourcePoolManager {
    pub fn init_spawn_pool(
        &mut self,
        states: &mut SourceManagers,
        state: PersistentStateIdentity,
        pool: u16,
        owner: &mut impl PoolWork,
    ) -> Result<()> {
        self.run(|this| {
            this.ensure_loaded()?;
            if this.template(pool)?.map == Some(states.state(state)?.entry.id) {
                this.spawn_inner(states, state, pool, true, owner)?;
            }
            Ok(())
        })
    }
    pub fn spawn_pool_in_maps(
        &mut self,
        states: &mut SourceManagers,
        pool: u16,
        instantly: bool,
        owner: &mut impl PoolStateOwner,
    ) -> Result<()> {
        self.in_maps(states, pool, MapAction::Spawn(instantly), owner)
    }
    pub fn despawn_pool_in_maps(
        &mut self,
        states: &mut SourceManagers,
        pool: u16,
        owner: &mut impl PoolStateOwner,
    ) -> Result<()> {
        self.in_maps(states, pool, MapAction::Despawn, owner)
    }
    pub fn update_pool_in_maps(
        &mut self,
        states: &mut SourceManagers,
        pool: u16,
        kind: PoolKind,
        trigger: u32,
        owner: &mut impl PoolStateOwner,
    ) -> Result<()> {
        self.in_maps(states, pool, MapAction::Update(kind, trigger), owner)
    }
    fn at_state(
        &mut self,
        states: &mut SourceManagers,
        state: PersistentStateIdentity,
        pool: u16,
        action: MapAction,
        owner: &mut impl PoolWork,
    ) -> Result<()> {
        match action {
            MapAction::Spawn(instantly) => self.spawn_inner(states, state, pool, instantly, owner),
            MapAction::Despawn => self.despawn_inner(states, state, pool, owner),
            MapAction::Update(kind, trigger) => {
                self.update_pool(states, state, pool, kind, trigger, owner)
            }
        }
    }
    fn in_maps(
        &mut self,
        states: &mut SourceManagers,
        pool: u16,
        action: MapAction,
        owner: &mut impl PoolStateOwner,
    ) -> Result<()> {
        self.run(|this| {
            this.ensure_loaded()?;
            let Some(map) = this.template(pool)?.map else {
                return Ok(());
            };
            let Some(entry) = owner.pool_map_entry(map)? else {
                return Ok(());
            };
            require(entry.id == map, "execution.pool.map_entry_binding")?;
            if !entry.instanceable() {
                if let Some(state) = states.lookup_state(map, 0) {
                    this.at_state(states, state, pool, action, owner)?;
                }
                return Ok(());
            }
            let mut next = owner.pool_next_instance_state(states, map, None)?;
            while let Some(state) = next {
                require(
                    states.state(state)?.entry == entry && state.instance != 0,
                    "execution.pool.state_traversal_binding",
                )?;
                // Source advances its iterator before invoking the callback; MapPersistentStateMgr.h:386.
                next = owner.pool_next_instance_state(states, map, Some(state))?;
                this.at_state(states, state, pool, action, owner)?;
            }
            Ok(())
        })
    }
}
