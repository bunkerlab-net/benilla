// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_sim::source_calendar::{self, Tm};
use std::fmt;

const DAY: i64 = 86_400;
const WEEK: i64 = 7 * DAY;

#[derive(Clone, Copy)]
pub struct WeeklyQuestResetConfig {
    weekday: i32,
    hour: i32,
}
impl WeeklyQuestResetConfig {
    // World.cpp:669-670,2395-2402,2460-2473 normalizes the two signed inputs.
    pub fn new(weekday: i32, hour: i32) -> Self {
        Self {
            weekday: if weekday < 0 { 3 } else { weekday.min(6) },
            hour: if hour < 0 { 6 } else { hour.min(23) },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldStartupClockSite {
    MissingWeeklyReset,
    WeeklyResetNow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldStartupSql {
    SelectWeeklyReset(Option<u64>),
    InsertWeeklyReset(u64),
}
impl fmt::Display for WorldStartupSql {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SelectWeeklyReset(_) => {
                out.write_str("SELECT NextWeeklyQuestResetTime FROM saved_variables")
            }
            Self::InsertWeeklyReset(seconds) => write!(
                out,
                "INSERT INTO saved_variables (NextWeeklyQuestResetTime) VALUES ('{seconds}')"
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldStartupObservation {
    Sql(WorldStartupSql),
    LocalTime {
        seconds: i64,
        civil: Tm,
    },
    MakeTime {
        input: Tm,
        seconds: i64,
        normalized: Tm,
    },
}

pub trait WorldStartupWork {
    fn clock(&mut self, site: WorldStartupClockSite) -> Result<ClockSample>;
    fn observe(&mut self, observation: WorldStartupObservation) -> Result<()>;
}

#[derive(Default)]
pub(super) struct SourceSavedVariables {
    row_exists: bool,
    next_weekly_reset: i64,
}
impl SourceSavedVariables {
    pub(super) fn next_weekly_reset(&self) -> i64 {
        self.next_weekly_reset
    }

    // World.cpp:2275-2304 normalizes the stored reset against the configured weekly slot.
    pub(super) fn init_weekly_reset(
        &mut self,
        config: WeeklyQuestResetConfig,
        work: &mut impl WorldStartupWork,
    ) -> Result<()> {
        work.observe(WorldStartupObservation::Sql(
            WorldStartupSql::SelectWeeklyReset(
                self.row_exists.then_some(self.next_weekly_reset as u64),
            ),
        ))?;
        if !self.row_exists {
            self.next_weekly_reset = wall(work, WorldStartupClockSite::MissingWeeklyReset)?;
        }
        let now = wall(work, WorldStartupClockSite::WeeklyResetNow)?;
        let mut civil = source_calendar::localtime(now)
            .map_err(|_| refuse("execution.world.World_2285.localtime_null_undefined"))?;
        work.observe(WorldStartupObservation::LocalTime {
            seconds: now,
            civil,
        })?;
        let offset = i64::from(civil.wday - config.weekday) * DAY;
        civil.hour = config.hour;
        civil.min = 0;
        civil.sec = 0;
        let input = civil;
        let seconds = source_calendar::mktime(&mut civil)
            .map_err(|_| refuse("execution.world.World_2293.mktime_range"))?;
        work.observe(WorldStartupObservation::MakeTime {
            input,
            seconds,
            normalized: civil,
        })?;
        let mut reset = seconds - offset;
        if now >= reset {
            reset += WEEK;
        }
        self.next_weekly_reset = if self.next_weekly_reset < now {
            reset - WEEK
        } else {
            reset
        };
        if !self.row_exists {
            self.row_exists = true;
            work.observe(WorldStartupObservation::Sql(
                WorldStartupSql::InsertWeeklyReset(self.next_weekly_reset as u64),
            ))?;
        }
        Ok(())
    }
}

fn wall(work: &mut impl WorldStartupWork, site: WorldStartupClockSite) -> Result<i64> {
    let sample = work.clock(site)?;
    require(
        sample.kind() == ClockKind::WallSeconds,
        "execution.world.weekly_reset_clock_kind",
    )?;
    Ok(sample.value())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    #[derive(Default)]
    struct Work {
        clocks: VecDeque<(WorldStartupClockSite, i64)>,
        sql: Vec<WorldStartupSql>,
    }
    impl WorldStartupWork for Work {
        fn clock(&mut self, site: WorldStartupClockSite) -> Result<ClockSample> {
            let (expected, seconds) = self.clocks.pop_front().unwrap();
            assert_eq!(site, expected);
            Ok(ClockSample::authored(ClockKind::WallSeconds, seconds))
        }

        fn observe(&mut self, observation: WorldStartupObservation) -> Result<()> {
            if let WorldStartupObservation::Sql(operation) = observation {
                self.sql.push(operation);
            }
            Ok(())
        }
    }

    #[test]
    fn weekly_reset_selects_past_slot_when_missing_sample_precedes_now() {
        let mut saved = SourceSavedVariables::default();
        let config = WeeklyQuestResetConfig::new(3, 6);
        let mut work = Work {
            clocks: [
                (WorldStartupClockSite::MissingWeeklyReset, 1794314192),
                (WorldStartupClockSite::WeeklyResetNow, 1794314193),
            ]
            .into(),
            ..Work::default()
        };
        saved.init_weekly_reset(config, &mut work).unwrap();
        assert!(work.clocks.is_empty());
        assert_eq!(saved.next_weekly_reset(), 1793772000);
        assert_eq!(
            work.sql,
            [
                WorldStartupSql::SelectWeeklyReset(None),
                WorldStartupSql::InsertWeeklyReset(1793772000),
            ]
        );

        work.clocks
            .push_back((WorldStartupClockSite::WeeklyResetNow, 1794314193));
        saved.init_weekly_reset(config, &mut work).unwrap();
        assert!(work.clocks.is_empty());
        assert_eq!(saved.next_weekly_reset(), 1793772000);
        assert_eq!(
            &work.sql[2..],
            [WorldStartupSql::SelectWeeklyReset(Some(1793772000))]
        );
    }

    #[test]
    fn weekly_reset_refuses_a_clock_outside_localtime_range() {
        let mut saved = SourceSavedVariables::default();
        let mut work = Work {
            clocks: [
                (WorldStartupClockSite::MissingWeeklyReset, 1794314193),
                (WorldStartupClockSite::WeeklyResetNow, i64::MAX),
            ]
            .into(),
            ..Work::default()
        };
        assert_eq!(
            saved
                .init_weekly_reset(WeeklyQuestResetConfig::new(3, 6), &mut work)
                .unwrap_err()
                .id,
            "execution.world.World_2285.localtime_null_undefined"
        );
        assert!(work.clocks.is_empty());
        assert_eq!(work.sql, [WorldStartupSql::SelectWeeklyReset(None)]);
    }
}
