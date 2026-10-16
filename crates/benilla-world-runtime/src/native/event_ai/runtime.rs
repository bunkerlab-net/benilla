// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 CreatureEventAI::EnterCombat/ProcessEvent/ProcessAction.
//! Generation-bound continuation and semantic replay of the bounded text callback.
use super::*;
use benilla_sim::{
    creature_motion as m, event_ai as source, event_processor::EventCallbackSnapshot,
};

pub const EVENT_AI_RUNTIME_POLICY: &str = "cmangos-8ec338a1-native-aggro-text-v1";
pub const MAX_EVENT_AI_HISTORY: usize = MAX_RECORDS;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum EventAiCallbackKind {
    CombatReceipt {
        receipt: ai::Receipt,
    },
    Sight {
        notification_owner: VisitorParticipant,
        callback: EventCallbackSnapshot,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventAiCallback {
    pub world: crate::WorldId,
    pub tick: u64,
    pub now_ms: u64,
    pub moment: InteractionMoment,
    pub catalog: crate::Digest,
    pub npc: crate::NpcId,
    pub invoker: crate::PlayerId,
    pub kind: EventAiCallbackKind,
    pub speaker_pose: m::Pose,
    pub player_pose: m::Pose,
    pub draft: crate::Digest,
}
impl EventAiCallback {
    pub fn digest(&self) -> crate::Digest {
        crate::digest(b"benilla-native-event-ai-callback-v35\0", self)
    }
    fn validate(&self, profile: &Profile, state: &State) -> Result<(), NativeError> {
        let selected = profile.event_ai().ok_or(NativeError::InvalidCheckpoint)?;
        if self.world != state.world
            || self.invoker != state.player
            || self.catalog != selected.digest()
            || self.tick == 0
            || self.tick > state.tick
            || self.now_ms
                != profile
                    .clock_origin_ms
                    .checked_add(
                        self.tick
                            .checked_mul(u64::from(profile.tick_ms))
                            .ok_or(NativeError::InvalidCheckpoint)?,
                    )
                    .ok_or(NativeError::InvalidCheckpoint)?
            || !matches!(self.moment, InteractionMoment::Frame { action_ordinal } if action_ordinal < 20_000)
            || self.draft == [0; 32]
            || state
                .npcs
                .get(&self.npc.spawn)
                .is_none_or(|n| n.id.generation < self.npc.generation)
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        self.speaker_pose.validate()?;
        self.player_pose.validate()?;
        match &self.kind {
            EventAiCallbackKind::CombatReceipt { receipt } => {
                if receipt.id == 0
                    || receipt.id > state.npcs[&self.npc.spawn].ai.last_receipt_id()
                    || !matches!(
                        receipt.event,
                        ai::CombatEvent::AttackedBy { target: SOLO_UNIT }
                            | ai::CombatEvent::Threat {
                                target: SOLO_UNIT,
                                kind: ai::ThreatKind::DirectDamage,
                                ..
                            }
                    )
                {
                    return Err(NativeError::InvalidCheckpoint);
                }
            }
            EventAiCallbackKind::Sight {
                notification_owner,
                callback,
            } => {
                use benilla_sim::event_processor::{
                    CallbackKind, EventKind, EventProcessor, MAX_CALLBACKS_PER_UPDATE,
                };
                let queue = EventProcessor::restore(callback.queue.clone())?;
                let elapsed = self
                    .tick
                    .checked_mul(u64::from(profile.tick_ms))
                    .ok_or(NativeError::InvalidCheckpoint)?;
                let live_queue = match notification_owner {
                    VisitorParticipant::Player(p) if *p == state.player => {
                        state.player_events.queue_snapshot()
                    }
                    VisitorParticipant::Creature(n) if *n == self.npc => {
                        state.npcs[&n.spawn].events.queue_snapshot()
                    }
                    _ => return Err(NativeError::InvalidCheckpoint),
                };
                let detached = queue
                    .event(callback.due.event_id)
                    .ok_or(NativeError::InvalidCheckpoint)?;
                if callback.due.callback != CallbackKind::Execute
                    || callback.due.kind != EventKind::AiNotify
                    || callback.due.diff_ms != Some(profile.tick_ms)
                    || callback.due.now_ms != callback.queue.clock_ms
                    || callback.due.now_ms > elapsed
                    || (matches!(notification_owner, VisitorParticipant::Player(_))
                        && callback.due.now_ms != elapsed)
                    || callback.queue.aborting
                    || callback.queue.detached.len() != 1
                    || callback.callback_ordinal == 0
                    || callback.callback_ordinal as usize > MAX_CALLBACKS_PER_UPDATE
                    || callback.queue.next_event_id > live_queue.next_event_id
                    || callback.queue.next_insertion_id > live_queue.next_insertion_id
                    || callback.queue.clock_ms > live_queue.clock_ms
                    || detached.kind != callback.due.kind
                    || detached.to_abort
                    || detached.queue.is_some()
                    || detached.add_time_ms != callback.due.add_time_ms
                    || detached.exec_time_ms != callback.due.exec_time_ms
                    || callback.due.scheduled_ms != callback.due.exec_time_ms
                    || callback.due.scheduled_ms > callback.due.now_ms
                    || callback.due.insertion_id == 0
                    || callback.due.insertion_id >= callback.queue.next_insertion_id
                    || callback.queue.queued.iter().any(|event| {
                        event
                            .queue
                            .is_some_and(|key| key.insertion_id == callback.due.insertion_id)
                    })
                    || (matches!(notification_owner, VisitorParticipant::Creature(_))
                        && callback.due.insertion_id != callback.due.event_id)
                {
                    return Err(NativeError::InvalidCheckpoint);
                }
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventAiSpeech {
    pub callback: EventAiCallback,
    pub event_id: u32,
    pub occurrence: u64,
    pub broadcast_id: u32,
    pub text_slot: u8,
    pub recipient: crate::PlayerId,
    pub locale: EventAiLocale,
    pub speaker_name: String,
    pub text: String,
    pub language: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventAiAudienceReceipt {
    pub request: EventAiAudienceRequest,
    pub facts: EventAiAudienceFacts,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventAiActionReceipt {
    pub event_id: u32,
    pub chance_counter: u64,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub action_counter: Option<u64>,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub broadcast_id: Option<u32>,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub text_slot: Option<u8>,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub audience: Option<EventAiAudienceReceipt>,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub speech: Option<EventAiSpeech>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventAiEvaluation {
    pub callback: EventAiCallback,
    pub occurrence: u64,
    pub actions: Vec<EventAiActionReceipt>,
}
/// Independent Unit event-context anchor, retained when source respawn keeps its processor.
/// Semantic history must reproduce this, even after journal compaction.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::native) struct EventAiCallbackAnchor {
    count: u64,
    tip: crate::Digest,
}
impl EventAiCallbackAnchor {
    pub(in crate::native) fn append(
        &mut self,
        evaluation: &EventAiEvaluation,
    ) -> Result<(), NativeError> {
        self.count = self.count.checked_add(1).ok_or(NativeError::Limit)?;
        self.tip = crate::digest(
            b"benilla-native-event-ai-callback-chain-v35\0",
            &(self.tip, self.count, evaluation),
        );
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::native) enum EventAiHistory {
    EnterCombat(Box<EventAiEvaluation>),
    Respawn {
        previous: crate::NpcId,
        replacement: crate::NpcId,
        tick: u64,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::native) struct EventAiOwnerState {
    pub id: crate::NpcId,
    pub next_occurrence: u64,
    pub holders: Vec<source::AggroTextHolderSnapshot>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventAiState {
    pub(in crate::native) policy: String,
    pub(in crate::native) catalog: crate::Digest,
    pub(in crate::native) owners: BTreeMap<u64, EventAiOwnerState>,
    pub(in crate::native) history: Vec<EventAiHistory>,
}
impl Profile {
    pub fn event_ai(&self) -> Option<&EventAiRuntimeCatalog> {
        self.source_visitors
            .as_ref()
            .and_then(SourceVisitorCatalog::event_ai)
    }
}
impl EventAiState {
    pub(in crate::native) fn new(profile: &Profile) -> Result<Option<Self>, NativeError> {
        let Some(catalog) = profile.event_ai() else {
            return Ok(None);
        };
        let owners = profile
            .npcs
            .iter()
            .map(|n| {
                let id = crate::NpcId {
                    spawn: n.ai.creature_id,
                    generation: 0,
                };
                Ok((id.spawn, Self::owner(catalog, id)?))
            })
            .collect::<Result<_, NativeError>>()?;
        Ok(Some(Self {
            policy: EVENT_AI_RUNTIME_POLICY.into(),
            catalog: catalog.digest(),
            owners,
            history: Vec::new(),
        }))
    }
    fn owner(
        catalog: &EventAiRuntimeCatalog,
        id: crate::NpcId,
    ) -> Result<EventAiOwnerState, NativeError> {
        Ok(EventAiOwnerState {
            id,
            next_occurrence: 0,
            holders: catalog
                .catalog()
                .events(id.spawn)?
                .into_iter()
                .map(|e| {
                    source::AggroTextHolder::new(e)
                        .snapshot()
                        .map_err(NativeError::EventAi)
                })
                .collect::<Result<_, _>>()?,
        })
    }
    pub(in crate::native) fn respawn(
        &mut self,
        profile: &Profile,
        replacement: crate::NpcId,
        tick: u64,
    ) -> Result<(), NativeError> {
        self.reserve_history()?;
        let selected = profile.event_ai().ok_or(NativeError::InvalidState)?;
        let old = self
            .owners
            .get(&replacement.spawn)
            .ok_or(NativeError::InvalidState)?;
        if old.id.generation.checked_add(1) != Some(replacement.generation) {
            return Err(NativeError::InvalidState);
        }
        self.history.push(EventAiHistory::Respawn {
            previous: old.id,
            replacement,
            tick,
        });
        self.owners
            .insert(replacement.spawn, Self::owner(selected, replacement)?);
        Ok(())
    }
    pub(in crate::native) fn replay_work(&self) -> Result<u64, NativeError> {
        if self.history.len() > MAX_EVENT_AI_HISTORY {
            return Err(NativeError::TooLarge);
        }
        self.history.iter().try_fold(0u64, |total, history| {
            let work = match history {
                EventAiHistory::Respawn { .. } => 1,
                EventAiHistory::EnterCombat(e) => {
                    if e.actions.len() > MAX_EVENTS {
                        return Err(NativeError::TooLarge);
                    }
                    let queue = match &e.callback.kind {
                        EventAiCallbackKind::Sight { callback, .. } => {
                            callback.queue.queued.len() + callback.queue.detached.len()
                        }
                        _ => 0,
                    };
                    // Callback validation/anchor hashing and each action's
                    // request hashing, speech clone and comparison traverse the
                    // callback queue. Count nested recorded speech copies too;
                    // malformed copies must not evade typed-restore work bounds.
                    let action_work = 4 + 4 * e.actions.len() as u64;
                    let speech_queues = e.actions.iter().try_fold(0u64, |sum, action| {
                        let count = match action.speech.as_ref().map(|s| &s.callback.kind) {
                            Some(EventAiCallbackKind::Sight { callback, .. }) => {
                                callback.queue.queued.len() + callback.queue.detached.len()
                            }
                            _ => 0,
                        };
                        sum.checked_add(count as u64).ok_or(NativeError::TooLarge)
                    })?;
                    (queue as u64)
                        .checked_mul(action_work)
                        .and_then(|n| n.checked_add(speech_queues.checked_mul(2)?))
                        .and_then(|n| n.checked_add(1 + e.actions.len() as u64))
                        .ok_or(NativeError::TooLarge)?
                }
            };
            total.checked_add(work).ok_or(NativeError::TooLarge)
        })
    }
    pub(in crate::native) fn reserve_history(&self) -> Result<(), NativeError> {
        if self.history.len() >= MAX_EVENT_AI_HISTORY {
            Err(NativeError::TooLarge)
        } else {
            Ok(())
        }
    }
    pub(in crate::native) fn validate_optional(
        profile: &Profile,
        state: &State,
    ) -> Result<(), NativeError> {
        match (profile.event_ai(), &state.event_ai) {
            (None, None) => Ok(()),
            (Some(catalog), Some(actual)) => actual.validate(catalog, profile, state),
            _ => Err(NativeError::InvalidCheckpoint),
        }
    }
    fn validate(
        &self,
        catalog: &EventAiRuntimeCatalog,
        profile: &Profile,
        state: &State,
    ) -> Result<(), NativeError> {
        if self.policy != EVENT_AI_RUNTIME_POLICY
            || self.catalog != catalog.digest()
            || self.history.len() > MAX_EVENT_AI_HISTORY
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        let mut expected = Self::new(profile)?.ok_or(NativeError::InvalidCheckpoint)?;
        let mut anchors: BTreeMap<_, _> = profile
            .npcs
            .iter()
            .map(|n| (n.ai.creature_id, EventAiCallbackAnchor::default()))
            .collect();
        let mut previous_tick = 0;
        let mut next_counter = 0;
        let mut last_callbacks: BTreeMap<crate::NpcId, EventAiCallbackKind> = BTreeMap::new();
        for history in &self.history {
            let tick = match history {
                EventAiHistory::EnterCombat(e) => e.callback.tick,
                EventAiHistory::Respawn { tick, .. } => *tick,
            };
            if tick < previous_tick || tick == 0 || tick > state.tick {
                return Err(NativeError::InvalidCheckpoint);
            }
            previous_tick = tick;
            match history {
                EventAiHistory::Respawn {
                    previous,
                    replacement,
                    ..
                } => {
                    if previous.spawn != replacement.spawn
                        || previous.generation.checked_add(1) != Some(replacement.generation)
                        || expected
                            .owners
                            .get(&replacement.spawn)
                            .is_none_or(|o| o.id != *previous)
                    {
                        return Err(NativeError::InvalidCheckpoint);
                    }
                    expected.respawn(profile, *replacement, tick)?;
                }
                EventAiHistory::EnterCombat(e) => {
                    e.callback.validate(profile, state)?;
                    anchors
                        .get_mut(&e.callback.npc.spawn)
                        .ok_or(NativeError::InvalidCheckpoint)?
                        .append(e)?;
                    let owner = expected
                        .owners
                        .get_mut(&e.callback.npc.spawn)
                        .ok_or(NativeError::InvalidCheckpoint)?;
                    if owner.id != e.callback.npc
                        || owner.next_occurrence != e.occurrence
                        || last_callbacks.get(&owner.id) == Some(&e.callback.kind)
                    {
                        return Err(NativeError::InvalidCheckpoint);
                    }
                    last_callbacks.insert(owner.id, e.callback.kind.clone());
                    let events = catalog.catalog().events(owner.id.spawn)?;
                    if e.actions.len() != events.len() {
                        return Err(NativeError::InvalidCheckpoint);
                    }
                    for (index, (event, action)) in events.iter().zip(&e.actions).enumerate() {
                        if action.event_id != event.event_id()
                            || action.chance_counter < next_counter
                            || (index > 0 && action.chance_counter != next_counter)
                            || action.chance_counter >= state.rng_counter
                        {
                            return Err(NativeError::InvalidCheckpoint);
                        }
                        let mut holder = source::AggroTextHolder::new(*event);
                        holder.begin_enter_combat();
                        let chance = rng::full_u32(state.world, state.seed, action.chance_counter);
                        next_counter = action
                            .chance_counter
                            .checked_add(1)
                            .ok_or(NativeError::InvalidCheckpoint)?;
                        let passed = holder
                            .consume_chance(chance)
                            .map_err(NativeError::EventAi)?
                            == source::ChanceResult::ActionRequired;
                        if passed {
                            if action.action_counter != Some(next_counter)
                                || next_counter >= state.rng_counter
                            {
                                return Err(NativeError::InvalidCheckpoint);
                            }
                            let selection = holder
                                .consume_action(rng::full_u32(
                                    state.world,
                                    state.seed,
                                    next_counter,
                                ))
                                .map_err(NativeError::EventAi)?;
                            next_counter += 1;
                            if action.broadcast_id != Some(selection.text_id)
                                || action.text_slot != Some(selection.text_slot)
                            {
                                return Err(NativeError::InvalidCheckpoint);
                            }
                            let audience = action
                                .audience
                                .as_ref()
                                .ok_or(NativeError::InvalidCheckpoint)?;
                            let request =
                                EventAiAudienceRequest::from_callback(profile, &e.callback)?;
                            if audience.request != request {
                                return Err(NativeError::InvalidCheckpoint);
                            }
                            let delivered = request.validate(&audience.facts)?;
                            let speech = delivered
                                .then(|| {
                                    Self::speech(catalog, &e.callback, e.occurrence, selection)
                                })
                                .transpose()?;
                            if action.speech != speech {
                                return Err(NativeError::InvalidCheckpoint);
                            }
                        } else if action.action_counter.is_some()
                            || action.broadcast_id.is_some()
                            || action.text_slot.is_some()
                            || action.audience.is_some()
                            || action.speech.is_some()
                        {
                            return Err(NativeError::InvalidCheckpoint);
                        }
                    }
                    owner.next_occurrence = owner
                        .next_occurrence
                        .checked_add(1)
                        .ok_or(NativeError::InvalidCheckpoint)?;
                }
            }
        }
        if state.npcs.iter().any(|(spawn, n)| {
            n.events.event_ai_anchor.as_ref() != anchors.get(spawn)
                || (n.ai.phase() == ai::Phase::Combat
                    && catalog
                        .catalog()
                        .events(*spawn)
                        .is_ok_and(|events| !events.is_empty())
                    && self
                        .owners
                        .get(spawn)
                        .is_none_or(|o| o.next_occurrence == 0))
        }) {
            return Err(NativeError::InvalidCheckpoint);
        }
        if self.owners != expected.owners
            || self.owners.len() != state.npcs.len()
            || self
                .owners
                .iter()
                .any(|(spawn, o)| state.npcs.get(spawn).is_none_or(|n| n.id != o.id))
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok(())
    }
    pub(in crate::native) fn speech(
        catalog: &EventAiRuntimeCatalog,
        callback: &EventAiCallback,
        occurrence: u64,
        selection: source::TextSelection,
    ) -> Result<EventAiSpeech, NativeError> {
        let text = catalog.catalog().localized(
            callback.npc.spawn,
            selection.text_id as i32,
            catalog.locale(),
        )?;
        Ok(EventAiSpeech {
            callback: callback.clone(),
            event_id: selection.event_id,
            occurrence,
            broadcast_id: selection.text_id,
            text_slot: selection.text_slot,
            recipient: callback.invoker,
            locale: catalog.locale(),
            speaker_name: text.speaker_name,
            text: text.text,
            language: text.language,
        })
    }
}

impl EventAiAudienceRequest {
    pub(in crate::native) fn from_callback(
        profile: &Profile,
        callback: &EventAiCallback,
    ) -> Result<Self, NativeError> {
        let selected = profile.event_ai().ok_or(NativeError::InvalidState)?;
        let npc = profile
            .npc(callback.npc.spawn)
            .ok_or(NativeError::InvalidState)?;
        Ok(Self {
            callback: callback.digest(),
            world: callback.world,
            tick: callback.tick,
            moment: callback.moment,
            catalog: callback.catalog,
            speaker: callback.npc,
            map: profile.map_id,
            speaker_pose: callback.speaker_pose,
            player: callback.invoker,
            player_pose: callback.player_pose,
            camera: EventAiCamera {
                owner: callback.invoker,
                body: VisitorParticipant::Player(callback.invoker),
                body_pose: callback.player_pose,
            },
            speaker_bounding_radius: npc.ai.bounding_radius,
            speaker_combat_reach: npc.ai.combat_reach,
            body_combat_reach: profile.player.combat_reach,
            say_range: selected.say_range(),
            visibility_distance: selected.visibility_distance(),
            selection: EventAiAudienceSelection::SoloSelfCamera,
        })
    }
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
