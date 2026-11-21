// SPDX-License-Identifier: GPL-3.0-or-later
use super::{unhex, WorldStartupSql, WorldStateRow};
use std::io::{BufRead, BufReader};
use std::path::Path;

const SQL: &[u8] = b"SELECT Id, Data FROM world_state";

#[derive(Default)]
pub(super) struct WorldStateEvidence {
    lines: Vec<u64>,
    rows: Option<usize>,
    observed: bool,
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
            _ => return,
        };
        assert_eq!(kind, expected_kind, "trace line {line}");
        assert_eq!(fields, expected, "trace line {line}");
        self.lines.push(line);
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
        let row = reader.lines().nth(379).unwrap().unwrap();
        let row: Vec<_> = row.split('\t').collect();
        assert_eq!(row.len(), 4);
        assert_eq!(row[0], "1880");
        assert_eq!(row[1], "1794314193.920001");
        assert_eq!(unhex(row[2]), SQL);
        assert_eq!(row[3], "0");
        &[]
    }

    pub(super) fn observe_load(&mut self, sql: WorldStartupSql) {
        let WorldStartupSql::SelectWorldState { rows } = sql else {
            panic!("WorldState SQL trace diverged");
        };
        assert_eq!(sql.to_string().as_bytes(), SQL);
        assert_eq!(Some(rows), self.rows);
        assert!(!std::mem::replace(&mut self.observed, true));
    }

    pub(super) fn assert_loaded(&self) {
        assert!(self.observed);
    }
}
