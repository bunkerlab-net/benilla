// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored composition only. No sealed execution, live F5 callbacks or F6 geometry.
use super::*;
use benilla_sim::resident_map::CellPair;
use std::sync::Arc;

fn map(id: u32) -> MapIdentity {
    MapIdentity {
        map: id,
        instance: 0,
        creation_generation: u64::from(id) + 1,
    }
}
fn key(guid: u32) -> SpawnKey {
    SpawnKey {
        kind: GridSpawnKind::Creature,
        guid,
    }
}
#[derive(Default)]
struct Work {
    calls: Vec<&'static str>,
    startup: Vec<ManagerStartupPhase>,
    map_startup: Vec<MapManagerStartupPhase>,
    create_on_players: bool,
    created: Option<ObjectHandle>,
    reject: Option<MissingManagerOwner>,
    dynamic_models: SourceDynamicModels,
    dynamic_maps: BTreeMap<MapIdentity, SourceDynamicMap>,
}
impl ManagerStartupWork for Work {
    fn startup(&mut self, phase: ManagerStartupPhase) -> Result<()> {
        self.startup.push(phase);
        if phase == ManagerStartupPhase::MapSystem {
            for id in [0, 1] {
                SourceMapManagerStartup::new(map(id))?.run(ManagerStart::Fresh, self)?;
            }
        }
        Ok(())
    }
    fn initialize_map(&mut self, _: MapIdentity, phase: MapManagerStartupPhase) -> Result<()> {
        self.map_startup.push(phase);
        Ok(())
    }
}
impl PersistentEffects for Work {
    fn sql(&mut self, _: PersistentSql) -> Result<()> {
        Ok(())
    }
    fn query_respawns(&mut self, _: GridSpawnKind) -> Result<CharacterQueryStatus> {
        Err(refuse("authored.unexpected_respawn_query"))
    }
    fn observe(&mut self, _: PersistentOperation) -> Result<()> {
        Ok(())
    }
    fn dungeon_reset(&mut self, _: PersistentMapEntry) -> Result<i64> {
        Err(refuse("authored.unexpected_dungeon"))
    }
}
impl SpawnWork for Work {
    fn thread(&self) -> u64 {
        1
    }
    fn wall_seconds(&mut self, _: SpawnClockCall) -> Result<ClockSample> {
        Ok(ClockSample::authored(ClockKind::WallSeconds, 100))
    }
    fn world_clock_millis(&mut self) -> Result<i64> {
        Ok(100_000)
    }
    fn world_game_seconds(&mut self) -> Result<i64> {
        Ok(100)
    }
    fn dynamic_guids(&mut self, _: u32, _: GridSpawnKind) -> Result<Arc<[u32]>> {
        Ok(Arc::from([]))
    }
    fn definition(&mut self, _: SpawnKey) -> Result<Option<DynamicSpawnDefinition>> {
        Ok(Some(DynamicSpawnDefinition {
            map: 0,
            cell: CellPair::from_indices(512, 512).unwrap(),
            spawn_mask: 1,
        }))
    }
    fn linking_can_spawn(&mut self, _: u32) -> Result<bool> {
        Ok(true)
    }
    fn groups(&mut self) -> Result<Arc<BTreeMap<u32, DynamicSpawnGroup>>> {
        Ok(Arc::new(BTreeMap::new()))
    }
    fn create_group(&mut self, _: DynamicSpawnGroup) -> Result<SpawnGroupHandle> {
        Err(refuse("authored.unexpected_group"))
    }
    fn update_group(
        &mut self,
        _: &mut SourceSpawnManager,
        _: &mut SourceManagers,
        _: SpawnGroupHandle,
    ) -> Result<()> {
        Err(refuse("authored.unexpected_group"))
    }
    fn respawn_group_in_vicinity(
        &mut self,
        _: &mut SourceSpawnManager,
        _: &mut SourceManagers,
        _: SpawnGroupHandle,
        _: [u32; 4],
        _: u32,
    ) -> Result<()> {
        Err(refuse("authored.unexpected_group"))
    }
    fn construct(
        &mut self,
        _: &mut SourceSpawnManager,
        _: &mut SourceManagers,
        _: DynamicSpawnRequest,
    ) -> Result<bool> {
        self.calls.push("construct");
        Ok(true)
    }
    fn spawn_operation(&mut self, _: MapIdentity, operation: SpawnOperation) -> Result<()> {
        if matches!(operation, SpawnOperation::Update { begin: true }) {
            self.calls.push("spawn");
        }
        Ok(())
    }
}
impl EventMapOwner for Work {
    fn event_definition(&mut self, key: SpawnKey) -> Result<Option<EventSpawnDefinition>> {
        self.calls.push("message");
        Ok(Some(EventSpawnDefinition {
            map: 0,
            cell: CellPair::from_indices(512, 512).unwrap(),
            entry: 1,
            guid: u64::from(key.guid),
        }))
    }
    fn event_grid_loaded(&mut self, _: MapIdentity, _: CellPair) -> Result<bool> {
        Ok(true)
    }
    fn event_construct(
        &mut self,
        request: DynamicSpawnRequest,
        spawn: &mut SourceSpawnManager,
        states: &mut SourceManagers,
    ) -> Result<bool> {
        // An authored constructor callback schedules another source-owned spawn.
        spawn.add(states, request.key, self)?;
        Ok(true)
    }
    fn event_lookup(&mut self, _: MapIdentity, _: SpawnKey) -> Result<Option<ObjectHandle>> {
        Ok(None)
    }
    fn event_remove(
        &mut self,
        _: EventLiveRequest,
        _: &mut SourceSpawnManager,
        _: &mut SourceManagers,
    ) -> Result<()> {
        Err(refuse("authored.unexpected_live_event_removal"))
    }
}
impl EventCreatureOwner for Work {}
impl MoTransportOwner for Work {
    fn transport_template(&mut self, _: u32) -> Result<Option<GameObjectTemplate>> {
        Err(refuse("authored.unexpected_transport"))
    }
    fn transport_allocation(&mut self, _: ObjectHandle, _: &mut SourceObjects) -> Result<()> {
        Err(refuse("authored.unexpected_transport"))
    }
}
impl SourceVisitInputs for Work {
    fn visibility_distance(&mut self, _: ObjectHandle) -> Result<f32> {
        Ok(0.0)
    }
    fn farsight_guid(&mut self, _: ObjectHandle) -> Result<SourceGuid> {
        Ok(SourceGuid::from_raw(0))
    }
}
impl SourceObjectUpdateOwner for Work {
    fn update(
        &mut self,
        object: ObjectHandle,
        _: u32,
        objects: &mut SourceObjects,
        _: &mut SourceCells,
    ) -> Result<()> {
        self.calls.push("object");
        objects.set_pose(object, SourcePose([0, 0, 1.0_f32.to_bits(), 0]))
    }
}
impl WeatherPacketOwner for Work {
    fn zone_id(&mut self, _: ObjectHandle, _: &SourceObjects) -> Result<u32> {
        Ok(7)
    }
    fn send_weather(&mut self, _: ObjectHandle, _: [u8; 13]) -> Result<()> {
        self.calls.push("weather_packet");
        Ok(())
    }
}
impl SourceMapEndWork for Work {
    fn map_end_phase(
        &mut self,
        phase: MapEndPhase,
        _: MapIdentity,
        _: u32,
        _: &mut SourceObjects,
    ) -> Result<()> {
        self.calls.push(match phase {
            MapEndPhase::ObjectFields => "fields",
            MapEndPhase::GridStates => "grids",
            MapEndPhase::Scripts => "scripts",
            MapEndPhase::Instance => "instance",
        });
        Ok(())
    }
}
impl MapManagerWork for Work {
    fn calendar(&mut self, _: MapIdentity) -> Result<()> {
        self.calls.push("calendar");
        Ok(())
    }
    fn dynamic_geometry(
        &mut self,
        map: MapIdentity,
    ) -> Result<(&mut SourceDynamicMap, &SourceDynamicModels)> {
        if self.reject == Some(MissingManagerOwner::F6DynamicCollisionTree) {
            return Err(MissingManagerOwner::F6DynamicCollisionTree.refusal());
        }
        self.calls.push("collision");
        Ok((
            self.dynamic_maps
                .entry(map)
                .or_insert_with(|| SourceDynamicMap::new(map)),
            &self.dynamic_models,
        ))
    }
    fn sessions(&mut self, _: &mut MapManagerContext<'_>, _: u32) -> Result<()> {
        if self.reject == Some(MissingManagerOwner::F5MapSessions) {
            return Err(MissingManagerOwner::F5MapSessions.refusal());
        }
        self.calls.push("sessions");
        Ok(())
    }
    fn players(&mut self, context: &mut MapManagerContext<'_>, _: u32) -> Result<()> {
        self.calls.push("players");
        if self.create_on_players {
            self.create_on_players = false;
            self.created = Some(actor(
                context.objects,
                context.cells,
                SourceObjectKind::GameObject,
            ));
        }
        Ok(())
    }
    fn begin_map_message(&mut self, _: &MessageEnvelope) -> Result<()> {
        Ok(())
    }
}
struct NoWeather;
impl benilla_sim::source_weather::Random for NoWeather {
    type Error = Refusal;
    fn urand(&mut self, _: u32, _: u32) -> Result<u32> {
        panic!("unexpected weather draw")
    }
    fn rand_norm_f(&mut self) -> Result<f32> {
        panic!("unexpected weather draw")
    }
}
impl benilla_sim::source_weather::Context for NoWeather {
    fn year_day(&mut self) -> Result<u32> {
        panic!("unexpected calendar read")
    }
}
fn actor(
    objects: &mut SourceObjects,
    cells: &mut SourceCells,
    kind: SourceObjectKind,
) -> ObjectHandle {
    let object = objects.construct(1, kind).unwrap();
    let high = if kind == SourceObjectKind::Player {
        HighGuid::Player
    } else {
        HighGuid::GameObject
    };
    objects
        .assign_guid(
            object,
            0,
            SourceGuid::with_entry(high, 0, object.construction as u32),
        )
        .unwrap();
    objects.bind_map(object, cells.map(), cells).unwrap();
    objects
        .bind_source_allocation(object, object.construction * 256, 128, 0)
        .unwrap();
    objects.set_pose(object, SourcePose([0; 4])).unwrap();
    let pair = SourcePose([0; 4]).cell().unwrap();
    let grid = cells.ensure_grid(pair.grid_x(), pair.grid_y()).unwrap();
    cells.set_object_data_loaded(grid).unwrap();
    objects
        .link_cell(object, cells.cell(pair).unwrap(), cells)
        .unwrap();
    objects.add_to_world(object).unwrap();
    if kind == SourceObjectKind::Player {
        objects.register_player_lookup(object).unwrap();
        objects.link_map_player(object).unwrap();
    }
    object
}
struct Fixture {
    spawn: SourceSpawnManager,
    persistent: SourceManagers,
    messages: SourceMessager,
    transports: SourceMoTransports,
    templates: SourceTransportTemplates,
    weather: SourceMapWeather,
    objects: SourceObjects,
    cells: SourceCells,
    dispatch: SourceMapManagerDispatch,
}
impl Fixture {
    fn new(id: u32) -> (Self, Work) {
        let mut work = Work::default();
        let mut persistent = SourceManagers::default();
        let state = persistent
            .create_state(
                PersistentMapEntry { id, map_type: 0 },
                0,
                0,
                false,
                true,
                0,
                &mut work,
            )
            .unwrap();
        persistent
            .bind_map(state, Some(map(id)), &mut work)
            .unwrap();
        let mut spawn = SourceSpawnManager::new(map(id), state).unwrap();
        spawn.initialize(&mut persistent, &mut work).unwrap();
        let templates = SourceTransportTemplates {
            templates_loaded: true,
            ..Default::default()
        };
        let mut objects = SourceObjects::default();
        let mut cells = SourceCells::new(map(id)).unwrap();
        cells.configure_grids(60_000, true).unwrap();
        let mut transports = SourceMoTransports::default();
        transports
            .initialize_map(
                &templates,
                &mut MoTransportContext {
                    objects: &mut objects,
                    cells: &mut cells,
                    owner: &mut work,
                },
                1,
                0,
            )
            .unwrap();
        (
            Self {
                spawn,
                persistent,
                messages: SourceMessager::new(MessageOwner::Map(map(id))),
                transports,
                templates,
                weather: SourceMapWeather::new(map(id), true, 100).unwrap(),
                objects,
                cells,
                dispatch: SourceMapManagerDispatch::default(),
            },
            work,
        )
    }
    fn context(&mut self) -> MapManagerContext<'_> {
        MapManagerContext {
            spawn: &mut self.spawn,
            persistent: &mut self.persistent,
            messages: &mut self.messages,
            transports: &mut self.transports,
            templates: &self.templates,
            weather: &mut self.weather,
            objects: &mut self.objects,
            cells: &mut self.cells,
        }
    }
    fn update(
        &mut self,
        work: &mut Work,
        weather: &mut impl benilla_sim::source_weather::Context<Error = Refusal>,
    ) -> Result<Vec<ManagerDispatchEvent>> {
        let mut dispatch = std::mem::take(&mut self.dispatch);
        let mut events = Vec::new();
        let result = dispatch.update(
            &mut self.context(),
            work,
            weather,
            100,
            100_100,
            0.0,
            &mut |event| {
                events.push(event);
                Ok(())
            },
            &mut |_| Ok(()),
        );
        self.dispatch = dispatch;
        result.map(|()| events)
    }
    fn message(&mut self, id: u64, spawn: bool) {
        self.messages
            .enqueue(MessageEnvelope {
                id,
                cause: 99,
                payload: SourceMessage::MapGameEvent {
                    creatures: vec![7],
                    gameobjects: vec![],
                    spawn,
                },
            })
            .unwrap();
    }
    fn inventory(&self) -> ManagerCompositionInventory {
        // Authored empty definition inventories test foreign-owner joins, not loader evidence.
        let mut pools = SourcePoolManager::default().inventory();
        pools.loaded = true;
        let mut groups = SourceSpawnGroups::default().inventory();
        groups.loaded = true;
        ManagerCompositionInventory {
            persistent: self.persistent.inventory(),
            pools,
            groups,
            events: GameEventInventory::default(),
            corpses: SourceCorpseRegistry::default()
                .inventory(&self.objects)
                .unwrap(),
            event_effects: SourceEventEffects::default().inventory(),
            maps: vec![MapManagerInventory {
                dispatch_stopped: self.dispatch.is_stopped(),
                spawn: self.spawn.inventory(),
                weather: self.weather.inventory(),
                linking: SourceCreatureLinkingHolder::new(self.cells.map())
                    .unwrap()
                    .inventory(),
                messages: self.messages.inventory(),
            }],
        }
    }
}
#[test]
fn startup_dependencies_and_map_initialization_follow_pinned_source() {
    let mut work = Work::default();
    SourceManagerStartup::default()
        .run(ManagerStart::Fresh, &mut work)
        .unwrap();
    assert_eq!(
        work.startup
            .iter()
            .map(|p| p.source_line())
            .collect::<Vec<_>>(),
        [
            970, 1092, 1095, 1098, 1101, 1109, 1120, 1123, 1127, 1130, 1134, 1137, 1158, 1196,
            1333, 1345, 1389, 1416
        ]
    );
    let expected = [190, 192, 194, 196, 198, 200, 202, 214, 216, 218];
    assert_eq!(
        work.map_startup
            .iter()
            .map(|p| p.source_line())
            .collect::<Vec<_>>(),
        expected.repeat(2)
    );
}
#[test]
fn continuation_and_repeated_initialization_never_restart_managers() {
    let mut work = Work::default();
    let mut startup = SourceManagerStartup::default();
    assert_eq!(
        startup
            .run(ManagerStart::Continuation, &mut work)
            .unwrap_err(),
        MissingManagerOwner::F7Continuation.refusal()
    );
    assert!(work.startup.is_empty());
    assert_eq!(
        startup.run(ManagerStart::Fresh, &mut work).unwrap_err().id,
        "execution.managers.startup_repeated"
    );
    let mut map_start = SourceMapManagerStartup::new(map(0)).unwrap();
    assert_eq!(
        map_start
            .run(ManagerStart::Continuation, &mut work)
            .unwrap_err(),
        MissingManagerOwner::F7Continuation.refusal()
    );
    assert!(work.map_startup.is_empty());
    assert_eq!(
        map_start
            .run(ManagerStart::Fresh, &mut work)
            .unwrap_err()
            .id,
        "execution.managers.map_startup_repeated"
    );
    let mut startup = SourceManagerStartup::default();
    startup.run(ManagerStart::Fresh, &mut work).unwrap();
    let calls = work.startup.len();
    assert!(startup.run(ManagerStart::Fresh, &mut work).is_err());
    assert_eq!(work.startup.len(), calls);
}
#[test]
fn messages_mutate_spawn_owner_before_spawn_and_transport_phases() {
    let (mut f, mut work) = Fixture::new(0);
    f.message(1, true);
    let events = f.update(&mut work, &mut NoWeather).unwrap();
    assert_eq!(
        work.calls[..5],
        ["calendar", "collision", "message", "spawn", "construct"]
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| e.complete)
            .map(|e| e.phase.source_line())
            .collect::<Vec<_>>(),
        [702, 712, 714, 715, 725, 743, 823, 894, 995, 1007, 1011, 1024, 1027, 1030]
    );
    assert!(f.spawn.inventory().active.is_empty());
    assert_eq!(f.messages.pending(), 0);
    f.spawn.add(&f.persistent, key(7), &mut work).unwrap();
    let attempts = work.calls.iter().filter(|&&c| c == "construct").count();
    f.message(2, false);
    f.update(&mut work, &mut NoWeather).unwrap();
    assert_eq!(
        work.calls.iter().filter(|&&c| c == "construct").count(),
        attempts
    );
}
#[test]
fn player_callback_creates_collected_owner_before_objects_grid_scripts_and_weather() {
    let (mut f, mut work) = Fixture::new(0);
    actor(&mut f.objects, &mut f.cells, SourceObjectKind::Player);
    work.create_on_players = true;
    let events = f.update(&mut work, &mut NoWeather).unwrap();
    assert!(work
        .calls
        .windows(5)
        .any(|c| c == ["object", "fields", "grids", "scripts", "instance"]));
    assert_eq!(
        f.objects.pose(work.created.unwrap()).unwrap().0[2],
        1.0_f32.to_bits()
    );
    assert_eq!(events.last().unwrap().phase, MapManagerPhase::Weather);
}
#[test]
fn missing_owners_stop_at_first_reached_operation_without_later_work() {
    for (missing, last) in [
        (MissingManagerOwner::F6DynamicCollisionTree, "calendar"),
        (MissingManagerOwner::F5MapSessions, "spawn"),
    ] {
        let (mut f, mut work) = Fixture::new(0);
        work.reject = Some(missing);
        assert_eq!(
            f.update(&mut work, &mut NoWeather).unwrap_err(),
            missing.refusal()
        );
        assert_eq!(work.calls.last(), Some(&last));
        assert_eq!(
            f.update(&mut work, &mut NoWeather).unwrap_err().id,
            "execution.managers.map_stopped"
        );
    }
}
#[test]
fn comparison_answers_cannot_change_manager_execution() {
    let (mut a, mut wa) = Fixture::new(0);
    let (mut b, mut wb) = Fixture::new(0);
    a.message(1, true);
    b.message(1, true);
    let actual = a.update(&mut wa, &mut NoWeather).unwrap();
    let mut captured = actual.clone();
    captured[4].phase = MapManagerPhase::Weather;
    assert!(DifferentialOracle::compare_manager_dispatch(captured, actual).is_err());
    b.update(&mut wb, &mut NoWeather).unwrap();
    assert_eq!(wa.calls, wb.calls);
    assert_eq!(a.spawn.inventory(), b.spawn.inventory());
}
#[test]
fn first_divergence_and_either_exhaustion_report_exact_ordinal() {
    let (mut f, mut work) = Fixture::new(0);
    let events = f.update(&mut work, &mut NoWeather).unwrap();
    assert_eq!(
        DifferentialOracle::compare_manager_dispatch(events.clone(), events.clone()),
        Ok(28)
    );
    let mut changed = events.clone();
    changed[5].complete = false;
    changed[9].map = map(1);
    let mismatch =
        DifferentialOracle::compare_manager_dispatch(events.clone(), changed).unwrap_err();
    assert_eq!(mismatch.ordinal, 5);
    assert_eq!(mismatch.reason.id, "execution.managers.dispatch_divergence");
    for (expected, actual) in [
        (events[..27].to_vec(), events.clone()),
        (events.clone(), events[..27].to_vec()),
    ] {
        let mismatch = DifferentialOracle::compare_manager_dispatch(expected, actual).unwrap_err();
        assert_eq!(mismatch.ordinal, 27);
        assert_eq!(mismatch.reason.id, "execution.managers.dispatch_exhaustion");
    }
}
#[test]
fn cross_manager_inventory_rejects_stale_omitted_and_foreign_owners() {
    let (f, _) = Fixture::new(0);
    f.inventory()
        .validate(&f.objects, &f.transports, &SourceFormations::default())
        .unwrap();
    let mut state = f.inventory();
    state.maps[0].weather.map = map(1);
    assert_eq!(
        state
            .validate(&f.objects, &f.transports, &SourceFormations::default())
            .unwrap_err()
            .id,
        "execution.managers.map_inventory"
    );
    let mut state = f.inventory();
    state.maps.clear();
    assert_eq!(
        state
            .validate(&f.objects, &f.transports, &SourceFormations::default())
            .unwrap_err()
            .id,
        "execution.managers.omitted_map_owner"
    );
    let mut state = f.inventory();
    state.maps[0].spawn.groups.insert(
        9,
        SpawnGroupHandle {
            id: 9,
            generation: 1,
        },
    );
    assert_eq!(
        state
            .validate(&f.objects, &f.transports, &SourceFormations::default())
            .unwrap_err()
            .id,
        "execution.managers.spawn_group_owner"
    );
    let mut state = f.inventory();
    state.events.active.insert(7);
    assert_eq!(
        state
            .validate(&f.objects, &f.transports, &SourceFormations::default())
            .unwrap_err()
            .id,
        "execution.managers.event_foreign_owner"
    );
    let mut state = f.inventory();
    state.maps[0].messages.owner = MessageOwner::Map(map(1));
    assert_eq!(
        state
            .validate(&f.objects, &f.transports, &SourceFormations::default())
            .unwrap_err()
            .id,
        "execution.managers.map_inventory"
    );
    assert_eq!(
        f.inventory().require_durable_checkpoint().unwrap_err(),
        MissingManagerOwner::F7Continuation.refusal()
    );
}
#[test]
fn cross_map_context_refuses_before_any_effect() {
    let (mut f, mut work) = Fixture::new(0);
    f.weather = SourceMapWeather::new(map(1), true, 100).unwrap();
    assert_eq!(
        f.update(&mut work, &mut NoWeather).unwrap_err().id,
        "execution.managers.cross_map_binding"
    );
    assert!(work.calls.is_empty());
}
#[test]
fn remote_map_weather_consumes_its_actual_logical_thread_draws() {
    use benilla_sim::source_rng::{DrawCursor, Mt19937, SourceRandom};
    let (mut f, mut work) = Fixture::new(1);
    let mut definitions = SourceWeatherDefinitions::default();
    definitions.insert(WeatherChanceRow {
        ordinal: 1,
        zone: 7,
        seasons: [[100, 0, 0]; 4],
    });
    f.weather
        .set_weather(7, 0, 0.0, false, &definitions, &f.objects, &mut work)
        .unwrap();
    let mut engine = Mt19937::from_seed(1234);
    let mut cursor = DrawCursor::default();
    let mut draws = Vec::new();
    {
        let mut sink = |event| draws.push(event);
        let mut random =
            SourceRandom::new(work.thread(), &mut engine, &mut cursor, &mut sink).unwrap();
        let mut weather = WeatherExecution {
            random: &mut random,
            world_calendar: &Calendar::new(CalendarOwner::World, 0),
        };
        f.update(&mut work, &mut weather).unwrap();
    }
    assert!(!draws.is_empty());
    assert!(work.calls.contains(&"spawn"));
    assert!(!work.calls.contains(&"object"));
}

#[test]
fn unbound_startup_names_each_missing_callback_and_collision_owner() {
    struct Unbound;
    impl ManagerStartupWork for Unbound {}
    let mut owner = Unbound;
    assert_eq!(
        owner.startup(ManagerStartupPhase::Scripts).unwrap_err(),
        MissingManagerOwner::F5ScriptInitialization.refusal()
    );
    assert_eq!(
        owner
            .initialize_map(map(0), MapManagerStartupPhase::InstanceData)
            .unwrap_err(),
        MissingManagerOwner::F5InstanceInitialization.refusal()
    );
    assert_eq!(
        owner
            .initialize_map(map(0), MapManagerStartupPhase::Navigation)
            .unwrap_err(),
        MissingManagerOwner::F6Navigation.refusal()
    );
    assert_eq!(
        SourceManagerStartup::default()
            .run(ManagerStart::Fresh, &mut owner)
            .unwrap_err(),
        MissingManagerOwner::F3SourceCutover.refusal()
    );
}

#[test]
fn unloaded_pool_selections_and_pending_message_references_are_validated() {
    let (f, _) = Fixture::new(0);
    let mut state = f.inventory();
    state.persistent.states[0]
        .pools
        .add(PoolMember::Creature(77), 4);
    assert_eq!(
        state
            .validate(&f.objects, &f.transports, &SourceFormations::default())
            .unwrap_err()
            .id,
        "execution.managers.selected_pool_owner"
    );
    let mut state = f.inventory();
    state.maps[0].messages.queue.push(MessageEnvelope {
        id: 1,
        cause: 1,
        payload: SourceMessage::AttachTransport {
            transfer: TransportTransfer {
                transport: ObjectHandle {
                    thread: 1,
                    construction: 999,
                },
            },
        },
    });
    state.maps[0].messages.last_id = 1;
    assert_eq!(
        state
            .validate(&f.objects, &f.transports, &SourceFormations::default())
            .unwrap_err()
            .id,
        "execution.managers.transport_transfer_message"
    );
}

#[test]
fn event_allocator_and_formation_foreign_owners_cannot_be_omitted() {
    let (f, _) = Fixture::new(0);
    let mut state = f.inventory();
    state.event_effects.world.last_id = 1;
    assert_eq!(
        state
            .validate(&f.objects, &f.transports, &SourceFormations::default())
            .unwrap_err()
            .id,
        "execution.managers.event_message_owner"
    );
    let mut groups = SourceSpawnGroups::default().inventory();
    groups.states.push(SpawnGroupState {
        identity: GroupIdentity {
            map: map(0),
            handle: SpawnGroupHandle {
                id: 9,
                generation: 1,
            },
        },
        persistent: f.spawn.persistent(),
        objects: BTreeMap::new(),
        chosen_entries: BTreeMap::new(),
        chosen_spawns: BTreeMap::new(),
        chosen_squad: -1,
        enabled: true,
        cooldown_millis: 0,
        constructing: false,
        has_formation: true,
        linkage_targets: Default::default(),
    });
    assert_eq!(
        SourceFormations::default()
            .validate_manager_owners(&groups, &f.objects)
            .unwrap_err()
            .id,
        "execution.managers.group_formation_owner"
    );
}

#[test]
fn default_map_collision_dispatch_runs_the_real_tree_timer() {
    let (mut models, mut dynamic, mut vmap) = dynamic_models::tests::fixture();
    let object = ObjectHandle {
        thread: 1,
        construction: 1,
    };
    models.bind_allocation(object, 0x1000, 128).unwrap();
    models
        .operation(
            &mut dynamic,
            &mut vmap,
            dynamic_models::tests::object(object),
            GameObjectModelOperation::ConstructAndInsert,
        )
        .unwrap();
    let mut work = Work {
        dynamic_models: models,
        dynamic_maps: BTreeMap::from([(map(0), dynamic)]),
        ..Default::default()
    };
    work.collision_tree(map(0), 199).unwrap();
    assert_eq!(work.dynamic_maps[&map(0)].tree.timer_ms(), 1);
    assert_eq!(work.dynamic_maps[&map(0)].tree.unbalanced(), 1);
    work.collision_tree(map(0), 1).unwrap();
    assert_eq!(work.dynamic_maps[&map(0)].tree.timer_ms(), 200);
    assert_eq!(work.dynamic_maps[&map(0)].tree.unbalanced(), 0);
    assert_eq!(work.calls, ["collision", "collision"]);
}
