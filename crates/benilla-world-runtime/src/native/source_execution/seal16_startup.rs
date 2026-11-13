// SPDX-License-Identifier: GPL-3.0-or-later
//! Frozen diagnostic clock/calendar inputs. Manager outcomes are computed, never injected.
use super::*;
use crate::native::production_compiler::VerifiedCatalog;
use benilla_sim::{source_calendar::Tm, source_moon as moon};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

fn retained_f1_file(root: &Path, recorded: &Path, expected: &Sha256Id) -> Result<Vec<u8>> {
    let prefix = recorded
        .ancestors()
        .find(|path| path.ends_with("benilla-config/full-server/f1"))
        .ok_or_else(|| refuse("execution.startup.recorded_f1_path_component_missing"))?;
    let suffix = recorded.strip_prefix(prefix).unwrap();
    require(
        !suffix.as_os_str().is_empty()
            && suffix
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_))),
        "execution.startup.recorded_f1_path_suffix",
    )?;
    let bytes = std::fs::read(root.join(suffix))
        .map_err(|_| refuse("execution.startup.recorded_f1_file_missing"))?;
    require(
        Sha256Id::of(&bytes) == *expected,
        "execution.startup.recorded_f1_hash_mismatch",
    )?;
    Ok(bytes)
}

#[test]
fn retained_f1_snapshot_preserves_nested_suffix_after_relocation() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(root.join("src/lib.rs")).unwrap();
    assert_eq!(
        retained_f1_file(
            root,
            Path::new("/old/benilla-config/full-server/f1/src/lib.rs"),
            &Sha256Id::of(&bytes),
        )
        .unwrap(),
        bytes
    );
}

#[test]
fn retained_f1_snapshot_refuses_missing_component_and_hash_mismatch() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let wrong = Sha256Id::of(b"not the retained snapshot");
    assert_eq!(
        retained_f1_file(root, Path::new("/old/src/lib.rs"), &wrong)
            .unwrap_err()
            .id,
        "execution.startup.recorded_f1_path_component_missing"
    );
    assert_eq!(
        retained_f1_file(
            root,
            Path::new("/old/benilla-config/full-server/f1/src/lib.rs"),
            &wrong,
        )
        .unwrap_err()
        .id,
        "execution.startup.recorded_f1_hash_mismatch"
    );
    assert_eq!(
        retained_f1_file(
            root,
            Path::new("/old/benilla-config/full-server/f1/../src/lib.rs"),
            &wrong,
        )
        .unwrap_err()
        .id,
        "execution.startup.recorded_f1_path_suffix"
    );
}

#[derive(Clone, Debug)]
enum CalendarRecord {
    Now(i64),
    Local(i64, Tm),
    Epoch([i32; 7], i64, Tm),
}
fn tm(p: &[&str]) -> Tm {
    let n = |i: usize| p[i].parse().unwrap();
    Tm {
        sec: n(0),
        min: n(1),
        hour: n(2),
        mday: n(3),
        mon: n(4),
        year: n(5),
        wday: n(6),
        yday: n(7),
        isdst: n(8),
    }
}
#[derive(Default)]
struct MapThreadEvidence {
    initial: Option<benilla_sim::source_rng::MtState>,
    first_word: Option<(u64, u64, u64)>,
    before_sql: Vec<(u64, u64, u64)>,
    sql_line: Option<u64>,
    total_words: usize,
}
#[derive(Default)]
pub(super) struct Inputs {
    query: Vec<u8>,
    pointer: String,
    event: u16,
    clock_site: u64,
    query_operation: u64,
    query_occurrence: u64,
    load_clocks: VecDeque<(EventClockSite, ClockSample)>,
    update_clocks: VecDeque<i64>,
    check_clocks: VecDeque<i64>,
    calendar: VecDeque<CalendarRecord>,
    epoch: Option<[i32; 7]>,
    world_seconds: Option<i64>,
    persistent_owners: [Option<InputOwner>; 2],
    updating: bool,
    expected_sql: Vec<String>,
    map_births: usize,
    persistent_creations: Vec<(u32, u32)>,
    map_identities: BTreeMap<u64, (u64, String)>,
    map_bindings: BTreeMap<u32, (u64, u64, u64)>,
    map_threads: BTreeMap<u64, MapThreadEvidence>,
}
impl Inputs {
    pub(super) fn observe_thread(&mut self, thread: u64, line: u64, kind: &str, p: &[&str]) {
        if kind == "logical_birth" && p[1] == "map" {
            self.map_identities
                .insert(p[0].parse().unwrap(), (line, p[4].into()));
        }
        if kind == "constructor_persistent_state" && p[1] == "used_map_binding" {
            self.map_bindings
                .insert(p[2].parse().unwrap(), (thread, p[4].parse().unwrap(), line));
        }
        if !matches!(thread, 6 | 7) {
            return;
        }
        let evidence = self.map_threads.entry(thread).or_default();
        match kind {
            "mt_state" => {
                assert!(evidence.initial.is_none());
                evidence.initial = Some(benilla_sim::source_rng::MtState {
                    words: std::array::from_fn(|i| p[i + 1].parse().unwrap()),
                    index: p[625].parse().unwrap(),
                });
            }
            "mt_raw" => {
                let word = (line, p[1].parse().unwrap(), p[2].parse().unwrap());
                evidence.first_word.get_or_insert(word);
                if evidence.sql_line.is_none() {
                    evidence.before_sql.push(word);
                }
                evidence.total_words += 1;
            }
            "sql_request_bytes" => {
                evidence.sql_line.get_or_insert(line);
            }
            _ => (),
        }
    }
    pub(super) fn observe(&mut self, kind: &str, p: &[&str]) {
        match kind {
            "logical_birth" if p[1] == "map" => self.map_births += 1,
            "constructor_persistent_add" => self
                .persistent_creations
                .push((p[0].parse().unwrap(), p[1].parse().unwrap())),
            "sql_query_request" => self.query_operation = p[1].parse().unwrap(),
            "sql_query" => {
                self.query = unhex(p.last().unwrap());
                self.pointer = p[0].to_owned();
                self.event = 0;
                self.query_occurrence += 1;
                for (slot, query, line, function) in [
                    (0, &b"SELECT guid, respawntime, map, instance, resettime, encountersMask FROM creature_respawn LEFT JOIN instance ON instance = id"[..], 997, "MapPersistentStateManager::LoadCreatureRespawnTimes"),
                    (1, &b"SELECT guid, respawntime, map, instance, resettime, encountersMask FROM gameobject_respawn LEFT JOIN instance ON instance = id"[..], 1062, "MapPersistentStateManager::LoadGameobjectRespawnTimes"),
                ] {
                    if self.query == query {
                        assert!(self.persistent_owners[slot].is_none());
                        self.persistent_owners[slot] = Some(InputOwner {
                            thread: 1, occurrence: self.query_occurrence,
                            operation: OperationId { thread: 1, ordinal: self.query_operation },
                            map_instance: None, object: None,
                            callsite: Callsite {
                                core_revision: "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f".into(),
                                source_file: "src/game/Maps/MapPersistentStateMgr.cpp".into(), line,
                                function: function.into(), inlined_caller_chain: Vec::new(),
                                site_ordinal: self.query_occurrence,
                            },
                        });
                    }
                }
            }
            "sql_field"
                if p[0] == self.pointer
                    && p[1] == "0"
                    && (self.query.starts_with(b"SELECT entry,schedule_type,")
                        || self.query
                            == b"SELECT entry, start_time, end_time FROM game_event_time") =>
            {
                self.event = std::str::from_utf8(&unhex(p.last().unwrap()))
                    .unwrap()
                    .parse()
                    .unwrap();
            }
            "clock_callsite" if p[1] == "3678789157770927477" => {
                self.clock_site = p[2].parse().unwrap()
            }
            "clock_callsite" => self.clock_site = 0,
            "clock_time" => {
                let value = p[0].parse().unwrap();
                // Addresses resolve in the captured module-1 ELF, including inlined callers.
                match self.clock_site {
                    0x8329fa => self.world_seconds = Some(value), // World constructor, before respawn loading.
                    0xa40e8e => self.load_clocks.push_back((
                        EventClockSite::LoadSchedule(self.event),
                        ClockSample::authored(ClockKind::WallSeconds, value),
                    )),
                    0xa41f27 => self.load_clocks.push_back((
                        EventClockSite::LoadYearly(self.event),
                        ClockSample::authored(ClockKind::WallSeconds, value),
                    )),
                    0xa45ec1 | 0xa45f3e if self.lunar() => {
                        self.calendar.push_back(CalendarRecord::Now(value))
                    }
                    0xa3fd72 => {
                        self.update_clocks.push_back(value);
                        self.updating = true;
                    }
                    0xa400bb => self.check_clocks.push_back(value),
                    _ => (),
                }
            }
            "clock_localtime" if self.lunar() => self
                .calendar
                .push_back(CalendarRecord::Local(p[0].parse().unwrap(), tm(&p[1..]))),
            "clock_mktime_input" if self.lunar() => {
                assert_eq!(p.len(), 7);
                self.epoch = Some(std::array::from_fn(|i| p[i].parse().unwrap()));
            }
            "clock_mktime_result" if self.lunar() => {
                self.calendar.push_back(CalendarRecord::Epoch(
                    self.epoch.take().unwrap(),
                    p[0].parse().unwrap(),
                    tm(&p[1..]),
                ))
            }
            "sql_write_bytes" if self.updating => {
                let sql = String::from_utf8(unhex(p.last().unwrap())).unwrap();
                if sql.contains("game_event_status") {
                    self.expected_sql.push(sql);
                }
            }
            _ => (),
        }
    }
    fn lunar(&self) -> bool {
        self.event == 7 && self.query.starts_with(b"SELECT entry,schedule_type,")
    }
}
struct Calendar(VecDeque<CalendarRecord>);
impl moon::Calendar for Calendar {
    fn owner(&self) -> moon::CallOwner {
        moon::CallOwner { thread: 1, call: 7 }
    }
    fn now(&mut self) -> moon::Result<i64> {
        match self.0.pop_front() {
            Some(CalendarRecord::Now(value)) => Ok(value),
            _ => Err(moon::Error::MissingObservation),
        }
    }
    fn local(&mut self, seconds: i64) -> moon::Result<Tm> {
        match self.0.pop_front() {
            Some(CalendarRecord::Local(expected, value)) if expected == seconds => Ok(value),
            _ => Err(moon::Error::ObservationMismatch),
        }
    }
    fn epoch(&mut self, input: moon::CalendarInput) -> moon::Result<(i64, Tm)> {
        let fields = match input {
            moon::CalendarInput::Retained(t) => {
                [t.sec, t.min, t.hour, t.mday, t.mon, t.year, t.isdst]
            }
            moon::CalendarInput::AnchorProbe(d) => [0, 0, 3, d.day, d.month - 1, d.year - 1900, -1],
        };
        match self.0.pop_front() {
            Some(CalendarRecord::Epoch(expected, seconds, normalized)) if expected == fields => {
                Ok((seconds, normalized))
            }
            other => {
                println!("F6_FIRST_DIVERGENCE calendar_input expected={other:?} got={fields:?} candidate_causes=lunar_arithmetic,sine_dispatch,calendar_order");
                Err(moon::Error::ObservationMismatch)
            }
        }
    }
    fn finish(&self) -> moon::Result<()> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(moon::Error::SurplusObservation)
        }
    }
}
struct Definitions<'a> {
    spawns: &'a SourceEffectiveSpawns,
    maps: &'a BTreeMap<u32, u32>,
}
impl PersistentDefinitions for Definitions<'_> {
    fn map_entries(&mut self) -> Result<Vec<PersistentMapEntry>> {
        Ok(self
            .maps
            .iter()
            .map(|(&id, &map_type)| PersistentMapEntry { id, map_type })
            .collect())
    }
    fn spawn_map(&mut self, kind: GridSpawnKind, guid: u32) -> Result<Option<u32>> {
        Ok(self
            .spawns
            .pre_terrain_data(SpawnKey { kind, guid })?
            .map(|d| d.row.map))
    }
}
struct InitialEffects<'a> {
    initial: &'a ManagerInitialInputs<'a>,
    operations: Vec<PersistentOperation>,
    cleaned: [bool; 2],
}
impl PersistentEffects for InitialEffects<'_> {
    fn sql(&mut self, sql: PersistentSql) -> Result<()> {
        if let PersistentSql::Cleanup {
            kind,
            database_seconds,
        } = sql
        {
            require(
                database_seconds == self.initial.query(kind).database_seconds,
                "execution.persistent.cleanup_clock",
            )?;
            self.cleaned[usize::from(kind == GridSpawnKind::GameObject)] = true;
            Ok(())
        } else {
            Err(refuse("execution.persistent.initial_write_owner"))
        }
    }
    fn query_respawns(&mut self, kind: GridSpawnKind) -> Result<CharacterQueryStatus> {
        require(
            self.cleaned[usize::from(kind == GridSpawnKind::GameObject)],
            "execution.persistent.cleanup_owner",
        )?;
        Ok(self.initial.frozen_query_result(kind))
    }
    fn observe(&mut self, operation: PersistentOperation) -> Result<()> {
        self.operations.push(operation);
        Ok(())
    }
    fn dungeon_reset(&mut self, _: PersistentMapEntry) -> Result<i64> {
        Err(refuse("execution.persistent.dungeon_reset_owner"))
    }
}
struct Owner<'a> {
    spawns: &'a SourceEffectiveSpawns,
    maps: &'a BTreeMap<u32, u32>,
    ids: [BTreeSet<u32>; 5],
    lunar: ReferenceLunarEventOwner<'a>,
    clocks: VecDeque<(EventClockSite, ClockSample)>,
    update: VecDeque<i64>,
    checks: VecDeque<i64>,
    clock_count: usize,
    pools: SourcePoolManager,
    states: SourceManagers,
    loaded_maps: BTreeMap<(u32, u32), MapIdentity>,
    effects: SourceEventEffects,
    quests: BTreeMap<u32, bool>,
    status: BTreeSet<u16>,
    sql: Vec<String>,
    pool_operations: Vec<PoolOperation>,
    observations: Vec<GameEventObservation>,
    link_errors: Vec<(PoolKind, u32, i16)>,
    engine: &'a mut Mt19937,
    cursor: &'a mut DrawCursor,
    raw: Vec<(u64, u64)>,
}
impl GameEventOwner for Owner<'_> {
    fn clock(&mut self, site: EventClockSite) -> Result<ClockSample> {
        self.clock_count += 1;
        match site {
            EventClockSite::LoadSchedule(_) | EventClockSite::LoadYearly(_) => {
                let (expected, sample) = self
                    .clocks
                    .pop_front()
                    .ok_or_else(|| refuse("execution.event.recorded_load_clock_missing"))?;
                require(
                    expected == site,
                    "execution.event.recorded_load_clock_order",
                )?;
                Ok(sample)
            }
            EventClockSite::Update => self
                .update
                .pop_front()
                .map(|n| ClockSample::authored(ClockKind::WallSeconds, n))
                .ok_or_else(|| refuse("execution.event.recorded_update_clock_missing")),
            EventClockSite::NextCheck(_) => self
                .checks
                .pop_front()
                .map(|n| ClockSample::authored(ClockKind::WallSeconds, n))
                .ok_or_else(|| refuse("execution.event.recorded_check_clock_missing")),
            _ => Err(refuse("execution.event.recorded_clock_owner_missing")),
        }
    }
    fn lunar_bindings(&mut self) -> Result<LunarEventBindings<'_>> {
        Ok(self.lunar.bindings())
    }
    fn reference(&mut self, kind: EventReference, id: u32) -> Result<bool> {
        Ok(self.ids[kind as usize].contains(&id))
    }
    fn set_quest_active(&mut self, quest: u32, active: bool) -> Result<()> {
        require(
            self.ids[3].contains(&quest),
            "execution.event.quest_definition_missing",
        )?;
        self.quests.insert(quest, active);
        Ok(())
    }
    fn check_pool_event(
        &mut self,
        pools: &SourcePoolManager,
        pool: u16,
        event: i16,
        creatures: &BTreeMap<u32, i16>,
        gameobjects: &BTreeMap<u32, i16>,
    ) -> Result<()> {
        fn check(
            pools: &SourcePoolManager,
            pool: u16,
            event: i16,
            creatures: &BTreeMap<u32, i16>,
            gameobjects: &BTreeMap<u32, i16>,
            errors: &mut Vec<(PoolKind, u32, i16)>,
        ) -> Result<()> {
            let template = pools.pool_template(pool)?;
            for (kind, index) in [
                (PoolKind::GameObject, 1),
                (PoolKind::Creature, 2),
                (PoolKind::Child, 0),
            ] {
                let group = &template.groups[index];
                for object in group.equal.iter().chain(&group.explicit) {
                    if kind == PoolKind::Child {
                        check(
                            pools,
                            object.guid as u16,
                            event,
                            creatures,
                            gameobjects,
                            errors,
                        )?;
                    } else if (if kind == PoolKind::Creature {
                        creatures
                    } else {
                        gameobjects
                    })
                    .get(&object.guid)
                        != Some(&event)
                    {
                        errors.push((kind, object.guid, event));
                    }
                }
            }
            Ok(())
        }
        check(
            pools,
            pool,
            event,
            creatures,
            gameobjects,
            &mut self.link_errors,
        )
    }
    fn effect(
        &mut self,
        manager: &mut SourceGameEventManager,
        effect: GameEventEffect,
    ) -> Result<()> {
        let mut effects = std::mem::take(&mut self.effects);
        let mut pools = std::mem::take(&mut self.pools);
        let mut states = std::mem::take(&mut self.states);
        let result = effects.execute(manager, effect, 1, &mut pools, &mut states, self);
        self.effects = effects;
        self.pools = pools;
        self.states = states;
        result
    }
    fn observe(&mut self, observation: GameEventObservation) -> Result<()> {
        self.observations.push(observation);
        Ok(())
    }
}
impl PoolWork for Owner<'_> {
    fn engine_access(&mut self) -> Result<()> {
        Ok(())
    }
    fn chance(&mut self) -> Result<f64> {
        let mut sink = |e: TraceEvent| {
            if let EventKind::Raw { position, word, .. } = e.kind {
                self.raw.push((position, word));
            }
        };
        SourceRandom::new(1, self.engine, self.cursor, &mut sink)
            .unwrap()
            .rand_chance()
            .map_err(|_| refuse("execution.pool.rand_chance"))
    }
    fn shuffle(&mut self, values: &mut [usize]) -> Result<()> {
        let mut sink = |e: TraceEvent| {
            if let EventKind::Raw { position, word, .. } = e.kind {
                self.raw.push((position, word));
            }
        };
        SourceRandom::new(1, self.engine, self.cursor, &mut sink)
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
        require(
            states.state(state)?.map_binding().is_none(),
            "execution.pool.map_can_spawn_owner",
        )?;
        Ok(true)
    }
    fn leaf_effect(
        &mut self,
        _: &mut SourcePoolManager,
        states: &mut SourceManagers,
        effect: PoolLeafEffect,
    ) -> Result<()> {
        execute_pool_leaf(states, effect, self)
    }
    fn observe(&mut self, operation: PoolOperation) -> Result<()> {
        self.pool_operations.push(operation);
        Ok(())
    }
}
impl PoolStateOwner for Owner<'_> {
    fn pool_map_entry(&mut self, map: u32) -> Result<Option<PersistentMapEntry>> {
        Ok(self
            .maps
            .get(&map)
            .map(|&map_type| PersistentMapEntry { id: map, map_type }))
    }
    fn pool_next_instance_state(
        &mut self,
        states: &SourceManagers,
        map: u32,
        _: Option<PersistentStateIdentity>,
    ) -> Result<Option<PersistentStateIdentity>> {
        require(
            !states
                .inventories()
                .iter()
                .any(|s| s.identity.map == map && s.identity.instance != 0),
            "execution.pool.instance_state_traversal_owner",
        )?;
        Ok(None)
    }
}
impl EventEffectOwner for Owner<'_> {
    fn event_definition(&mut self, key: SpawnKey) -> Result<Option<EventSpawnDefinition>> {
        Ok(self
            .spawns
            .pre_terrain_group_definition(key)?
            .map(|d| EventSpawnDefinition {
                map: d.map,
                cell: d.cell,
                entry: d.entry,
                guid: u64::from(key.guid),
            }))
    }
    fn event_next_map(
        &mut self,
        map_id: Option<u32>,
        after: Option<MapIdentity>,
    ) -> Result<Option<MapIdentity>> {
        Ok(self
            .loaded_maps
            .values()
            .find(|map| {
                map_id.is_none_or(|id| id == map.map)
                    && after.is_none_or(|previous| {
                        (map.map, map.instance) > (previous.map, previous.instance)
                    })
            })
            .copied())
    }
    fn event_post_map(&mut self, _: MapIdentity, _: MessageEnvelope) -> Result<()> {
        Err(refuse("execution.event.map_traversal_owner"))
    }
    fn event_sql(&mut self, _: u64, sql: &str) -> Result<()> {
        if sql == "TRUNCATE TABLE game_event_status" {
            self.status.clear();
        } else if let Some(id) = sql
            .strip_prefix("INSERT INTO game_event_status (event) VALUES (")
            .and_then(|s| s.strip_suffix(')'))
        {
            self.status.insert(
                id.parse()
                    .map_err(|_| refuse("execution.event.status_sql"))?,
            );
        } else if let Some(id) = sql.strip_prefix("DELETE FROM game_event_status WHERE event = ") {
            self.status.remove(
                &id.parse()
                    .map_err(|_| refuse("execution.event.status_sql"))?,
            );
        } else {
            return Err(refuse("execution.event.status_sql"));
        }
        self.sql.push(sql.to_owned());
        Ok(())
    }
    fn event_announce(&mut self, _: u16, _: &[u8]) -> Result<()> {
        Err(refuse("execution.event.startup_announce_owner"))
    }
    fn event_quest(&mut self, quest: u32, active: bool) -> Result<()> {
        self.set_quest_active(quest, active)
    }
    fn event_has_instance(&mut self, _: MapIdentity) -> Result<bool> {
        Err(EventPending::Instance.refusal())
    }
}
impl PersistentEffects for Owner<'_> {
    fn sql(&mut self, _: PersistentSql) -> Result<()> {
        Err(refuse("execution.pool.respawn_storage_owner"))
    }
    fn query_respawns(&mut self, _: GridSpawnKind) -> Result<CharacterQueryStatus> {
        Err(refuse("execution.pool.respawn_query_unreached"))
    }
    fn observe(&mut self, _: PersistentOperation) -> Result<()> {
        Ok(())
    }
    fn dungeon_reset(&mut self, _: PersistentMapEntry) -> Result<i64> {
        Err(refuse("execution.persistent.dungeon_reset_owner"))
    }
}
impl PoolLeafOwner for Owner<'_> {
    fn pool_spawn(
        &mut self,
        kind: GridSpawnKind,
        guid: u32,
    ) -> Result<Option<PoolSpawnDefinition>> {
        let key = SpawnKey { kind, guid };
        let Some(data) = self.spawns.pre_terrain_data(key)? else {
            return Ok(None);
        };
        let cell = self.spawns.pre_terrain_group_definition(key)?.unwrap().cell;
        Ok(Some(PoolSpawnDefinition {
            map: data.row.map,
            cell,
            entry: data.row.entry,
            respawn: data.row.respawn,
            gameobject_min: data.row.respawn[0] as i32,
        }))
    }
    fn pool_grid_loaded(
        &mut self,
        _: MapIdentity,
        _: benilla_sim::resident_map::CellPair,
    ) -> Result<bool> {
        Err(refuse("execution.pool.loaded_grid_owner"))
    }
    fn pool_random_respawn(&mut self, bounds: [u32; 2]) -> Result<u32> {
        let mut sink = |e: TraceEvent| {
            if let EventKind::Raw { position, word, .. } = e.kind {
                self.raw.push((position, word));
            }
        };
        SourceRandom::new(1, self.engine, self.cursor, &mut sink)
            .unwrap()
            .urand(bounds[0], bounds[1])
            .map_err(|_| refuse("execution.pool.respawn_rng"))
    }
    fn pool_wall(&mut self, _: PoolLeafClock) -> Result<ClockSample> {
        Err(refuse("execution.pool.wall_clock_owner"))
    }
    fn pool_world_seconds(&mut self) -> Result<i64> {
        Err(refuse("execution.pool.world_seconds_owner"))
    }
    fn pool_load(
        &mut self,
        _: PoolLoadRequest,
        _: &mut SourceManagers,
    ) -> Result<GridCandidateOutcome> {
        Err(PoolPending::LeafEffects.refusal())
    }
    fn pool_lookup(&mut self, _: MapIdentity, _: SpawnKey) -> Result<Option<ObjectHandle>> {
        Err(PoolPending::LeafEffects.refusal())
    }
    fn pool_live(&mut self, _: PoolLiveRequest, _: &mut SourceManagers) -> Result<()> {
        Err(PoolPending::LeafEffects.refusal())
    }
    fn pool_save_immediately(&mut self) -> Result<bool> {
        Err(refuse("execution.pool.respawn_config_owner"))
    }
    fn pool_creature_world_boss(&mut self, _: ObjectHandle) -> Result<bool> {
        Err(PoolPending::LeafEffects.refusal())
    }
    fn pool_gameobject_dynamic(&mut self, _: u32) -> Result<Option<bool>> {
        Err(PoolPending::LeafEffects.refusal())
    }
    fn pool_schedule(&mut self, _: PoolScheduleRequest, _: &mut SourceManagers) -> Result<()> {
        Err(PoolPending::LeafEffects.refusal())
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn continue_startup(
    recording: &Path,
    catalog: &mut VerifiedCatalog,
    spawns: &mut SourceEffectiveSpawns,
    inputs: &EffectiveSpawnInputs,
    pools: SourcePoolManager,
    event_occurrences: &Path,
    mut observed: Inputs,
    geometry: &mut SourceTerrainManager,
    engine: &mut Mt19937,
    cursor: &mut DrawCursor,
    next_word: (u64, u64),
) {
    let mut dynamic = BTreeMap::<u32, [Vec<u32>; 2]>::new();
    for ((map, kind), guids) in std::mem::take(&mut spawns.dynamic_guids) {
        dynamic.entry(map).or_default()[usize::from(kind == GridSpawnKind::GameObject)] = guids;
    }
    let receipt_bytes = std::fs::read(recording.join("actual-process-environment.json")).unwrap();
    let receipt: benilla_sim::source_sine::ProcessReceipt =
        serde_json::from_slice(&receipt_bytes).unwrap();
    assert_eq!(
        Sha256Id::of(&receipt_bytes),
        Sha256Id::parse("4b7d35a74a48e0829d942539b97f940f05b4d57416eae71a3caf4dd6269859b5")
            .unwrap()
    );
    for (file, expected) in [
        (
            "acquisition-input-manifest.json",
            &receipt.source_acquisition_bindings.manifest,
        ),
        (
            "trace-schema.json",
            &receipt.source_acquisition_bindings.schema,
        ),
        (
            "instrumentation-identity.json",
            &receipt.source_acquisition_bindings.instrumentation,
        ),
        (
            "run/trace.tsv.process-environment.bin",
            &receipt.source_process_receipt_sha256,
        ),
    ] {
        assert_eq!(
            Sha256Id::of(&std::fs::read(recording.join(file)).unwrap()),
            Sha256Id::parse(expected).unwrap()
        );
    }
    let dispatch = benilla_sim::source_sine::DispatchBinding::from_process_receipt(
        16,
        &receipt.source_acquisition_bindings,
        &receipt,
    )
    .unwrap();
    let mut calendar = Calendar(std::mem::take(&mut observed.calendar));
    let mut lunar_trace = Vec::new();
    let lunar = ReferenceLunarEventOwner::new(
        &mut calendar,
        &mut lunar_trace,
        moon::SineOrder::SolarFirst,
        dispatch,
    );
    let ids = catalog.execution_event_reference_ids().unwrap();
    let mut owner = Owner {
        spawns,
        maps: &inputs.maps,
        ids,
        lunar,
        clocks: observed.load_clocks,
        update: observed.update_clocks,
        checks: observed.check_clocks,
        clock_count: 0,
        pools,
        states: SourceManagers::default(),
        loaded_maps: BTreeMap::new(),
        effects: SourceEventEffects::new(dynamic),
        quests: BTreeMap::new(),
        status: BTreeSet::new(),
        sql: Vec::new(),
        pool_operations: Vec::new(),
        observations: Vec::new(),
        link_errors: Vec::new(),
        engine,
        cursor,
        raw: Vec::new(),
    };
    let mut events = SourceGameEventManager::default();
    let mut pools = std::mem::take(&mut owner.pools);
    events
        .load_catalog_definitions(catalog, event_occurrences, &mut pools, &mut owner)
        .unwrap();
    owner.pools = pools;
    assert!(owner.clocks.is_empty());
    assert!(events.inventory().loaded);
    let mut event_load_counts = [0usize; 4];
    for observation in &owner.observations {
        match observation {
            GameEventObservation::Definition { .. } => event_load_counts[1] += 1,
            GameEventObservation::Member { kind, .. } => {
                event_load_counts[2 + usize::from(*kind == EventSpawnKind::GameObject)] += 1;
            }
            GameEventObservation::Decision { slot: 1, .. } => event_load_counts[1] += 1,
            GameEventObservation::Decision { slot: 3, .. } => event_load_counts[2] += 1,
            GameEventObservation::Decision { slot: 4, .. } => event_load_counts[3] += 1,
            _ => {}
        }
    }
    assert_eq!(event_load_counts[1..], [67, 3239, 11204]);
    let reset: serde_json::Value =
        serde_json::from_slice(&std::fs::read(recording.join("reset.json")).unwrap()).unwrap();
    let character = &reset["databases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["database"] == "characters")
        .unwrap();
    let snapshot = retained_f1_file(
        recording.ancestors().nth(3).unwrap(),
        Path::new(character["snapshot"].as_str().unwrap()),
        &Sha256Id::parse(character["sha256"].as_str().unwrap()).unwrap(),
    )
    .unwrap();
    for line in snapshot.split(|&b| b == b'\n') {
        if let Some(mut rest) = line.strip_prefix(b"INSERT INTO `game_event_status` VALUES ") {
            loop {
                if rest == b";" {
                    break;
                }
                assert_eq!(rest.first(), Some(&b'('));
                let (value, next) = super::super::manager_inputs::sql_value(&rest[1..]).unwrap();
                owner.status.insert(
                    std::str::from_utf8(value.as_ref().unwrap())
                        .unwrap()
                        .parse()
                        .unwrap(),
                );
                assert_eq!(next.first(), Some(&b')'));
                rest = &next[1..];
                if rest.first() == Some(&b',') {
                    rest = &rest[1..];
                }
            }
        }
    }
    let db: serde_json::Value = serde_json::from_slice(
        &std::fs::read(recording.join("db-proxy-clock-input.json")).unwrap(),
    )
    .unwrap();
    let mut query_clock = |slot: usize, table: &str| {
        let sql = format!("DELETE FROM {table} WHERE respawntime <= UNIX_TIMESTAMP(NOW())");
        let digest = Sha256Id::of(sql.as_bytes());
        let clocks = db["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["statement_sha256"] == digest.as_str())
            .collect::<Vec<_>>();
        assert_eq!(clocks.len(), 1);
        CharacterQueryOccurrence {
            owner: observed.persistent_owners[slot].take().unwrap(),
            database_seconds: clocks[0]["microseconds"].as_i64().unwrap() / 1_000_000,
            world_seconds: observed.world_seconds.unwrap(),
            ordinals: Vec::new(),
        }
    };
    let initial = ManagerInitialInputs::frozen_snapshot(
        &snapshot,
        [
            query_clock(0, "creature_respawn"),
            query_clock(1, "gameobject_respawn"),
        ],
    )
    .unwrap();
    let mut persistent = InitialEffects {
        initial: &initial,
        operations: Vec::new(),
        cleaned: [false; 2],
    };
    owner
        .states
        .initialize_persistent(
            &initial,
            &mut Definitions {
                spawns,
                maps: &inputs.maps,
            },
            &mut persistent,
        )
        .unwrap();
    let config = std::fs::read(recording.join("mangosd.conf")).unwrap();
    let continent_config = ContinentConfig::parse(&config).unwrap();
    let definitions = ContinentDefinitions::load(catalog, &inputs.maps, &config).unwrap();
    let temporary_starts = definitions.temporary_starts;
    let script_ids = &definitions.script_ids;
    let mut ignore_constructor_trace = |_| {};
    let mut random = SourceRandom::new(
        1,
        &mut *owner.engine,
        &mut *owner.cursor,
        &mut ignore_constructor_trace,
    )
    .unwrap();
    let mut continents = SourceContinents::construct(
        definitions.entries,
        GuidMaximum::Value(initial.maximum_instance_id()),
        geometry,
        &continent_config,
        &mut random,
    )
    .unwrap();
    assert_eq!(random.cursor().raw, 8712); // Both TerrainInfo objects already exist.
    assert_eq!(continents.max_instance_id, 0); // MAX(id) is NULL at trace:60100919.
    for map in &continents.maps {
        assert!(owner
            .loaded_maps
            .insert((map.identity.map, 0), map.identity)
            .is_none());
    }
    assert_eq!(owner.loaded_maps.len(), observed.map_births);
    assert_eq!(observed.map_births, 2);
    assert_eq!(observed.map_bindings[&0], (6, 4294967667, 60101188));
    assert_eq!(observed.map_bindings[&1], (7, 4294967669, 60117267));
    assert_eq!(observed.map_identities[&4294967667].0, 60100929);
    assert_eq!(observed.map_identities[&4294967669].0, 60100933);
    // These independent preludes commute. No script, SQL, pool, MMap lock or shared-manager
    // mutation is executed in either job. This is not an ordering of the two initializers.
    for map in &mut continents.maps {
        map.initialize_prelude(temporary_starts, geometry, &continent_config)
            .unwrap();
        assert!(map.prelude_complete());
        assert_eq!(map.visibility.to_bits(), 100.0f32.to_bits());
        assert_eq!(geometry.terrain(map.identity.map).unwrap().references(), 1);
        assert!(map.persistent.is_none() && map.variables.is_empty() && !map.spawn_updated);
        for y in 0..64 {
            for x in 0..64 {
                assert!(!map.grid_loaded(x, y).unwrap());
                assert!(map.cells.grids.identity(x, y).is_none());
            }
        }
    }
    assert_eq!(
        observed.map_threads[&6].before_sql,
        [(60100952, 0, 2440933613), (60100962, 1, 3174165029)]
    );
    assert!(observed.map_threads[&7].before_sql.is_empty());
    assert_eq!(observed.map_threads[&6].sql_line, Some(60100980));
    assert_eq!(observed.map_threads[&7].sql_line, Some(60101153));
    for (slot, thread, first_line, first_word) in
        [(0, 6, 60100952, 2440933613), (1, 7, 60117712, 159038541)]
    {
        let evidence = &observed.map_threads[&thread];
        let stream = Mt19937::from_state(evidence.initial.clone().unwrap()).unwrap();
        let mut cursor = DrawCursor::default();
        let mut trace = Vec::new();
        let mut sink = |event| trace.push(event);
        let mut engine = stream;
        let random = SourceRandom::new(thread, &mut engine, &mut cursor, &mut sink).unwrap();
        let script_id = script_ids[&(slot as u32)];
        assert_ne!(script_id, 0);
        let refusal = continents.maps[slot]
            .create_instance_data(script_id)
            .unwrap_err();
        assert_eq!(
            refusal,
            MissingManagerOwner::F5InstanceInitialization.refusal()
        );
        assert_eq!(random.cursor().raw, 0);
        assert!(trace.is_empty());
        assert_eq!(evidence.first_word, Some((first_line, 0, first_word)));
        // Clone-only stream projection, not execution of the missing script/pool caller.
        assert_eq!(u64::from(engine.clone().next_u32()), first_word);
    }
    if continents.maps.iter().any(|map| map.script_id != 0) {
        let refusal = MissingManagerOwner::F5InstanceInitialization.refusal();
        assert!(!events.inventory().initialized);
        assert!(owner.sql.is_empty() && owner.raw.is_empty());
        assert_eq!(owner.cursor.raw, 8712);
        assert_eq!(next_word, (8712, 3411341330));
        assert_eq!(u64::from(owner.engine.clone().next_u32()), next_word.1);
        println!("F6_SEAL16_STARTUP matched_spawn_selections=5324 matched_raw_words={} maps_born=2 maps=[0,1] map_birth_lines=[60100929,60100933] map_threads=[6,7] matched_map_thread_words=[0,0] prelude_complete=[true,true] matched_map_sql=0 matched_status_writes=0 pool_shuffles=0 pool_results=0 event_initialization_reached=false next_owner={} next_source=Map.cpp:1679 caller=Map.cpp:190 next_raw={} next_word={} clone_next_word_checked=true",owner.cursor.raw,refusal.id,next_word.0,next_word.1);
        return;
    }
    let initial_status = owner.status.iter().copied().collect::<Vec<_>>();
    let announce = super::super::spawn_loader_config::assignment(&config, "Event.Announce")
        .unwrap()
        .1
        .parse::<u32>()
        .unwrap()
        != 0;
    let delay = events
        .initialize(
            (!initial_status.is_empty()).then_some(initial_status.as_slice()),
            announce,
            &mut owner,
        )
        .unwrap();
    if owner.sql != observed.expected_sql {
        let index = owner
            .sql
            .iter()
            .zip(&observed.expected_sql)
            .position(|(a, b)| a != b)
            .unwrap_or(owner.sql.len().min(observed.expected_sql.len()));
        println!("F6_FIRST_DIVERGENCE event_sql_index={index} expected={:?} got={:?} raw_words={} candidate_causes=calendar_inputs,source_query_order,event_effect_order",observed.expected_sql.get(index),owner.sql.get(index),owner.cursor.raw);
    }
    assert_eq!(owner.sql, observed.expected_sql);
    assert_eq!(&owner.status, &events.inventory().active);
    assert_eq!(&owner.quests, &events.inventory().quest_active);
    assert!(!events.active(27));
    assert_eq!(owner.loaded_maps.len(), observed.map_births);
    let created = persistent
        .operations
        .iter()
        .filter_map(|op| {
            if let PersistentOperation::Created { state } = op {
                Some((state.map, state.instance))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(created, observed.persistent_creations);
    assert!(events.active(400));
    assert!(owner.update.is_empty() && owner.checks.is_empty());
    if !owner.raw.is_empty() {
        println!("F6_FIRST_DIVERGENCE raw_index={} expected_owner=WorldState.cpp:605 got_owner=PoolManager expected_word={} got_words={:?} candidate_causes=event_windows,source_query_order,pool_event_membership",next_word.0,next_word.1,owner.raw);
    }
    assert!(owner.raw.is_empty());
    assert_eq!(owner.cursor.raw, next_word.0);
    let next = u64::from(owner.engine.clone().next_u32());
    assert_eq!(next, next_word.1);
    let refusal = refuse("execution.world_state.startup_owner");
    println!("F6_SEAL16_STARTUP matched_spawn_selections=5324 matched_raw_words={} event_definitions={} event7_clock_bound=true actual_sine_dispatch_bound=true event27_active=false pool_shuffles=0 pool_results=0 matched_status_writes={} persistent_states={} event_delay_ms={delay} next_raw={} next_word={next} next_owner={} next_source=WorldState.cpp:605",owner.cursor.raw,events.inventory().definitions.len(),owner.sql.len(),owner.states.inventories().len(),next_word.0,refusal.id);
}

#[test]
fn map_thread_evidence_keeps_raw_positions_and_pre_sql_boundaries_independent() {
    let mut inputs = Inputs::default();
    inputs.observe_thread(6, 11, "mt_raw", &["engine6", "0", "42"]);
    inputs.observe_thread(7, 12, "sql_request_bytes", &[]);
    inputs.observe_thread(7, 13, "mt_raw", &["engine7", "0", "19"]);
    inputs.observe_thread(6, 14, "mt_raw", &["engine6", "1", "99"]);
    inputs.observe_thread(6, 15, "sql_request_bytes", &[]);
    inputs.observe_thread(6, 16, "mt_raw", &["engine6", "2", "3"]);
    assert_eq!(
        inputs.map_threads[&6].before_sql,
        [(11, 0, 42), (14, 1, 99)]
    );
    assert!(inputs.map_threads[&7].before_sql.is_empty());
    assert_eq!(inputs.map_threads[&6].first_word, Some((11, 0, 42)));
    assert_eq!(inputs.map_threads[&7].first_word, Some((13, 0, 19)));
    assert_eq!(inputs.map_threads[&6].total_words, 3);
    assert_eq!(inputs.map_threads[&7].total_words, 1);
    assert_eq!(inputs.map_threads[&6].sql_line, Some(15));
    assert_eq!(inputs.map_threads[&7].sql_line, Some(12));
}
