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
    words: Vec<(u64, u64, u64)>,
    sql_line: Option<u64>,
    sql_statements: Vec<Vec<u8>>,
    first_shuffle: Option<String>,
    shuffle_before: Vec<u32>,
    shuffle_after: Vec<u32>,
    total_words: usize,
    pool_raw_end: Option<usize>,
    total_shuffles: usize,
    pool_shuffles: Option<usize>,
    pool_members: [BTreeSet<u32>; 2],
    pool_counts: BTreeMap<u32, u32>,
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
                evidence.words.push(word);
                evidence.first_word.get_or_insert(word);
                if evidence.sql_line.is_none() {
                    evidence.before_sql.push(word);
                }
                evidence.total_words += 1;
            }
            "sql_request_bytes" | "sql_write_bytes" => {
                if evidence.sql_statements.len() < 2 {
                    assert_eq!(p[4], "0");
                    let bytes = p[5]
                        .as_bytes()
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|pair| {
                            u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap()
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(bytes.len(), p[3].parse::<usize>().unwrap());
                    evidence.sql_statements.push(bytes);
                }
                if kind == "sql_request_bytes" {
                    evidence.sql_line.get_or_insert(line);
                }
            }
            "rng_shuffle_begin" => {
                evidence.total_shuffles += 1;
                evidence.first_shuffle.get_or_insert_with(|| p[0].into());
            }
            "rng_shuffle_member" if evidence.first_shuffle.as_deref() == Some(p[0]) => {
                let values = if p[1] == "before" {
                    &mut evidence.shuffle_before
                } else {
                    &mut evidence.shuffle_after
                };
                assert_eq!(values.len(), p[2].parse::<usize>().unwrap());
                values.push(p[3].parse().unwrap());
            }
            "constructor_persistent_pool_state" if p[1] == "1" => {
                assert!(evidence
                    .pool_raw_end
                    .replace(evidence.total_words)
                    .is_none());
                assert!(evidence
                    .pool_shuffles
                    .replace(evidence.total_shuffles)
                    .is_none());
            }
            "constructor_persistent_pool_member" => {
                let index = match p[1] {
                    "creature" => 0,
                    "gameobject" => 1,
                    _ => panic!("unknown pool member"),
                };
                assert!(evidence.pool_members[index].insert(p[2].parse().unwrap()));
            }
            "constructor_persistent_pool_count" => {
                assert!(evidence
                    .pool_counts
                    .insert(p[1].parse().unwrap(), p[2].parse().unwrap())
                    .is_none());
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
    outbound: Vec<PersistentSql>,
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
        } else if matches!(sql, PersistentSql::CreateWorld { .. }) {
            self.outbound.push(sql);
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

struct LinkingDefinitions<'a> {
    catalog: &'a mut VerifiedCatalog,
    spawns: &'a SourceEffectiveSpawns,
    inputs: &'a EffectiveSpawnInputs,
}
fn linking_spawn(spawns: &SourceEffectiveSpawns, guid: u32) -> Result<Option<LinkingSpawnData>> {
    Ok(spawns
        .pre_terrain_data(SpawnKey {
            kind: GridSpawnKind::Creature,
            guid,
        })?
        .map(|data| LinkingSpawnData {
            entry: data.row.entry,
            map: data.row.map,
            respawn_xy: [data.row.pose[0].to_bits(), data.row.pose[1].to_bits()],
        }))
}
impl LinkingLoadOwner for LinkingDefinitions<'_> {
    fn template_exists(&mut self, entry: u32) -> Result<bool> {
        Ok(self.inputs.creatures.contains_key(&entry))
    }
    fn map_exists(&mut self, map: u32) -> Result<bool> {
        Ok(self.inputs.maps.contains_key(&map))
    }
    fn creature_data(&mut self, guid: u32) -> Result<Option<LinkingSpawnData>> {
        linking_spawn(self.spawns, guid)
    }
    fn query_unique_master(&mut self, entry: u32, map: u32) -> Result<LinkingMasterQuery> {
        query_catalog_linking_master(self.catalog, entry, map)
    }
}
struct MapLinkingGate<'a> {
    spawns: &'a SourceEffectiveSpawns,
    pools: &'a SourcePoolManager,
}
impl LinkingGateOwner for MapLinkingGate<'_> {
    fn creature_data(&mut self, guid: u32) -> Result<Option<LinkingSpawnData>> {
        linking_spawn(self.spawns, guid)
    }
    fn wall_time(&mut self, _: MapIdentity, _: u32) -> Result<ClockSample> {
        Err(refuse("execution.linking.wall_clock_owner"))
    }
    fn creature_pool(&mut self, guid: u32) -> Result<Option<u16>> {
        Ok(Some(self.pools.part_of(PoolKind::Creature, guid)?))
    }
    fn encounter_in_progress(&mut self, _: MapIdentity) -> Result<Option<bool>> {
        Err(refuse("execution.linking.instance_encounter_owner"))
    }
}
/// Independent map-job projection. Only RNG calls change stream; dependent owners stay real.
struct MapPoolWork<'a, 'b, 'r, S: FnMut(TraceEvent)> {
    world: &'a mut Owner<'b>,
    random: &'a mut SourceRandom<'r, Mt19937, S>,
    map: &'a SourceContinent,
    links: &'a SourceCreatureLinking,
    objects: &'a SourceObjects,
    shuffles: Vec<Vec<usize>>,
    persistent: Vec<PersistentOperation>,
}
impl<S: FnMut(TraceEvent)> PoolWork for MapPoolWork<'_, '_, '_, S> {
    fn engine_access(&mut self) -> Result<()> {
        self.world.engine_access()
    }
    fn chance(&mut self) -> Result<f64> {
        self.random
            .rand_chance()
            .map_err(|_| refuse("execution.pool.rand_chance"))
    }
    fn shuffle(&mut self, values: &mut [usize]) -> Result<()> {
        self.random
            .shuffle(values)
            .map_err(|_| refuse("execution.pool.shuffle"))?;
        self.shuffles.push(values.to_vec());
        Ok(())
    }
    fn can_spawn(
        &mut self,
        manager: &mut SourcePoolManager,
        states: &mut SourceManagers,
        state: PersistentStateIdentity,
        kind: PoolKind,
        guid: u32,
    ) -> Result<bool> {
        let kind = match kind {
            PoolKind::Creature => SourceObjectKind::Creature,
            PoolKind::GameObject => SourceObjectKind::GameObject,
            PoolKind::Child => return Ok(true),
        };
        self.map.can_spawn(
            kind,
            guid,
            self.links,
            self.objects,
            states.state(state)?,
            &mut MapLinkingGate {
                spawns: self.world.spawns,
                pools: manager,
            },
        )
    }
    fn leaf_effect(
        &mut self,
        manager: &mut SourcePoolManager,
        states: &mut SourceManagers,
        effect: PoolLeafEffect,
    ) -> Result<()> {
        let _ = manager;
        execute_pool_leaf(states, effect, self)
    }
    fn observe(&mut self, operation: PoolOperation) -> Result<()> {
        PoolWork::observe(self.world, operation)
    }
}
impl<S: FnMut(TraceEvent)> PoolLeafOwner for MapPoolWork<'_, '_, '_, S> {
    fn pool_spawn(
        &mut self,
        kind: GridSpawnKind,
        guid: u32,
    ) -> Result<Option<PoolSpawnDefinition>> {
        self.world.pool_spawn(kind, guid)
    }
    fn pool_grid_loaded(
        &mut self,
        map: MapIdentity,
        cell: benilla_sim::resident_map::CellPair,
    ) -> Result<bool> {
        require(map == self.map.identity, "execution.pool.map_binding")?;
        self.map.grid_loaded(cell.grid_x(), cell.grid_y())
    }
    fn pool_random_respawn(&mut self, bounds: [u32; 2]) -> Result<u32> {
        self.random
            .urand(bounds[0], bounds[1])
            .map_err(|_| refuse("execution.pool.respawn_rng"))
    }
    fn pool_wall(&mut self, site: PoolLeafClock) -> Result<ClockSample> {
        self.world.pool_wall(site)
    }
    fn pool_world_seconds(&mut self) -> Result<i64> {
        self.world.pool_world_seconds()
    }
    fn pool_load(
        &mut self,
        request: PoolLoadRequest,
        states: &mut SourceManagers,
    ) -> Result<GridCandidateOutcome> {
        self.world.pool_load(request, states)
    }
    fn pool_lookup(&mut self, map: MapIdentity, key: SpawnKey) -> Result<Option<ObjectHandle>> {
        self.world.pool_lookup(map, key)
    }
    fn pool_live(&mut self, request: PoolLiveRequest, states: &mut SourceManagers) -> Result<()> {
        self.world.pool_live(request, states)
    }
    fn pool_save_immediately(&mut self) -> Result<bool> {
        self.world.pool_save_immediately()
    }
    fn pool_creature_world_boss(&mut self, object: ObjectHandle) -> Result<bool> {
        self.world.pool_creature_world_boss(object)
    }
    fn pool_gameobject_dynamic(&mut self, entry: u32) -> Result<Option<bool>> {
        self.world.pool_gameobject_dynamic(entry)
    }
    fn pool_schedule(
        &mut self,
        request: PoolScheduleRequest,
        states: &mut SourceManagers,
    ) -> Result<()> {
        self.world.pool_schedule(request, states)
    }
}
impl<S: FnMut(TraceEvent)> PersistentEffects for MapPoolWork<'_, '_, '_, S> {
    fn sql(&mut self, operation: PersistentSql) -> Result<()> {
        PersistentEffects::sql(self.world, operation)
    }
    fn query_respawns(&mut self, kind: GridSpawnKind) -> Result<CharacterQueryStatus> {
        self.world.query_respawns(kind)
    }
    fn observe(&mut self, operation: PersistentOperation) -> Result<()> {
        self.persistent.push(operation);
        Ok(())
    }
    fn dungeon_reset(&mut self, entry: PersistentMapEntry) -> Result<i64> {
        self.world.dungeon_reset(entry)
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
    geometry
        .vmap
        .bind_rotation_dispatch(16, &receipt.source_acquisition_bindings, &receipt)
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
        outbound: Vec::new(),
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
    let script_names = &definitions.script_names;
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
    // Execute each job's local prelude independently. No cross-thread order is claimed.
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
    let mut links = SourceCreatureLinking::default();
    let occurrences = std::env::var_os("BENILLA_F2_GROUP_OCCURRENCES").unwrap();
    let mut rows = catalog
        .execution_linking_rows(Path::new(&occurrences))
        .unwrap();
    links
        .load(
            &mut rows,
            &mut LinkingDefinitions {
                catalog,
                spawns: owner.spawns,
                inputs,
            },
        )
        .unwrap();
    let objects = SourceObjects::default();
    let mut matched_map_words = [0u64; 2];
    let mut matched_map_sql = 0;
    for (slot, thread, first_line, first_word) in
        [(0, 6, 60100952, 2440933613), (1, 7, 60117712, 159038541)]
    {
        let evidence = &observed.map_threads[&thread];
        let stream = Mt19937::from_state(evidence.initial.clone().unwrap()).unwrap();
        let mut cursor = DrawCursor::default();
        let mut trace = Vec::new();
        let mut sink = |event| trace.push(event);
        let mut engine = stream;
        let mut random = SourceRandom::new(thread, &mut engine, &mut cursor, &mut sink).unwrap();
        let script_id = script_ids[&(slot as u32)];
        assert_ne!(script_id, 0);
        let mut map_effects = InitialEffects {
            initial: &initial,
            operations: Vec::new(),
            cleaned: [false; 2],
            outbound: Vec::new(),
        };
        let mut storage = SnapshotWorldStorage {
            initial: &initial,
            effects: &mut map_effects,
            pending: BTreeSet::new(),
            issued: Vec::new(),
        };
        continents.maps[slot]
            .create_instance_data(
                script_id,
                &script_names[&(slot as u32)],
                true,
                &mut random,
                &mut storage,
            )
            .unwrap();
        assert_eq!(random.cursor().raw, [2, 0][slot]);
        assert_eq!(storage.issued.len(), 2);
        for (statement, recorded) in storage.issued.iter().zip(&evidence.sql_statements) {
            assert_eq!(statement.as_bytes(), recorded);
        }
        assert_eq!(evidence.sql_statements.len(), 2);
        matched_map_sql += 2;
        assert!(storage.pending.contains(&(slot as u32)));
        assert_eq!(
            initial.world_data(slot as u32).unwrap(),
            WorldInstanceRow::Missing
        );
        assert_eq!(
            map_effects.outbound,
            [PersistentSql::CreateWorld { map: slot as u32 }]
        );
        assert!(continents.maps[slot].instance_data.is_some());
        assert!(continents.maps[slot].persistent.is_none() && !continents.maps[slot].spawn_updated);
        if slot == 0 {
            assert_eq!(
                continents.maps[slot].variables,
                BTreeMap::from([(330001, 1), (330002, 1)])
            );
        } else {
            assert!(continents.maps[slot].variables.is_empty());
            assert_eq!(random.cursor().raw, 0);
        }
        // Only the loaded definitions are forked for two independent diagnostic prefixes.
        // A refused job's stopped flag must not invent a scheduling dependency on its peer.
        let mut pools = owner.pools.clone();
        let mut states = std::mem::take(&mut owner.states);
        continents.maps[slot]
            .attach_persistent_state(&mut states, &mut persistent)
            .unwrap();
        let shuffles = {
            let mut work = MapPoolWork {
                world: &mut owner,
                random: &mut random,
                map: &continents.maps[slot],
                links: &links,
                objects: &objects,
                shuffles: Vec::new(),
                persistent: Vec::new(),
            };
            let refusal = continents.maps[slot]
                .initialize_pools(&mut states, &mut pools, &mut work)
                .unwrap_err();
            assert_eq!(refusal.id, "execution.continents.map_event_pool_owner");
            work.shuffles
        };
        matched_map_words[slot] = random.cursor().raw;
        assert_eq!(
            matched_map_words[slot] as usize,
            evidence.pool_raw_end.unwrap()
        );
        let state = continents.maps[slot].persistent.unwrap();
        assert_eq!(
            states.state(state).unwrap().map_binding(),
            Some(continents.maps[slot].identity)
        );
        assert!(states.state(state).unwrap().pools().initialized());
        assert!(!continents.maps[slot].spawn_updated);
        let inventory = pools.inventory();
        assert!(inventory.frames.is_empty() && !inventory.stopped);
        let (pool, kind) = owner
            .pool_operations
            .iter()
            .find_map(|operation| {
                if let PoolOperation::Roll {
                    state: reached,
                    pool,
                    kind,
                    ..
                } = operation
                {
                    (*reached == state).then_some((*pool, *kind))
                } else {
                    None
                }
            })
            .unwrap();
        let kind_index = match kind {
            PoolKind::Child => 0,
            PoolKind::GameObject => 1,
            PoolKind::Creature => 2,
        };
        let members = &inventory.templates[usize::from(pool)].groups[kind_index].equal;
        assert_eq!(
            members.iter().map(|member| member.guid).collect::<Vec<_>>(),
            evidence.shuffle_before
        );
        assert_eq!(shuffles.len(), evidence.pool_shuffles.unwrap());
        assert_eq!(
            shuffles[0]
                .iter()
                .map(|&index| members[index].guid)
                .collect::<Vec<_>>(),
            evidence.shuffle_after
        );
        let expected = serde_json::json!({
            "creatures": evidence.pool_members[0],
            "gameobjects": evidence.pool_members[1],
            "pools": evidence.pool_counts,
            "initialized": true,
        });
        assert_eq!(
            serde_json::to_value(states.state(state).unwrap().pools()).unwrap(),
            expected
        );
        println!("F6_MAP_POOL thread={thread} matched_words={} matched_pool_members={} matched_pool_counts={} shuffles={} next_owner=execution.continents.map_event_pool_owner next_source=Maps/MapPersistentStateMgr.cpp:205", matched_map_words[slot], evidence.pool_members.iter().map(BTreeSet::len).sum::<usize>(), evidence.pool_counts.len(), shuffles.len());
        owner.states = states;
        let raw = trace
            .iter()
            .filter_map(|event: &TraceEvent| {
                if let EventKind::Raw { position, word, .. } = event.kind {
                    Some((position, word))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            &raw[..evidence.before_sql.len()],
            evidence
                .before_sql
                .iter()
                .map(|&(_, position, word)| (position, word))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            raw,
            evidence.words[..matched_map_words[slot] as usize]
                .iter()
                .map(|&(_, position, word)| (position, word))
                .collect::<Vec<_>>()
        );
        assert!(trace.iter().all(|event| event.stream == thread));
        assert_eq!(evidence.first_word, Some((first_line, 0, first_word)));
        // Initial-state projection remains independent of the now-executed script draws.
        let mut initial_engine = Mt19937::from_state(evidence.initial.clone().unwrap()).unwrap();
        assert_eq!(u64::from(initial_engine.next_u32()), first_word);
    }
    if continents.maps.iter().any(|map| map.script_id != 0) {
        let refusal = refuse("execution.continents.map_event_pool_owner");
        assert!(!events.inventory().initialized);
        assert!(owner.sql.is_empty() && owner.raw.is_empty());
        assert_eq!(owner.cursor.raw, 8712);
        assert_eq!(next_word, (8712, 3411341330));
        assert_eq!(u64::from(owner.engine.clone().next_u32()), next_word.1);
        assert_eq!(matched_map_sql, 4);
        let results = owner
            .pool_operations
            .iter()
            .filter(|operation| matches!(operation, PoolOperation::Selected { add: true, .. }))
            .count();
        assert!(results > 0);
        println!("F6_SEAL16_STARTUP matched_spawn_selections=5324 matched_raw_words={} matched_map_thread_words={matched_map_words:?} matched_map_sql=4 pool_results={results} event_initialization_reached=false next_owner={} next_source=Maps/MapPersistentStateMgr.cpp:205 next_raw={} next_word={} clone_next_word_checked=true",owner.cursor.raw,refusal.id,next_word.0,next_word.1);
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
    inputs.observe_thread(
        7,
        12,
        "sql_request_bytes",
        &["db", "0", "0", "1", "0", "42"],
    );
    inputs.observe_thread(7, 13, "mt_raw", &["engine7", "0", "19"]);
    inputs.observe_thread(6, 14, "mt_raw", &["engine6", "1", "99"]);
    inputs.observe_thread(
        6,
        15,
        "sql_request_bytes",
        &["db", "0", "0", "1", "0", "41"],
    );
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
