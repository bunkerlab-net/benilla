// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Called by WorldWork, never an alternative world scheduler or timer loop.
use super::*;
use benilla_sim::source_time::IntervalTimer;

pub struct WorldManagerContext<'a> {
    pub thread: u64,
    pub messages: &'a mut SourceMessager,
    pub events: &'a mut SourceGameEventManager,
    pub event_timer: &'a mut IntervalTimer,
    pub announce: bool,
    pub corpses: &'a mut SourceCorpseRegistry,
    pub objects: &'a mut SourceObjects,
    pub conversion: CorpseConversion<'a>,
}
impl WorldManagerContext<'_> {
    /// Call exactly once from each corresponding SourceWorld phase.
    /// World.cpp:1543-1554 advances before messages; :1663-1668 updates events
    /// only after map join/unload, SQL callbacks and the corpse sweep.
    pub fn phase(
        &mut self,
        phase: WorldPhase,
        diff: u32,
        owner: &mut (impl MessageWork + GameEventOwner),
    ) -> Result<()> {
        require(
            self.thread > 0 && self.messages.owner() == MessageOwner::World,
            "execution.managers.world_binding",
        )?;
        match phase {
            WorldPhase::Timers => {
                if self.event_timer.get_current() < 0 {
                    self.event_timer.set_current(0);
                } else {
                    self.event_timer
                        .update(i64::from(diff))
                        .map_err(|_| refuse("execution.event.timer_overflow"))?;
                }
            }
            WorldPhase::Messages => {
                self.messages
                    .execute(MessageOwner::World, self.thread, owner)?;
            }
            WorldPhase::GameEvents => {
                self.events
                    .update_world_timer(self.event_timer, self.announce, owner)?;
            }
            _ => {}
        }
        Ok(())
    }
    /// SourceWorld owns the 20-minute conditional and Reset, World.cpp:1655-1659.
    /// Call from WorldWork::remove_old_corpses, not WorldWork::phase(Corpses).
    pub fn sweep(&mut self, owner: &mut impl CorpseConversionWork) -> Result<()> {
        require(
            self.thread == self.conversion.thread,
            "execution.managers.corpse_thread_binding",
        )?;
        self.corpses
            .remove_old_corpses(self.objects, &mut self.conversion, owner)
    }
}
