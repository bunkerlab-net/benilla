// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored handlers exercise the real active/loader/relocation seam, not gameplay admission.
use super::*;
use benilla_sim::resident_map::CellPair;
use std::collections::{BTreeMap, BTreeSet};
fn map() -> MapIdentity {
    MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    }
}
fn pose(x: f32, y: f32) -> SourcePose {
    SourcePose([x.to_bits(), y.to_bits(), 4_f32.to_bits(), 0_f32.to_bits()])
}
fn pair(x: f32, y: f32) -> CellPair {
    pose(x, y).cell().unwrap()
}
#[derive(Default)]
struct Definitions(BTreeMap<u32, SourcePose>);
impl GridDefinitions for Definitions {
    type Row = SourcePose;
    fn read(&mut self, attempt: GridLoadAttempt) -> Result<Self::Row> {
        self.0.get(&attempt.guid).copied().ok_or_else(|| Refusal {
            id: "authored.definition".into(),
        })
    }
}
#[derive(Default)]
struct Products {
    calls: Vec<ProductCoordinates>,
    fail: bool,
}
impl GridProductOwner for Products {
    fn load(&mut self, _: MapIdentity, coordinates: ProductCoordinates) -> Result<bool> {
        self.calls.push(coordinates);
        if self.fail {
            Err(Refusal {
                id: "authored.products_missing".into(),
            })
        } else {
            Ok(true)
        }
    }
}
#[derive(Default)]
struct Factory {
    loaded: Vec<ObjectHandle>,
    phases: Vec<(ObjectHandle, RelocationPhase, SourcePose)>,
    stopped: BTreeSet<ObjectHandle>,
    motion_cleared: BTreeSet<ObjectHandle>,
    initialized: BTreeSet<ObjectHandle>,
    notified: Vec<ObjectHandle>,
    refuse: Option<RelocationPhase>,
}
impl GridObjectFactory<Definitions, Products> for Factory {
    fn load_from_db(
        &mut self,
        a: GridLoadAttempt,
        row: SourcePose,
        context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<GridCandidateOutcome> {
        let object = context.objects.construct(2, SourceObjectKind::Creature)?;
        context.objects.assign_guid(
            object,
            a.guid,
            SourceGuid::with_entry(HighGuid::Creature, 7, a.guid),
        )?;
        context
            .objects
            .bind_map(object, a.grid.map, context.cells)?;
        context.objects.set_pose(object, row)?;
        context.objects.set_respawn_definition(object, row, true)?;
        context.objects.bind_source_allocation(
            object,
            0x10000 + object.construction * 256,
            128,
            0,
        )?;
        context
            .objects
            .link_cell(object, context.cells.cell(row.cell()?)?, context.cells)?;
        context.objects.add_to_world(object)?;
        self.loaded.push(object);
        Ok(GridCandidateOutcome::Loaded(object))
    }
    fn viewpoint_added(
        &mut self,
        _: ObjectHandle,
        _: CellIdentity,
        _: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        Ok(())
    }
    fn battleground_db_load(
        &mut self,
        _: ObjectHandle,
        _: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        Ok(())
    }
    fn cell_corpses(
        &mut self,
        _: CellPair,
        _: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        Ok(())
    }
    fn resurrectable_corpses(
        &mut self,
        _: CellPair,
        _: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        Ok(())
    }
}
impl ActiveRelocationOwner<Definitions, Products> for Factory {
    fn relocation_phase(
        &mut self,
        object: ObjectHandle,
        phase: RelocationPhase,
        context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        if self.refuse == Some(phase) {
            return Err(phase.missing_owner());
        }
        self.phases
            .push((object, phase, context.objects.pose(object)?));
        match phase {
            RelocationPhase::ViewpointGridChanged { new, .. } => {
                assert_eq!(context.objects.get(object)?.current_cell, Some(new));
                assert_eq!(context.cells.registered_cell(object), Some(new));
                assert_eq!(
                    context.cells.visit(new, CellContainer::Grid)?.next(),
                    Some(object)
                );
            }
            RelocationPhase::CombatStopWithPets => {
                self.stopped.insert(object);
            }
            RelocationPhase::MotionClear => {
                self.motion_cleared.insert(object);
            }
            RelocationPhase::MotionInitialize => {
                self.initialized.insert(object);
            }
            RelocationPhase::CreatureOnRelocated => {
                self.notified.push(object);
            }
            _ => {}
        }
        Ok(())
    }
}
struct World {
    definitions: Definitions,
    products: Products,
    factory: Factory,
    sets: GridSpawnSets,
    objects: SourceObjects,
    cells: SourceCells,
}
impl World {
    fn new(unload: bool) -> Self {
        let mut cells = SourceCells::new(map()).unwrap();
        cells.configure_grids(300000, unload).unwrap();
        Self {
            definitions: Definitions::default(),
            products: Products::default(),
            factory: Factory::default(),
            sets: GridSpawnSets::new(map()),
            objects: SourceObjects::default(),
            cells,
        }
    }
    fn context(&mut self) -> (GridFactoryContext<'_, Definitions, Products>, &mut Factory) {
        (
            GridFactoryContext {
                definitions: &mut self.definitions,
                products: &mut self.products,
                sets: &mut self.sets,
                objects: &mut self.objects,
                cells: &mut self.cells,
            },
            &mut self.factory,
        )
    }
    fn load(&mut self, pose: SourcePose, trigger: GridLoadTrigger) -> GridIdentity {
        let (mut context, factory) = self.context();
        context
            .load_grid(pose.cell().unwrap(), trigger, factory)
            .unwrap()
            .unwrap()
    }
    fn actor(
        &mut self,
        current: SourcePose,
        respawn: SourcePose,
        kind: SourceObjectKind,
        database: u32,
        pointer: u64,
    ) -> ObjectHandle {
        self.load(current, GridLoadTrigger::Enter);
        let object = self.objects.construct(2, kind).unwrap();
        let high = match kind {
            SourceObjectKind::Pet => HighGuid::Pet,
            SourceObjectKind::Player => HighGuid::Player,
            _ => HighGuid::Creature,
        };
        let guid = if kind == SourceObjectKind::Player {
            SourceGuid::without_entry(high, database)
        } else {
            SourceGuid::with_entry(high, 7, database)
        };
        self.objects.assign_guid(object, database, guid).unwrap();
        self.objects.bind_map(object, map(), &self.cells).unwrap();
        self.objects.set_pose(object, current).unwrap();
        self.objects
            .set_respawn_definition(object, respawn, true)
            .unwrap();
        self.objects
            .bind_source_allocation(object, pointer, 128, 0)
            .unwrap();
        self.objects
            .link_cell(
                object,
                self.cells.cell(current.cell().unwrap()).unwrap(),
                &mut self.cells,
            )
            .unwrap();
        self.objects.add_to_world(object).unwrap();
        object
    }
    fn add(&mut self, object: ObjectHandle) -> ActiveChange {
        let (mut context, factory) = self.context();
        context.add_to_active(object, factory).unwrap()
    }
    fn set(&mut self, object: ObjectHandle, active: bool) -> Option<ActiveChange> {
        let (mut context, factory) = self.context();
        context
            .set_active_object_state(object, active, factory)
            .unwrap()
    }
    fn remove(&mut self, object: ObjectHandle) -> ActiveChange {
        let (mut context, _) = self.context();
        context.remove_from_active(object).unwrap()
    }
}
#[test]
fn two_active_owners_share_spawn_grid_count_and_explicit_lock_is_independent() {
    let mut world = World::new(true);
    let spawn = pose(0.0, 0.0);
    let a = world.actor(spawn, spawn, SourceObjectKind::Creature, 40, 1000);
    let b = world.actor(spawn, spawn, SourceObjectKind::Creature, 41, 2000);
    let grid = world.objects.get(a).unwrap().current_cell.unwrap().grid;
    assert_eq!(
        world.add(a).lock,
        ActiveLockOutcome::Changed { grid, count: 1 }
    );
    assert_eq!(
        world.add(b).lock,
        ActiveLockOutcome::Changed { grid, count: 2 }
    );
    assert!(world.cells.grids().unload_locked(grid).unwrap());
    assert!(!world.cells.grids().explicit_lock(grid).unwrap());
    assert_eq!(
        world.remove(a).lock,
        ActiveLockOutcome::Changed { grid, count: 1 }
    );
    assert_eq!(
        world.remove(b).lock,
        ActiveLockOutcome::Changed { grid, count: 0 }
    );
    assert!(!world.cells.grids().unload_locked(grid).unwrap());
    let mut configured = World::new(false);
    let actor = configured.actor(spawn, spawn, SourceObjectKind::Creature, 40, 1000);
    let grid = configured
        .objects
        .get(actor)
        .unwrap()
        .current_cell
        .unwrap()
        .grid;
    configured.add(actor);
    configured.remove(actor);
    assert_eq!(configured.cells.grids().active_lock_count(grid).unwrap(), 0);
    assert!(configured.cells.grids().explicit_lock(grid).unwrap());
    assert!(configured.cells.grids().unload_locked(grid).unwrap());
}
#[test]
fn force_load_only_locks_the_initially_unloaded_branch_and_grid_unload_false_is_separate() {
    let mut world = World::new(true);
    let origin = pose(0.0, 0.0);
    let loaded = world.load(origin, GridLoadTrigger::Enter);
    world.load(origin, GridLoadTrigger::Force);
    assert!(!world.cells.grids().explicit_lock(loaded).unwrap());
    let cold = world.load(pose(600.0, 0.0), GridLoadTrigger::Force);
    assert!(world.cells.grids().explicit_lock(cold).unwrap());
    assert_eq!(world.cells.grids().active_lock_count(cold).unwrap(), 0);
    assert!(world.cells.grids().unload_locked(cold).unwrap());
}
#[test]
fn pet_does_not_lock_respawn_grid_and_database_dynamic_guid_still_does() {
    let mut world = World::new(true);
    let origin = pose(0.0, 0.0);
    let pet = world.actor(origin, origin, SourceObjectKind::Pet, 40, 1000);
    assert_eq!(world.add(pet).lock, ActiveLockOutcome::NotDatabaseCreature);
    let creature = world.actor(origin, origin, SourceObjectKind::Creature, 41, 2000);
    world.objects.remove_from_world(creature).unwrap();
    world
        .objects
        .assign_guid(
            creature,
            41,
            SourceGuid::with_entry(HighGuid::Creature, 7, 100),
        )
        .unwrap();
    world.objects.add_to_world(creature).unwrap();
    let grid = world
        .objects
        .get(creature)
        .unwrap()
        .current_cell
        .unwrap()
        .grid;
    assert_eq!(
        world.add(creature).lock,
        ActiveLockOutcome::Changed { grid, count: 1 }
    );
}
#[test]
fn remote_actor_locks_respawn_grid_not_current_grid_and_missing_spawn_grid_stays_absent() {
    let mut world = World::new(true);
    let origin = pose(0.0, 0.0);
    let remote = pose(600.0, 0.0);
    let respawn_grid = world.load(origin, GridLoadTrigger::TerrainOnly);
    let creature = world.actor(remote, origin, SourceObjectKind::Creature, 40, 1000);
    let current_grid = world
        .objects
        .get(creature)
        .unwrap()
        .current_cell
        .unwrap()
        .grid;
    assert_eq!(
        world.add(creature).lock,
        ActiveLockOutcome::Changed {
            grid: respawn_grid,
            count: 1
        }
    );
    assert_eq!(
        world.cells.grids().active_lock_count(current_grid).unwrap(),
        0
    );
    assert_eq!(
        world.remove(creature).lock,
        ActiveLockOutcome::Changed {
            grid: respawn_grid,
            count: 0
        }
    );
    let missing_pose = pose(1200.0, 0.0);
    let missing_pair = missing_pose.cell().unwrap();
    world
        .objects
        .set_respawn_definition(creature, missing_pose, true)
        .unwrap();
    let missing = ActiveLockOutcome::MissingSpawnGrid {
        x: missing_pair.grid_x(),
        y: missing_pair.grid_y(),
    };
    assert_eq!(world.add(creature).lock, missing);
    assert_eq!(world.remove(creature).lock, missing);
    assert!(world
        .cells
        .grids()
        .identity(missing_pair.grid_x(), missing_pair.grid_y())
        .is_none());
}
#[test]
fn repeated_state_requests_skip_but_direct_map_add_repeats_lock_side_effects() {
    let mut world = World::new(true);
    let origin = pose(0.0, 0.0);
    let creature = world.actor(origin, origin, SourceObjectKind::Creature, 40, 1000);
    let grid = world
        .objects
        .get(creature)
        .unwrap()
        .current_cell
        .unwrap()
        .grid;
    assert!(world.set(creature, true).unwrap().membership_changed);
    assert_eq!(world.set(creature, true), None);
    assert!(!world.add(creature).membership_changed);
    assert_eq!(world.cells.grids().active_lock_count(grid).unwrap(), 2);
    assert!(world.set(creature, false).unwrap().membership_changed);
    assert_eq!(world.set(creature, false), None);
    assert_eq!(world.cells.grids().active_lock_count(grid).unwrap(), 1);
    assert!(!world.remove(creature).membership_changed);
    assert_eq!(world.cells.grids().active_lock_count(grid).unwrap(), 0);
    world.remove(creature);
    assert_eq!(world.cells.grids().active_lock_count(grid).unwrap(), 0);
}
#[test]
fn active_iteration_uses_allocation_order_handles_next_erasure_and_does_not_restart_at_end() {
    let mut world = World::new(true);
    let origin = pose(0.0, 0.0);
    let a = world.actor(origin, origin, SourceObjectKind::Creature, 40, 3000);
    let b = world.actor(origin, origin, SourceObjectKind::Creature, 41, 1000);
    let c = world.actor(origin, origin, SourceObjectKind::Creature, 42, 2000);
    world.add(a);
    world.add(b);
    world.add(c);
    assert_eq!(
        world.cells.active().members().collect::<Vec<_>>(),
        [b, c, a]
    );
    world.cells.active_mut().begin_visit().unwrap();
    assert_eq!(world.cells.active_mut().next_member().unwrap(), Some(b));
    world.remove(c);
    assert_eq!(world.cells.active_mut().next_member().unwrap(), Some(a));
    let d = world.actor(origin, origin, SourceObjectKind::Creature, 43, 4000);
    world.add(d);
    assert_eq!(world.cells.active_mut().next_member().unwrap(), None);
    world.cells.active_mut().finish_visit();
    world.cells.active_mut().begin_visit().unwrap();
    assert_eq!(world.cells.active_mut().next_member().unwrap(), Some(b));
    assert_eq!(world.cells.active_mut().next_member().unwrap(), Some(a));
    assert_eq!(world.cells.active_mut().next_member().unwrap(), Some(d));
    world.cells.active_mut().finish_visit();
}
#[test]
fn inactive_cold_move_refuses_without_loading_and_wrapper_executes_respawn_motion_order() {
    let mut world = World::new(true);
    let origin = pose(0.0, 0.0);
    let creature = world.actor(origin, origin, SourceObjectKind::Creature, 40, 1000);
    let products = world.products.calls.len();
    {
        let (mut context, factory) = world.context();
        assert_eq!(
            context
                .creature_cell_relocation(creature, pair(600.0, 0.0), factory)
                .unwrap(),
            CellRelocation::RefusedUnloaded
        );
    }
    assert_eq!(world.products.calls.len(), products);
    assert_eq!(world.objects.pose(creature).unwrap(), origin);
    {
        let (mut context, factory) = world.context();
        assert_eq!(
            context
                .move_creature(creature, pose(600.0, 0.0), factory)
                .unwrap(),
            CreatureMoveOutcome::Respawned
        );
    }
    let phases: Vec<_> = world
        .factory
        .phases
        .iter()
        .map(|(_, phase, _)| *phase)
        .collect();
    assert_eq!(
        phases,
        [
            RelocationPhase::CombatStopWithPets,
            RelocationPhase::MotionClear,
            RelocationPhase::MotionInitialize,
            RelocationPhase::CreatureOnRelocated
        ]
    );
    assert_eq!(world.objects.pose(creature).unwrap(), origin);
}
#[test]
fn active_cross_grid_move_loads_real_factory_then_registers_before_viewpoint_and_pose_change() {
    let mut world = World::new(true);
    let origin = pose(0.0, 0.0);
    let destination = pose(600.0, 0.0);
    let creature = world.actor(origin, origin, SourceObjectKind::Creature, 40, 1000);
    world.set(creature, true);
    world.definitions.0.insert(99, destination);
    world
        .sets
        .register(
            destination.cell().unwrap(),
            GridSpawnKind::Creature,
            GridSpawnOrigin::Static,
            99,
            99,
        )
        .unwrap();
    {
        let (mut context, factory) = world.context();
        assert_eq!(
            context
                .move_creature(creature, destination, factory)
                .unwrap(),
            CreatureMoveOutcome::Moved
        );
    }
    assert_eq!(world.factory.loaded.len(), 1);
    assert_eq!(world.objects.pose(creature).unwrap(), destination);
    assert!(matches!(
        world.factory.phases[0].1,
        RelocationPhase::ViewpointGridChanged { .. }
    ));
    assert_eq!(world.factory.phases[0].2, origin);
    assert_eq!(
        world.factory.phases[1].1,
        RelocationPhase::CreatureOnRelocated
    );
    assert_eq!(world.factory.phases[1].2, destination);
    let current = world.objects.get(creature).unwrap().current_cell.unwrap();
    assert_eq!(
        world
            .cells
            .visit(current, CellContainer::Grid)
            .unwrap()
            .collect::<Vec<_>>(),
        [creature, world.factory.loaded[0]]
    );
}
#[test]
fn unavailable_relocation_owner_refuses_after_registration_and_before_pose_change() {
    let mut world = World::new(true);
    let origin = pose(0.0, 0.0);
    let creature = world.actor(origin, origin, SourceObjectKind::Creature, 40, 1000);
    let old = world.objects.get(creature).unwrap().current_cell.unwrap();
    let new = world.cells.cell(pair(34.0, 0.0)).unwrap();
    world.factory.refuse = Some(RelocationPhase::ViewpointGridChanged { old, new });
    let (mut context, factory) = world.context();
    assert_eq!(
        context
            .move_creature(creature, pose(34.0, 0.0), factory)
            .unwrap_err()
            .id,
        "execution.relocation.viewpoint_owner"
    );
    assert_eq!(context.cells.registered_cell(creature), Some(new));
    assert_eq!(context.objects.pose(creature).unwrap(), origin);
}
#[test]
fn active_neighbourhood_uses_expanded_cells_including_extra_upper_boundary_and_remote_players() {
    let mut world = World::new(true);
    let origin = pose(34.0, 0.0);
    let creature = world.actor(origin, origin, SourceObjectKind::Creature, 40, 1000);
    world.set(creature, true);
    assert!(world
        .cells
        .active_objects_near_grid(31, 32, 0.0, &world.objects)
        .unwrap());
    assert!(!world
        .cells
        .active_objects_near_grid(30, 32, 0.0, &world.objects)
        .unwrap());
    {
        let (mut context, factory) = world.context();
        assert_eq!(
            context
                .move_creature(creature, pose(100.0, 0.0), factory)
                .unwrap(),
            CreatureMoveOutcome::Moved
        );
    }
    assert!(world
        .cells
        .active_objects_near_grid(31, 32, 50.0, &world.objects)
        .unwrap());
    assert!(!world
        .cells
        .active_objects_near_grid(31, 32, 33.0, &world.objects)
        .unwrap());
    world.remove(creature);
    let player = world.actor(origin, origin, SourceObjectKind::Player, 41, 2000);
    world.objects.link_map_player(player).unwrap();
    assert!(world
        .cells
        .active_objects_near_grid(31, 32, 0.0, &world.objects)
        .unwrap());
    world.objects.unlink_map_player(player).unwrap();
    assert!(!world
        .cells
        .active_objects_near_grid(31, 32, 0.0, &world.objects)
        .unwrap());
    assert_eq!(world.set(player, false), None);
    assert!(world.objects.get(player).unwrap().active);
}
#[test]
fn preload_eligibility_preserves_loader_occurrence_order_and_go_dynamic_branch() {
    let mut catalog = ActivePreloadCatalog::default();
    let row = |kind, guid| ActivePreloadSpawn {
        kind,
        map: 0,
        database_guid: guid,
        pose: pose(0.0, 0.0),
        game_event: 0,
        guid_pool: 0,
        entry_pool: 0,
        dynamic_guid: false,
        template_extra_flags: Some(0x1000),
    };
    catalog
        .register_loaded_spawn(row(GridSpawnKind::GameObject, 9), true)
        .unwrap();
    catalog
        .register_loaded_spawn(row(GridSpawnKind::Creature, 8), true)
        .unwrap();
    catalog
        .register_loaded_spawn(row(GridSpawnKind::Creature, 3), true)
        .unwrap();
    for (guid, event, pool, autoload, flags, dynamic, kind) in [
        (10, 1, 0, true, Some(0x1000), false, GridSpawnKind::Creature),
        (11, 0, 1, true, Some(0x1000), false, GridSpawnKind::Creature),
        (
            12,
            0,
            0,
            false,
            Some(0x1000),
            false,
            GridSpawnKind::Creature,
        ),
        (13, 0, 0, true, None, false, GridSpawnKind::Creature),
        (14, 0, 0, true, Some(0), false, GridSpawnKind::Creature),
        (
            15,
            0,
            0,
            true,
            Some(0x1000),
            true,
            GridSpawnKind::GameObject,
        ),
    ] {
        let mut spawn = row(kind, guid);
        spawn.game_event = event;
        spawn.guid_pool = pool;
        spawn.template_extra_flags = flags;
        spawn.dynamic_guid = dynamic;
        catalog.register_loaded_spawn(spawn, autoload).unwrap();
    }
    let mut dynamic_creature = row(GridSpawnKind::Creature, 16);
    dynamic_creature.dynamic_guid = true;
    catalog
        .register_loaded_spawn(dynamic_creature, true)
        .unwrap();
    assert_eq!(
        catalog
            .normal_order(0)
            .map(|row| row.database_guid)
            .collect::<Vec<_>>(),
        [8, 3, 16, 9]
    );
    let mut world = World::new(true);
    let (mut context, factory) = world.context();
    catalog.load(false, &mut context, factory).unwrap();
    assert_eq!(context.cells.grids().linked().count(), 1);
    assert!(context
        .cells
        .grids()
        .explicit_lock(context.cells.grids().linked().next().unwrap())
        .unwrap());
    assert_eq!(
        catalog.load(true, &mut context, factory).unwrap_err().id,
        "execution.active.forced_creature_data_traversal_owner"
    );
}
#[test]
fn allocation_binding_refuses_overlap_and_missing_identity_never_sorts_by_guid() {
    let mut world = World::new(true);
    let origin = pose(0.0, 0.0);
    let creature = world.actor(origin, origin, SourceObjectKind::Creature, 40, 1000);
    let unbound = world
        .objects
        .construct(2, SourceObjectKind::Creature)
        .unwrap();
    world
        .objects
        .bind_map(unbound, map(), &world.cells)
        .unwrap();
    world.objects.set_pose(unbound, origin).unwrap();
    assert_eq!(
        world
            .objects
            .bind_source_allocation(unbound, 1050, 128, 0)
            .unwrap_err()
            .id,
        "execution.active.allocation_overlap"
    );
    {
        let (mut context, factory) = world.context();
        assert_eq!(
            context.add_to_active(unbound, factory).unwrap_err().id,
            "execution.active.source_allocation_owner"
        );
    }
    world.add(creature);
    world.objects.remove_from_world(creature).unwrap();
    assert_eq!(
        world
            .objects
            .destroy(creature, &mut world.cells)
            .unwrap_err()
            .id,
        "execution.destroy_registered_object"
    );
    world.remove(creature);
    world.objects.destroy(creature, &mut world.cells).unwrap();
    world
        .objects
        .bind_source_allocation(unbound, 1000, 128, 0)
        .unwrap();
}

#[test]
fn cold_respawn_failure_keeps_current_registration_after_stop_and_clear() {
    let mut world = World::new(true);
    let origin = pose(0.0, 0.0);
    let creature = world.actor(
        origin,
        pose(1200.0, 0.0),
        SourceObjectKind::Creature,
        40,
        1000,
    );
    let old = world.objects.get(creature).unwrap().current_cell;
    let (mut context, factory) = world.context();
    assert_eq!(
        context
            .move_creature(creature, pose(600.0, 0.0), factory)
            .unwrap(),
        CreatureMoveOutcome::RefusedColdRespawn
    );
    assert_eq!(context.objects.get(creature).unwrap().current_cell, old);
    assert_eq!(context.objects.pose(creature).unwrap(), origin);
    assert_eq!(
        factory
            .phases
            .iter()
            .map(|(_, phase, _)| *phase)
            .collect::<Vec<_>>(),
        [
            RelocationPhase::CombatStopWithPets,
            RelocationPhase::MotionClear
        ]
    );
}

#[test]
fn active_lock_count_wraps_at_source_uint16_width_and_zero_decrement_stays_zero() {
    let mut world = World::new(true);
    let origin = pose(0.0, 0.0);
    let creature = world.actor(origin, origin, SourceObjectKind::Creature, 40, 1000);
    let grid = world
        .objects
        .get(creature)
        .unwrap()
        .current_cell
        .unwrap()
        .grid;
    for _ in 0..u16::MAX {
        world.add(creature);
    }
    assert_eq!(
        world.cells.grids().active_lock_count(grid).unwrap(),
        u16::MAX
    );
    assert!(!world.add(creature).membership_changed);
    assert_eq!(world.cells.grids().active_lock_count(grid).unwrap(), 0);
    assert!(!world.cells.grids().unload_locked(grid).unwrap());
    world.remove(creature);
    assert_eq!(world.cells.grids().active_lock_count(grid).unwrap(), 0);
}
