// SPDX-License-Identifier: GPL-3.0-or-later
//! Source active membership, lock ownership and executable relocation boundaries.
use super::*;
use benilla_sim::resident_map::{compute_cell_pair, compute_grid_pair, CellPair};
use std::collections::BTreeMap;
use std::ops::Bound::{Excluded, Unbounded};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourcePose(pub [u32; 4]);
impl SourcePose {
    pub fn cell(self) -> Result<CellPair> {
        compute_cell_pair(self.0[0], self.0[1])
            .map_err(|_| refuse("execution.active.position_coordinates"))
    }
}
#[derive(Default)]
pub struct SourceActive {
    members: BTreeMap<u64, ObjectHandle>,
    cursor: Option<u64>,
    walking: bool,
}
impl SourceActive {
    pub(super) fn contains_source_pointer(&self, pointer: u64) -> bool {
        self.members.contains_key(&pointer)
    }
    pub fn members(&self) -> impl Iterator<Item = ObjectHandle> + '_ {
        self.members.values().copied()
    }
    pub fn len(&self) -> usize {
        self.members.len()
    }
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }
    fn insert(&mut self, pointer: u64, object: ObjectHandle) -> Result<bool> {
        require(
            self.members.get(&pointer).is_none_or(|old| *old == object),
            "execution.active.pointer_lifetime",
        )?;
        Ok(self.members.insert(pointer, object).is_none())
    }
    fn following(&self, pointer: u64) -> Option<u64> {
        self.members
            .range((Excluded(pointer), Unbounded))
            .next()
            .map(|(&key, _)| key)
    }
    fn remove(&mut self, pointer: u64) -> bool {
        if self.cursor == Some(pointer) {
            self.cursor = self.following(pointer);
        }
        self.members.remove(&pointer).is_some()
    }
    // Advance before callbacks; erasing the next member advances the stored iterator; Map.cpp:945-952,1609-1618.
    pub fn begin_visit(&mut self) -> Result<()> {
        require(!self.walking, "execution.active.nested_traversal")?;
        self.walking = true;
        self.cursor = self.members.first_key_value().map(|(&key, _)| key);
        Ok(())
    }
    pub fn next_member(&mut self) -> Result<Option<ObjectHandle>> {
        require(self.walking, "execution.active.traversal_not_started")?;
        let Some(pointer) = self.cursor else {
            return Ok(None);
        };
        let object = self.members[&pointer];
        self.cursor = self.following(pointer);
        Ok(Some(object))
    }
    pub fn finish_visit(&mut self) {
        self.walking = false;
        self.cursor = None;
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveLockOutcome {
    NotDatabaseCreature,
    Changed { grid: GridIdentity, count: u16 },
    MissingSpawnGrid { x: u16, y: u16 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActiveChange {
    pub membership_changed: bool,
    pub lock: ActiveLockOutcome,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelocationPhase {
    ViewpointGridChanged {
        old: CellIdentity,
        new: CellIdentity,
    },
    CombatStopWithPets,
    MotionClear,
    MotionInitialize,
    CreatureOnRelocated,
    GameObjectModelPosition,
    GameObjectVisibility,
}
impl RelocationPhase {
    pub fn missing_owner(self) -> Refusal {
        refuse(match self {
            Self::ViewpointGridChanged { .. } => "execution.relocation.viewpoint_owner",
            Self::CombatStopWithPets => "execution.relocation.combat_and_pets_owner",
            Self::MotionClear | Self::MotionInitialize => "execution.relocation.motion_owner",
            Self::CreatureOnRelocated => "execution.relocation.creature_notification_owner",
            Self::GameObjectModelPosition => "execution.relocation.dynamic_collision_owner",
            Self::GameObjectVisibility => "execution.relocation.visibility_owner",
        })
    }
}
pub trait ActiveRelocationOwner<D: GridDefinitions, P: GridProductOwner>:
    GridObjectFactory<D, P>
{
    fn relocation_phase(
        &mut self,
        _object: ObjectHandle,
        phase: RelocationPhase,
        _context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<()> {
        Err(phase.missing_owner())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreatureMoveOutcome {
    Moved,
    Respawned,
    RefusedColdRespawn,
}
impl SourceCells {
    pub fn active(&self) -> &SourceActive {
        &self.active
    }
    pub fn active_mut(&mut self) -> &mut SourceActive {
        &mut self.active
    }
    // Include the extra boundary cell, then expand by ceilf(visibility/cell_width)+1; Map.cpp:1546-1576.
    pub fn active_objects_near_grid(
        &self,
        x: u16,
        y: u16,
        visibility: f32,
        objects: &SourceObjects,
    ) -> Result<bool> {
        require(
            x < 64 && y < 64 && visibility.is_finite() && visibility >= 0.0,
            "execution.active.visibility_domain",
        )?;
        let range = (visibility / (533.333_3_f32 / 16.0)).ceil();
        require(
            range < i32::MAX as f32 - 1.0,
            "execution.active.visibility_domain",
        )?;
        let range = range as u32 + 1;
        let low = [u32::from(x) * 16, u32::from(y) * 16].map(|v| v.saturating_sub(range));
        let high = [u32::from(x) * 16 + 16, u32::from(y) * 16 + 16].map(|v| (v + range).min(1023));
        for object in objects.map_players(self.map()).chain(self.active.members()) {
            let pair = objects.pose(object)?.cell()?;
            if low[0] <= u32::from(pair.x())
                && u32::from(pair.x()) <= high[0]
                && low[1] <= u32::from(pair.y())
                && u32::from(pair.y()) <= high[1]
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn active_lock(
        &mut self,
        objects: &SourceObjects,
        object: ObjectHandle,
        add: bool,
    ) -> Result<ActiveLockOutcome> {
        let object = objects.get(object)?;
        if object.kind != SourceObjectKind::Creature || !object.database_spawn_data {
            return Ok(ActiveLockOutcome::NotDatabaseCreature);
        }
        let respawn = object
            .respawn_pose
            .ok_or_else(|| refuse("execution.active.respawn_definition_owner"))?;
        let pair = compute_grid_pair(respawn.0[0], respawn.0[1])
            .map_err(|_| refuse("execution.active.respawn_coordinates"))?;
        let Some(grid) = self.grids.identity(pair.x(), pair.y()) else {
            return Ok(ActiveLockOutcome::MissingSpawnGrid {
                x: pair.x(),
                y: pair.y(),
            });
        };
        let count = self.grids.change_active_lock(grid, add)?;
        Ok(ActiveLockOutcome::Changed { grid, count })
    }
}
impl<D: GridDefinitions, P: GridProductOwner> GridFactoryContext<'_, D, P> {
    // Insert before loading the current grid and increment even after duplicate insertion; Map.cpp:1579-1604.
    pub fn add_to_active<F: GridObjectFactory<D, P>>(
        &mut self,
        object: ObjectHandle,
        factory: &mut F,
    ) -> Result<ActiveChange> {
        let value = self.objects.get(object)?;
        require(
            value.map == Some(self.cells.map()) && value.kind != SourceObjectKind::Player,
            "execution.active.object_map_kind",
        )?;
        let pointer = value
            .source_allocation
            .ok_or_else(|| refuse("execution.active.source_allocation_owner"))?;
        let pair = self.objects.pose(object)?.cell()?;
        let membership_changed = self.cells.active.insert(pointer, object)?;
        self.load_grid(pair, GridLoadTrigger::ActiveObject, factory)?;
        let lock = self.cells.active_lock(self.objects, object, true)?;
        Ok(ActiveChange {
            membership_changed,
            lock,
        })
    }
    // Removal adjusts the live cursor, then conditionally decrements the spawn-grid count; Map.cpp:1607-1639.
    pub fn remove_from_active(&mut self, object: ObjectHandle) -> Result<ActiveChange> {
        let value = self.objects.get(object)?;
        require(
            value.map == Some(self.cells.map()) && value.kind != SourceObjectKind::Player,
            "execution.active.object_map_kind",
        )?;
        let pointer = value
            .source_allocation
            .ok_or_else(|| refuse("execution.active.source_allocation_owner"))?;
        let membership_changed = self.cells.active.remove(pointer);
        let lock = self.cells.active_lock(self.objects, object, false)?;
        Ok(ActiveChange {
            membership_changed,
            lock,
        })
    }
    // Flag assignment follows any in-world add/remove, while equal requests do nothing; Object.cpp:2566-2580.
    pub fn set_active_object_state<F: GridObjectFactory<D, P>>(
        &mut self,
        object: ObjectHandle,
        active: bool,
        factory: &mut F,
    ) -> Result<Option<ActiveChange>> {
        let value = self.objects.get(object)?;
        if value.active == active || (value.kind == SourceObjectKind::Player && !active) {
            return Ok(None);
        }
        let changed = if value.in_world && value.kind != SourceObjectKind::Player {
            Some(if active {
                self.add_to_active(object, factory)?
            } else {
                self.remove_from_active(object)?
            })
        } else {
            None
        };
        self.objects.get_mut(object)?.active = active;
        Ok(changed)
    }
    pub fn creature_cell_relocation<F: ActiveRelocationOwner<D, P>>(
        &mut self,
        object: ObjectHandle,
        target: CellPair,
        factory: &mut F,
    ) -> Result<CellRelocation> {
        let result = {
            let mut loader = SourceGridLoader {
                definitions: &mut *self.definitions,
                factory: &mut *factory,
                products: &mut *self.products,
                sets: &mut *self.sets,
                objects: &mut *self.objects,
            };
            // The loader receives the same object registry, not a reconstructed snapshot; Map.cpp:1229.
            loader.relocate_creature(self.cells, object, target)?
        };
        if let CellRelocation::Moved { old, new } = result {
            factory.relocation_phase(
                object,
                RelocationPhase::ViewpointGridChanged { old, new },
                self,
            )?;
        }
        Ok(result)
    }
    pub fn creature_respawn_relocation<F: ActiveRelocationOwner<D, P>>(
        &mut self,
        object: ObjectHandle,
        factory: &mut F,
    ) -> Result<bool> {
        let respawn = self
            .objects
            .get(object)?
            .respawn_pose
            .ok_or_else(|| refuse("execution.relocation.respawn_definition_owner"))?;
        let pair = respawn.cell()?;
        factory.relocation_phase(object, RelocationPhase::CombatStopWithPets, self)?;
        factory.relocation_phase(object, RelocationPhase::MotionClear, self)?;
        if self.creature_cell_relocation(object, pair, factory)? == CellRelocation::RefusedUnloaded
        {
            return Ok(false);
        }
        self.objects.set_pose(object, respawn)?;
        factory.relocation_phase(object, RelocationPhase::MotionInitialize, self)?;
        factory.relocation_phase(object, RelocationPhase::CreatureOnRelocated, self)?;
        Ok(true)
    }
    // Refused cold moves try respawn after combat/motion cleanup, without removing the actor; Map.cpp:1166-1183,1244-1265.
    pub fn move_creature<F: ActiveRelocationOwner<D, P>>(
        &mut self,
        object: ObjectHandle,
        pose: SourcePose,
        factory: &mut F,
    ) -> Result<CreatureMoveOutcome> {
        if self.creature_cell_relocation(object, pose.cell()?, factory)?
            != CellRelocation::RefusedUnloaded
        {
            self.objects.set_pose(object, pose)?;
            factory.relocation_phase(object, RelocationPhase::CreatureOnRelocated, self)?;
            return Ok(CreatureMoveOutcome::Moved);
        }
        Ok(if self.creature_respawn_relocation(object, factory)? {
            CreatureMoveOutcome::Respawned
        } else {
            CreatureMoveOutcome::RefusedColdRespawn
        })
    }
}

#[derive(Clone, Debug)]
pub struct ActivePreloadSpawn {
    pub kind: GridSpawnKind,
    pub map: u32,
    pub database_guid: u32,
    pub pose: SourcePose,
    pub game_event: i32,
    pub guid_pool: u32,
    pub entry_pool: u32,
    pub dynamic_guid: bool,
    pub template_extra_flags: Option<u32>,
}
#[derive(Default)]
pub struct ActivePreloadCatalog {
    definitions: BTreeMap<(GridSpawnKind, u32, u32), ActivePreloadSpawn>,
    creatures: BTreeMap<u32, Vec<u32>>,
    gameobjects: BTreeMap<u32, Vec<u32>>,
}
impl ActivePreloadCatalog {
    // Equivalent multimap keys retain actual loader occurrence order, not GUID order; ObjectMgr.cpp:2040-2045,2233-2242, ObjectMgr.h:1351.
    pub fn register_loaded_spawn(
        &mut self,
        spawn: ActivePreloadSpawn,
        autoload_active: bool,
    ) -> Result<()> {
        let key = (spawn.kind, spawn.map, spawn.database_guid);
        require(
            !self.definitions.contains_key(&key),
            "execution.active.preload_definition_occurrence",
        )?;
        let dynamic_go =
            spawn.kind == GridSpawnKind::GameObject && spawn.dynamic_guid && spawn.game_event == 0;
        let eligible = !dynamic_go
            && spawn.game_event == 0
            && spawn.guid_pool == 0
            && spawn.entry_pool == 0
            && autoload_active
            && spawn
                .template_extra_flags
                .is_some_and(|flags| flags & 0x1000 != 0);
        if eligible {
            let by_map = if spawn.kind == GridSpawnKind::Creature {
                &mut self.creatures
            } else {
                &mut self.gameobjects
            };
            by_map
                .entry(spawn.map)
                .or_default()
                .push(spawn.database_guid);
        }
        self.definitions.insert(key, spawn);
        Ok(())
    }
    pub fn normal_order(&self, map: u32) -> impl Iterator<Item = &ActivePreloadSpawn> + '_ {
        [GridSpawnKind::Creature, GridSpawnKind::GameObject]
            .into_iter()
            .flat_map(move |kind| {
                let rows = if kind == GridSpawnKind::Creature {
                    &self.creatures
                } else {
                    &self.gameobjects
                };
                rows.get(&map)
                    .into_iter()
                    .flatten()
                    .map(move |guid| &self.definitions[&(kind, map, *guid)])
            })
    }
    // Forced-map traversal is the actual unordered CreatureDataMap owner, not a sorted substitute; ObjectMgr.cpp:9135-9141, ObjectMgr.h:162.
    pub fn load<D: GridDefinitions, P: GridProductOwner, F: GridObjectFactory<D, P>>(
        &self,
        force_map: bool,
        context: &mut GridFactoryContext<'_, D, P>,
        factory: &mut F,
    ) -> Result<()> {
        if force_map {
            return Err(refuse(
                "execution.active.forced_creature_data_traversal_owner",
            ));
        }
        for spawn in self.normal_order(context.cells.map().map) {
            context.load_grid(spawn.pose.cell()?, GridLoadTrigger::Force, factory)?;
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameObjectMoveOutcome {
    Moved,
    RefusedAbsentGrid,
    RefusedColdGrid,
}
impl<D: GridDefinitions, P: GridProductOwner> GridFactoryContext<'_, D, P> {
    // GO relinks before viewpoint, then changes pose, collision position and visibility; Map.cpp:1186-1216.
    pub fn move_gameobject<F: ActiveRelocationOwner<D, P> + GameObjectFactoryOwner<D, P>>(
        &mut self,
        object: ObjectHandle,
        pose: SourcePose,
        respawn_relocation_on_fail: bool,
        factory: &mut F,
    ) -> Result<GameObjectMoveOutcome> {
        let pair = pose.cell()?;
        let value = self.objects.get(object)?;
        require(
            value.map == Some(self.cells.map())
                && matches!(
                    value.kind,
                    SourceObjectKind::GameObject | SourceObjectKind::Transport
                ),
            "execution.relocation.gameobject_map_kind",
        )?;
        let old = value
            .current_cell
            .ok_or_else(|| refuse("execution.relocation.gameobject_cell"))?;
        if !respawn_relocation_on_fail
            && self
                .cells
                .grids
                .identity(pair.grid_x(), pair.grid_y())
                .is_none()
        {
            return Ok(GameObjectMoveOutcome::RefusedAbsentGrid);
        }
        if (old.grid.x, old.grid.y) != (pair.grid_x(), pair.grid_y()) {
            if !value.active && !self.cells.object_data_loaded(pair.grid_x(), pair.grid_y()) {
                return Ok(GameObjectMoveOutcome::RefusedColdGrid);
            }
            self.load_grid(pair, GridLoadTrigger::Enter, factory)?;
        }
        let new = self.cells.cell(pair)?;
        if old != new {
            self.objects.link_cell(object, new, self.cells)?;
            factory.relocation_phase(
                object,
                RelocationPhase::ViewpointGridChanged { old, new },
                self,
            )?;
        }
        self.objects.set_pose(object, pose)?;
        factory.gameobjects().get_mut(object)?.position_bits = pose.0;
        factory.relocation_phase(object, RelocationPhase::GameObjectModelPosition, self)?;
        factory.relocation_phase(object, RelocationPhase::GameObjectVisibility, self)?;
        Ok(GameObjectMoveOutcome::Moved)
    }
}
