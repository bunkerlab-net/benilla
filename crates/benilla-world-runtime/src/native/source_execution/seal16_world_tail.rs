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
