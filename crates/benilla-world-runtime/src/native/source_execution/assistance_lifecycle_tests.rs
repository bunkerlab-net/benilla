use super::*;
use std::collections::BTreeMap;

struct Bound<'a> {
    host: &'a mut Host,
    units: SourceUnits,
}
impl GridObjectFactory<Definitions, Products> for Bound<'_> {
    fn load_from_db(
        &mut self,
        attempt: GridLoadAttempt,
        row: CreatureSpawn,
        context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<GridCandidateOutcome> {
        self.host.load_from_db(attempt, row, context)
    }
    fn viewpoint_added(
        &mut self,
        object: ObjectHandle,
        cell: CellIdentity,
        context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        self.host.viewpoint_added(object, cell, context)
    }
    fn battleground_db_load(
        &mut self,
        object: ObjectHandle,
        context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        self.host.battleground_db_load(object, context)
    }
    fn cell_corpses(
        &mut self,
        cell: CellPair,
        context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        self.host.cell_corpses(cell, context)
    }
    fn resurrectable_corpses(
        &mut self,
        cell: CellPair,
        context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        self.host.resurrectable_corpses(cell, context)
    }
}
impl ActiveRelocationOwner<Definitions, Products> for Bound<'_> {
    fn relocation_phase(
        &mut self,
        object: ObjectHandle,
        phase: RelocationPhase,
        context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        self.host.relocation_phase(object, phase, context)
    }
}
impl ResidencyOwner<Definitions, Products> for Bound<'_> {
    fn residency_observe(&mut self, event: ResidencyEvent) -> Result<()> {
        self.host.residency_observe(event)
    }
    fn residency_phase(
        &mut self,
        object: ObjectHandle,
        phase: ResidencyPhase,
        context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        self.host.residency_phase(object, phase, context)
    }
}
impl PersistentEffects for Bound<'_> {
    fn sql(&mut self, operation: PersistentSql) -> Result<()> {
        self.host.sql(operation)
    }
    fn query_respawns(&mut self, kind: GridSpawnKind) -> Result<CharacterQueryStatus> {
        self.host.query_respawns(kind)
    }
    fn observe(&mut self, operation: PersistentOperation) -> Result<()> {
        self.host.observe(operation)
    }
    fn dungeon_reset(&mut self, entry: PersistentMapEntry) -> Result<i64> {
        self.host.dungeon_reset(entry)
    }
}
impl SpawnWork for Bound<'_> {
    fn thread(&self) -> u64 {
        self.host.thread()
    }
    fn wall_seconds(&mut self, call: SpawnClockCall) -> Result<ClockSample> {
        self.host.wall_seconds(call)
    }
    fn world_clock_millis(&mut self) -> Result<i64> {
        self.host.world_clock_millis()
    }
    fn world_game_seconds(&mut self) -> Result<i64> {
        self.host.world_game_seconds()
    }
    fn dynamic_guids(&mut self, map: u32, kind: GridSpawnKind) -> Result<Arc<[u32]>> {
        self.host.dynamic_guids(map, kind)
    }
    fn definition(&mut self, key: SpawnKey) -> Result<Option<DynamicSpawnDefinition>> {
        self.host.definition(key)
    }
    fn linking_can_spawn(&mut self, guid: u32) -> Result<bool> {
        self.host.linking_can_spawn(guid)
    }
    fn groups(&mut self) -> Result<Arc<BTreeMap<u32, DynamicSpawnGroup>>> {
        self.host.groups()
    }
    fn create_group(&mut self, group: DynamicSpawnGroup) -> Result<SpawnGroupHandle> {
        self.host.create_group(group)
    }
    fn update_group(
        &mut self,
        manager: &mut SourceSpawnManager,
        persistent: &mut SourceManagers,
        group: SpawnGroupHandle,
    ) -> Result<()> {
        self.host.update_group(manager, persistent, group)
    }
    fn respawn_group_in_vicinity(
        &mut self,
        manager: &mut SourceSpawnManager,
        persistent: &mut SourceManagers,
        group: SpawnGroupHandle,
        position: [u32; 4],
        range: u32,
    ) -> Result<()> {
        self.host
            .respawn_group_in_vicinity(manager, persistent, group, position, range)
    }
    fn construct(
        &mut self,
        manager: &mut SourceSpawnManager,
        persistent: &mut SourceManagers,
        request: DynamicSpawnRequest,
    ) -> Result<bool> {
        self.host.construct(manager, persistent, request)
    }
    fn spawn_operation(&mut self, map: MapIdentity, operation: SpawnOperation) -> Result<()> {
        self.host.spawn_operation(map, operation)
    }
}
struct Unused;
impl AssistanceExecutionOwner for Unused {}
impl CreatureLifecycleOwner<Definitions, Products> for Bound<'_> {
    fn lifecycle_random(&mut self) -> Result<&mut dyn CreatureRandom> {
        self.host.lifecycle_random()
    }
    fn lifecycle_wall(
        &mut self,
        object: ObjectHandle,
        call: CreatureLifecycleClock,
    ) -> Result<ClockSample> {
        self.host.lifecycle_wall(object, call)
    }
    fn lifecycle_phase(
        &mut self,
        phase: CreatureLifecyclePhase,
        creature: &mut SourceCreatureLifecycle,
        context: &mut CreatureLifecycleContext<'_, '_, Definitions, Products>,
    ) -> Result<()> {
        self.host.lifecycle_phase(phase, creature, context)
    }
    fn lifecycle_assistance_event(
        &mut self,
        capture: &AssistanceCapture,
        queue: &mut impl AssistanceQueue,
        _: &mut SourceCreatureLifecycle,
        context: &mut CreatureLifecycleContext<'_, '_, Definitions, Products>,
    ) -> Result<()> {
        SourceAssistance::execute(
            capture,
            &mut self.units,
            context.grid.objects,
            &mut Unused,
            queue,
        )
    }
}
fn help_event(unit: ObjectHandle) -> AssistanceCapture {
    AssistanceCapture {
        sender: unit,
        enemy: None,
        receivers: Vec::new(),
        kind: AssistanceEvent::ReenableHelp,
    }
}
#[test]
fn lifecycle_assistance_default_refuses_only_at_its_due_callback() {
    let mut f = Fixture::new(false);
    f.life
        .unit_events
        .add_assistance(10, help_event(f.life.object()))
        .unwrap();
    f.apply(|life, context, host| update_source_creature_events(life, 9, context, host))
        .unwrap();
    assert_eq!(f.life.unit_events.assistance_captures().len(), 1);
    let error = f
        .apply(|life, context, host| update_source_creature_events(life, 1, context, host))
        .unwrap_err();
    assert_eq!(
        error.id,
        "execution.assistance.lifecycle_live_binding_owner"
    );
}
#[test]
fn lifecycle_assistance_bound_owner_executes_real_help_reenable() {
    let mut f = Fixture::new(false);
    let unit = f.life.object();
    let mut template = super::super::motion_master_tests::template();
    template.entry = f.life.construction.template.entry;
    let mut fields = SourceUnitFields::creature(&template);
    fields.can_check_help = false;
    let mut bound = Bound {
        host: &mut f.host,
        units: SourceUnits::default(),
    };
    bound
        .units
        .register(&f.world.objects, unit, fields)
        .unwrap();
    f.life
        .unit_events
        .add_assistance(10, help_event(unit))
        .unwrap();
    update_source_creature_events(
        &mut f.life,
        10,
        &mut CreatureLifecycleContext {
            grid: &mut f.world.context(),
            persistent: &mut f.persistent,
            spawns: &mut f.spawns,
        },
        &mut bound,
    )
    .unwrap();
    assert!(
        bound
            .units
            .get(&f.world.objects, unit)
            .unwrap()
            .can_check_help
    );
    assert!(f.life.unit_events.assistance_captures().is_empty());
    f.life.unit_events.validate().unwrap();
}
