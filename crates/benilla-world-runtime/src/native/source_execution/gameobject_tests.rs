// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored executable dependencies test constructor chronology, not F6 geometry.
use super::*;
use benilla_sim::resident_map::{compute_cell_pair, CellPair};
use benilla_sim::source_rng::{DrawCursor, Mt19937, SourceRandom};
use std::collections::BTreeMap;
use std::sync::Arc;

fn map() -> MapIdentity {
    MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    }
}
fn template(entry: u32, kind: u32) -> GameObjectTemplate {
    GameObjectTemplate {
        entry,
        kind,
        display: 123,
        faction: 35,
        flags: 0,
        extra_flags: 0,
        scale_bits: 1.0_f32.to_bits(),
        data: [0; 24],
        string_id: 0,
    }
}
fn spawn() -> GameObjectSpawn {
    GameObjectSpawn {
        database_guid: 7,
        entry: 100,
        position_bits: [0; 4],
        rotation_bits: [0, 0, 0, 1.0_f32.to_bits()],
        animation: 100,
        state: -1,
        respawn_min: 25,
        respawn_max: 25,
    }
}
struct Definitions {
    rows: BTreeMap<u32, GameObjectSpawn>,
}
impl GridDefinitions for Definitions {
    type Row = GameObjectSpawn;
    fn read(&mut self, attempt: GridLoadAttempt) -> Result<Self::Row> {
        self.rows
            .get(&attempt.guid)
            .cloned()
            .ok_or_else(|| Refusal {
                id: "authored.spawn_absent".into(),
            })
    }
}
struct Products;
impl GridProductOwner for Products {
    fn load(&mut self, _: MapIdentity, _: ProductCoordinates) -> Result<bool> {
        Ok(true)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Action {
    Model(ObjectHandle, GameObjectModelOperation),
    Callback(ObjectHandle, GameObjectCallback),
    RespawnRead,
    Calendar,
    Save(u32, i64),
    Irand(i32, i32),
    Urand(u32, u32),
    LoaderViewpoint(ObjectHandle),
}
struct Factory {
    objects: SourceGameObjects,
    guids: MapGuidAllocators,
    templates: BTreeMap<u32, GameObjectTemplate>,
    events: bool,
    forced: bool,
    group: Option<GameObjectGroupEntry>,
    entries: Option<Arc<[u32]>>,
    saved: i64,
    now: i64,
    engine: Mt19937,
    cursor: DrawCursor,
    actions: Vec<Action>,
    models: BTreeMap<ObjectHandle, bool>,
    collision_available: bool,
    recurse: Option<GameObjectSpawn>,
}
impl Factory {
    fn new() -> Self {
        let mut globals = GlobalGuidAllocators::from_seed_rows(GuidSeedRows {
            player: GuidMaximum::NoResult,
            item: GuidMaximum::NoResult,
            corpse: GuidMaximum::NoResult,
            creature: GuidMaximum::Value(99),
            gameobject: GuidMaximum::Value(199),
            creature_reserve: 2,
            gameobject_reserve: 3,
        });
        Self {
            objects: SourceGameObjects::default(),
            guids: globals.map_allocators(map()).unwrap(),
            templates: BTreeMap::from([(100, template(100, 5))]),
            events: false,
            forced: false,
            group: None,
            entries: None,
            saved: 0,
            now: 100,
            engine: Mt19937::from_seed_seq(&[1, 2]),
            cursor: DrawCursor::default(),
            actions: Vec::new(),
            models: BTreeMap::new(),
            collision_available: true,
            recurse: None,
        }
    }
}
impl GridObjectFactory<Definitions, Products> for Factory {
    fn load_from_db(
        &mut self,
        attempt: GridLoadAttempt,
        row: GameObjectSpawn,
        context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<GridCandidateOutcome> {
        context.load_gameobject(self, attempt, row)
    }
    fn viewpoint_added(
        &mut self,
        object: ObjectHandle,
        _: CellIdentity,
        _: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        self.actions.push(Action::LoaderViewpoint(object));
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
impl GameObjectFactoryOwner<Definitions, Products> for Factory {
    fn gameobjects(&mut self) -> &mut SourceGameObjects {
        &mut self.objects
    }
    fn gameobject_guids(&mut self) -> &mut MapGuidAllocators {
        &mut self.guids
    }
    fn gameobject_spawn(
        &mut self,
        _: GridLoadAttempt,
        row: GameObjectSpawn,
    ) -> Result<GameObjectSpawn> {
        Ok(row)
    }
    fn gameobject_template(&mut self, entry: u32) -> Result<Option<GameObjectTemplate>> {
        Ok(self.templates.get(&entry).cloned())
    }
    fn gameobject_thread(&self) -> u64 {
        1
    }
    fn gameobject_event_guid(&mut self, _: u32) -> Result<bool> {
        Ok(self.events)
    }
    fn gameobject_force_dynamic(&mut self) -> Result<bool> {
        Ok(self.forced)
    }
    fn gameobject_group(&mut self, _: u32) -> Result<Option<GameObjectGroupEntry>> {
        Ok(self.group)
    }
    fn gameobject_group_entry(&mut self, _: u32, _: u32) -> Result<u32> {
        Ok(101)
    }
    fn gameobject_random_entries(&mut self, _: u32) -> Result<Option<Arc<[u32]>>> {
        Ok(self.entries.clone())
    }
    fn gameobject_irand(&mut self, min: i32, max: i32) -> Result<i32> {
        self.actions.push(Action::Irand(min, max));
        SourceRandom::new(1, &mut self.engine, &mut self.cursor, &mut |_| {})
            .unwrap()
            .irand(min, max)
            .map_err(|_| Refusal {
                id: "authored.random".into(),
            })
    }
    fn gameobject_urand(&mut self, min: u32, max: u32) -> Result<u32> {
        self.actions.push(Action::Urand(min, max));
        SourceRandom::new(1, &mut self.engine, &mut self.cursor, &mut |_| {})
            .unwrap()
            .urand(min, max)
            .map_err(|_| Refusal {
                id: "authored.random".into(),
            })
    }
    fn gameobject_saved_respawn(&mut self, _: u32) -> Result<i64> {
        self.actions.push(Action::RespawnRead);
        Ok(self.saved)
    }
    fn gameobject_save_respawn(&mut self, guid: u32, time: i64) -> Result<()> {
        self.actions.push(Action::Save(guid, time));
        self.saved = time;
        Ok(())
    }
    fn gameobject_calendar(&mut self) -> Result<i64> {
        self.actions.push(Action::Calendar);
        Ok(self.now)
    }
    fn gameobject_rotation(&mut self, orientation: u32) -> Result<[u32; 2]> {
        if f32::from_bits(orientation) == 0.0 {
            Ok([orientation, 1.0_f32.to_bits()])
        } else {
            Err(Refusal {
                id: "execution.gameobject_rotation_owner_unavailable".into(),
            })
        }
    }
    fn gameobject_model(
        &mut self,
        handle: ObjectHandle,
        op: GameObjectModelOperation,
        context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<bool> {
        if !self.collision_available {
            return Err(Refusal {
                id: "execution.dynamic_collision_owner_unavailable".into(),
            });
        }
        self.actions.push(Action::Model(handle, op));
        match op {
            GameObjectModelOperation::ConstructAndInsert => {
                assert!(!context.objects.get(handle)?.in_world);
                self.models.insert(handle, false);
            }
            GameObjectModelOperation::Insert => {
                assert!(!context.objects.get(handle)?.in_world);
                assert_eq!(
                    context.objects.lookup_database(
                        map(),
                        DatabaseLookupKind::GameObject,
                        context.objects.get(handle)?.database_guid
                    ),
                    Some(handle)
                );
                assert!(context.cells.registered_cell(handle).is_some());
                assert!(self.models.contains_key(&handle));
            }
            GameObjectModelOperation::Enable(enabled) => {
                assert!(context.objects.get(handle)?.in_world);
                *self.models.get_mut(&handle).unwrap() = enabled;
            }
            GameObjectModelOperation::Remove | GameObjectModelOperation::Destroy => {
                self.models.remove(&handle);
            }
        }
        Ok(true)
    }
    fn gameobject_callback(
        &mut self,
        handle: ObjectHandle,
        callback: GameObjectCallback,
        context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        self.actions.push(Action::Callback(handle, callback));
        if callback == GameObjectCallback::InitializeAi {
            assert!(context.objects.get(handle)?.in_world);
            assert!(context.cells.registered_cell(handle).is_some());
            if let Some(spawn) = self.recurse.take() {
                let grid = context.cells.registered_cell(handle).unwrap().grid;
                let cell =
                    compute_cell_pair(spawn.position_bits[0], spawn.position_bits[1]).unwrap();
                let attempt = GridLoadAttempt {
                    grid,
                    cell,
                    kind: GridSpawnKind::GameObject,
                    origin: GridSpawnOrigin::Static,
                    guid: spawn.database_guid,
                    catalog_ordinal: 1,
                };
                context.load_gameobject(self, attempt, spawn)?;
            }
        }
        Ok(())
    }
}
struct World {
    definitions: Definitions,
    products: Products,
    sets: GridSpawnSets,
    objects: SourceObjects,
    cells: SourceCells,
}
impl World {
    fn new() -> Self {
        let mut cells = SourceCells::new(map()).unwrap();
        cells.configure_grids(300000, true).unwrap();
        Self {
            definitions: Definitions {
                rows: BTreeMap::new(),
            },
            products: Products,
            sets: GridSpawnSets::new(map()),
            objects: SourceObjects::default(),
            cells,
        }
    }
    fn context(&mut self) -> GridFactoryContext<'_, Definitions, Products> {
        GridFactoryContext {
            definitions: &mut self.definitions,
            products: &mut self.products,
            sets: &mut self.sets,
            objects: &mut self.objects,
            cells: &mut self.cells,
        }
    }
    fn load(
        &mut self,
        factory: &mut Factory,
        spawn: GameObjectSpawn,
    ) -> Result<GridCandidateOutcome> {
        let cell = compute_cell_pair(spawn.position_bits[0], spawn.position_bits[1]).unwrap();
        let grid = self
            .context()
            .load_grid(cell, GridLoadTrigger::TerrainOnly, factory)?
            .unwrap();
        let attempt = GridLoadAttempt {
            grid,
            cell,
            kind: GridSpawnKind::GameObject,
            origin: GridSpawnOrigin::Static,
            guid: spawn.database_guid,
            catalog_ordinal: 0,
        };
        self.context().load_gameobject(factory, attempt, spawn)
    }
}
fn loaded(outcome: GridCandidateOutcome) -> ObjectHandle {
    match outcome {
        GridCandidateOutcome::Loaded(handle) => handle,
        _ => panic!("expected loaded candidate"),
    }
}

#[test]
fn go_duplicate_is_destroyed_without_draws_or_callbacks() {
    let (mut world, mut factory) = (World::new(), Factory::new());
    let first = loaded(world.load(&mut factory, spawn()).unwrap());
    let (draws, actions) = (factory.cursor.raw, factory.actions.len());
    let duplicate = world.load(&mut factory, spawn()).unwrap();
    assert_eq!(
        duplicate,
        GridCandidateOutcome::FailedAndDestroyed(ObjectHandle {
            thread: 1,
            construction: 2
        })
    );
    assert_eq!(
        (factory.cursor.raw, factory.actions.len()),
        (draws, actions)
    );
    assert_eq!(
        world
            .objects
            .lookup_database(map(), DatabaseLookupKind::GameObject, 7),
        Some(first)
    );
}
#[test]
fn random_entry_overrides_group_and_database_and_draws_for_singleton() {
    let (mut world, mut factory) = (World::new(), Factory::new());
    factory.templates.insert(101, template(101, 5));
    factory.templates.insert(102, template(102, 5));
    factory.group = Some(GameObjectGroupEntry {
        id: 9,
        string_id: 17,
    });
    factory.entries = Some(Arc::from([102]));
    let mut row = spawn();
    row.entry = 0;
    let handle = loaded(world.load(&mut factory, row).unwrap());
    assert_eq!(factory.objects.get(handle).unwrap().template.entry, 102);
    assert_eq!(
        world.objects.get(handle).unwrap().guid.unwrap().counter(),
        203
    );
    assert_eq!(factory.cursor.raw, 2);
    assert!(factory.actions.contains(&Action::Irand(0, 0)));
    assert!(factory.actions.contains(&Action::Callback(
        handle,
        GameObjectCallback::GroupString(17)
    )));
}
#[test]
fn zero_random_entry_retains_database_entry() {
    let (mut world, mut factory) = (World::new(), Factory::new());
    factory.entries = Some(Arc::from([0]));
    let handle = loaded(world.load(&mut factory, spawn()).unwrap());
    assert_eq!(factory.objects.get(handle).unwrap().template.entry, 100);
    assert_eq!(factory.cursor.raw, 2);
}
#[test]
fn static_dynamic_and_event_guids_use_source_allocator_branches() {
    for mode in 0..4 {
        let (mut world, mut factory) = (World::new(), Factory::new());
        factory.events = mode == 1;
        factory.forced = mode == 2;
        if mode == 3 {
            factory.templates.get_mut(&100).unwrap().extra_flags = 2;
        }
        let handle = loaded(world.load(&mut factory, spawn()).unwrap());
        let guid = world.objects.get(handle).unwrap().guid.unwrap();
        assert_eq!(
            guid.raw(),
            if mode == 0 {
                0xf110_0000_6400_0007
            } else {
                0xf110_0000_6400_00cb
            }
        );
    }
}
#[test]
fn negative_respawn_never_draws_reads_saved_time_or_summons_link() {
    let (mut world, mut factory) = (World::new(), Factory::new());
    factory.templates.get_mut(&100).unwrap().kind = 3;
    factory.templates.get_mut(&100).unwrap().data[7] = 101;
    let mut row = spawn();
    row.respawn_min = -30;
    row.respawn_max = -5;
    let handle = loaded(world.load(&mut factory, row).unwrap());
    let object = factory.objects.get(handle).unwrap();
    assert!(!object.spawned_by_default && !object.is_spawned() && !object.collision_enabled());
    assert_eq!(
        (
            object.respawn_delay,
            object.respawn_time,
            object.linked_trap
        ),
        (30, 0, None)
    );
    assert_eq!(factory.cursor.raw, 0);
    assert!(!factory.actions.contains(&Action::RespawnRead));
    assert!(!factory.actions.contains(&Action::Calendar));
}
#[test]
fn permanent_default_skips_respawn_draw_and_saved_lookup() {
    let (mut world, mut factory) = (World::new(), Factory::new());
    factory.templates.get_mut(&100).unwrap().kind = 0;
    factory.saved = 200;
    let handle = loaded(world.load(&mut factory, spawn()).unwrap());
    let object = factory.objects.get(handle).unwrap();
    assert_eq!(
        (
            object.respawn_delay,
            object.respawn_time,
            object.flags & 0x20
        ),
        (0, 0, 0x20)
    );
    assert_eq!(factory.cursor.raw, 0);
    assert!(!factory.actions.contains(&Action::RespawnRead));
}
#[test]
fn expired_saved_respawn_writes_zero_before_world_add_future_stays_disabled() {
    for saved in [0, 100, 101] {
        let (mut world, mut factory) = (World::new(), Factory::new());
        factory.saved = saved;
        let handle = loaded(world.load(&mut factory, spawn()).unwrap());
        let object = factory.objects.get(handle).unwrap();
        assert_eq!(object.respawn_time, if saved > 100 { saved } else { 0 });
        assert_eq!(factory.models[&handle], saved <= 100);
        assert_eq!(factory.actions.contains(&Action::Calendar), saved != 0);
        assert_eq!(factory.actions.contains(&Action::Save(7, 0)), saved == 100);
        assert_eq!(factory.cursor.raw, 1);
        if saved == 100 {
            let save = factory
                .actions
                .iter()
                .position(|a| *a == Action::Save(7, 0))
                .unwrap();
            let add = factory
                .actions
                .iter()
                .position(|a| *a == Action::Model(handle, GameObjectModelOperation::Insert))
                .unwrap();
            assert!(save < add);
        }
    }
}
#[test]
fn transform_initial_state_and_model_registration_ai_order_are_source_ordered() {
    let (mut world, mut factory) = (World::new(), Factory::new());
    let info = factory.templates.get_mut(&100).unwrap();
    info.kind = 0;
    info.data[0] = 1;
    let mut row = spawn();
    row.rotation_bits = [0x8000_0000, 0, 0, 0];
    row.position_bits[3] = 0x8000_0000;
    row.state = 2;
    let handle = loaded(world.load(&mut factory, row).unwrap());
    let object = factory.objects.get(handle).unwrap();
    assert_eq!(object.position_bits, [0, 0, 0, 0x8000_0000]);
    assert_eq!(
        object.rotation_bits,
        [0x8000_0000, 0, 0x8000_0000, 0x3f80_0000]
    );
    assert_eq!(
        (object.state, object.loot_state, object.animation),
        (2, 2, 100)
    );
    assert_eq!(
        factory.actions,
        [
            Action::Model(handle, GameObjectModelOperation::ConstructAndInsert),
            Action::Callback(handle, GameObjectCallback::CreateBattlegroundOrOutdoor),
            Action::Callback(handle, GameObjectCallback::CreateInstance),
            Action::Callback(handle, GameObjectCallback::LargeVisibility),
            Action::Model(handle, GameObjectModelOperation::Insert),
            Action::Model(handle, GameObjectModelOperation::Enable(false)),
            Action::Callback(handle, GameObjectCallback::MapViewpoint),
            Action::Callback(handle, GameObjectCallback::MapVisibility),
            Action::Callback(handle, GameObjectCallback::InitializeAi),
        ]
    );
}
#[test]
fn linked_trap_is_constructed_and_initialized_before_parent_ai_without_respawn_draw() {
    let (mut world, mut factory) = (World::new(), Factory::new());
    factory.templates.get_mut(&100).unwrap().kind = 3;
    factory.templates.get_mut(&100).unwrap().data[7] = 101;
    factory.templates.insert(101, template(101, 6));
    let parent = loaded(world.load(&mut factory, spawn()).unwrap());
    let child = factory.objects.get(parent).unwrap().linked_trap.unwrap();
    assert_eq!(
        child,
        ObjectHandle {
            thread: 1,
            construction: 2
        }
    );
    assert_eq!(
        world.objects.get(child).unwrap().guid.unwrap().counter(),
        203
    );
    assert_eq!(world.objects.get(child).unwrap().database_guid, 203);
    assert_eq!(factory.objects.get(child).unwrap().respawn_delay, 0);
    assert_eq!(factory.objects.get(child).unwrap().loot_state, 0);
    assert_eq!(factory.cursor.raw, 1);
    let child_ai = factory
        .actions
        .iter()
        .position(|a| *a == Action::Callback(child, GameObjectCallback::InitializeAi))
        .unwrap();
    let parent_ai = factory
        .actions
        .iter()
        .position(|a| *a == Action::Callback(parent, GameObjectCallback::InitializeAi))
        .unwrap();
    assert!(child_ai < parent_ai);
}
#[test]
fn missing_and_invalid_templates_destroy_failed_candidates_after_consumed_selection() {
    for invalid in [false, true] {
        let (mut world, mut factory) = (World::new(), Factory::new());
        factory.entries = Some(Arc::from([101]));
        if invalid {
            factory.templates.insert(101, template(101, 31));
        }
        let outcome = world.load(&mut factory, spawn()).unwrap();
        assert_eq!(
            outcome,
            GridCandidateOutcome::FailedAndDestroyed(ObjectHandle {
                thread: 1,
                construction: 1
            })
        );
        assert!(world
            .objects
            .get(ObjectHandle {
                thread: 1,
                construction: 1
            })
            .is_err());
        assert_eq!(factory.cursor.raw, 1);
        assert!(factory.models.is_empty());
    }
}
#[test]
fn missing_collision_refuses_before_create_callbacks_or_world_membership() {
    let (mut world, mut factory) = (World::new(), Factory::new());
    factory.collision_available = false;
    assert_eq!(
        world.load(&mut factory, spawn()).unwrap_err().id,
        "execution.dynamic_collision_owner_unavailable"
    );
    let object = world
        .objects
        .get(ObjectHandle {
            thread: 1,
            construction: 1,
        })
        .unwrap();
    assert!(!object.in_world);
    assert!(object.guid.is_some());
    assert!(factory.actions.is_empty());
    assert_eq!(factory.cursor.raw, 0);
}
#[test]
fn callback_can_construct_another_go_through_same_reentrant_factory() {
    let (mut world, mut factory) = (World::new(), Factory::new());
    let mut child = spawn();
    child.database_guid = 8;
    factory.recurse = Some(child);
    let parent = loaded(world.load(&mut factory, spawn()).unwrap());
    let child = world
        .objects
        .lookup_database(map(), DatabaseLookupKind::GameObject, 8)
        .unwrap();
    assert_eq!(parent.construction, 1);
    assert_eq!(child.construction, 2);
    assert_eq!(factory.cursor.raw, 2);
}
#[test]
fn second_real_grid_load_reconstructs_new_object_and_grid_generations() {
    let (mut world, mut factory) = (World::new(), Factory::new());
    let pair = compute_cell_pair(0, 0).unwrap();
    world.definitions.rows.insert(7, spawn());
    world
        .sets
        .register(
            pair,
            GridSpawnKind::GameObject,
            GridSpawnOrigin::Static,
            7,
            0,
        )
        .unwrap();
    world
        .context()
        .load_grid(pair, GridLoadTrigger::Enter, &mut factory)
        .unwrap();
    let first = world
        .objects
        .lookup_database(map(), DatabaseLookupKind::GameObject, 7)
        .unwrap();
    let old_grid = world.cells.registered_cell(first).unwrap().grid;
    let old_guid = world.objects.get(first).unwrap().guid;
    factory
        .gameobject_model(
            first,
            GameObjectModelOperation::Remove,
            &mut world.context(),
        )
        .unwrap();
    world.objects.remove_from_world(first).unwrap();
    factory
        .gameobject_model(
            first,
            GameObjectModelOperation::Destroy,
            &mut world.context(),
        )
        .unwrap();
    world.objects.destroy(first, &mut world.cells).unwrap();
    factory
        .objects
        .forget_destroyed(first, &world.objects)
        .unwrap();
    world.cells.destroy_empty_grid(old_grid).unwrap();
    world
        .context()
        .load_grid(pair, GridLoadTrigger::Enter, &mut factory)
        .unwrap();
    let next = world
        .objects
        .lookup_database(map(), DatabaseLookupKind::GameObject, 7)
        .unwrap();
    assert_eq!((first.construction, next.construction), (1, 2));
    assert_eq!(world.objects.get(next).unwrap().guid, old_guid);
    assert_eq!(
        world.cells.registered_cell(next).unwrap().grid.generation,
        old_grid.generation + 1
    );
    assert_eq!(factory.cursor.raw, 2);
    assert!(world.objects.get(first).is_err());
}
#[test]
fn factory_selection_uses_original_entry_and_other_owners_refuse() {
    let (mut world, mut factory) = (World::new(), Factory::new());
    factory.templates.get_mut(&100).unwrap().kind = 11;
    factory.entries = Some(Arc::from([101]));
    assert_eq!(
        world.load(&mut factory, spawn()).unwrap_err().id,
        "execution.elevator_constructor_owner_unavailable"
    );
    assert!(factory.actions.is_empty());
    assert_eq!(factory.cursor.raw, 0);
    for (kind, id) in [
        (
            SourceObjectKind::Pet,
            "execution.pet_constructor_owner_unavailable",
        ),
        (
            SourceObjectKind::Corpse,
            "execution.corpse_constructor_owner_unavailable",
        ),
        (
            SourceObjectKind::DynamicObject,
            "execution.dynamic_object_constructor_owner_unavailable",
        ),
        (
            SourceObjectKind::Transport,
            "execution.transport_attachment_owner_unavailable",
        ),
    ] {
        assert_eq!(refuse_unavailable_object_constructor(kind).id, id);
    }
}
