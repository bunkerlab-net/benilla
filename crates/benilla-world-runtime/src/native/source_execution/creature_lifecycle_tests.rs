// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored lifecycle hosts exercise real grid/removal/scheduling owners, not F5 acceptance.
use super::*;
use std::sync::Arc;

#[path = "unit_events_lifecycle_tests.rs"]
mod unit_events_lifecycle_tests;

type Phase = CreatureLifecyclePhase;
#[derive(Clone, Debug, PartialEq, Eq)]
enum Call {
    Phase(Phase),
    Wall(CreatureLifecycleClock),
    Snapshot,
    Sql(PersistentSql),
    Spawn(SpawnOperation),
    AiRemoved(u32),
    Relocate(RelocationPhase),
    Removal(ResidencyPhase),
    RemovalEvent(ResidencyEvent),
    GroupDelay,
    Gate,
}
struct Host {
    factory: Factory,
    calls: Vec<Call>,
    clock_ms: i64,
    wall: i64,
    save_wall: i64,
    script_delay: u32,
    group_delay: Option<u32>,
    refuse: Option<Phase>,
    gate: bool,
    die_in_unit: bool,
    construction_world: Option<World>,
    groups: Option<(SourceSpawnGroups, spawn_group_tests::Work<'static>)>,
}
impl Host {
    fn new() -> Self {
        Self {
            factory: Factory::new(),
            calls: vec![],
            clock_ms: 100_500,
            wall: 101,
            save_wall: 102,
            script_delay: 0,
            group_delay: None,
            refuse: None,
            gate: true,
            die_in_unit: false,
            construction_world: None,
            groups: None,
        }
    }
}
impl GridObjectFactory<Definitions, Products> for Host {
    fn load_from_db(
        &mut self,
        a: GridLoadAttempt,
        row: CreatureSpawn,
        c: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<GridCandidateOutcome> {
        self.factory.load_from_db(a, row, c)
    }
    fn viewpoint_added(
        &mut self,
        o: ObjectHandle,
        cell: CellIdentity,
        c: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        self.factory.viewpoint_added(o, cell, c)
    }
    fn battleground_db_load(
        &mut self,
        o: ObjectHandle,
        c: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        self.factory.battleground_db_load(o, c)
    }
    fn cell_corpses(
        &mut self,
        p: CellPair,
        c: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        self.factory.cell_corpses(p, c)
    }
    fn resurrectable_corpses(
        &mut self,
        p: CellPair,
        c: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        self.factory.resurrectable_corpses(p, c)
    }
}
impl ActiveRelocationOwner<Definitions, Products> for Host {
    fn relocation_phase(
        &mut self,
        _: ObjectHandle,
        phase: RelocationPhase,
        _: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        self.calls.push(Call::Relocate(phase));
        Ok(())
    }
}
impl ResidencyOwner<Definitions, Products> for Host {
    fn residency_observe(&mut self, e: ResidencyEvent) -> Result<()> {
        self.calls.push(Call::RemovalEvent(e));
        Ok(())
    }
    fn residency_phase(
        &mut self,
        o: ObjectHandle,
        p: ResidencyPhase,
        c: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        self.calls.push(Call::Removal(p));
        self.factory.residency_phase(o, p, c)
    }
}
impl PersistentEffects for Host {
    fn sql(&mut self, s: PersistentSql) -> Result<()> {
        self.calls.push(Call::Sql(s));
        Ok(())
    }
    fn query_respawns(&mut self, _: GridSpawnKind) -> Result<CharacterQueryStatus> {
        panic!("not an initial load")
    }
    fn observe(&mut self, _: PersistentOperation) -> Result<()> {
        Ok(())
    }
    fn dungeon_reset(&mut self, _: PersistentMapEntry) -> Result<i64> {
        panic!("world map")
    }
}
impl SpawnWork for Host {
    fn thread(&self) -> u64 {
        2
    }
    fn wall_seconds(&mut self, _: SpawnClockCall) -> Result<ClockSample> {
        Ok(ClockSample::authored(ClockKind::WallSeconds, self.wall))
    }
    fn world_clock_millis(&mut self) -> Result<i64> {
        self.calls.push(Call::Snapshot);
        Ok(self.clock_ms)
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
            cell: cell(0.0),
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
        let mut world = self
            .construction_world
            .take()
            .expect("authored map constructor owner");
        self.factory.saved_time = p
            .state(manager.persistent())?
            .respawn(GridSpawnKind::Creature, 40);
        self.factory.now = self.wall;
        let pair = cell(0.0);
        let grid = world
            .cells
            .grids()
            .identity(pair.grid_x(), pair.grid_y())
            .unwrap();
        let attempt = GridLoadAttempt {
            grid,
            cell: pair,
            kind: GridSpawnKind::Creature,
            origin: GridSpawnOrigin::Persistent,
            guid: 40,
            catalog_ordinal: 40,
        };
        let result = request.spawn_creature(
            &mut self.factory,
            attempt,
            spawn(40, 0.0),
            &mut world.context(),
        );
        self.construction_world = Some(world);
        result
    }
    fn spawn_operation(&mut self, _: MapIdentity, o: SpawnOperation) -> Result<()> {
        self.calls.push(Call::Spawn(o));
        Ok(())
    }
}
impl CreatureLifecycleOwner<Definitions, Products> for Host {
    fn lifecycle_random(&mut self) -> Result<&mut dyn CreatureRandom> {
        Ok(&mut self.factory.random)
    }
    fn lifecycle_wall(
        &mut self,
        _: ObjectHandle,
        call: CreatureLifecycleClock,
    ) -> Result<ClockSample> {
        self.calls.push(Call::Wall(call));
        Ok(ClockSample::authored(
            ClockKind::WallSeconds,
            if call == CreatureLifecycleClock::Save {
                self.save_wall
            } else {
                self.wall
            },
        ))
    }
    fn lifecycle_phase(
        &mut self,
        phase: Phase,
        life: &mut SourceCreatureLifecycle,
        c: &mut CreatureLifecycleContext<'_, '_, Definitions, Products>,
    ) -> Result<()> {
        self.calls.push(Call::Phase(phase));
        if self.refuse == Some(phase) {
            return Err(phase.missing_owner());
        }
        if matches!(phase, Phase::ClearGroup | Phase::GroupRespawn) {
            if let Some((groups, work)) = &mut self.groups {
                return groups.creature_lifecycle_phase(phase, life, c, work);
            }
        }
        match phase {
            Phase::UnitUpdate(_) if self.die_in_unit => {
                self.die_in_unit = false;
                life.die(c, self)?;
            }
            Phase::ResetEntry => {
                self.factory
                    .health
                    .insert(life.object(), life.construction.level * 10);
            }
            Phase::MapAdd => {
                add_source_creature(&mut self.factory, &mut life.construction, c.grid)?;
            }
            Phase::SetHealthZero => {
                self.factory.health.insert(life.object(), 0);
            }
            _ => {}
        }
        Ok(())
    }
    fn lifecycle_group_delay(
        &mut self,
        life: &mut SourceCreatureLifecycle,
        context: &mut CreatureLifecycleContext<'_, '_, Definitions, Products>,
    ) -> Result<Option<u32>> {
        self.calls.push(Call::GroupDelay);
        if let Some((groups, work)) = &mut self.groups {
            return groups.lifecycle_respawn_delay(
                context.grid.cells.map(),
                SpawnKey {
                    kind: GridSpawnKind::Creature,
                    guid: life.construction.spawn.database_guid,
                },
                work,
            );
        }
        Ok(self.group_delay)
    }
    fn lifecycle_corpse_removed(
        &mut self,
        _: &mut SourceCreatureLifecycle,
        delay: &mut u32,
        _: &mut CreatureLifecycleContext<'_, '_, Definitions, Products>,
    ) -> Result<()> {
        *delay = self.script_delay;
        self.calls.push(Call::AiRemoved(*delay));
        Ok(())
    }
    fn lifecycle_can_spawn(
        &mut self,
        _: &mut SourceCreatureLifecycle,
        _: &mut CreatureLifecycleContext<'_, '_, Definitions, Products>,
    ) -> Result<bool> {
        self.calls.push(Call::Gate);
        Ok(self.gate)
    }
}
struct Fixture {
    world: World,
    host: Host,
    life: SourceCreatureLifecycle,
    spawns: SourceSpawnManager,
    persistent: SourceManagers,
}
impl Fixture {
    fn new(dynamic: bool) -> Self {
        let mut host = Host::new();
        host.factory.dynamic = dynamic;
        let mut world = World::new(vec![spawn(40, 0.0)]);
        world.load(0.0, &mut host.factory).unwrap();
        let handle = *host.factory.states.keys().next().unwrap();
        let life = host
            .factory
            .states
            .remove(&handle)
            .unwrap()
            .into_lifecycle(
                CreatureLifecycleSettings {
                    save_immediately: true,
                    world_boss: false,
                    forever_corpse: false,
                    corpse_acceleration_ms: 120_000,
                    pool: None,
                },
                &world.objects,
            )
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
                &mut host,
            )
            .unwrap();
        persistent.bind_map(id, Some(map()), &mut host).unwrap();
        let mut spawns = SourceSpawnManager::new(map(), id).unwrap();
        spawns.initialize(&mut persistent, &mut host).unwrap();
        host.calls.clear();
        Self {
            world,
            host,
            life,
            spawns,
            persistent,
        }
    }
    fn apply(
        &mut self,
        f: impl FnOnce(
            &mut SourceCreatureLifecycle,
            &mut CreatureLifecycleContext<'_, '_, Definitions, Products>,
            &mut Host,
        ) -> Result<()>,
    ) -> Result<()> {
        f(
            &mut self.life,
            &mut CreatureLifecycleContext {
                grid: &mut self.world.context(),
                persistent: &mut self.persistent,
                spawns: &mut self.spawns,
            },
            &mut self.host,
        )
    }
    fn die(&mut self) {
        self.apply(|l, c, h| l.die(c, h)).unwrap();
    }
    fn update(&mut self) {
        self.apply(|l, c, h| l.update(100, c, h)).unwrap();
    }
    fn deadline(&self) -> i64 {
        self.persistent
            .state(self.spawns.persistent())
            .unwrap()
            .respawn(GridSpawnKind::Creature, 40)
    }
}
#[test]
fn death_equal_endpoint_draw_and_pre_sentinel_save_use_distinct_clocks() {
    // Creature.cpp:1868-1892: delay always draws, save sees finite deadline before sentinel.
    for dynamic in [false, true] {
        let mut f = Fixture::new(dynamic);
        let before = f.host.factory.random.cursor.raw;
        f.die();
        assert_eq!(f.host.factory.random.cursor.raw, before + 1);
        assert_eq!(f.life.construction.respawn_delay, 100);
        assert_eq!(
            f.life.corpse_expiration_ms,
            if dynamic { 220_500 } else { 190_500 }
        );
        assert_eq!(f.deadline(), 201);
        assert_eq!(
            f.life.construction.respawn_time,
            if dynamic { i64::MAX } else { 201 }
        );
        let calls = &f.host.calls;
        assert_eq!(
            &calls[..3],
            [
                Call::Snapshot,
                Call::Wall(CreatureLifecycleClock::Death),
                Call::Wall(CreatureLifecycleClock::Save)
            ]
        );
        let save = calls
            .iter()
            .position(|c| matches!(c, Call::Sql(PersistentSql::Commit)))
            .unwrap();
        let unit = calls
            .iter()
            .position(|c| *c == Call::Phase(Phase::UnitJustDied))
            .unwrap();
        assert!(save < unit);
        if dynamic {
            let add = calls
                .iter()
                .position(|c| matches!(c, Call::Spawn(SpawnOperation::Add { .. })))
                .unwrap();
            assert!(save < add && add < unit);
            assert_eq!(f.spawns.inventory().active[0].deadline_millis, 201_000);
        } else {
            assert!(f.spawns.inventory().active.is_empty());
        }
    }
}
#[test]
fn delayed_save_keeps_zero_schedule_and_later_saves_local_sentinel() {
    // The AddCreature deadline comes from persistent state, not the local object; Creature.cpp:1885-1892.
    let mut f = Fixture::new(true);
    f.life.settings.save_immediately = false;
    f.die();
    assert_eq!(f.deadline(), 0);
    assert_eq!(f.spawns.inventory().active[0].deadline_millis, 0);
    f.apply(|l, c, h| l.save_respawn(c, h)).unwrap();
    assert_eq!(f.deadline(), i64::MAX);
}
#[test]
fn one_shot_override_skips_one_death_draw_and_then_redraws() {
    // Creature.cpp:1875-1876 clears only the override-active flag.
    let mut f = Fixture::new(false);
    f.life.set_respawn_delay(7, true);
    let before = f.host.factory.random.cursor.raw;
    f.die();
    assert_eq!(f.life.construction.respawn_time, 108);
    assert_eq!(f.host.factory.random.cursor.raw, before);
    assert!(!f.life.respawn_overridden && f.life.respawn_override_once);
    f.apply(|l, c, h| l.remove_corpse(false, c, h)).unwrap();
    f.host.wall = 108;
    f.update();
    f.die();
    assert_eq!(f.life.construction.respawn_time, 208);
    assert_eq!(f.host.factory.random.cursor.raw, before + 1);
}
#[test]
fn forever_corpse_ignores_inspection_and_completion_but_expires_inclusively() {
    // Creature.cpp:1878-1881,2809-2810,2823-2824,3013-3015.
    let mut f = Fixture::new(false);
    f.life.settings.forever_corpse = true;
    f.die();
    assert_eq!(f.life.corpse_expiration_ms, 604_900_500);
    f.life.inspecting_loot(604_900_500).unwrap();
    f.life
        .reduce_corpse_decay(100_500, &f.world.objects)
        .unwrap();
    assert_eq!(f.life.corpse_expiration_ms, 604_900_500);
    f.host.clock_ms = 604_900_499;
    f.update();
    assert_eq!(f.life.death_state, CreatureDeathState::Corpse);
    f.host.clock_ms += 1;
    f.update();
    assert_eq!(f.life.death_state, CreatureDeathState::Dead);
}
#[test]
fn loot_inspection_extends_completion_shortens_and_equality_does_not_change() {
    // Creature.cpp:2813-2814,2820-2827 uses snapshot milliseconds, including zero delay.
    let mut f = Fixture::new(false);
    f.die();
    f.life.inspecting_loot(100_500).unwrap();
    assert_eq!(f.life.corpse_expiration_ms, 220_500);
    f.life.inspecting_loot(100_500).unwrap();
    assert_eq!(f.life.corpse_expiration_ms, 220_500);
    f.life.reduce_corpse_decay(0, &f.world.objects).unwrap();
    assert_eq!(f.life.corpse_expiration_ms, 120_000);
    f.life
        .reduce_corpse_decay(100_500, &f.world.objects)
        .unwrap();
    assert_eq!(f.life.corpse_expiration_ms, 120_000);
    f.world.objects.remove_from_world(f.life.object()).unwrap();
    f.life
        .reduce_corpse_decay(-100_000, &f.world.objects)
        .unwrap();
    assert_eq!(f.life.corpse_expiration_ms, 120_000);
    f.life.settings.corpse_acceleration_ms = 0;
    f.life.inspecting_loot(120_001).unwrap();
    assert_eq!(f.life.corpse_expiration_ms, 120_001);
}
#[test]
fn script_mutates_delay_after_callbacks_and_saves_with_a_fresh_wall_sample() {
    // Creature.cpp:281-304: the mutable AI result overrides even the new-system sentinel.
    for dynamic in [false, true] {
        let mut f = Fixture::new(dynamic);
        f.die();
        f.host.script_delay = 9;
        f.host.wall = 111;
        f.host.calls.clear();
        f.apply(|l, c, h| l.remove_corpse(false, c, h)).unwrap();
        assert_eq!(f.life.construction.respawn_time, 120);
        assert_eq!(f.deadline(), 120);
        if dynamic {
            assert_eq!(f.spawns.inventory().active[0].deadline_millis, 201_000);
        }
        let calls = &f.host.calls;
        let ai = calls.iter().position(|c| *c == Call::AiRemoved(9)).unwrap();
        let instance = calls
            .iter()
            .position(|c| *c == Call::Phase(Phase::InstanceDespawn))
            .unwrap();
        let clock = calls
            .iter()
            .position(|c| *c == Call::Wall(CreatureLifecycleClock::ScriptCorpseDelay))
            .unwrap();
        assert!(ai < instance && instance < clock);
        assert_eq!(calls[clock + 1], Call::Wall(CreatureLifecycleClock::Save));
    }
}
#[test]
fn corpse_branch_removes_at_equality_without_same_branch_dead_respawn() {
    // Creature.cpp:754-764 breaks after removal even when the wall deadline is due.
    let mut f = Fixture::new(false);
    f.die();
    f.life.has_loot = true;
    f.host.clock_ms = 190_500;
    f.host.wall = 201;
    f.host.calls.clear();
    let before = f
        .world
        .objects
        .get(f.life.object())
        .unwrap()
        .life_generation;
    f.update();
    assert_eq!(f.life.death_state, CreatureDeathState::Dead);
    assert_eq!(
        f.world
            .objects
            .get(f.life.object())
            .unwrap()
            .life_generation,
        before
    );
    assert!(!f
        .host
        .calls
        .contains(&Call::Wall(CreatureLifecycleClock::DeadUpdate)));
    assert_eq!(
        &f.host.calls[..3],
        [
            Call::Phase(Phase::UnitUpdate(100)),
            Call::Phase(Phase::LootUpdate),
            Call::Snapshot
        ]
    );
    f.update();
    assert_eq!(f.life.death_state, CreatureDeathState::Alive);
    assert_eq!(
        f.world
            .objects
            .get(f.life.object())
            .unwrap()
            .life_generation,
        before + 1
    );
}
#[test]
fn alive_unit_death_defers_corpse_and_regeneration_until_next_update() {
    // Creature.cpp:768-773 defers CORPSE/DEAD when Unit::Update kills the actor.
    let mut f = Fixture::new(false);
    f.host.die_in_unit = true;
    f.update();
    assert_eq!(f.life.death_state, CreatureDeathState::Corpse);
    assert!(!f.host.calls.contains(&Call::Phase(Phase::Regenerate(100))));
    assert!(!f.host.calls.contains(&Call::AiRemoved(0)));
}
#[test]
fn legacy_revival_preserves_construction_guid_and_allocator_without_new_constructor() {
    // Creature.cpp:717-750 resets the retained object rather than LoadFromDB.
    let mut f = Fixture::new(false);
    let object = f.life.object();
    let guid = f.world.objects.get(object).unwrap().guid;
    let raw = f.host.factory.random.cursor.raw;
    f.die();
    f.apply(|l, c, h| l.remove_corpse(false, c, h)).unwrap();
    f.host.wall = 201;
    f.host.calls.clear();
    f.update();
    assert_eq!(f.life.object(), object);
    let revived = f.world.objects.get(object).unwrap();
    assert_eq!(revived.guid, guid);
    assert_eq!(revived.life_generation, 2);
    assert_eq!(f.host.factory.random.cursor.raw, raw + 1);
    assert_eq!(
        f.host
            .factory
            .observations
            .iter()
            .filter(|o| matches!(o, Observation::Construct(_)))
            .count(),
        1
    );
    assert_eq!(
        f.host
            .calls
            .iter()
            .filter_map(|c| if let Call::Phase(p) = c {
                Some(*p)
            } else {
                None
            })
            .collect::<Vec<_>>(),
        [
            Phase::ClearAggroLootAurasAndDynamicFlags,
            Phase::UnitAlive,
            Phase::ResetEntry,
            Phase::ResetKillerSpellHitsLootAndFaction,
            Phase::RestoreDamageSchoolFlagsAndAddon,
            Phase::MotionInitialize,
            Phase::AiJustRespawned,
            Phase::InstanceRespawn,
            Phase::MapAdd,
        ]
    );
}
#[test]
fn new_system_enqueues_real_removal_then_scheduler_constructs_a_distinct_object() {
    // Creature.cpp:313-314 and SpawnManager.cpp:269-277 construct through the existing factory.
    let mut f = Fixture::new(true);
    let old = f.life.object();
    let old_guid = f.world.objects.get(old).unwrap().guid;
    f.die();
    f.apply(|l, c, h| l.remove_corpse(false, c, h)).unwrap();
    assert_eq!(f.world.cells.removals().len(), 1);
    assert!(!f.world.objects.get(old).unwrap().in_world);
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
        .drain(&mut f.world.context(), &mut f.host)
        .unwrap();
    assert!(f.world.objects.get(old).is_err());
    f.host.clock_ms = 201_000;
    f.host.wall = 201;
    f.host.construction_world = Some(f.world);
    f.spawns.update(&mut f.persistent, &mut f.host).unwrap();
    let world = f.host.construction_world.as_ref().unwrap();
    let new = world
        .objects
        .lookup_database(map(), DatabaseLookupKind::Creature, 40)
        .unwrap();
    assert_ne!(old, new);
    assert_ne!(old_guid, world.objects.get(new).unwrap().guid);
    assert_eq!(world.objects.get(new).unwrap().life_generation, 1);
    assert!(f.spawns.inventory().active.is_empty());
}
#[test]
fn full_corpse_callback_order_includes_linking_relocation_visibility_and_cleanup() {
    // Creature.cpp:272-314 executes all callbacks before deferred destruction.
    let mut f = Fixture::new(true);
    f.life.construction.linking_trigger = true;
    f.die();
    f.life.has_loot = true;
    f.life.loot_status = 2;
    f.host.calls.clear();
    f.apply(|l, c, h| l.remove_corpse(false, c, h)).unwrap();
    assert_eq!(
        f.host.calls,
        [
            Call::Phase(Phase::UnitDead),
            Call::Phase(Phase::Visibility),
            Call::Phase(Phase::DestroyLoot),
            Call::AiRemoved(0),
            Call::Phase(Phase::LinkingDespawn),
            Call::Phase(Phase::InstanceDespawn),
            Call::Phase(Phase::InterruptMoving),
            Call::Relocate(RelocationPhase::CreatureOnRelocated),
            Call::Phase(Phase::VisibilityRemoveCorpse),
            Call::Phase(Phase::VisibilityRestore),
            Call::Removal(ResidencyPhase::CleanupBeforeDelete),
            Call::Removal(ResidencyPhase::RemoveFromWorld),
            Call::RemovalEvent(ResidencyEvent::Queue(f.life.object(), true)),
        ]
    );
    assert!(!f.life.has_loot);
    assert_eq!(f.life.loot_status, 0);
    assert_eq!(f.life.corpse_expiration_ms, 0);
}
#[test]
fn group_delay_clear_and_zero_delay_do_not_add_a_nongroup_schedule() {
    // Creature.cpp:1870-1873,1891,1903 tests group membership before ClearCreatureGroup.
    let mut f = Fixture::new(true);
    f.life.construction.group = Some(CreatureGroup {
        id: 1,
        string_id: 0,
        has_formation: false,
    });
    f.host.group_delay = Some(8);
    let before = f.host.factory.random.cursor.raw;
    f.die();
    assert_eq!(f.deadline(), 109);
    assert_eq!(f.host.factory.random.cursor.raw, before);
    assert!(f.spawns.inventory().active.is_empty());
    assert!(f.life.construction.group.is_none());
    assert!(f.host.calls.contains(&Call::Phase(Phase::ClearGroup)));
    let mut f = Fixture::new(true);
    f.life.set_respawn_delay(0, false);
    f.die();
    assert!(f.spawns.inventory().active.is_empty());
}
#[test]
fn missing_live_callbacks_refuse_at_the_source_call_and_poison_the_prefix() {
    for phase in [
        Phase::LinkingDespawn,
        Phase::ClearGroup,
        Phase::PoolRespawn(2),
    ] {
        let mut f = Fixture::new(false);
        match phase {
            Phase::LinkingDespawn => f.life.construction.linking_trigger = true,
            Phase::ClearGroup => {
                f.life.construction.group = Some(CreatureGroup {
                    id: 1,
                    string_id: 0,
                    has_formation: false,
                })
            }
            Phase::PoolRespawn(_) => f.life.settings.pool = Some(2),
            _ => unreachable!(),
        }
        f.host.refuse = Some(phase);
        let result = match phase {
            Phase::ClearGroup => f.apply(|l, c, h| l.die(c, h)),
            Phase::LinkingDespawn => {
                f.die();
                f.apply(|l, c, h| l.remove_corpse(false, c, h))
            }
            _ => {
                f.die();
                f.apply(|l, c, h| l.remove_corpse(false, c, h)).unwrap();
                f.host.wall = 201;
                f.apply(|l, c, h| l.update(100, c, h))
            }
        };
        assert_eq!(result.unwrap_err(), phase.missing_owner());
        assert_eq!(
            f.apply(|l, c, h| l.update(100, c, h)).unwrap_err().id,
            "execution.creature_lifecycle.stopped"
        );
    }
}
#[test]
fn save_uses_corpse_timepoint_conversion_when_wall_deadline_has_passed() {
    // Creature.cpp:2287-2293 truncates milliseconds rather than using the map wall calendar.
    let mut f = Fixture::new(false);
    f.die();
    f.host.save_wall = 202;
    f.host.clock_ms = 190_499;
    f.apply(|l, c, h| l.save_respawn(c, h)).unwrap();
    assert_eq!(f.deadline(), 190);
    f.host.clock_ms = 190_500;
    f.host.calls.clear();
    f.apply(|l, c, h| l.save_respawn(c, h)).unwrap();
    assert!(!f.host.calls.iter().any(|c| matches!(c, Call::Sql(_))));
}
#[test]
fn linked_dead_gate_checks_only_due_wall_deadlines() {
    // Creature.cpp:719 short-circuits linking until the fresh wall deadline is due.
    let mut f = Fixture::new(false);
    f.die();
    f.apply(|l, c, h| l.remove_corpse(false, c, h)).unwrap();
    f.life.construction.spawning_linked = true;
    f.host.gate = false;
    f.host.wall = 200;
    f.host.calls.clear();
    f.update();
    assert!(!f.host.calls.contains(&Call::Gate));
    f.host.wall = 201;
    f.update();
    assert!(f.host.calls.contains(&Call::Gate));
    assert_eq!(f.life.death_state, CreatureDeathState::Dead);
    f.host.gate = true;
    f.update();
    assert_eq!(f.life.death_state, CreatureDeathState::Alive);
}
#[test]
fn explicit_respawn_clears_persistence_and_waits_for_next_update() {
    // Creature.cpp:1950-1958 never revives directly from Respawn().
    let mut f = Fixture::new(false);
    f.die();
    f.apply(|l, c, h| l.respawn(c, h)).unwrap();
    assert_eq!(f.deadline(), 0);
    assert_eq!(f.life.construction.respawn_time, 101);
    assert_eq!(f.life.death_state, CreatureDeathState::Dead);
    f.update();
    assert_eq!(f.life.death_state, CreatureDeathState::Alive);
}
#[test]
fn signed_clock_overflow_refuses_instead_of_inventing_source_behavior() {
    let mut f = Fixture::new(false);
    f.host.wall = i64::MAX;
    assert_eq!(
        f.apply(|l, c, h| l.die(c, h)).unwrap_err().id,
        "execution.creature_lifecycle.signed_time_overflow_undefined"
    );
    let mut f = Fixture::new(false);
    f.host.clock_ms = i64::MAX;
    assert_eq!(
        f.apply(|l, c, h| l.die(c, h)).unwrap_err().id,
        "execution.creature_lifecycle.timepoint_overflow_undefined"
    );
}

#[test]
fn source_object_update_dispatches_the_retained_lifecycle() {
    let Fixture {
        mut world,
        mut host,
        life,
        mut persistent,
        mut spawns,
    } = Fixture::new(false);
    let handle = life.object();
    let mut creatures = SourceCreatures::default();
    creatures
        .retain(life.construction, life.settings, &world.objects)
        .unwrap();
    let mut updates = SourceUpdateSet::new(map()).unwrap();
    updates
        .insert(&world.objects, handle, UpdateInsertion::Creature)
        .unwrap();
    let mut dispatch = CreatureLifecycleDispatch {
        creatures: &mut creatures,
        definitions: &mut world.definitions,
        products: &mut world.products,
        sets: &mut world.sets,
        persistent: &mut persistent,
        spawns: &mut spawns,
        owner: &mut host,
    };
    updates
        .dispatch(
            123,
            &mut world.objects,
            &mut world.cells,
            &mut dispatch,
            &mut |_| Ok(()),
        )
        .unwrap();
    assert_eq!(
        host.calls,
        [
            Call::Phase(Phase::UnitUpdate(123)),
            Call::Phase(Phase::Regenerate(123))
        ]
    );
    assert_eq!(
        creatures
            .forget_destroyed(handle, &world.objects)
            .unwrap_err()
            .id,
        "execution.creature_lifecycle.still_constructed"
    );
}

#[test]
fn legacy_respawn_callbacks_keep_link_group_map_and_pool_order() {
    // Creature.cpp:733-750: callbacks precede Map::Add, which precedes pool replacement.
    let mut f = Fixture::new(false);
    f.die();
    f.apply(|l, c, h| l.remove_corpse(false, c, h)).unwrap();
    f.life.construction.linking_trigger = true;
    f.life.construction.group = Some(CreatureGroup {
        id: 1,
        string_id: 0,
        has_formation: false,
    });
    f.life.settings.pool = Some(2);
    f.host.wall = 201;
    f.host.calls.clear();
    f.update();
    assert_eq!(
        &f.host.calls[f.host.calls.len() - 6..],
        [
            Call::Phase(Phase::AiJustRespawned),
            Call::Phase(Phase::InstanceRespawn),
            Call::Phase(Phase::LinkingRespawn),
            Call::Phase(Phase::GroupRespawn),
            Call::Phase(Phase::MapAdd),
            Call::Phase(Phase::PoolRespawn(2)),
        ]
    );
}

#[test]
fn retained_seal8_lifecycle_projection_or_explicit_skip() {
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
        println!("SKIP unsealed seal8-08 MR/MG/MPR lifecycle: set BENILLA_F1_SEAL8_FRAMING");
        return;
    }
    let report = inspect_seal8_unsealed_creature_lifecycle(&path).unwrap();
    assert_eq!(report.direct_lifecycle_sites, 0);
    assert_eq!(report.independently_matched_transitions, 0);
    assert!(!report.reference_execution_sealed && !report.whole_execution_accepted);
    println!("SKIP unsealed seal8-08 MR/MG/MPR lifecycle comparison: no death/corpse/respawn/loot projection");
    println!("{}", serde_json::to_string(&report).unwrap());
}

#[test]
fn corpse_relocation_uses_existing_cell_owner_and_respawn_pose() {
    // Creature.cpp:302-304 relocates to the retained spawn coordinates before removal.
    let mut f = Fixture::new(false);
    let handle = f.life.object();
    let initial = f.world.objects.get(handle).unwrap().current_cell;
    f.world
        .context()
        .move_creature(
            handle,
            SourcePose([200.0_f32.to_bits(), 0, 4.0_f32.to_bits(), 0]),
            &mut f.host,
        )
        .unwrap();
    assert_ne!(f.world.objects.get(handle).unwrap().current_cell, initial);
    f.die();
    f.apply(|l, c, h| l.remove_corpse(false, c, h)).unwrap();
    assert_eq!(f.world.objects.get(handle).unwrap().current_cell, initial);
    assert_eq!(
        f.world.objects.pose(handle).unwrap(),
        SourcePose([0, 0, 4.0_f32.to_bits(), 0])
    );
    assert_eq!(f.life.construction.position, [0.0, 0.0, 4.0, 0.0]);
}

#[test]
fn unload_save_adapter_exposes_the_same_local_deadline_and_corpse_timepoint() {
    // F3.11 consumes this exact state through RespawnSaveState; Creature.cpp:2285-2293.
    let mut f = Fixture::new(true);
    f.die();
    match f.life.respawn_save_state() {
        RespawnSaveState::Creature {
            respawn_seconds,
            new_spawning_system,
            corpse_expiry_ms,
        } => {
            assert_eq!(respawn_seconds, i64::MAX);
            assert!(new_spawning_system);
            assert_eq!(corpse_expiry_ms, 220_500);
        }
        _ => panic!("creature state"),
    }
}

#[test]
fn new_system_can_construct_the_next_life_while_the_old_corpse_remains() {
    // Only legacy corpse delay is clipped; Creature.cpp:1690-1693,1888-1892.
    let mut f = Fixture::new(true);
    let old = f.life.object();
    f.die();
    f.host.clock_ms = 201_000;
    f.host.wall = 201;
    f.host.construction_world = Some(f.world);
    f.spawns.update(&mut f.persistent, &mut f.host).unwrap();
    let world = f.host.construction_world.as_ref().unwrap();
    let new = world
        .objects
        .lookup_database(map(), DatabaseLookupKind::Creature, 40)
        .unwrap();
    assert_ne!(old, new);
    assert_eq!(f.life.death_state, CreatureDeathState::Corpse);
    assert_eq!(f.life.corpse_expiration_ms, 220_500);
    assert!(world.objects.get(old).unwrap().in_world);
    assert!(!world.objects.get(old).unwrap().alive);
    assert!(world.objects.get(new).unwrap().alive);
    assert_eq!(world.objects.get(old).unwrap().life_generation, 1);
    assert_eq!(world.objects.get(new).unwrap().life_generation, 1);
}

#[test]
fn pool_live_actions_reuse_retained_delay_save_readd_and_removal() {
    let mut f = Fixture::new(false);
    let object = f.life.object();
    let load = PoolLoadRequest {
        state: f.spawns.persistent(),
        map: map(),
        key: SpawnKey {
            kind: GridSpawnKind::Creature,
            guid: 40,
        },
    };
    for action in [
        PoolLiveAction::SetCreatureDelay(40),
        PoolLiveAction::SetRespawnTime(40),
        PoolLiveAction::SaveRespawn,
        PoolLiveAction::MapAdd,
        PoolLiveAction::Remove,
    ] {
        f.apply(|l, c, o| {
            PoolLiveRequest {
                load,
                object,
                action,
            }
            .creature(l, c, o)
        })
        .unwrap();
    }
    assert_eq!(f.life.construction.respawn_delay, 40);
    assert!(f.life.respawn_overridden);
    assert!(!f.life.respawn_override_once);
    assert_eq!(f.life.construction.respawn_time, 141);
    assert_eq!(f.deadline(), 141);
    assert!(f
        .host
        .calls
        .contains(&Call::Wall(CreatureLifecycleClock::SetRespawn)));
    assert!(f.host.calls.contains(&Call::Phase(Phase::MapAdd)));
    assert!(!f.world.cells.removals().is_empty());
    f.host.calls.clear();
    f.apply(|l, _, o| l.set_respawn_time::<Definitions, Products>(0, o))
        .unwrap();
    assert_eq!(f.life.construction.respawn_time, 0);
    assert!(f.host.calls.is_empty());
}

#[path = "pools/creature_effect_tests.rs"]
mod pool_effects;

struct LifecycleLinkRow(Option<LinkingTemplateRow>);
impl LinkingRows for LifecycleLinkRow {
    fn next_template(&mut self) -> Result<Option<LinkingTemplateRow>> {
        Ok(self.0.take())
    }
    fn next_guid(&mut self) -> Result<Option<LinkingGuidRow>> {
        Ok(None)
    }
}
struct LifecycleLinkDefinitions;
impl LinkingLoadOwner for LifecycleLinkDefinitions {
    fn template_exists(&mut self, _: u32) -> Result<bool> {
        Ok(true)
    }
    fn map_exists(&mut self, _: u32) -> Result<bool> {
        Ok(true)
    }
    fn creature_data(&mut self, _: u32) -> Result<Option<LinkingSpawnData>> {
        panic!("entry-only fixture")
    }
    fn query_unique_master(&mut self, _: u32, _: u32) -> Result<LinkingMasterQuery> {
        panic!("nonpassive nonfollow fixture")
    }
}
impl LinkingEventOwner for Fixture {
    fn linking_creature(
        &mut self,
        m: MapIdentity,
        object: ObjectHandle,
    ) -> Result<LinkingCreature> {
        assert_eq!(m, map());
        assert_eq!(object, self.life.object());
        LinkingCreature::read(&self.world.objects, &self.life, false, false)
    }
    fn linking_lookup(
        &mut self,
        m: MapIdentity,
        lookup: LinkingLookup,
    ) -> Result<Option<ObjectHandle>> {
        Ok(lookup.resolve(m, &self.world.objects))
    }
    fn linking_wall(&mut self) -> Result<ClockSample> {
        Ok(ClockSample::authored(
            ClockKind::WallSeconds,
            self.host.wall,
        ))
    }
    fn linking_schedule(
        &mut self,
        request: LinkingScheduleRequest,
        _: &mut SourceCreatureLinkingHolder,
    ) -> Result<()> {
        request.execute(&mut self.spawns, &mut self.persistent, &mut self.host)
    }
    fn linking_action(
        &mut self,
        request: LinkingActionRequest,
        _: &mut SourceCreatureLinkingHolder,
    ) -> Result<()> {
        self.apply(|l, c, h| request.lifecycle(l, c, h))
    }
}
fn lifecycle_link(
    f: &mut Fixture,
    flags: u16,
) -> (SourceCreatureLinking, SourceCreatureLinkingHolder) {
    let entry = f.life.construction.template.entry;
    let mut links = SourceCreatureLinking::default();
    links
        .load(
            &mut LifecycleLinkRow(Some(LinkingTemplateRow {
                ordinal: 0,
                slave: entry,
                map: 0,
                master: entry,
                flags,
                range: 0,
            })),
            &mut LifecycleLinkDefinitions,
        )
        .unwrap();
    let mut holder = SourceCreatureLinkingHolder::new(map()).unwrap();
    register_source_creature_linking(
        &links,
        &mut holder,
        &f.world.objects,
        f.life.object(),
        entry,
    )
    .unwrap();
    (links, holder)
}
#[test]
fn linking_forced_despawn_executes_real_death_remove_and_health_owners() {
    // ForcedDespawn goes through SetDeathState and in-place RemoveCorpse before health; Creature.cpp:1972-1980.
    let mut f = Fixture::new(false);
    let (links, mut holder) = lifecycle_link(&mut f, 0x0010);
    let object = f.life.object();
    holder
        .event(&links, &mut f, LinkingEvent::Die, object, None)
        .unwrap();
    assert_eq!(f.life.death_state, CreatureDeathState::Dead);
    assert_eq!(f.host.factory.health[&object], 0);
    assert_eq!(
        f.host.calls.last(),
        Some(&Call::Phase(Phase::SetHealthZero))
    );
    assert!(f.host.calls.contains(&Call::Phase(Phase::UnitJustDied)));
    assert!(f.host.calls.contains(&Call::Phase(Phase::UnitDead)));
    assert_eq!(f.world.objects.get(object).unwrap().life_generation, 1);
    let calls = f.host.calls.len();
    f.apply(|l, c, h| l.forced_despawn(c, h)).unwrap();
    assert_eq!(f.host.calls.len(), calls);
}
#[test]
fn linking_respawn_uses_real_legacy_deadline_and_repeated_loop_guard() {
    // Respawn clears persisted time and writes fresh wall time without reviving immediately; Creature.cpp:1948-1959.
    let mut f = Fixture::new(false);
    let (links, mut holder) = lifecycle_link(&mut f, 0x0080);
    f.die();
    let object = f.life.object();
    holder
        .event(&links, &mut f, LinkingEvent::Respawn, object, None)
        .unwrap();
    assert_eq!(f.life.death_state, CreatureDeathState::Dead);
    assert_eq!(f.life.construction.respawn_time, 101);
    assert_eq!(f.deadline(), 0);
    f.host.calls.clear();
    holder
        .event(&links, &mut f, LinkingEvent::Respawn, object, None)
        .unwrap();
    assert!(f.host.calls.is_empty());
    f.update();
    assert_eq!(f.life.death_state, CreatureDeathState::Alive);
    assert_eq!(f.world.objects.get(object).unwrap().life_generation, 2);
}
#[test]
fn linking_absent_dynamic_slave_enters_real_spawn_manager_before_pruning() {
    // Missing dynamic slave schedules via RespawnCreature before erasing the list record; CreatureLinkingMgr.cpp:533-538.
    let mut f = Fixture::new(true);
    let (links, mut holder) = lifecycle_link(&mut f, 0x0040);
    let object = f.life.object();
    f.world.objects.remove_from_world(object).unwrap();
    holder
        .event(&links, &mut f, LinkingEvent::Die, object, None)
        .unwrap();
    let active = f.spawns.inventory().active;
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].key.guid, 40);
    assert_eq!(active[0].key.kind, GridSpawnKind::Creature);
    assert!(holder
        .inventory()
        .entries
        .values()
        .flatten()
        .all(|g| g.slaves.is_empty()));
}

#[test]
fn event_unspawn_removes_only_alive_creatures_and_preserves_lootable_corpses() {
    // GameEventMgr.cpp:956-958 queues alive creatures without killing dead lootables.
    for dead in [false, true] {
        let mut f = Fixture::new(false);
        if dead {
            f.die();
        }
        let object = f.life.object();
        f.apply(|life, context, owner| {
            EventLiveRequest {
                map: map(),
                key: SpawnKey {
                    kind: GridSpawnKind::Creature,
                    guid: 40,
                },
                object,
            }
            .creature(life, context.grid, owner)
        })
        .unwrap();
        assert_eq!(f.world.cells.removals().len(), usize::from(!dead));
        assert_eq!(f.world.objects.get(object).unwrap().in_world, dead);
        if dead {
            assert_eq!(f.life.death_state, CreatureDeathState::Corpse);
        }
    }
}

#[test]
fn group_death_dispatch_removes_real_membership_after_respawn_save() {
    // Creature.cpp:1870-1903 saves before ClearCreatureGroup -> SpawnGroup.cpp:74-100,514-522.
    let mut f = Fixture::new(true);
    let mut input = spawn_group_tests::template(1, 0, 1);
    input.respawn_min = Some(5);
    let (mut groups, mut states, group, mut work) =
        spawn_group_tests::fixture(input, &[(40, 0)], false, |_, _| {});
    groups
        .spawn(group, &mut states, false, false, &mut work)
        .unwrap();
    let object = f.life.object();
    work.removed.insert(
        object,
        GroupRemovedObject {
            key: SpawnKey {
                kind: GridSpawnKind::Creature,
                guid: 40,
            },
            respawn_delay: 5,
        },
    );
    f.life.construction.group = groups.creature_group(map(), 40).unwrap();
    f.persistent = states;
    f.host.groups = Some((groups, work));
    f.die();
    assert_eq!(f.deadline(), 106);
    assert!(f.life.construction.group.is_none());
    let (groups, _) = f.host.groups.as_ref().unwrap();
    assert!(groups.state(group).unwrap().objects.is_empty());
    assert_eq!(groups.state(group).unwrap().cooldown_millis, 105_000);
    assert!(!f
        .host
        .calls
        .iter()
        .any(|call| matches!(call, Call::Spawn(SpawnOperation::Add { .. }))));
}
#[test]
fn group_forced_despawn_dispatches_real_death_corpse_health_and_delayed_event_refusal() {
    // SpawnGroup.cpp:640-643 sets a one-shot delay before Creature.cpp:1964-1980.
    for delay_millis in [0, 25] {
        let mut f = Fixture::new(false);
        if delay_millis != 0 {
            f.host.refuse = Some(Phase::ScheduleForcedDespawn {
                delay_millis,
                only_alive: false,
            });
        }
        let callback = GroupCallback::ForcedDespawn {
            object: f.life.object(),
            delay_millis,
            forced_seconds: 5,
            only_alive: false,
        };
        let result =
            f.apply(|life, context, host| callback.creature_lifecycle(map(), life, context, host));
        assert_eq!(f.life.construction.respawn_delay, 5);
        if delay_millis == 0 {
            result.unwrap();
            assert_eq!(f.life.death_state, CreatureDeathState::Dead);
            assert_eq!(f.host.factory.health[&f.life.object()], 0);
            assert!(f.host.calls.contains(&Call::Phase(Phase::SetHealthZero)));
        } else {
            assert_eq!(
                result.unwrap_err().id,
                "execution.group.f5_event_forced_despawn_owner"
            );
            assert_eq!(f.life.death_state, CreatureDeathState::Alive);
        }
    }
}

#[test]
fn creature_group_clear_refuses_formation_removal_before_mutating_group_membership() {
    // Creature.cpp:1580-1582 removes formation membership before the group callback/reset sequence.
    let mut f = Fixture::new(true);
    let (mut groups, mut states, group, mut work) = spawn_group_tests::fixture(
        spawn_group_tests::template(1, 0, 1),
        &[(40, 0)],
        false,
        |rows, _| {
            rows.push(
                benilla_sim::source_spawn_groups::GroupPhase::Formations,
                benilla_sim::source_spawn_groups::GroupInputData::Formation(
                    spawn_group_tests::formation(1),
                ),
            );
        },
    );
    groups
        .spawn(group, &mut states, false, false, &mut work)
        .unwrap();
    f.life.construction.group = groups.creature_group(map(), 40).unwrap();
    work.callback_hook = Some(|_, _, _, _, callback| Err(callback.missing_owner()));
    f.persistent = states;
    f.host.groups = Some((groups, work));
    assert_eq!(
        f.apply(|life, context, host| life.die(context, host))
            .unwrap_err()
            .id,
        "execution.group.f4_13_formation_owner"
    );
    assert!(f.life.construction.group.is_some());
    let (groups, _) = f.host.groups.as_ref().unwrap();
    assert_eq!(groups.state(group).unwrap().objects.len(), 1);
    assert!(!f.host.calls.contains(&Call::Phase(Phase::CreatorCooldown)));
    assert_eq!(f.deadline(), 201);
}
