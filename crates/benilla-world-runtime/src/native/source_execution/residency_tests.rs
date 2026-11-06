// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored residency fixtures, not PR or playable-world acceptance.
use super::*;

#[test]
fn expiry_equality_does_not_collapse_active_idle_and_removal_in_one_visit() {
    let mut fixture = Fixture::new(60001, false);
    let grid = fixture.load_empty(0.0, GridLoadTrigger::Enter);
    assert_eq!(fixture.cells.grids().expiry(grid).unwrap(), 6000);
    fixture.update(5999).unwrap();
    assert_eq!(
        fixture.cells.grids().phase(grid).unwrap(),
        NGridPhase::Active
    );
    fixture.update(1).unwrap();
    assert_eq!(fixture.cells.grids().phase(grid).unwrap(), NGridPhase::Idle);
    assert_eq!(fixture.cells.grids().expiry(grid).unwrap(), 0);
    fixture.update(10000).unwrap();
    assert_eq!(
        fixture.cells.grids().phase(grid).unwrap(),
        NGridPhase::Removal
    );
    assert_eq!(fixture.cells.grids().expiry(grid).unwrap(), 60001);
    fixture.update(60000).unwrap();
    assert_eq!(fixture.cells.grids().expiry(grid).unwrap(), 1);
    fixture.update(1).unwrap();
    assert!(fixture.cells.grids().identity(grid.x, grid.y).is_none());
}

use benilla_sim::resident_map::CellPair;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
fn map() -> MapIdentity {
    MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    }
}
fn pose(x: f32) -> SourcePose {
    SourcePose([x.to_bits(), 0, 0, 0])
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Trace {
    Event(ResidencyEvent),
    Phase(ObjectHandle, ResidencyPhase),
    Relocation(ObjectHandle, RelocationPhase),
    ProductLoad(ProductCoordinates),
    ProductRelease(ProductCoordinates),
    Terrain(TerrainTileIdentity, TerrainCleanupPhase),
    Wall(ObjectHandle),
    WorldClock,
    Saved(GridSpawnKind, u32, i64),
}
type Log = Rc<RefCell<Vec<Trace>>>;
struct Definitions;
impl GridDefinitions for Definitions {
    type Row = ();
    fn read(&mut self, _: GridLoadAttempt) -> Result<()> {
        panic!("empty authored spawn sets")
    }
}
struct Products {
    log: Log,
    loaded: BTreeSet<(MapIdentity, u16, u16)>,
}
impl GridProductOwner for Products {
    fn load(&mut self, map: MapIdentity, c: ProductCoordinates) -> Result<bool> {
        assert!(self.loaded.insert((map, c.x, c.y)));
        self.log.borrow_mut().push(Trace::ProductLoad(c));
        Ok(true)
    }
}
impl ResidencyProductOwner for Products {
    fn release_grid_products(&mut self, map: MapIdentity, c: ProductCoordinates) -> Result<()> {
        assert!(self.loaded.remove(&(map, c.x, c.y)));
        self.log.borrow_mut().push(Trace::ProductRelease(c));
        Ok(())
    }
}
struct Host {
    log: Log,
    states: BTreeMap<ObjectHandle, RespawnSaveState>,
    actions: Vec<(ObjectHandle, ResidencyPhase, ObjectHandle)>,
    relocation_actions: Vec<(ObjectHandle, RelocationPhase, ObjectHandle)>,
    refuse: Option<ResidencyPhase>,
    wall: i64,
    world_ms: i64,
}
impl<P: ResidencyProductOwner> GridObjectFactory<Definitions, P> for Host {
    fn load_from_db(
        &mut self,
        _: GridLoadAttempt,
        _: (),
        _: &mut GridFactoryContext<'_, Definitions, P>,
    ) -> Result<GridCandidateOutcome> {
        panic!("empty authored spawn sets")
    }
    fn viewpoint_added(
        &mut self,
        _: ObjectHandle,
        _: CellIdentity,
        _: &mut GridFactoryContext<'_, Definitions, P>,
    ) -> Result<()> {
        Ok(())
    }
    fn battleground_db_load(
        &mut self,
        _: ObjectHandle,
        _: &mut GridFactoryContext<'_, Definitions, P>,
    ) -> Result<()> {
        Ok(())
    }
    fn cell_corpses(
        &mut self,
        _: CellPair,
        _: &mut GridFactoryContext<'_, Definitions, P>,
    ) -> Result<()> {
        Ok(())
    }
    fn resurrectable_corpses(
        &mut self,
        _: CellPair,
        _: &mut GridFactoryContext<'_, Definitions, P>,
    ) -> Result<()> {
        Ok(())
    }
}
impl<P: ResidencyProductOwner> ActiveRelocationOwner<Definitions, P> for Host {
    fn relocation_phase(
        &mut self,
        object: ObjectHandle,
        phase: RelocationPhase,
        context: &mut GridFactoryContext<'_, Definitions, P>,
    ) -> Result<()> {
        self.log.borrow_mut().push(Trace::Relocation(object, phase));
        while let Some(index) = self
            .relocation_actions
            .iter()
            .position(|(o, p, _)| *o == object && *p == phase)
        {
            let (_, _, target) = self.relocation_actions.remove(index);
            context.add_to_remove(target, self)?;
        }
        Ok(())
    }
}
impl<P: ResidencyProductOwner> ResidencyOwner<Definitions, P> for Host {
    fn residency_observe(&mut self, event: ResidencyEvent) -> Result<()> {
        self.log.borrow_mut().push(Trace::Event(event));
        Ok(())
    }
    fn residency_phase(
        &mut self,
        object: ObjectHandle,
        phase: ResidencyPhase,
        context: &mut GridFactoryContext<'_, Definitions, P>,
    ) -> Result<()> {
        self.log.borrow_mut().push(Trace::Phase(object, phase));
        if self.refuse == Some(phase) {
            return Err(phase.missing_owner());
        }
        if phase == ResidencyPhase::VisibilityBeforeUnlink {
            assert!(context.cells.registered_cell(object).is_some());
            assert!(!context.objects.get(object)?.in_world);
        }
        while let Some(index) = self
            .actions
            .iter()
            .position(|(o, p, _)| *o == object && *p == phase)
        {
            let (_, _, target) = self.actions.remove(index);
            context.add_to_remove(target, self)?;
        }
        if phase == ResidencyPhase::Destructor {
            self.states.remove(&object);
        }
        Ok(())
    }
    fn respawn_save_state(&mut self, object: ObjectHandle) -> Result<RespawnSaveState> {
        self.states.get(&object).copied().ok_or_else(|| Refusal {
            id: "authored.respawn_state".into(),
        })
    }
    fn respawn_wall_time(&mut self, object: ObjectHandle) -> Result<ClockSample> {
        self.log.borrow_mut().push(Trace::Wall(object));
        Ok(ClockSample::authored(ClockKind::WallSeconds, self.wall))
    }
    fn world_clock_milliseconds(&mut self) -> Result<i64> {
        self.log.borrow_mut().push(Trace::WorldClock);
        Ok(self.world_ms)
    }
    fn save_respawn(
        &mut self,
        _: MapIdentity,
        kind: GridSpawnKind,
        db: u32,
        deadline: i64,
    ) -> Result<()> {
        self.log.borrow_mut().push(Trace::Saved(kind, db, deadline));
        Ok(())
    }
}
struct Fixture {
    definitions: Definitions,
    products: Products,
    sets: GridSpawnSets,
    objects: SourceObjects,
    cells: SourceCells,
    host: Host,
    driver: SourceResidency,
}
impl Fixture {
    fn new(expiry: u32, immediate: bool) -> Self {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut cells = SourceCells::new(map()).unwrap();
        cells.configure_grids(expiry, true).unwrap();
        Self {
            definitions: Definitions,
            products: Products {
                log: log.clone(),
                loaded: BTreeSet::new(),
            },
            sets: GridSpawnSets::new(map()),
            objects: SourceObjects::default(),
            cells,
            host: Host {
                log,
                states: BTreeMap::new(),
                actions: vec![],
                relocation_actions: vec![],
                refuse: None,
                wall: 100,
                world_ms: 100500,
            },
            driver: SourceResidency::new(
                map(),
                ResidencyConfig {
                    visibility: 0.0,
                    save_respawn_immediately: immediate,
                    battleground: false,
                },
            )
            .unwrap(),
        }
    }
    fn update(&mut self, diff: u32) -> Result<()> {
        self.driver.update(
            diff,
            &mut GridFactoryContext {
                definitions: &mut self.definitions,
                products: &mut self.products,
                sets: &mut self.sets,
                objects: &mut self.objects,
                cells: &mut self.cells,
            },
            &mut self.host,
        )
    }
    fn unload(&mut self, grid: GridIdentity) -> Result<bool> {
        self.driver.unload_grid(
            grid,
            &mut GridFactoryContext {
                definitions: &mut self.definitions,
                products: &mut self.products,
                sets: &mut self.sets,
                objects: &mut self.objects,
                cells: &mut self.cells,
            },
            &mut self.host,
        )
    }
    fn drain(&mut self) -> Result<()> {
        self.driver.drain(
            &mut GridFactoryContext {
                definitions: &mut self.definitions,
                products: &mut self.products,
                sets: &mut self.sets,
                objects: &mut self.objects,
                cells: &mut self.cells,
            },
            &mut self.host,
        )
    }
    fn queue(&mut self, object: ObjectHandle) -> bool {
        GridFactoryContext {
            definitions: &mut self.definitions,
            products: &mut self.products,
            sets: &mut self.sets,
            objects: &mut self.objects,
            cells: &mut self.cells,
        }
        .add_to_remove(object, &mut self.host)
        .unwrap()
    }
    fn load_empty(&mut self, x: f32, trigger: GridLoadTrigger) -> GridIdentity {
        GridFactoryContext {
            definitions: &mut self.definitions,
            products: &mut self.products,
            sets: &mut self.sets,
            objects: &mut self.objects,
            cells: &mut self.cells,
        }
        .load_grid(pose(x).cell().unwrap(), trigger, &mut self.host)
        .unwrap()
        .unwrap()
    }
    fn actor(
        &mut self,
        kind: SourceObjectKind,
        x: f32,
        respawn: f32,
        active: bool,
    ) -> ObjectHandle {
        self.load_empty(x, GridLoadTrigger::Enter);
        let object = self.objects.construct(1, kind).unwrap();
        let high = match kind {
            SourceObjectKind::Player => HighGuid::Player,
            SourceObjectKind::Pet => HighGuid::Pet,
            SourceObjectKind::GameObject | SourceObjectKind::Transport => HighGuid::GameObject,
            SourceObjectKind::DynamicObject => HighGuid::DynamicObject,
            SourceObjectKind::Corpse | SourceObjectKind::Bones => HighGuid::Corpse,
            _ => HighGuid::Creature,
        };
        if kind != SourceObjectKind::Camera {
            self.objects
                .assign_guid(
                    object,
                    object.construction as u32,
                    SourceGuid::with_entry(high, 0, object.construction as u32),
                )
                .unwrap();
        }
        self.objects.bind_map(object, map(), &self.cells).unwrap();
        self.objects
            .bind_source_allocation(object, 0x1000 + object.construction * 256, 128, 0)
            .unwrap();
        self.objects.set_pose(object, pose(x)).unwrap();
        self.objects
            .set_respawn_definition(object, pose(respawn), false)
            .unwrap();
        self.objects.get_mut(object).unwrap().active = active;
        self.objects
            .link_cell(
                object,
                self.cells.cell(pose(x).cell().unwrap()).unwrap(),
                &mut self.cells,
            )
            .unwrap();
        if kind != SourceObjectKind::Camera {
            self.objects.add_to_world(object).unwrap();
        }
        if kind == SourceObjectKind::Player {
            self.objects.link_map_player(object).unwrap();
            self.objects.register_player_lookup(object).unwrap();
        } else if active {
            GridFactoryContext {
                definitions: &mut self.definitions,
                products: &mut self.products,
                sets: &mut self.sets,
                objects: &mut self.objects,
                cells: &mut self.cells,
            }
            .add_to_active(object, &mut self.host)
            .unwrap();
        }
        object
    }
    fn log(&self) -> Vec<Trace> {
        self.host.log.borrow().clone()
    }
    fn phases(&self, object: ObjectHandle) -> Vec<ResidencyPhase> {
        self.log()
            .into_iter()
            .filter_map(|t| {
                if let Trace::Phase(o, p) = t {
                    (o == object).then_some(p)
                } else {
                    None
                }
            })
            .collect()
    }
}

#[test]
fn explicit_lock_freezes_removal_timer_until_its_later_release() {
    let mut f = Fixture::new(60000, true);
    let grid = f.load_empty(0.0, GridLoadTrigger::Force);
    f.update(6000).unwrap();
    f.update(1).unwrap();
    assert_eq!(f.cells.grids().phase(grid).unwrap(), NGridPhase::Removal);
    f.update(100000).unwrap();
    assert_eq!(f.cells.grids().expiry(grid).unwrap(), 60000);
    f.cells.grids.set_explicit_lock(grid, false).unwrap();
    f.update(59999).unwrap();
    assert_eq!(f.cells.grids().expiry(grid).unwrap(), 1);
    f.update(1).unwrap();
    assert!(f.cells.grids().identity(grid.x, grid.y).is_none());
}

#[test]
fn remote_spawn_active_lock_freezes_timer_then_remove_active_releases_it() {
    let mut f = Fixture::new(60000, true);
    let grid = f.load_empty(0.0, GridLoadTrigger::Enter);
    let remote = f.actor(SourceObjectKind::Creature, 3000.0, 0.0, false);
    f.objects
        .set_respawn_definition(remote, pose(0.0), true)
        .unwrap();
    {
        let mut c = GridFactoryContext {
            definitions: &mut f.definitions,
            products: &mut f.products,
            sets: &mut f.sets,
            objects: &mut f.objects,
            cells: &mut f.cells,
        };
        c.set_active_object_state(remote, true, &mut f.host)
            .unwrap();
    }
    assert_eq!(f.cells.grids().active_lock_count(grid).unwrap(), 1);
    f.update(6000).unwrap();
    f.update(1).unwrap();
    f.update(90000).unwrap();
    assert_eq!(f.cells.grids().expiry(grid).unwrap(), 60000);
    {
        let mut c = GridFactoryContext {
            definitions: &mut f.definitions,
            products: &mut f.products,
            sets: &mut f.sets,
            objects: &mut f.objects,
            cells: &mut f.cells,
        };
        c.set_active_object_state(remote, false, &mut f.host)
            .unwrap();
    }
    assert_eq!(f.cells.grids().active_lock_count(grid).unwrap(), 0);
    f.update(60000).unwrap();
    assert!(f.cells.grids().identity(grid.x, grid.y).is_none());
}

#[test]
fn failed_natural_unload_resets_full_expiry_without_changing_removal_state() {
    let mut f = Fixture::new(60000, true);
    let target = f.load_empty(0.0, GridLoadTrigger::Enter);
    f.update(6000).unwrap();
    f.update(1).unwrap();
    // Cell 511 is outside grid 32 but inside its expanded activity bounds.
    f.actor(SourceObjectKind::Player, -20.0, -20.0, true);
    f.update(60000).unwrap();
    assert_eq!(f.cells.grids().phase(target).unwrap(), NGridPhase::Removal);
    assert_eq!(f.cells.grids().expiry(target).unwrap(), 60000);
    assert!(!f
        .log()
        .contains(&Trace::Event(ResidencyEvent::Stop(target))));
}

#[test]
fn linked_grid_iteration_advances_before_deletion_and_keeps_front_order() {
    let mut f = Fixture::new(60000, true);
    let a = f.load_empty(0.0, GridLoadTrigger::TerrainOnly);
    let b = f.load_empty(1000.0, GridLoadTrigger::TerrainOnly);
    let c = f.load_empty(2000.0, GridLoadTrigger::TerrainOnly);
    f.update(0).unwrap(); // IDLE only, no removal on this same visit.
    assert_eq!(f.cells.grids().linked().collect::<Vec<_>>(), [c, b, a]);
    f.host.log.borrow_mut().clear();
    f.update(60000).unwrap();
    let visits: Vec<_> = f
        .log()
        .into_iter()
        .filter_map(|trace| match trace {
            Trace::Event(ResidencyEvent::GridVisit(grid, NGridPhase::Removal)) => Some(grid),
            _ => None,
        })
        .collect();
    assert_eq!(visits, [c, b, a]);
    assert_eq!(f.cells.grids().linked().count(), 0);
}

#[test]
fn all_three_drains_consume_reentrant_removals_and_respawn_move_precedes_unload() {
    let mut f = Fixture::new(60000, true);
    let target = f.load_empty(0.0, GridLoadTrigger::Enter);
    f.load_empty(3000.0, GridLoadTrigger::Enter);
    let mover = f.actor(SourceObjectKind::Creature, 0.0, 3000.0, false);
    let stays = f.actor(SourceObjectKind::Creature, 1.0, 1.0, false);
    let a = f.actor(SourceObjectKind::DynamicObject, 0.0, 0.0, false);
    let b = f.actor(SourceObjectKind::Pet, 0.0, 0.0, false);
    let c = f.actor(SourceObjectKind::Pet, 0.0, 0.0, false);
    f.host
        .actions
        .push((stays, ResidencyPhase::RemoveDynamicObjects, a));
    f.host
        .relocation_actions
        .push((mover, RelocationPhase::MotionClear, b));
    f.host
        .actions
        .push((stays, ResidencyPhase::RemoveFromWorld, c));
    f.host.log.borrow_mut().clear();
    assert!(f.unload(target).unwrap());
    assert_eq!(f.objects.pose(mover).unwrap(), pose(3000.0));
    assert!(f.objects.get(mover).unwrap().in_world);
    assert!(f.objects.get(stays).is_err());
    for object in [a, b, c] {
        assert!(f.objects.get(object).is_err());
    }
    let sequence: Vec<_> = f
        .log()
        .into_iter()
        .filter_map(|trace| {
            if let Trace::Event(event) = trace {
                matches!(
                    event,
                    ResidencyEvent::Stop(_)
                        | ResidencyEvent::Drain(_)
                        | ResidencyEvent::MoveToRespawn(_)
                        | ResidencyEvent::UnloadObjects(_)
                        | ResidencyEvent::Remove(_)
                        | ResidencyEvent::DeleteGrid(_)
                        | ResidencyEvent::ReleaseProducts(_)
                )
                .then_some(event)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        sequence,
        [
            ResidencyEvent::Stop(target),
            ResidencyEvent::Drain(1),
            ResidencyEvent::Remove(a),
            ResidencyEvent::MoveToRespawn(target),
            ResidencyEvent::Drain(2),
            ResidencyEvent::Remove(b),
            ResidencyEvent::UnloadObjects(target),
            ResidencyEvent::Drain(3),
            ResidencyEvent::Remove(c),
            ResidencyEvent::DeleteGrid(target),
            ResidencyEvent::ReleaseProducts(target)
        ]
    );
    assert_eq!(
        f.phases(stays),
        [
            ResidencyPhase::CombatStop,
            ResidencyPhase::RemoveDynamicObjects,
            ResidencyPhase::RemoveFromWorld,
            ResidencyPhase::CleanupBeforeDelete,
            ResidencyPhase::RemoveFromWorld,
            ResidencyPhase::Destructor
        ]
    );
    for object in [b, c] {
        assert_eq!(
            f.phases(object),
            [
                ResidencyPhase::CleanupBeforeDelete,
                ResidencyPhase::RemoveFromWorld,
                ResidencyPhase::CleanupBeforeDelete,
                ResidencyPhase::RemoveFromWorld,
                ResidencyPhase::VisibilityBeforeUnlink,
                ResidencyPhase::CleanupBeforeDelete,
                ResidencyPhase::RemoveFromWorld,
                ResidencyPhase::Destructor
            ]
        );
    }
    let relocations: Vec<_> = f
        .log()
        .into_iter()
        .filter_map(|trace| match trace {
            Trace::Relocation(o, phase) if o == mover => Some(phase),
            _ => None,
        })
        .collect();
    assert!(matches!(
        relocations.as_slice(),
        [
            RelocationPhase::CombatStopWithPets,
            RelocationPhase::MotionClear,
            RelocationPhase::ViewpointGridChanged { .. },
            RelocationPhase::MotionInitialize,
            RelocationPhase::CreatureOnRelocated
        ]
    ));
    assert!(f.cells.removals().is_empty());
}

#[test]
fn cold_respawn_failure_is_ignored_and_remaining_creature_is_unloaded() {
    let mut f = Fixture::new(60000, true);
    let creature = f.actor(SourceObjectKind::Creature, 0.0, 3000.0, false);
    let grid = f.objects.get(creature).unwrap().current_cell.unwrap().grid;
    assert!(f.unload(grid).unwrap());
    assert!(f.objects.get(creature).is_err());
    assert!(!f
        .cells
        .object_data_loaded(pose(3000.0).cell().unwrap().grid_x(), 32));
    let phases: Vec<_> = f
        .log()
        .into_iter()
        .filter_map(|trace| {
            if let Trace::Relocation(o, p) = trace {
                (o == creature).then_some(p)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        phases,
        [
            RelocationPhase::CombatStopWithPets,
            RelocationPhase::MotionClear
        ]
    );
}

#[test]
fn duplicate_queue_attempts_still_cleanup_and_drain_new_lower_pointer_members() {
    let mut f = Fixture::new(60000, true);
    let earlier = f.actor(SourceObjectKind::Creature, 0.0, 0.0, false);
    let later = f.actor(SourceObjectKind::Creature, 0.0, 0.0, false);
    assert!(f.queue(later));
    assert!(!f.queue(later));
    f.host
        .actions
        .push((later, ResidencyPhase::VisibilityBeforeUnlink, earlier));
    f.drain().unwrap();
    assert_eq!(f.cells.removals().len(), 0);
    assert!(f.objects.get(earlier).is_err() && f.objects.get(later).is_err());
    let removes: Vec<_> = f
        .log()
        .into_iter()
        .filter_map(|trace| match trace {
            Trace::Event(ResidencyEvent::Remove(object)) => Some(object),
            _ => None,
        })
        .collect();
    assert_eq!(removes, [later, earlier]);
    assert_eq!(
        f.phases(later),
        [
            ResidencyPhase::CleanupBeforeDelete,
            ResidencyPhase::RemoveFromWorld,
            ResidencyPhase::CleanupBeforeDelete,
            ResidencyPhase::RemoveFromWorld,
            ResidencyPhase::CleanupBeforeDelete,
            ResidencyPhase::RemoveFromWorld,
            ResidencyPhase::VisibilityBeforeUnlink,
            ResidencyPhase::CleanupBeforeDelete,
            ResidencyPhase::RemoveFromWorld,
            ResidencyPhase::Destructor
        ]
    );
}

#[test]
fn captured_grid_activity_is_not_recomputed_from_current_object_flags() {
    let mut f = Fixture::new(60000, true);
    let creature = f.actor(SourceObjectKind::Creature, 0.0, 0.0, true);
    let grid = f.objects.get(creature).unwrap().current_cell.unwrap().grid;
    assert_eq!(f.cells.active_objects_in_grid(grid).unwrap(), 1);
    {
        let mut c = GridFactoryContext {
            definitions: &mut f.definitions,
            products: &mut f.products,
            sets: &mut f.sets,
            objects: &mut f.objects,
            cells: &mut f.cells,
        };
        c.set_active_object_state(creature, false, &mut f.host)
            .unwrap();
    }
    assert_eq!(f.cells.active_objects_in_grid(grid).unwrap(), 1);
    assert!(f.cells.active().is_empty());
    f.update(6000).unwrap();
    assert_eq!(f.cells.grids().phase(grid).unwrap(), NGridPhase::Active);
    assert_eq!(f.cells.grids().expiry(grid).unwrap(), 6000);
}

#[test]
fn grid_activity_keeps_pointer_identity_across_allocation_reuse() {
    let mut f = Fixture::new(60000, true);
    let old = f.actor(SourceObjectKind::Creature, 0.0, 0.0, true);
    let cell = f.objects.get(old).unwrap().current_cell.unwrap();
    let pointer = f.objects.get(old).unwrap().source_allocation.unwrap();
    {
        let mut context = GridFactoryContext {
            definitions: &mut f.definitions,
            products: &mut f.products,
            sets: &mut f.sets,
            objects: &mut f.objects,
            cells: &mut f.cells,
        };
        context
            .set_active_object_state(old, false, &mut f.host)
            .unwrap();
    }
    f.queue(old);
    f.drain().unwrap();
    assert!(f.objects.get(old).is_err());
    assert_eq!(f.cells.active_objects_in_grid(cell.grid).unwrap(), 1);

    let new = f.objects.construct(1, SourceObjectKind::Creature).unwrap();
    f.objects
        .assign_guid(new, 2, SourceGuid::with_entry(HighGuid::Creature, 7, 2))
        .unwrap();
    f.objects.bind_map(new, map(), &f.cells).unwrap();
    f.objects
        .bind_source_allocation(new, pointer, 128, 0)
        .unwrap();
    f.objects.get_mut(new).unwrap().active = true;
    f.objects.link_cell(new, cell, &mut f.cells).unwrap();
    // Grid.h:103-105 inserts void*, so the reused address is a duplicate,
    // despite the fresh native construction identity.
    assert_eq!(f.cells.active_objects_in_grid(cell.grid).unwrap(), 1);
    f.objects.unlink_cell(new, &mut f.cells).unwrap();
    assert_eq!(f.cells.active_objects_in_grid(cell.grid).unwrap(), 0);
}

#[test]
fn save_respawn_uses_fresh_wall_time_and_world_snapshot_only_in_its_source_branch() {
    let cases = [
        (
            SourceObjectKind::Creature,
            true,
            RespawnSaveState::Creature {
                respawn_seconds: 101,
                new_spawning_system: false,
                corpse_expiry_ms: 0,
            },
            Some(101),
            false,
        ),
        (
            SourceObjectKind::Creature,
            true,
            RespawnSaveState::Creature {
                respawn_seconds: 100,
                new_spawning_system: false,
                corpse_expiry_ms: 100501,
            },
            Some(100),
            true,
        ),
        (
            SourceObjectKind::Creature,
            true,
            RespawnSaveState::Creature {
                respawn_seconds: 100,
                new_spawning_system: false,
                corpse_expiry_ms: 100500,
            },
            None,
            true,
        ),
        (
            SourceObjectKind::Creature,
            true,
            RespawnSaveState::Creature {
                respawn_seconds: 0,
                new_spawning_system: true,
                corpse_expiry_ms: 0,
            },
            Some(0),
            false,
        ),
        (
            SourceObjectKind::Creature,
            false,
            RespawnSaveState::Creature {
                respawn_seconds: 101,
                new_spawning_system: false,
                corpse_expiry_ms: 0,
            },
            None,
            false,
        ),
        (
            SourceObjectKind::GameObject,
            false,
            RespawnSaveState::GameObject {
                respawn_seconds: 101,
                spawned_by_default: true,
            },
            Some(101),
            false,
        ),
        (
            SourceObjectKind::GameObject,
            false,
            RespawnSaveState::GameObject {
                respawn_seconds: 100,
                spawned_by_default: true,
            },
            None,
            false,
        ),
        (
            SourceObjectKind::GameObject,
            false,
            RespawnSaveState::GameObject {
                respawn_seconds: 101,
                spawned_by_default: false,
            },
            None,
            false,
        ),
    ];
    for immediate in [false, true] {
        for (kind, database, state, deadline, world_clock) in cases {
            let mut f = Fixture::new(60000, immediate);
            let object = f.actor(kind, 0.0, 0.0, false);
            f.objects
                .set_respawn_definition(object, pose(0.0), database)
                .unwrap();
            f.host.states.insert(object, state);
            let grid = f.objects.get(object).unwrap().current_cell.unwrap().grid;
            assert!(f.unload(grid).unwrap());
            let saves: Vec<_> = f
                .log()
                .into_iter()
                .filter_map(|trace| {
                    if let Trace::Saved(_, db, seconds) = trace {
                        Some((db, seconds))
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(
                saves,
                if immediate {
                    vec![]
                } else {
                    deadline.map(|seconds| (1, seconds)).into_iter().collect()
                }
            );
            assert_eq!(
                f.log()
                    .iter()
                    .filter(|trace| **trace == Trace::WorldClock)
                    .count(),
                usize::from(!immediate && world_clock)
            );
            let wall_calls = !immediate && (kind == SourceObjectKind::GameObject || database);
            assert_eq!(
                f.log()
                    .iter()
                    .filter(|trace| **trace == Trace::Wall(object))
                    .count(),
                usize::from(wall_calls)
            );
            if let Some(index) = f
                .log()
                .iter()
                .position(|trace| matches!(trace, Trace::Saved(..)))
            {
                let log = f.log();
                assert!(
                    index
                        < log
                            .iter()
                            .position(|trace| *trace
                                == Trace::Phase(object, ResidencyPhase::RemoveFromWorld))
                            .unwrap()
                );
            }
        }
    }
}

#[test]
fn missing_reached_cleanup_owner_refuses_before_object_or_product_deletion() {
    let mut f = Fixture::new(60000, true);
    let actor = f.actor(SourceObjectKind::Creature, 0.0, 0.0, false);
    let grid = f.objects.get(actor).unwrap().current_cell.unwrap().grid;
    f.host.refuse = Some(ResidencyPhase::CombatStop);
    assert_eq!(
        f.unload(grid).unwrap_err().id,
        "execution.residency.combat_owner"
    );
    assert!(f.objects.get(actor).unwrap().in_world);
    assert!(f.cells.grids().identity(grid.x, grid.y).is_some());
    assert!(!f
        .log()
        .contains(&Trace::ProductRelease(grid.product_coordinates())));
}

use benilla_sim::source_rng::{DrawCursor, EventKind, Mt19937, SourceRandom, TraceEvent};
struct Geometry(Log);
impl GridProductOwner for Geometry {
    fn load(&mut self, _: MapIdentity, coordinates: ProductCoordinates) -> Result<bool> {
        self.0.borrow_mut().push(Trace::ProductLoad(coordinates));
        Ok(true)
    }
}
impl TerrainResidencyOwner for Geometry {
    fn terrain_cleanup(
        &mut self,
        tile: TerrainTileIdentity,
        phase: TerrainCleanupPhase,
    ) -> Result<()> {
        self.0.borrow_mut().push(Trace::Terrain(tile, phase));
        Ok(())
    }
}
fn terrain() -> (SourceTerrainResidency, Vec<TraceEvent>, DrawCursor) {
    let mut mt = Mt19937::from_seed(5489);
    let mut cursor = DrawCursor::default();
    let mut events = Vec::new();
    let terrain = SourceTerrainResidency::new(
        0,
        1,
        &mut SourceRandom::new(1, &mut mt, &mut cursor, &mut |event| events.push(event)).unwrap(),
    )
    .unwrap();
    (terrain, events, cursor)
}
#[test]
fn terrain_minute_cleanup_has_one_exact_source_draw_and_shared_instance_references() {
    let (mut terrain, events, cursor) = terrain();
    assert_eq!(cursor.raw, 1);
    assert_eq!(
        events
            .iter()
            .filter_map(|event| match event.kind {
                EventKind::Raw { word, .. } => Some(word),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [3499211612]
    );
    assert_eq!(terrain.cleanup_current(), 37000);
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut geometry = Geometry(log.clone());
    let tile = ProductCoordinates { x: 3, y: 4 };
    let second = MapIdentity {
        instance: 1,
        creation_generation: 2,
        ..map()
    };
    {
        let mut products = SourceTerrainProducts {
            terrain: &mut terrain,
            owner: &mut geometry,
        };
        assert!(products.load(map(), tile).unwrap());
        assert!(products.load(map(), tile).unwrap()); // m_bLoadedGrids prevents a second reference.
        assert!(products.load(second, tile).unwrap());
        products.release_grid_products(map(), tile).unwrap();
    }
    assert_eq!(terrain.reference_count(3, 4).unwrap(), 1);
    assert_eq!(log.borrow().as_slice(), [Trace::ProductLoad(tile)]);
    terrain.cleanup(23000, &mut geometry).unwrap();
    assert_eq!(terrain.cleanup_current(), 0);
    assert!(terrain.tile(3, 4).unwrap().is_some());
    let old = terrain.tile(3, 4).unwrap().unwrap();
    SourceTerrainProducts {
        terrain: &mut terrain,
        owner: &mut geometry,
    }
    .release_grid_products(second, tile)
    .unwrap();
    assert_eq!(terrain.reference_count(3, 4).unwrap(), 0);
    assert!(!terrain.load_attempted(3, 4).unwrap());
    terrain.cleanup(59999, &mut geometry).unwrap();
    assert!(terrain.tile(3, 4).unwrap().is_some());
    terrain.cleanup(1, &mut geometry).unwrap();
    assert_eq!(terrain.tile(3, 4).unwrap(), None);
    assert_eq!(
        &log.borrow()[1..],
        [
            Trace::Terrain(old, TerrainCleanupPhase::UnloadData),
            Trace::Terrain(old, TerrainCleanupPhase::DeleteGridMap),
            Trace::Terrain(old, TerrainCleanupPhase::UnloadVMap),
            Trace::Terrain(old, TerrainCleanupPhase::UnloadMMap)
        ]
    );
    SourceTerrainProducts {
        terrain: &mut terrain,
        owner: &mut geometry,
    }
    .load(map(), tile)
    .unwrap();
    assert!(terrain.tile(3, 4).unwrap().unwrap().generation > old.generation);
    assert_eq!(terrain.reference_count(3, 4).unwrap(), 1);
    terrain.cleanup(180000, &mut geometry).unwrap();
    assert_eq!(terrain.cleanup_current(), 120000); // One Reset, not a while-passed loop.
}

#[test]
fn terrain_cleanup_visits_y_outside_x_and_releases_geometry_in_source_order() {
    let (mut terrain, _, _) = terrain();
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut geometry = Geometry(log.clone());
    for (x, y) in [(0, 1), (1, 0)] {
        let c = ProductCoordinates { x, y };
        let mut products = SourceTerrainProducts {
            terrain: &mut terrain,
            owner: &mut geometry,
        };
        products.load(map(), c).unwrap();
        products.release_grid_products(map(), c).unwrap();
    }
    let first = terrain.tile(1, 0).unwrap().unwrap();
    let second = terrain.tile(0, 1).unwrap().unwrap();
    log.borrow_mut().clear();
    terrain.cleanup(23000, &mut geometry).unwrap();
    assert_eq!(
        log.borrow().as_slice(),
        [
            Trace::Terrain(first, TerrainCleanupPhase::UnloadData),
            Trace::Terrain(first, TerrainCleanupPhase::DeleteGridMap),
            Trace::Terrain(first, TerrainCleanupPhase::UnloadVMap),
            Trace::Terrain(first, TerrainCleanupPhase::UnloadMMap),
            Trace::Terrain(second, TerrainCleanupPhase::UnloadData),
            Trace::Terrain(second, TerrainCleanupPhase::DeleteGridMap),
            Trace::Terrain(second, TerrainCleanupPhase::UnloadVMap),
            Trace::Terrain(second, TerrainCleanupPhase::UnloadMMap)
        ]
    );
}

#[test]
fn object_unload_deletes_registration_before_later_independent_terrain_cleanup() {
    let mut f = Fixture::new(60000, true);
    let (mut terrain, _, _) = terrain();
    let mut geometry = Geometry(f.host.log.clone());
    let mut products = SourceTerrainProducts {
        terrain: &mut terrain,
        owner: &mut geometry,
    };
    let mut cells = SourceCells::new(map()).unwrap();
    cells.configure_grids(60000, true).unwrap();
    let grid;
    let object = f
        .objects
        .construct(1, SourceObjectKind::DynamicObject)
        .unwrap();
    {
        let mut context = GridFactoryContext {
            definitions: &mut f.definitions,
            products: &mut products,
            sets: &mut f.sets,
            objects: &mut f.objects,
            cells: &mut cells,
        };
        grid = context
            .load_grid(
                pose(0.0).cell().unwrap(),
                GridLoadTrigger::Enter,
                &mut f.host,
            )
            .unwrap()
            .unwrap();
        context
            .objects
            .assign_guid(
                object,
                0,
                SourceGuid::without_entry(HighGuid::DynamicObject, 1),
            )
            .unwrap();
        context
            .objects
            .bind_map(object, map(), context.cells)
            .unwrap();
        context
            .objects
            .bind_source_allocation(object, 4096, 128, 0)
            .unwrap();
        context.objects.set_pose(object, pose(0.0)).unwrap();
        context
            .objects
            .link_cell(
                object,
                context.cells.cell(pose(0.0).cell().unwrap()).unwrap(),
                context.cells,
            )
            .unwrap();
        context.objects.add_to_world(object).unwrap();
        f.driver.update(6000, &mut context, &mut f.host).unwrap();
        f.driver.update(1, &mut context, &mut f.host).unwrap();
        f.driver.update(60000, &mut context, &mut f.host).unwrap();
    }
    assert!(f.objects.get(object).is_err());
    assert!(cells.grids().identity(grid.x, grid.y).is_none());
    let c = grid.product_coordinates();
    assert_eq!(products.terrain.reference_count(c.x, c.y).unwrap(), 0);
    assert!(products.terrain.tile(c.x, c.y).unwrap().is_some());
    assert!(!f
        .log()
        .iter()
        .any(|trace| matches!(trace, Trace::Terrain(..))));
    products.terrain.cleanup(23000, products.owner).unwrap();
    let log = f.log();
    let deleted = log
        .iter()
        .position(|trace| *trace == Trace::Event(ResidencyEvent::DeleteGrid(grid)))
        .unwrap();
    let cleanup = log
        .iter()
        .position(|trace| matches!(trace, Trace::Terrain(_, TerrainCleanupPhase::UnloadData)))
        .unwrap();
    assert!(deleted < cleanup);
    assert_eq!(products.terrain.tile(c.x, c.y).unwrap(), None);
}

#[test]
fn shortened_expiry_promotes_the_binary32_factor_before_multiplication() {
    let mut f = Fixture::new(u32::MAX, true);
    let grid = f.load_empty(0.0, GridLoadTrigger::Enter);
    assert_eq!(f.cells.grids().expiry(grid).unwrap(), 429496735);
}

#[test]
fn grid_unload_false_and_battleground_keep_their_distinct_source_guards() {
    let mut f = Fixture::new(60000, true);
    f.cells = SourceCells::new(map()).unwrap();
    f.cells.configure_grids(60000, false).unwrap();
    let grid = f.load_empty(0.0, GridLoadTrigger::Enter);
    f.update(6000).unwrap();
    f.update(0).unwrap();
    f.update(100000).unwrap();
    assert_eq!(f.cells.grids().phase(grid).unwrap(), NGridPhase::Removal);
    assert_eq!(f.cells.grids().expiry(grid).unwrap(), 60000);
    let mut battle = Fixture::new(60000, true);
    battle.driver = SourceResidency::new(
        map(),
        ResidencyConfig {
            visibility: 0.0,
            save_respawn_immediately: true,
            battleground: true,
        },
    )
    .unwrap();
    let grid = battle.load_empty(0.0, GridLoadTrigger::Enter);
    battle.update(100000).unwrap();
    assert_eq!(
        battle.cells.grids().phase(grid).unwrap(),
        NGridPhase::Active
    );
    assert_eq!(battle.cells.grids().expiry(grid).unwrap(), 6000);
}

#[test]
fn corpse_timepoint_seconds_truncate_negative_values_and_reject_source_overflow() {
    let mut f = Fixture::new(60000, false);
    let object = f.actor(SourceObjectKind::Creature, 0.0, 0.0, false);
    f.objects
        .set_respawn_definition(object, pose(0.0), true)
        .unwrap();
    f.host.wall = -2;
    f.host.world_ms = -1501;
    f.host.states.insert(
        object,
        RespawnSaveState::Creature {
            respawn_seconds: -3,
            new_spawning_system: false,
            corpse_expiry_ms: -1500,
        },
    );
    let grid = f.objects.get(object).unwrap().current_cell.unwrap().grid;
    assert!(f.unload(grid).unwrap());
    assert!(f
        .log()
        .contains(&Trace::Saved(GridSpawnKind::Creature, 1, -1)));
    let mut overflow = Fixture::new(60000, false);
    let object = overflow.actor(SourceObjectKind::Creature, 0.0, 0.0, false);
    overflow
        .objects
        .set_respawn_definition(object, pose(0.0), true)
        .unwrap();
    overflow.host.states.insert(
        object,
        RespawnSaveState::Creature {
            respawn_seconds: 0,
            new_spawning_system: false,
            corpse_expiry_ms: i64::MAX,
        },
    );
    let grid = overflow
        .objects
        .get(object)
        .unwrap()
        .current_cell
        .unwrap()
        .grid;
    assert_eq!(
        overflow.unload(grid).unwrap_err().id,
        "execution.residency.corpse_timepoint_overflow"
    );
    assert!(overflow.objects.get(object).unwrap().in_world);
}

#[test]
fn map_remove_skips_a_missing_corpse_and_grid_destruction_only_unlinks_world_references() {
    let mut f = Fixture::new(60000, true);
    let corpse = f.actor(SourceObjectKind::Corpse, 0.0, 0.0, false);
    let grid = f.cells.registered_cell(corpse).unwrap().grid;
    assert!(f.queue(corpse));
    f.drain().unwrap();
    assert!(f
        .log()
        .contains(&Trace::Event(ResidencyEvent::MissingCorpse(corpse))));
    assert!(f.objects.get(corpse).is_ok());
    assert!(f.cells.registered_cell(corpse).is_some());
    assert!(f.unload(grid).unwrap());
    assert!(f.objects.get(corpse).is_ok());
    assert!(f.cells.registered_cell(corpse).is_none());
}

#[test]
fn stale_queued_generation_refuses_instead_of_removing_a_reused_guid() {
    let mut f = Fixture::new(60000, true);
    let object = f.actor(SourceObjectKind::Creature, 0.0, 0.0, false);
    let guid = f.objects.get(object).unwrap().guid.unwrap();
    f.queue(object);
    f.objects.destroy(object, &mut f.cells).unwrap();
    let replacement = f.objects.construct(1, SourceObjectKind::Creature).unwrap();
    f.objects.assign_guid(replacement, 1, guid).unwrap();
    f.objects.bind_map(replacement, map(), &f.cells).unwrap();
    f.objects
        .bind_source_allocation(replacement, 0x1000 + object.construction * 256, 128, 0)
        .unwrap();
    f.objects.set_pose(replacement, pose(0.0)).unwrap();
    f.objects
        .link_cell(
            replacement,
            f.cells.cell(pose(0.0).cell().unwrap()).unwrap(),
            &mut f.cells,
        )
        .unwrap();
    f.objects.add_to_world(replacement).unwrap();
    assert_eq!(f.drain().unwrap_err().id, "execution.stale_object_handle");
    assert!(f.objects.get(replacement).unwrap().in_world);
}

#[test]
fn independent_get_grid_tiles_preserve_half_loads_without_acquiring_map_references() {
    struct Queries {
        log: Log,
        calls: Vec<(ProductCoordinates, bool)>,
    }
    impl GridProductOwner for Queries {
        fn load(&mut self, _: MapIdentity, c: ProductCoordinates) -> Result<bool> {
            self.log.borrow_mut().push(Trace::ProductLoad(c));
            Ok(true)
        }
    }
    impl TerrainResidencyOwner for Queries {
        fn terrain_cleanup(
            &mut self,
            tile: TerrainTileIdentity,
            phase: TerrainCleanupPhase,
        ) -> Result<()> {
            self.log.borrow_mut().push(Trace::Terrain(tile, phase));
            Ok(())
        }
        fn load_query_tile(
            &mut self,
            _: MapIdentity,
            c: ProductCoordinates,
            map_only: bool,
        ) -> Result<TerrainQueryLoad> {
            self.calls.push((c, map_only));
            Ok(TerrainQueryLoad {
                present: true,
                fully_loaded: !map_only,
            })
        }
    }
    let (mut terrain, _, _) = terrain();
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut owner = Queries {
        log: log.clone(),
        calls: vec![],
    };
    let tile = terrain
        .query_at(map(), [0, 0], true, &mut owner)
        .unwrap()
        .unwrap();
    assert_eq!((tile.x, tile.y), (32, 32)); // GetGrid is not the NGrid 63-coordinate conversion.
    assert_eq!(terrain.reference_count(32, 32).unwrap(), 0);
    let coordinates = ProductCoordinates { x: 32, y: 32 };
    {
        let mut products = SourceTerrainProducts {
            terrain: &mut terrain,
            owner: &mut owner,
        };
        products.load(map(), coordinates).unwrap(); // Cached GridMap skips LoadMapAndVMap even if half loaded.
        products.release_grid_products(map(), coordinates).unwrap();
    }
    assert!(log.borrow().is_empty());
    assert_eq!(
        terrain.query_at(map(), [0, 0], true, &mut owner).unwrap(),
        Some(tile)
    );
    assert_eq!(
        terrain.query_at(map(), [0, 0], false, &mut owner).unwrap(),
        Some(tile)
    );
    assert_eq!(owner.calls, [(coordinates, true), (coordinates, false)]);
    terrain.cleanup(23000, &mut owner).unwrap();
    assert_eq!(terrain.tile(32, 32).unwrap(), None);
    assert_eq!(
        log.borrow().as_slice(),
        [
            Trace::Terrain(tile, TerrainCleanupPhase::UnloadData),
            Trace::Terrain(tile, TerrainCleanupPhase::DeleteGridMap),
            Trace::Terrain(tile, TerrainCleanupPhase::UnloadVMap),
            Trace::Terrain(tile, TerrainCleanupPhase::UnloadMMap)
        ]
    );
}

#[test]
fn missing_query_geometry_owner_refuses_instead_of_claiming_a_loaded_terrain_tile() {
    let (mut terrain, _, _) = terrain();
    let mut owner = Geometry(Rc::new(RefCell::new(Vec::new())));
    assert_eq!(
        terrain
            .query_at(map(), [0, 0], false, &mut owner)
            .unwrap_err()
            .id,
        "execution.residency.terrain_query_geometry_owner"
    );
    assert_eq!(terrain.tile(32, 32).unwrap(), None);
}
