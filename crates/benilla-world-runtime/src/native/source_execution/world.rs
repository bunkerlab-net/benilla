// SPDX-License-Identifier: GPL-3.0-or-later
//! Component execution only. Unsupported managers must refuse, never replay results.
use super::*;
use benilla_sim::source_time::IntervalTimer;
use std::collections::BTreeMap;

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
    fn run_map_jobs(
        &mut self,
        thread: u64,
        jobs: &[MapJob],
        scheduler: &mut SourceScheduler,
    ) -> Result<()>;
    fn run_map_inline(&mut self, thread: u64, job: MapJob) -> Result<()>;
    fn unload_maps(&mut self, diff: u32, maps: &mut SourceMapManager) -> Result<()>;
}
pub struct SourceWorld {
    pub maps: SourceMapManager,
    pub scheduler: SourceScheduler,
    thread: u64,
    stopped: bool,
}
impl SourceWorld {
    pub fn new(thread: u64, map_interval: u32) -> Result<Self> {
        require(thread > 0, "execution.world_thread")?;
        Ok(Self {
            maps: SourceMapManager::new(map_interval),
            scheduler: SourceScheduler::default(),
            thread,
            stopped: false,
        })
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
                    work.unload_maps(self.maps.active_diff.unwrap(), &mut self.maps)?;
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
