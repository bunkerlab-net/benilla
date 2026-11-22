// SPDX-License-Identifier: GPL-3.0-or-later
use super::{unhex, WorldStartupSql, WorldStateRow};
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
