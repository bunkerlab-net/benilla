// SPDX-License-Identifier: GPL-3.0-or-later
//! Component execution only. Unsupported managers must refuse, never replay results.
use super::*;
use benilla_sim::source_time::IntervalTimer;
use std::collections::BTreeMap;

#[path = "world_startup.rs"]
mod startup;
pub use startup::{
    initialize_world_tail, SourceHonorStanding, WeeklyQuestResetConfig, WorldStartupClockSite,
    WorldStartupObservation, WorldStartupSql, WorldStartupWork, WorldTailConfig, WorldTailOwner,
};
#[path = "world_state.rs"]
mod world_state;
pub use world_state::WorldStateRow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldPhase {
    ClockSnapshot,
    Timers,
    GameTime,
    Messages,
    MassMail,
    WeeklyQuests,
    Auctions,
    Sessions,
    Uptime,
    Maps,
    Battlegrounds,
    OutdoorPvp,
    WorldState,
    Groups,
    DeleteCharacters,
    SqlCallbacks,
    Corpses,
    GameEvents,
    DelayedRemovals,
    PersistentState,
    Maintenance,
    Cli,
    Terrain,
}
// Preserve disabled-bot source order, including conditional phase owners; World.cpp:1511-1702.
pub const WORLD_PHASES: [WorldPhase; 23] = [
    WorldPhase::ClockSnapshot,
    WorldPhase::Timers,
    WorldPhase::GameTime,
    WorldPhase::Messages,
    WorldPhase::MassMail,
    WorldPhase::WeeklyQuests,
    WorldPhase::Auctions,
    WorldPhase::Sessions,
    WorldPhase::Uptime,
    WorldPhase::Maps,
    WorldPhase::Battlegrounds,
    WorldPhase::OutdoorPvp,
    WorldPhase::WorldState,
    WorldPhase::Groups,
    WorldPhase::DeleteCharacters,
    WorldPhase::SqlCallbacks,
    WorldPhase::Corpses,
    WorldPhase::GameEvents,
    WorldPhase::DelayedRemovals,
    WorldPhase::PersistentState,
    WorldPhase::Maintenance,
    WorldPhase::Cli,
    WorldPhase::Terrain,
];
impl WorldPhase {
    pub fn source_line(self) -> u32 {
        match self {
            Self::ClockSnapshot => 1511,
            Self::Timers => 1543,
            Self::GameTime => 1552,
            Self::Messages => 1554,
            Self::MassMail => 1557,
            Self::WeeklyQuests => 1560,
            Self::Auctions => 1564,
            Self::Sessions => 1606,
            Self::Uptime => 1609,
            Self::Maps => 1623,
            Self::Battlegrounds => 1627,
            Self::OutdoorPvp => 1628,
            Self::WorldState => 1629,
            Self::Groups => 1634,
            Self::DeleteCharacters => 1645,
            Self::SqlCallbacks => 1652,
            Self::Corpses => 1655,
            Self::GameEvents => 1663,
            Self::DelayedRemovals => 1681,
            Self::PersistentState => 1684,
            Self::Maintenance => 1686,
            Self::Cli => 1699,
            Self::Terrain => 1702,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ClockSchedulerPending {
    QueuePopLinearization,
    SealedClockOwners,
    CalendarTimezoneAndConversion,
    WorldPhaseInputs,
    MapGenerationBindings,
    StartupThreadBirths,
    CompleteJobAndBarrierEdges,
    MessageCaptureAndHandlerOwners,
}
pub const CLOCK_SCHEDULER_PENDING: [ClockSchedulerPending; 8] = [
    ClockSchedulerPending::SealedClockOwners,
    ClockSchedulerPending::QueuePopLinearization,
    ClockSchedulerPending::CalendarTimezoneAndConversion,
    ClockSchedulerPending::WorldPhaseInputs,
    ClockSchedulerPending::MapGenerationBindings,
    ClockSchedulerPending::StartupThreadBirths,
    ClockSchedulerPending::CompleteJobAndBarrierEdges,
    ClockSchedulerPending::MessageCaptureAndHandlerOwners,
];

pub struct SourceMapManager {
    timer: IntervalTimer,
    maps: BTreeMap<(u32, u32), MapIdentity>,
    active_diff: Option<u32>,
    next_job: u64,
}
impl SourceMapManager {
    pub fn new(interval: u32) -> Self {
        let mut timer = IntervalTimer::default();
        timer.set_interval(i64::from(interval));
        Self {
            timer,
            maps: BTreeMap::new(),
            active_diff: None,
            next_job: 1,
        }
    }
    pub fn register(&mut self, map: MapIdentity) -> Result<()> {
        require(
            self.active_diff.is_none()
                && map.creation_generation > 0
                && !self.maps.contains_key(&(map.map, map.instance)),
            "execution.map_registration",
        )?;
        self.maps.insert((map.map, map.instance), map);
        Ok(())
    }
    pub fn remove(&mut self, map: MapIdentity) -> Result<()> {
        require(
            self.maps.get(&(map.map, map.instance)) == Some(&map),
            "execution.map_generation",
        )?;
        self.maps.remove(&(map.map, map.instance));
        Ok(())
    }
    // Accumulate once and visit std::map key order, not completion order; MapManager.cpp:196-205, MapManager.h:40-45.
    pub fn begin(&mut self, diff: u32) -> Result<Option<Vec<MapJob>>> {
        require(
            self.active_diff.is_none(),
            "execution.map_update_in_progress",
        )?;
        self.timer
            .update(i64::from(diff))
            .map_err(|_| refuse("execution.map_timer_overflow"))?;
        if !self.timer.passed() {
            return Ok(None);
        }
        let diff = self.timer.get_current() as u32;
        self.active_diff = Some(diff);
        let mut jobs = Vec::with_capacity(self.maps.len());
        for map in self.maps.values() {
            let id = self.next_job;
            self.next_job = self
                .next_job
                .checked_add(1)
                .ok_or_else(|| refuse("execution.job_id_exhausted"))?;
            jobs.push(MapJob {
                id,
                map: *map,
                diff,
            });
        }
        Ok(Some(jobs))
    }
    // Clear only after worker wait and unload checks, discarding excess; MapManager.cpp:208-226.
    pub fn finish(&mut self) -> Result<()> {
        require(
            self.active_diff.is_some(),
            "execution.map_finish_without_begin",
        )?;
        self.timer.set_current(0);
        self.active_diff = None;
        Ok(())
    }
    pub fn current(&self) -> i64 {
        self.timer.get_current()
    }
}

/// Phase handlers execute managers, including timer-conditional branches, independently.
pub trait WorldWork {
    fn phase(&mut self, phase: WorldPhase, thread: u64, diff: u32) -> Result<()>;
    fn execute_world_messages(&mut self, thread: u64, messages: &mut SourceMessager) -> Result<()>;
    fn run_map_jobs(
        &mut self,
        thread: u64,
        jobs: &[MapJob],
        scheduler: &mut SourceScheduler,
    ) -> Result<()>;
    fn run_map_inline(&mut self, thread: u64, job: MapJob) -> Result<()>;
    fn remove_old_corpses(&mut self, _thread: u64) -> Result<()> {
        Err(refuse("execution.corpse.world_sweep_owner"))
    }
    fn update_game_events(
        &mut self,
        _thread: u64,
        timer: &mut IntervalTimer,
        _messages: &mut SourceMessager,
    ) -> Result<()> {
        require(
            !timer.passed(),
            "execution.world.World_1666.game_event_update_owner",
        )
    }
    fn unload_maps(&mut self, diff: u32, maps: &mut SourceMapManager) -> Result<()>;
}
pub struct SourceWorld {
    pub maps: SourceMapManager,
    pub scheduler: SourceScheduler,
    /// World.h:595,717 owns the world queue for event work and OutdoorPvP.
    pub messages: SourceMessager,
    thread: u64,
    corpse_timer: IntervalTimer,
    pub(super) event_timer: IntervalTimer,
    saved_variables: startup::SourceSavedVariables,
    honor: startup::SourceHonor,
    spam_records: Vec<Vec<u8>>,
    world_state: world_state::SourceWorldState,
    stopped: bool,
}
impl SourceWorld {
    pub fn new(thread: u64, map_interval: u32) -> Result<Self> {
        require(thread > 0, "execution.world_thread")?;
        let mut corpse_timer = IntervalTimer::default();
        corpse_timer.set_interval(1_200_000);
        Ok(Self {
            maps: SourceMapManager::new(map_interval),
            scheduler: SourceScheduler::default(),
            messages: SourceMessager::new(MessageOwner::World),
            thread,
            corpse_timer,
            event_timer: IntervalTimer::default(),
            saved_variables: startup::SourceSavedVariables::default(),
            honor: startup::SourceHonor::default(),
            spam_records: Vec::new(),
            world_state: world_state::SourceWorldState::default(),
            stopped: false,
        })
    }
    pub fn corpse_timer_current(&self) -> i64 {
        self.corpse_timer.get_current()
    }

    pub fn event_interval(&self) -> i64 {
        self.event_timer.get_interval()
    }

    pub fn next_weekly_quest_reset(&self) -> i64 {
        self.saved_variables.next_weekly_reset()
    }

    pub(super) fn init_weekly_quest_reset_time(
        &mut self,
        config: WeeklyQuestResetConfig,
        work: &mut impl WorldStartupWork,
    ) -> Result<()> {
        self.saved_variables.init_weekly_reset(config, work)
    }

    pub fn next_maintenance_date(&self) -> u32 {
        self.saved_variables.next_maintenance
    }

    pub(super) fn init_server_maintenance_check(
        &mut self,
        calendar: Calendar,
        weekday: i32,
        work: &mut impl WorldStartupWork,
    ) -> Result<()> {
        self.saved_variables.init_maintenance(
            calendar.world_seconds()?,
            weekday,
            &mut self.honor,
            work,
        )
    }

    pub fn honor_standings(&self) -> (&[SourceHonorStanding], &[SourceHonorStanding]) {
        (&self.honor.alliance, &self.honor.horde)
    }

    pub(super) fn load_honor_standings(
        &mut self,
        calendar: Calendar,
        weekday: i32,
        work: &mut impl WorldStartupWork,
    ) -> Result<()> {
        self.honor
            .load_standings(calendar.world_seconds()?, weekday, work)
    }

    pub fn spam_records(&self) -> &[Vec<u8>] {
        &self.spam_records
    }

    // World.cpp:2307-2324 replaces the list on reload only when the query returns rows.
    pub fn load_spam_records(
        &mut self,
        reload: bool,
        mut rows: Vec<Vec<u8>>,
        work: &mut impl WorldStartupWork,
    ) -> Result<()> {
        let sql = WorldStartupSql::SpamRecords(rows.len());
        work.observe(WorldStartupObservation::Sql(sql))?;
        if rows.is_empty() {
            return Ok(());
        }
        if reload {
            self.spam_records.clear();
        }
        // Field.h:49-52 reads each value as a C string.
        for record in &mut rows {
            if let Some(end) = record.iter().position(|&byte| byte == 0) {
                record.truncate(end);
            }
        }
        if self.spam_records.is_empty() {
            self.spam_records = rows;
        } else {
            self.spam_records.append(&mut rows);
        }
        Ok(())
    }

    // A failed owner ends the accepted prefix before later phases; World.cpp:1542-1702.
    pub fn update<W: WorldWork>(&mut self, diff: u32, work: &mut W) -> Result<()> {
        require(!self.stopped, "execution.world_stopped")?;
        let result = self.update_inner(diff, work);
        self.stopped |= result.is_err();
        result
    }
    fn update_inner<W: WorldWork>(&mut self, diff: u32, work: &mut W) -> Result<()> {
        for phase in WORLD_PHASES {
            work.phase(phase, self.thread, diff)?;
            // World.cpp:1551-1554 drains the world queue after updating the game time.
            if phase == WorldPhase::Messages {
                work.execute_world_messages(self.thread, &mut self.messages)?;
            }
            // Advance all timers before the conditional sweep resets one interval; World.cpp:1543-1549,1655-1659.
            if phase == WorldPhase::Timers {
                self.corpse_timer
                    .update(i64::from(diff))
                    .map_err(|_| refuse("execution.corpse.world_timer_overflow"))?;
                // World.cpp:1543-1549 advances WUPDATE_EVENTS with the other timers.
                if self.event_timer.get_current() < 0 {
                    self.event_timer.set_current(0);
                } else {
                    self.event_timer
                        .update(i64::from(diff))
                        .map_err(|_| refuse("execution.event.timer_overflow"))?;
                }
            }
            if phase == WorldPhase::Corpses && self.corpse_timer.passed() {
                self.corpse_timer
                    .reset()
                    .map_err(|_| refuse("execution.corpse.world_timer_overflow"))?;
                work.remove_old_corpses(self.thread)?;
            }
            // World.cpp:1663-1668 lets the game event owner use the world timer.
            if phase == WorldPhase::GameEvents {
                work.update_game_events(self.thread, &mut self.event_timer, &mut self.messages)?;
            }
            if phase == WorldPhase::Maps {
                if let Some(jobs) = self.maps.begin(diff)? {
                    if self.scheduler.worker_count() == 0 {
                        // Without workers MapManager calls Update inline; MapManager.cpp:202-205.
                        for job in &jobs {
                            work.run_map_inline(self.thread, *job)?;
                        }
                    } else {
                        for job in &jobs {
                            self.scheduler.submit(self.thread, *job)?;
                        }
                        work.run_map_jobs(self.thread, &jobs, &mut self.scheduler)?;
                        self.scheduler.barrier(self.thread)?;
                    }
                    let active_diff = self
                        .maps
                        .active_diff
                        .ok_or_else(|| refuse("execution.map.MapManager_216.active_diff_owner"))?;
                    work.unload_maps(active_diff, &mut self.maps)?;
                    self.maps.finish()?;
                }
            }
        }
        Ok(())
    }
}

/// Map.cpp:702-729 prelude only. Object visits, grid teardown and later phases have other owners.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapPreludePhase {
    Calendar,
    CollisionTree,
    Messages,
    Transports,
}
pub trait SourceMapPreludeWork: SpawnWork {
    fn map_prelude_phase(
        &mut self,
        phase: MapPreludePhase,
        manager: &mut SourceSpawnManager,
        persistent: &mut SourceManagers,
        diff: u32,
    ) -> Result<()>;
}
pub fn update_source_map_prelude(
    manager: &mut SourceSpawnManager,
    persistent: &mut SourceManagers,
    work: &mut impl SourceMapPreludeWork,
    diff: u32,
) -> Result<()> {
    for phase in [
        MapPreludePhase::Calendar,
        MapPreludePhase::CollisionTree,
        MapPreludePhase::Messages,
    ] {
        work.map_prelude_phase(phase, manager, persistent, diff)?;
    }
    manager.update(persistent, work)?;
    work.map_prelude_phase(MapPreludePhase::Transports, manager, persistent, diff)
}

/// Execute the real conditional owners before weather; Map.cpp:1006-1030.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapEndPhase {
    ObjectFields,
    GridStates,
    Scripts,
    Instance,
}
pub trait SourceMapEndWork: WeatherPacketOwner {
    fn map_end_phase(
        &mut self,
        phase: MapEndPhase,
        map: MapIdentity,
        diff: u32,
        objects: &mut SourceObjects,
    ) -> Result<()>;
}
pub fn update_source_map_end(
    weather: &mut SourceMapWeather,
    objects: &mut SourceObjects,
    context: &mut impl benilla_sim::source_weather::Context<Error = Refusal>,
    owner: &mut impl SourceMapEndWork,
    diff: u32,
) -> Result<()> {
    require(!weather.stopped, "execution.weather.stopped")?;
    for phase in [
        MapEndPhase::ObjectFields,
        MapEndPhase::GridStates,
        MapEndPhase::Scripts,
        MapEndPhase::Instance,
    ] {
        if let Err(error) = owner.map_end_phase(phase, weather.map(), diff, objects) {
            weather.stopped = true;
            return Err(error);
        }
    }
    weather.update(diff, objects, context, owner)
}
