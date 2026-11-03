// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 CreatureEventAI.cpp:35-48,575-662,1382-1452,
// 1597-1632,1929-1975. Copyright CMaNGOS contributors.
use super::*;
use source::{OocDraw, OocSnapshot, OocTimers, TextSelection};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::native) enum OocOperation {
    Initialize,
    EnterCombat,
    Reset,
    Update { combat: bool, evade: bool },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::native) struct OocDelivery {
    pub selection: TextSelection,
    pub audience: EventAiAudienceReceipt,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub speech: Option<EventAiSpeech>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::native) struct OocRecord {
    pub npc: crate::NpcId,
    pub tick: u64,
    pub operation: OocOperation,
    pub draws: Vec<Draw>,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub callback: Option<EventAiCallback>,
    pub deliveries: Vec<OocDelivery>,
}
impl OocRecord {
    pub fn tip(&self, previous: crate::Digest) -> crate::Digest {
        crate::digest(
            b"benilla-native-event-ai-ooc-chain-v89\0",
            &(previous, self),
        )
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::native) struct OocState {
    pub owners: BTreeMap<u64, (crate::NpcId, OocSnapshot)>,
    pub history: Vec<OocRecord>,
}

pub(in crate::native) fn ooc_draw(
    request: OocDraw,
    npc: crate::NpcId,
    occurrence: u64,
) -> (DrawPurpose, i64, i64) {
    match request {
        OocDraw::Initial { event_id, min, max } | OocDraw::Repeat { event_id, min, max } => (
            DrawPurpose::EventAiTimer {
                npc,
                event_id,
                initial: matches!(request, OocDraw::Initial { .. }),
            },
            i64::from(min),
            i64::from(max),
        ),
        OocDraw::Chance { event_id } => (
            DrawPurpose::EventAiChance {
                npc,
                event_id,
                occurrence,
            },
            0,
            i64::from(u32::MAX),
        ),
        OocDraw::Action { event_id } => (
            DrawPurpose::EventAiAction {
                npc,
                event_id,
                occurrence,
            },
            0,
            i64::from(u32::MAX),
        ),
    }
}
impl OocState {
    pub fn replay_work(&self) -> Result<u64, NativeError> {
        if self.history.len() > MAX_EVENT_AI_HISTORY {
            return Err(NativeError::TooLarge);
        }
        self.history.iter().try_fold(0u64, |total, record| {
            if record.draws.len() > MAX_EVENTS * 3 || record.deliveries.len() > MAX_EVENTS {
                return Err(NativeError::TooLarge);
            }
            let nested = record
                .deliveries
                .iter()
                .map(|d| {
                    d.speech
                        .as_ref()
                        .map_or(0, |s| s.callback.kind.callback_work()) as u64
                })
                .sum::<u64>();
            let callback = record
                .callback
                .as_ref()
                .map_or(0, |c| c.kind.callback_work()) as u64;
            total
                .checked_add(
                    1 + record.draws.len() as u64
                        + record.deliveries.len() as u64 * 4
                        + nested * 2
                        + callback,
                )
                .ok_or(NativeError::TooLarge)
        })
    }
    pub fn initialize(
        &mut self,
        catalog: &EventAiRuntimeCatalog,
        world: crate::WorldId,
        seed: u64,
        npc: crate::NpcId,
        cursor: &mut u64,
    ) -> Result<crate::Digest, NativeError> {
        if catalog.catalog().ooc_events(npc.spawn)?.is_empty() {
            return Ok([0; 32]);
        }
        let mut draws = Vec::new();
        self.apply(catalog, npc, OocOperation::Initialize, 0, &mut |request| {
            let (purpose, low, high) = ooc_draw(request, npc, 0);
            let sample = rng::sample_inclusive_v2(world, seed, cursor, low, high)?;
            draws.push(Draw {
                cast_id: 0,
                purpose,
                first_counter: sample.first_counter,
                consumed: sample.consumed_words,
                low,
                high,
                value: sample.value,
            });
            Ok(sample.value as u32)
        })?;
        let record = OocRecord {
            npc,
            tick: 0,
            operation: OocOperation::Initialize,
            draws,
            callback: None,
            deliveries: Vec::new(),
        };
        let tip = record.tip([0; 32]);
        self.history.push(record);
        Ok(tip)
    }
    pub fn apply(
        &mut self,
        catalog: &EventAiRuntimeCatalog,
        npc: crate::NpcId,
        operation: OocOperation,
        diff: u32,
        draw: &mut impl FnMut(OocDraw) -> Result<u32, NativeError>,
    ) -> Result<Vec<TextSelection>, NativeError> {
        let events = catalog.catalog().ooc_events(npc.spawn)?;
        let mut timers = if operation == OocOperation::Initialize {
            if self
                .owners
                .get(&npc.spawn)
                .map_or(npc.generation != 0, |(old, _)| {
                    old.generation.checked_add(1) != Some(npc.generation)
                })
            {
                return Err(NativeError::InvalidState);
            }
            OocTimers::new(events, draw)?
        } else {
            let (id, snapshot) = self
                .owners
                .remove(&npc.spawn)
                .ok_or(NativeError::InvalidState)?;
            if id != npc {
                return Err(NativeError::InvalidState);
            }
            OocTimers::restore(events, snapshot)?
        };
        let selections = match operation {
            OocOperation::Initialize => Vec::new(),
            OocOperation::EnterCombat => {
                timers.enter_combat();
                Vec::new()
            }
            OocOperation::Reset => {
                timers.reset(draw)?;
                Vec::new()
            }
            OocOperation::Update { combat, evade } => timers.update(diff, combat, evade, draw)?,
        };
        self.owners.insert(npc.spawn, (npc, timers.into_snapshot()));
        Ok(selections)
    }
    pub fn validate(
        &self,
        catalog: &EventAiRuntimeCatalog,
        profile: &Profile,
        state: &State,
    ) -> Result<BTreeMap<u64, crate::Digest>, NativeError> {
        if self.history.len() > MAX_EVENT_AI_HISTORY {
            return Err(NativeError::TooLarge);
        }
        let mut expected = Self::default();
        let mut tips = BTreeMap::new();
        let mut previous_tick = 0;
        let mut end_counter = state.initial_cast_genesis.end_counter();
        let mut spans = Vec::new();
        for record in &self.history {
            if record.tick < previous_tick
                || record.tick > state.tick
                || record.draws.len() > MAX_EVENTS * 3
                || record.deliveries.len() > MAX_EVENTS
            {
                return Err(NativeError::InvalidCheckpoint);
            }
            if record.operation == OocOperation::Initialize {
                if record.npc.generation == 0 {
                    if record.tick != 0 { return Err(NativeError::InvalidCheckpoint); }
                } else if !state.event_ai.as_ref().is_some_and(|events| events.history.iter().any(|history| {
                    matches!(history, EventAiHistory::Respawn { replacement, tick, .. } if *replacement == record.npc && *tick == record.tick)
                })) {
                    return Err(NativeError::InvalidCheckpoint);
                }
            } else if record.tick == 0 {
                return Err(NativeError::InvalidCheckpoint);
            }
            if let Some(owner) = state
                .npcs
                .get(&record.npc.spawn)
                .and_then(|npc| npc.source_generation.as_ref())
            {
                let life = owner
                    .historical(record.npc)
                    .ok_or(NativeError::InvalidCheckpoint)?;
                if record.tick < life.creation_tick()
                    || life.death_tick().is_some_and(|death| record.tick > death)
                    || (record.operation == OocOperation::Initialize
                        && record.tick != life.creation_tick())
                {
                    return Err(NativeError::InvalidCheckpoint);
                }
            }
            previous_tick = record.tick;
            let mut index = 0;
            let selected = expected.apply(
                catalog,
                record.npc,
                record.operation,
                profile.tick_ms,
                &mut |request| {
                    let actual = record
                        .draws
                        .get(index)
                        .ok_or(NativeError::InvalidCheckpoint)?;
                    let (purpose, low, high) = ooc_draw(request, record.npc, record.tick);
                    let mut cursor = actual.first_counter;
                    let sample =
                        rng::sample_inclusive_v2(state.world, state.seed, &mut cursor, low, high)?;
                    if actual.cast_id != 0
                        || actual.purpose != purpose
                        || actual.low != low
                        || actual.high != high
                        || actual.value != sample.value
                        || actual.consumed != sample.consumed_words
                        || actual.first_counter < end_counter
                        || cursor > state.rng_counter
                    {
                        return Err(NativeError::InvalidCheckpoint);
                    }
                    spans.push((actual.first_counter, cursor));
                    if index > 0 && actual.first_counter != end_counter {
                        return Err(NativeError::InvalidCheckpoint);
                    }
                    end_counter = cursor;
                    index += 1;
                    Ok(sample.value as u32)
                },
            )?;
            if selected.is_empty() != record.callback.is_none() {
                return Err(NativeError::InvalidCheckpoint);
            }
            if index != record.draws.len() || selected.len() != record.deliveries.len() {
                return Err(NativeError::InvalidCheckpoint);
            }
            if let Some(callback) = &record.callback {
                callback.validate(profile, state)?;
                let OocOperation::Update { combat, evade } = record.operation else {
                    return Err(NativeError::InvalidCheckpoint);
                };
                if callback.npc != record.npc
                    || callback.tick != record.tick
                    || callback.kind
                        != (EventAiCallbackKind::Timer {
                            diff_ms: profile.tick_ms,
                            combat,
                            evade,
                        })
                {
                    return Err(NativeError::InvalidCheckpoint);
                }
                for (selection, delivery) in selected.iter().zip(&record.deliveries) {
                    let npc = state
                        .npcs
                        .get(&record.npc.spawn)
                        .ok_or(NativeError::InvalidCheckpoint)?;
                    let projected = npc.historical_profile(profile, record.npc)?;
                    let request =
                        EventAiAudienceRequest::from_callback(profile, callback, &projected)?;
                    if delivery.selection != *selection || delivery.audience.request != request {
                        return Err(NativeError::InvalidCheckpoint);
                    }
                    let delivered = request.validate(&delivery.audience.facts)?;
                    let gender = npc
                        .source_generation
                        .as_ref()
                        .map(|owner| {
                            owner
                                .historical(record.npc)
                                .map(|g| g.result().model.gender)
                                .ok_or(NativeError::InvalidCheckpoint)
                        })
                        .transpose()?;
                    let speech = delivered
                        .then(|| {
                            EventAiState::speech(catalog, callback, record.tick, *selection, gender)
                        })
                        .transpose()?;
                    if delivery.speech != speech {
                        return Err(NativeError::InvalidCheckpoint);
                    }
                }
            } else if !selected.is_empty() {
                return Err(NativeError::InvalidCheckpoint);
            }
            let tip = tips.entry(record.npc.spawn).or_insert([0; 32]);
            *tip = record.tip(*tip);
        }
        if let Some(events) = &state.event_ai {
            for history in &events.history {
                if let EventAiHistory::EnterCombat(evaluation) = history {
                    for action in &evaluation.actions {
                        for counter in
                            std::iter::once(action.chance_counter).chain(action.action_counter)
                        {
                            spans.push((
                                counter,
                                counter
                                    .checked_add(1)
                                    .ok_or(NativeError::InvalidCheckpoint)?,
                            ));
                        }
                    }
                }
            }
        }
        spans.sort_unstable();
        if spans.windows(2).any(|pair| pair[0].1 > pair[1].0) {
            return Err(NativeError::InvalidCheckpoint);
        }
        if self.owners != expected.owners {
            return Err(NativeError::InvalidCheckpoint);
        }
        for (spawn, npc) in &state.npcs {
            let has_timers = !catalog.catalog().ooc_events(*spawn)?.is_empty();
            if has_timers != self.owners.contains_key(spawn)
                || npc.events.event_ai_anchor.as_ref().is_none_or(|anchor| {
                    anchor.timer_tip != tips.get(spawn).copied().unwrap_or([0; 32])
                })
                || self.owners.get(spawn).is_some_and(|(id, _)| *id != npc.id)
            {
                return Err(NativeError::InvalidCheckpoint);
            }
        }
        Ok(tips)
    }
}
