// SPDX-License-Identifier: GPL-3.0-or-later
use super::world_state::start_war_effort_event;
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
    SelectMaintenance(Option<u32>),
    InsertMaintenance(u32),
    UpdateMaintenance(u32),
    HonorDates {
        date_top: u32,
        rows_present: bool,
    },
    HonorKills {
        through: u32,
        rows_present: bool,
    },
    HonorBegin,
    HonorDelete {
        through: u32,
        affected: usize,
    },
    HonorCommit,
    HonorStandings {
        from: u32,
        through: u32,
        rows_present: bool,
    },
    SpamRecords(usize),
    SelectWorldState {
        rows: usize,
    },
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
            Self::SelectMaintenance(_) => {
                out.write_str("SELECT NextMaintenanceDate FROM saved_variables")
            }
            Self::InsertMaintenance(date) => write!(
                out,
                "INSERT INTO saved_variables (NextMaintenanceDate) VALUES ('{date}')"
            ),
            Self::UpdateMaintenance(date) => write!(
                out,
                "UPDATE saved_variables SET NextMaintenanceDate = '{date}'"
            ),
            Self::HonorDates { date_top, .. } => write!(
                out,
                "SELECT date FROM character_honor_cp WHERE TYPE = 1 AND date <= {date_top} GROUP BY date ORDER BY date DESC"
            ),
            Self::HonorKills { through, .. } => write!(
                out,
                "SELECT guid,TYPE,COUNT(*) AS kills FROM character_honor_cp WHERE date <= {through} AND victim_type>0 GROUP BY guid,type"
            ),
            Self::HonorBegin => out.write_str("START TRANSACTION"),
            Self::HonorDelete { through, .. } => write!(
                out,
                "DELETE FROM character_honor_cp WHERE date <= {through}"
            ),
            Self::HonorCommit => out.write_str("COMMIT"),
            Self::HonorStandings { from, through, .. } => write!(
                out,
                "SELECT guid,SUM(honor) as honor_sum FROM character_honor_cp WHERE TYPE = {HONORABLE} AND date BETWEEN {from} AND {through} GROUP BY guid ORDER BY honor_sum DESC"
            ),
            Self::SpamRecords(_) => out.write_str("SELECT record FROM spam_records"),
            Self::SelectWorldState { .. } => out.write_str("SELECT Id, Data FROM world_state"),
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

pub trait WorldTailOwner: GameEventOwner + EventEffectOwner {
    fn ready_world(&mut self) -> Result<&mut SourceWorld>;
    fn event_effects(&self) -> &SourceEventEffects;

    fn war_effort_gameobject(&mut self, _map: MapIdentity, _db_guid: u32) -> Result<()> {
        Err(refuse(WAR_EFFORT_GAMEOBJECT_FACTORY))
    }
}

pub struct WorldTailConfig {
    pub weekly: WeeklyQuestResetConfig,
    pub calendar: Calendar,
    pub maintenance_weekday: i32,
    pub event_announce: bool,
}

pub fn initialize_world_tail(
    owner: &mut impl WorldTailOwner,
    config: WorldTailConfig,
    spam_rows: Vec<Vec<u8>>,
    state_rows: &[WorldStateRow<'_>],
    clock: &SourceWorldClock,
    work: &mut impl WorldStartupWork,
    events: &mut SourceGameEventManager,
) -> Result<()> {
    let world = owner.ready_world()?;
    // Deviation: Omit World.cpp:1398 because solo worlds have no login database.
    // World.cpp:1402 initializes the weekly quest reset after the ban cleanup.
    world.init_weekly_quest_reset_time(config.weekly, work)?;
    // World.cpp:1406 starts the server maintenance check.
    world.init_server_maintenance_check(config.calendar, config.maintenance_weekday, work)?;
    // World.cpp:1409 loads the honor standings after maintenance.
    world.load_honor_standings(config.calendar, config.maintenance_weekday, work)?;
    // World.cpp:1412 loads spam records after honor standings.
    world.load_spam_records(false, spam_rows, work)?;
    // World.cpp:1416-1417 initializes events and sets the same timer World::Update uses.
    let delay = events.initialize(config.event_announce, owner, |owner| {
        owner.event_sql(1, "SELECT event FROM game_event_status")?;
        let status = owner.event_effects().status();
        Ok((!status.is_empty()).then(|| status.clone()))
    })?;
    owner
        .ready_world()?
        .event_timer
        .set_interval(i64::from(delay));
    // Deviation: Omit World.cpp:1421 because solo worlds have no realm character deletion queue.
    // Deviation: Omit World.cpp:1424 because solo worlds have no realm anticheat service.

    // CMakeCache.txt:18 disables BUILD_AHBOT, so World.cpp:1426-1430 does not run.
    // World.cpp:1433 and WorldState.cpp:108-214 load the live world state.
    work.observe(WorldStartupObservation::Sql(
        WorldStartupSql::SelectWorldState {
            rows: state_rows.len(),
        },
    ))?;
    let state = &mut owner.ready_world()?.world_state;
    state.load_rows(state_rows, clock)?;
    // WorldState.cpp:215 starts the war effort event before the resource spawns.
    let (phase, killed_bosses) = state.war_effort();
    start_war_effort_event(phase, killed_bosses, |id| {
        events.start_event(id, false, false, config.event_announce, owner)
    })?;
    // WorldState.cpp:216,863-869 visits the team map even when the pass has no GUIDs.
    for resource in 0.. {
        let Some(pass) = owner.ready_world()?.world_state.war_effort_pass(resource) else {
            break;
        };
        let mut after = None;
        while let Some(map) = owner.event_next_map(Some(pass.map), after)? {
            for &guid in pass.guids() {
                owner.war_effort_gameobject(map, guid)?;
            }
            after = Some(map);
        }
    }
    owner.ready_world()?.world_state.resume_scourge_invasion()?;
    Err(refuse(
        "execution.world.WorldState_222.emerald_dragons_owner",
    ))
}

#[derive(Default)]
pub(super) struct SourceSavedVariables {
    row_exists: bool,
    next_weekly_reset: i64,
    pub(super) next_maintenance: u32,
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

const HONORABLE: u8 = 1; // Player.h:328.

#[derive(Clone, Copy)]
struct HonorContribution {
    date: u32,
    kind: u8,
    victim_type: u8,
}

// ObjectMgr.h:413-427 defines each team standing.
#[derive(Clone, Copy, Default)]
pub struct SourceHonorStanding {
    pub guid: u32,
    pub honor_points: f32,
    pub honor_kills: u32,
    pub rp_earning: f32,
}

#[derive(Default)]
pub(super) struct SourceHonor {
    contributions: Vec<HonorContribution>,
    pub(super) alliance: Vec<SourceHonorStanding>,
    pub(super) horde: Vec<SourceHonorStanding>,
}
impl SourceHonor {
    // ObjectMgr.cpp:3724-3745,3776-3784 loads last week's team standings.
    pub(super) fn load_standings(
        &mut self,
        cached_game_seconds: i64,
        weekday: i32,
        work: &mut impl WorldStartupWork,
    ) -> Result<()> {
        let through = last_maintenance_day(cached_game_seconds, weekday, work)?;
        let from = through.wrapping_sub(7);
        self.alliance.clear();
        self.horde.clear();
        let rows_present = self
            .contributions
            .iter()
            .any(|row| row.kind == HONORABLE && (from..=through).contains(&row.date));
        work.observe(WorldStartupObservation::Sql(
            WorldStartupSql::HonorStandings {
                from,
                through,
                rows_present,
            },
        ))?;
        // ObjectMgr.cpp:3864-3865 returns from both rank distributions on empty lists.
        require(
            !rows_present,
            "execution.honor.ObjectMgr_3745.player_team_lookup_owner",
        )
    }

    // ObjectMgr.cpp:3791-3851 flushes rank points, then prunes the old kill records.
    pub(super) fn flush(&mut self, date_top: u32, work: &mut impl WorldStartupWork) -> Result<()> {
        let dates = self
            .contributions
            .iter()
            .any(|row| row.kind == HONORABLE && row.date <= date_top);
        work.observe(WorldStartupObservation::Sql(WorldStartupSql::HonorDates {
            date_top,
            rows_present: dates,
        }))?;
        require(
            !dates,
            "execution.honor.ObjectMgr_3797.nonempty_rank_dates_owner",
        )?;

        let through = date_top.wrapping_sub(7);
        let kills = self
            .contributions
            .iter()
            .any(|row| row.victim_type > 0 && row.date <= through);
        work.observe(WorldStartupObservation::Sql(WorldStartupSql::HonorKills {
            through,
            rows_present: kills,
        }))?;
        require(
            !kills,
            "execution.honor.ObjectMgr_3831.nonempty_aggregate_kills_owner",
        )?;

        // SqlOperations.cpp:46-67 sends START only when the queued DELETE commits.
        work.observe(WorldStartupObservation::Sql(WorldStartupSql::HonorBegin))?;
        let before = self.contributions.len();
        self.contributions.retain(|row| row.date > through);
        work.observe(WorldStartupObservation::Sql(WorldStartupSql::HonorDelete {
            through,
            affected: before - self.contributions.len(),
        }))?;
        work.observe(WorldStartupObservation::Sql(WorldStartupSql::HonorCommit))
    }
}

// World.h:457-459 converts the existing game time to a packed local date.
fn date_today(seconds: i64, work: &mut impl WorldStartupWork) -> Result<u32> {
    let civil = maintenance_local(seconds, work)?;
    Ok(((civil.year as u32) << 16) | civil.yday as u32)
}

// World.h:461-468 reads and rounds the same game time to the configured weekday.
fn last_maintenance_day(
    seconds: i64,
    weekday: i32,
    work: &mut impl WorldStartupWork,
) -> Result<u32> {
    let weekday = if weekday < 0 { 4 } else { weekday.min(6) } as u32;
    let today = date_today(seconds, work)?;
    let civil = maintenance_local(seconds, work)?;
    Ok(today.wrapping_sub((civil.wday as u32 + 7 - weekday) % 7))
}

fn maintenance_local(seconds: i64, work: &mut impl WorldStartupWork) -> Result<Tm> {
    let civil = source_calendar::localtime(seconds)
        .map_err(|_| refuse("execution.world.World_458.localtime_null_undefined"))?;
    work.observe(WorldStartupObservation::LocalTime { seconds, civil })?;
    Ok(civil)
}

impl SourceSavedVariables {
    // World.cpp:2199-2215 reads the next maintenance date and starts it when due.
    pub(super) fn init_maintenance(
        &mut self,
        cached_game_seconds: i64,
        weekday: i32,
        honor: &mut SourceHonor,
        work: &mut impl WorldStartupWork,
    ) -> Result<()> {
        work.observe(WorldStartupObservation::Sql(
            WorldStartupSql::SelectMaintenance(self.row_exists.then_some(self.next_maintenance)),
        ))?;
        if !self.row_exists {
            let date = last_maintenance_day(cached_game_seconds, weekday, work)?;
            self.next_maintenance = if date == date_today(cached_game_seconds, work)? {
                date
            } else {
                date.wrapping_add(7)
            };
            self.row_exists = true;
            work.observe(WorldStartupObservation::Sql(
                WorldStartupSql::InsertMaintenance(self.next_maintenance),
            ))?;
        }
        if self.next_maintenance <= date_today(cached_game_seconds, work)? {
            self.start_maintenance(cached_game_seconds, weekday, honor, work)?;
        }
        Ok(())
    }

    // World.cpp:2180-2196 performs maintenance before the first session can exist.
    fn start_maintenance(
        &mut self,
        cached_game_seconds: i64,
        weekday: i32,
        honor: &mut SourceHonor,
        work: &mut impl WorldStartupWork,
    ) -> Result<()> {
        let last = last_maintenance_day(cached_game_seconds, weekday, work)?;
        self.next_maintenance = last.wrapping_add(7);
        if self.next_maintenance <= date_today(cached_game_seconds, work)? {
            self.next_maintenance = self.next_maintenance.wrapping_add(7);
        }
        honor.flush(last, work)?;
        // Master.cpp:126,142 starts the world runnable after this startup call.
        work.observe(WorldStartupObservation::Sql(
            WorldStartupSql::UpdateMaintenance(self.next_maintenance),
        ))
    }
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

    #[test]
    fn maintenance_closes_missing_row_without_flushing_before_due_day() {
        let mut saved = SourceSavedVariables::default();
        let mut honor = SourceHonor::default();
        let mut work = Work::default();
        saved
            .init_maintenance(1794314189, 3, &mut honor, &mut work)
            .unwrap();
        assert_eq!(saved.next_maintenance, 8257850);
        assert_eq!(
            work.sql,
            [
                WorldStartupSql::SelectMaintenance(None),
                WorldStartupSql::InsertMaintenance(8257850),
            ]
        );
    }

    #[test]
    fn maintenance_prunes_old_non_kill_rows_and_keeps_the_newer_rows() {
        let mut honor = SourceHonor {
            contributions: vec![
                HonorContribution {
                    date: 8257836,
                    kind: 2,
                    victim_type: 0,
                },
                HonorContribution {
                    date: 8257837,
                    kind: 2,
                    victim_type: 1,
                },
            ],
            ..SourceHonor::default()
        };
        let mut work = Work::default();
        honor.flush(8257843, &mut work).unwrap();
        assert_eq!(honor.contributions.len(), 1);
        assert_eq!(honor.contributions[0].date, 8257837);
        assert_eq!(
            work.sql,
            [
                WorldStartupSql::HonorDates {
                    date_top: 8257843,
                    rows_present: false
                },
                WorldStartupSql::HonorKills {
                    through: 8257836,
                    rows_present: false
                },
                WorldStartupSql::HonorBegin,
                WorldStartupSql::HonorDelete {
                    through: 8257836,
                    affected: 1
                },
                WorldStartupSql::HonorCommit,
            ]
        );
    }

    #[test]
    fn maintenance_refuses_the_first_unowned_nonempty_honor_result() {
        let mut work = Work::default();
        for (kind, expected) in [
            (
                1,
                "execution.honor.ObjectMgr_3797.nonempty_rank_dates_owner",
            ),
            (
                2,
                "execution.honor.ObjectMgr_3831.nonempty_aggregate_kills_owner",
            ),
        ] {
            let mut honor = SourceHonor {
                contributions: vec![HonorContribution {
                    date: 8257836,
                    kind,
                    victim_type: 1,
                }],
                ..SourceHonor::default()
            };
            assert_eq!(honor.flush(8257843, &mut work).unwrap_err().id, expected);
            assert_eq!(honor.contributions.len(), 1);
        }
    }

    #[test]
    fn load_standings_clears_both_teams_on_an_empty_query() {
        let mut honor = SourceHonor {
            contributions: [(8257835, HONORABLE), (8257844, HONORABLE), (8257836, 2)]
                .map(|(date, kind)| HonorContribution {
                    date,
                    kind,
                    victim_type: 1,
                })
                .into(),
            alliance: vec![SourceHonorStanding::default()],
            horde: vec![SourceHonorStanding::default()],
        };
        let mut work = Work::default();
        honor.load_standings(1794314189, 3, &mut work).unwrap();
        assert!(honor.alliance.is_empty() && honor.horde.is_empty());
        assert_eq!(
            work.sql,
            [WorldStartupSql::HonorStandings {
                from: 8257836,
                through: 8257843,
                rows_present: false,
            }]
        );
    }

    #[test]
    fn load_standings_stops_at_team_lookup_for_either_inclusive_boundary() {
        for date in [8257836, 8257843] {
            let mut honor = SourceHonor {
                contributions: vec![HonorContribution {
                    date,
                    kind: HONORABLE,
                    victim_type: 1,
                }],
                alliance: vec![SourceHonorStanding::default()],
                horde: vec![SourceHonorStanding::default()],
            };
            let mut work = Work::default();
            let refusal = honor.load_standings(1794314189, 3, &mut work).unwrap_err();
            assert_eq!(
                refusal.id,
                "execution.honor.ObjectMgr_3745.player_team_lookup_owner"
            );
            assert!(honor.alliance.is_empty() && honor.horde.is_empty());
            assert_eq!(
                work.sql,
                [WorldStartupSql::HonorStandings {
                    from: 8257836,
                    through: 8257843,
                    rows_present: true,
                }]
            );
        }
    }

    #[test]
    fn load_spam_records_appends_or_replaces_in_query_order() {
        let mut world = SourceWorld::new(1, 100).unwrap();
        let mut work = Work::default();
        world
            .load_spam_records(
                false,
                vec![b"one".to_vec(), b"\0ignored".to_vec()],
                &mut work,
            )
            .unwrap();
        world
            .load_spam_records(false, vec![b"two".to_vec()], &mut work)
            .unwrap();
        assert_eq!(
            world.spam_records(),
            [b"one".to_vec(), vec![], b"two".to_vec()]
        );
        world
            .load_spam_records(
                true,
                vec![b"new\0ignored".to_vec(), b"last".to_vec()],
                &mut work,
            )
            .unwrap();
        assert_eq!(world.spam_records(), [b"new".to_vec(), b"last".to_vec()]);
        assert_eq!(
            work.sql,
            [
                WorldStartupSql::SpamRecords(2),
                WorldStartupSql::SpamRecords(1),
                WorldStartupSql::SpamRecords(2),
            ]
        );
    }

    #[test]
    fn load_spam_records_keeps_existing_rows_when_query_is_empty() {
        let mut world = SourceWorld::new(1, 100).unwrap();
        let mut work = Work::default();
        world
            .load_spam_records(false, vec![b"keep".to_vec()], &mut work)
            .unwrap();
        for reload in [false, true] {
            world.load_spam_records(reload, vec![], &mut work).unwrap();
            assert_eq!(world.spam_records(), [b"keep".to_vec()]);
        }
        assert_eq!(
            work.sql,
            [
                WorldStartupSql::SpamRecords(1),
                WorldStartupSql::SpamRecords(0),
                WorldStartupSql::SpamRecords(0),
            ]
        );
    }
}
