// SPDX-License-Identifier: GPL-3.0-or-later
//! Weekly reset and maintenance trace bindings.
use super::world_state::WorldStateEvidence;
use super::{
    tm, unhex, CalendarOwner, ClockKind, ClockSample, Result, SourceWorld, WeeklyQuestResetConfig,
    WorldStartupClockSite, WorldStartupObservation, WorldStartupSql, WorldStartupWork,
};
use crate::native::source_execution::Calendar;
use benilla_sim::source_calendar::Tm;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[path = "seal16_world_tail_maintenance.rs"]
mod maintenance;

pub(super) fn stock_spam_records(f1: &Path) -> Vec<Vec<u8>> {
    let file = std::fs::File::open(f1.join("initial-classicmangos.sql")).unwrap();
    let mut sql = BufReader::new(file);
    let mut line = Vec::new();
    loop {
        line.clear();
        assert_ne!(sql.read_until(b'\n', &mut line).unwrap(), 0);
        if line == b"LOCK TABLES `spam_records` WRITE;\n" {
            break;
        }
    }
    // initial-classicmangos.sql:823225-823229 has no rows between the key toggles.
    let expected: [&[u8]; 4] = [
        b"/*!40000 ALTER TABLE `spam_records` DISABLE KEYS */;\n",
        b"/*!40000 ALTER TABLE `spam_records` ENABLE KEYS */;\n",
        b"UNLOCK TABLES;\n",
        b"COMMIT;\n",
    ];
    for expected in expected {
        line.clear();
        sql.read_until(b'\n', &mut line).unwrap();
        assert_eq!(line, expected);
    }
    Vec::new()
}

#[derive(Default)]
pub(super) struct WeeklyEvidence {
    map_interval: Option<u32>,
    weekday: Option<i32>,
    hour: Option<i32>,
    clocks: VecDeque<(WorldStartupClockSite, ClockSample)>,
    reads: VecDeque<WeeklyObservation>,
    epoch_input: Option<[i32; 7]>,
    sql: VecDeque<(String, u64)>,
    pending_sql: Option<String>,
    wrapper_clocks: Vec<(u64, u64)>,
    maintenance: maintenance::MaintenanceEvidence,
    pub(super) world_state: WorldStateEvidence,
}

enum WeeklyObservation {
    Sql,
    Local(i64, Tm),
    Epoch([i32; 7], i64, Tm),
}

fn numeric<T: std::str::FromStr>(p: &[&str], i: usize) -> T
where
    T::Err: std::fmt::Debug,
{
    p[i].parse().unwrap()
}

impl WeeklyEvidence {
    pub(super) fn bind_config(&self) -> (SourceWorld, WeeklyQuestResetConfig) {
        (
            SourceWorld::new(1, self.map_interval.unwrap()).unwrap(),
            WeeklyQuestResetConfig::new(self.weekday.unwrap(), self.hour.unwrap()),
        )
    }

    pub(super) fn maintenance_inputs(&self) -> (Calendar, i32) {
        let (seconds, weekday) = self.maintenance.inputs();
        (Calendar::new(CalendarOwner::World, seconds), weekday)
    }

    pub(super) fn observe_trace(&mut self, line: u64, kind: &str, p: &[&str]) {
        match (line, kind) {
            (6100, "config") => {
                assert_eq!(&p[..2], ["MapUpdateInterval", "file"]);
                assert!(self.map_interval.replace(numeric(p, 2)).is_none());
            }
            (10035, "config") => {
                assert_eq!(&p[..2], ["Quests.Weekly.ResetWeekDay", "file"]);
                assert!(self.weekday.replace(numeric(p, 2)).is_none());
            }
            (10074, "config") => {
                assert_eq!(&p[..2], ["Quests.Weekly.ResetHour", "file"]);
                assert!(self.hour.replace(numeric(p, 2)).is_none());
            }
            (61722127, "sql_request_bytes") | (61722159, "sql_write_bytes") => {
                let sql = String::from_utf8(unhex(p[5])).unwrap();
                assert!(self.pending_sql.replace(sql).is_none());
            }
            (61722134, "sql_query_result") => {
                assert_eq!(&p[..2], ["4294967301", "203328"]);
                assert_eq!(&p[4..], ["1", "0"]);
                self.sql
                    .push_back((self.pending_sql.take().unwrap(), numeric(p, 3)));
                self.reads.push_back(WeeklyObservation::Sql);
            }
            (61722144 | 61722146, "clock_time") => {
                let site = if line == 61722144 {
                    WorldStartupClockSite::MissingWeeklyReset
                } else {
                    WorldStartupClockSite::WeeklyResetNow
                };
                self.clocks.push_back((
                    site,
                    ClockSample::authored(ClockKind::WallSeconds, numeric(p, 0)),
                ));
            }
            (61722147, "clock_localtime") => self
                .reads
                .push_back(WeeklyObservation::Local(numeric(p, 0), tm(&p[1..]))),
            (61722148, "clock_mktime_input") => {
                assert!(self
                    .epoch_input
                    .replace(std::array::from_fn(|i| numeric(p, i)))
                    .is_none());
            }
            (61722149, "clock_mktime_result") => self.reads.push_back(WeeklyObservation::Epoch(
                self.epoch_input.take().unwrap(),
                numeric(p, 0),
                tm(&p[1..]),
            )),
            (61722185, "sql_write_result") => {
                assert_eq!(&p[..2], ["4294967302", "203329"]);
                assert_eq!(p[3], "0");
                self.sql
                    .push_back((self.pending_sql.take().unwrap(), numeric(p, 2)));
                self.reads.push_back(WeeklyObservation::Sql);
            }
            (61722130 | 61722162, "clock_ms") => {
                self.wrapper_clocks.push((line, numeric(p, 0)));
            }
            _ => self.maintenance.observe_trace(line, kind, p),
        }
    }

    pub(super) fn assert_consumed(&self) {
        assert!(self.clocks.is_empty() && self.reads.is_empty() && self.sql.is_empty());
        assert!(self.pending_sql.is_none() && self.epoch_input.is_none());
        assert_eq!(
            self.wrapper_clocks,
            [(61722130, 136475), (61722162, 136476)]
        );
        self.maintenance.assert_consumed();
        self.world_state.assert_loaded();
    }
}

impl WorldStartupWork for WeeklyEvidence {
    fn clock(&mut self, site: WorldStartupClockSite) -> Result<ClockSample> {
        let (expected, sample) = self.clocks.pop_front().unwrap();
        assert_eq!(site, expected);
        Ok(sample)
    }

    fn observe(&mut self, value: WorldStartupObservation) -> Result<()> {
        if let WorldStartupObservation::Sql(sql @ WorldStartupSql::SelectWorldState { .. }) = value
        {
            self.world_state.observe_load(sql);
            return Ok(());
        }
        if self.reads.is_empty() {
            self.maintenance.observe(value);
            return Ok(());
        }
        match (self.reads.pop_front().unwrap(), value) {
            (WeeklyObservation::Sql, WorldStartupObservation::Sql(sql)) => {
                let (expected, rows) = self.sql.pop_front().unwrap();
                assert_eq!(sql.to_string(), expected);
                let actual_rows = match sql {
                    WorldStartupSql::SelectWeeklyReset(value) => u64::from(value.is_some()),
                    WorldStartupSql::InsertWeeklyReset(_) => 1,
                    _ => panic!("weekly reset SQL trace diverged"),
                };
                assert_eq!(actual_rows, rows);
            }
            (
                WeeklyObservation::Local(seconds, civil),
                WorldStartupObservation::LocalTime {
                    seconds: actual_seconds,
                    civil: actual_civil,
                },
            ) => assert_eq!((actual_seconds, actual_civil), (seconds, civil)),
            (
                WeeklyObservation::Epoch(fields, seconds, normalized),
                WorldStartupObservation::MakeTime {
                    input,
                    seconds: actual_seconds,
                    normalized: actual_tm,
                },
            ) => {
                assert_eq!(
                    [
                        input.sec,
                        input.min,
                        input.hour,
                        input.mday,
                        input.mon,
                        input.year,
                        input.isdst
                    ],
                    fields
                );
                assert_eq!((actual_seconds, actual_tm), (seconds, normalized));
            }
            _ => panic!("weekly reset trace order diverged"),
        }
        Ok(())
    }
}
