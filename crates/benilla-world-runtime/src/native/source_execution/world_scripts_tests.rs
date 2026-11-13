// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::native::source_execution::manager_inputs::tests::{binding, snapshot};
use benilla_sim::source_rng::EventKind;
use std::collections::VecDeque;

#[derive(Default)]
struct Outbound {
    operations: Vec<PersistentSql>,
    fail: bool,
}
impl PersistentEffects for Outbound {
    fn sql(&mut self, operation: PersistentSql) -> Result<()> {
        require(!self.fail, "authored.world_sql_owner")?;
        self.operations.push(operation);
        Ok(())
    }
    fn query_respawns(&mut self, _: GridSpawnKind) -> Result<CharacterQueryStatus> {
        panic!("unexpected respawn query")
    }
    fn observe(&mut self, _: PersistentOperation) -> Result<()> {
        Ok(())
    }
    fn dungeon_reset(&mut self, _: PersistentMapEntry) -> Result<i64> {
        panic!("unexpected dungeon reset")
    }
}

struct Words(VecDeque<u64>);
impl RawGenerator for Words {
    const MIN: u64 = 0;
    const MAX: u64 = u32::MAX as u64;
    fn next_word(&mut self) -> u64 {
        self.0.pop_front().expect("unexpected source draw")
    }
}
fn identity(map: u32) -> MapIdentity {
    MapIdentity {
        map,
        instance: 0,
        creation_generation: 1,
    }
}
fn initial(world: Option<&str>) -> ManagerInitialInputs<'static> {
    let mut raw = snapshot("", "", "");
    if let Some(rows) = world {
        raw.extend_from_slice(b"CREATE TABLE `world` (\n  `map` smallint,\n  `data` longtext,\n  PRIMARY KEY (`map`)\n) ENGINE=InnoDB;\nLOCK TABLES `world` WRITE;\n");
        if !rows.is_empty() {
            raw.extend_from_slice(format!("INSERT INTO `world` VALUES {rows};\n").as_bytes());
        }
        raw.extend_from_slice(b"UNLOCK TABLES;\n");
    }
    ManagerInitialInputs::authored(&raw, binding()).unwrap()
}
fn ready() -> SourceContinents {
    let mut terrain = SourceTerrainManager::default();
    let (mut maps, _) = construct(&mut terrain, GuidMaximum::NoResult);
    let cfg = ContinentConfig::parse(&config("120", "1", 300000)).unwrap();
    for map in &mut maps.maps {
        map.initialize_prelude((10, 20), &mut terrain, &cfg)
            .unwrap();
    }
    maps
}

#[test]
fn eastern_constructor_and_initialize_draw_in_order_and_retain_base_stores() {
    // world_eastern_kingdoms.cpp:30-38; both outcomes of each urand(0,1).
    let mut engine = Words([0, u32::MAX as u64, u32::MAX as u64, 0].into());
    let mut cursor = DrawCursor::default();
    let mut trace = Vec::new();
    let mut sink = |event| trace.push(event);
    let mut random = SourceRandom::new(6, &mut engine, &mut cursor, &mut sink).unwrap();
    let mut vars = BTreeMap::from([(7, 9)]);
    let mut script = SourceWorldScript::construct(
        identity(0),
        "world_map_eastern_kingdoms",
        &mut vars,
        &mut random,
    )
    .unwrap();
    assert_eq!(script.map, identity(0));
    assert_eq!(script.state, WorldScriptState::EasternKingdoms);
    assert_eq!(script.stores, WorldScriptStores::default());
    assert_eq!(vars, BTreeMap::from([(7, 9), (330001, 0), (330002, 1)]));
    assert_eq!(random.cursor().raw, 2);
    script.stores.creatures.insert(42, 100);
    script.initialize(&mut vars, &mut random).unwrap();
    assert_eq!(vars, BTreeMap::from([(7, 9), (330001, 1), (330002, 0)]));
    assert_eq!(script.stores.creatures[&42], 100);
    assert_eq!(random.cursor().raw, 4);
    assert!(trace.iter().all(|event| event.stream == 6));
    assert_eq!(
        trace
            .iter()
            .filter(|event| matches!(event.kind, EventKind::Raw { .. }))
            .count(),
        4
    );
}

#[test]
fn kalimdor_constructor_and_initialize_clear_every_derived_member_without_draws() {
    // world_kalimdor.cpp:42-68, including each empty/nonempty rift-list branch.
    let mut engine = Words(VecDeque::new());
    let mut cursor = DrawCursor::default();
    let mut trace = Vec::new();
    let mut sink = |event| trace.push(event);
    let mut random = SourceRandom::new(7, &mut engine, &mut cursor, &mut sink).unwrap();
    let mut vars = BTreeMap::from([(7, 9)]);
    let mut script =
        SourceWorldScript::construct(identity(1), "world_map_kalimdor", &mut vars, &mut random)
            .unwrap();
    assert_eq!(script.map, identity(1));
    assert_eq!(script.stores, WorldScriptStores::default());
    assert_eq!(
        script.state,
        WorldScriptState::Kalimdor(KalimdorWorldState::default())
    );
    let WorldScriptState::Kalimdor(state) = &mut script.state else {
        panic!("wrong factory")
    };
    state.murkdeep_killed_adds = 2;
    state.ghost_events.push(GhostOPlasmEvent {
        guid: 3,
        despawn_timer: 9,
        phase_counter: 1,
        summoned_magrami: vec![4],
    });
    state.omen_reset_timer = 5;
    state.omen_moonlight_timer = 6;
    state.rockets_counter = 7;
    state.encounters.fill(3);
    state.omen_spell_credit_done = true;
    for (i, rifts) in state.elemental_rifts.iter_mut().enumerate() {
        rifts.push(i as u64 + 1);
    }
    state.drones_timer = 8;
    state.freed_sprite_darter = 9;
    script.stores.gameobjects.insert(44, 45);
    script.initialize(&mut vars, &mut random).unwrap();
    assert_eq!(
        script.state,
        WorldScriptState::Kalimdor(KalimdorWorldState::default())
    );
    assert_eq!(script.stores.gameobjects[&44], 45);
    assert_eq!(vars, BTreeMap::from([(7, 9)]));
    assert_eq!(random.cursor().raw, 0);
    assert!(trace.is_empty());
}

#[test]
fn load_false_runs_initialize_again_after_the_constructor_for_both_scripts() {
    for (slot, name, words) in [
        (0, "world_map_eastern_kingdoms", 4),
        (1, "world_map_kalimdor", 0),
    ] {
        let mut maps = ready();
        let mut engine = Words([0, 0, u32::MAX as u64, u32::MAX as u64].into());
        let mut cursor = DrawCursor::default();
        let mut sink = |_| {};
        let mut random =
            SourceRandom::new(slot as u64 + 6, &mut engine, &mut cursor, &mut sink).unwrap();
        maps.maps[slot]
            .create_instance_data(1, name, false, &mut random, &mut UnreachedStorage)
            .unwrap();
        assert_eq!(random.cursor().raw, words);
        if slot == 0 {
            assert_eq!(
                maps.maps[slot].variables,
                BTreeMap::from([(330001, 1), (330002, 1)])
            );
        }
        assert!(maps.maps[slot].instance_data.is_some());
    }
}

#[test]
fn load_true_uses_snapshot_rows_and_inherited_empty_load_for_null_empty_and_nonempty_data() {
    for data in ["NULL", "''", "'not a serialized encounter'"] {
        let initial = initial(Some(&format!("(0,{data}),(1,{data})")));
        let mut maps = ready();
        for (slot, name) in [(0, "world_map_eastern_kingdoms"), (1, "world_map_kalimdor")] {
            let mut engine = Words([0, u32::MAX as u64].into());
            let mut cursor = DrawCursor::default();
            let mut sink = |_| {};
            let mut random =
                SourceRandom::new(slot as u64 + 6, &mut engine, &mut cursor, &mut sink).unwrap();
            let mut effects = Outbound::default();
            let mut storage = SnapshotWorldStorage {
                initial: &initial,
                effects: &mut effects,
                pending: Default::default(),
                issued: Vec::new(),
            };
            maps.maps[slot]
                .create_instance_data(1, name, true, &mut random, &mut storage)
                .unwrap();
            assert_eq!(
                storage.issued,
                [format!("SELECT data FROM world WHERE map = '{slot}'")]
            );
            assert_eq!(random.cursor().raw, [2, 0][slot]);
            if slot == 0 {
                assert_eq!(
                    maps.maps[slot].variables,
                    BTreeMap::from([(330001, 0), (330002, 1)])
                );
            } else {
                assert_eq!(
                    maps.maps[slot].instance_data.as_ref().unwrap().state,
                    WorldScriptState::Kalimdor(KalimdorWorldState::default())
                );
            }
        }
    }
}

#[test]
fn empty_world_snapshot_submits_pending_insert_after_query_and_constructor_on_each_thread() {
    let initial = initial(Some(""));
    let mut maps = ready();
    for (slot, name) in [(0, "world_map_eastern_kingdoms"), (1, "world_map_kalimdor")] {
        let mut engine = Words([u32::MAX as u64, u32::MAX as u64].into());
        let mut cursor = DrawCursor::default();
        let mut sink = |_| {};
        let mut random =
            SourceRandom::new(slot as u64 + 6, &mut engine, &mut cursor, &mut sink).unwrap();
        let mut effects = Outbound::default();
        let mut storage = SnapshotWorldStorage {
            initial: &initial,
            effects: &mut effects,
            pending: Default::default(),
            issued: Vec::new(),
        };
        maps.maps[slot]
            .create_instance_data(1, name, true, &mut random, &mut storage)
            .unwrap();
        assert_eq!(
            storage.issued,
            [
                format!("SELECT data FROM world WHERE map = '{slot}'"),
                format!("INSERT INTO world VALUES ('{slot}', '')")
            ]
        );
        assert_eq!(
            storage.query(slot as u32, "later read").unwrap_err().id,
            "execution.world_script.pending_world_read_owner"
        );
        assert_eq!(random.cursor().raw, [2, 0][slot]);
        assert_eq!(
            effects.operations,
            [PersistentSql::CreateWorld { map: slot as u32 }]
        );
        assert!(maps.maps[slot].instance_data.is_some());
        assert!(maps.maps[slot].persistent.is_none() && !maps.maps[slot].spawn_updated);
        assert_eq!(
            initial.world_data(slot as u32).unwrap(),
            WorldInstanceRow::Missing
        );
    }
}

#[test]
fn absent_world_snapshot_refuses_at_query_not_at_factory_or_insert() {
    let initial = initial(None);
    let mut maps = ready();
    for (slot, name) in [(0, "world_map_eastern_kingdoms"), (1, "world_map_kalimdor")] {
        let mut engine = Words([0, 0].into());
        let mut cursor = DrawCursor::default();
        let mut sink = |_| {};
        let mut random =
            SourceRandom::new(slot as u64 + 6, &mut engine, &mut cursor, &mut sink).unwrap();
        let mut effects = Outbound::default();
        let mut storage = SnapshotWorldStorage {
            initial: &initial,
            effects: &mut effects,
            pending: Default::default(),
            issued: Vec::new(),
        };
        assert_eq!(
            maps.maps[slot]
                .create_instance_data(1, name, true, &mut random, &mut storage)
                .unwrap_err()
                .id,
            "execution.world_script.character_world_query_owner"
        );
        assert!(maps.maps[slot].instance_data.is_some());
        assert_eq!(random.cursor().raw, [2, 0][slot]);
        assert!(storage.issued.is_empty());
    }
}

#[test]
fn existing_instance_data_returns_before_script_lookup_initialize_and_sql() {
    let mut maps = ready();
    let mut engine = Words([0, 0, 0, 0].into());
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    let mut random = SourceRandom::new(6, &mut engine, &mut cursor, &mut sink).unwrap();
    let map = &mut maps.maps[0];
    map.create_instance_data(
        1,
        "world_map_eastern_kingdoms",
        false,
        &mut random,
        &mut UnreachedStorage,
    )
    .unwrap();
    let before = random.cursor().raw;
    map.create_instance_data(99, "missing", true, &mut random, &mut UnreachedStorage)
        .unwrap();
    assert_eq!(map.script_id, 1);
    assert_eq!(random.cursor().raw, before);
}

#[test]
fn eastern_second_draw_failure_preserves_first_variable_and_refuses_before_sql() {
    let mut maps = ready();
    let mut engine = Words([u32::MAX as u64, u64::MAX].into());
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    let mut random = SourceRandom::new(6, &mut engine, &mut cursor, &mut sink).unwrap();
    let map = &mut maps.maps[0];
    assert_eq!(
        map.create_instance_data(
            1,
            "world_map_eastern_kingdoms",
            true,
            &mut random,
            &mut UnreachedStorage
        )
        .unwrap_err()
        .id,
        "execution.world_script.urand"
    );
    assert_eq!(map.variables, BTreeMap::from([(330001, 1)]));
    assert!(map.instance_data.is_none());
}

#[test]
fn world_snapshot_preserves_null_empty_and_escaped_data_without_using_recorded_results() {
    let initial = initial(Some("(0,NULL),(1,''),(2,'a\\'b\\\\c')"));
    assert_eq!(initial.world_data(0).unwrap(), WorldInstanceRow::Null);
    assert_eq!(initial.world_data(1).unwrap(), WorldInstanceRow::Data(b""));
    assert_eq!(
        initial.world_data(2).unwrap(),
        WorldInstanceRow::Data(b"a'b\\c")
    );
    assert_eq!(initial.world_data(3).unwrap(), WorldInstanceRow::Missing);
}

#[test]
fn world_snapshot_refuses_duplicate_map_and_incomplete_relation() {
    let raw = snapshot("", "", "");
    let prefix = "CREATE TABLE `world` (\n  `map` smallint,\n  `data` longtext,\n  PRIMARY KEY (`map`)\n) ENGINE=InnoDB;\nLOCK TABLES `world` WRITE;\n";
    for (suffix, expected) in [
        (
            "INSERT INTO `world` VALUES (0,''),(0,NULL);\nUNLOCK TABLES;\n",
            "execution.managers.snapshot_duplicate_key",
        ),
        ("", "execution.managers.character_relation_absent"),
    ] {
        let mut bytes = raw.clone();
        bytes.extend_from_slice(prefix.as_bytes());
        bytes.extend_from_slice(suffix.as_bytes());
        assert_eq!(
            ManagerInitialInputs::authored(&bytes, binding())
                .err()
                .unwrap()
                .id,
            expected
        );
    }
}

#[test]
fn world_insert_submission_failure_stops_before_persistent_attach() {
    let initial = initial(Some(""));
    let mut maps = ready();
    let mut engine = Words(VecDeque::new());
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    let mut random = SourceRandom::new(7, &mut engine, &mut cursor, &mut sink).unwrap();
    let mut effects = Outbound {
        fail: true,
        ..Outbound::default()
    };
    let mut storage = SnapshotWorldStorage {
        initial: &initial,
        effects: &mut effects,
        pending: Default::default(),
        issued: Vec::new(),
    };
    assert_eq!(
        maps.maps[1]
            .create_instance_data(1, "world_map_kalimdor", true, &mut random, &mut storage)
            .unwrap_err()
            .id,
        "authored.world_sql_owner"
    );
    assert_eq!(storage.issued, ["SELECT data FROM world WHERE map = '1'"]);
    assert!(storage.pending.is_empty());
    assert!(maps.maps[1].persistent.is_none());
    assert!(!maps.maps[1].instance_data_complete);
}

struct PoolInputs {
    template: Option<PoolTemplateRow>,
    members: VecDeque<PoolMembershipRow>,
    slot: usize,
}
impl PoolRows for PoolInputs {
    fn maximum(&mut self) -> Result<Option<u16>> {
        Ok(self.template.as_ref().map(|row| row.id))
    }
    fn template(&mut self) -> Result<Option<PoolTemplateRow>> {
        Ok(self.template.take())
    }
    fn member(&mut self, slot: usize) -> Result<Option<PoolMembershipRow>> {
        Ok(if slot == self.slot {
            self.members.pop_front()
        } else {
            None
        })
    }
}
struct PoolDefinitions(u32);
impl PoolLoadOwner for PoolDefinitions {
    fn spawn_map(&mut self, _: PoolKind, _: u32) -> Result<Option<u32>> {
        Ok(Some(self.0))
    }
    fn map_exists(&mut self, map: u32) -> Result<bool> {
        Ok(map == self.0)
    }
}
struct PoolEffects {
    engine: Mt19937,
    cursor: DrawCursor,
    thread: u64,
    persistent: Vec<PersistentOperation>,
    requested: bool,
}
impl PoolEffects {
    fn new(thread: u64) -> Self {
        Self {
            engine: Mt19937::from_seed(7),
            cursor: DrawCursor::default(),
            thread,
            persistent: Vec::new(),
            requested: false,
        }
    }
}
impl PersistentEffects for PoolEffects {
    fn sql(&mut self, _: PersistentSql) -> Result<()> {
        panic!("world persistent construction has no SQL")
    }
    fn query_respawns(&mut self, _: GridSpawnKind) -> Result<CharacterQueryStatus> {
        panic!("unexpected respawn query")
    }
    fn observe(&mut self, operation: PersistentOperation) -> Result<()> {
        self.persistent.push(operation);
        Ok(())
    }
    fn dungeon_reset(&mut self, _: PersistentMapEntry) -> Result<i64> {
        panic!("continent is not a dungeon")
    }
}
impl PoolWork for PoolEffects {
    fn engine_access(&mut self) -> Result<()> {
        Ok(())
    }
    fn chance(&mut self) -> Result<f64> {
        panic!("authored equal pool must not roll a chance")
    }
    fn shuffle(&mut self, values: &mut [usize]) -> Result<()> {
        let mut sink = |_| {};
        SourceRandom::new(self.thread, &mut self.engine, &mut self.cursor, &mut sink)
            .unwrap()
            .shuffle(values)
            .map_err(|_| refuse("execution.pool.shuffle"))
    }
    fn can_spawn(
        &mut self,
        _: &mut SourcePoolManager,
        states: &mut SourceManagers,
        state: PersistentStateIdentity,
        _: PoolKind,
        _: u32,
    ) -> Result<bool> {
        assert!(states.state(state)?.map_binding().is_some());
        assert!(states.state(state)?.pools().initialized());
        self.requested = true;
        Err(refuse("execution.pool.map_can_spawn_owner"))
    }
    fn leaf_effect(
        &mut self,
        _: &mut SourcePoolManager,
        _: &mut SourceManagers,
        _: PoolLeafEffect,
    ) -> Result<()> {
        panic!("refused eligibility must precede leaf effects")
    }
    fn observe(&mut self, _: PoolOperation) -> Result<()> {
        panic!("refused eligibility must precede roll result")
    }
}

fn empty_events(pools: &mut SourcePoolManager) -> SourceGameEventManager {
    struct Rows;
    impl GameEventRows for Rows {
        fn maximum(&mut self) -> Result<Option<u16>> {
            Ok(None)
        }
        fn definition(&mut self) -> Result<Option<GameEventDefinitionRow>> {
            Ok(None)
        }
        fn date(&mut self) -> Result<Option<GameEventDateRow>> {
            Ok(None)
        }
        fn member(&mut self, _: EventSpawnKind) -> Result<Option<GameEventMemberRow>> {
            Ok(None)
        }
        fn substitution(&mut self) -> Result<Option<GameEventSubstitutionRow>> {
            Ok(None)
        }
        fn quest(&mut self) -> Result<Option<(u64, u32, u16)>> {
            Ok(None)
        }
        fn mail(&mut self) -> Result<Option<GameEventMailRow>> {
            Ok(None)
        }
    }
    struct Owner;
    impl GameEventOwner for Owner {
        fn clock(&mut self, _: EventClockSite) -> Result<ClockSample> {
            panic!("no events")
        }
        fn reference(&mut self, _: EventReference, _: u32) -> Result<bool> {
            panic!("no events")
        }
        fn set_quest_active(&mut self, _: u32, _: bool) -> Result<()> {
            panic!("no events")
        }
        fn check_pool_event(
            &mut self,
            _: &SourcePoolManager,
            _: u16,
            _: i16,
            _: &BTreeMap<u32, i16>,
            _: &BTreeMap<u32, i16>,
        ) -> Result<()> {
            panic!("no events")
        }
        fn effect(&mut self, _: &mut SourceGameEventManager, _: GameEventEffect) -> Result<()> {
            panic!("no events")
        }
        fn observe(&mut self, _: GameEventObservation) -> Result<()> {
            Ok(())
        }
    }
    let mut events = SourceGameEventManager::default();
    events.load(&mut Rows, pools, &mut Owner).unwrap();
    events
}

#[test]
fn persistent_attach_and_pool_shuffle_precede_map_eligibility_refusal_on_each_map() {
    for slot in 0..2 {
        for member_slot in [2, 4] {
            let mut maps = ready();
            let map = &mut maps.maps[slot];
            create_data(map, 0).unwrap();
            let mut inputs = PoolInputs {
                template: Some(PoolTemplateRow {
                    ordinal: 0,
                    id: 0,
                    limit: 1,
                    description: Vec::new(),
                }),
                members: [1, 2]
                    .map(|guid| PoolMembershipRow {
                        ordinal: u64::from(guid - 1),
                        guid,
                        pool: 0,
                        chance: 0.0,
                    })
                    .into(),
                slot: member_slot,
            };
            let mut pools = SourcePoolManager::default();
            pools
                .load(&mut inputs, &mut PoolDefinitions(slot as u32))
                .unwrap();
            let mut states = SourceManagers::default();
            let mut work = PoolEffects::new(slot as u64 + 6);
            assert_eq!(
                map.initialize_persistent_pools(
                    &mut states,
                    &mut pools,
                    &SourceGameEventManager::default(),
                    &mut work
                )
                .unwrap_err()
                .id,
                "execution.pool.map_can_spawn_owner"
            );
            assert!(work.requested);
            assert_eq!(work.cursor.raw, 1);
            assert!(
                matches!(work.persistent.as_slice(), [PersistentOperation::Created { .. }, PersistentOperation::Binding { after: Some(bound), .. }] if *bound == map.identity)
            );
            assert!(states
                .state(map.persistent.unwrap())
                .unwrap()
                .pools()
                .initialized());
            assert!(!map.spawn_updated);
        }
    }
}

#[test]
fn incomplete_instance_refuses_before_attach_and_empty_events_reach_graveyard_owner() {
    let mut maps = ready();
    let map = &mut maps.maps[0];
    let mut states = SourceManagers::default();
    let mut pools = SourcePoolManager::default();
    let mut work = PoolEffects::new(6);
    assert_eq!(
        map.initialize_persistent_pools(
            &mut states,
            &mut pools,
            &SourceGameEventManager::default(),
            &mut work
        )
        .unwrap_err()
        .id,
        "execution.continents.instance_initialization_incomplete"
    );
    assert!(work.persistent.is_empty() && map.persistent.is_none());
    create_data(map, 0).unwrap();
    pools
        .load(
            &mut PoolInputs {
                template: None,
                members: VecDeque::new(),
                slot: 0,
            },
            &mut PoolDefinitions(0),
        )
        .unwrap();
    let events = empty_events(&mut pools);
    assert_eq!(
        map.initialize_persistent_pools(&mut states, &mut pools, &events, &mut work)
            .unwrap_err()
            .id,
        "execution.continents.graveyard_init_owner"
    );
    assert_eq!(work.cursor.raw, 0);
    assert!(states
        .state(map.persistent.unwrap())
        .unwrap()
        .pools()
        .initialized());
    assert!(!map.spawn_updated);
}

struct MapGate {
    reads: Vec<u32>,
}
impl MapGate {
    fn data(guid: u32) -> Option<LinkingSpawnData> {
        (guid == 1 || guid == 2).then_some(LinkingSpawnData {
            entry: guid + 10,
            map: 0,
            respawn_xy: [25.0_f32.to_bits(), 30.0_f32.to_bits()],
        })
    }
}
impl LinkingLoadOwner for MapGate {
    fn template_exists(&mut self, _: u32) -> Result<bool> {
        Ok(true)
    }
    fn map_exists(&mut self, map: u32) -> Result<bool> {
        Ok(map == 0)
    }
    fn creature_data(&mut self, guid: u32) -> Result<Option<LinkingSpawnData>> {
        Ok(Self::data(guid))
    }
    fn query_unique_master(&mut self, _: u32, _: u32) -> Result<LinkingMasterQuery> {
        panic!("GUID linking must not query a unique entry master")
    }
}
impl LinkingGateOwner for MapGate {
    fn creature_data(&mut self, guid: u32) -> Result<Option<LinkingSpawnData>> {
        self.reads.push(guid);
        Ok(Self::data(guid))
    }
    fn wall_time(&mut self, _: MapIdentity, _: u32) -> Result<ClockSample> {
        panic!("zero deadline short circuits time(nullptr)")
    }
    fn creature_pool(&mut self, guid: u32) -> Result<Option<u16>> {
        assert_eq!(guid, 2);
        Ok(None)
    }
    fn encounter_in_progress(&mut self, _: MapIdentity) -> Result<Option<bool>> {
        panic!("BOSS_ALIVE must not query InstanceData")
    }
}
struct GuidLink(Option<LinkingGuidRow>);
impl LinkingRows for GuidLink {
    fn next_template(&mut self) -> Result<Option<LinkingTemplateRow>> {
        Ok(None)
    }
    fn next_guid(&mut self) -> Result<Option<LinkingGuidRow>> {
        Ok(self.0.take())
    }
}
struct BoundMapWork<'a> {
    map: &'a SourceContinent,
    links: &'a SourceCreatureLinking,
    objects: SourceObjects,
    gate: MapGate,
    base: PoolEffects,
    effects: Vec<PoolLeafEffect>,
    results: Vec<bool>,
}
impl PoolWork for BoundMapWork<'_> {
    fn engine_access(&mut self) -> Result<()> {
        self.base.engine_access()
    }
    fn chance(&mut self) -> Result<f64> {
        self.base.chance()
    }
    fn shuffle(&mut self, indices: &mut [usize]) -> Result<()> {
        self.base.shuffle(indices)
    }
    fn can_spawn(
        &mut self,
        _: &mut SourcePoolManager,
        states: &mut SourceManagers,
        state: PersistentStateIdentity,
        kind: PoolKind,
        guid: u32,
    ) -> Result<bool> {
        let kind = match kind {
            PoolKind::Creature => SourceObjectKind::Creature,
            PoolKind::GameObject => SourceObjectKind::GameObject,
            PoolKind::Child => panic!("child bypasses map"),
        };
        let before = self.map.linking.inventory();
        let persistent = states.state(state)?.inventory();
        let result = self.map.can_spawn(
            kind,
            guid,
            self.links,
            &self.objects,
            states.state(state)?,
            &mut self.gate,
        )?;
        assert_eq!(before, self.map.linking.inventory());
        assert_eq!(persistent, states.state(state)?.inventory());
        self.results.push(result);
        Ok(result)
    }
    fn leaf_effect(
        &mut self,
        _: &mut SourcePoolManager,
        states: &mut SourceManagers,
        effect: PoolLeafEffect,
    ) -> Result<()> {
        assert!(states
            .state(effect.state)?
            .pools()
            .contains(match effect.kind {
                PoolKind::Creature => PoolMember::Creature(effect.guid),
                PoolKind::GameObject => PoolMember::GameObject(effect.guid),
                PoolKind::Child => panic!("child is not a leaf"),
            }));
        self.effects.push(effect);
        Ok(())
    }
    fn observe(&mut self, _: PoolOperation) -> Result<()> {
        Ok(())
    }
}
#[test]
fn map_can_spawn_runs_linking_but_pool_discards_false_and_null_map_skips_it() {
    // Map.cpp:137-144, PoolManager.cpp:369-381, CreatureLinkingMgr.cpp:663-750.
    for loaded in [false, true] {
        for slot in [2, 4] {
            let mut maps = ready();
            let map = &mut maps.maps[0];
            create_data(map, 0).unwrap();
            let mut states = SourceManagers::default();
            let state = map
                .attach_persistent_state(&mut states, &mut PoolEffects::new(6))
                .unwrap();
            if !loaded {
                states
                    .bind_map(state, None, &mut PoolEffects::new(6))
                    .unwrap();
            }
            let mut links = SourceCreatureLinking::default();
            links
                .load(
                    &mut GuidLink(Some(LinkingGuidRow {
                        ordinal: 0,
                        slave: 1,
                        master: 2,
                        flags: LINKING_BOSS_ALIVE,
                    })),
                    &mut MapGate { reads: Vec::new() },
                )
                .unwrap();
            let mut pools = SourcePoolManager::default();
            pools
                .load(
                    &mut PoolInputs {
                        template: Some(PoolTemplateRow {
                            ordinal: 0,
                            id: 1,
                            limit: 1,
                            description: Vec::new(),
                        }),
                        members: [PoolMembershipRow {
                            ordinal: 0,
                            guid: 1,
                            pool: 1,
                            chance: 0.0,
                        }]
                        .into(),
                        slot,
                    },
                    &mut PoolDefinitions(0),
                )
                .unwrap();
            let mut work = BoundMapWork {
                map,
                links: &links,
                objects: SourceObjects::default(),
                gate: MapGate { reads: Vec::new() },
                base: PoolEffects::new(6),
                effects: Vec::new(),
                results: Vec::new(),
            };
            pools
                .spawn_pool(&mut states, state, 1, true, &mut work)
                .unwrap();
            assert_eq!(work.effects.len(), 1);
            assert_eq!(work.base.cursor.raw, 0);
            assert_eq!(work.results, if loaded { vec![slot == 4] } else { vec![] });
            assert_eq!(
                work.gate.reads,
                if loaded && slot == 2 {
                    vec![1, 2]
                } else {
                    vec![]
                }
            );
            assert!(!map
                .can_spawn(
                    SourceObjectKind::Player,
                    1,
                    &links,
                    &work.objects,
                    states.state(state).unwrap(),
                    &mut work.gate
                )
                .unwrap());
        }
    }
}
