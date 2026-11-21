// SPDX-License-Identifier: GPL-3.0-or-later
// Spell.h:589 and Spell.cpp:3444-3453 at CMaNGOS Classic 8ec338a1.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub(in crate::native) enum MeleeResetWitness {
    Projectile(Box<ProjectileResetWitness>),
    Instant(Box<InstantResetWitness>),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::native) struct InstantResetWitness {
    cast_id: u64,
    profile: crate::Digest,
    domain: Domain,
    world: crate::WorldId,
    player: crate::PlayerId,
    binding: quest_progression::MeleeExecutionBinding,
    intervals_ms: [u32; 2],
}
impl MeleeResetWitness {
    pub(in crate::native) fn capture(
        profile: &Profile,
        state: &State,
        cast: &Cast,
        before: spell::CastCheckpoint,
        reply: &AuthorityReply,
        intervals: [u32; 2],
    ) -> Result<Self, NativeError> {
        Ok(Self::Projectile(Box::new(ProjectileResetWitness::capture(
            profile, state, cast, before, reply, intervals,
        )?)))
    }
    pub(in crate::native) fn capture_frost(
        profile: &Profile,
        state: &State,
        cast_id: u64,
        intervals_ms: [u32; 2],
    ) -> Result<Self, NativeError> {
        Ok(Self::Instant(Box::new(InstantResetWitness {
            cast_id,
            profile: profile.digest(),
            domain: state.domain,
            world: state.world,
            player: state.player,
            binding: quest_progression::MeleeExecutionBinding::capture(profile, state)?,
            intervals_ms,
        })))
    }
    pub(in crate::native) fn source_origin(&self) -> (u64, [u32; 2]) {
        match self {
            Self::Projectile(w) => w.source_origin(),
            Self::Instant(w) => (w.binding.captured_ms(), w.intervals_ms),
        }
    }
    pub(in crate::native) fn release_ms(&self) -> u64 {
        self.source_origin().0
    }
    pub(in crate::native) fn identity(
        &self,
    ) -> (crate::Digest, Domain, crate::WorldId, crate::PlayerId) {
        match self {
            Self::Projectile(w) => (w.profile, w.domain, w.world, w.player),
            Self::Instant(w) => (w.profile, w.domain, w.world, w.player),
        }
    }
    pub(in crate::native) fn replay_work(&self) -> u64 {
        match self {
            Self::Projectile(w) => w.replay_work(),
            Self::Instant(w) => w.binding.replay_work(),
        }
    }
    pub(in crate::native) fn release_queue(
        &self,
        now: u64,
        actual: &benilla_sim::event_processor::EventCallbackSnapshot,
    ) -> Result<benilla_sim::event_processor::EventProcessor, NativeError> {
        match self {
            Self::Projectile(w) => w.release_queue(now, actual),
            Self::Instant(_) => Err(NativeError::InvalidState),
        }
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(in crate::native) fn validate(
        &self,
        profile: &Profile,
        state: &State,
    ) -> Result<(), NativeError> {
        match self {
            Self::Projectile(w) => w.validate(profile, state),
            Self::Instant(w) => {
                if self.identity() != (profile.digest(), state.domain, state.world, state.player)
                    || w.cast_id == 0
                    || w.cast_id >= state.next_cast_id
                    || !interactions::spell_authority::resets_melee_timers(profile, 168)?
                {
                    return Err(NativeError::InvalidCheckpoint);
                }
                w.binding.validate_history(
                    profile,
                    state.quest_progression.as_ref(),
                    state.now_ms,
                    self.identity(),
                )?;
                let expected = profile
                    .combat
                    .as_ref()
                    .ok_or(NativeError::InvalidCheckpoint)?
                    .source_attack_times_for_recipe(w.binding.recipe())?;
                if w.intervals_ms != expected {
                    return Err(NativeError::InvalidCheckpoint);
                }
                Ok(())
            }
        }
    }
    pub(super) fn remaining_ms(&self, now: u64, diff: u32) -> Result<[u32; 2], NativeError> {
        let (at, intervals) = self.source_origin();
        let elapsed = now
            .checked_sub(at)
            .and_then(|n| n.checked_add(u64::from(diff)))
            .ok_or(NativeError::InvalidState)?;
        Ok([u64::from(intervals[0]).saturating_sub(elapsed) as u32, 0])
    }
}
