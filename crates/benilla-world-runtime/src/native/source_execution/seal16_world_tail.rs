// SPDX-License-Identifier: GPL-3.0-or-later
//! Reference evidence for startup after both continents return.
use super::unhex;
use std::io::{BufRead, BufReader};
use std::path::Path;

const BAN_SQL: &[u8] =
    b"DELETE FROM ip_banned WHERE expires_at<=UNIX_TIMESTAMP() AND expires_at<>banned_at";

#[derive(Default)]
pub(super) struct BansEvidence {
    lines: Vec<u64>,
}

impl BansEvidence {
    pub(super) fn observe(&mut self, line: u64, kind: &str, p: &[&str]) {
        let (expected_kind, fields, expected): (&str, &[&str], &[&str]) = match line {
            61722084 | 61722109 => ("clock_time", &p[..1], &["1794314193"]),
            61722085 | 61722110 => (
                "clock_localtime",
                p,
                &[
                    "1794314193",
                    "33",
                    "36",
                    "12",
                    "10",
                    "10",
                    "126",
                    "2",
                    "313",
                    "0",
                    "0",
                ],
            ),
            61722088 => ("sql_connection_lock", p, &["4294967305", "acquire"]),
            61722094 => ("sql_write_request", p, &["4294967305", "203327"]),
            61722095 => {
                assert_eq!(unhex(p[5]), BAN_SQL);
                ("sql_write_bytes", &p[1..5], &["0", "0", "82", "0"])
            }
            61722097 => ("clock_sample", &p[..3], &["1", "443365", "923750612"]),
            61722098 => ("clock_ms", p, &["136473"]),
            61722099 => ("sql_clock_request", p, &["4294967305", "8", "1883", "1"]),
            61722100 => ("sql_write_result", p, &["4294967305", "203327", "0", "0"]),
            61722106 => ("sql_connection_lock", p, &["4294967305", "release"]),
            _ => return,
        };
        assert_eq!(kind, expected_kind, "trace line {line}");
        assert_eq!(fields, expected, "trace line {line}");
        self.lines.push(line);
    }

    pub(super) fn assert_matches(&self, recording: &Path) {
        assert_eq!(
            self.lines,
            [
                61722084, 61722085, 61722088, 61722094, 61722095, 61722097, 61722098, 61722099,
                61722100, 61722106, 61722109, 61722110,
            ]
        );
        // The SQL server owns UNIX_TIMESTAMP(). The wrapper's steady sample is a separate clock.
        let reader = BufReader::new(
            std::fs::File::open(recording.join("private-db-statement-clocks.tsv")).unwrap(),
        );
        let row = reader.lines().nth(357).unwrap().unwrap();
        let row: Vec<_> = row.split('\t').collect();
        assert_eq!(row.len(), 4);
        assert_eq!(row[0], "1883");
        assert_eq!(row[1], "1794314193.344626");
        assert_eq!(unhex(row[2]), BAN_SQL);
        assert_eq!(row[3], "0");
    }
}

const OLD_CHARACTERS_SQL: &[u8] =
    b"SELECT guid, deleteInfos_Account FROM characters WHERE deleteDate IS NOT NULL AND deleteDate < '1791722193'";

#[derive(Default)]
pub(super) struct OldCharactersEvidence {
    lines: Vec<u64>,
}

impl OldCharactersEvidence {
    pub(super) fn observe(&mut self, line: u64, kind: &str, p: &[&str]) {
        let (expected_kind, fields, expected): (&str, &[&str], &[&str]) = match line {
            12334 => ("config", p, &["CharDelete.KeepDays", "file", "30"]),
            61727838 => ("clock_time", &p[..1], &["1794314193"]),
            61727851 => ("sql_query_request", p, &["4294967301", "203353"]),
            61727852 | 61727857 => {
                assert_eq!(unhex(p[5]), OLD_CHARACTERS_SQL);
                let query_kind = if line == 61727852 {
                    "sql_request_bytes"
                } else {
                    "sql_query"
                };
                (query_kind, &p[1..5], &["0", "0", "107", "0"])
            }
            61727854 => ("clock_sample", &p[..3], &["1", "443366", "375721835"]),
            61727855 => ("clock_ms", p, &["136925"]),
            61727856 => ("sql_clock_request", p, &["4294967301", "45", "1880", "1"]),
            61727859 => (
                "sql_query_result",
                p,
                &["4294967301", "203353", "0x55cbd2105430", "0", "2", "0"],
            ),
            _ => return,
        };
        assert_eq!(kind, expected_kind, "trace line {line}");
        assert_eq!(fields, expected, "trace line {line}");
        self.lines.push(line);
    }

    pub(super) fn assert_matches(&self, recording: &Path) {
        assert_eq!(
            self.lines,
            [
                12334, 61727838, 61727851, 61727852, 61727854, 61727855, 61727856, 61727857,
                61727859,
            ]
        );
        let reader = BufReader::new(
            std::fs::File::open(recording.join("private-db-statement-clocks.tsv")).unwrap(),
        );
        let row = reader.lines().nth(374).unwrap().unwrap();
        let row: Vec<_> = row.split('\t').collect();
        assert_eq!(row.len(), 4);
        assert_eq!(row[0], "1880");
        assert_eq!(row[1], "1794314193.796495");
        assert_eq!(unhex(row[2]), OLD_CHARACTERS_SQL);
        assert_eq!(row[3], "0");
    }
}

struct AnticheatQuery {
    request_line: u64,
    clock_line: u64,
    result_line: u64,
    sql: &'static [u8],
    connection: &'static str,
    request: &'static str,
    result: &'static str,
    rows: &'static str,
    columns: &'static str,
    steady_ns: &'static str,
    steady_ms: &'static str,
    clock_request: &'static str,
    db_thread: &'static str,
    db_clock: &'static str,
}

const ANTICHEAT_QUERIES: [AnticheatQuery; 4] = [
    AnticheatQuery {
        request_line: 61732722,
        clock_line: 61732729,
        result_line: 61732749,
        sql: b"SELECT `string` FROM antispam_blacklist",
        connection: "4294967304",
        request: "203674",
        result: "0x55cbd2100cb0",
        rows: "211",
        columns: "1",
        steady_ns: "392099218",
        steady_ms: "136941",
        clock_request: "5",
        db_thread: "1882",
        db_clock: "1794314193.812922",
    },
    AnticheatQuery {
        request_line: 61734059,
        clock_line: 61734062,
        result_line: 61734179,
        sql: b"SELECT `from`, `to` FROM antispam_replacement",
        connection: "4294967304",
        request: "203675",
        result: "0x55cbd2105f90",
        rows: "13",
        columns: "2",
        steady_ns: "400405862",
        steady_ms: "136949",
        clock_request: "6",
        db_thread: "1882",
        db_clock: "1794314193.821174",
    },
    AnticheatQuery {
        request_line: 61734364,
        clock_line: 61734367,
        result_line: 61734484,
        sql: b"SELECT `from`, `to` FROM antispam_unicode_replacement",
        connection: "4294967304",
        request: "203676",
        result: "0x55cbd2105430",
        rows: "42",
        columns: "2",
        steady_ns: "442367424",
        steady_ms: "136991",
        clock_request: "7",
        db_thread: "1882",
        db_clock: "1794314193.863159",
    },
    AnticheatQuery {
        request_line: 61734907,
        clock_line: 61734910,
        result_line: 61734922,
        sql: b"SELECT id,type,str,data,address,length,result,flags,comment FROM warden_scans",
        connection: "4294967298",
        request: "203677",
        result: "0x55cbd2100cb0",
        rows: "89",
        columns: "9",
        steady_ns: "485533432",
        steady_ms: "137035",
        clock_request: "268",
        db_thread: "1878",
        db_clock: "1794314193.906322",
    },
];

impl AnticheatQuery {
    fn lines(&self) -> [u64; 7] {
        [
            self.request_line,
            self.request_line + 1,
            self.clock_line,
            self.clock_line + 1,
            self.clock_line + 2,
            self.result_line - 2,
            self.result_line,
        ]
    }

    fn observe(&self, index: usize, kind: &str, p: &[&str], line: u64) {
        let size = self.sql.len().to_string();
        let (expected_kind, fields, expected): (&str, &[&str], &[&str]) = match index {
            0 => ("sql_query_request", p, &[self.connection, self.request]),
            1 | 5 => {
                assert_eq!(unhex(p[5]), self.sql, "trace line {line}");
                if index == 1 {
                    ("sql_request_bytes", &p[1..5], &["0", "0", &size, "0"])
                } else {
                    ("sql_query", &p[..5], &[self.result, "0", "0", &size, "0"])
                }
            }
            2 => ("clock_sample", &p[..3], &["1", "443366", self.steady_ns]),
            3 => ("clock_ms", p, &[self.steady_ms]),
            4 => (
                "sql_clock_request",
                p,
                &[self.connection, self.clock_request, self.db_thread, "1"],
            ),
            6 => (
                "sql_query_result",
                p,
                &[
                    self.connection,
                    self.request,
                    self.result,
                    self.rows,
                    self.columns,
                    "0",
                ],
            ),
            _ => unreachable!(),
        };
        assert_eq!(kind, expected_kind, "trace line {line}");
        assert_eq!(fields, expected, "trace line {line}");
    }
}

#[derive(Default)]
pub(super) struct AnticheatEvidence {
    config_reads: usize,
    key_lines: Vec<u64>,
    worker_lines: Vec<u64>,
    query_lines: Vec<u64>,
}

impl AnticheatEvidence {
    pub(super) fn observe(&mut self, thread: u64, line: u64, kind: &str, p: &[&str]) {
        assert!(!kind.starts_with("sql_write") && !kind.starts_with("rng_") && kind != "mt_raw");
        if thread == 11 {
            let (expected_kind, fields, expected): (&str, &[&str], &[&str]) = match line {
                61732724 => ("thread_birth", p, &["11", "1", "10", "895", "8096", "16"]),
                61732736 => ("clock_sample", &p[..3], &["0", "1794314193", "812834653"]),
                61732738 => ("clock_sample", &p[..3], &["1", "443366", "392374092"]),
                61732739 => ("clock_ms", p, &["136941"]),
                61734893 => ("clock_sample", &p[..3], &["0", "1794314193", "905868133"]),
                61734898 => ("clock_sample", &p[..3], &["0", "1794314193", "905932094"]),
                _ => return,
            };
            assert_eq!(kind, expected_kind, "trace line {line}");
            assert_eq!(fields, expected, "trace line {line}");
            self.worker_lines.push(line);
            return;
        }
        self.config_reads += usize::from(kind == "config");
        let key: Option<&[&str]> = match line {
            61727921 => Some(&["Enable", "default", "false"]),
            61728041 => Some(&["Antispam.Enable", "default", "false"]),
            61732668 => Some(&["Warden.Enable", "default", "false"]),
            61732693 => Some(&["Warden.ModuleDir", "default", "warden_modules"]),
            _ => None,
        };
        if let Some(key) = key {
            assert_eq!(kind, "config", "trace line {line}");
            assert_eq!(p, key, "trace line {line}");
            self.key_lines.push(line);
        }
        for query in &ANTICHEAT_QUERIES {
            if let Some(index) = query
                .lines()
                .iter()
                .position(|&candidate| candidate == line)
            {
                query.observe(index, kind, p, line);
                self.query_lines.push(line);
                return;
            }
        }
        assert!(!kind.starts_with("sql_query") && kind != "sql_request_bytes");
    }

    pub(super) fn assert_matches(&self, recording: &Path) {
        assert_eq!(self.config_reads, 139);
        assert_eq!(self.key_lines, [61727921, 61728041, 61732668, 61732693]);
        assert_eq!(
            self.worker_lines,
            [61732724, 61732736, 61732738, 61732739, 61734893, 61734898]
        );
        assert!(self
            .query_lines
            .iter()
            .copied()
            .eq(ANTICHEAT_QUERIES.iter().flat_map(AnticheatQuery::lines)));

        let reader = BufReader::new(
            std::fs::File::open(recording.join("private-db-statement-clocks.tsv")).unwrap(),
        );
        let mut lines = reader.lines().skip(375);
        for query in &ANTICHEAT_QUERIES {
            let row = lines.next().unwrap().unwrap();
            let row: Vec<_> = row.split('\t').collect();
            assert_eq!(row.len(), 4);
            assert_eq!(row[0], query.db_thread);
            assert_eq!(row[1], query.db_clock);
            assert_eq!(unhex(row[2]), query.sql);
            assert_eq!(row[3], "0");
        }

        let rules: serde_json::Value = serde_json::from_reader(BufReader::new(
            std::fs::File::open(recording.join("runtime-anticheat-rule-stores.json")).unwrap(),
        ))
        .unwrap();
        assert_eq!(rules["typed_getter_count"], 96);
        assert_eq!(rules["effective_store_count"], 96);
        assert_eq!(rules["all_96_rule_stores_bound"], true);
        let bindings = rules["bindings"].as_array().unwrap();
        assert_eq!(bindings.len(), 96);
        assert!(bindings.iter().all(|binding| binding["matched"] == true));
        let console =
            BufReader::new(std::fs::File::open(recording.join("run/console.log")).unwrap());
        assert_eq!(
            console.lines().nth(633).unwrap().unwrap(),
            ">> 9 scripted Warden scans loaded from anticheat module"
        );
    }
}
