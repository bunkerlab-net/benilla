// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored executable callback host, not a production AI/geometry implementation.
use super::*;
use benilla_sim::creature_init::DisplayChoice;
use benilla_sim::resident_map::{compute_cell_pair, CellPair};
use benilla_sim::source_rng::{DrawCursor, Mt19937, SourceRandom, TraceEvent};
use std::collections::BTreeMap;
fn map() -> MapIdentity {
    MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    }
}
fn cell(x: f32) -> CellPair {
    compute_cell_pair(x.to_bits(), 0_f32.to_bits()).unwrap()
}
fn spawn(guid: u32, x: f32) -> CreatureSpawn {
    CreatureSpawn {
        database_guid: guid,
        entry: 7,
        position: [x, 0.0, 4.0, 0.0],
        model: 0,
        respawn_seconds: [100, 100],
        respawn_radius: 5.0,
        relay: 0,
    }
}
fn template(entry: u32) -> CreatureTemplate {
    CreatureTemplate {
        entry,
        dynamic_guid: false,
        displays: [
            DisplayChoice {
                id: 11,
                probability: 1,
            },
            DisplayChoice {
                id: 0,
                probability: 0,
            },
            DisplayChoice {
                id: 0,
                probability: 0,
            },
            DisplayChoice {
                id: 0,
                probability: 0,
            },
        ],
        levels: [5, 5],
        corpse_seconds: 120,
        can_fly: false,
    }
}
struct Definitions(BTreeMap<u32, CreatureSpawn>);
impl GridDefinitions for Definitions {
    type Row = CreatureSpawn;
    fn read(&mut self, attempt: GridLoadAttempt) -> Result<CreatureSpawn> {
        require(
            attempt.catalog_ordinal == u64::from(attempt.guid),
            "authored.ordinal",
        )?;
        self.0.get(&attempt.guid).cloned().ok_or_else(|| Refusal {
            id: "authored.spawn_missing".into(),
        })
    }
}
#[derive(Default)]
struct Products(usize);
impl GridProductOwner for Products {
    fn load(&mut self, _: MapIdentity, _: ProductCoordinates) -> Result<bool> {
        self.0 += 1;
        Ok(true)
    }
}
struct Random {
    mt: Mt19937,
    cursor: DrawCursor,
    events: Vec<TraceEvent>,
}
impl Random {
    fn new() -> Self {
        Self {
            mt: Mt19937::from_seed(5489),
            cursor: DrawCursor::default(),
            events: vec![],
        }
    }
}
impl CreatureRandom for Random {
    fn creature_irand(&mut self, min: i32, max: i32) -> Result<i32> {
        SourceRandom::new(1, &mut self.mt, &mut self.cursor, &mut |e| {
            self.events.push(e)
        })
        .unwrap()
        .creature_irand(min, max)
    }
    fn creature_urand(&mut self, min: u32, max: u32) -> Result<u32> {
        SourceRandom::new(1, &mut self.mt, &mut self.cursor, &mut |e| {
            self.events.push(e)
        })
        .unwrap()
        .creature_urand(min, max)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Observation {
    Construct(ObjectHandle),
    Phase(u32, CreaturePhase),
    LoaderView(u32),
    LoaderBg(u32),
    Entry(CreatureEntryOwner),
    EventGuid(u32),
    Persistent(u32),
    Clock,
    Save(u32, i64),
    Destroy(ObjectHandle),
    Complete(u32),
}
struct Factory {
    guids: MapGuidAllocators,
    random: Random,
    templates: BTreeMap<u32, CreatureTemplate>,
    models: BTreeMap<u32, CreatureModel>,
    observations: Vec<Observation>,
    states: BTreeMap<ObjectHandle, CreatureConstruction>,
    group: Option<CreatureGroup>,
    entries: [u32; 3],
    event: Option<CreatureEventData>,
    event_guid: bool,
    dynamic: bool,
    saved_time: i64,
    now: i64,
    linked: bool,
    can_spawn: bool,
    linked_checks: usize,
    linking_trigger: bool,
    active: bool,
    nested: Option<CellPair>,
    refuse: Option<CreaturePhase>,
    health: BTreeMap<ObjectHandle, u32>,
    kernel: Option<benilla_sim::creature_init::Input>,
    initialized: BTreeMap<ObjectHandle, benilla_sim::creature_init::InitializedCreature>,
}
impl Factory {
    fn new() -> Self {
        let mut global = GlobalGuidAllocators::from_seed_rows(GuidSeedRows {
            player: GuidMaximum::NoResult,
            item: GuidMaximum::NoResult,
            corpse: GuidMaximum::NoResult,
            creature: GuidMaximum::Value(99),
            gameobject: GuidMaximum::NoResult,
            creature_reserve: 10,
            gameobject_reserve: 10,
        });
        Self {
            guids: global.map_allocators(map()).unwrap(),
            random: Random::new(),
            templates: [(7, template(7)), (8, template(8))].into(),
            models: [(
                11,
                CreatureModel {
                    id: 11,
                    gender: 0,
                    other_gender: 0,
                },
            )]
            .into(),
            observations: vec![],
            states: BTreeMap::new(),
            group: None,
            entries: [0; 3],
            event: None,
            event_guid: false,
            dynamic: false,
            saved_time: 0,
            now: 1000,
            linked: false,
            can_spawn: true,
            linked_checks: 0,
            linking_trigger: false,
            active: false,
            nested: None,
            refuse: None,
            health: BTreeMap::new(),
            kernel: None,
            initialized: BTreeMap::new(),
        }
    }
}
impl GridObjectFactory<Definitions, Products> for Factory {
    fn load_from_db(
        &mut self,
        a: GridLoadAttempt,
        row: CreatureSpawn,
        context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<GridCandidateOutcome> {
        context.load_creature(self, a, row)
    }
    fn viewpoint_added(
        &mut self,
        object: ObjectHandle,
        _: CellIdentity,
        context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        self.observations.push(Observation::LoaderView(
            context.objects.get(object)?.database_guid,
        ));
        Ok(())
    }
    fn battleground_db_load(
        &mut self,
        object: ObjectHandle,
        context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        self.observations.push(Observation::LoaderBg(
            context.objects.get(object)?.database_guid,
        ));
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
impl CreatureFactoryOwner<Definitions, Products> for Factory {
    fn creature_thread(&self) -> u64 {
        2
    }
    fn creature_construct_candidate(
        &mut self,
        object: ObjectHandle,
        _: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        self.observations.push(Observation::Construct(object));
        self.health.insert(object, 0);
        Ok(())
    }
    fn creature_spawn(
        &mut self,
        _: GridLoadAttempt,
        row: CreatureSpawn,
    ) -> Result<Option<CreatureSpawn>> {
        Ok(Some(row))
    }
    fn creature_event_guid(&mut self, guid: u32) -> Result<bool> {
        self.observations.push(Observation::EventGuid(guid));
        Ok(self.event_guid)
    }
    fn creature_force_dynamic(&mut self) -> Result<bool> {
        Ok(self.dynamic)
    }
    fn creature_group(&mut self, _: u32) -> Result<Option<CreatureGroup>> {
        Ok(self.group)
    }
    fn creature_entry(&mut self, _: u32, owner: CreatureEntryOwner) -> Result<u32> {
        self.observations.push(Observation::Entry(owner));
        Ok(self.entries[match owner {
            CreatureEntryOwner::Group(_) => 0,
            CreatureEntryOwner::Conditional => 1,
            CreatureEntryOwner::Random { .. } => 2,
        }])
    }
    fn creature_event(&mut self, _: u32) -> Result<Option<CreatureEventData>> {
        Ok(self.event)
    }
    fn creature_template(&mut self, entry: u32) -> Result<Option<CreatureTemplate>> {
        Ok(self.templates.get(&entry).cloned())
    }
    fn creature_model(&mut self, model: u32) -> Result<Option<CreatureModel>> {
        Ok(self.models.get(&model).copied())
    }
    fn creature_guids(&mut self) -> &mut MapGuidAllocators {
        &mut self.guids
    }
    fn creature_random(&mut self) -> Result<&mut dyn CreatureRandom> {
        Ok(&mut self.random)
    }
    fn creature_respawn_time(&mut self, guid: u32) -> Result<i64> {
        self.observations.push(Observation::Persistent(guid));
        Ok(self.saved_time)
    }
    fn creature_save_respawn_time(&mut self, guid: u32, time: i64) -> Result<()> {
        self.observations.push(Observation::Save(guid, time));
        self.saved_time = time;
        Ok(())
    }
    fn creature_wall_time(&mut self, _: ObjectHandle) -> Result<ClockSample> {
        self.observations.push(Observation::Clock);
        Ok(ClockSample::authored(ClockKind::WallSeconds, self.now))
    }
    fn creature_spawned_by_linked_mob(&mut self, _: &CreatureConstruction) -> Result<bool> {
        Ok(self.linked)
    }
    fn creature_linked_can_spawn(&mut self, _: &CreatureConstruction) -> Result<bool> {
        self.linked_checks += 1;
        Ok(self.can_spawn)
    }
    fn creature_dead_flying_height(&mut self, _: &CreatureConstruction) -> Result<f32> {
        Ok(2.0)
    }
    fn creature_phase(
        &mut self,
        phase: CreaturePhase,
        state: &mut CreatureConstruction,
        context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        if self.refuse == Some(phase) {
            return Err(phase.missing_owner());
        }
        self.observations
            .push(Observation::Phase(state.spawn.database_guid, phase));
        match phase {
            CreaturePhase::InitEntryBase => {
                self.health.insert(state.object, 1);
            }
            CreaturePhase::SelectLevelStats => {
                if let Some(input) = &self.kernel {
                    let initialized = resolve_supported_creature_kernel(input, state)?;
                    self.health
                        .insert(state.object, initialized.stats.resources.health);
                    self.initialized.insert(state.object, initialized);
                } else {
                    self.health.insert(state.object, state.level * 10);
                }
            }
            CreaturePhase::SetDeadAndZeroHealth => {
                self.health.insert(state.object, 0);
            }
            CreaturePhase::LinkingRegistration => {
                state.linking_trigger = self.linking_trigger;
            }
            CreaturePhase::LoadAddon => {
                state.active = self.active;
            }
            CreaturePhase::MapAddViewpoint => {
                assert!(context.objects.get(state.object)?.in_world);
                assert!(context.cells.registered_cell(state.object).is_some());
            }
            CreaturePhase::Relay(_) => {
                if let Some(nested) = self.nested.take() {
                    context.load_grid(nested, GridLoadTrigger::Enter, self)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn creature_created(&mut self, state: CreatureConstruction) -> Result<()> {
        self.observations
            .push(Observation::Complete(state.spawn.database_guid));
        self.states.insert(state.object, state);
        Ok(())
    }
    fn creature_destroy_candidate(
        &mut self,
        object: ObjectHandle,
        _: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        self.observations.push(Observation::Destroy(object));
        self.health.remove(&object);
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
    fn new(spawns: Vec<CreatureSpawn>) -> Self {
        let mut sets = GridSpawnSets::new(map());
        for row in &spawns {
            sets.register(
                cell(row.position[0]),
                GridSpawnKind::Creature,
                GridSpawnOrigin::Static,
                row.database_guid,
                u64::from(row.database_guid),
            )
            .unwrap();
        }
        let mut cells = SourceCells::new(map()).unwrap();
        cells.configure_grids(300000, true).unwrap();
        Self {
            definitions: Definitions(spawns.into_iter().map(|s| (s.database_guid, s)).collect()),
            products: Products::default(),
            sets,
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
    fn load(&mut self, x: f32, factory: &mut Factory) -> Result<()> {
        self.context()
            .load_grid(cell(x), GridLoadTrigger::Enter, factory)
            .map(|_| ())
    }
}
#[test]
fn startup_loader_constructs_before_player_entry_and_equal_model_draws_skip_equal_level() {
    let mut world = World::new(vec![spawn(40, 0.0)]);
    let mut factory = Factory::new();
    world.load(0.0, &mut factory).unwrap();
    let (&handle, state) = factory.states.first_key_value().unwrap();
    assert_eq!(
        handle,
        ObjectHandle {
            thread: 2,
            construction: 1
        }
    );
    assert_eq!(state.level, 5);
    assert_eq!(state.respawn_delay, 100);
    assert_eq!(state.corpse_delay, 90);
    assert_eq!(factory.random.cursor.raw, 2);
    let raw: Vec<_> = factory
        .random
        .events
        .iter()
        .filter_map(|event| {
            if let benilla_sim::source_rng::EventKind::Raw { word, .. } = event.kind {
                Some(word)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(raw, [3_499_211_612, 581_869_302]);
    assert_eq!(
        world.objects.get(handle).unwrap().guid.unwrap().raw(),
        0xf130_0000_0700_0028
    );
    let player = world
        .objects
        .construct(2, SourceObjectKind::Player)
        .unwrap();
    assert_eq!(player.construction, 2);
    let view = factory
        .observations
        .iter()
        .position(|o| *o == Observation::Phase(40, CreaturePhase::MapAddViewpoint))
        .unwrap();
    let motion = factory
        .observations
        .iter()
        .position(|o| *o == Observation::Phase(40, CreaturePhase::MotionInitialize))
        .unwrap();
    let loader = factory
        .observations
        .iter()
        .position(|o| *o == Observation::LoaderView(40))
        .unwrap();
    assert!(view < motion && motion < loader);
}
#[test]
fn duplicate_alive_skip_constructs_and_destroys_without_draws_or_entry_queries() {
    let mut world = World::new(vec![spawn(40, 0.0)]);
    let mut factory = Factory::new();
    world.load(0.0, &mut factory).unwrap();
    world
        .sets
        .register(
            cell(0.0),
            GridSpawnKind::Creature,
            GridSpawnOrigin::Persistent,
            40,
            40,
        )
        .unwrap();
    let grid = world
        .cells
        .grids()
        .identity(cell(0.0).grid_x(), cell(0.0).grid_y())
        .unwrap();
    let attempt = GridLoadAttempt {
        grid,
        cell: cell(0.0),
        kind: GridSpawnKind::Creature,
        origin: GridSpawnOrigin::Persistent,
        guid: 40,
        catalog_ordinal: 40,
    };
    let before = factory.random.cursor.raw;
    factory.observations.clear();
    let outcome = world
        .context()
        .load_creature(&mut factory, attempt, spawn(40, 0.0))
        .unwrap();
    let failed = ObjectHandle {
        thread: 2,
        construction: 2,
    };
    assert_eq!(outcome, GridCandidateOutcome::FailedAndDestroyed(failed));
    assert!(world.objects.get(failed).is_err());
    assert_eq!(factory.random.cursor.raw, before);
    assert_eq!(
        factory.observations,
        [
            Observation::Construct(failed),
            Observation::EventGuid(40),
            Observation::Destroy(failed)
        ]
    );
}
#[test]
fn zero_entry_group_conditional_random_and_event_precedence_preserves_zero_db_lookup() {
    let group = CreatureGroup {
        id: 1,
        string_id: 2,
        has_formation: false,
    };
    for (entries, expected_queries, expected_original) in [
        ([7, 0, 0], 1, 7),
        ([0, 7, 0], 2, 7),
        ([0, 0, 7], 3, 7),
        ([0, 0, 0], 3, 8),
    ] {
        let mut row = spawn(40, 0.0);
        row.entry = 0;
        let mut world = World::new(vec![row]);
        let mut factory = Factory::new();
        factory.group = Some(group);
        factory.entries = entries;
        factory.event = Some(CreatureEventData {
            entry: 8,
            ..Default::default()
        });
        world.load(0.0, &mut factory).unwrap();
        let state = factory.states.values().next().unwrap();
        assert_eq!(state.original_entry, expected_original);
        assert_eq!(state.template.entry, 8);
        let queries: Vec<_> = factory
            .observations
            .iter()
            .filter_map(|o| {
                if let Observation::Entry(e) = o {
                    Some(*e)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(queries.len(), expected_queries);
        if expected_queries == 3 {
            assert_eq!(
                queries[2],
                CreatureEntryOwner::Random {
                    lookup_database_guid: 0
                }
            );
        }
        assert_eq!(
            world
                .objects
                .get(state.object)
                .unwrap()
                .guid
                .unwrap()
                .entry(),
            expected_original
        );
    }
}
#[test]
fn static_extra_dynamic_group_event_and_forced_maps_use_exact_guid_branch() {
    for (extra, group, event, forced, expected) in [
        (false, false, false, false, 40),
        (true, false, false, false, 110),
        (false, true, false, false, 110),
        (false, false, true, false, 110),
        (false, false, false, true, 110),
    ] {
        let mut world = World::new(vec![spawn(40, 0.0)]);
        let mut factory = Factory::new();
        factory.templates.get_mut(&7).unwrap().dynamic_guid = extra;
        factory.group = group.then_some(CreatureGroup {
            id: 1,
            string_id: 0,
            has_formation: false,
        });
        factory.event_guid = event;
        factory.dynamic = forced;
        world.load(0.0, &mut factory).unwrap();
        let state = factory.states.values().next().unwrap();
        assert_eq!(
            world
                .objects
                .get(state.object)
                .unwrap()
                .guid
                .unwrap()
                .counter(),
            expected
        );
    }
}
#[test]
fn failed_missing_model_keeps_display_draw_and_dynamic_allocation_before_destruction() {
    let mut world = World::new(vec![spawn(40, 0.0)]);
    let mut factory = Factory::new();
    factory.event_guid = true;
    factory.models.clear();
    world.load(0.0, &mut factory).unwrap();
    assert!(factory.states.is_empty());
    assert_eq!(factory.random.cursor.raw, 1);
    let object = ObjectHandle {
        thread: 2,
        construction: 1,
    };
    assert!(world.objects.get(object).is_err());
    assert_eq!(
        factory.observations.last(),
        Some(&Observation::Destroy(object))
    );
    assert_eq!(
        factory
            .guids
            .allocate(map(), HighGuid::Creature)
            .unwrap()
            .counter,
        111
    );
}
#[test]
fn future_respawn_static_dead_dynamic_without_group_fails_and_group_dead_survives() {
    for (dynamic, grouped, loaded) in [
        (false, false, true),
        (true, false, false),
        (true, true, true),
    ] {
        let mut world = World::new(vec![spawn(40, 0.0)]);
        let mut factory = Factory::new();
        factory.dynamic = dynamic;
        factory.group = grouped.then_some(CreatureGroup {
            id: 1,
            string_id: 0,
            has_formation: false,
        });
        factory.saved_time = 1001;
        factory.templates.get_mut(&7).unwrap().can_fly = true;
        factory.linked = true;
        factory.can_spawn = false;
        world.load(0.0, &mut factory).unwrap();
        assert_eq!(factory.random.cursor.raw, 2);
        assert_eq!(factory.states.len(), usize::from(loaded));
        assert_eq!(factory.linked_checks, 0);
        if loaded {
            let state = factory.states.values().next().unwrap();
            assert!(!world.objects.get(state.object).unwrap().alive);
            assert_eq!(state.position[2], 2.0);
            assert_eq!(factory.health[&state.object], 0);
            assert_eq!(state.respawn_time, 1001);
        }
        assert!(!factory
            .observations
            .iter()
            .any(|o| matches!(o, Observation::Save(..))));
    }
}
#[test]
fn expired_respawn_including_threshold_equality_saves_zero_before_map_add() {
    for saved in [1, 999, 1000] {
        let mut world = World::new(vec![spawn(40, 0.0)]);
        let mut factory = Factory::new();
        factory.saved_time = saved;
        world.load(0.0, &mut factory).unwrap();
        let state = factory.states.values().next().unwrap();
        assert_eq!(state.respawn_time, 0);
        let save = factory
            .observations
            .iter()
            .position(|o| *o == Observation::Save(40, 0))
            .unwrap();
        let add = factory
            .observations
            .iter()
            .position(|o| *o == Observation::Phase(40, CreaturePhase::MapAddToWorld))
            .unwrap();
        assert!(save < add);
        assert!(world.objects.get(state.object).unwrap().alive);
    }
}
#[test]
fn relay_reenters_real_loader_and_finishes_child_before_parent_link_group_callbacks() {
    let mut parent = spawn(40, 0.0);
    parent.relay = 9;
    let mut world = World::new(vec![parent, spawn(41, 600.0)]);
    let mut factory = Factory::new();
    factory.nested = Some(cell(600.0));
    factory.group = Some(CreatureGroup {
        id: 1,
        string_id: 2,
        has_formation: true,
    });
    factory.linking_trigger = true;
    world.load(0.0, &mut factory).unwrap();
    assert_eq!(factory.states.len(), 2);
    assert_eq!(factory.random.cursor.raw, 4);
    let filtered: Vec<_> = factory
        .observations
        .iter()
        .filter(|o| {
            matches!(
                o,
                Observation::Phase(40, CreaturePhase::Relay(_))
                    | Observation::Complete(41)
                    | Observation::Phase(
                        40,
                        CreaturePhase::LinkingRespawn
                            | CreaturePhase::FormationSlot
                            | CreaturePhase::GroupRespawn
                    )
                    | Observation::Complete(40)
            )
        })
        .cloned()
        .collect();
    assert_eq!(
        filtered,
        [
            Observation::Phase(40, CreaturePhase::Relay(9)),
            Observation::Complete(41),
            Observation::Phase(40, CreaturePhase::LinkingRespawn),
            Observation::Phase(40, CreaturePhase::FormationSlot),
            Observation::Phase(40, CreaturePhase::GroupRespawn),
            Observation::Complete(40)
        ]
    );
}
#[test]
fn second_grid_load_reconstructs_static_guid_with_new_construction_and_continuing_rng() {
    let mut world = World::new(vec![spawn(40, 0.0)]);
    let mut factory = Factory::new();
    world.load(0.0, &mut factory).unwrap();
    let old = factory.states.keys().next().copied().unwrap();
    let old_guid = world.objects.get(old).unwrap().guid.unwrap();
    let grid = world
        .cells
        .grids()
        .identity(cell(0.0).grid_x(), cell(0.0).grid_y())
        .unwrap();
    world.objects.remove_from_world(old).unwrap();
    world.objects.destroy(old, &mut world.cells).unwrap();
    world.cells.destroy_empty_grid(grid).unwrap();
    factory.states.clear();
    world.load(0.0, &mut factory).unwrap();
    let next = factory.states.keys().next().copied().unwrap();
    assert_ne!(old, next);
    assert_eq!(next.construction, 2);
    assert_eq!(factory.random.cursor.raw, 4);
    assert_eq!(world.objects.get(next).unwrap().guid.unwrap(), old_guid);
    assert!(world.objects.get(old).is_err());
}
#[test]
fn unavailable_reached_ai_owner_refuses_after_map_add_without_fake_destruction_or_callbacks() {
    let mut world = World::new(vec![spawn(40, 0.0)]);
    let mut factory = Factory::new();
    factory.refuse = Some(CreaturePhase::SelectAi);
    let refusal = world.load(0.0, &mut factory).unwrap_err();
    assert_eq!(refusal.id, "execution.creature.ai_owner");
    assert_eq!(factory.random.cursor.raw, 2);
    assert!(factory.states.is_empty());
    assert!(
        world
            .objects
            .get(ObjectHandle {
                thread: 2,
                construction: 1
            })
            .unwrap()
            .in_world
    );
    assert!(!factory
        .observations
        .iter()
        .any(|o| matches!(o, Observation::Destroy(_) | Observation::LoaderView(_))));
    assert_eq!(world.load(0.0, &mut factory).unwrap_err(), refusal);
}
#[test]
fn missing_gender_alternate_preserves_original_after_gender_draw_and_varied_level_draw() {
    let mut world = World::new(vec![spawn(40, 0.0)]);
    let mut factory = Factory::new();
    factory.models.get_mut(&11).unwrap().other_gender = 12;
    factory.templates.get_mut(&7).unwrap().levels = [5, 6];
    world.load(0.0, &mut factory).unwrap();
    let state = factory.states.values().next().unwrap();
    assert_eq!(state.model.unwrap().id, 11);
    assert!((5..=6).contains(&state.level));
    assert_eq!(factory.random.cursor.raw, 4);
}

#[test]
fn supported_kernel_reuses_consumed_source_rolls_at_select_level_without_extra_rng() {
    use benilla_sim::creature_init as init;
    let input = super::super::synthetic_creature_generation_fixture::source(40, 5, 6).input;
    let mut row = spawn(40, 0.0);
    row.entry = input.template.entry;
    let mut world = World::new(vec![row]);
    let mut factory = Factory::new();
    let mut template = template(input.template.entry);
    template.displays = input.template.displays;
    template.levels = [5, 6];
    template.corpse_seconds = input.template.corpse_delay_seconds;
    factory.templates.insert(template.entry, template);
    let init::Rows::Complete(models) = &input.models else {
        unreachable!()
    };
    factory.models = models
        .iter()
        .map(|model| {
            (
                model.id,
                CreatureModel {
                    id: model.id,
                    gender: model.gender,
                    other_gender: model.other_gender_id,
                },
            )
        })
        .collect();
    factory.kernel = Some(input);
    world.load(0.0, &mut factory).unwrap();
    let state = factory.states.values().next().unwrap();
    let initialized = &factory.initialized[&state.object];
    assert_eq!(initialized.entry, 60001);
    assert_eq!(u32::from(initialized.level), state.level);
    assert_eq!(initialized.model.display_id, state.model.unwrap().id);
    assert_eq!(factory.health[&state.object], 1);
    assert_eq!(factory.random.cursor.raw, 4);
    let mut wrong = factory.kernel.as_ref().unwrap().clone();
    wrong.spawn_id = 41;
    assert_eq!(
        resolve_supported_creature_kernel(&wrong, state)
            .unwrap_err()
            .id,
        "execution.creature.kernel_operands"
    );
    assert_eq!(factory.random.cursor.raw, 4);
}

#[test]
fn zero_entry_missing_templates_and_post_draw_invalid_position_destroy_at_source_failure() {
    for branch in 0..3 {
        let mut world = World::new(vec![spawn(40, 0.0)]);
        let mut factory = Factory::new();
        match branch {
            0 => {
                world.definitions.0.get_mut(&40).unwrap().entry = 0;
            }
            1 => {
                factory.templates.clear();
            }
            2 => {
                world.definitions.0.get_mut(&40).unwrap().position[2] = f32::NAN;
            }
            _ => unreachable!(),
        }
        world.load(0.0, &mut factory).unwrap();
        assert!(factory.states.is_empty());
        assert!(world
            .objects
            .get(ObjectHandle {
                thread: 2,
                construction: 1
            })
            .is_err());
        assert_eq!(factory.random.cursor.raw, if branch == 2 { 1 } else { 0 });
    }
}

#[test]
fn event_model_overrides_spawn_model_without_display_draw_and_missing_override_template_fails_after_guid(
) {
    let mut row = spawn(40, 0.0);
    row.model = 99;
    let mut world = World::new(vec![row]);
    let mut factory = Factory::new();
    factory.event = Some(CreatureEventData {
        entry: 8,
        model: 11,
        equipment: 0,
    });
    world.load(0.0, &mut factory).unwrap();
    let state = factory.states.values().next().unwrap();
    assert_eq!(state.template.entry, 8);
    assert_eq!(state.original_entry, 7);
    assert_eq!(factory.random.cursor.raw, 1);
    assert_eq!(state.rolls.model, None);
    let mut world = World::new(vec![spawn(40, 0.0)]);
    let mut factory = Factory::new();
    factory.event_guid = true;
    factory.event = Some(CreatureEventData {
        entry: 99,
        ..Default::default()
    });
    world.load(0.0, &mut factory).unwrap();
    assert!(factory.states.is_empty());
    assert_eq!(factory.random.cursor.raw, 0);
    assert_eq!(
        factory
            .guids
            .allocate(map(), HighGuid::Creature)
            .unwrap()
            .counter,
        111
    );
}

#[test]
fn display_fallback_keeps_equal_bound_draw_and_respawn_cap_keeps_uint32_wrap() {
    let mut world = World::new(vec![spawn(40, 0.0)]);
    let mut factory = Factory::new();
    factory.templates.get_mut(&7).unwrap().displays = [DisplayChoice {
        id: 0,
        probability: 0,
    }; 4];
    let mut fallback = template(1);
    fallback.displays[0].id = 11;
    factory.templates.insert(1, fallback);
    world.definitions.0.get_mut(&40).unwrap().respawn_seconds = [u32::MAX; 2];
    factory.templates.get_mut(&7).unwrap().corpse_seconds = u32::MAX;
    world.load(0.0, &mut factory).unwrap();
    let state = factory.states.values().next().unwrap();
    assert_eq!(state.model.unwrap().id, 11);
    assert_eq!(state.respawn_delay, u32::MAX);
    assert_eq!(state.corpse_delay, 429_496_728);
    assert_eq!(factory.random.cursor.raw, 2);
}

#[test]
fn linking_denial_kills_before_add_and_active_add_does_not_recurse_into_loaded_grid() {
    let mut world = World::new(vec![spawn(40, 0.0)]);
    let mut factory = Factory::new();
    factory.linked = true;
    factory.can_spawn = false;
    factory.linking_trigger = true;
    factory.active = true;
    world.load(0.0, &mut factory).unwrap();
    let state = factory.states.values().next().unwrap();
    assert_eq!(factory.linked_checks, 1);
    assert!(state.spawning_linked);
    assert!(!world.objects.get(state.object).unwrap().alive);
    assert_eq!(state.respawn_position, [0.0, 0.0, 4.0, 0.0]);
    assert_eq!(state.spawn.respawn_radius, 5.0);
    assert_eq!(world.products.0, 1);
    let death = factory
        .observations
        .iter()
        .position(|o| *o == Observation::Phase(40, CreaturePhase::SetDeadAndZeroHealth))
        .unwrap();
    let active = factory
        .observations
        .iter()
        .position(|o| *o == Observation::Phase(40, CreaturePhase::MapAddActive))
        .unwrap();
    let view = factory
        .observations
        .iter()
        .position(|o| *o == Observation::Phase(40, CreaturePhase::MapAddViewpoint))
        .unwrap();
    assert!(death < active && active < view);
    assert!(!factory
        .observations
        .contains(&Observation::Phase(40, CreaturePhase::LinkingRespawn)));
}
