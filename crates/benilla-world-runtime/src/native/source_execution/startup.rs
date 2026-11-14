// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[derive(Default)]
pub struct StartupMaps {
    launched: usize,
    completed: usize,
}
impl StartupMaps {
    // Register continent 0 then 1 before launching each initializer; MapManager.cpp:96-106.
    pub fn launch(
        &mut self,
        maps: &mut SourceMapManager,
        scheduler: &mut SourceScheduler,
        birth: ThreadBirth,
        map: MapIdentity,
    ) -> Result<Vec<ScheduleEdge>> {
        require(
            self.launched < 2 && map.map as usize == self.launched && map.instance == 0,
            "execution.startup_continent_order",
        )?;
        maps.register(map)?;
        let born = scheduler.birth(birth, ThreadRole::StartupMap(map))?;
        let dispatched = scheduler.start_async(
            birth.thread,
            MapJob {
                id: u64::MAX - self.launched as u64,
                map,
                diff: 0,
            },
        )?;
        self.launched += 1;
        Ok(vec![born, dispatched])
    }
    // Call only after Map::Initialize returns, then destroy its thread context; MapManager.cpp:106-110.
    pub fn complete(
        &mut self,
        scheduler: &mut SourceScheduler,
        thread: u64,
        job: u64,
    ) -> Result<Vec<ScheduleEdge>> {
        require(
            self.completed < self.launched && job >= u64::MAX - 1,
            "execution.startup_completion",
        )?;
        let complete = scheduler.complete(thread, job)?;
        let exit = scheduler.exit(thread)?;
        self.completed += 1;
        Ok(vec![complete, exit])
    }
    pub fn wait(&self, scheduler: &mut SourceScheduler, world_thread: u64) -> Result<ScheduleEdge> {
        require(
            self.launched == 2 && self.completed == 2,
            "execution.startup_pending",
        )?;
        scheduler.barrier(world_thread)
    }
}
