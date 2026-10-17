// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, Spell::_handle_immediate_phase and Unit::Update.
//! Latest source release survives the SpellEvent, projectile and target lifetime.
use super::*;
use serde::{Deserialize, Serialize};

const POLICY: &str = "cmangos-8ec338a1-native-melee-reset-witness-v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::native) struct MeleeResetWitness {
    policy: String,
    profile: crate::Digest,
    domain: Domain,
    world: crate::WorldId,
    player: crate::PlayerId,
    cast_id: u64,
    target: crate::NpcId,
    prepared: ExecutionBinding,
    released: ExecutionBinding,
    origin: ai::Point,
    before_execution: spell::CastCheckpoint,
    execution: spell::CastCheckpoint,
    release_receipt: AuthorityReply,
    release_receipt_digest: crate::Digest,
    intervals_ms: [u32; 2],
}

impl MeleeResetWitness {
    pub(in crate::native) fn capture(
        profile: &Profile,
        state: &State,
        cast: &Cast,
        before_execution: spell::CastCheckpoint,
        release_receipt: &AuthorityReply,
        intervals_ms: [u32; 2],
    ) -> Result<Self, NativeError> {
        let released = cast
            .release_binding
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        if released.captured_ms != state.now_ms
            || cast.execution.last_update_ms() != state.now_ms
            || !matches!(cast.execution.state(), spell::CastState::Traveling { .. })
        {
            return Err(NativeError::InvalidState);
        }
        Ok(Self {
            policy: POLICY.into(),
            profile: profile.digest(),
            domain: state.domain,
            world: state.world,
            player: state.player,
            cast_id: cast.id,
            target: cast.target,
            prepared: cast.binding.clone(),
            released: released.clone(),
            origin: cast.origin,
            before_execution,
            execution: cast.execution.checkpoint(),
            release_receipt: release_receipt.clone(),
            release_receipt_digest: crate::digest(
                b"benilla-native-melee-release-receipt-v49\0",
                release_receipt,
            ),
            intervals_ms,
        })
    }

    pub(in crate::native) fn replay_work(&self) -> u64 {
        let Ok(context) = self.release_context() else {
            return u64::MAX;
        };
        let Some(scheduler) = &context.scheduler else {
            return u64::MAX;
        };
        self.prepared
            .replay_work()
            .saturating_add(self.released.replay_work())
            .saturating_add(scheduler.queue.queued.len() as u64)
            .saturating_add(scheduler.queue.detached.len() as u64)
    }

    fn release_context(&self) -> Result<&SourceSpellRequest, NativeError> {
        match &self.release_receipt.request.query {
            Query::SourceSpell(SourceSpellQuery::Semantics(context))
                if context.phase == SourceSpellPhase::ReleaseHit =>
            {
                Ok(context)
            }
            _ => Err(NativeError::InvalidCheckpoint),
        }
    }

    #[cfg(any(test, feature = "synthetic-native-tests"))]
    fn validate(&self, profile: &Profile, state: &State) -> Result<(), NativeError> {
        let identity = (profile.digest(), state.domain, state.world, state.player);
        if self.policy != POLICY
            || self.release_receipt_digest
                != crate::digest(
                    b"benilla-native-melee-release-receipt-v49\0",
                    &self.release_receipt,
                )
            || (self.profile, self.domain, self.world, self.player) != identity
            || self.cast_id == 0
            || self.cast_id >= state.next_cast_id
            || self.prepared.ability.id != self.released.ability.id
            || !state
                .npcs
                .get(&self.target.spawn)
                .is_some_and(|n| self.target.generation <= n.id.generation)
            || !interactions::spell_authority::resets_melee_timers(
                profile,
                self.released.ability.id,
            )?
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        for binding in [&self.prepared, &self.released] {
            binding.validate(
                profile,
                state.quest_progression.as_ref(),
                state.now_ms,
                identity,
            )?;
        }
        let execution = spell::MageProjectileCast::restore_with_release(
            self.execution.clone(),
            self.prepared.spec()?.clone(),
            Some(self.released.release_context(&self.prepared)?),
            self.released.captured_ms,
        )?;
        execution.validate_fixed_step(profile.clock_origin_ms, profile.tick_ms)?;
        let launch = execution
            .launch_context()
            .ok_or(NativeError::InvalidCheckpoint)?;
        if execution.created_ms() != self.prepared.captured_ms
            || execution.caster_id() != SOLO_UNIT
            || execution.target_id() != self.target.spawn
            || !matches!(execution.state(), spell::CastState::Traveling { .. })
            || self.intervals_ms != self.released.historical_melee_attack_times(profile)?
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        interactions::spell_authority::validate_historical_launch(
            profile,
            state
                .npcs
                .get(&self.target.spawn)
                .ok_or(NativeError::InvalidCheckpoint)?,
            &self.released,
            self.target,
            &launch,
        )?;
        self.validate_receipt(profile, state, &launch, &execution)?;
        self.validate_scheduler(profile, state, &execution)?;
        Ok(())
    }

    #[cfg(any(test, feature = "synthetic-native-tests"))]
    fn validate_receipt(
        &self,
        profile: &Profile,
        state: &State,
        launch: &spell::LaunchContext,
        execution: &spell::MageProjectileCast,
    ) -> Result<(), NativeError> {
        let context = self.release_context()?;
        let scheduler = context
            .scheduler
            .as_ref()
            .ok_or(NativeError::InvalidCheckpoint)?;
        let tick = self
            .released
            .captured_ms
            .checked_sub(profile.clock_origin_ms)
            .ok_or(NativeError::InvalidCheckpoint)?
            / u64::from(profile.tick_ms);
        let binding = Binding {
            world: self.world,
            tick,
            profile: self.profile,
            authority_policy: profile.authority_policy,
            geometry: profile.geometry_revision,
            npc: Some(self.target),
        };
        let bytes = serde_json::to_vec(&(
            &self.prepared,
            &Option::<ExecutionBinding>::None,
            self.cast_id,
            self.released.ability.id,
            self.target,
            self.origin,
            &self.before_execution,
            &self.released,
            scheduler,
        ))
        .map_err(|_| NativeError::InvalidCheckpoint)?;
        let history = crate::digest(b"benilla-source-spell-phase-history-v1\0", &bytes);
        let before = spell::MageProjectileCast::restore_with_release(
            self.before_execution.clone(),
            self.prepared.spec()?.clone(),
            None,
            self.released
                .captured_ms
                .checked_sub(u64::from(profile.tick_ms))
                .ok_or(NativeError::InvalidCheckpoint)?,
        )?;
        before.validate_fixed_step(profile.clock_origin_ms, profile.tick_ms)?;
        execution.validate_release_transition(&before, profile.tick_ms)?;
        if self.release_receipt.request.binding != binding
            || before.created_ms() != self.prepared.captured_ms
            || before.caster_id() != SOLO_UNIT
            || before.target_id() != self.target.spawn
            || !before.release_due(self.released.captured_ms, profile.tick_ms)
            || context.cast_id != self.cast_id
            || context.ability != self.released.ability.id
            || context.player_level != self.released.level
            || context.target_level
                != interactions::spell_authority::historical_target_profile(
                    profile,
                    state
                        .npcs
                        .get(&self.target.spawn)
                        .ok_or(NativeError::InvalidCheckpoint)?,
                    self.target,
                    self.released.captured_ms,
                )?
                .ai
                .level
            || context.history != history
            || context
                .player_position
                .distance_squared(context.target_position)?
                .bits()
                != launch.distance_squared.bits()
            || self.release_receipt.facts
                != Facts::SourceSpell(SourceSpellFacts::Semantics(SpellFacts {
                    other_failure: None,
                    allowed: true,
                    immune: false,
                    resist_chance: launch.resist_chance,
                    critical_chance: launch.critical_chance,
                }))
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok(())
    }

    #[cfg(any(test, feature = "synthetic-native-tests"))]
    fn validate_scheduler(
        &self,
        profile: &Profile,
        state: &State,
        execution: &spell::MageProjectileCast,
    ) -> Result<(), NativeError> {
        use benilla_sim::event_processor::{
            CallbackKind, EventKind, EventProcessor, MAX_CALLBACKS_PER_UPDATE,
        };
        let invalid = || NativeError::InvalidCheckpoint;
        let callback = self
            .release_context()?
            .scheduler
            .as_ref()
            .ok_or_else(invalid)?;
        let due = &callback.due;
        let queue = EventProcessor::restore(callback.queue.clone())?;
        let current = state.player_events.queue_snapshot();
        let event = queue.event(due.event_id).ok_or_else(invalid)?;
        if due.kind
            != (EventKind::SpellUpdate {
                cast_id: self.cast_id,
            })
            || due.callback != CallbackKind::Execute
            || due.diff_ms != Some(profile.tick_ms)
            || due.now_ms
                != self
                    .released
                    .captured_ms
                    .checked_sub(profile.clock_origin_ms)
                    .ok_or_else(invalid)?
            || callback.queue.clock_ms != due.now_ms
            || callback.queue.aborting
            || callback.callback_ordinal == 0
            || callback.callback_ordinal as usize > MAX_CALLBACKS_PER_UPDATE
            || due.scheduled_ms > due.now_ms
            || due.scheduled_ms != due.exec_time_ms
            || due
                .now_ms
                .checked_sub(u64::from(profile.tick_ms))
                .and_then(|t| t.checked_add(1))
                != Some(due.scheduled_ms)
            || due.add_time_ms
                != self
                    .prepared
                    .captured_ms
                    .checked_sub(profile.clock_origin_ms)
                    .and_then(|t| t.checked_sub(u64::from(profile.tick_ms)))
                    .ok_or_else(invalid)?
            || due.insertion_id < due.event_id
            || due.insertion_id >= callback.queue.next_insertion_id
            || callback.queue.next_insertion_id > current.next_insertion_id
            || callback.queue.next_event_id > current.next_event_id
            || event.kind != due.kind
            || event.to_abort
            || event.queue.is_some()
            || event.add_time_ms != due.add_time_ms
            || event.exec_time_ms != due.exec_time_ms
            || callback.queue.detached.len() != 1
            || state
                .player_events
                .spell_lifetime(self.cast_id)
                .is_some_and(|lifetime| {
                    lifetime.event_id != due.event_id
                        || lifetime.created_ms != self.prepared.captured_ms
                })
            || state.casts.get(&self.cast_id).is_some_and(|cast| {
                cast.target != self.target
                    || cast.binding != self.prepared
                    || cast.release_binding.as_ref() != Some(&self.released)
                    || cast.execution.launch_context() != execution.launch_context()
            })
        {
            return Err(invalid());
        }
        Ok(())
    }

    fn remaining_ms(&self, now: u64, diff: u32) -> Result<[u32; 2], NativeError> {
        // The release event precedes the BASE/OFF subtraction in this same Unit update.
        let elapsed = now
            .checked_sub(self.released.captured_ms)
            .and_then(|n| n.checked_add(u64::from(diff)))
            .ok_or(NativeError::InvalidState)?;
        Ok([
            u64::from(self.intervals_ms[0]).saturating_sub(elapsed) as u32,
            0,
        ])
    }
}

/// Validate owned idle state and exact countdown independently of outer hashes.
pub(in crate::native) fn validate_current(
    profile: &Profile,
    state: &State,
) -> Result<(), NativeError> {
    let Some(combat) = &state.combat else {
        return Ok(());
    };
    let genesis = profile.combat.as_ref().ok_or(NativeError::InvalidState)?;
    combat.validate_melee_timers(genesis)?;
    if interactions::spell_authority::resets_melee_timers(profile, 133)? {
        // The selected single active spell has positive effective source mana cost.
        // TakePower's retained latest-use timestamp anchors the latest release,
        // even after its projectile/holder/event has disappeared or expired.
        if combat.melee_reset.as_ref().map(|w| w.released.captured_ms)
            != state.resources.caster.last_mana_use_ms
        {
            return Err(NativeError::InvalidState);
        }
        if let Some(witness) = &combat.melee_reset {
            if (
                witness.profile,
                witness.domain,
                witness.world,
                witness.player,
            ) != (profile.digest(), state.domain, state.world, state.player)
                || state
                    .casts
                    .values()
                    .filter_map(|c| c.release_binding.as_ref())
                    .any(|b| b.captured_ms > witness.released.captured_ms)
            {
                return Err(NativeError::InvalidState);
            }
        }
    } else if combat.melee_reset.is_some() {
        return Err(NativeError::InvalidState);
    }
    let expected = match &combat.melee_reset {
        None => [0, 0],
        Some(witness) => witness.remaining_ms(state.now_ms, profile.tick_ms)?,
    };
    if combat.melee_timers.remaining_ms() != expected {
        return Err(NativeError::InvalidState);
    }
    Ok(())
}

#[cfg(any(test, feature = "synthetic-native-tests"))]
pub(in crate::native) fn validate_restored(
    profile: &Profile,
    state: &State,
) -> Result<(), NativeError> {
    validate_current(profile, state).map_err(|_| NativeError::InvalidCheckpoint)?;
    if let Some(witness) = state.combat.as_ref().and_then(|c| c.melee_reset.as_ref()) {
        witness.validate(profile, state)?;
    }
    Ok(())
}
