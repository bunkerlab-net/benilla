// SPDX-License-Identifier: GPL-3.0-or-later
use super::active_init::{ActiveClock, ActiveClocks};
use super::{
    unhex, ObjectHandle, SourceContinent, SourceCreatureSwimmingCensus, SourcePose,
    WorldStartupSql, WorldStateRow,
};
use super::{LiveLookupKind, SourceTempSpawnType, SourceUnitEvent};
use benilla_sim::source_rng::{EventKind, Operation, TraceEvent, Value};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::Path;

const SQL: &[u8] = b"SELECT Id, Data FROM world_state";
// Trace :61738460 and :61738482.
const DRAGON_DELETE: &[u8] = b"DELETE FROM world_state WHERE Id='0'";
const DRAGON_INSERT: &[u8] =
    b"INSERT INTO world_state(Id,Data) VALUES('0','0 0 14890 14888 14887 14889')";

// Trace :61741101-53, call, raw word and result on consecutive lines.
const TAERAR_DRAWS: [(u64, u64, [u64; 2], u64, u64); 6] = [
    (61741101, 23, [0, 0], 75745173, 0),
    (61741111, 23, [0, 0], 1977936151, 0),
    (61741121, 70, [9000, 12000], 2651444282, 10852),
    (61741131, 71, [10000, 20000], 2739243785, 16378),
    (61741141, 70, [9000, 12000], 2056961313, 10437),
    (61741151, 71, [10000, 20000], 3270547554, 17615),
];

#[derive(Default)]
pub(super) struct WorldStateEvidence {
    lines: Vec<u64>,
    rows: Option<usize>,
    observed: bool,
    dragon_bounds: Vec<u32>,
    dragon_results: Vec<u32>,
    dragon_words: Vec<(u64, u64)>,
    next_word: Option<(u64, u64)>,
    first_summon_identity: Option<[u32; 3]>,
    first_display_bounds: Option<[u64; 2]>,
    first_display_result: Option<i64>,
    init_entry_height: bool,
    taerar_rows: u8,
    next_ai_word: Option<(u64, u64)>,
    addon_clock_site: Option<(u64, [u64; 3], bool)>,
    addon_clocks: Option<ActiveClocks>,
    save: VecDeque<Vec<u8>>,
    affected: Vec<u64>,
}

impl WorldStateEvidence {
    pub(super) fn observe(&mut self, line: u64, kind: &str, p: &[&str]) {
        let (expected_kind, fields, expected): (&str, &[&str], &[&str]) = match line {
            61738404 => ("sql_query_request", p, &["4294967301", "203678"]),
            61738405 => {
                assert_eq!(unhex(p[5]), SQL);
                ("sql_request_bytes", &p[1..5], &["0", "0", "32", "0"])
            }
            61738407 => ("clock_sample", &p[..3], &["1", "443366", "499203009"]),
            61738408 => ("clock_ms", p, &["137048"]),
            61738409 => ("sql_clock_request", p, &["4294967301", "46", "1880", "1"]),
            61738410 => {
                assert_eq!(unhex(p[5]), SQL);
                (
                    "sql_query",
                    &p[..5],
                    &["0x55cbd2105f90", "0", "0", "32", "0"],
                )
            }
            61738412 => {
                assert!(self.rows.replace(p[3].parse().unwrap()).is_none());
                (
                    "sql_query_result",
                    p,
                    &["4294967301", "203678", "0x55cbd2105f90", "0", "2", "0"],
                )
            }
            _ => return self.observe_dragon_trace(line, kind, p),
        };
        assert_eq!(kind, expected_kind, "trace line {line}");
        assert_eq!(fields, expected, "trace line {line}");
        self.lines.push(line);
    }

    fn observe_dragon_trace(&mut self, line: u64, kind: &str, p: &[&str]) {
        match line {
            61738425 | 61738435 | 61738445 => {
                assert_eq!(kind, "rng_call");
                assert_eq!(
                    p[..4],
                    [
                        "urand",
                        "/reference/source/src/game/World/WorldState.cpp",
                        "605",
                        "0"
                    ]
                );
                self.dragon_bounds.push(p[4].parse().unwrap());
            }
            61738426 | 61738436 | 61738446 | 61738521 => {
                assert_eq!((kind, p[0]), ("mt_raw", "0x200205904f00"));
                let word = (p[1].parse().unwrap(), p[2].parse().unwrap());
                if line == 61738521 {
                    assert!(self.next_word.replace(word).is_none());
                } else {
                    self.dragon_words.push(word);
                }
            }
            61738427 | 61738437 | 61738447 => {
                assert_eq!((kind, p[0]), ("rng_result", "urand"));
                self.dragon_results.push(p[1].parse().unwrap());
            }
            61738460 | 61738482 => {
                assert_eq!(kind, "sql_write_bytes");
                let sql = unhex(p[5]);
                assert_eq!(
                    sql.as_slice(),
                    if line == 61738460 {
                        DRAGON_DELETE
                    } else {
                        DRAGON_INSERT
                    }
                );
                self.save.push_back(sql);
            }
            61738465 | 61738501 => {
                assert_eq!((kind, p[0], p[3]), ("sql_write_result", "4294967302", "0"));
                assert_eq!(p[1], if line == 61738465 { "203682" } else { "203683" });
                self.affected.push(p[2].parse().unwrap());
            }
            61738512 => {
                assert_eq!(
                    (kind, p[0]),
                    ("constructor_creature_original_entry", "4294967687")
                );
                self.first_summon_identity =
                    Some([p[1], p[2], p[3]].map(|item| item.parse().unwrap()));
            }
            61738519 => {
                assert_eq!(kind, "rng_call");
                assert_eq!(
                    p,
                    [
                        "irand",
                        "/reference/source/src/game/Entities/Creature.cpp",
                        "678",
                        "0",
                        "99"
                    ]
                );
                self.first_display_bounds = Some([p[3], p[4]].map(|item| item.parse().unwrap()));
            }
            61738523 => {
                assert_eq!((kind, p[0]), ("rng_result", "irand"));
                assert!(self
                    .first_display_result
                    .replace(p[1].parse().unwrap())
                    .is_none());
            }
            61738535 => {
                assert_eq!(kind, "terrain_height");
                assert_eq!(
                    p,
                    [
                        "0",
                        "-0x1.45e666p+13",
                        "-0x1.882d0ep+8",
                        "0x1.5dedc6p+5",
                        "1",
                        "0x1.9p+5"
                    ]
                );
                self.init_entry_height = true;
            }
            61740996 | 61740998 | 61741079 | 61741081 => {
                assert_eq!(kind, "clock_callsite");
                let site = [p[0], p[1], p[2]].map(|value| value.parse().unwrap());
                assert!(self
                    .addon_clock_site
                    .replace((line, site, p[3].ends_with("/bin/mangosd")))
                    .is_none());
            }
            61740997 | 61740999 | 61741080 | 61741082 => {
                assert_eq!(kind, "clock_time");
                let (site_line, site, module) = self.addon_clock_site.take().unwrap();
                self.addon_clocks
                    .get_or_insert_with(ActiveClocks::default)
                    .0
                    .push_back(ActiveClock {
                        site_line,
                        time_line: line,
                        thread: 1,
                        site,
                        module,
                        seconds: p[0].parse().unwrap(),
                        raw: self.next_word.unwrap().0 + 1,
                    });
            }
            61741261 => {
                assert_eq!((kind, p[0]), ("mt_raw", "0x200205904f00"));
                let word = (p[1].parse().unwrap(), p[2].parse().unwrap());
                assert!(self.next_ai_word.replace(word).is_none());
            }
            61741101..=61741153 => {
                for (i, (start, site, bounds, word, result)) in TAERAR_DRAWS.into_iter().enumerate()
                {
                    match line.checked_sub(start) {
                        Some(0) => {
                            assert_eq!((kind, p[0]), ("rng_call", "urand"));
                            let path = if site == 23 {
                                "/reference/source/src/game/AI/ScriptDevAI/base/TimerAI.cpp"
                            } else {
                                "/reference/source/src/game/AI/ScriptDevAI/scripts/world/bosses_emerald_dragons.cpp"
                            };
                            assert_eq!(p[1], path);
                            assert_eq!(
                                [p[2], p[3], p[4]].map(|n| n.parse::<u64>().unwrap()),
                                [site, bounds[0], bounds[1]]
                            );
                        }
                        Some(1) => {
                            assert_eq!((kind, p[0]), ("mt_raw", "0x200205904f00"));
                            assert_eq!(
                                [p[1], p[2]].map(|n| n.parse::<u64>().unwrap()),
                                [8716 + i as u64, word]
                            );
                        }
                        Some(2) => {
                            assert_eq!((kind, p[0]), ("rng_result", "urand"));
                            assert_eq!(p[1].parse::<u64>().unwrap(), result);
                        }
                        _ => continue,
                    }
                    self.taerar_rows += 1;
                    break;
                }
            }
            _ => (),
        }
    }

    pub(super) fn assert_dragon_draws(&self, rolls: &[(u32, u32)], words: &[(u64, u64)]) {
        assert_eq!(self.dragon_bounds, [3, 2, 1]); // Trace :61738425/35/45.
        assert_eq!(self.dragon_results, [3, 1, 0]); // Trace :61738427/37/47.
        assert_eq!(rolls.len(), self.dragon_bounds.len());
        assert!(rolls.iter().copied().eq(self
            .dragon_bounds
            .iter()
            .copied()
            .zip(self.dragon_results.iter().copied())));
        assert_eq!(
            self.dragon_words,
            [(8712, 3411341330), (8713, 2323586215), (8714, 1284187731)]
        ); // Trace :61738426/36/46.
        assert_eq!(words, self.dragon_words);
    }

    pub(super) fn next_dragon_word(&self) -> (u64, u64) {
        let next = self.next_word.unwrap();
        assert_eq!(next, (8715, 174174399)); // Trace :61738521.
        next
    }

    pub(super) fn next_dragon_ai_word(&self) -> (u64, u64) {
        let next = self.next_ai_word.unwrap();
        assert_eq!(next, (8722, 728388447)); // Trace :61741261.
        next
    }

    pub(super) fn bind_addon_clocks(&mut self) -> ActiveClocks {
        let clocks = self.addon_clocks.take().unwrap();
        assert_eq!(clocks.0.len(), 4); // Trace :61740996-99, :61741079-82.
        assert!(self.addon_clock_site.is_none());
        clocks
    }

    pub(super) fn assert_first_summon(
        &self,
        map: &SourceContinent,
        handle: ObjectHandle,
        trace: &[TraceEvent],
        swimming: &SourceCreatureSwimmingCensus,
    ) {
        let object = map.objects.get(handle).unwrap();
        let live = map.units.get(&map.objects, handle).unwrap();
        let guid = object.guid.unwrap();
        let identity = [object.database_guid, guid.counter(), guid.entry()];
        assert_eq!(identity, [5341346, 5341346, 14890]); // Trace :61738512-13.
        assert_eq!(Some(identity), self.first_summon_identity);
        assert_eq!(object.map, Some(map.identity));
        assert_eq!(live.entry, guid.entry());
        assert_eq!(live.loaded_creature.native_display, 15363); // initial-classicmangos.sql:430292.
        let temporary = live.temporary_spawn.as_ref().unwrap();
        assert!(live.spawner.is_none());
        // TemporarySpawn.cpp:217-225, WorldState.cpp:617 and cached World.cpp:108.
        assert_eq!(temporary.spawn_type, SourceTempSpawnType::DeadDespawn);
        assert_eq!((temporary.lifetime_ms, temporary.expiration_ms), (0, 0));
        assert_eq!(live.respawn_delay, 2592000); // Common.h:113-117.
        assert!(object.in_world && !object.database_spawn_data && !object.active);
        let cell = map.cells.registered_cell(handle).unwrap();
        assert_eq!((cell.grid.x, cell.grid.y), (12, 31)); // Trace :61741083-84.
        assert!(!map.cells.object_data_loaded(cell.grid.x, cell.grid.y));
        assert_eq!(
            map.objects
                .lookup(map.identity, LiveLookupKind::Creature, guid),
            Some(handle)
        ); // Trace :61741085.
        assert_eq!(map.temporary_creatures.get(&14890), Some(&1));
        assert_eq!(map.motions[&handle].path_id().unwrap(), 1); // WorldState.cpp:588-590.
        assert!(!live.has_ai); // The factory has yet to return at trace :61741236.
        assert!(map.ais.get(&map.objects, handle).is_err());
        let pose = object.pose.unwrap();
        // Trace :61740831-41 and Object.cpp:2062.
        assert_eq!(object.respawn_pose, Some(pose));
        assert_eq!(
            pose,
            SourcePose([0xc622f333, 0xc3c41687, 0x42300be4, 0x3f6eb021])
        );
        for spell in [19818, 25041] {
            let aura = live.auras.first_aura_holder(spell).unwrap();
            assert!(aura.ready);
            assert_eq!(aura.apply_time, 1794314193); // Trace :61740997, :61741080.
            let effect = aura.effects[0].as_ref().unwrap();
            assert!(effect.applied);
            assert_eq!(effect.apply_time, 1794314193); // Trace :61740999, :61741082.
        }
        let events = live.unit_events.inventory();
        assert_eq!(events.queued.len(), 4); // Trace :61740919, :61741002, :61741088, :61741159.
        let SourceUnitEvent::Spell { spell } = events.queued[2].kind else {
            panic!("Taerar's constructor spell event is missing");
        };
        assert_eq!(events.queued[2].exec_time_ms, 1); // Trace :61741158-59.
        let cast = &live.spell_starts.spells[&spell];
        assert_eq!(cast.spell, 24886); // bosses_emerald_dragons.cpp:366,394.
        assert_eq!(cast.targets.unit, Some(handle));
        // Spell.cpp:2407-2412 and the live pose from trace :61740831-41.
        let SourcePose([x, y, z, _]) = pose;
        assert_eq!(cast.targets.source, Some([x, y, z]));
        assert_ne!(cast.targets.mask & super::TARGET_FLAG_SOURCE_LOCATION, 0);
        assert!(cast.flags.old_triggered() && cast.executing);
        assert!(matches!(events.queued[3].kind, SourceUnitEvent::AiNotify));
        assert_eq!(events.queued[3].exec_time_ms, 5000); // Trace :61741088.
        assert!(self.init_entry_height); // Trace :61738535 queries this pre-placement pose.
        assert_eq!(swimming.reached_441, 1);
        assert_eq!(swimming.swimming_set + swimming.swimming_not_set, 1);

        let (position, word) = self.next_dragon_word();
        let bounds = self.first_display_bounds.unwrap();
        let result = self.first_display_result.unwrap();
        assert_eq!((bounds, result), ([0, 99], 4)); // Trace :61738519-23.
        assert!(trace.iter().all(|event| event.stream == 1));
        let EventKind::Begin {
            span,
            parent: None,
            at,
            operation: Operation::Irand,
            args,
        } = trace[0].kind
        else {
            panic!("first dragon display helper diverged");
        };
        assert_eq!((at, args), (position, [bounds[0], bounds[1], 0]));
        let raw: Vec<_> = trace
            .iter()
            .filter_map(|event| match event.kind {
                EventKind::Raw { position, word, .. } => Some((position, word)),
                _ => None,
            })
            .collect();
        assert_eq!(raw.len(), TAERAR_DRAWS.len() + 1);
        assert_eq!(raw[0], (position, word));
        assert_eq!(
            trace
                .iter()
                .find_map(|event| match event.kind {
                    EventKind::End { span: id, .. } if id == span => Some(event.kind),
                    _ => None,
                })
                .unwrap(),
            EventKind::End {
                span,
                begin: position,
                end: position + 1,
                result: Ok(Value::I64(result))
            }
        );
        assert_eq!(usize::from(self.taerar_rows), TAERAR_DRAWS.len() * 3);
        let mut calls = trace.iter().filter_map(|event| match event.kind {
            EventKind::Begin {
                span,
                parent: None,
                at,
                operation: Operation::Urand,
                args,
            } => Some((span, at, args)),
            _ => None,
        });
        for ((_, _, bounds, word, result), (at, raw_word)) in
            TAERAR_DRAWS.into_iter().zip(raw.into_iter().skip(1))
        {
            let (span, call_at, args) = calls.next().unwrap();
            assert_eq!(
                (call_at, args, raw_word),
                (at, [bounds[0], bounds[1], 0], word)
            );
            assert!(trace.iter().any(|event| event.kind
                == EventKind::End {
                    span,
                    begin: at,
                    end: at + 1,
                    result: Ok(Value::U64(result)),
                }));
        }
        assert!(calls.next().is_none());
    }

    pub(super) fn bind_rows(&self, recording: &Path) -> &'static [WorldStateRow<'static>] {
        assert_eq!(self.rows, Some(0));
        assert_eq!(
            self.lines,
            [61738404, 61738405, 61738407, 61738408, 61738409, 61738410, 61738412]
        );
        let reader = BufReader::new(
            std::fs::File::open(recording.join("private-db-statement-clocks.tsv")).unwrap(),
        );
        // private-db-statement-clocks.tsv:380-382 independently binds bytes and affected rows.
        let expected = [
            ("1880", "1794314193.920001", SQL, "0"),
            ("1881", "1794314193.920935", DRAGON_DELETE, "0"),
            ("1881", "1794314193.921488", DRAGON_INSERT, "1"),
        ];
        let mut rows = reader.lines().skip(379);
        for (statement, clock, sql, affected) in expected {
            let row = rows.next().unwrap().unwrap();
            let row: Vec<_> = row.split('\t').collect();
            assert_eq!(row.len(), 4);
            assert_eq!((row[0], row[1], row[3]), (statement, clock, affected));
            assert_eq!(unhex(row[2]), sql);
        }
        &[]
    }

    pub(super) fn observe_sql(&mut self, sql: WorldStartupSql) {
        match sql {
            WorldStartupSql::SelectWorldState { rows } => {
                assert_eq!(sql.to_string().as_bytes(), SQL);
                assert_eq!(Some(rows), self.rows);
                assert!(!std::mem::replace(&mut self.observed, true));
            }
            WorldStartupSql::DeleteEmeraldDragons
            | WorldStartupSql::InsertEmeraldDragons { .. } => {
                assert!(self.observed);
                assert_eq!(
                    sql.to_string().as_bytes(),
                    self.save.pop_front().unwrap().as_slice()
                );
            }
            _ => panic!("WorldState SQL trace diverged"),
        }
    }

    pub(super) fn assert_loaded(&self) {
        assert!(self.observed);
        assert!(self.save.is_empty());
        assert_eq!(self.affected, [0, 1]); // Trace :61738465 and :61738501.
    }
}
