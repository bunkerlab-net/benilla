// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f.
// Copyright CMaNGOS Project contributors; source GPL-2.0-or-later.
//! Per-creature Unit event ownership. Death/respawn retains the Unit processor.
use super::*;
use benilla_sim::event_processor::{
    BasicEventSnapshot, EventCallbackSnapshot, EventKind, EventProcessor, EventProcessorSnapshot,
};
use serde::{Deserialize, Serialize};

pub const NPC_EVENT_POLICY: &str = "cmangos-8ec338a1-native-npc-events-v2";

/// The generation guards external handles even though source respawn retains
/// the underlying Unit, its clock, and its monotonic event allocator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NpcEventHandle {
    pub world: crate::WorldId,
    pub npc: crate::NpcId,
    pub event_id: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NpcEvents {
    pub(super) event_ai_anchor: Option<EventAiCallbackAnchor>,
    world: crate::WorldId,
    npc: crate::NpcId,
    pub(super) queue: EventProcessor,
    pub(super) spells: BTreeMap<u64, NpcSpellLifetime>,
    pub(super) next_cast_id: u64,
    pub(super) notify: Option<u64>,
    pub(super) can_aggro: bool,
    /// Creature::Update's DEAD branch omits Unit::Update, including respawn.
    pub(super) skipped_updates: u64,
    /// Read-only view of the lent transaction, never an independently owned queue.
    pub(super) callback: Option<EventCallbackSnapshot>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NpcEventsSnapshot {
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    event_ai_anchor: Option<EventAiCallbackAnchor>,
    policy: String,
    world: crate::WorldId,
    npc: crate::NpcId,
    queue: EventProcessorSnapshot,
    spells: BTreeMap<u64, NpcSpellLifetime>,
    next_cast_id: u64,
    notify: Option<u64>,
    can_aggro: bool,
    skipped_updates: u64,
}
impl NpcEvents {
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn new(
        world: crate::WorldId,
        npc: crate::NpcId,
        profile: &Profile,
    ) -> Result<Self, NativeError> {
        let mut result = Self {
            event_ai_anchor: profile.event_ai().map(|_| EventAiCallbackAnchor::default()),
            world,
            npc,
            queue: EventProcessor::new(),
            spells: BTreeMap::new(),
            next_cast_id: 1,
            notify: None,
            can_aggro: false,
            skipped_updates: 0,
            callback: None,
        };
        if let Some(policy) = &profile.source_motion {
            result.schedule_notify(policy.creature_initial_aggro_delay_ms, false)?;
        }
        Ok(result)
    }
    pub fn clock_ms(&self) -> u64 {
        self.callback
            .as_ref()
            .map_or_else(|| self.queue.clock_ms(), |c| c.queue.clock_ms)
    }
    pub fn queue_snapshot(&self) -> EventProcessorSnapshot {
        self.callback
            .as_ref()
            .map_or_else(|| self.queue.snapshot(), |c| c.queue.clone())
    }
    pub fn spells(&self) -> impl Iterator<Item = &NpcSpellLifetime> {
        self.spells.values()
    }
    pub fn can_aggro(&self) -> bool {
        self.can_aggro
    }
    pub fn notify_handle(&self) -> Option<NpcEventHandle> {
        self.notify.map(|event_id| NpcEventHandle {
            world: self.world,
            npc: self.npc,
            event_id,
        })
    }
    pub fn event(&self, handle: NpcEventHandle) -> Option<BasicEventSnapshot> {
        if handle.world != self.world || handle.npc != self.npc {
            return None;
        }
        let snapshot = self.queue_snapshot();
        snapshot
            .queued
            .into_iter()
            .chain(snapshot.detached)
            .find(|event| event.event_id == handle.event_id)
    }
    pub(super) fn schedule_notify(
        &mut self,
        delay_ms: u32,
        forced: bool,
    ) -> Result<(), NativeError> {
        // The source pointer remains set during Execute's visitor. Relocation
        // caused synchronously by the visitor must not create another notify.
        if self.notify.is_some() && !forced {
            return Ok(());
        }
        if self.callback.is_some() {
            return Err(NativeError::UnsupportedMechanic(
                "NPC event queue mutation during notify visitor",
            ));
        }
        let mut queue = self.queue.clone();
        if let Some(id) = self.notify {
            if queue.kill_event(id).is_none() {
                return Err(NativeError::InvalidState);
            }
        }
        let id = queue.next_event_id();
        queue.add_event(
            id,
            EventKind::AiNotify,
            queue.calculate_time(u64::from(delay_ms))?,
        )?;
        self.queue = queue;
        self.notify = Some(id);
        Ok(())
    }
    pub(super) fn entered_combat(&mut self) -> Result<(), NativeError> {
        if !self.can_aggro {
            if self.callback.is_some() {
                return Err(NativeError::InvalidState);
            }
            if let Some(id) = self.notify {
                if self.queue.kill_event(id).is_none() {
                    return Err(NativeError::InvalidState);
                }
            }
            self.notify = None;
            self.can_aggro = true;
        }
        Ok(())
    }
    pub(super) fn respawned(&mut self, delay_ms: u32) -> Result<(), NativeError> {
        self.schedule_notify(delay_ms, true)?;
        self.can_aggro = false;
        Ok(())
    }
    pub(super) fn rebind_respawn(&mut self, npc: crate::NpcId) -> Result<(), NativeError> {
        if npc.spawn != self.npc.spawn
            || self.npc.generation.checked_add(1) != Some(npc.generation)
            || self.callback.is_some()
            || !self.spells.is_empty()
        {
            return Err(NativeError::InvalidState);
        }
        self.npc = npc;
        Ok(())
    }
    pub(super) fn snapshot(&self) -> NpcEventsSnapshot {
        NpcEventsSnapshot {
            event_ai_anchor: self.event_ai_anchor.clone(),
            policy: NPC_EVENT_POLICY.into(),
            world: self.world,
            npc: self.npc,
            queue: self.queue_snapshot(),
            spells: self.spells.clone(),
            next_cast_id: self.next_cast_id,
            notify: self.notify,
            can_aggro: self.can_aggro,
            skipped_updates: self.skipped_updates,
        }
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn restore(
        snapshot: NpcEventsSnapshot,
        world: crate::WorldId,
        npc: crate::NpcId,
        player: crate::PlayerId,
        profile: &Profile,
        ai: &ai::SoloCreatureAi,
        tick: u64,
    ) -> Result<Self, NativeError> {
        let invalid = || NativeError::InvalidCheckpoint;
        let elapsed = tick
            .checked_sub(snapshot.skipped_updates)
            .ok_or_else(invalid)?
            .checked_mul(u64::from(profile.tick_ms))
            .ok_or_else(invalid)?;
        if snapshot.event_ai_anchor.is_some() != profile.event_ai().is_some()
            || snapshot.policy != NPC_EVENT_POLICY
            || snapshot.world != world
            || snapshot.npc != npc
            || snapshot.queue.clock_ms != elapsed
            || snapshot.skipped_updates < npc.generation
            || (npc.generation == 0
                && ai.phase() != ai::Phase::Despawned
                && snapshot.skipped_updates != 0)
            || snapshot.queue.aborting
            || !snapshot.queue.detached.is_empty()
            || snapshot.queue.queued.len() != usize::from(snapshot.notify.is_some()) + snapshot.spells.len()
            // Notify and bounded finished instant spells are added once, never re-added.
            || snapshot.queue.next_event_id != snapshot.queue.next_insertion_id
            || snapshot.queue.next_event_id > 2 + tick.saturating_mul(MAX_INPUTS_PER_TICK as u64 + 4)
        {
            return Err(invalid());
        }
        // The current DEAD interval is independently bound by the AI lifecycle.
        // Older generations retain their accumulated skipped updates; restore
        // validates consistency, not authenticated history of those generations.
        let current_skips = if let Some(at) = ai.despawned_at_ms() {
            Some(
                profile
                    .clock_origin_ms
                    .checked_add(
                        tick.checked_mul(u64::from(profile.tick_ms))
                            .ok_or_else(invalid)?,
                    )
                    .and_then(|now| now.checked_sub(at))
                    .ok_or_else(invalid)?
                    / u64::from(profile.tick_ms),
            )
        } else {
            None
        };
        if current_skips.is_some_and(|skips| {
            snapshot.skipped_updates < skips.saturating_add(npc.generation)
                || (npc.generation == 0 && snapshot.skipped_updates != skips)
        }) {
            return Err(invalid());
        }
        let result = Self {
            event_ai_anchor: snapshot.event_ai_anchor,
            world,
            npc,
            queue: EventProcessor::restore(snapshot.queue)?,
            spells: snapshot.spells,
            next_cast_id: snapshot.next_cast_id,
            notify: snapshot.notify,
            can_aggro: snapshot.can_aggro,
            skipped_updates: snapshot.skipped_updates,
            callback: None,
        };
        if let Some(policy) = &profile.source_motion {
            if result.queue.next_event_id() < npc.generation.checked_add(2).ok_or_else(invalid)?
                || (!matches!(ai.phase(), ai::Phase::Corpse | ai::Phase::Despawned)
                    && !result.can_aggro
                    && (result.notify.is_none()
                        || (npc.generation == 0 && result.notify != Some(1))))
                || (matches!(ai.phase(), ai::Phase::Combat | ai::Phase::ReturningHome)
                    && !result.can_aggro)
            {
                return Err(invalid());
            }
            if let Some(id) = result.notify {
                let event = result.queue.event(id).ok_or_else(invalid)?;
                let delay = event
                    .exec_time_ms
                    .checked_sub(event.add_time_ms)
                    .ok_or_else(invalid)?;
                if event.kind != EventKind::AiNotify
                    || event.to_abort
                    || event.queue.is_none_or(|key| key.insertion_id != id)
                    || !event.add_time_ms.is_multiple_of(u64::from(profile.tick_ms))
                    || event.exec_time_ms < result.clock_ms()
                    || (event.exec_time_ms == result.clock_ms()
                        && event.add_time_ms < result.clock_ms())
                    || ![
                        u64::from(policy.creature_initial_aggro_delay_ms),
                        u64::from(policy.relocation_aggro_delay_ms),
                    ]
                    .contains(&delay)
                    || (id == 1
                        && (result.can_aggro
                            || event.add_time_ms != 0
                            || delay != u64::from(policy.creature_initial_aggro_delay_ms)))
                {
                    return Err(invalid());
                }
            }
        } else if result.notify.is_some() || result.can_aggro || result.queue.next_event_id() != 1 {
            return Err(invalid());
        }
        result.validate_spells(player, profile, tick)?;
        if tick == 0 && result != Self::new(world, npc, profile)? {
            return Err(invalid());
        }
        Ok(result)
    }
    /// Unit destruction, not ordinary death, corpse removal or durable suspend.
    pub(super) fn teardown(&mut self) -> Result<(), NativeError> {
        if self.callback.is_some() {
            return Err(NativeError::InvalidState);
        }
        let mut transaction = self.queue.begin_kill_all_events(true);
        while let Some(due) = transaction.pop_due()? {
            match due.kind {
                EventKind::AiNotify if self.notify == Some(due.event_id) => {
                    self.notify = None; // UnitVisitObjectsInRangeNotifyEvent::Abort
                }
                EventKind::SpellUpdate { cast_id } => {
                    let lifetime = self
                        .spells
                        .remove(&cast_id)
                        .ok_or(NativeError::InvalidState)?;
                    if lifetime.event_id != due.event_id || lifetime.terminal.is_none() {
                        return Err(NativeError::InvalidState);
                    }
                }
                _ => return Err(NativeError::InvalidState),
            }
            transaction.finish_abort(true)?;
        }
        transaction.commit()?;
        Ok(())
    }
}

impl NpcEvents {
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn start_spell(
        &mut self,
        target: crate::PlayerId,
        spell_id: u32,
        source_binding: crate::Digest,
        tick: u64,
        now_ms: u64,
        after_npc_update: bool,
    ) -> Result<NpcSpellLifetime, NativeError> {
        if self.callback.is_some() || self.spells.len() >= MAX_CASTS {
            return Err(NativeError::UnsupportedMechanic("nested NPC SpellStart"));
        }
        let cast_id = self.next_cast_id;
        let next = cast_id.checked_add(1).ok_or(NativeError::Limit)?;
        let lifetime = NpcSpellLifetime {
            cast_id,
            event_id: self.queue.next_event_id(),
            caster: self.npc,
            target,
            spell_id,
            source_binding,
            started_tick: tick,
            started_at_ms: now_ms,
            event_add_ms: self.queue.clock_ms(),
            after_npc_update,
            terminal: None,
            applied_holder: None,
        };
        // SpellStart owns an event before the very first PreCastCheck.
        self.queue.add_event(
            lifetime.event_id,
            EventKind::SpellUpdate { cast_id },
            self.queue.calculate_time(1)?,
        )?;
        self.next_cast_id = next;
        self.spells.insert(cast_id, lifetime.clone());
        Ok(lifetime)
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    fn validate_spells(
        &self,
        player: crate::PlayerId,
        profile: &Profile,
        tick: u64,
    ) -> Result<(), NativeError> {
        let invalid = || NativeError::InvalidCheckpoint;
        let triggers = profile
            .player_slow
            .as_ref()
            .map(|g| g.fixture_triggers.as_slice())
            .unwrap_or_default();
        let elapsed: Vec<_> = triggers
            .iter()
            .filter(|t| t.caster.spawn == self.npc.spawn && t.tick <= tick)
            .collect();
        let minimum_event_id = self
            .next_cast_id
            .checked_add(self.npc.generation)
            .and_then(|id| id.checked_add(1))
            .ok_or_else(invalid)?;
        if self.next_cast_id != elapsed.len() as u64 + 1
            || self.spells.len() > MAX_CASTS
            || (profile.source_motion.is_some() && self.queue.next_event_id() < minimum_event_id)
        {
            return Err(invalid());
        }
        let expected: Vec<_> = elapsed
            .iter()
            .enumerate()
            .filter(|(_, t)| t.tick == tick && t.after_npc_update)
            .collect();
        if self.spells.len() != expected.len() {
            return Err(invalid());
        }
        let retained_count = expected.len() as u64;
        let first_retained_event = self
            .queue
            .next_event_id()
            .checked_sub(retained_count)
            .ok_or_else(invalid)?;
        for (ordinal, (index, trigger)) in expected.into_iter().enumerate() {
            let cast_id = index as u64 + 1;
            let lifetime = self.spells.get(&cast_id).ok_or_else(invalid)?;
            let event = self.queue.event(lifetime.event_id).ok_or_else(invalid)?;
            let recipe = profile
                .player_slow
                .as_ref()
                .ok_or_else(invalid)?
                .triggered_recipe(
                    trigger.spell_id,
                    profile.npc(self.npc.spawn).ok_or_else(invalid)?.ai.level,
                )?;
            if lifetime.event_id != first_retained_event + ordinal as u64
                || lifetime.cast_id != cast_id
                || lifetime.caster != self.npc
                || trigger.caster != self.npc
                || lifetime.target != player
                || lifetime.spell_id != trigger.spell_id
                || lifetime.source_binding != recipe.binding
                || lifetime.started_tick != tick
                || lifetime.started_at_ms
                    != profile
                        .clock_origin_ms
                        .checked_add(
                            tick.checked_mul(u64::from(profile.tick_ms))
                                .ok_or_else(invalid)?,
                        )
                        .ok_or_else(invalid)?
                || lifetime.event_add_ms != self.clock_ms()
                || !lifetime.after_npc_update
                || lifetime.terminal.is_none()
                || matches!(
                    lifetime.terminal,
                    Some(NpcSpellTerminal::Hit {
                        effect_applied: true
                    })
                ) != lifetime.applied_holder.is_some()
                || event.kind != (EventKind::SpellUpdate { cast_id })
                || event.to_abort
                || event.add_time_ms != lifetime.event_add_ms
                || event.exec_time_ms != event.add_time_ms.checked_add(1).ok_or_else(invalid)?
                || event
                    .queue
                    .is_none_or(|q| q.insertion_id != lifetime.event_id)
            {
                return Err(invalid());
            }
        }
        // Every retained spell has its sole owning queue entry and vice versa.
        for event in self.queue.snapshot().queued {
            if let EventKind::SpellUpdate { cast_id } = event.kind {
                if self
                    .spells
                    .get(&cast_id)
                    .is_none_or(|s| s.event_id != event.event_id)
                {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
}
