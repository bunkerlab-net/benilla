// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors, upstream GPL-2.0-or-later.
//! Natural grid expiry and reentrant teardown, not forced-shutdown acceptance.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
mod terrain;
pub use terrain::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResidencyPending {
    SealedLifecycleInputs,
    PrRemoteUnloadReload,
    F4PersistentRespawnEffects,
    F4WorldClockSnapshot,
    F5ReachedCleanupHandlers,
    F6TerrainGeometryEffects,
}
pub const RESIDENCY_PENDING: [ResidencyPending; 6] = [
    ResidencyPending::SealedLifecycleInputs,
    ResidencyPending::PrRemoteUnloadReload,
    ResidencyPending::F4PersistentRespawnEffects,
    ResidencyPending::F4WorldClockSnapshot,
    ResidencyPending::F5ReachedCleanupHandlers,
    ResidencyPending::F6TerrainGeometryEffects,
];
#[derive(Default)]
pub struct SourceRemovals {
    members: BTreeMap<u64, ObjectHandle>,
    removed_guids: BTreeSet<SourceGuid>,
}
impl SourceRemovals {
    pub fn len(&self) -> usize {
        self.members.len()
    }
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }
    pub fn removed_guids(&self) -> impl Iterator<Item = SourceGuid> + '_ {
        self.removed_guids.iter().copied()
    }
}
impl SourceCells {
    pub fn removals(&self) -> &SourceRemovals {
        &self.removals
    }
    pub(super) fn record_removed_guid(&mut self, guid: SourceGuid) {
        self.removals.removed_guids.insert(guid);
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResidencyPhase {
    CombatStop,
    RemoveDynamicObjects,
    CleanupBeforeDelete,
    RemoveFromWorld,
    VisibilityBeforeUnlink,
    Destructor,
}
impl ResidencyPhase {
    pub fn missing_owner(self) -> Refusal {
        refuse(match self {
            Self::CombatStop => "execution.residency.combat_owner",
            Self::RemoveDynamicObjects => "execution.residency.dynamic_objects_owner",
            Self::CleanupBeforeDelete | Self::Destructor => {
                "execution.residency.entity_cleanup_owner"
            }
            Self::RemoveFromWorld => "execution.residency.entity_remove_world_owner",
            Self::VisibilityBeforeUnlink => "execution.residency.visibility_owner",
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResidencyEvent {
    GridVisit(GridIdentity, NGridPhase),
    Stop(GridIdentity),
    Drain(u8),
    MoveToRespawn(GridIdentity),
    UnloadObjects(GridIdentity),
    DeleteGrid(GridIdentity),
    ReleaseProducts(GridIdentity),
    Queue(ObjectHandle, bool),
    Remove(ObjectHandle),
    MissingCorpse(ObjectHandle),
    IgnoredRemoval(ObjectHandle),
    RemovedGuid(SourceGuid),
    SaveRespawn(ObjectHandle, GridSpawnKind, i64),
    ResetMap(ObjectHandle),
}
/// Runtime entity state, not an observed SaveRespawnTime result.
#[derive(Clone, Copy, Debug)]
pub enum RespawnSaveState {
    Creature {
        respawn_seconds: i64,
        new_spawning_system: bool,
        corpse_expiry_ms: i64,
    },
    GameObject {
        respawn_seconds: i64,
        spawned_by_default: bool,
    },
}
/// F4 owns persistence/clock snapshots; F5 owns reached entity cleanup effects.
/// No successful default callback or persisted result fallback exists.
pub trait ResidencyOwner<D: GridDefinitions, P: ResidencyProductOwner>:
    ActiveRelocationOwner<D, P>
{
    fn residency_observe(&mut self, event: ResidencyEvent) -> Result<()>;
    fn residency_phase(
        &mut self,
        _object: ObjectHandle,
        phase: ResidencyPhase,
        _context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<()> {
        Err(phase.missing_owner())
    }
    fn respawn_save_state(&mut self, _object: ObjectHandle) -> Result<RespawnSaveState> {
        Err(refuse("execution.residency.runtime_respawn_state_owner"))
    }
    fn respawn_wall_time(&mut self, _object: ObjectHandle) -> Result<ClockSample> {
        Err(refuse("execution.residency.fresh_respawn_clock_owner"))
    }
    fn world_clock_milliseconds(&mut self) -> Result<i64> {
        Err(refuse("execution.residency.world_clock_owner"))
    }
    fn save_respawn(
        &mut self,
        _map: MapIdentity,
        _kind: GridSpawnKind,
        _database_guid: u32,
        _deadline: i64,
    ) -> Result<()> {
        Err(refuse("execution.residency.persistent_respawn_owner"))
    }
}
pub trait ResidencyProductOwner: GridProductOwner {
    fn release_grid_products(
        &mut self,
        map: MapIdentity,
        coordinates: ProductCoordinates,
    ) -> Result<()>;
}
#[derive(Clone, Copy, Debug)]
pub struct ResidencyConfig {
    pub visibility: f32,
    pub save_respawn_immediately: bool,
    pub battleground: bool,
}
pub struct SourceResidency {
    map: MapIdentity,
    config: ResidencyConfig,
}
impl SourceResidency {
    pub fn new(map: MapIdentity, config: ResidencyConfig) -> Result<Self> {
        require(
            map.creation_generation > 0
                && config.visibility.is_finite()
                && config.visibility >= 0.0,
            "execution.residency.configuration",
        )?;
        Ok(Self { map, config })
    }
    // Advance the intrusive iterator before visiting, never collapse states; Map.cpp:1013-1019.
    pub fn update<D: GridDefinitions, P: ResidencyProductOwner, F: ResidencyOwner<D, P>>(
        &self,
        diff: u32,
        context: &mut GridFactoryContext<'_, D, P>,
        owner: &mut F,
    ) -> Result<()> {
        require(
            context.cells.map() == self.map,
            "execution.residency.map_generation",
        )?;
        if self.config.battleground {
            return Ok(());
        }
        let mut next = context.cells.grids().linked().next();
        while let Some(grid) = next {
            next = context.cells.grids().next_linked(grid)?;
            let phase = context.cells.grids().phase(grid)?;
            owner.residency_observe(ResidencyEvent::GridVisit(grid, phase))?;
            match phase {
                NGridPhase::Invalid => {}
                NGridPhase::Active => {
                    if context.cells.grids.advance_expiry(grid, diff)? {
                        if context.cells.active_objects_in_grid(grid)? == 0
                            && !context.cells.active_objects_near_grid(
                                grid.x,
                                grid.y,
                                self.config.visibility,
                                context.objects,
                            )?
                        {
                            context.cells.grids.set_phase(grid, NGridPhase::Idle)?;
                        } else {
                            context.cells.grids.reset_expiry(grid, true)?;
                        }
                    }
                }
                NGridPhase::Idle => {
                    context.cells.grids.reset_expiry(grid, false)?;
                    context.cells.grids.set_phase(grid, NGridPhase::Removal)?;
                }
                NGridPhase::Removal => {
                    if !context.cells.grids().unload_locked(grid)?
                        && context.cells.grids.advance_expiry(grid, diff)?
                        && !self.unload_grid(grid, context, owner)?
                    {
                        context.cells.grids.reset_expiry(grid, false)?;
                    }
                }
            }
        }
        Ok(())
    }
    // No force option at the natural-residency seam; Map.cpp:1268-1317.
    pub fn unload_grid<D: GridDefinitions, P: ResidencyProductOwner, F: ResidencyOwner<D, P>>(
        &self,
        grid: GridIdentity,
        context: &mut GridFactoryContext<'_, D, P>,
        owner: &mut F,
    ) -> Result<bool> {
        require(
            context.cells.map() == self.map && grid.map == self.map,
            "execution.residency.map_generation",
        )?;
        context.cells.grids().validate(grid)?;
        if context.cells.active_objects_near_grid(
            grid.x,
            grid.y,
            self.config.visibility,
            context.objects,
        )? {
            return Ok(false);
        }
        owner.residency_observe(ResidencyEvent::Stop(grid))?;
        for cell in grid_cells(grid) {
            let mut next = context.cells.first_in_slot(cell, 1)?;
            while let Some(object) = next {
                owner.residency_phase(object, ResidencyPhase::CombatStop, context)?;
                owner.residency_phase(object, ResidencyPhase::RemoveDynamicObjects, context)?;
                // Stoper advances after the callbacks, unlike the respawn mover.
                next = context.cells.next_in_slot(object, cell, 1)?;
            }
        }
        owner.residency_observe(ResidencyEvent::Drain(1))?;
        self.drain(context, owner)?;
        owner.residency_observe(ResidencyEvent::MoveToRespawn(grid))?;
        for cell in grid_cells(grid) {
            let mut next = context.cells.first_in_slot(cell, 1)?;
            while let Some(object) = next {
                next = context.cells.next_in_slot(object, cell, 1)?;
                let value = context.objects.get(object)?;
                require(
                    value.kind == SourceObjectKind::Creature,
                    "execution.residency.pet_in_grid_creature_list",
                )?;
                let respawn = value
                    .respawn_pose
                    .ok_or_else(|| refuse("execution.residency.respawn_definition_owner"))?
                    .cell()?;
                let current = value
                    .current_cell
                    .ok_or_else(|| refuse("execution.residency.current_cell_owner"))?;
                if (current.grid.x, current.grid.y) != (respawn.grid_x(), respawn.grid_y()) {
                    // The source deliberately ignores false. Cold failures remain for UnloadN.
                    let _ = context.creature_respawn_relocation(object, owner)?;
                }
            }
        }
        owner.residency_observe(ResidencyEvent::Drain(2))?;
        self.drain(context, owner)?;
        owner.residency_observe(ResidencyEvent::UnloadObjects(grid))?;
        for cell in grid_cells(grid) {
            for slot in 0..4 {
                while let Some(object) = context.cells.first_in_slot(cell, slot)? {
                    self.save_respawn(object, context, owner)?;
                    remove_world(object, context, owner)?;
                    destroy(object, context, owner)?;
                }
            }
        }
        owner.residency_observe(ResidencyEvent::Drain(3))?;
        self.drain(context, owner)?;
        let products = context.cells.grids().product_loaded(grid)?;
        owner.residency_observe(ResidencyEvent::DeleteGrid(grid))?;
        context.cells.destroy_unloaded_grid(grid, context.objects)?;
        if products {
            owner.residency_observe(ResidencyEvent::ReleaseProducts(grid))?;
            context
                .products
                .release_grid_products(self.map, grid.product_coordinates())?;
        }
        Ok(true)
    }
    fn save_respawn<D: GridDefinitions, P: ResidencyProductOwner, F: ResidencyOwner<D, P>>(
        &self,
        object: ObjectHandle,
        context: &mut GridFactoryContext<'_, D, P>,
        owner: &mut F,
    ) -> Result<()> {
        if self.config.save_respawn_immediately {
            return Ok(());
        }
        let value = context.objects.get(object)?;
        let kind = match value.kind {
            SourceObjectKind::Creature if value.database_spawn_data => GridSpawnKind::Creature,
            SourceObjectKind::GameObject | SourceObjectKind::Transport => GridSpawnKind::GameObject,
            _ => return Ok(()), // Object::SaveRespawnTime and pets have no persistent write.
        };
        let database = value.database_guid;
        let state = owner.respawn_save_state(object)?;
        let sample = owner.respawn_wall_time(object)?;
        require(
            sample.kind() == ClockKind::WallSeconds,
            "execution.residency.respawn_clock_kind",
        )?;
        let deadline = match (kind, state) {
            (
                GridSpawnKind::Creature,
                RespawnSaveState::Creature {
                    respawn_seconds,
                    new_spawning_system,
                    corpse_expiry_ms,
                },
            ) => {
                if respawn_seconds > sample.value() || new_spawning_system {
                    Some(respawn_seconds)
                } else if owner.world_clock_milliseconds()? < corpse_expiry_ms {
                    // system_clock::to_time_t first converts TimePoint ms to ns; chrono.h:1127-1132.
                    let ns = corpse_expiry_ms
                        .checked_mul(1_000_000)
                        .ok_or_else(|| refuse("execution.residency.corpse_timepoint_overflow"))?;
                    Some(ns / 1_000_000_000)
                } else {
                    None
                }
            }
            (
                GridSpawnKind::GameObject,
                RespawnSaveState::GameObject {
                    respawn_seconds,
                    spawned_by_default,
                },
            ) => {
                (respawn_seconds > sample.value() && spawned_by_default).then_some(respawn_seconds)
            }
            _ => return Err(refuse("execution.residency.respawn_state_kind")),
        };
        if let Some(deadline) = deadline {
            owner.residency_observe(ResidencyEvent::SaveRespawn(object, kind, deadline))?;
            owner.save_respawn(self.map, kind, database, deadline)?;
        }
        Ok(())
    }
    // Erase the pointer-set front before callbacks, then reconsider its live front;
    // newly queued removals participate in this same drain; Map.cpp:1472-1508.
    pub fn drain<D: GridDefinitions, P: ResidencyProductOwner, F: ResidencyOwner<D, P>>(
        &self,
        context: &mut GridFactoryContext<'_, D, P>,
        owner: &mut F,
    ) -> Result<()> {
        require(
            context.cells.map() == self.map,
            "execution.residency.map_generation",
        )?;
        while let Some((pointer, object)) = context.cells.removals.members.pop_first() {
            let value = context.objects.get(object)?;
            require(
                value.source_allocation == Some(pointer),
                "execution.residency.remove_pointer_generation",
            )?;
            match value.kind {
                SourceObjectKind::Corpse | SourceObjectKind::Bones => {
                    let guid = value
                        .guid
                        .ok_or_else(|| refuse("execution.object_guid_absent"))?;
                    if context
                        .objects
                        .lookup_world_object(self.map, guid)
                        .is_none()
                    {
                        owner.residency_observe(ResidencyEvent::MissingCorpse(object))?;
                        continue;
                    }
                }
                SourceObjectKind::Creature
                | SourceObjectKind::Pet
                | SourceObjectKind::GameObject
                | SourceObjectKind::Transport
                | SourceObjectKind::DynamicObject => {}
                _ => {
                    owner.residency_observe(ResidencyEvent::IgnoredRemoval(object))?;
                    continue;
                }
            }
            // Map::Remove's invalid/cold-position branches leave the instance alive.
            let pose = context.objects.pose(object)?;
            let pair = match benilla_sim::resident_map::compute_cell_pair(pose.0[0], pose.0[1]) {
                Ok(pair) => pair,
                Err(benilla_sim::resident_map::Error::OutsideMap) => continue,
                Err(benilla_sim::resident_map::Error::Numeric) => {
                    return Err(refuse("execution.residency.remove_coordinates"))
                }
            };
            if !context
                .cells
                .object_data_loaded(pair.grid_x(), pair.grid_y())
            {
                continue;
            }
            owner.residency_observe(ResidencyEvent::Remove(object))?;
            if context.objects.get(object)?.active {
                context.remove_from_active(object)?;
            }
            cleanup(object, context, owner)?;
            owner.residency_phase(object, ResidencyPhase::VisibilityBeforeUnlink, context)?;
            context.objects.unlink_cell(object, context.cells)?;
            let guid = context
                .objects
                .get(object)?
                .guid
                .ok_or_else(|| refuse("execution.object_guid_absent"))?;
            context.cells.removals.removed_guids.insert(guid);
            owner.residency_observe(ResidencyEvent::RemovedGuid(guid))?;
            self.save_respawn(object, context, owner)?;
            context.objects.reset_map(object)?;
            owner.residency_observe(ResidencyEvent::ResetMap(object))?;
            destroy(object, context, owner)?;
        }
        Ok(())
    }
}
fn grid_cells(grid: GridIdentity) -> impl Iterator<Item = CellIdentity> {
    (grid.x * 16..grid.x * 16 + 16).flat_map(move |x| {
        (grid.y * 16..grid.y * 16 + 16).map(move |y| CellIdentity { grid, x, y })
    })
}
fn remove_world<D: GridDefinitions, P: ResidencyProductOwner, F: ResidencyOwner<D, P>>(
    object: ObjectHandle,
    context: &mut GridFactoryContext<'_, D, P>,
    owner: &mut F,
) -> Result<()> {
    owner.residency_phase(object, ResidencyPhase::RemoveFromWorld, context)?;
    context.objects.remove_from_world(object)
}
fn cleanup<D: GridDefinitions, P: ResidencyProductOwner, F: ResidencyOwner<D, P>>(
    object: ObjectHandle,
    context: &mut GridFactoryContext<'_, D, P>,
    owner: &mut F,
) -> Result<()> {
    owner.residency_phase(object, ResidencyPhase::CleanupBeforeDelete, context)?;
    remove_world(object, context, owner)
}
fn destroy<D: GridDefinitions, P: ResidencyProductOwner, F: ResidencyOwner<D, P>>(
    object: ObjectHandle,
    context: &mut GridFactoryContext<'_, D, P>,
    owner: &mut F,
) -> Result<()> {
    if matches!(
        context.objects.get(object)?.kind,
        SourceObjectKind::Creature | SourceObjectKind::Pet
    ) {
        cleanup(object, context, owner)?; // Creature::~Creature calls CleanupsBeforeDelete again.
    }
    owner.residency_phase(object, ResidencyPhase::Destructor, context)?;
    if context.objects.get(object)?.kind == SourceObjectKind::Bones {
        owner
            .corpse_bindings()?
            .registry
            .retire_deleted_bones(object)?;
    }
    context.objects.destroy(object, context.cells)?;
    Ok(())
}
impl<D: GridDefinitions, P: ResidencyProductOwner> GridFactoryContext<'_, D, P> {
    pub fn add_to_remove<F: ResidencyOwner<D, P>>(
        &mut self,
        object: ObjectHandle,
        owner: &mut F,
    ) -> Result<bool> {
        let value = self.objects.get(object)?;
        require(
            value.map == Some(self.cells.map()),
            "execution.residency.remove_map_generation",
        )?;
        let pointer = value
            .source_allocation
            .ok_or_else(|| refuse("execution.residency.remove_allocation_owner"))?;
        require(
            pointer != 0 && pointer.is_multiple_of(8),
            "execution.residency.remove_allocation_token",
        )?;
        cleanup(object, self, owner)?;
        require(
            self.cells
                .removals
                .members
                .get(&pointer)
                .is_none_or(|old| *old == object),
            "execution.residency.remove_pointer_generation",
        )?;
        let inserted = self
            .cells
            .removals
            .members
            .insert(pointer, object)
            .is_none();
        owner.residency_observe(ResidencyEvent::Queue(object, inserted))?;
        Ok(inserted)
    }
}
