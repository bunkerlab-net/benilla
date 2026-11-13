// SPDX-License-Identifier: GPL-3.0-or-later
//! Frozen products are diagnostic inputs, never repaired producer lineage.
use super::*;
use benilla_sim::source_rng::{
    DrawCursor, EventKind, Mt19937, MtState, Operation, SourceRandom, TraceEvent, Value,
};
use std::io::{BufRead, BufReader};
#[path = "seal16_startup.rs"]
mod seal16;

#[derive(Default)]
struct PendingEventStartup {
    first_clock: Option<EventClockSite>,
    clocks: std::collections::VecDeque<(EventClockSite, ClockSample)>,
    clocks_consumed: usize,
}
impl GameEventOwner for PendingEventStartup {
    fn clock(&mut self, site: EventClockSite) -> Result<ClockSample> {
        self.first_clock = Some(site);
        let (expected, sample) = self
            .clocks
            .pop_front()
            .ok_or_else(|| refuse("execution.event.load_schedule_wall_clock_owner"))?;
        require(
            expected == site,
            "execution.event.recorded_clock_owner_mismatch",
        )?;
        self.clocks_consumed += 1;
        Ok(sample)
    }
    fn reference(&mut self, _: EventReference, _: u32) -> Result<bool> {
        Err(refuse("execution.event.reference_owner"))
    }
    fn set_quest_active(&mut self, _: u32, _: bool) -> Result<()> {
        Err(refuse("execution.event.quest_active_owner"))
    }
    fn check_pool_event(
        &mut self,
        _: &SourcePoolManager,
        _: u16,
        _: i16,
        _: &std::collections::BTreeMap<u32, i16>,
        _: &std::collections::BTreeMap<u32, i16>,
    ) -> Result<()> {
        Err(refuse("execution.event.pool_event_link_owner"))
    }
    fn effect(&mut self, _: &mut SourceGameEventManager, _: GameEventEffect) -> Result<()> {
        Err(refuse("execution.event.startup_effect_owner"))
    }
    fn observe(&mut self, _: GameEventObservation) -> Result<()> {
        Ok(())
    }
}

/// Mark input occurrences only; expected observations never enter loader execution.
struct SelectionOccurrences<'a> {
    rows: super::super::production_compiler::ObjectMgrSpawnRows,
    inputs: &'a EffectiveSpawnInputs,
    candidates: std::collections::BTreeSet<(GridSpawnKind, u64)>,
}
impl EffectiveSpawnRows for SelectionOccurrences<'_> {
    fn next_spawn(
        &mut self,
        kind: GridSpawnKind,
    ) -> Result<Option<super::super::production_compiler::ObjectMgrSpawnRow>> {
        let row = self.rows.next(kind.table())?;
        if let Some(row) = &row {
            let key = SpawnKey {
                kind,
                guid: row.guid,
            };
            let conditional =
                kind == GridSpawnKind::Creature && self.inputs.conditional.contains_key(&row.guid);
            if row.entry == 0 && !conditional && self.inputs.random.contains_key(&key) {
                self.candidates.insert((kind, row.ordinal));
            }
        }
        Ok(row)
    }
}
fn unhex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn seal8_full08_terrain_and_spawn_prefix_matches_or_names_first_unported_owner() {
    frozen_startup(false);
}

#[test]
fn seal16_full01_terrain_spawn_and_event_startup_matches_or_names_first_unported_owner() {
    frozen_startup(true);
}

fn frozen_startup(later: bool) {
    let (
        Some(root),
        Some(trace),
        Some(occurrences),
        Some(config),
        Some(pool_occurrences),
        Some(event_occurrences),
    ) = (
        std::env::var_os("BENILLA_F2_CATALOG"),
        std::env::var_os("BENILLA_F3_STARTUP_TRACE"),
        std::env::var_os("BENILLA_F2_SPAWN_OCCURRENCES"),
        std::env::var_os("BENILLA_F5_EVENT_AI_CONFIG"),
        std::env::var_os("BENILLA_F2_POOL_OCCURRENCES"),
        std::env::var_os("BENILLA_F2_EVENT_OCCURRENCES"),
    )
    else {
        println!("SKIP F6 terrain prefix: bind catalog, seal8 trace, spawn/pool/event occurrences and config");
        return;
    };
    let later_trace = Path::new(&config).parent().unwrap().join("run/trace.tsv");
    let trace = if later {
        later_trace.as_path()
    } else {
        Path::new(&trace)
    };
    let mut event_inputs = seal16::Inputs::default();
    let frozen = trace.ancestors().nth(5).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(frozen.join("products-manifest.json")).unwrap())
            .unwrap();
    let mut files = SourceTerrainFiles::default();
    for entry in manifest.as_array().unwrap() {
        let name = entry["path"].as_str().unwrap();
        let Some(tile) = name
            .strip_prefix("maps/")
            .and_then(|s| s.strip_suffix(".map"))
        else {
            continue;
        };
        let map = tile[..3].parse::<u32>().unwrap();
        let x = tile[3..5].parse::<u16>().unwrap();
        let y = tile[5..7].parse::<u16>().unwrap();
        files
            .retain_present(
                map,
                x,
                y,
                &frozen.join("products").join(name),
                &Sha256Id::parse(entry["sha256"].as_str().unwrap()).unwrap(),
            )
            .unwrap();
    }
    let mut words = Vec::new();
    let mut semantic = Vec::new();
    let mut recorded_item_semantic = Vec::new();
    let mut recorded_item_raw_end = None;
    let mut initial = None;
    let mut terrain_reached = false;
    let mut terrain_calls = 0;
    let mut spawn_calls = 0;
    let mut spawn_query = None;
    let mut source_guid = 0;
    let mut source_selections = Vec::new();
    let mut spawn_word_end = 0;
    let mut next_shuffle = None;
    let mut next_shuffle_word = None;
    let mut event_definition_query = None;
    let mut source_event = 0;
    let mut clock_site = None;
    let mut event_load_clocks = std::collections::VecDeque::new();
    let mut reader = BufReader::with_capacity(1 << 20, std::fs::File::open(trace).unwrap());
    let mut next_rng = None;
    let next_rng_word;
    let mut line = String::new();
    loop {
        line.clear();
        assert!(
            reader.read_line(&mut line).unwrap() != 0,
            "missing WorldState boundary"
        );
        let mut fields = line.trim_end().splitn(4, '\t');
        if fields.next() != Some("1") {
            continue;
        }
        let _seq = fields.next().unwrap();
        let kind = fields.next().unwrap();
        let p = fields
            .next()
            .unwrap()
            .split_whitespace()
            .collect::<Vec<_>>();
        if later {
            event_inputs.observe(kind, &p);
        }
        if kind == "sql_query" {
            let query = unhex(p.last().unwrap());
            event_definition_query = query
                .starts_with(b"SELECT entry,schedule_type,occurence,length,holiday,linkedTo,description FROM game_event")
                .then(|| p[0].to_owned());
            spawn_query = if query.starts_with(b"SELECT creature.guid, creature.id, map,") {
                Some((GridSpawnKind::Creature, p[0].to_owned()))
            } else if query.starts_with(b"SELECT gameobject.guid, gameobject.id, map,") {
                Some((GridSpawnKind::GameObject, p[0].to_owned()))
            } else {
                None
            };
        }
        if kind == "sql_field"
            && p[1] == "0"
            && spawn_query
                .as_ref()
                .is_some_and(|(_, address)| *address == p[0])
        {
            source_guid = std::str::from_utf8(&unhex(p.last().unwrap()))
                .unwrap()
                .parse()
                .unwrap();
        }
        if kind == "sql_field" && p[1] == "0" && event_definition_query.as_deref() == Some(p[0]) {
            source_event = std::str::from_utf8(&unhex(p.last().unwrap()))
                .unwrap()
                .parse::<u16>()
                .unwrap();
        }
        if kind == "clock_callsite" {
            clock_site = Some((p[0].to_owned(), p[2].to_owned()));
        }
        // Frozen diagnostic INPUT: GameEventMgr::LoadFromDB's recorded time() site.
        if kind == "clock_time"
            && event_definition_query.is_some()
            && clock_site
                .as_ref()
                .is_some_and(|(kind, offset)| kind == "1" && offset == "9986148")
        {
            event_load_clocks.push_back((
                EventClockSite::LoadSchedule(source_event),
                ClockSample::authored(ClockKind::WallSeconds, p[0].parse().unwrap()),
            ));
        }
        if kind == "mt_state" && initial.is_none() {
            let mut state = [0; 624];
            for (word, value) in state.iter_mut().zip(&p[1..625]) {
                *word = value.parse().unwrap();
            }
            initial = Some(MtState {
                words: state,
                index: p[625].parse().unwrap(),
            });
        }
        if kind == "rng_call" {
            if p[1].ends_with("WorldState.cpp") {
                next_rng = Some((p[1].to_owned(), p[2].parse::<u32>().unwrap()));
            } else if p[1].ends_with("GridMap.cpp") {
                terrain_reached = true;
                terrain_calls += 1;
                assert_eq!(p[2..], ["708", "20", "40"]);
            } else if terrain_reached {
                assert!(p[1].ends_with("ObjectMgr.h"), "unported RNG call {p:?}");
                spawn_calls += 1;
                source_selections.push(SpawnKey {
                    kind: spawn_query.as_ref().unwrap().0,
                    guid: source_guid,
                });
            }
        }
        if kind == "rng_shuffle_begin" && spawn_calls == 5324 && next_shuffle.is_none() {
            next_shuffle = Some((p[2].to_owned(), p[3].parse::<u32>().unwrap()));
        }
        if kind == "mt_raw" {
            let word = (p[1].parse::<u64>().unwrap(), p[2].parse::<u64>().unwrap());
            // Expected boundary comes from the first GridMap mt_raw position, not this cohort's seed.
            if terrain_reached && recorded_item_raw_end.is_none() {
                recorded_item_raw_end = Some(word.0);
            }
            if next_rng.is_some() {
                next_rng_word = Some(word);
                break;
            }
            words.push(word);
            if next_shuffle.is_some() && next_shuffle_word.is_none() {
                next_shuffle_word = words.last().copied();
            }
        }
        if kind == "rng_shuffle_end" && next_shuffle.is_some() {
            assert!(next_shuffle_word.is_some());
        }
        if kind == "rng_result" && !terrain_reached {
            match p[0] {
                "urand" => recorded_item_semantic.push(Value::U64(p[1].parse().unwrap())),
                "irand" => recorded_item_semantic.push(Value::I64(p[1].parse().unwrap())),
                _ => (),
            }
        }
        if kind == "rng_result" && terrain_reached {
            semantic.push(match p[0] {
                "urand" => Value::U64(p[1].parse().unwrap()),
                "irand" => Value::I64(p[1].parse().unwrap()),
                _ => panic!("unported semantic helper {p:?}"),
            });
            if p[0] == "irand" {
                spawn_word_end = words.len();
            }
        }
    }
    assert_eq!((terrain_calls, spawn_calls), (2, 5324));
    if later {
        assert_eq!(spawn_word_end, 8712);
        assert!(next_shuffle.is_none());
        assert_eq!(next_rng_word, Some((8712, 3411341330)));
    } else {
        assert_eq!(words[3387..3389], [(3387, 3133881188), (3388, 2748007659)]);
        assert_eq!(spawn_word_end, 8713);
        assert_eq!(words[spawn_word_end], (8713, 2637113002));
        assert_eq!(next_shuffle_word, Some((8713, 2637113002)));
        let next_shuffle = next_shuffle.as_ref().unwrap();
        assert!(next_shuffle.0.ends_with("Pools/PoolManager.cpp"));
        assert_eq!(next_shuffle.1, 248);
    }
    let next_source = next_shuffle
        .as_ref()
        .unwrap_or_else(|| next_rng.as_ref().unwrap());
    let mut catalog =
        super::super::production_compiler::VerifiedCatalog::open(Path::new(&root), false).unwrap();
    let mut geometry =
        SourceTerrainManager::new(files, SourceTerrainDbc::catalog(&mut catalog).unwrap());
    let narrowed = geometry
        .dbc
        .areas
        .values()
        .filter(|a| a.flag > 65535)
        .count();
    let config_bytes = std::fs::read(&config).unwrap();
    let config_hash = Sha256Id::of(&config_bytes);
    geometry
        .vmap
        .initialize_recorded_world(
            geometry.dbc.linked_zones.keys().copied(),
            &config_bytes,
            &config_hash,
        )
        .unwrap();
    assert_eq!(geometry.vmap.configuration.as_ref().unwrap().1, [265, 266]);
    assert_eq!(geometry.vmap.settings(), [true, true]);
    assert!(!geometry.vmap.thread_safe());
    let autoload = AutoloadActiveConfig::recorded(Path::new(&config), &config_hash).unwrap();
    let reset = InstanceResetConfig::recorded(Path::new(&config), &config_hash).unwrap();
    struct Unbound;
    impl EffectiveSpawnOwner for Unbound {}
    let baseline = SourceEffectiveSpawns::inspect_catalog_startup_owners(
        &mut catalog,
        Path::new(&occurrences),
        Path::new(&config),
        &config_hash,
        &mut Unbound,
    )
    .unwrap();
    let terrain_keys = baseline
        .pre_terrain_refusals()
        .iter()
        .filter(|(_, e)| e.id == "execution.effective_spawns.terrain_original_zone_owner")
        .map(|(key, _)| *key)
        .collect::<Vec<_>>();
    let selection_keys = baseline
        .pre_terrain_refusals()
        .iter()
        .filter(|(_, e)| {
            e.id == "execution.effective_spawns.world_thread_startup_rng_position_owner"
        })
        .map(|(key, _)| *key)
        .collect::<Vec<_>>();
    let terrain_occurrences = baseline
        .records
        .iter()
        .filter(|r| r.reason == "execution.effective_spawns.terrain_original_zone_owner")
        .count();
    assert_eq!(
        (
            terrain_keys.len(),
            terrain_occurrences,
            selection_keys.len()
        ),
        (92977, 93321, 5324)
    );
    let instances = LoadedInstanceTemplates::catalog_diagnostic(&mut catalog, &reset).unwrap();
    let mut inputs = EffectiveSpawnInputs::catalog_diagnostic(&mut catalog).unwrap();
    instances.bind_effective_spawns(&mut inputs);
    let addons = catalog
        .execution_effective_spawn_operands(
            "gameobject_addon",
            ["guid", "animprogress", "state", "stringId"],
        )
        .unwrap();
    let mut rows = SelectionOccurrences {
        rows: super::super::production_compiler::ObjectMgrSpawnRows::open(
            &mut catalog,
            Path::new(&occurrences),
        )
        .unwrap(),
        inputs: &inputs,
        candidates: Default::default(),
    };
    let initial = initial.unwrap();
    let mut mt = Mt19937::from_state(initial.clone()).unwrap();
    let mut cursor = DrawCursor::default();
    let mut actual_words = Vec::new();
    let mut actual_semantic = Vec::new();
    let mut top = None;
    let semantic_count = std::cell::Cell::new(0);
    let mut sink = |event: TraceEvent| match event.kind {
        EventKind::Raw { position, word, .. } => actual_words.push((position, word)),
        EventKind::Begin {
            span,
            parent: None,
            operation,
            ..
        } => top = Some((span, operation)),
        EventKind::End {
            span,
            result: Ok(value),
            ..
        } if top.is_some_and(|(s, op)| {
            s == span && matches!(op, Operation::Irand | Operation::Urand)
        }) =>
        {
            actual_semantic.push(value);
            semantic_count.set(actual_semantic.len());
        }
        _ => (),
    };
    let mut random = SourceRandom::new(1, &mut mt, &mut cursor, &mut sink).unwrap();
    let items = SourceStartupItems::catalog_prefix(&mut catalog, &mut random).unwrap();
    let item_semantic_end = semantic_count.get();
    assert_eq!(random.cursor().raw, recorded_item_raw_end.unwrap());
    if !later {
        assert_eq!(random.cursor().raw, 3387);
        assert_eq!(item_semantic_end, 1);
    }
    let mut execution = items
        .bind_spawns(&mut random)
        .unwrap()
        .with_terrain(&mut geometry);
    let mut owner = ConfiguredEffectiveSpawnOwner {
        execution: &mut execution,
        autoload: &autoload,
    };
    let mut spawns = SourceEffectiveSpawns::default();
    let result = spawns.load_rows(&mut rows, &inputs, &addons, &mut owner);
    let refusal = result.as_ref().err().map(|e| e.id.as_str());
    println!("F6_CATALOG_DIAGNOSTIC before_terrain_unique=92977 before_terrain_occurrences=93321 before_selections=5324 after_complete={} pre_terrain={} selections={} creature_selections={} go_selections={} refusal={refusal:?} last={:?} area_integer_narrowing_rows={narrowed}",spawns.complete_count(),spawns.pre_terrain_map().len(),spawns.draws_needed,spawns.random_rows[0],spawns.random_rows[1],spawns.records.last());
    let completed = |keys: &[SpawnKey]| {
        keys.iter()
            .filter(|key| spawns.complete_data(**key).is_ok_and(|data| data.is_some()))
            .count()
    };
    println!("F6_TRANSITIONS terrain_unique_before={} terrain_occurrences_before={terrain_occurrences} terrain_complete_after={} selections_before={} selections_complete_after={}",terrain_keys.len(),completed(&terrain_keys),selection_keys.len(),completed(&selection_keys));
    drop(execution);
    let raw = random.cursor().raw;
    let _ = sink;
    let actual_selections = spawns
        .records
        .iter()
        .filter(|r| r.line == 0 && rows.candidates.contains(&(r.key.kind, r.ordinal)))
        .map(|r| r.key)
        .collect::<Vec<_>>();
    let identity_mismatch = actual_selections
        .iter()
        .zip(&source_selections)
        .position(|(got, expected)| got != expected);
    if let Some(index) = identity_mismatch {
        println!("F6_FIRST_DIVERGENCE selection={} expected={:?} got={:?} expected_entry={:?} got_entry={:?} raw_words={raw} candidate_causes=base_row_order,event_join_order,random_list_presence",
            index + 1, source_selections[index], actual_selections[index],
            inputs.random.get(&source_selections[index]),
            spawns.pre_terrain_map().get(&actual_selections[index]).map(|d| d.row.entry));
    }
    require(
        identity_mismatch.is_none() && actual_selections.len() == source_selections.len(),
        "execution.effective_spawns.recorded_selection_identity_mismatch",
    )
    .unwrap();
    assert_eq!(actual_semantic[..item_semantic_end], recorded_item_semantic);
    let numeric_mismatch = actual_semantic[item_semantic_end..]
        .iter()
        .zip(&semantic)
        .position(|(got, expected)| got != expected);
    if let Some(index) = numeric_mismatch {
        let selection = semantic[..=index]
            .iter()
            .filter(|v| matches!(v, Value::I64(_)))
            .count();
        println!("F6_FIRST_DIVERGENCE selection={selection} key={:?} expected={:?} got={:?} words_expected={:?} words_got={:?} candidate_causes=random_list_order,helper_bounds,terrain_rng_position",
            source_selections.get(selection.wrapping_sub(1)), semantic[index],
            actual_semantic[index + item_semantic_end], words.get(recorded_item_raw_end.unwrap() as usize + 2 + selection.wrapping_sub(1)),
            actual_words.get(recorded_item_raw_end.unwrap() as usize + 2 + selection.wrapping_sub(1)));
    }
    require(
        numeric_mismatch.is_none() && actual_semantic.len() - item_semantic_end == semantic.len(),
        "execution.effective_spawns.recorded_selection_result_mismatch",
    )
    .unwrap();
    assert_eq!(actual_words, words[..spawn_word_end]);
    assert_eq!(actual_selections.len(), 5324);
    assert_eq!(raw, spawn_word_end as u64);
    println!("F6_FROZEN_PREFIX recording={} matched_selections={} total_selections=5324 matched_raw_words={raw} matched_terrain_calls=2 first_divergence=none next_startup_rng={}:{} next_startup_raw={spawn_word_end}",if later {"seal16"}else{"seal8"},actual_selections.len(),next_source.0,next_source.1);
    result.unwrap();
    // World.cpp:1095 loads pools without drawing; :1109 loads events before InitWorldMaps.
    let mut pools = SourcePoolManager::default();
    pools
        .load_catalog(
            &mut catalog,
            Path::new(&pool_occurrences),
            &mut EffectivePoolDefinitions {
                spawns: &spawns,
                maps: &inputs.maps,
            },
        )
        .unwrap();
    let pool = pools.pool_template(1163).unwrap();
    assert_eq!(pool.map, Some(0));
    assert_eq!(pool.description, b"Leprithus (572)");
    let group = &pool.groups[2];
    let members = group.equal.iter().map(|row| row.guid).collect::<Vec<_>>();
    assert_eq!(members, [28458, 134022, 160354]);
    assert_eq!(cursor.raw, raw);
    let inventory = pools.inventory();
    assert_eq!(
        inventory.records.iter().filter(|row| row.slot == 1).count(),
        4096
    );
    assert_eq!(
        inventory
            .records
            .iter()
            .filter(|row| (2..=6).contains(&row.slot))
            .count(),
        24768
    );
    println!("F6_POOL_LOAD templates={} membership_records={} first_recorded_shuffle_pool=1163 map=0 independent_members={members:?} additional_raw_words=0",
        inventory.records.iter().filter(|row| row.slot == 1).count(),
        inventory.records.iter().filter(|row| (2..=6).contains(&row.slot)).count());
    if later {
        seal16::continue_startup(
            trace.parent().unwrap().parent().unwrap(),
            &mut catalog,
            &mut spawns,
            &inputs,
            pools,
            Path::new(&event_occurrences),
            event_inputs,
            &mut mt,
            &mut cursor,
            next_rng_word.unwrap(),
        );
        return;
    }
    let mut events = SourceGameEventManager::default();
    let mut pending = PendingEventStartup {
        clocks: event_load_clocks,
        ..Default::default()
    };
    let refusal = events
        .load_catalog_definitions(
            &mut catalog,
            Path::new(&event_occurrences),
            &mut pools,
            &mut pending,
        )
        .unwrap_err();
    assert_eq!(pending.first_clock, Some(EventClockSite::LoadSchedule(7)));
    assert_eq!(pending.clocks_consumed, 1);
    assert_eq!(
        refusal.id,
        "execution.event.lunar_reference_dispatch_unbound"
    );
    assert_eq!(cursor.raw, raw);
    println!("F6_NEXT_STARTUP refusal={} event=7 schedule=12 raw={raw} additional_raw_words=0 pool_results=0 recorded_later_shuffle_pool=1163 map=0 recorded_word={next_shuffle_word:?}", refusal.id);
    assert_eq!(spawns.draws_needed, 5324);
    assert_eq!(spawns.random_rows, [3468, 1856]);
    assert_eq!(actual_semantic.len() - item_semantic_end, 5326);
    assert!(spawns.pre_terrain_refusals().is_empty());
    assert_eq!(geometry.len(), 2);
    assert_eq!(geometry.vmap.loaded_trees(), 0);
    assert_eq!(
        geometry.terrain(0).unwrap().residency.cleanup_current(),
        35000
    );
    assert_eq!(
        geometry.terrain(1).unwrap().residency.cleanup_current(),
        33000
    );
}
