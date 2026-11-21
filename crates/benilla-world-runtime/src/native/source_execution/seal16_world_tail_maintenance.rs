// SPDX-License-Identifier: GPL-3.0-or-later
//! Maintenance trace bindings and comparisons.
use super::{numeric, tm, unhex, Tm, VecDeque, WorldStartupObservation, WorldStartupSql};

#[derive(Default)]
pub(super) struct MaintenanceEvidence {
    weekday: Option<i32>,
    game_time: Option<i64>,
    read_value: Option<u32>,
    calls: VecDeque<MaintenanceObservation>,
    pending_sql: Option<Vec<u8>>,
    milliseconds: Vec<(u64, u64)>,
    transaction_clocks: Vec<(u64, u64)>,
}

enum MaintenanceObservation {
    Sql(Vec<u8>, u64),
    Local(i64, Tm),
}

impl MaintenanceEvidence {
    pub(super) fn observe_trace(&mut self, line: u64, kind: &str, p: &[&str]) {
        match (line, kind) {
            (7330, "config") => {
                assert_eq!(&p[..2], ["MaintenanceDay", "file"]);
                assert!(self.weekday.replace(numeric(p, 2)).is_none());
            }
            (60100830, "clock_time") => {
                assert_eq!(p[1], "0x55cb9c2b2c58");
                assert!(self.game_time.replace(numeric(p, 0)).is_none());
            }
            (61722219 | 61722373 | 61722515, "sql_request_bytes")
            | (61722657 | 61722689, "sql_transaction_command")
            | (61722672 | 61722719, "sql_write_bytes") => {
                assert!(self.pending_sql.replace(unhex(p[5])).is_none());
            }
            (61722338 | 61722492 | 61722634, "sql_query_result") => {
                assert_eq!(p[5], "0");
                self.push_sql(numeric(p, 3));
            }
            (61722354, "sql_conversion_logical") => {
                assert_eq!(&p[..5], ["4294967672", "1", "0", "3", "8"]);
                let value = u32::try_from(u64::from_str_radix(p[5], 16).unwrap()).unwrap();
                assert!(self.read_value.replace(value).is_none());
            }
            (61722356..=61722359, "clock_localtime") => self
                .calls
                .push_back(MaintenanceObservation::Local(numeric(p, 0), tm(&p[1..]))),
            (61722659 | 61722691, "sql_transaction_result") => {
                assert_eq!(p[0], "4294967302");
                self.push_sql(0);
            }
            (61722677 | 61722752, "sql_write_result") => {
                assert_eq!(p[3], "0");
                self.push_sql(numeric(p, 2));
            }
            (61722222 | 61722376 | 61722518 | 61722675 | 61722722, "clock_ms") => {
                self.milliseconds.push((line, numeric(p, 0)));
            }
            (61722658 | 61722690, "sql_clock_request") => {
                assert_eq!((p[0], p[2], p[3]), ("4294967302", "1881", "3"));
                self.transaction_clocks.push((line, numeric(p, 1)));
            }
            _ => (),
        }
    }

    pub(super) fn inputs(&self) -> (i64, i32) {
        (self.game_time.unwrap(), self.weekday.unwrap())
    }

    fn push_sql(&mut self, rows: u64) {
        self.calls.push_back(MaintenanceObservation::Sql(
            self.pending_sql.take().unwrap(),
            rows,
        ));
    }

    pub(super) fn observe(&mut self, value: WorldStartupObservation) {
        match (self.calls.pop_front().unwrap(), value) {
            (MaintenanceObservation::Sql(request, rows), WorldStartupObservation::Sql(sql)) => {
                assert_eq!(sql.to_string().as_bytes(), request);
                let actual = match sql {
                    WorldStartupSql::SelectMaintenance(value) => {
                        assert_eq!(value, self.read_value);
                        u64::from(value.is_some())
                    }
                    WorldStartupSql::InsertMaintenance(_)
                    | WorldStartupSql::UpdateMaintenance(_) => 1,
                    WorldStartupSql::HonorDates { rows_present, .. }
                    | WorldStartupSql::HonorKills { rows_present, .. } => u64::from(rows_present),
                    WorldStartupSql::HonorDelete { affected, .. } => affected as u64,
                    WorldStartupSql::HonorBegin | WorldStartupSql::HonorCommit => 0,
                    _ => panic!("maintenance SQL trace diverged"),
                };
                assert_eq!(actual, rows);
            }
            (
                MaintenanceObservation::Local(seconds, civil),
                WorldStartupObservation::LocalTime {
                    seconds: actual_seconds,
                    civil: actual_civil,
                },
            ) => assert_eq!((actual_seconds, actual_civil), (seconds, civil)),
            _ => panic!("maintenance trace order diverged"),
        }
    }

    pub(super) fn assert_consumed(&self) {
        assert!(self.calls.is_empty() && self.pending_sql.is_none());
        assert_eq!(
            self.milliseconds,
            [
                (61722222, 136482),
                (61722376, 136524),
                (61722518, 136566),
                (61722675, 136608),
                (61722722, 136609),
            ]
        );
        assert_eq!(self.transaction_clocks, [(61722658, 34), (61722690, 36)]);
    }
}
