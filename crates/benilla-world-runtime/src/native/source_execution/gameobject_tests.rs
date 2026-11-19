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
    Animation(u32),
    Observation(GameObjectFactoryObservation),
    Area([u32; 3]),
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum UnboundOperand {
    SinCos,
    Battleground,
    Zone,
    OutdoorScript,
    VisibilityDistance,
    VisibilityCensus,
    ScriptRegistry,
}

struct Factory {
    objects: SourceGameObjects,
    guids: MapGuidAllocators,
    templates: BTreeMap<u32, GameObjectTemplate>,
    events: bool,
    battleground: bool,
    outdoor: Option<OutdoorPvpScript>,
    instance: Option<SourceWorldScript>,
    map_visibility_bits: u32,
    visibility: SourceVisibilityCensus,
    script_id: u32,
    unbound_operand: Option<UnboundOperand>,
    scripts: SourceScriptRegistry,
    bell_areas: Option<std::collections::VecDeque<u32>>,
    half_yaw: Option<u32>,
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
    lifecycle: Option<gameobject_lifecycle_tests::HostState>,
    animations: SourceTransportTemplates,
    passengers: SourcePassengers,
    real_dynamic: Option<(SourceDynamicModels, SourceDynamicMap, SourceVMapManager)>,
}
impl Factory {
    fn new() -> Self {
        Self::new_on(map())
    }
    fn new_on(map: MapIdentity) -> Self {
        let mut globals = GlobalGuidAllocators::from_seed_rows(GuidSeedRows {
            player: GuidMaximum::NoResult,
            item: GuidMaximum::NoResult,
            corpse: GuidMaximum::NoResult,
            pet_number: GuidMaximum::NoResult,
            creature: GuidMaximum::Value(99),
            gameobject: GuidMaximum::Value(199),
            creature_reserve: 2,
            gameobject_reserve: 3,
        });
        let mut animations = SourceTransportTemplates::default();
        animations.load_animations([], &mut |_| Ok(())).unwrap();
        let mut scripts = SourceScriptRegistry::default();
        scripts.load_script_names(|| Ok(None)).unwrap();
        scripts.initialize_registry([]).unwrap();
        Self {
            objects: SourceGameObjects::default(),
            guids: globals.map_allocators(map).unwrap(),
            templates: BTreeMap::from([(100, template(100, 5))]),
            events: false,
            battleground: false,
            outdoor: None,
            instance: None,
            map_visibility_bits: 100.0_f32.to_bits(),
            visibility: SourceVisibilityCensus::default(),
            script_id: 0,
            unbound_operand: None,
            scripts,
            bell_areas: None,
            half_yaw: None,
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
            lifecycle: None,
            animations,
            passengers: SourcePassengers::default(),
            real_dynamic: None,
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
    fn gameobject_constructed(
        &mut self,
        handle: ObjectHandle,
        objects: &SourceObjects,
    ) -> Result<()> {
        self.passengers.construct(handle, objects)
    }
    fn gameobject_transport_animation(
        &mut self,
        entry: u32,
    ) -> Result<Option<Arc<SourceTransportAnimation>>> {
        self.actions.push(Action::Animation(entry));
        Ok(self.animations.animation(entry)?.cloned().map(Arc::new))
    }
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
    fn gameobject_read_persistent_respawn(
        &mut self,
        _: u32,
        _: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<i64> {
        self.actions.push(Action::RespawnRead);
        Ok(self.saved)
    }
    fn gameobject_write_persistent_respawn(
        &mut self,
        guid: u32,
        time: i64,
        _: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        self.actions.push(Action::Save(guid, time));
        self.saved = time;
        Ok(())
    }
    fn gameobject_calendar(&mut self) -> Result<i64> {
        self.actions.push(Action::Calendar);
        Ok(self.now)
    }
    fn gameobject_sin_cos(&mut self, half_yaw_bits: u32) -> Result<[u32; 2]> {
        if self.unbound_operand == Some(UnboundOperand::SinCos) {
            return UnboundCallbacks(self).gameobject_sin_cos(half_yaw_bits);
        }
        self.half_yaw = Some(half_yaw_bits);
        Ok(benilla_sim::source_trig_fma::sin_cos_bits(half_yaw_bits))
    }
    fn gameobject_is_battleground(
        &mut self,
        handle: ObjectHandle,
        objects: &SourceObjects,
    ) -> Result<bool> {
        if self.unbound_operand == Some(UnboundOperand::Battleground) {
            return UnboundCallbacks(self).gameobject_is_battleground(handle, objects);
        }
        Ok(self.battleground)
    }
    fn gameobject_zone(
        &mut self,
        handle: ObjectHandle,
        position: [f32; 3],
        context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<u32> {
        if self.unbound_operand == Some(UnboundOperand::Zone) {
            return UnboundCallbacks(self).gameobject_zone(handle, position, context);
        }
        assert!(!self.battleground);
        assert!(position.into_iter().all(f32::is_finite));
        Ok(139)
    }
    fn gameobject_area(
        &mut self,
        handle: ObjectHandle,
        position: [f32; 3],
        context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<u32> {
        if let Some(areas) = &mut self.bell_areas {
            self.actions.push(Action::Area(position.map(f32::to_bits)));
            Ok(areas.pop_front().unwrap())
        } else {
            UnboundCallbacks(self).gameobject_area(handle, position, context)
        }
    }
    fn observe_gameobject_factory(&mut self, observation: GameObjectFactoryObservation) {
        if matches!(
            observation,
            GameObjectFactoryObservation::InstanceOnObjectSpawn
        ) {
            self.actions.push(Action::Observation(observation));
        }
    }
    fn gameobject_outdoor_script(&mut self, zone: u32) -> Result<Option<OutdoorPvpScript>> {
        if self.unbound_operand == Some(UnboundOperand::OutdoorScript) {
            return UnboundCallbacks(self).gameobject_outdoor_script(zone);
        }
        assert_eq!(zone, 139);
        Ok(self.outdoor)
    }
    fn gameobject_instance_data(
        &mut self,
    ) -> Result<Option<(&SourceGameObjects, &mut SourceWorldScript)>> {
        Ok(self.instance.as_mut().map(|script| (&self.objects, script)))
    }
    fn gameobject_map_visibility_distance(&mut self) -> Result<u32> {
        if self.unbound_operand == Some(UnboundOperand::VisibilityDistance) {
            return UnboundCallbacks(self).gameobject_map_visibility_distance();
        }
        Ok(self.map_visibility_bits)
    }
    fn gameobject_visibility_census(&mut self) -> Result<&mut SourceVisibilityCensus> {
        if self.unbound_operand == Some(UnboundOperand::VisibilityCensus) {
            UnboundCallbacks(self).gameobject_visibility_census()?;
        }
        Ok(&mut self.visibility)
    }
    fn gameobject_script_id(&mut self, entry: u32) -> Result<u32> {
        assert!(self.templates.contains_key(&entry));
        Ok(self.script_id)
    }
    fn gameobject_script_registry(&mut self) -> Result<&SourceScriptRegistry> {
        if self.unbound_operand == Some(UnboundOperand::ScriptRegistry) {
            UnboundCallbacks(self).gameobject_script_registry()?;
        }
        Ok(&self.scripts)
    }
    fn gameobject_geometry(
        &mut self,
        _: MapIdentity,
    ) -> Result<(
        &mut SourceDynamicModels,
        &mut SourceDynamicMap,
        &mut SourceVMapManager,
    )> {
        let (models, map, vmap) = self.real_dynamic.as_mut().unwrap();
        Ok((models, map, vmap))
    }
    fn gameobject_model(
        &mut self,
        handle: ObjectHandle,
        op: GameObjectModelOperation,
        context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<bool> {
        if self.real_dynamic.is_some() {
            self.actions.push(Action::Model(handle, op));
            if context.objects.get(handle)?.source_allocation.is_none() {
                context.objects.bind_source_allocation(
                    handle,
                    handle.construction * 256,
                    128,
                    0,
                )?;
            }
            return gameobjects::execute_source_gameobject_model(self, handle, op, context);
        }
        if !self.collision_available {
            return Err(Refusal {
                id: "execution.dynamic_collision.gameobject_world_geometry_owner".into(),
            });
        }
        if self.lifecycle.is_some() {
            return self.lifecycle_model(handle, op, context);
        }
        self.actions.push(Action::Model(handle, op));
        if op == GameObjectModelOperation::ConstructAndInsert {
            context
                .objects
                .bind_source_allocation(handle, handle.construction * 256, 128, 0)?;
        }
        match op {
            GameObjectModelOperation::ConstructAndInsert => {
                assert!(!context.objects.get(handle)?.in_world);
                self.models.insert(handle, false);
            }
            GameObjectModelOperation::Insert => {
                assert!(!context.objects.get(handle)?.in_world);
                assert_eq!(
                    context.objects.lookup_database(
                        context.cells.map(),
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
            GameObjectModelOperation::Contains => return Ok(self.models.contains_key(&handle)),
            GameObjectModelOperation::Relocate => assert!(self.models.contains_key(&handle)),
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
        if self.lifecycle.is_some() {
            self.lifecycle_callback(handle, callback);
        }
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
        match callback {
            GameObjectCallback::CreateBattlegroundOrOutdoor
            | GameObjectCallback::CreateInstance
            | GameObjectCallback::LargeVisibility
            | GameObjectCallback::MapViewpoint
            | GameObjectCallback::MapVisibility
            | GameObjectCallback::InitializeAi => {
                gameobjects::execute_source_gameobject_callback(self, handle, callback, context)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn gameobject_dynamic_spawn_allowed(
        &mut self,
        database: u32,
        row: &GameObjectSpawn,
    ) -> Result<bool> {
        Ok(row.database_guid == database)
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
        Self::new_on(map())
    }
    fn new_on(map: MapIdentity) -> Self {
        let mut cells = SourceCells::new(map).unwrap();
        cells.configure_grids(300000, true).unwrap();
        Self {
            definitions: Definitions {
                rows: BTreeMap::new(),
            },
            products: Products,
            sets: GridSpawnSets::new(map),
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
    for (mode, map_id) in [(0, 0), (1, 0), (2, 533), (2, 543), (3, 0)] {
        let map = MapIdentity {
            map: map_id,
            ..map()
        };
        let (mut world, mut factory) = (World::new_on(map), Factory::new_on(map));
        factory.events = mode == 1;
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
fn gameobject_fallback_rotation_passes_half_yaw_only_for_two_zero_components() {
    for fallback in [false, true] {
        let (mut world, mut factory) = (World::new(), Factory::new());
        let mut row = spawn();
        row.position_bits[3] = 1.0_f32.to_bits();
        if fallback {
            row.rotation_bits[3] = 0x8000_0000;
        }
        let handle = loaded(world.load(&mut factory, row).unwrap());
        assert_eq!(factory.half_yaw, fallback.then_some(0x3f00_0000));
        assert_eq!(
            factory.objects.get(handle).unwrap().rotation_bits,
            if fallback {
                [0, 0, 0x3ef5_7744, 0x3f60_a940]
            } else {
                [0, 0, 0, 0x3f80_0000]
            }
        );
    }
}

#[test]
fn gameobject_pvp_branch_refuses_before_instance_or_world_add() {
    for (battleground, expected) in [
        (
            true,
            "execution.gameobject.loaded_battleground_create_owner",
        ),
        (
            false,
            "execution.gameobject.loaded_outdoor_pvp_create_owner",
        ),
    ] {
        let (mut world, mut factory) = (World::new(), Factory::new());
        factory.battleground = battleground;
        factory.outdoor = Some(OutdoorPvpScript::EasternPlaguelands);
        assert_eq!(world.load(&mut factory, spawn()).unwrap_err().id, expected);
        let handle = ObjectHandle {
            thread: 1,
            construction: 1,
        };
        assert_eq!(
            factory.actions,
            [
                Action::Model(handle, GameObjectModelOperation::ConstructAndInsert),
                Action::Callback(handle, GameObjectCallback::CreateBattlegroundOrOutdoor)
            ]
        );
        assert!(!world.objects.get(handle).unwrap().in_world);
        assert_eq!(factory.cursor.raw, 0);
    }
}

#[test]
fn gameobject_instance_create_uses_live_guid_and_activates_at_map_add() {
    let map = MapIdentity { map: 1, ..map() };
    let (mut world, mut factory) = (World::new_on(map), Factory::new_on(map));
    factory.instance = Some(SourceWorldScript {
        map,
        stores: WorldScriptStores::default(),
        state: WorldScriptState::Kalimdor(KalimdorWorldState::default()),
    });
    let entry = 177746;
    factory.templates.insert(entry, template(entry, 5));
    let handle = loaded(
        world
            .load(&mut factory, GameObjectSpawn { entry, ..spawn() })
            .unwrap(),
    );
    let script = factory.instance.unwrap();
    let WorldScriptState::Kalimdor(state) = script.state else {
        panic!("expected Kalimdor script");
    };
    assert_eq!(state.ghost_events.len(), 1);
    assert_eq!(state.ghost_events[0].guid, 0xf110_02b6_5200_0007);
    assert_eq!(
        world
            .cells
            .active_objects_in_grid(world.cells.registered_cell(handle).unwrap().grid)
            .unwrap(),
        1,
    );
    let object = world.objects.get(handle).unwrap();
    assert!(object.in_world && object.active);
    assert!(world.cells.active().contains(handle));
}

#[test]
fn gameobject_large_visibility_compares_the_effective_distance_and_uses_default_radius() {
    for (large, map_distance, expected) in [
        (false, 100.0_f32, 0),
        (true, 199.0, 200.0_f32.to_bits()),
        (true, 200.0, 0),
        (true, 201.0, 0),
    ] {
        let (mut world, mut factory) = (World::new(), Factory::new());
        factory.templates.get_mut(&100).unwrap().data[3] = u32::from(large);
        factory.map_visibility_bits = map_distance.to_bits();
        let handle = loaded(world.load(&mut factory, spawn()).unwrap());
        assert_eq!(
            world.objects.visibility_override_bits(handle).unwrap(),
            expected
        );
    }

    let (mut world, mut factory) = (World::new(), Factory::new());
    factory.templates.get_mut(&100).unwrap().scale_bits = 0.1_f32.to_bits();
    factory.map_visibility_bits = 23.0_f32.to_bits();
    let mut row = spawn();
    row.position_bits[..2].fill(10.25_f32.to_bits());
    world.load(&mut factory, row).unwrap();
    // CellImpl.h:54-81 adds the unscaled 0.389 radius. Both axes cross the
    // 33.333-yard cell boundary, so the visit covers 3 by 3 cells, not 2 by 2.
    assert_eq!(factory.visibility.considered_cells, 9);
}

#[test]
fn gameobject_nonnull_script_factory_refuses_after_world_add_and_visibility() {
    let (mut world, mut factory) = (World::new(), Factory::new());
    factory.scripts = SourceScriptRegistry::default();
    factory
        .scripts
        .load_script_names(|| Ok(Some(vec![b"go".to_vec(), b"go_bells".to_vec()])))
        .unwrap();
    factory
        .scripts
        .initialize_registry([b"go".as_slice(), b"go_bells"].into_iter().enumerate().map(
            |(ordinal, name)| SourceScriptDefinition {
                name: name.to_vec(),
                registration: ordinal as u64,
                operation: "authored".into(),
                source_file: "authored.cpp".into(),
                source_line: 1,
                source_function: "AddSC_authored".into(),
                slots: BTreeMap::from([("GetGameObjectAI".into(), true)]),
            },
        ))
        .unwrap();
    factory.script_id = factory.scripts.get_script_id(Some(b"go"));
    assert_eq!(factory.script_id, 1);
    assert_eq!(
        world.load(&mut factory, spawn()).unwrap_err().id,
        format!("{}.go", script_registry::SCRIPTDEV_GET_GAMEOBJECT_AI_OWNER),
    );
    let handle = world
        .objects
        .lookup_database(map(), DatabaseLookupKind::GameObject, 7)
        .unwrap();
    assert!(world.objects.get(handle).unwrap().in_world);
    assert!(factory.objects.get(handle).unwrap().ai.is_none());
    assert!(factory.visibility.considered_cells > 0);
    factory.script_id = factory.scripts.get_script_id(Some(b"go_bells"));
    factory
        .gameobject_callback(
            handle,
            GameObjectCallback::InitializeAi,
            &mut world.context(),
        )
        .unwrap();
    factory.script_id = factory.scripts.get_script_id(Some(b"go"));
    assert_eq!(
        factory
            .gameobject_callback(
                handle,
                GameObjectCallback::InitializeAi,
                &mut world.context(),
            )
            .unwrap_err()
            .id,
        format!("{}.go", script_registry::SCRIPTDEV_GET_GAMEOBJECT_AI_OWNER),
    );
    let retained = factory.objects.get(handle).unwrap().ai.as_ref().unwrap();
    assert_eq!(retained.object, handle);
    factory.script_id = 0;
    gameobjects::execute_source_gameobject_callback(
        &mut factory,
        handle,
        GameObjectCallback::InitializeAi,
        &mut world.context(),
    )
    .unwrap();
    assert!(factory.objects.get(handle).unwrap().ai.is_none());
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
fn signed32_database_state_preserves_sentinel_until_byte_state_assignment() {
    for (source_state, expected) in [(-1_i32, 0_u8), (0, 0), (1, 1), (2, 2)] {
        let (mut world, mut factory) = (World::new(), Factory::new());
        let info = factory.templates.get_mut(&100).unwrap();
        info.kind = 0;
        info.data[0] = 1;
        let mut row = spawn();
        row.state = source_state;
        assert_eq!(std::mem::size_of_val(&row.state), 4);
        let handle = loaded(world.load(&mut factory, row).unwrap());
        let object = factory.objects.get(handle).unwrap();
        assert_eq!(object.state, expected);
        assert_eq!(object.loot_state, 2);
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
    let child_guid = factory.objects.get(parent).unwrap().linked_trap.unwrap();
    let child = world
        .objects
        .lookup_world_object(map(), child_guid)
        .unwrap();
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
        "execution.dynamic_collision.gameobject_world_geometry_owner"
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
    // Virtual class selection precedes random entry resolution; GameObject.cpp:107-112, Transports.cpp:444-449 at 8ec338a1.
    let (mut world, mut factory) = (World::new(), Factory::new());
    factory.templates.get_mut(&100).unwrap().kind = 11;
    factory.templates.insert(101, template(101, 5));
    factory.entries = Some(Arc::from([101]));
    let object = loaded(world.load(&mut factory, spawn()).unwrap());
    assert_eq!(factory.objects.get(object).unwrap().template.entry, 101);
    assert!(factory.objects.get(object).unwrap().elevator.is_some());
    assert!(factory.actions.contains(&Action::Animation(101)));
    assert_eq!(factory.cursor.raw, 2);
    for (kind, id) in [
        (
            SourceObjectKind::Pet,
            "execution.pet_constructor_owner_unavailable",
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

#[test]
fn dynamic_gameobject_spawn_uses_zero_requested_guid_and_preserves_grid_path() {
    // Object.cpp:2173-2188 versus ObjectGridLoader.cpp:120-140.
    let mut world = World::new();
    let mut factory = Factory::new();
    world.load(&mut factory, spawn()).unwrap();
    let object = world
        .objects
        .lookup_database(map(), DatabaseLookupKind::GameObject, 7)
        .unwrap();
    assert_eq!(
        world.objects.get(object).unwrap().guid.unwrap().counter(),
        7
    );
    let mut world = World::new();
    let mut factory = Factory::new();
    let cell = compute_cell_pair(0, 0).unwrap();
    let grid = world
        .context()
        .load_grid(cell, GridLoadTrigger::TerrainOnly, &mut factory)
        .unwrap()
        .unwrap();
    let attempt = GridLoadAttempt {
        grid,
        cell,
        kind: GridSpawnKind::GameObject,
        origin: GridSpawnOrigin::Persistent,
        guid: 7,
        catalog_ordinal: 0,
    };
    let request = DynamicSpawnRequest {
        map: map(),
        thread: factory.gameobject_thread(),
        key: SpawnKey {
            kind: GridSpawnKind::GameObject,
            guid: 7,
        },
    };
    assert!(request
        .spawn_gameobject(&mut factory, attempt, spawn(), &mut world.context())
        .unwrap());
    let object = world
        .objects
        .lookup_database(map(), DatabaseLookupKind::GameObject, 7)
        .unwrap();
    assert_eq!(
        world.objects.get(object).unwrap().guid.unwrap().counter(),
        203
    );
    let mut world = World::new();
    let mut factory = Factory::new();
    factory.templates.insert(100, template(100, 11));
    assert!(request
        .spawn_gameobject(&mut factory, attempt, spawn(), &mut world.context())
        .unwrap());
    let object = world
        .objects
        .lookup_database(map(), DatabaseLookupKind::GameObject, 7)
        .unwrap();
    assert_eq!(
        world.objects.get(object).unwrap().guid.unwrap().counter(),
        203
    );
    assert!(factory.objects.get(object).unwrap().elevator.is_some());
    let wrong_map = DynamicSpawnRequest {
        map: MapIdentity {
            creation_generation: 2,
            ..map()
        },
        ..request
    };
    assert_eq!(
        wrong_map
            .spawn_gameobject(&mut factory, attempt, spawn(), &mut world.context())
            .unwrap_err()
            .id,
        "execution.spawn.constructor_binding"
    );
    let mut world = World::new();
    let mut factory = Factory::new();
    factory.templates.clear();
    assert!(!request
        .spawn_gameobject(&mut factory, attempt, spawn(), &mut world.context())
        .unwrap());
    assert!(world
        .objects
        .lookup_database(map(), DatabaseLookupKind::GameObject, 7)
        .is_none());
    let mut wrong_row = spawn();
    wrong_row.database_guid = 99;
    assert!(!request
        .spawn_gameobject(&mut factory, attempt, wrong_row, &mut world.context())
        .unwrap());
}

#[test]
fn group_gameobject_constructor_forwards_forced_entry_and_zero_fallback() {
    // Object.cpp:2175-2183 and GameObject.cpp:960-965.
    for (forced, expected) in [(0, 100), (101, 101)] {
        let mut world = World::new();
        let mut factory = Factory::new();
        factory.templates.insert(101, template(101, 5));
        let cell = compute_cell_pair(0, 0).unwrap();
        let grid = world
            .context()
            .load_grid(cell, GridLoadTrigger::TerrainOnly, &mut factory)
            .unwrap()
            .unwrap();
        let attempt = GridLoadAttempt {
            grid,
            cell,
            kind: GridSpawnKind::GameObject,
            origin: GridSpawnOrigin::Persistent,
            guid: 7,
            catalog_ordinal: 0,
        };
        let request = GroupSpawnRequest {
            map: map(),
            thread: factory.gameobject_thread(),
            key: SpawnKey {
                kind: GridSpawnKind::GameObject,
                guid: 7,
            },
            entry: forced,
        };
        assert!(request
            .spawn_gameobject(&mut factory, attempt, spawn(), &mut world.context())
            .unwrap());
        let object = world
            .objects
            .lookup_database(map(), DatabaseLookupKind::GameObject, 7)
            .unwrap();
        assert_eq!(
            factory.objects.get(object).unwrap().template.entry,
            expected
        );
        assert_eq!(
            world.objects.get(object).unwrap().guid.unwrap().entry(),
            expected
        );
        assert_eq!(
            world.objects.get(object).unwrap().guid.unwrap().counter(),
            203
        );
    }
}

#[path = "gameobject_lifecycle_tests.rs"]
mod gameobject_lifecycle_tests;

#[test]
fn pool_load_uses_static_guid_and_failed_load_is_really_destroyed() {
    for fail in [false, true] {
        let mut world = World::new();
        let mut factory = Factory::new();
        factory.events = true;
        if fail {
            factory.templates.clear();
        }
        let pair = compute_cell_pair(0_f32.to_bits(), 0_f32.to_bits()).unwrap();
        let grid = world
            .context()
            .load_grid(pair, GridLoadTrigger::TerrainOnly, &mut factory)
            .unwrap()
            .unwrap();
        let attempt = GridLoadAttempt {
            grid,
            cell: pair,
            kind: GridSpawnKind::GameObject,
            origin: GridSpawnOrigin::Persistent,
            guid: 7,
            catalog_ordinal: 0,
        };
        let request = PoolLoadRequest {
            state: PersistentStateIdentity {
                map: 0,
                instance: 0,
                generation: 1,
            },
            map: map(),
            key: SpawnKey {
                kind: GridSpawnKind::GameObject,
                guid: 7,
            },
        };
        let outcome = request
            .load_gameobject(&mut factory, attempt, spawn(), &mut world.context())
            .unwrap();
        match outcome {
            GridCandidateOutcome::FailedAndDestroyed(object) => {
                assert!(fail);
                assert!(world.objects.get(object).is_err());
                assert_eq!(factory.cursor.raw, 0);
                assert!(factory.objects.get(object).is_err());
            }
            GridCandidateOutcome::Loaded(object) => {
                assert!(!fail);
                assert_eq!(
                    world.objects.get(object).unwrap().guid.unwrap().counter(),
                    7
                );
                assert_eq!(factory.cursor.raw, 1);
            }
        }
    }
}

// Keep only the required bindings so the production defaults remain in use.
struct UnboundCallbacks<'a>(&'a mut Factory);

impl GridObjectFactory<Definitions, Products> for UnboundCallbacks<'_> {
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
        cell: CellIdentity,
        context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        self.0.viewpoint_added(object, cell, context)
    }
    fn battleground_db_load(
        &mut self,
        object: ObjectHandle,
        context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        self.0.battleground_db_load(object, context)
    }
}

impl GameObjectFactoryOwner<Definitions, Products> for UnboundCallbacks<'_> {
    fn gameobjects(&mut self) -> &mut SourceGameObjects {
        self.0.gameobjects()
    }
    fn gameobject_constructed(
        &mut self,
        handle: ObjectHandle,
        objects: &SourceObjects,
    ) -> Result<()> {
        self.0.gameobject_constructed(handle, objects)
    }
    fn gameobject_guids(&mut self) -> &mut MapGuidAllocators {
        self.0.gameobject_guids()
    }
    fn gameobject_spawn(
        &mut self,
        attempt: GridLoadAttempt,
        row: GameObjectSpawn,
    ) -> Result<GameObjectSpawn> {
        self.0.gameobject_spawn(attempt, row)
    }
    fn gameobject_template(&mut self, entry: u32) -> Result<Option<GameObjectTemplate>> {
        self.0.gameobject_template(entry)
    }
    fn gameobject_thread(&self) -> u64 {
        self.0.gameobject_thread()
    }
    fn gameobject_event_guid(&mut self, guid: u32) -> Result<bool> {
        self.0.gameobject_event_guid(guid)
    }
    fn gameobject_group(&mut self, guid: u32) -> Result<Option<GameObjectGroupEntry>> {
        self.0.gameobject_group(guid)
    }
    fn gameobject_group_entry(&mut self, group: u32, guid: u32) -> Result<u32> {
        self.0.gameobject_group_entry(group, guid)
    }
    fn gameobject_random_entries(&mut self, guid: u32) -> Result<Option<Arc<[u32]>>> {
        self.0.gameobject_random_entries(guid)
    }
    fn gameobject_irand(&mut self, min: i32, max: i32) -> Result<i32> {
        self.0.gameobject_irand(min, max)
    }
    fn gameobject_urand(&mut self, min: u32, max: u32) -> Result<u32> {
        self.0.gameobject_urand(min, max)
    }
    fn gameobject_read_persistent_respawn(
        &mut self,
        guid: u32,
        context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<i64> {
        self.0.gameobject_read_persistent_respawn(guid, context)
    }
}

fn assert_unbound_factory_operand(operand: UnboundOperand, id: &str) {
    let (mut world, mut factory) = (World::new(), Factory::new());
    factory.unbound_operand = Some(operand);
    let mut row = spawn();
    if operand == UnboundOperand::SinCos {
        row.rotation_bits = [0; 4];
    }
    if operand == UnboundOperand::VisibilityDistance {
        factory.templates.get_mut(&100).unwrap().data[3] = 1;
    }
    assert_eq!(world.load(&mut factory, row).unwrap_err().id, id);
}

#[test]
fn gameobject_unbound_sin_cos_refuses() {
    assert_unbound_factory_operand(
        UnboundOperand::SinCos,
        "execution.gameobject.GameObject_2059.sin_cos_owner",
    );
}

#[test]
fn gameobject_unbound_battleground_refuses() {
    assert_unbound_factory_operand(
        UnboundOperand::Battleground,
        "execution.gameobject.GameObject_287.IsBattleGround_owner",
    );
}

#[test]
fn gameobject_unbound_zone_refuses() {
    assert_unbound_factory_operand(
        UnboundOperand::Zone,
        "execution.gameobject.GameObject_289.GetZoneId_owner",
    );
}

#[test]
fn gameobject_unbound_outdoor_script_refuses() {
    assert_unbound_factory_operand(
        UnboundOperand::OutdoorScript,
        "execution.gameobject.GameObject_289.GetScript_owner",
    );
}

#[test]
fn gameobject_unbound_visibility_distance_refuses() {
    assert_unbound_factory_operand(
        UnboundOperand::VisibilityDistance,
        "execution.gameobject.GameObject_299.visibility_distance_owner",
    );
}

#[test]
fn gameobject_unbound_visibility_census_refuses() {
    assert_unbound_factory_operand(
        UnboundOperand::VisibilityCensus,
        "execution.visibility.Map_504.gameobject_update_owner",
    );
}

#[test]
fn gameobject_unbound_script_registry_refuses() {
    assert_unbound_factory_operand(
        UnboundOperand::ScriptRegistry,
        "execution.gameobject.ScriptDevAIMgr_375.script_registry_owner",
    );
}

#[test]
fn gameobject_unbound_create_and_respawn_callbacks_refuse_by_name() {
    let (mut world, mut factory) = (World::new(), Factory::new());
    let handle = loaded(world.load(&mut factory, spawn()).unwrap());
    let mut unbound = UnboundCallbacks(&mut factory);
    for (callback, id) in [
        (
            GameObjectCallback::TemplateString(1),
            "execution.gameobject.loaded_string_id_owner",
        ),
        (
            GameObjectCallback::GroupString(1),
            "execution.gameobject.loaded_string_id_owner",
        ),
        (
            GameObjectCallback::CreateInstance,
            "execution.gameobject.loaded_instance_create_owner",
        ),
        (
            GameObjectCallback::BindGroup(1),
            "execution.gameobject.loaded_bind_group_owner",
        ),
        (
            GameObjectCallback::RemoveOutdoorAndOwner,
            "execution.gameobject.loaded_remove_owner",
        ),
        (
            GameObjectCallback::InitializeAi,
            "execution.gameobject.template_binding",
        ),
    ] {
        assert_eq!(
            unbound
                .gameobject_callback(handle, callback, &mut world.context())
                .unwrap_err()
                .id,
            id,
        );
    }
    assert_eq!(
        unbound.gameobject_calendar().unwrap_err().id,
        "execution.gameobject.loaded_respawn_calendar_owner",
    );
    assert_eq!(
        unbound
            .gameobject_write_persistent_respawn(7, 0, &mut world.context())
            .unwrap_err()
            .id,
        "execution.gameobject.loaded_save_respawn_effects_owner",
    );
}

#[test]
fn gameobject_map_viewpoint_refuses_without_cell_or_camera_owner() {
    let (mut world, mut factory) = (World::new(), Factory::new());
    let missing_cell = world
        .objects
        .construct(1, SourceObjectKind::GameObject)
        .unwrap();
    assert_eq!(
        world
            .context()
            .map_gameobject_viewpoint(missing_cell, &mut factory)
            .unwrap_err()
            .id,
        "execution.visibility.map_add_cell_binding",
    );
    let handle = loaded(world.load(&mut factory, spawn()).unwrap());
    let camera = world
        .objects
        .construct(1, SourceObjectKind::Camera)
        .unwrap();
    world
        .objects
        .attach_viewpoint_camera(handle, camera)
        .unwrap();
    assert_eq!(
        world
            .context()
            .map_gameobject_viewpoint(handle, &mut factory)
            .unwrap_err()
            .id,
        "execution.camera.Camera_104.added_to_world_owner",
    );
    assert_eq!(
        world.objects.viewpoint_cell(handle).unwrap(),
        world.cells.registered_cell(handle)
    );
}

fn bell_fixture(entry: u32, areas: Option<[u32; 2]>) -> (Factory, GameObjectSpawn) {
    let mut factory = Factory::new();
    factory.scripts = SourceScriptRegistry::default();
    factory
        .scripts
        .load_script_names(|| Ok(Some(vec![b"go_bells".to_vec()])))
        .unwrap();
    factory
        .scripts
        .initialize_registry([SourceScriptDefinition {
            name: b"go_bells".to_vec(),
            registration: 0,
            operation: "authored".into(),
            source_file: "authored.cpp".into(),
            source_line: 1,
            source_function: "AddSC_authored".into(),
            slots: BTreeMap::from([("GetGameObjectAI".into(), true)]),
        }])
        .unwrap();
    factory.script_id = factory.scripts.get_script_id(Some(b"go_bells"));
    factory.bell_areas = areas.map(Into::into);
    factory.templates.insert(entry, template(entry, 5));
    (factory, GameObjectSpawn { entry, ..spawn() })
}

#[test]
fn gameobject_bells_reads_both_areas_and_retains_constructor_state() {
    for (entry, area, routing_area, sound, zone) in [
        (175885, 1497, 1497, 6595, true),
        (175885, 2118, 2118, 6595, false),
        (175885, 272, 272, 6595, false),
        (175885, 0, 1497, 6675, true),
        (176573, 809, 809, 7234, false),
        (176573, 1537, 1537, 7234, true),
        (176573, 1657, 1657, 6674, true),
        (176573, 0, 0, 6594, false),
        (100, 0, 0, 0, false),
    ] {
        let (mut factory, spawn) = bell_fixture(entry, Some([area, routing_area]));
        let position: [u32; 3] = spawn.position_bits[..3].try_into().unwrap();
        let mut world = World::new();
        let handle = loaded(world.load(&mut factory, spawn).unwrap());
        let ai = factory.objects.get(handle).unwrap().ai.as_ref().unwrap();
        assert_eq!(
            (
                ai.object,
                ai.bell_tolls,
                ai.bell_sound,
                ai.bell_timer,
                ai.bell_plays_to_zone
            ),
            (handle, 0, sound, 0, zone)
        );
        let object = world.objects.get(handle).unwrap();
        assert!(object.in_world && object.active && object.notify_on_event);
        assert!(world.objects.event_notified(map(), handle));
        assert!(world.cells.active().contains(handle));
        let area_calls = factory
            .actions
            .iter()
            .filter_map(|action| match action {
                Action::Area(position) => Some(*position),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(area_calls, vec![position; if entry == 100 { 0 } else { 2 }]);
        assert!(!factory.actions.contains(&Action::Observation(
            GameObjectFactoryObservation::InstanceOnObjectSpawn
        )));
    }
}

#[test]
fn gameobject_bells_requires_the_live_area_owner() {
    let (mut factory, spawn) = bell_fixture(175885, None);
    assert_eq!(
        World::new().load(&mut factory, spawn).unwrap_err().id,
        "execution.gameobject.Object_1234.GetAreaId_owner"
    );
}

#[test]
fn gameobject_bells_refuses_event_dispatch() {
    let (mut factory, spawn) = bell_fixture(175885, Some([1497; 2]));
    let handle = loaded(World::new().load(&mut factory, spawn).unwrap());
    let ai = factory.objects.get(handle).unwrap().ai.as_ref().unwrap();
    assert_eq!(
        ai.on_event_happened(1024, true, false).unwrap_err().id,
        "execution.gameobject.go_scripts_191.OnEventHappened_owner"
    );
}

#[test]
fn gameobject_bells_refuses_update_dispatch() {
    let (mut factory, spawn) = bell_fixture(175885, Some([1497; 2]));
    let handle = loaded(World::new().load(&mut factory, spawn).unwrap());
    let ai = factory.objects.get(handle).unwrap().ai.as_ref().unwrap();
    assert_eq!(
        ai.update_ai(100).unwrap_err().id,
        "execution.gameobject.go_scripts_206.UpdateAI_owner"
    );
}

#[test]
fn gameobject_bell_event_membership_follows_world_lifetime() {
    let (mut factory, spawn) = bell_fixture(175885, Some([1497; 2]));
    let mut world = World::new();
    let first = loaded(world.load(&mut factory, spawn.clone()).unwrap());
    factory.bell_areas = Some([1497; 2].into());
    let second = loaded(
        world
            .load(
                &mut factory,
                GameObjectSpawn {
                    database_guid: spawn.database_guid + 1,
                    ..spawn
                },
            )
            .unwrap(),
    );
    assert_eq!(
        world
            .objects
            .event_notification_member(map())
            .unwrap_err()
            .id,
        "execution.map.Map_2983.event_notify_pointer_order_owner"
    );
    world.objects.remove_from_world(second).unwrap();
    assert!(world.objects.get(second).unwrap().notify_on_event);
    assert!(!world.objects.event_notified(map(), second));
    assert_eq!(
        world.objects.event_notification_member(map()).unwrap(),
        Some(first)
    );
    world.objects.add_to_world(second).unwrap();
    assert!(world.objects.event_notified(map(), second));
    world
        .objects
        .set_notify_on_event_state(second, false)
        .unwrap();
    assert!(!world.objects.event_notified(map(), second));
    assert!(!world.objects.get(second).unwrap().notify_on_event);
}
