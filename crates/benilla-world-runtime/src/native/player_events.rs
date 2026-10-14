// SPDX-License-Identifier: GPL-3.0-or-later
//! Per-player source Unit event ownership. Queue time is not world/map time.
use super::*;
use benilla_sim::event_processor::{EventKind, EventProcessor, EventProcessorSnapshot, QueueEntry};
use serde::{Deserialize, Serialize};

pub const PLAYER_EVENT_POLICY: &str = "cmangos-8ec338a1-native-player-events-v1";
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum FinishedSpellReason {
    PrepareFailed,
    Interrupted,
    UpdateFinished,
    DelayedFinished,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellEventLifetime {
    pub event_id: u64,
    pub created_ms: u64,
    pub last_callback_ms: Option<u64>,
    /// Erased source key on the callback that performed this event's last re-add.
    pub last_dispatch: Option<QueueEntry>,
    pub delay_start_ms: Option<u64>,
    pub travel_deadline_ms: Option<u64>,
    /// Finished spell objects remain owned by their event until source deletion.
    pub finished: Option<(u64, FinishedSpellReason)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerEvents {
    pub(super) world: crate::WorldId,
    pub(super) player: crate::PlayerId,
    pub(super) queue: EventProcessor,
    pub(super) notify: Option<u64>,
    pub(super) current_spell: Option<u64>,
    pub(super) last_current_spell: Option<u64>,
    pub(super) spells: BTreeMap<u64, SpellEventLifetime>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlayerEventsSnapshot {
    policy: String,
    world: crate::WorldId,
    player: crate::PlayerId,
    queue: EventProcessorSnapshot,
    notify: Option<u64>,
    current_spell: Option<u64>,
    last_current_spell: Option<u64>,
    spells: BTreeMap<u64, SpellEventLifetime>,
}
impl PlayerEvents {
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn new(world: crate::WorldId, player: crate::PlayerId) -> Self {
        Self {
            world,
            player,
            queue: EventProcessor::new(),
            notify: None,
            current_spell: None,
            last_current_spell: None,
            spells: BTreeMap::new(),
        }
    }
    pub fn clock_ms(&self) -> u64 {
        self.queue.clock_ms()
    }
    pub fn queue_snapshot(&self) -> EventProcessorSnapshot {
        self.queue.snapshot()
    }
    pub fn spell_lifetime(&self, cast_id: u64) -> Option<&SpellEventLifetime> {
        self.spells.get(&cast_id)
    }
    pub fn current_spell(&self) -> Option<u64> {
        self.current_spell
    }
    pub(super) fn schedule_notify(
        &mut self,
        delay_ms: u32,
        forced: bool,
    ) -> Result<(), NativeError> {
        if self.notify.is_some() && !forced {
            return Ok(());
        }
        if let Some(id) = self.notify.take() {
            self.queue.kill_event(id);
        }
        let id = self.queue.next_event_id();
        let due = self.queue.calculate_time(u64::from(delay_ms))?;
        self.queue.add_event(id, EventKind::AiNotify, due)?;
        self.notify = Some(id);
        Ok(())
    }
    pub(super) fn prepare(&mut self, cast_id: u64, created_ms: u64) -> Result<(), NativeError> {
        if self.spells.len() >= MAX_CASTS + MAX_INPUTS_PER_TICK
            || self.spells.contains_key(&cast_id)
        {
            return Err(NativeError::Limit);
        }
        let event_id = self.queue.next_event_id();
        self.queue.add_event(
            event_id,
            EventKind::SpellUpdate { cast_id },
            self.queue.calculate_time(1)?,
        )?;
        self.spells.insert(
            cast_id,
            SpellEventLifetime {
                event_id,
                created_ms,
                last_callback_ms: None,
                last_dispatch: None,
                delay_start_ms: None,
                travel_deadline_ms: None,
                finished: None,
            },
        );
        Ok(())
    }
    pub(super) fn finish(
        &mut self,
        cast_id: u64,
        now_ms: u64,
        reason: FinishedSpellReason,
    ) -> Result<(), NativeError> {
        let lifetime = self
            .spells
            .get_mut(&cast_id)
            .ok_or(NativeError::InvalidState)?;
        lifetime.finished = Some((now_ms, reason));
        Ok(())
    }
    pub(super) fn snapshot(&self) -> PlayerEventsSnapshot {
        PlayerEventsSnapshot {
            policy: PLAYER_EVENT_POLICY.into(),
            world: self.world,
            player: self.player,
            queue: self.queue.snapshot(),
            notify: self.notify,
            current_spell: self.current_spell,
            last_current_spell: self.last_current_spell,
            spells: self.spells.clone(),
        }
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn restore(snapshot: PlayerEventsSnapshot) -> Result<Self, NativeError> {
        if snapshot.policy != PLAYER_EVENT_POLICY
            || snapshot.spells.len() > MAX_CASTS + MAX_INPUTS_PER_TICK
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok(Self {
            world: snapshot.world,
            player: snapshot.player,
            queue: EventProcessor::restore(snapshot.queue)?,
            notify: snapshot.notify,
            current_spell: snapshot.current_spell,
            last_current_spell: snapshot.last_current_spell,
            spells: snapshot.spells,
        })
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn validate(&self, state: &State, profile: &Profile) -> Result<(), NativeError> {
        let invalid = || NativeError::InvalidCheckpoint;
        let snapshot = self.queue.snapshot();
        let mut chronology = Vec::new();
        let mut predecessors = snapshot
            .queued
            .iter()
            .map(|e| e.queue.unwrap().insertion_id)
            .collect::<std::collections::BTreeSet<_>>();
        let mut enqueue_history = Vec::new();
        if self.world != state.world
            || self.player != state.player
            || snapshot.clock_ms
                != state
                    .now_ms
                    .checked_sub(profile.clock_origin_ms)
                    .ok_or_else(invalid)?
            || snapshot.next_event_id
                > state
                    .next_sequence
                    .checked_add(1 + u64::from(profile.source_motion.is_some()))
                    .ok_or_else(invalid)?
            || snapshot.next_insertion_id
                > snapshot.next_event_id.saturating_add(
                    state.tick.saturating_mul(
                        benilla_sim::event_processor::MAX_CALLBACKS_PER_UPDATE as u64,
                    ),
                )
            || snapshot.next_event_id < state.next_cast_id
            || (profile.source_motion.is_some() && snapshot.next_event_id <= state.next_cast_id)
            || (profile.source_motion.is_none() && snapshot.next_event_id != state.next_cast_id)
            || snapshot.next_insertion_id < snapshot.next_event_id
            || snapshot.aborting
            || !snapshot.detached.is_empty()
            || snapshot.queued.len() != self.spells.len() + usize::from(self.notify.is_some())
            || self
                .last_current_spell
                .is_some_and(|id| id == 0 || id >= state.next_cast_id)
            || self.current_spell
                != self
                    .last_current_spell
                    .filter(|id| state.casts.contains_key(id))
            || state
                .casts
                .keys()
                .any(|id| self.last_current_spell.is_none_or(|last| *id > last))
            || state.casts.keys().any(|id| !self.spells.contains_key(id))
        {
            return Err(invalid());
        }
        if state.tick == 0 {
            let mut expected = Self::new(state.world, state.player);
            if profile.source_motion.is_some() {
                expected.schedule_notify(0, false)?;
            }
            if self != &expected {
                return Err(invalid());
            }
        }
        if let Some(id) = self.notify {
            let e = self.queue.event(id).ok_or_else(invalid)?;
            let policy = profile.source_motion.as_ref().ok_or_else(invalid)?;
            if e.kind != EventKind::AiNotify
                || e.to_abort
                || e.queue.is_none()
                || !e.add_time_ms.is_multiple_of(u64::from(profile.tick_ms))
                || (state.tick > 0 && (e.add_time_ms == 0 || e.event_id == 1))
                || e.exec_time_ms < snapshot.clock_ms
                || (state.tick > 0 && e.exec_time_ms <= snapshot.clock_ms)
                || (state.tick > 0
                    && e.exec_time_ms
                        != e.add_time_ms
                            .checked_add(u64::from(policy.relocation_aggro_delay_ms))
                            .ok_or_else(invalid)?)
                || e.exec_time_ms - snapshot.clock_ms > u64::from(policy.relocation_aggro_delay_ms)
                || (state.tick == 0 && e.exec_time_ms != 0)
            {
                return Err(invalid());
            }
            chronology.push((e.queue.unwrap().insertion_id, (e.add_time_ms, 1u8, 0, 0)));
            enqueue_history.push((e.queue.unwrap().insertion_id, (e.add_time_ms, 1u8)));
        } else if state.tick == 0 && profile.source_motion.is_some() {
            return Err(invalid());
        }
        for (&id, lifetime) in &self.spells {
            let e = self.queue.event(lifetime.event_id).ok_or_else(invalid)?;
            if id == 0
                || id >= state.next_cast_id
                || lifetime.event_id < id
                || (profile.source_motion.is_some() && lifetime.event_id <= id)
                || (profile.source_motion.is_none() && lifetime.event_id != id)
                || e.kind != (EventKind::SpellUpdate { cast_id: id })
                || e.to_abort
                || e.queue.is_none()
                || e.exec_time_ms <= snapshot.clock_ms
                || lifetime.created_ms < profile.clock_origin_ms
                || lifetime.created_ms > state.now_ms
                || !(lifetime.created_ms - profile.clock_origin_ms)
                    .is_multiple_of(u64::from(profile.tick_ms))
                || lifetime
                    .created_ms
                    .checked_sub(profile.clock_origin_ms)
                    .and_then(|t| t.checked_sub(u64::from(profile.tick_ms)))
                    != Some(e.add_time_ms)
            {
                return Err(invalid());
            }
            let callback_clock = lifetime
                .last_callback_ms
                .and_then(|t| t.checked_sub(profile.clock_origin_ms))
                .ok_or_else(invalid)?;
            let previous = lifetime.last_dispatch.ok_or_else(invalid)?;
            let delayed_callback = lifetime.delay_start_ms.is_some()
                && lifetime.finished.is_some_and(|(_, reason)| {
                    matches!(
                        reason,
                        FinishedSpellReason::DelayedFinished | FinishedSpellReason::UpdateFinished
                    )
                });
            let previous_due = if delayed_callback {
                lifetime.travel_deadline_ms.ok_or_else(invalid)?
            } else {
                callback_clock
                    .checked_sub(u64::from(profile.tick_ms))
                    .and_then(|t| t.checked_add(1))
                    .ok_or_else(invalid)?
            };
            if callback_clock > snapshot.clock_ms
                || !callback_clock.is_multiple_of(u64::from(profile.tick_ms))
                || previous.deadline_ms != previous_due
                || previous.deadline_ms > callback_clock
                || previous.insertion_id < lifetime.event_id
                || previous.insertion_id >= e.queue.unwrap().insertion_id
                || !predecessors.insert(previous.insertion_id)
            {
                return Err(invalid());
            }
            chronology.push((
                e.queue.unwrap().insertion_id,
                (
                    callback_clock,
                    0u8,
                    previous.deadline_ms,
                    previous.insertion_id,
                ),
            ));
            let predecessor_moment = if delayed_callback {
                (lifetime.delay_start_ms.ok_or_else(invalid)?, 0u8)
            } else {
                (
                    callback_clock
                        .checked_sub(u64::from(profile.tick_ms))
                        .ok_or_else(invalid)?,
                    u8::from(lifetime.last_callback_ms == Some(lifetime.created_ms)),
                )
            };
            enqueue_history.push((previous.insertion_id, predecessor_moment));
            enqueue_history.push((e.queue.unwrap().insertion_id, (callback_clock, 0u8)));
            match (state.casts.get(&id), lifetime.finished) {
                (Some(cast), None) => {
                    if lifetime.created_ms != cast.execution.created_ms()
                        || lifetime.last_callback_ms != Some(cast.execution.last_update_ms())
                    {
                        return Err(invalid());
                    }
                    match cast.execution.state() {
                        spell::CastState::Casting { .. } => {
                            if self.current_spell != Some(id)
                                || lifetime.delay_start_ms.is_some()
                                || lifetime.travel_deadline_ms.is_some()
                                || lifetime.last_callback_ms != Some(state.now_ms)
                                || e.exec_time_ms
                                    != snapshot.clock_ms.checked_add(1).ok_or_else(invalid)?
                            {
                                return Err(invalid());
                            }
                        }
                        spell::CastState::Traveling { impact_at_ms } => {
                            let released = cast
                                .release_binding
                                .as_ref()
                                .ok_or_else(invalid)?
                                .captured_ms;
                            if lifetime.delay_start_ms
                                != released.checked_sub(profile.clock_origin_ms)
                                || lifetime.last_callback_ms != Some(released)
                                || lifetime.travel_deadline_ms != Some(e.exec_time_ms)
                                || e.exec_time_ms
                                    != impact_at_ms
                                        .checked_sub(profile.clock_origin_ms)
                                        .ok_or_else(invalid)?
                            {
                                return Err(invalid());
                            }
                        }
                        _ => return Err(invalid()),
                    }
                }
                (None, Some((finished_ms, reason))) => {
                    let legacy_flight_cancel = reason == FinishedSpellReason::Interrupted
                        && lifetime.delay_start_ms.is_some();
                    if legacy_flight_cancel {
                        // Preserve the explicitly synthetic no-death-host fallback; its
                        // canceled flight retains its original queued deadline until due.
                        if profile.source_motion.is_some()
                            || profile.combat.as_ref().is_some_and(|g| g.death.is_some())
                            || lifetime.travel_deadline_ms != Some(e.exec_time_ms)
                            || finished_ms > state.now_ms
                            || lifetime
                                .last_callback_ms
                                .is_none_or(|at| at > finished_ms || at < lifetime.created_ms)
                            || lifetime.last_callback_ms
                                != lifetime
                                    .delay_start_ms
                                    .and_then(|start| start.checked_add(profile.clock_origin_ms))
                            || finished_ms < lifetime.created_ms
                            || !(finished_ms - profile.clock_origin_ms)
                                .is_multiple_of(u64::from(profile.tick_ms))
                        {
                            return Err(invalid());
                        }
                    } else if finished_ms != state.now_ms
                        || e.exec_time_ms != snapshot.clock_ms.checked_add(1).ok_or_else(invalid)?
                        || lifetime.last_callback_ms != Some(state.now_ms)
                    {
                        return Err(invalid());
                    }
                    if self.current_spell == Some(id)
                        || reason == FinishedSpellReason::PrepareFailed
                        || (reason == FinishedSpellReason::DelayedFinished
                            && lifetime.delay_start_ms.is_none())
                        || lifetime.delay_start_ms.is_some()
                            != lifetime.travel_deadline_ms.is_some()
                        || lifetime.delay_start_ms.is_some_and(|start| {
                            start == 0
                                || start > snapshot.clock_ms
                                || start < lifetime.created_ms - profile.clock_origin_ms
                                || !start.is_multiple_of(u64::from(profile.tick_ms))
                        })
                        || lifetime.travel_deadline_ms.is_some_and(|at| {
                            lifetime.delay_start_ms.is_none_or(|start| at < start)
                                || (!legacy_flight_cancel && at > snapshot.clock_ms)
                        })
                    {
                        return Err(invalid());
                    }
                }
                _ => return Err(invalid()),
            }
        }
        // Source input at queue clock k follows that Unit's completed callback
        // phase at k. Within one callback phase erased keys order re-additions.
        enqueue_history.sort_by_key(|entry| entry.0);
        if enqueue_history.windows(2).any(|pair| pair[0].1 > pair[1].1) {
            return Err(invalid());
        }
        chronology.sort_by_key(|entry| entry.0);
        if chronology.windows(2).any(|pair| pair[0].1 > pair[1].1) {
            return Err(invalid());
        }
        Ok(())
    }
}

impl NativeWorld {
    /// Explicit permanent Unit disposal, distinct from a durable session suspend.
    /// Unit::~Unit clears current references before EventProcessor::~EventProcessor
    /// forces Abort/destruction. Consuming the world prevents resuming torn-down state.
    pub fn teardown(self) -> Result<Vec<Event>, NativeError> {
        let mut state = self.state;
        state.player_events.current_spell = None;
        let mut queue = std::mem::take(&mut state.player_events.queue);
        let mut transaction = queue.begin_kill_all_events(true);
        let mut events = Vec::new();
        while let Some(due) = transaction.pop_due()? {
            match due.kind {
                EventKind::AiNotify => {
                    if state.player_events.notify != Some(due.event_id) {
                        return Err(NativeError::InvalidState);
                    }
                    state.player_events.notify = None;
                }
                EventKind::SpellUpdate { cast_id } => {
                    let lifetime = state
                        .player_events
                        .spells
                        .remove(&cast_id)
                        .ok_or(NativeError::InvalidState)?;
                    if lifetime.event_id != due.event_id {
                        return Err(NativeError::InvalidState);
                    }
                    if let Some(mut cast) = state.casts.remove(&cast_id) {
                        for event in cast
                            .execution
                            .cancel(&mut state.resources.caster, spell::CastFailure::Interrupted)
                        {
                            events.push(Event::Spell {
                                cast_id,
                                ability: cast.ability,
                                target: cast.target,
                                event,
                            });
                        }
                    } else if lifetime.finished.is_none() {
                        return Err(NativeError::InvalidState);
                    }
                    // Abort finished the spell; its destructor cannot cancel twice.
                }
            }
            transaction.finish_abort(true)?;
        }
        transaction.commit()?;
        if !state.casts.is_empty() || !state.player_events.spells.is_empty() {
            return Err(NativeError::InvalidState);
        }
        Ok(events)
    }
}
