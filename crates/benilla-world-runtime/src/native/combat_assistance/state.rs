// SPDX-License-Identifier: GPL-3.0-or-later
//! The two UnitLambdaEvents share the physical NPC EventProcessor and lifetime history.
use super::*;
use benilla_sim::event_processor::{
    BasicEventSnapshot, CallbackKind, DueEvent, EventKind, EventProcessor, EventTransaction,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssistanceStopReason {
    Evade,
    Death,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssistanceOutcome {
    Reenabled,
    EnemyMissing,
    OwnerNotInCombat,
    EmptyContinuation { call_assistance: bool },
    Aborted,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssistancePayload {
    pub request: CombatAssistanceRequest,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub receivers: Option<CombatAssistanceFacts>,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub traversal: Option<AssistanceTraversalReceipt>,
    pub saved_already_called: bool,
    pub reenable_event: u64,
    pub linking_event: u64,
    pub reenable_insertion: u64,
    pub linking_insertion: u64,
    pub deadline_ms: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AssistanceHistory {
    Enter(Box<AssistancePayload>),
    CombatStop {
        npc: crate::NpcId,
        tick: u64,
        clock_ms: u64,
        reason: AssistanceStopReason,
    },
    Callback {
        npc: crate::NpcId,
        tick: u64,
        due: DueEvent,
        #[serde(deserialize_with = "creature_collision_width::required_option")]
        enemy: Option<crate::PlayerId>,
        in_combat: bool,
        outcome: AssistanceOutcome,
    },
}
/// Independent Unit-owner anchor prevents deleting completed assistance history.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::native) struct CombatAssistanceAnchor {
    count: u64,
    tip: crate::Digest,
}
impl CombatAssistanceAnchor {
    fn append(&mut self, history: &AssistanceHistory) -> Result<(), NativeError> {
        self.count = self.count.checked_add(1).ok_or(NativeError::Limit)?;
        self.tip = crate::digest(
            b"benilla-native-assistance-chain-v47\0",
            &(self.tip, self.count, history),
        );
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingAssistance {
    payload: AssistancePayload,
    reenabled: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatAssistanceState {
    source: crate::Digest,
    can_check_for_help: bool,
    already_called: bool,
    next_occurrence: u64,
    pending: BTreeMap<u64, PendingAssistance>,
    history: Vec<AssistanceHistory>,
}
impl CombatAssistanceState {
    pub(in crate::native) fn new(source: crate::Digest) -> Self {
        Self {
            source,
            can_check_for_help: true,
            already_called: false,
            next_occurrence: 1,
            pending: BTreeMap::new(),
            history: vec![],
        }
    }
    pub(in crate::native) fn source_identity(&self) -> crate::Digest {
        self.source
    }
    pub fn can_check_for_help(&self) -> bool {
        self.can_check_for_help
    }
    pub fn already_called(&self) -> bool {
        self.already_called
    }
    pub fn next_occurrence(&self) -> u64 {
        self.next_occurrence
    }
    pub fn history(&self) -> &[AssistanceHistory] {
        &self.history
    }
    pub fn pending(&self) -> impl Iterator<Item = &AssistancePayload> {
        self.pending.values().map(|p| &p.payload)
    }
    pub(in crate::native) fn pending_events(&self) -> usize {
        self.pending
            .values()
            .map(|p| if p.reenabled { 1 } else { 2 })
            .sum()
    }
    pub(in crate::native) fn replay_work(&self) -> Result<u64, NativeError> {
        if self.history.len() > MAX_ASSISTANCE_HISTORY
            || self.pending.len() > MAX_ASSISTANCE_HISTORY / 3
        {
            return Err(NativeError::TooLarge);
        }
        self.history.iter().try_fold(0u64, |work, history| {
            let additional = match history {
                AssistanceHistory::Enter(payload) => {
                    1 + payload.request.scene_inventory.len()
                        + 3 * payload.request.callback.callback_work()
                        + payload
                            .traversal
                            .as_ref()
                            .map_or(0, |traversal| traversal.candidates.len())
                }
                _ => 1,
            };
            work.checked_add(additional as u64)
                .ok_or(NativeError::TooLarge)
        })
    }
    fn reserve(&self, additional: usize) -> Result<(), NativeError> {
        if self
            .history
            .len()
            .checked_add(self.pending_events())
            .and_then(|n| n.checked_add(additional))
            .is_none_or(|n| n > MAX_ASSISTANCE_HISTORY)
        {
            return Err(NativeError::TooLarge);
        }
        Ok(())
    }
    fn enter(&mut self, payload: AssistancePayload) -> Result<(), NativeError> {
        self.reserve(3)?;
        if payload.request.source != self.source
            || payload.request.occurrence != self.next_occurrence
            || self.pending.contains_key(&self.next_occurrence)
        {
            return Err(NativeError::AuthorityBinding);
        }
        self.can_check_for_help = false;
        self.already_called = true;
        self.pending.insert(
            self.next_occurrence,
            PendingAssistance {
                payload: payload.clone(),
                reenabled: false,
            },
        );
        self.next_occurrence = self
            .next_occurrence
            .checked_add(1)
            .ok_or(NativeError::Limit)?;
        self.history
            .push(AssistanceHistory::Enter(Box::new(payload)));
        Ok(())
    }
    fn stop(
        &mut self,
        npc: crate::NpcId,
        tick: u64,
        clock_ms: u64,
        reason: AssistanceStopReason,
    ) -> Result<(), NativeError> {
        self.reserve(1)?;
        // Unit::CombatStop resets only this latch, not m_checkForHelp or m_events.
        self.already_called = false;
        self.history.push(AssistanceHistory::CombatStop {
            npc,
            tick,
            clock_ms,
            reason,
        });
        Ok(())
    }
    fn dispatch(
        &mut self,
        npc: crate::NpcId,
        tick: u64,
        due: DueEvent,
        enemy: Option<crate::PlayerId>,
        in_combat: bool,
    ) -> Result<AssistanceOutcome, NativeError> {
        let (occurrence, reenable) = match due.kind {
            EventKind::ReenableHelp { occurrence } => (occurrence, true),
            EventKind::AggroLinking { occurrence } => (occurrence, false),
            _ => return Err(NativeError::InvalidState),
        };
        let pending = self
            .pending
            .get_mut(&occurrence)
            .ok_or(NativeError::InvalidState)?;
        let payload = &pending.payload;
        let event_id = if reenable {
            payload.reenable_event
        } else {
            payload.linking_event
        };
        let insertion_id = if reenable {
            payload.reenable_insertion
        } else {
            payload.linking_insertion
        };
        if due.event_id != event_id
            || due.insertion_id != insertion_id
            || due.scheduled_ms != payload.deadline_ms
            || due.exec_time_ms != payload.deadline_ms
            || due.add_time_ms != payload.request.unit_clock_ms
            || (reenable && pending.reenabled)
            || (!reenable && !pending.reenabled)
            || (due.callback == CallbackKind::Execute && due.now_ms < due.scheduled_ms)
        {
            return Err(NativeError::InvalidState);
        }
        let outcome = if due.callback == CallbackKind::Abort {
            // UnitLambdaEvent inherits the no-op BasicEvent::Abort.
            AssistanceOutcome::Aborted
        } else if reenable {
            self.can_check_for_help = true;
            AssistanceOutcome::Reenabled
        } else if enemy != Some(payload.request.enemy) {
            AssistanceOutcome::EnemyMissing
        } else if !in_combat {
            AssistanceOutcome::OwnerNotInCombat
        } else {
            AssistanceOutcome::EmptyContinuation {
                call_assistance: payload.saved_already_called,
            }
        };
        if reenable {
            pending.reenabled = true;
        } else {
            self.pending.remove(&occurrence);
        }
        self.history.push(AssistanceHistory::Callback {
            npc,
            tick,
            due,
            enemy,
            in_combat,
            outcome,
        });
        if self.history.len() > MAX_ASSISTANCE_HISTORY {
            return Err(NativeError::TooLarge);
        }
        Ok(outcome)
    }
}
pub(in crate::native) struct AssistanceEntryToken {
    pub occurrence: u64,
    saved_already_called: bool,
    reenable_event: u64,
    reenable_insertion: u64,
    deadline_ms: u64,
}
/// Mutable access always stays with the live Unit processor. A callback view
/// is refreshed after each insertion so scene witnesses observe that same queue.
enum AssistanceQueue<'a, 'b> {
    Owned(&'a mut EventProcessor),
    Callback(&'a mut EventTransaction<'b>),
}
impl AssistanceQueue<'_, '_> {
    fn clock_ms(&self) -> u64 {
        match self {
            Self::Owned(q) => q.clock_ms(),
            Self::Callback(q) => q.clock_ms(),
        }
    }
    fn next_event_id(&self) -> u64 {
        match self {
            Self::Owned(q) => q.next_event_id(),
            Self::Callback(q) => q.next_event_id(),
        }
    }
    fn event(&self, id: u64) -> Option<&BasicEventSnapshot> {
        match self {
            Self::Owned(q) => q.event(id),
            Self::Callback(q) => q.event(id),
        }
    }
    fn add_event(&mut self, id: u64, kind: EventKind, at: u64) -> Result<(), NativeError> {
        match self {
            Self::Owned(q) => q.add_event(id, kind, at)?,
            Self::Callback(q) => q.add_event(id, kind, at)?,
        }
        Ok(())
    }
    fn refresh(
        &self,
        callback: &mut Option<benilla_sim::event_processor::EventCallbackSnapshot>,
    ) -> Result<(), NativeError> {
        if let Self::Callback(q) = self {
            *callback = Some(q.callback_snapshot()?);
        }
        Ok(())
    }
}
impl NpcEvents {
    pub fn assistance(&self) -> Option<&CombatAssistanceState> {
        self.assistance.as_ref()
    }
    pub(in crate::native) fn prepare_assistance(
        &mut self,
        delay_ms: u32,
        extra_flags: u32,
    ) -> Result<AssistanceEntryToken, NativeError> {
        if self.callback.is_some() {
            return Err(NativeError::UnsupportedMechanic(
                "assistance requires owning live transaction",
            ));
        }
        self.prepare_assistance_with_transaction(delay_ms, extra_flags, None)
    }
    pub(in crate::native) fn prepare_assistance_with_transaction(
        &mut self,
        delay_ms: u32,
        extra_flags: u32,
        transaction: Option<&mut EventTransaction<'_>>,
    ) -> Result<AssistanceEntryToken, NativeError> {
        let mut queue = match transaction {
            Some(q) if self.callback.as_ref() == Some(&q.callback_snapshot()?) => {
                AssistanceQueue::Callback(q)
            }
            None if self.callback.is_none() => AssistanceQueue::Owned(&mut self.queue),
            _ => return Err(NativeError::AuthorityBinding),
        };
        let selected = self.assistance.as_mut().ok_or(NativeError::InvalidState)?;
        selected.reserve(3)?;
        let deadline_ms = queue
            .clock_ms()
            .checked_add(u64::from(delay_ms))
            .ok_or(NativeError::Limit)?;
        let reenable_event = queue.next_event_id();
        selected.can_check_for_help = false;
        queue.add_event(
            reenable_event,
            EventKind::ReenableHelp {
                occurrence: selected.next_occurrence,
            },
            deadline_ms,
        )?;
        let reenable_insertion = queue
            .event(reenable_event)
            .and_then(|e| e.queue)
            .ok_or(NativeError::InvalidState)?
            .insertion_id;
        queue.refresh(&mut self.callback)?;
        // Source saves the previous latch, not its negation, then marks it true.
        let saved_already_called = extra_flags & NO_CALL_ASSIST == 0 && selected.already_called;
        selected.already_called = true;
        Ok(AssistanceEntryToken {
            occurrence: selected.next_occurrence,
            saved_already_called,
            reenable_event,
            reenable_insertion,
            deadline_ms,
        })
    }
    pub(in crate::native) fn finish_assistance(
        &mut self,
        request: CombatAssistanceRequest,
        receivers: Option<CombatAssistanceFacts>,
        traversal: Option<AssistanceTraversalReceipt>,
        token: AssistanceEntryToken,
        extra_flags: u32,
    ) -> Result<(), NativeError> {
        self.finish_assistance_with_transaction(
            request,
            receivers,
            traversal,
            token,
            extra_flags,
            None,
        )
    }
    pub(in crate::native) fn finish_assistance_with_transaction(
        &mut self,
        request: CombatAssistanceRequest,
        receivers: Option<CombatAssistanceFacts>,
        traversal: Option<AssistanceTraversalReceipt>,
        token: AssistanceEntryToken,
        extra_flags: u32,
        transaction: Option<&mut EventTransaction<'_>>,
    ) -> Result<(), NativeError> {
        let mut queue = match transaction {
            Some(q) if self.callback.as_ref() == Some(&q.callback_snapshot()?) => {
                AssistanceQueue::Callback(q)
            }
            None if self.callback.is_none() => AssistanceQueue::Owned(&mut self.queue),
            _ => return Err(NativeError::AuthorityBinding),
        };
        if request.binding.world != self.world
            || request.binding.npc != Some(self.npc)
            || request.unit_clock_ms != queue.clock_ms()
            || request.occurrence != token.occurrence
            || queue.next_event_id()
                != token
                    .reenable_event
                    .checked_add(1)
                    .ok_or(NativeError::Limit)?
        {
            return Err(NativeError::AuthorityBinding);
        }
        match (&receivers, &traversal, extra_flags & NO_CALL_ASSIST == 0) {
            (Some(facts), Some(traversal), true) => traversal.validate(&request, facts)?,
            (None, None, false) => {}
            _ => return Err(NativeError::AuthorityBinding),
        }
        let linking_event = queue.next_event_id();
        queue.add_event(
            linking_event,
            EventKind::AggroLinking {
                occurrence: token.occurrence,
            },
            token.deadline_ms,
        )?;
        let linking_insertion = queue
            .event(linking_event)
            .and_then(|e| e.queue)
            .ok_or(NativeError::InvalidState)?
            .insertion_id;
        queue.refresh(&mut self.callback)?;
        self.assistance
            .as_mut()
            .ok_or(NativeError::InvalidState)?
            .enter(AssistancePayload {
                request,
                receivers,
                traversal,
                saved_already_called: token.saved_already_called,
                reenable_event: token.reenable_event,
                linking_event,
                reenable_insertion: token.reenable_insertion,
                linking_insertion,
                deadline_ms: token.deadline_ms,
            })?;
        self.append_assistance_anchor()
    }
    fn append_assistance_anchor(&mut self) -> Result<(), NativeError> {
        let history = self
            .assistance
            .as_ref()
            .and_then(|s| s.history.last())
            .ok_or(NativeError::InvalidState)?;
        self.assistance_anchor
            .as_mut()
            .ok_or(NativeError::InvalidState)?
            .append(history)
    }
    pub(in crate::native) fn assistance_combat_stop(
        &mut self,
        tick: u64,
        reason: AssistanceStopReason,
    ) -> Result<(), NativeError> {
        let clock = self.clock_ms();
        if let Some(state) = &mut self.assistance {
            state.stop(self.npc, tick, clock, reason)?;
            self.append_assistance_anchor()?;
        }
        Ok(())
    }
    pub(in crate::native) fn dispatch_assistance(
        &mut self,
        tick: u64,
        due: DueEvent,
        enemy: Option<crate::PlayerId>,
        in_combat: bool,
    ) -> Result<AssistanceOutcome, NativeError> {
        let outcome = self
            .assistance
            .as_mut()
            .ok_or(NativeError::InvalidState)?
            .dispatch(self.npc, tick, due, enemy, in_combat)?;
        self.append_assistance_anchor()?;
        Ok(outcome)
    }
    pub(in crate::native) fn validate_assistance_queue(&self) -> Result<(), NativeError> {
        if self.assistance.is_some() != self.assistance_anchor.is_some() {
            return Err(NativeError::InvalidCheckpoint);
        }
        let Some(state) = &self.assistance else {
            if self.queue_snapshot().queued.iter().any(|event| {
                matches!(
                    event.kind,
                    EventKind::ReenableHelp { .. } | EventKind::AggroLinking { .. }
                )
            }) {
                return Err(NativeError::InvalidCheckpoint);
            }
            return Ok(());
        };
        state.replay_work()?;
        let snapshot = self.queue_snapshot();
        let events: Vec<_> = snapshot
            .queued
            .iter()
            .filter(|e| {
                matches!(
                    e.kind,
                    EventKind::ReenableHelp { .. } | EventKind::AggroLinking { .. }
                )
            })
            .collect();
        if events.len() != state.pending_events() {
            return Err(NativeError::InvalidCheckpoint);
        }
        for event in events {
            let (occurrence, reenable) = match event.kind {
                EventKind::ReenableHelp { occurrence } => (occurrence, true),
                EventKind::AggroLinking { occurrence } => (occurrence, false),
                _ => unreachable!(),
            };
            let pending = state
                .pending
                .get(&occurrence)
                .ok_or(NativeError::InvalidCheckpoint)?;
            let p = &pending.payload;
            if event.event_id
                != if reenable {
                    p.reenable_event
                } else {
                    p.linking_event
                }
                || (reenable && pending.reenabled)
                || event.to_abort
                || event.add_time_ms != p.request.unit_clock_ms
                || event.exec_time_ms != p.deadline_ms
                || event.queue.is_none_or(|q| {
                    q.insertion_id
                        != if reenable {
                            p.reenable_insertion
                        } else {
                            p.linking_insertion
                        }
                })
                || event.exec_time_ms < snapshot.clock_ms
                || (event.exec_time_ms == snapshot.clock_ms
                    && event.add_time_ms < snapshot.clock_ms)
            {
                return Err(NativeError::InvalidCheckpoint);
            }
        }
        Ok(())
    }
}

pub(in crate::native) fn validate_current(
    profile: &Profile,
    state: &State,
) -> Result<(), NativeError> {
    for npc in state.npcs.values() {
        npc.events.validate_assistance_queue()?;
        match (profile.combat_assistance(), npc.events.assistance.as_ref()) {
            (None, None) => continue,
            (Some(catalog), Some(actual)) => {
                actual.replay_work()?;
                let mut expected = CombatAssistanceState::new(catalog.digest());
                let source = catalog.source(npc.id.spawn)?;
                let can_call = profile.assistance_extra_flags(npc.id.spawn)? & NO_CALL_ASSIST == 0;
                let mut last_tick = 0;
                let mut last_clock = 0;
                let mut last_generation = 0;
                let mut last_receipt = 0;
                let mut callbacks = BTreeSet::new();
                let mut last_event = 0;
                let mut combat_active = false;
                for history in &actual.history {
                    // These two equal-deadline events were inserted consecutively;
                    // the first lambda cannot invoke an intervening callback.
                    if let Some((occurrence, _)) =
                        expected.pending.iter().find(|(_, p)| p.reenabled)
                    {
                        if !matches!(history, AssistanceHistory::Callback { due, .. } if due.kind == (EventKind::AggroLinking { occurrence: *occurrence }))
                        {
                            return Err(NativeError::InvalidCheckpoint);
                        }
                    }
                    let (id, tick, clock) = match history {
                        AssistanceHistory::Enter(p) => (
                            p.request
                                .binding
                                .npc
                                .ok_or(NativeError::InvalidCheckpoint)?,
                            p.request.binding.tick,
                            p.request.unit_clock_ms,
                        ),
                        AssistanceHistory::CombatStop {
                            npc,
                            tick,
                            clock_ms,
                            ..
                        } => (*npc, *tick, *clock_ms),
                        AssistanceHistory::Callback { npc, tick, due, .. } => {
                            (*npc, *tick, due.now_ms)
                        }
                    };
                    if tick == 0
                        || tick < last_tick
                        || tick > state.tick
                        || clock < last_clock
                        || clock > npc.events.clock_ms()
                        || !clock.is_multiple_of(u64::from(profile.tick_ms))
                        || id.spawn != npc.id.spawn
                        || id.generation < last_generation
                        || id.generation > npc.id.generation
                    {
                        return Err(NativeError::InvalidCheckpoint);
                    }
                    last_tick = tick;
                    last_clock = clock;
                    last_generation = id.generation;
                    let life = npc
                        .source_generation
                        .as_ref()
                        .and_then(|o| o.historical(id))
                        .ok_or(NativeError::InvalidCheckpoint)?;
                    if tick < life.creation_tick() {
                        return Err(NativeError::InvalidCheckpoint);
                    }
                    match history {
                        AssistanceHistory::Enter(p) => {
                            let r = &p.request;
                            r.callback.validate_for(profile, state, id, tick)?;
                            if let EventAiCallbackKind::CombatReceipt { receipt } = &r.callback {
                                if receipt.id <= last_receipt {
                                    return Err(NativeError::InvalidCheckpoint);
                                }
                                last_receipt = receipt.id;
                            }
                            let callback_key = crate::digest(
                                b"benilla-native-assistance-cause-v47\0",
                                &(id, &r.callback),
                            );
                            if combat_active
                                || !callbacks.insert(callback_key)
                                || life.death_tick().is_some_and(|death| tick > death)
                                || r.binding.world != state.world
                                || r.binding.profile != profile.digest()
                                || r.binding.authority_policy != profile.authority_policy
                                || r.binding.geometry != profile.geometry_revision
                                || r.content != profile.content_digest
                                || r.source != catalog.digest()
                                || r.generation_source != source.generation_source
                                || r.enemy != state.player
                                || r.radius != source.call_for_help
                                || r.rng_counter > state.rng_counter
                                || r.now_ms
                                    != profile
                                        .clock_origin_ms
                                        .checked_add(
                                            tick.checked_mul(u64::from(profile.tick_ms))
                                                .ok_or(NativeError::InvalidCheckpoint)?,
                                        )
                                        .ok_or(NativeError::InvalidCheckpoint)?
                                || clock > tick.saturating_mul(u64::from(profile.tick_ms))
                                || r.occurrence != expected.next_occurrence
                                || p.saved_already_called != (can_call && expected.already_called)
                                || p.reenable_event <= last_event
                                || p.linking_event
                                    != p.reenable_event
                                        .checked_add(1)
                                        .ok_or(NativeError::InvalidCheckpoint)?
                                || p.linking_event >= npc.events.queue_snapshot().next_event_id
                                || p.reenable_insertion == 0
                                || p.linking_insertion
                                    != p.reenable_insertion
                                        .checked_add(1)
                                        .ok_or(NativeError::InvalidCheckpoint)?
                                || p.linking_insertion
                                    >= npc.events.queue_snapshot().next_insertion_id
                                || p.deadline_ms
                                    != clock
                                        .checked_add(u64::from(catalog.delay_ms()))
                                        .ok_or(NativeError::InvalidCheckpoint)?
                            {
                                return Err(NativeError::InvalidCheckpoint);
                            }
                            r.owner_pose.validate()?;
                            r.enemy_pose.validate()?;
                            r.traversal_radius()?;
                            validate_historical_inventory(profile, state, r)?;
                            match (&p.receivers, &p.traversal, can_call) {
                                (Some(facts), Some(traversal), true) => {
                                    traversal.validate(r, facts)?
                                }
                                (None, None, false) => {}
                                _ => return Err(NativeError::InvalidCheckpoint),
                            }
                            let initialization = life.result();
                            if r.owner_bounding_radius.bits()
                                != initialization.model.bounding_radius.bits()
                            {
                                return Err(NativeError::InvalidCheckpoint);
                            }
                            validate_event_ai_link(profile, state, r)?;
                            last_event = p.linking_event;
                            combat_active = true;
                            expected.enter((**p).clone())?;
                        }
                        AssistanceHistory::CombatStop { reason, .. } => {
                            if *reason == AssistanceStopReason::Death
                                && life.death_tick() != Some(tick)
                            {
                                return Err(NativeError::InvalidCheckpoint);
                            }
                            combat_active = false;
                            expected.stop(id, tick, clock, *reason)?;
                        }
                        AssistanceHistory::Callback {
                            due,
                            enemy,
                            in_combat,
                            outcome,
                            ..
                        } => {
                            // A same-queue callback can insert a zero-delay lambda
                            // that executes later in this update. A peer's ordinary
                            // queue starts dispatch at its next Unit update instead.
                            let step = u64::from(profile.tick_ms);
                            let rounded = due
                                .scheduled_ms
                                .checked_add(step - 1)
                                .ok_or(NativeError::InvalidCheckpoint)?
                                / step
                                * step;
                            let occurrence = match due.kind {
                                EventKind::ReenableHelp { occurrence }
                                | EventKind::AggroLinking { occurrence } => occurrence,
                                _ => return Err(NativeError::InvalidCheckpoint),
                            };
                            let entered_inside = expected
                                .pending
                                .get(&occurrence)
                                .ok_or(NativeError::InvalidCheckpoint)?
                                .payload
                                .request
                                .callback
                                .owns_active_queue(id);
                            let first_update = due
                                .add_time_ms
                                .checked_add(if entered_inside { 0 } else { step })
                                .ok_or(NativeError::InvalidCheckpoint)?;
                            if due.now_ms != rounded.max(first_update)
                                || *in_combat != combat_active
                                || *enemy != Some(state.player)
                                || due.callback != CallbackKind::Execute
                                || due.diff_ms != Some(profile.tick_ms)
                            {
                                return Err(NativeError::InvalidCheckpoint);
                            }
                            if expected.dispatch(id, tick, *due, *enemy, *in_combat)? != *outcome {
                                return Err(NativeError::InvalidCheckpoint);
                            }
                        }
                    }
                }
                let mut anchor = CombatAssistanceAnchor::default();
                for history in &expected.history {
                    anchor.append(history)?;
                }
                if npc.events.assistance_anchor.as_ref() != Some(&anchor)
                    || &expected != actual
                    || combat_active != (npc.ai.phase() == ai::Phase::Combat)
                {
                    return Err(NativeError::InvalidCheckpoint);
                }
            }
            _ => return Err(NativeError::InvalidCheckpoint),
        }
    }
    Ok(())
}
/// Validate historical identity, not later flags, combat state or reputation.
/// Entry-time denials belong to the independently anchored Unit-owner history.
fn validate_historical_inventory(
    profile: &Profile,
    state: &State,
    request: &CombatAssistanceRequest,
) -> Result<(), NativeError> {
    if request.scene_inventory.len() != profile.npcs.len()
        || !request
            .scene_inventory
            .iter()
            .map(|id| id.spawn)
            .eq(profile.npcs.iter().map(|npc| npc.ai.creature_id))
    {
        return Err(NativeError::InvalidCheckpoint);
    }
    for id in &request.scene_inventory {
        let generation = state
            .npcs
            .get(&id.spawn)
            .and_then(|npc| npc.source_generation.as_ref())
            .ok_or(NativeError::InvalidCheckpoint)?;
        let life = generation
            .historical(*id)
            .ok_or(NativeError::InvalidCheckpoint)?;
        let next = id.generation.checked_add(1).and_then(|generation_id| {
            generation.historical(crate::NpcId {
                spawn: id.spawn,
                generation: generation_id,
            })
        });
        if life.creation_tick() > request.binding.tick
            || next.is_some_and(|life| life.creation_tick() < request.binding.tick)
        {
            return Err(NativeError::InvalidCheckpoint);
        }
    }
    Ok(())
}
fn validate_event_ai_link(
    profile: &Profile,
    state: &State,
    request: &CombatAssistanceRequest,
) -> Result<(), NativeError> {
    let has_events = profile
        .event_ai()
        .map(|c| c.catalog().events(request.binding.npc.unwrap().spawn))
        .transpose()?
        .is_some_and(|events| !events.is_empty());
    if !has_events {
        return if request.event_ai_callback.is_none() {
            Ok(())
        } else {
            Err(NativeError::InvalidCheckpoint)
        };
    }
    let evaluation = state
        .event_ai
        .as_ref()
        .and_then(|events| {
            events.history.iter().find_map(|history| {
                let event_ai::EventAiHistory::EnterCombat(e) = history else {
                    return None;
                };
                (Some(e.callback.digest()) == request.event_ai_callback).then_some(e)
            })
        })
        .ok_or(NativeError::InvalidCheckpoint)?;
    let c = &evaluation.callback;
    if Some(c.npc) != request.binding.npc
        || c.tick != request.binding.tick
        || c.kind != request.callback
        || c.speaker_pose != request.owner_pose
        || c.player_pose != request.enemy_pose
        || c.invoker != request.enemy
        || evaluation
            .actions
            .last()
            .and_then(|a| a.action_counter.or(Some(a.chance_counter)))
            .and_then(|counter| counter.checked_add(1))
            != Some(request.rng_counter)
    {
        return Err(NativeError::InvalidCheckpoint);
    }
    Ok(())
}
