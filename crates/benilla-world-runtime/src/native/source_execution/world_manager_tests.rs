// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored world-manager composition, not sealed F1 chronology.
use super::*;
use benilla_sim::source_time::IntervalTimer;
use std::collections::BTreeMap;

struct EmptyEvents;
impl GameEventRows for EmptyEvents {
    fn maximum(&mut self) -> Result<Option<u16>> {
        Ok(None)
    }
    fn definition(&mut self) -> Result<Option<GameEventDefinitionRow>> {
        unreachable!()
    }
    fn date(&mut self) -> Result<Option<GameEventDateRow>> {
        unreachable!()
    }
    fn member(&mut self, _: EventSpawnKind) -> Result<Option<GameEventMemberRow>> {
        unreachable!()
    }
    fn substitution(&mut self) -> Result<Option<GameEventSubstitutionRow>> {
        unreachable!()
    }
    fn quest(&mut self) -> Result<Option<(u64, u32, u16)>> {
        unreachable!()
    }
    fn mail(&mut self) -> Result<Option<GameEventMailRow>> {
        unreachable!()
    }
}
#[derive(Default)]
struct Owner {
    calls: Vec<&'static str>,
    fail_sweep: bool,
}
impl MessageWork for Owner {
    fn begin_message(&mut self, _: &MessageEnvelope) -> Result<()> {
        self.calls.push("message");
        Ok(())
    }
    fn forward_game_event(&mut self, _: u64, _: &[EventMapGuids], _: bool) -> Result<()> {
        self.calls.push("forward");
        Ok(())
    }
    fn map_game_event(
        &mut self,
        _: u64,
        _: MapIdentity,
        _: &[u32],
        _: &[u32],
        _: bool,
    ) -> Result<()> {
        unreachable!()
    }
    fn creature_present(&self, _: MapIdentity, _: u64) -> bool {
        unreachable!()
    }
    fn update_creature_event(
        &mut self,
        _: u64,
        _: MapIdentity,
        _: EventCreatureUpdate,
    ) -> Result<()> {
        unreachable!()
    }
}
impl GameEventOwner for Owner {
    fn clock(&mut self, site: EventClockSite) -> Result<ClockSample> {
        assert_eq!(site, EventClockSite::Update);
        self.calls.push("events");
        Ok(ClockSample::authored(ClockKind::WallSeconds, 100))
    }
    fn reference(&mut self, _: EventReference, _: u32) -> Result<bool> {
        unreachable!()
    }
    fn set_quest_active(&mut self, _: u32, _: bool) -> Result<()> {
        unreachable!()
    }
    fn check_pool_event(
        &mut self,
        _: &SourcePoolManager,
        _: u16,
        _: i16,
        _: &BTreeMap<u32, i16>,
        _: &BTreeMap<u32, i16>,
    ) -> Result<()> {
        unreachable!()
    }
    fn effect(&mut self, _: &mut SourceGameEventManager, _: GameEventEffect) -> Result<()> {
        unreachable!()
    }
    fn observe(&mut self, _: GameEventObservation) -> Result<()> {
        Ok(())
    }
}
impl CorpseWork for Owner {
    fn constructor_clock(&mut self, _: ObjectHandle, _: &mut SourceObjects) -> Result<ClockSample> {
        unreachable!()
    }
    fn observe(&mut self, _: CorpseEvent) -> Result<()> {
        unreachable!()
    }
    fn query_corpses(&mut self) -> Result<CharacterQueryStatus> {
        unreachable!()
    }
}
impl CorpseConversionWork for Owner {
    fn sweep_clock(&mut self) -> Result<ClockSample> {
        self.calls.push("sweep");
        if self.fail_sweep {
            return Err(refuse("authored.sweep_clock"));
        }
        Ok(ClockSample::authored(ClockKind::WallSeconds, 100))
    }
    fn is_battleground(&mut self, _: MapIdentity) -> Result<bool> {
        unreachable!()
    }
    fn map_phase(
        &mut self,
        _: ObjectHandle,
        _: CorpseMapPhase,
        _: &mut SourceObjects,
        _: &mut SourceCells,
    ) -> Result<()> {
        unreachable!()
    }
}
struct Harness {
    owner: Owner,
    messages: SourceMessager,
    events: SourceGameEventManager,
    timer: IntervalTimer,
    corpses: SourceCorpseRegistry,
    objects: SourceObjects,
    maps: BTreeMap<MapIdentity, SourceCells>,
}
impl Harness {
    fn new() -> Self {
        let mut owner = Owner::default();
        let mut events = SourceGameEventManager::default();
        events
            .load(
                &mut EmptyEvents,
                &mut SourcePoolManager::default(),
                &mut owner,
            )
            .unwrap();
        let interval = events.initialize(None, false, &mut owner).unwrap();
        let mut timer = IntervalTimer::default();
        timer.set_interval(i64::from(interval));
        let mut messages = SourceMessager::new(MessageOwner::World);
        messages
            .enqueue(MessageEnvelope {
                id: 1,
                cause: 1,
                payload: SourceMessage::ForwardGameEvent {
                    maps: vec![],
                    spawn: true,
                },
            })
            .unwrap();
        owner.calls.clear();
        Self {
            owner,
            events,
            timer,
            messages,
            corpses: SourceCorpseRegistry::default(),
            objects: SourceObjects::default(),
            maps: BTreeMap::new(),
        }
    }
    fn context(&mut self) -> (WorldManagerContext<'_>, &mut Owner) {
        (
            WorldManagerContext {
                thread: 1,
                messages: &mut self.messages,
                events: &mut self.events,
                event_timer: &mut self.timer,
                announce: false,
                corpses: &mut self.corpses,
                objects: &mut self.objects,
                conversion: CorpseConversion {
                    thread: 1,
                    bones_world: true,
                    bones_battleground: true,
                    maps: &mut self.maps,
                },
            },
            &mut self.owner,
        )
    }
}
impl WorldWork for Harness {
    fn phase(&mut self, phase: WorldPhase, _: u64, diff: u32) -> Result<()> {
        let (mut context, owner) = self.context();
        context.phase(phase, diff, owner)?;
        if phase == WorldPhase::DelayedRemovals {
            owner.calls.push("removals");
        }
        Ok(())
    }
    fn remove_old_corpses(&mut self, _: u64) -> Result<()> {
        let (mut context, owner) = self.context();
        context.sweep(owner)
    }
    fn run_map_jobs(&mut self, _: u64, _: &[MapJob], _: &mut SourceScheduler) -> Result<()> {
        unreachable!()
    }
    fn run_map_inline(&mut self, _: u64, job: MapJob) -> Result<()> {
        self.owner
            .calls
            .push(if job.map.map == 0 { "map0" } else { "map1" });
        Ok(())
    }
    fn unload_maps(&mut self, _: u32, _: &mut SourceMapManager) -> Result<()> {
        self.owner.calls.push("unload");
        Ok(())
    }
}
#[test]
fn world_messages_maps_join_sweep_events_and_removals_use_source_order() {
    let mut harness = Harness::new();
    // Authored timer boundary makes both independent conditionals due this update.
    harness
        .timer
        .set_current(harness.timer.get_interval() - 1_200_000);
    let mut world = SourceWorld::new(1, 100).unwrap();
    for id in [1, 0] {
        world
            .maps
            .register(MapIdentity {
                map: id,
                instance: 0,
                creation_generation: 1,
            })
            .unwrap();
    }
    world.update(1_200_000, &mut harness).unwrap();
    assert_eq!(
        harness.owner.calls,
        ["message", "forward", "map0", "map1", "unload", "sweep", "events", "removals"]
    );
    assert_eq!(
        (
            WorldPhase::Corpses.source_line(),
            WorldPhase::GameEvents.source_line(),
            WorldPhase::DelayedRemovals.source_line()
        ),
        (1655, 1663, 1681)
    );
    assert_eq!(world.maps.current(), 0);
    assert_eq!(world.corpse_timer_current(), 0);
}
#[test]
fn world_sweep_refusal_prevents_events_and_removals() {
    let mut harness = Harness::new();
    harness.owner.fail_sweep = true;
    let mut world = SourceWorld::new(1, 100).unwrap();
    assert_eq!(
        world.update(1_200_000, &mut harness).unwrap_err().id,
        "authored.sweep_clock"
    );
    assert_eq!(harness.owner.calls.last(), Some(&"sweep"));
    assert!(!harness.owner.calls.contains(&"events"));
    assert!(!harness.owner.calls.contains(&"removals"));
    assert_eq!(
        world.update(1, &mut harness).unwrap_err().id,
        "execution.world_stopped"
    );
}
#[test]
fn event_timer_advance_preserves_negative_reset_branch() {
    let mut harness = Harness::new();
    harness.timer.set_current(-10);
    let (mut context, owner) = harness.context();
    context.phase(WorldPhase::Timers, 100, owner).unwrap();
    assert_eq!(context.event_timer.get_current(), 0);
    context.phase(WorldPhase::GameEvents, 100, owner).unwrap();
    assert!(owner.calls.is_empty());
}
