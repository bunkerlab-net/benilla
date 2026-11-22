// SPDX-License-Identifier: GPL-3.0-or-later
use super::{
    unhex, ObjectHandle, SourceContinent, SourceCreatureSwimmingCensus, SourcePose,
    WorldStartupSql, WorldStateRow,
};
use benilla_sim::source_rng::{EventKind, Operation, TraceEvent, Value};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::Path;

const SQL: &[u8] = b"SELECT Id, Data FROM world_state";
// Trace :61738460 and :61738482.
const DRAGON_DELETE: &[u8] = b"DELETE FROM world_state WHERE Id='0'";
const DRAGON_INSERT: &[u8] =
    b"INSERT INTO world_state(Id,Data) VALUES('0','0 0 14890 14888 14887 14889')";

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
    next_ai_word: Option<(u64, u64)>,
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
            61741101 => {
                assert_eq!(kind, "rng_call");
                assert_eq!(
                    p,
                    [
                        "urand",
                        "/reference/source/src/game/AI/ScriptDevAI/base/TimerAI.cpp",
                        "23",
                        "0",
                        "0"
                    ]
                );
            }
            61741102 => {
                assert_eq!((kind, p[0]), ("mt_raw", "0x200205904f00"));
                let word = (p[1].parse().unwrap(), p[2].parse().unwrap());
                assert!(self.next_ai_word.replace(word).is_none());
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
        assert_eq!(next, (8716, 75745173)); // Trace :61741102.
        next
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
        assert!(live.temporary_spawn.is_some() && live.spawner.is_none());
        assert!(!object.in_world && !object.database_spawn_data);
        // Trace :61740831-41 queries the relocated point. Object.cpp:2062 has not run.
        assert_eq!(
            object.pose,
            Some(SourcePose([0xc622f333, 0xc3c41687, 0x42300be4, 0x3f6eb021]))
        );
        let pose = object.respawn_pose.unwrap();
        assert_eq!(
            pose,
            SourcePose([0xc622f333, 0xc3c41687, 0x422ef6e3, 0x3f6eb021])
        );
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
        assert_eq!(raw, [(position, word)]);
        assert_eq!(
            trace.last().unwrap().kind,
            EventKind::End {
                span,
                begin: position,
                end: position + 1,
                result: Ok(Value::I64(result))
            }
        );
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
