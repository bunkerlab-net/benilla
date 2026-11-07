// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored F5/F6 adapters test source calls and real F3/F4 owners, not production geometry.
use super::*;
use std::collections::BTreeSet;
type Op = GameObjectModelOperation;
type Phase = GameObjectLifecyclePhase;
#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    Model(ObjectHandle, Op),
    Callback(ObjectHandle, GameObjectCallback),
    Phase(ObjectHandle, Phase),
    Wall(GameObjectLifecycleClock),
    Spawn(SpawnOperation),
    Sql(PersistentSql),
    Removal(ResidencyEvent),
    Cleanup(ObjectHandle, ResidencyPhase),
    Relocation(RelocationPhase),
    GroupDelay,
    ClearGroup,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Tree {
    Construct(ObjectHandle, u64, u32),
    Insert(ObjectHandle, u64),
    Remove(ObjectHandle, u64),
    Relocate(ObjectHandle, u64, [u32; 4]),
    Destroy(ObjectHandle, u64),
}
struct Model {
    generation: u64,
    inserted: bool,
    enabled: bool,
    position: [u32; 4],
}
pub(super) struct HostState {
    events: Vec<Event>,
    tree: Vec<Tree>,
    models: BTreeMap<ObjectHandle, Model>,
    definitions: BTreeSet<u32>,
    next_model: u64,
    refuse_model: Option<Op>,
    refuse_phase: Option<Phase>,
    wall: i64,
    save_wall: i64,
    clock_ms: i64,
    immediate: bool,
    pools: BTreeMap<u32, u32>,
    group_delay: Option<u32>,
    pending_events: usize,
    owner_combat: bool,
    world: Option<World>,
}
impl Default for HostState {
    fn default() -> Self {
        Self {
            events: vec![],
            tree: vec![],
            models: BTreeMap::new(),
            definitions: BTreeSet::from([123, 124]),
            next_model: 1,
            refuse_model: None,
            refuse_phase: None,
            wall: 101,
            save_wall: 102,
            clock_ms: 100_500,
            immediate: true,
            pools: BTreeMap::new(),
            group_delay: None,
            pending_events: 0,
            owner_combat: false,
            world: None,
        }
    }
}
impl Factory {
    fn life(&mut self) -> &mut HostState {
        self.lifecycle.as_mut().unwrap()
    }
    pub(super) fn lifecycle_callback(
        &mut self,
        object: ObjectHandle,
        callback: GameObjectCallback,
    ) {
        self.life().events.push(Event::Callback(object, callback));
    }
    pub(super) fn lifecycle_model(
        &mut self,
        object: ObjectHandle,
        op: Op,
        c: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<bool> {
        if c.objects.get(object)?.source_allocation.is_none() {
            c.objects
                .bind_source_allocation(object, object.construction * 256, 128, 0)?;
        }
        let go = self.objects.get(object)?;
        let (position, display) = (go.position_bits, go.display);
        let life = self.life();
        life.events.push(Event::Model(object, op));
        if life.refuse_model == Some(op) {
            return Err(GameObjectLifecyclePending::DynamicModel.refusal());
        }
        match op {
            Op::ConstructAndInsert => {
                if !life.definitions.contains(&display) {
                    return Ok(false);
                }
                assert!(!life.models.contains_key(&object));
                let generation = life.next_model;
                life.next_model += 1;
                life.models.insert(
                    object,
                    Model {
                        generation,
                        inserted: true,
                        enabled: false,
                        position,
                    },
                );
                life.tree.push(Tree::Construct(object, generation, display));
                life.tree.push(Tree::Insert(object, generation));
            }
            Op::Contains => return Ok(life.models.get(&object).unwrap().inserted),
            Op::Insert => {
                let model = life.models.get_mut(&object).unwrap();
                model.inserted = true;
                life.tree.push(Tree::Insert(object, model.generation));
            }
            Op::Remove => {
                let model = life.models.get_mut(&object).unwrap();
                assert!(model.inserted);
                model.inserted = false;
                life.tree.push(Tree::Remove(object, model.generation));
            }
            Op::Relocate => {
                let model = life.models.get_mut(&object).unwrap();
                assert!(!model.inserted);
                model.position = position;
                life.tree
                    .push(Tree::Relocate(object, model.generation, position));
            }
            Op::Enable(enabled) => life.models.get_mut(&object).unwrap().enabled = enabled,
            Op::Destroy => {
                let model = life.models.remove(&object).unwrap();
                life.tree.push(Tree::Destroy(object, model.generation));
            }
        }
        Ok(true)
    }
}
impl ResidencyProductOwner for Products {
    fn release_grid_products(&mut self, _: MapIdentity, _: ProductCoordinates) -> Result<()> {
        Ok(())
    }
}
impl ActiveRelocationOwner<Definitions, Products> for Factory {
    fn relocation_phase(
        &mut self,
        _: ObjectHandle,
        phase: RelocationPhase,
        _: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        self.life().events.push(Event::Relocation(phase));
        Ok(())
    }
}
impl ResidencyOwner<Definitions, Products> for Factory {
    fn residency_observe(&mut self, event: ResidencyEvent) -> Result<()> {
        self.life().events.push(Event::Removal(event));
        Ok(())
    }
    fn residency_phase(
        &mut self,
        object: ObjectHandle,
        phase: ResidencyPhase,
        grid: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        self.life().events.push(Event::Cleanup(object, phase));
        if phase == ResidencyPhase::RemoveFromWorld {
            source_gameobject_remove_world_effects(self, object, grid)?;
        }
        if phase == ResidencyPhase::Destructor {
            destroy_source_gameobject_model(self, object, grid)?;
        }
        Ok(())
    }
}
impl PersistentEffects for Factory {
    fn sql(&mut self, operation: PersistentSql) -> Result<()> {
        self.life().events.push(Event::Sql(operation));
        Ok(())
    }
    fn query_respawns(&mut self, _: GridSpawnKind) -> Result<CharacterQueryStatus> {
        panic!("not an initial SQL load")
    }
    fn observe(&mut self, _: PersistentOperation) -> Result<()> {
        Ok(())
    }
    fn dungeon_reset(&mut self, _: PersistentMapEntry) -> Result<i64> {
        panic!("world map")
    }
}
impl SpawnWork for Factory {
    fn thread(&self) -> u64 {
        1
    }
    fn wall_seconds(&mut self, _: SpawnClockCall) -> Result<ClockSample> {
        Ok(ClockSample::authored(
            ClockKind::WallSeconds,
            self.life().wall,
        ))
    }
    fn world_clock_millis(&mut self) -> Result<i64> {
        Ok(self.life().clock_ms)
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
            cell: compute_cell_pair(0, 0).unwrap(),
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
        panic!("no groups")
    }
    fn update_group(
        &mut self,
        _: &mut SourceSpawnManager,
        _: &mut SourceManagers,
        _: SpawnGroupHandle,
    ) -> Result<()> {
        panic!("no groups")
    }
    fn respawn_group_in_vicinity(
        &mut self,
        _: &mut SourceSpawnManager,
        _: &mut SourceManagers,
        _: SpawnGroupHandle,
        _: [u32; 4],
        _: u32,
    ) -> Result<()> {
        panic!("no groups")
    }
    fn construct(
        &mut self,
        manager: &mut SourceSpawnManager,
        p: &mut SourceManagers,
        request: DynamicSpawnRequest,
    ) -> Result<bool> {
        let mut world = self.life().world.take().unwrap();
        let pair = compute_cell_pair(0, 0).unwrap();
        let grid = world
            .cells
            .grids()
            .identity(pair.grid_x(), pair.grid_y())
            .unwrap();
        self.saved = p
            .state(manager.persistent())?
            .respawn(GridSpawnKind::GameObject, 7);
        self.now = self.life().wall;
        let attempt = GridLoadAttempt {
            grid,
            cell: pair,
            kind: GridSpawnKind::GameObject,
            origin: GridSpawnOrigin::Persistent,
            guid: 7,
            catalog_ordinal: 0,
        };
        let result = request.spawn_gameobject(self, attempt, spawn(), &mut world.context());
        self.life().world = Some(world);
        result
    }
    fn spawn_operation(&mut self, _: MapIdentity, operation: SpawnOperation) -> Result<()> {
        self.life().events.push(Event::Spawn(operation));
        Ok(())
    }
}
impl GameObjectLifecycleOwner<Definitions, Products> for Factory {
    fn go_lifecycle_wall(
        &mut self,
        _: ObjectHandle,
        call: GameObjectLifecycleClock,
    ) -> Result<ClockSample> {
        let life = self.life();
        life.events.push(Event::Wall(call));
        Ok(ClockSample::authored(
            ClockKind::WallSeconds,
            if call == GameObjectLifecycleClock::Save {
                life.save_wall
            } else {
                life.wall
            },
        ))
    }
    fn go_save_immediately(&mut self) -> Result<bool> {
        Ok(self.life().immediate)
    }
    fn go_lifecycle_phase(
        &mut self,
        object: ObjectHandle,
        phase: Phase,
        _c: &mut GameObjectLifecycleContext<'_, '_, Definitions, Products>,
    ) -> Result<()> {
        self.life().events.push(Event::Phase(object, phase));
        if self.life().refuse_phase == Some(phase) {
            return Err(phase.missing_owner());
        }
        Ok(())
    }
    fn go_group_delay(
        &mut self,
        _: ObjectHandle,
        _: &mut GameObjectLifecycleContext<'_, '_, Definitions, Products>,
    ) -> Result<Option<u32>> {
        self.life().events.push(Event::GroupDelay);
        Ok(self.life().group_delay)
    }
    fn go_pool(&mut self, database: u32) -> Result<Option<u32>> {
        Ok(self.life().pools.get(&database).copied())
    }
    fn go_pending_events(&mut self, _: ObjectHandle) -> Result<usize> {
        Ok(self.life().pending_events)
    }
    fn go_owner_in_combat(&mut self, _: ObjectHandle) -> Result<bool> {
        Ok(self.life().owner_combat)
    }
    fn go_clear_group(
        &mut self,
        _: ObjectHandle,
        _: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        self.life().events.push(Event::ClearGroup);
        Ok(())
    }
}
struct Fixture {
    world: World,
    factory: Factory,
    object: ObjectHandle,
    persistent: SourceManagers,
    spawns: SourceSpawnManager,
}
impl Fixture {
    fn with(row: GameObjectSpawn, dynamic: bool, configure: impl FnOnce(&mut Factory)) -> Self {
        let mut factory = Factory::new();
        factory.lifecycle = Some(HostState::default());
        factory.forced = dynamic;
        configure(&mut factory);
        let mut world = World::new();
        let database = row.database_guid;
        let pair = compute_cell_pair(row.position_bits[0], row.position_bits[1]).unwrap();
        world.definitions.rows.insert(database, row);
        world
            .sets
            .register(
                pair,
                GridSpawnKind::GameObject,
                GridSpawnOrigin::Static,
                database,
                0,
            )
            .unwrap();
        world
            .context()
            .load_grid(pair, GridLoadTrigger::Enter, &mut factory)
            .unwrap();
        let object = world
            .objects
            .lookup_database(map(), DatabaseLookupKind::GameObject, database)
            .unwrap();
        let mut persistent = SourceManagers::default();
        let id = persistent
            .create_state(
                PersistentMapEntry { id: 0, map_type: 0 },
                0,
                0,
                false,
                true,
                0,
                &mut factory,
            )
            .unwrap();
        if factory.saved != 0 {
            let initial = factory.saved;
            persistent
                .save_respawn(
                    id,
                    GridSpawnKind::GameObject,
                    database,
                    initial,
                    100,
                    &mut factory,
                )
                .unwrap();
        }
        persistent.bind_map(id, Some(map()), &mut factory).unwrap();
        let mut spawns = SourceSpawnManager::new(map(), id).unwrap();
        spawns.initialize(&mut persistent, &mut factory).unwrap();
        factory.life().events.clear();
        Self {
            world,
            factory,
            object,
            persistent,
            spawns,
        }
    }
    fn new(dynamic: bool) -> Self {
        Self::with(spawn(), dynamic, |_| {})
    }
    fn apply(
        &mut self,
        action: impl FnOnce(
            &mut Factory,
            ObjectHandle,
            &mut GameObjectLifecycleContext<'_, '_, Definitions, Products>,
        ) -> Result<()>,
    ) -> Result<()> {
        action(
            &mut self.factory,
            self.object,
            &mut GameObjectLifecycleContext {
                grid: &mut self.world.context(),
                persistent: &mut self.persistent,
                spawns: &mut self.spawns,
            },
        )
    }
    fn update(&mut self) {
        self.apply(|o, h, c| update_source_gameobject(o, h, 100, c))
            .unwrap();
    }
    fn deactivate(&mut self) {
        self.factory
            .objects
            .get_mut(self.object)
            .unwrap()
            .loot_state = 3;
        self.update();
    }
    fn go(&self) -> &SourceGameObject {
        self.factory.objects.get(self.object).unwrap()
    }
    fn deadline(&self) -> i64 {
        self.persistent
            .state(self.spawns.persistent())
            .unwrap()
            .respawn(GridSpawnKind::GameObject, 7)
    }
}
#[test]
fn positive_default_expiry_readds_the_same_construction_and_reinitializes_ai() {
    // GameObject.cpp:380-427 clears use data before Map::Add and AIM_Initialize.
    let mut f = Fixture::with(spawn(), false, |o| o.saved = 125);
    let handle = f.object;
    let guid = f.world.objects.get(handle).unwrap().guid;
    let go = f.factory.objects.get_mut(handle).unwrap();
    go.use_times = 4;
    go.first_user = SourceGuid::from_raw(1);
    go.unique_users.insert(SourceGuid::from_raw(1));
    go.skillup_users.insert(SourceGuid::from_raw(1));
    f.factory.life().wall = 124;
    f.update();
    assert_eq!(f.go().respawn_time, 125);
    f.factory.life().events.clear();
    f.factory.life().wall = 125;
    f.update();
    assert_eq!(f.object, handle);
    assert_eq!(f.world.objects.get(handle).unwrap().guid, guid);
    assert_eq!(f.world.objects.get(handle).unwrap().life_generation, 1);
    assert_eq!(f.go().respawn_time, 0);
    // Legacy Update clears only the local deadline; GameObject.cpp:384-426.
    assert_eq!(f.deadline(), 125);
    assert_eq!(f.go().use_times, 0);
    assert_eq!(f.go().first_user.raw(), 0);
    assert!(f.go().unique_users.is_empty() && f.go().skillup_users.is_empty());
    let events = &f.factory.life().events;
    let insert = events
        .iter()
        .position(|e| *e == Event::Model(handle, Op::Insert))
        .unwrap();
    let ai = events
        .iter()
        .position(|e| *e == Event::Callback(handle, GameObjectCallback::InitializeAi))
        .unwrap();
    assert!(insert < ai);
    assert_eq!(
        &events[events.len() - 2..],
        [
            Event::Phase(handle, Phase::AiUpdate(100)),
            Event::Phase(handle, Phase::WorldObjectUpdate(100))
        ]
    );
}
#[test]
fn negative_nondefault_expiry_deactivates_without_revival_or_another_draw() {
    // GameObject.cpp:410-421 returns before the AI/world tail for a static nondefault GO.
    let mut row = spawn();
    row.respawn_min = -30;
    row.respawn_max = -20;
    let mut f = Fixture::with(row, false, |_| {});
    assert!(!f.go().spawned_by_default);
    assert_eq!(f.go().respawn_delay, 30);
    let draws = f.factory.cursor.raw;
    f.factory.objects.get_mut(f.object).unwrap().respawn_time = 101;
    f.update();
    assert_eq!(f.go().loot_state, 3);
    assert_eq!(f.go().respawn_time, 0);
    assert_eq!(f.factory.cursor.raw, draws);
    assert!(!f.factory.life().events.iter().any(|e| matches!(
        e,
        Event::Callback(_, GameObjectCallback::InitializeAi) | Event::Phase(_, Phase::AiUpdate(_))
    )));
    f.deactivate();
    assert_eq!(f.go().respawn_time, 0);
    assert_eq!(f.deadline(), 0);
    assert_eq!(f.factory.cursor.raw, draws);
}
#[test]
fn zero_delay_returns_before_despawn_callbacks_timer_tail_and_override_consumption() {
    // GameObject.cpp:667-677 clears loot and changes loot state, then returns.
    let mut f = Fixture::new(false);
    let value = f.factory.objects.get_mut(f.object).unwrap();
    value.set_respawn_delay(0, true);
    value.forced_despawn = true;
    value.has_loot = true;
    value.delayed_action_timer = 1;
    f.deactivate();
    assert!(!f.go().has_loot);
    assert_eq!(f.go().loot_state, 1);
    assert_eq!(f.go().delayed_action_timer, 1);
    assert!(f.go().respawn_overridden && f.go().forced_despawn);
    assert!(!f.factory.life().events.iter().any(|e| matches!(
        e,
        Event::Phase(
            _,
            Phase::AiJustDespawned
                | Phase::InstanceDespawn
                | Phase::DelayedAction
                | Phase::AiUpdate(_)
        )
    )));
}
#[test]
fn one_shot_override_skips_one_delay_draw_then_source_bounds_redraw() {
    // GameObject.cpp:685-699 retains overrideOnce while clearing the active flag.
    let mut f = Fixture::new(false);
    f.factory
        .objects
        .get_mut(f.object)
        .unwrap()
        .set_respawn_delay(7, true);
    let before = f.factory.cursor.raw;
    f.deactivate();
    assert_eq!(f.go().respawn_time, 108);
    assert!(!f.go().respawn_overridden && f.go().respawn_override_once);
    assert_eq!(f.factory.cursor.raw, before);
    f.deactivate();
    assert_eq!(f.go().respawn_time, 126);
    assert_eq!(f.factory.cursor.raw, before + 1);
}
#[test]
fn group_delay_and_new_system_group_suppression_precede_removal() {
    // GameObject.cpp:690-709: group membership suppresses AddGameObject, not removal.
    let mut f = Fixture::with(spawn(), true, |o| {
        o.group = Some(GameObjectGroupEntry {
            id: 1,
            string_id: 0,
        })
    });
    f.factory.life().group_delay = Some(9);
    let before = f.factory.cursor.raw;
    f.deactivate();
    assert_eq!(f.deadline(), 110);
    assert_eq!(f.go().respawn_time, i64::MAX);
    assert_eq!(f.factory.cursor.raw, before);
    assert!(f.spawns.inventory().active.is_empty());
    assert_eq!(f.world.cells.removals().len(), 1);
    assert!(f.factory.life().events.contains(&Event::GroupDelay));
    assert!(f.factory.life().events.contains(&Event::ClearGroup));
}
#[test]
fn new_system_saves_before_sentinel_schedules_before_cleanup_and_never_updates_pool() {
    // GameObject.cpp:699-719: finite persistence precedes sentinel, schedule and removal.
    let mut f = Fixture::new(true);
    f.factory.life().pools.insert(7, 2);
    f.deactivate();
    assert_eq!(f.deadline(), 126);
    assert_eq!(f.go().respawn_time, i64::MAX);
    assert_eq!(f.spawns.inventory().active[0].deadline_millis, 126_000);
    let events = &f.factory.life().events;
    let save = events
        .iter()
        .position(|e| *e == Event::Sql(PersistentSql::Commit))
        .unwrap();
    let schedule = events
        .iter()
        .position(|e| matches!(e, Event::Spawn(SpawnOperation::Add { .. })))
        .unwrap();
    let cleanup = events
        .iter()
        .position(|e| *e == Event::Cleanup(f.object, ResidencyPhase::CleanupBeforeDelete))
        .unwrap();
    assert!(save < schedule && schedule < cleanup);
    assert!(!events
        .iter()
        .any(|e| matches!(e, Event::Phase(_, Phase::PoolRespawn(_)))));
    assert!(!f.world.objects.get(f.object).unwrap().in_world);
}
#[test]
fn legacy_pool_runs_after_save_and_before_visibility_without_new_schedule() {
    // GameObject.cpp:714-725 keeps the retained GO and finishes visibility after pool work.
    let mut f = Fixture::new(false);
    f.factory.life().pools.insert(7, 2);
    f.deactivate();
    assert_eq!(f.go().respawn_time, 126);
    assert_eq!(f.deadline(), 126);
    assert!(f.spawns.inventory().active.is_empty());
    let e = &f.factory.life().events;
    let save = e
        .iter()
        .position(|e| *e == Event::Sql(PersistentSql::Commit))
        .unwrap();
    let pool = e
        .iter()
        .position(|e| *e == Event::Phase(f.object, Phase::PoolRespawn(2)))
        .unwrap();
    let visibility = e
        .iter()
        .position(|e| *e == Event::Phase(f.object, Phase::Visibility))
        .unwrap();
    assert!(save < pool && pool < visibility);
}
#[test]
fn unsaved_new_deadline_uses_persistent_zero_and_nondefault_save_stays_absent() {
    // GameObject.cpp:702-709,1072 reads the persisted deadline, not the local sentinel.
    let mut f = Fixture::new(true);
    f.factory.life().immediate = false;
    f.deactivate();
    assert_eq!(f.deadline(), 0);
    assert_eq!(f.spawns.inventory().active[0].deadline_millis, 0);
    let mut row = spawn();
    row.respawn_min = -30;
    let mut f = Fixture::with(row, true, |_| {});
    f.deactivate();
    assert_eq!(f.deadline(), 0);
    assert_eq!(f.spawns.inventory().active[0].deadline_millis, 0);
    assert!(f
        .factory
        .life()
        .events
        .contains(&Event::Wall(GameObjectLifecycleClock::Save)));
}
#[test]
fn loot_state_collision_precedes_callback_and_preserves_user_and_equal_state_calls() {
    // GameObject.cpp:2067-2074 calls collision and AI even when the loot state is unchanged.
    let mut f = Fixture::new(false);
    let user = ObjectHandle {
        thread: 1,
        construction: 99,
    };
    for _ in 0..2 {
        f.factory.life().events.clear();
        f.apply(|o, h, c| set_source_gameobject_loot_state(o, h, 1, Some(user), c))
            .unwrap();
        assert_eq!(
            f.factory.life().events,
            [
                Event::Model(f.object, Op::Enable(true)),
                Event::Phase(f.object, Phase::LootStateChange(Some(user)))
            ]
        );
        assert!(f.factory.life().models[&f.object].enabled);
    }
}
#[test]
fn linked_trap_delete_uses_live_guid_lookup_and_real_deferred_removal() {
    // GameObject.cpp:575-579,2204-2206 resolves the retained GUID, not a stale allocation.
    let mut f = Fixture::with(spawn(), false, |o| {
        let t = o.templates.get_mut(&100).unwrap();
        t.kind = 3;
        t.data[3] = 1;
        t.data[7] = 101;
        o.templates.insert(101, template(101, 6));
    });
    let guid = f.go().linked_trap.unwrap();
    let child = f.world.objects.lookup_world_object(map(), guid).unwrap();
    f.deactivate();
    assert_eq!(f.world.cells.removals().len(), 1);
    assert!(f.world.objects.lookup_world_object(map(), guid).is_none());
    assert_eq!(f.go().linked_trap, Some(guid));
    assert!(!f.factory.life().models[&child].inserted);
    let residency = SourceResidency::new(
        map(),
        ResidencyConfig {
            visibility: 0.0,
            save_respawn_immediately: true,
            battleground: false,
        },
    )
    .unwrap();
    residency
        .drain(&mut f.world.context(), &mut f.factory)
        .unwrap();
    f.factory
        .objects
        .forget_destroyed(child, &f.world.objects)
        .unwrap();
    assert!(f.world.objects.get(child).is_err());
    f.deactivate();
    assert_eq!(f.go().linked_trap, Some(guid));
}
#[test]
fn moving_model_removes_relocates_reinserts_before_visibility_and_missing_membership_stays_absent()
{
    // Map.cpp:1211-1216 and GameObject.cpp:2129-2139 order model mutation before visibility.
    let mut f = Fixture::new(false);
    let h = f.object;
    f.factory.life().events.clear();
    let pose = SourcePose([200.0_f32.to_bits(), 0, 0, 0]);
    f.world
        .context()
        .move_gameobject(h, pose, false, &mut f.factory)
        .unwrap();
    let e = &f.factory.life().events;
    let expected = [
        Event::Model(h, Op::Contains),
        Event::Model(h, Op::Remove),
        Event::Model(h, Op::Relocate),
        Event::Model(h, Op::Insert),
        Event::Relocation(RelocationPhase::GameObjectVisibility),
    ];
    assert!(e.ends_with(&expected));
    assert_eq!(f.factory.life().models[&h].position, pose.0);
    remove_source_gameobject_model(&mut f.factory, h, &mut f.world.context()).unwrap();
    f.factory.life().events.clear();
    update_source_gameobject_model_position(&mut f.factory, h, &mut f.world.context()).unwrap();
    assert_eq!(f.factory.life().events, [Event::Model(h, Op::Contains)]);
    add_source_gameobject_model(&mut f.factory, h, &mut f.world.context()).unwrap();
    assert_eq!(
        &f.factory.life().events[1..],
        [
            Event::Model(h, Op::Contains),
            Event::Model(h, Op::Relocate),
            Event::Model(h, Op::Insert)
        ]
    );
}
#[test]
fn exact_model_history_includes_constructor_duplicate_insert_relocation_and_destruction() {
    // GameObject.cpp:125-126,2103-2105,2136-2138 intentionally keeps both initial inserts.
    let mut f = Fixture::new(false);
    let h = f.object;
    let pose = SourcePose([200.0_f32.to_bits(), 0, 0, 0]);
    f.world
        .context()
        .move_gameobject(h, pose, false, &mut f.factory)
        .unwrap();
    f.apply(delete_source_gameobject).unwrap();
    let residency = SourceResidency::new(
        map(),
        ResidencyConfig {
            visibility: 0.0,
            save_respawn_immediately: true,
            battleground: false,
        },
    )
    .unwrap();
    residency
        .drain(&mut f.world.context(), &mut f.factory)
        .unwrap();
    assert_eq!(
        f.factory.life().tree,
        [
            Tree::Construct(h, 1, 123),
            Tree::Insert(h, 1),
            Tree::Insert(h, 1),
            Tree::Remove(h, 1),
            Tree::Relocate(h, 1, pose.0),
            Tree::Insert(h, 1),
            Tree::Remove(h, 1),
            Tree::Destroy(h, 1)
        ]
    );
    f.factory
        .objects
        .forget_destroyed(h, &f.world.objects)
        .unwrap();
}
#[test]
fn display_replacement_removes_destroys_and_constructs_a_new_model_without_changing_template() {
    // GameObject.cpp:2083-2105 replaces the instance model, not the template definition.
    let mut f = Fixture::new(false);
    let h = f.object;
    f.factory.life().events.clear();
    set_source_gameobject_display(&mut f.factory, h, 124, &mut f.world.context()).unwrap();
    assert_eq!(f.go().display, 124);
    assert_eq!(f.go().template.display, 123);
    assert_eq!(
        f.factory.life().events,
        [
            Event::Model(h, Op::Contains),
            Event::Model(h, Op::Remove),
            Event::Model(h, Op::Destroy),
            Event::Model(h, Op::ConstructAndInsert)
        ]
    );
}
#[test]
fn unavailable_model_owner_refuses_at_the_reached_operation_without_ai_or_fake_geometry() {
    let mut f = Fixture::new(false);
    f.factory.life().refuse_model = Some(Op::Enable(true));
    assert_eq!(
        f.apply(|o, h, c| set_source_gameobject_loot_state(o, h, 3, None, c))
            .unwrap_err(),
        GameObjectLifecyclePending::DynamicModel.refusal()
    );
    assert_eq!(
        f.factory.life().events,
        [Event::Model(f.object, Op::Enable(true))]
    );
    assert_eq!(
        f.apply(|o, h, c| update_source_gameobject(o, h, 100, c))
            .unwrap_err()
            .id,
        "execution.gameobject_lifecycle.stopped"
    );
    let mut f = Fixture::new(false);
    f.factory.life().refuse_model = Some(Op::Contains);
    assert_eq!(
        update_source_gameobject_model_position(&mut f.factory, f.object, &mut f.world.context())
            .unwrap_err(),
        GameObjectLifecyclePending::DynamicModel.refusal()
    );
    assert!(f.factory.life().models[&f.object].inserted);
}
#[test]
fn chest_restock_and_nonconsumable_returns_do_not_run_the_common_tail() {
    // GameObject.cpp:625-634,672-674 has two distinct early-return paths.
    for restock in [0, 9] {
        let mut f = Fixture::with(spawn(), false, |o| {
            let t = o.templates.get_mut(&100).unwrap();
            t.kind = 3;
            t.data[2] = restock;
        });
        f.factory
            .objects
            .get_mut(f.object)
            .unwrap()
            .delayed_action_timer = 1;
        f.deactivate();
        assert_eq!(f.go().loot_state, if restock == 0 { 1 } else { 0 });
        assert_eq!(f.go().restock_time, if restock == 0 { 0 } else { 110 });
        assert_eq!(f.go().delayed_action_timer, 1);
        assert_eq!(f.deadline(), 0);
        assert!(!f.factory.life().events.iter().any(|e| matches!(
            e,
            Event::Phase(_, Phase::AiJustDespawned | Phase::AiUpdate(_))
        )));
    }
}
#[test]
fn trap_pending_events_prevent_despawn_but_keep_delayed_action_ai_and_world_tail() {
    // GameObject.cpp:585-590 breaks the loot-state switch rather than returning from Update.
    let mut f = Fixture::with(spawn(), false, |o| {
        o.templates.get_mut(&100).unwrap().kind = 6
    });
    f.factory.life().pending_events = 1;
    f.factory
        .objects
        .get_mut(f.object)
        .unwrap()
        .delayed_action_timer = 1;
    f.deactivate();
    assert_eq!(f.go().loot_state, 3);
    assert_eq!(f.go().delayed_action_timer, 0);
    let e = &f.factory.life().events;
    assert!(e.ends_with(&[
        Event::Phase(f.object, Phase::DelayedAction),
        Event::Phase(f.object, Phase::AiUpdate(100)),
        Event::Phase(f.object, Phase::WorldObjectUpdate(100))
    ]));
}
#[test]
fn forced_permanent_go_draws_once_and_consumes_its_override_at_deactivation() {
    // GameObject.cpp:2722-2730 redraws only the forced zero-delay static spawn.
    let mut f = Fixture::with(spawn(), false, |o| {
        o.templates.get_mut(&100).unwrap().kind = 0
    });
    assert_eq!(f.go().respawn_delay, 0);
    let before = f.factory.cursor.raw;
    f.apply(|o, h, c| force_despawn_source_gameobject(o, h, 0, c))
        .unwrap();
    assert_eq!(f.factory.cursor.raw, before + 1);
    assert!(f.go().forced_despawn && f.go().respawn_overridden && f.go().respawn_override_once);
    f.update();
    assert_eq!(f.factory.cursor.raw, before + 1);
    assert_eq!(f.go().respawn_time, 126);
    assert!(!f.go().forced_despawn && !f.go().respawn_overridden);
}
#[test]
fn direct_respawn_waits_for_update_and_refresh_does_not_override_a_default_timer() {
    // GameObject.cpp:759-769,1160-1165 keeps direct Respawn separate from Map::Add.
    let mut f = Fixture::with(spawn(), false, |o| o.saved = 125);
    f.apply(refresh_source_gameobject).unwrap();
    assert!(f.factory.life().events.is_empty());
    f.apply(respawn_source_gameobject).unwrap();
    assert_eq!(f.go().respawn_time, 101);
    assert_eq!(f.deadline(), 0);
    assert!(!f
        .factory
        .life()
        .events
        .iter()
        .any(|e| matches!(e, Event::Callback(_, GameObjectCallback::InitializeAi))));
    f.update();
    assert_eq!(f.go().respawn_time, 0);
}
#[test]
fn new_system_scheduler_constructs_a_new_go_after_real_removal() {
    // GameObject.cpp:705-712 schedules construction rather than reviving the old GO.
    let mut f = Fixture::new(true);
    let old = f.object;
    let guid = f.world.objects.get(old).unwrap().guid;
    f.deactivate();
    let residency = SourceResidency::new(
        map(),
        ResidencyConfig {
            visibility: 0.0,
            save_respawn_immediately: true,
            battleground: false,
        },
    )
    .unwrap();
    residency
        .drain(&mut f.world.context(), &mut f.factory)
        .unwrap();
    assert!(f.world.objects.get(old).is_err());
    f.factory
        .objects
        .forget_destroyed(old, &f.world.objects)
        .unwrap();
    f.factory.life().clock_ms = 126_000;
    f.factory.life().wall = 126;
    f.factory.life().world = Some(f.world);
    f.spawns.update(&mut f.persistent, &mut f.factory).unwrap();
    let world = f
        .factory
        .lifecycle
        .as_ref()
        .unwrap()
        .world
        .as_ref()
        .unwrap();
    let new = world
        .objects
        .lookup_database(map(), DatabaseLookupKind::GameObject, 7)
        .unwrap();
    assert_ne!(new, old);
    assert_ne!(world.objects.get(new).unwrap().guid, guid);
    assert!(f.spawns.inventory().active.is_empty());
}
#[test]
fn source_object_dispatch_uses_the_constructor_owned_go_and_missing_callback_stops() {
    let Fixture {
        mut world,
        mut factory,
        object,
        mut persistent,
        mut spawns,
    } = Fixture::new(false);
    let mut set = SourceUpdateSet::new(map()).unwrap();
    set.insert(&world.objects, object, UpdateInsertion::Generic)
        .unwrap();
    let mut owner = GameObjectLifecycleDispatch {
        definitions: &mut world.definitions,
        products: &mut world.products,
        sets: &mut world.sets,
        persistent: &mut persistent,
        spawns: &mut spawns,
        owner: &mut factory,
    };
    set.dispatch(
        123,
        &mut world.objects,
        &mut world.cells,
        &mut owner,
        &mut |_| Ok(()),
    )
    .unwrap();
    assert_eq!(
        factory.life().events,
        [
            Event::Phase(object, Phase::Events(123)),
            Event::Phase(object, Phase::AiUpdate(123)),
            Event::Phase(object, Phase::WorldObjectUpdate(123))
        ]
    );
    let mut f = Fixture::new(false);
    f.factory.life().refuse_phase = Some(Phase::AiJustDespawned);
    f.factory.objects.get_mut(f.object).unwrap().loot_state = 3;
    assert_eq!(
        f.apply(|o, h, c| update_source_gameobject(o, h, 100, c))
            .unwrap_err(),
        Phase::AiJustDespawned.missing_owner()
    );
}

#[test]
fn zero_group_redraw_suppresses_schedule_but_forced_despawn_still_removes() {
    // GameObject.cpp:676,690-712 tests zero before redraw and again in the new-system branch.
    for forced in [false, true] {
        let mut f = Fixture::with(spawn(), true, |o| {
            o.group = Some(GameObjectGroupEntry {
                id: 1,
                string_id: 0,
            })
        });
        f.factory.life().group_delay = Some(0);
        f.factory.objects.get_mut(f.object).unwrap().forced_despawn = forced;
        f.deactivate();
        assert_eq!(f.go().respawn_delay, 0);
        assert_eq!(f.go().respawn_time, i64::MAX);
        assert!(f.spawns.inventory().active.is_empty());
        assert_eq!(f.world.cells.removals().len(), usize::from(forced));
    }
}

#[test]
fn nonpositive_respawn_setter_reads_no_wall_and_positive_time_keeps_uint32_delay_cast() {
    // GameObject.h:757-760 truncates the delay field independently of the time_t deadline.
    let mut f = Fixture::new(false);
    for delay in [0, -7] {
        f.apply(|o, h, c| set_source_gameobject_respawn_time(o, h, delay, c))
            .unwrap();
        assert_eq!(f.go().respawn_delay, 0);
        assert_eq!(f.go().respawn_time, 0);
        assert!(f.factory.life().events.is_empty());
    }
    f.apply(|o, h, c| set_source_gameobject_respawn_time(o, h, 4_294_967_296, c))
        .unwrap();
    assert_eq!(f.go().respawn_time, 4_294_967_397);
    assert_eq!(f.go().respawn_delay, 0);
    assert!(!f.go().respawn_overridden);
}

#[test]
fn temporary_nondefault_expiry_queues_delete_and_returns_before_the_update_tail() {
    // GameObject.cpp:410-421 distinguishes a nonstatic wild summon from a DB spawn.
    let mut f = Fixture::new(false);
    let pose = f.world.objects.pose(f.object).unwrap();
    f.world
        .objects
        .set_respawn_definition(f.object, pose, false)
        .unwrap();
    let go = f.factory.objects.get_mut(f.object).unwrap();
    go.set_spell_id(7);
    go.respawn_time = 101;
    f.update();
    assert_eq!(f.world.cells.removals().len(), 1);
    assert!(f
        .factory
        .life()
        .events
        .contains(&Event::Phase(f.object, Phase::RemoveWildOwner)));
    assert!(!f
        .factory
        .life()
        .events
        .iter()
        .any(|e| matches!(e, Event::Phase(_, Phase::AiUpdate(_)))));
}

#[test]
fn trap_arming_and_rearming_use_separate_wall_calls_and_strict_expiry() {
    // GameObject.cpp:321-327,535-546 reads the rearm wall twice and uses strict less-than.
    let mut f = Fixture::with(spawn(), false, |o| {
        let t = o.templates.get_mut(&100).unwrap();
        t.kind = 6;
        t.data[7] = 4;
    });
    f.factory.life().owner_combat = true;
    f.update();
    assert_eq!(f.go().cooldown_time, 105);
    assert_eq!(f.go().loot_state, 1);
    f.factory.objects.get_mut(f.object).unwrap().loot_state = 2;
    f.factory.life().events.clear();
    f.update();
    assert_eq!(f.go().rearm_time, 126);
    assert_eq!(f.go().state, 2);
    let calls: Vec<_> = f
        .factory
        .life()
        .events
        .iter()
        .filter_map(|e| {
            if let Event::Wall(c) = e {
                Some(*c)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        calls,
        [
            GameObjectLifecycleClock::TrapRearmStart,
            GameObjectLifecycleClock::TrapRearmCheck
        ]
    );
    f.factory.life().wall = 126;
    f.update();
    assert_eq!(f.go().state, 2);
    f.factory.life().wall = 127;
    f.update();
    assert_eq!(f.go().state, 1);
    assert_eq!(f.go().rearm_time, 0);
    assert_eq!(f.go().loot_state, 1);
}

#[test]
fn chest_loot_expiry_is_inclusive_and_does_not_run_deactivation_in_the_same_branch() {
    // GameObject.cpp:526-533 writes loot state directly and still updates loot.
    let mut f = Fixture::with(spawn(), false, |o| {
        let t = o.templates.get_mut(&100).unwrap();
        t.kind = 3;
        t.data[3] = 1;
    });
    let go = f.factory.objects.get_mut(f.object).unwrap();
    go.loot_state = 2;
    go.has_loot = true;
    go.corpse_despawn_ms = 100_500;
    f.update();
    assert_eq!(f.go().loot_state, 3);
    assert!(f.go().has_loot);
    assert!(f
        .factory
        .life()
        .events
        .contains(&Event::Phase(f.object, Phase::LootUpdate)));
    assert!(!f
        .factory
        .life()
        .events
        .contains(&Event::Phase(f.object, Phase::WorldStateRevert)));
}

#[test]
fn capture_timer_subtracts_one_interval_without_a_catchup_loop() {
    // GameObject.cpp:558-564 invokes TickCapturePoint once, then subtracts 5000.
    let mut f = Fixture::with(spawn(), false, |o| {
        o.templates.get_mut(&100).unwrap().kind = 29
    });
    f.factory.objects.get_mut(f.object).unwrap().loot_state = 2;
    f.apply(|o, h, c| update_source_gameobject(o, h, 12_000, c))
        .unwrap();
    assert_eq!(f.go().capture_timer, 7000);
    assert_eq!(
        f.factory
            .life()
            .events
            .iter()
            .filter(|e| **e == Event::Phase(f.object, Phase::CaptureTick))
            .count(),
        1
    );
}

#[test]
fn full_deactivation_callback_order_keeps_collision_before_loot_ai_and_visibility_after_save() {
    // GameObject.cpp:573-725 preserves callbacks and early/late source responsibilities.
    let mut f = Fixture::new(false);
    f.deactivate();
    let phases: Vec<_> = f
        .factory
        .life()
        .events
        .iter()
        .filter_map(|e| {
            if let Event::Phase(_, p) = e {
                Some(*p)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        phases,
        [
            Phase::Events(100),
            Phase::WorldStateRevert,
            Phase::DespawnAnimation,
            Phase::DestroyLoot,
            Phase::ClearLootRecipient,
            Phase::LootStateChange(None),
            Phase::AiJustDespawned,
            Phase::InstanceDespawn,
            Phase::Visibility,
            Phase::AiUpdate(100),
            Phase::WorldObjectUpdate(100)
        ]
    );
    assert_eq!(f.factory.cursor.raw, 2);
}

#[test]
fn absent_model_definition_skips_collision_without_substituting_an_empty_dynamic_owner() {
    // GameObject.cpp:2089-2094 skips only an actually absent constructed model.
    let mut f = Fixture::with(spawn(), false, |o| o.life().definitions.clear());
    assert!(!f.go().has_model);
    f.apply(|o, h, c| set_source_gameobject_loot_state(o, h, 3, None, c))
        .unwrap();
    assert_eq!(
        f.factory.life().events,
        [Event::Phase(f.object, Phase::LootStateChange(None))]
    );
    let mut world = World::new();
    let mut factory = Factory::new();
    factory.lifecycle = Some(HostState::default());
    factory.life().refuse_model = Some(Op::ConstructAndInsert);
    assert_eq!(
        world.load(&mut factory, spawn()).unwrap_err(),
        GameObjectLifecyclePending::DynamicModel.refusal()
    );
    assert_eq!(factory.life().events.len(), 1);
    assert!(matches!(
        factory.life().events[0],
        Event::Model(_, Op::ConstructAndInsert)
    ));
}

#[test]
fn retained_seal8_go_identity_projection_matches_but_lifecycle_transitions_skip() {
    let explicit = std::env::var_os("BENILLA_F1_SEAL8_FRAMING");
    let path = explicit
        .as_ref()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(2)
                .unwrap()
                .join("benilla-config/full-server/f1/sealing/source-choice/seal8-full-framing-08")
        });
    if explicit.is_none() && !path.try_exists().unwrap() {
        println!("SKIP unsealed seal8-08 GO lifecycle: set BENILLA_F1_SEAL8_FRAMING");
        return;
    }
    let report = compare_seal8_unsealed_gameobjects(&path).unwrap();
    assert!(report.go_add_counters_matched > 0 && report.go_update_guids_matched > 0);
    assert!(report.first_difference.is_none());
    assert_eq!(report.independently_executed_constructors, 0);
    assert!(!report.reference_execution_sealed && !report.whole_execution_accepted);
    println!("{}", serde_json::to_string(&report).unwrap());
    let (held, raw) = metadata(&path.join("source-sites.json"), 1_048_576).unwrap();
    assert_eq!(
        Sha256Id::of(&raw).as_str(),
        "097b87217694b01c5a42c114358a7f30b2faf2062095640c0e50a4028e739cfa"
    );
    let sites: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    let sites = sites.as_object().unwrap();
    assert_eq!(
        sites
            .values()
            .filter(
                |s| s["callsite"]["source_file"] == "src/game/Entities/GameObject.cpp"
                    && s["callsite"]["function"] == "virtual void GameObject::Update(uint32)"
                    && s["callsite"]["line"] == 743
            )
            .count(),
        1
    );
    assert_eq!(
        sites
            .values()
            .filter(|s| {
                let function = s["callsite"]["function"].as_str().unwrap_or("");
                [
                    "GameObject::Delete(",
                    "GameObject::SetLootState(",
                    "GameObject::Respawn(",
                    "GameObject::SaveRespawnTime(",
                    "GameObject::UpdateModelPosition(",
                ]
                .iter()
                .any(|name| function.contains(name))
            })
            .count(),
        0
    );
    held.check().unwrap();
    println!("SKIP MR/MP/MPR GO lifecycle transition comparison: only GUID and AI-Update call projections exist; timers, collision and live callbacks are unbound");
}

#[test]
fn ready_expiry_reset_door_is_a_noop_when_loot_is_already_ready() {
    // GameObject.cpp:405-407 calls a helper whose GO_READY guard returns at :1339-1340.
    let mut row = spawn();
    row.state = 0;
    let mut f = Fixture::with(row, false, |o| {
        let t = o.templates.get_mut(&100).unwrap();
        t.kind = 0;
        t.data[3] = 1;
        o.saved = 125;
    });
    f.factory.life().wall = 125;
    f.update();
    assert_eq!(f.go().state, 0);
    assert_eq!(f.go().loot_state, 1);
    assert!(!f
        .factory
        .life()
        .events
        .iter()
        .any(|e| matches!(e, Event::Phase(_, Phase::LootStateChange(_)))));
}

#[test]
fn activated_door_autoclose_is_strict_and_toggles_a_ready_state_open() {
    // GameObject.cpp:523-524,1337-1344,1392-1395 preserves the surprising READY toggle.
    let mut f = Fixture::with(spawn(), false, |o| {
        let t = o.templates.get_mut(&100).unwrap();
        t.kind = 0;
        t.data[2] = 65_536;
    });
    let go = f.factory.objects.get_mut(f.object).unwrap();
    go.loot_state = 2;
    go.state = 1;
    go.cooldown_time = 101;
    go.flags |= 1;
    f.update();
    assert_eq!(f.go().loot_state, 2);
    f.factory.life().wall = 102;
    f.update();
    assert_eq!(f.go().loot_state, 3);
    assert_eq!(f.go().state, 0);
    assert_eq!(f.go().cooldown_time, 0);
    assert_eq!(f.go().flags & 1, 0);
    assert!(!f.factory.life().models[&f.object].enabled);
}

#[test]
fn chest_despawn_uses_five_snapshot_minutes_and_refuses_signed_overflow() {
    // GameObject.cpp:754-756,529 uses the World TimePoint, not a fresh time(nullptr).
    let mut f = Fixture::new(false);
    f.apply(set_source_gameobject_chest_despawn).unwrap();
    assert_eq!(f.go().corpse_despawn_ms, 400_500);
    assert!(f.factory.life().events.is_empty());
    f.factory.life().clock_ms = i64::MAX;
    assert_eq!(
        f.apply(set_source_gameobject_chest_despawn).unwrap_err().id,
        "execution.gameobject_lifecycle.timepoint_overflow_undefined"
    );
}

#[test]
fn refresh_readds_a_spawned_parent_and_replaces_only_its_linked_guid() {
    // GameObject.cpp:133-136,765-768 summons again without deleting the previous linked trap.
    let mut f = Fixture::with(spawn(), false, |o| {
        let t = o.templates.get_mut(&100).unwrap();
        t.kind = 3;
        t.data[3] = 1;
        t.data[7] = 101;
        o.templates.insert(101, template(101, 6));
    });
    let first_guid = f.go().linked_trap.unwrap();
    let first = f
        .world
        .objects
        .lookup_world_object(map(), first_guid)
        .unwrap();
    assert!(!f.factory.objects.get(first).unwrap().spawned_by_default);
    assert_eq!(f.factory.objects.get(first).unwrap().spell, 0);
    f.apply(refresh_source_gameobject).unwrap();
    let second_guid = f.go().linked_trap.unwrap();
    let second = f
        .world
        .objects
        .lookup_world_object(map(), second_guid)
        .unwrap();
    assert_ne!(first_guid, second_guid);
    assert!(f.world.objects.get(first).unwrap().in_world);
    f.apply(delete_source_gameobject).unwrap();
    assert!(f.world.objects.get(first).unwrap().in_world);
    assert!(!f.world.objects.get(second).unwrap().in_world);
    assert_eq!(f.world.cells.removals().len(), 2);
}

#[test]
fn later_save_persists_the_local_sentinel_without_rewriting_an_existing_schedule() {
    // GameObject.cpp:705-709,1070-1073 preserves the later SaveRespawnTime sentinel write.
    let mut f = Fixture::new(true);
    f.factory.life().immediate = false;
    f.deactivate();
    assert_eq!(f.deadline(), 0);
    assert_eq!(f.spawns.inventory().active[0].deadline_millis, 0);
    f.apply(save_source_gameobject_respawn).unwrap();
    assert_eq!(f.deadline(), i64::MAX);
    assert_eq!(f.spawns.inventory().active[0].deadline_millis, 0);
}
