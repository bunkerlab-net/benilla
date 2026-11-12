// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Live Unit BasicEvents. This owner deliberately has no serde or restore interface.
use super::*;
use benilla_sim::event_processor::{
    CallbackKind, EventError, EventPayload, EventProcessor, EventProcessorSnapshot,
};

/// Captures data, never a closure or native address. Creature.h:1037-1045.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceUnitEvent {
    ForcedDespawn { only_alive: bool },
    Assistance,
}
impl EventPayload for SourceUnitEvent {
    fn validate(self) -> std::result::Result<(), EventError> {
        Ok(())
    }
}

/// The live callback owner must stage effects or stop execution after a refusal.
/// Spell owners extend the typed event, not a second queue.
pub trait UnitEventOwner {
    fn forced_despawn(&mut self, only_alive: bool) -> Result<()>;
    fn assistance(
        &mut self,
        _capture: &AssistanceCapture,
        _queue: &mut impl AssistanceQueue,
    ) -> Result<()> {
        Err(refuse("execution.assistance.live_dispatch_binding_owner"))
    }
}

/// Source-only queues cannot become native persisted closure/event state.
/// ```compile_fail
/// use benilla_world_runtime::native::source_execution::SourceUnitEvents;
/// serde_json::to_vec(&SourceUnitEvents::default()).unwrap();
/// ```
#[derive(Clone, Debug, Default)]
pub struct SourceUnitEvents {
    queue: EventProcessor<SourceUnitEvent>,
    stopped: bool,
    captures: std::collections::BTreeMap<u64, AssistanceCapture>,
}
impl SourceUnitEvents {
    pub fn clock_millis(&self) -> u64 {
        self.queue.clock_ms()
    }
    /// Nonzero scheduling only; delay zero is a synchronous Creature call.
    pub fn schedule_forced_despawn(&mut self, delay: u32, only_alive: bool) -> Result<u64> {
        require(!self.stopped, "execution.unit_events.stopped")?;
        require(
            delay != 0,
            "execution.unit_events.zero_delay_is_synchronous",
        )?;
        // Creature.cpp:1964-1969; EventProcessor.cpp:101-107,124-126.
        let id = self.queue.next_event_id();
        let at = self
            .queue
            .calculate_time(u64::from(delay))
            .map_err(event_refusal)?;
        self.queue
            .add_event(id, SourceUnitEvent::ForcedDespawn { only_alive }, at)
            .map_err(event_refusal)?;
        Ok(id)
    }
    pub fn cancel(&mut self, id: u64) -> Result<()> {
        require(!self.stopped, "execution.unit_events.stopped")?;
        self.queue.set_aborted(id, true).map_err(event_refusal)
    }
    /// KillEvent deletes without Abort; EventProcessor.cpp:88-98.
    pub fn kill_event(&mut self, id: u64) -> Result<bool> {
        require(!self.stopped, "execution.unit_events.stopped")?;
        let removed = self.queue.kill_event(id).is_some();
        if removed {
            self.captures.remove(&id);
        }
        Ok(removed)
    }
    /// ForcedDespawnDelayEvent inherits empty Abort and true IsDeletable,
    /// EventProcessor.h:46-48; Creature.h:1037-1045. Force skips IsDeletable.
    pub fn kill_all(&mut self, force: bool) -> Result<()> {
        require(!self.stopped, "execution.unit_events.stopped")?;
        let result = (|| {
            let mut transaction = self.queue.begin_kill_all_events(force);
            while let Some(due) = transaction.pop_due().map_err(event_refusal)? {
                self.captures.remove(&due.event_id);
                transaction.finish_abort(true).map_err(event_refusal)?;
            }
            transaction.commit().map_err(event_refusal)
        })();
        self.stopped |= result.is_err();
        result
    }
    pub fn update(&mut self, diff: u32, owner: &mut impl UnitEventOwner) -> Result<()> {
        require(!self.stopped, "execution.unit_events.stopped")?;
        let result = (|| {
            let mut transaction = self.queue.begin_update(diff).map_err(event_refusal)?;
            while let Some(due) = transaction.pop_due().map_err(event_refusal)? {
                let capture = self.captures.remove(&due.event_id);
                if due.callback == CallbackKind::Execute {
                    match due.kind {
                        SourceUnitEvent::ForcedDespawn { only_alive } => {
                            owner.forced_despawn(only_alive)?
                        }
                        SourceUnitEvent::Assistance => {
                            let capture = capture
                                .ok_or_else(|| refuse("execution.assistance.capture_missing"))?;
                            owner.assistance(
                                &capture,
                                &mut UnitEventSchedule {
                                    transaction: &mut transaction,
                                    captures: &mut self.captures,
                                },
                            )?;
                        }
                    }
                }
                match due.callback {
                    CallbackKind::Execute => transaction.finish_execute(true),
                    CallbackKind::Abort => transaction.finish_abort(true),
                }
                .map_err(event_refusal)?;
            }
            transaction.commit().map_err(event_refusal)
        })();
        // Callback effects are not rolled back by the queue's transaction.
        self.stopped |= result.is_err();
        result
    }
    /// In-memory F7 inventory only. SourceUnitEvent intentionally lacks Serialize.
    pub fn inventory(&self) -> EventProcessorSnapshot<SourceUnitEvent> {
        self.queue.snapshot()
    }
    pub fn validate(&self) -> Result<()> {
        EventProcessor::restore(self.inventory()).map_err(event_refusal)?;
        for event in self.inventory().queued {
            require(
                matches!(event.kind, SourceUnitEvent::Assistance)
                    == self.captures.contains_key(&event.event_id),
                "execution.assistance.capture_inventory",
            )?;
        }
        Ok(())
    }
    pub fn assistance_captures(&self) -> &std::collections::BTreeMap<u64, AssistanceCapture> {
        &self.captures
    }
}
impl AssistanceQueue for SourceUnitEvents {
    fn add_assistance(&mut self, delay: u32, capture: AssistanceCapture) -> Result<u64> {
        require(!self.stopped, "execution.unit_events.stopped")?;
        let id = self.queue.next_event_id();
        let at = self
            .queue
            .calculate_time(u64::from(delay))
            .map_err(event_refusal)?;
        self.queue
            .add_event(id, SourceUnitEvent::Assistance, at)
            .map_err(event_refusal)?;
        self.captures.insert(id, capture);
        Ok(id)
    }
}
struct UnitEventSchedule<'a, 'b> {
    transaction: &'a mut benilla_sim::event_processor::EventTransaction<'b, SourceUnitEvent>,
    captures: &'a mut std::collections::BTreeMap<u64, AssistanceCapture>,
}
impl AssistanceQueue for UnitEventSchedule<'_, '_> {
    fn add_assistance(&mut self, delay: u32, capture: AssistanceCapture) -> Result<u64> {
        let id = self.transaction.next_event_id();
        let at = self
            .transaction
            .calculate_time(u64::from(delay))
            .map_err(event_refusal)?;
        self.transaction
            .add_event(id, SourceUnitEvent::Assistance, at)
            .map_err(event_refusal)?;
        self.captures.insert(id, capture);
        Ok(id)
    }
}
fn event_refusal(error: EventError) -> Refusal {
    refuse(&format!("execution.unit_events.processor.{error:?}"))
}

/// Executes the captured Creature reference against its original generation.
struct CreatureEventOwner<'a, 'b, 'c, D: GridDefinitions, P: ResidencyProductOwner, F> {
    creature: &'a mut SourceCreatureLifecycle,
    context: &'a mut CreatureLifecycleContext<'b, 'c, D, P>,
    owner: &'a mut F,
}
impl<D: GridDefinitions, P: ResidencyProductOwner, F: CreatureLifecycleOwner<D, P>> UnitEventOwner
    for CreatureEventOwner<'_, '_, '_, D, P, F>
{
    fn forced_despawn(&mut self, only_alive: bool) -> Result<()> {
        // Creature.cpp:96-102: onlyAlive is checked at Execute, not scheduling.
        self.context.grid.objects.get(self.creature.object())?;
        if only_alive && self.creature.death_state != CreatureDeathState::Alive {
            return Ok(());
        }
        self.creature
            .forced_despawn_after(0, false, self.context, self.owner)
    }
    fn assistance(
        &mut self,
        capture: &AssistanceCapture,
        queue: &mut impl AssistanceQueue,
    ) -> Result<()> {
        require(
            capture.sender == self.creature.object(),
            "execution.assistance.event_owner_generation",
        )?;
        self.owner
            .lifecycle_assistance_event(capture, queue, self.creature, self.context)
    }
}
pub fn update_source_creature_events<D, P, F>(
    creature: &mut SourceCreatureLifecycle,
    diff: u32,
    context: &mut CreatureLifecycleContext<'_, '_, D, P>,
    owner: &mut F,
) -> Result<()>
where
    D: GridDefinitions,
    P: ResidencyProductOwner,
    F: CreatureLifecycleOwner<D, P>,
{
    context.grid.objects.get(creature.object())?;
    // Dependent teardown must not accidentally mutate a fresh placeholder queue.
    let mut events = std::mem::replace(
        &mut creature.unit_events,
        SourceUnitEvents {
            stopped: true,
            ..SourceUnitEvents::default()
        },
    );
    let result = events.update(
        diff,
        &mut CreatureEventOwner {
            creature,
            context,
            owner,
        },
    );
    creature.unit_events = events;
    result
}
