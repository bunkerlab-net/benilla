// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded last-writer proof for the selected mainhand lifecycle and countdowns.
//! SourceCause::replay supplies source-derived values and write subphases; the
//! checkpoint never serializes an unchecked mask, duration or eligibility bit.
use super::context::SourceContext;
use super::source::SourceCause;
use crate::native::{Domain, Draw, NativeError, Profile, State};
use crate::{Digest, NpcId, PlayerId, WorldId};
use serde::{Deserialize, Serialize};

const POLICY: &str = "cmangos-8ec338a1-player-mainhand-last-causes-v1";
const SLOTS: usize = 4;
const MAX_TIMER_MS: u32 = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(super) enum Phase {
    PreUnit,
    PostUnit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CauseKey {
    pub tick: u64,
    pub phase: Phase,
    pub action: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WriteStamp {
    pub cause: CauseKey,
    pub subphase: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Lifecycle {
    pub melee_attacking: bool,
    pub victim: Option<NpcId>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Write<T> {
    pub stamp: WriteStamp,
    pub value: T,
}
/// Transient output of the private source re-execution, deliberately not serde.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ReplayedCause {
    pub key: CauseKey,
    pub lifecycle: Option<Write<Lifecycle>>,
    pub base: Option<Write<u32>>,
    pub off: Option<Write<u32>>,
    pub last_error: Option<Write<u8>>,
}
impl ReplayedCause {
    fn stamps(&self) -> [Option<WriteStamp>; SLOTS] {
        [
            self.lifecycle.map(|write| write.stamp),
            self.base.map(|write| write.stamp),
            self.off.map(|write| write.stamp),
            self.last_error.map(|write| write.stamp),
        ]
    }
    fn check(&self, state: &State, phase: Phase) -> Result<(), NativeError> {
        if self.key.tick == 0
            || self.key.action == 0
            || self.key.tick > state.tick
            || (self.key.tick == state.tick && self.key.phase > phase)
            || self
                .stamps()
                .into_iter()
                .flatten()
                .any(|stamp| stamp.cause != self.key || stamp.subphase == 0 || stamp.subphase > 16)
            || self
                .lifecycle
                .is_some_and(|write| write.value.melee_attacking != write.value.victim.is_some())
            || self.base.is_some_and(|write| write.value > MAX_TIMER_MS)
            || self.off.is_some_and(|write| write.value > MAX_TIMER_MS)
            || self.last_error.is_some_and(|write| write.value > 3)
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct MeleeFields {
    pub lifecycle: Lifecycle,
    pub remaining_ms: [u32; 2],
    pub last_error: u8,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    policy: String,
    profile: Digest,
    domain: Domain,
    world: WorldId,
    player: PlayerId,
}
impl Owner {
    fn capture(profile: &Profile, state: &State) -> Self {
        Self {
            policy: POLICY.into(),
            profile: profile.digest(),
            domain: state.domain,
            world: state.world,
            player: state.player,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::native) struct PlayerMeleeProof {
    owner: Owner,
    frontier: Option<WriteStamp>,
    /// Lifecycle, BASE, OFF, last-error. None is the explicit source genesis.
    origins: [Option<WriteStamp>; SLOTS],
    #[serde(deserialize_with = "deserialize_causes")]
    causes: Vec<SourceCause>,
}

fn deserialize_causes<'de, D>(deserializer: D) -> Result<Vec<SourceCause>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Causes;
    impl<'de> serde::de::Visitor<'de> for Causes {
        type Value = Vec<SourceCause>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "at most four referenced melee source causes")
        }
        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            if seq.size_hint().is_some_and(|count| count > SLOTS) {
                return Err(serde::de::Error::custom("melee cause count"));
            }
            let mut causes = Vec::new();
            while causes.len() < SLOTS {
                let Some(cause) = seq.next_element()? else {
                    return Ok(causes);
                };
                causes.push(cause);
            }
            if seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(serde::de::Error::custom("melee cause count"));
            }
            Ok(causes)
        }
    }
    deserializer.deserialize_seq(Causes)
}

impl PlayerMeleeProof {
    pub(super) fn validate_latest_reset(
        &self,
        reset: &crate::native::melee_reset::MeleeResetWitness,
        tick: u64,
    ) -> Result<(), NativeError> {
        let base = self.origins[1].ok_or(NativeError::InvalidCheckpoint)?;
        if (base.cause.tick, base.cause.phase) < (tick, Phase::PreUnit) {
            return Err(NativeError::InvalidCheckpoint);
        }
        if (base.cause.tick, base.cause.phase) == (tick, Phase::PreUnit)
            && !self.causes.iter().any(|cause| matches!(cause, SourceCause::SpellReset { key, witness } if *key == base.cause && witness.as_ref() == reset)) {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok(())
    }

    pub(super) fn validate_closed(&self, current: &State) -> Result<(), NativeError> {
        for cause in &self.causes {
            cause.validate_closed(current)?;
        }
        Ok(())
    }

    /// A retained reactive may refer to an older, pruned valid source cause.
    /// For every still-retained same action, require its exact numeric origin;
    /// a future action or same ordinal attached to another operation is refused.
    pub(super) fn validate_reactive_origin(
        &self,
        key: CauseKey,
        context: &SourceContext,
        draws: &[Draw],
    ) -> Result<(), NativeError> {
        let frontier = self.frontier.ok_or(NativeError::InvalidCheckpoint)?;
        if !key_within_frontier(key, frontier.cause) {
            return Err(NativeError::InvalidCheckpoint);
        }
        let mut matched = false;
        for cause in &self.causes {
            let (retained_key, origin) = cause.retained_origin();
            if retained_key.action == key.action {
                matched = true;
                let (retained_context, retained_draws) =
                    origin.ok_or(NativeError::InvalidCheckpoint)?;
                if retained_key != key
                    || retained_context != context
                    || retained_draws.get(..draws.len()) != Some(draws)
                {
                    return Err(NativeError::InvalidCheckpoint);
                }
            }
        }
        if key.action == frontier.cause.action && !matched {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok(())
    }

    pub(super) fn next_action(&self) -> Result<u64, NativeError> {
        self.frontier.map_or(Ok(1), |frontier| {
            frontier
                .cause
                .action
                .checked_add(1)
                .ok_or(NativeError::InvalidState)
        })
    }

    pub(in crate::native) fn replay_work(&self) -> u64 {
        self.causes
            .iter()
            .fold(0_u64, |sum, cause| sum.saturating_add(cause.replay_work()))
    }

    pub(super) fn genesis(profile: &Profile, state: &State) -> Self {
        Self {
            owner: Owner::capture(profile, state),
            frontier: None,
            origins: [None; SLOTS],
            causes: Vec::new(),
        }
    }

    fn replay(
        &self,
        profile: &Profile,
        state: &State,
        phase: Phase,
    ) -> Result<Vec<ReplayedCause>, NativeError> {
        if self.owner != Owner::capture(profile, state) || self.causes.len() > SLOTS {
            return Err(NativeError::InvalidCheckpoint);
        }
        let mut replayed = Vec::with_capacity(self.causes.len());
        for cause in &self.causes {
            let writes = cause.replay(profile, state)?;
            writes.check(state, phase)?;
            if writes.stamps().into_iter().all(|stamp| stamp.is_none())
                || replayed.last().is_some_and(|prior: &ReplayedCause| {
                    prior.key >= writes.key || prior.key.action >= writes.key.action
                })
            {
                return Err(NativeError::InvalidCheckpoint);
            }
            replayed.push(writes);
        }
        validate_joint(&self.origins, self.frontier, &replayed)?;
        Ok(replayed)
    }

    /// Source replay chooses all values/subphases. Empty writes are a real no-op
    /// and must not advance the frontier (notably range-error zero).
    pub(super) fn apply(
        &self,
        profile: &Profile,
        state: &State,
        phase: Phase,
        cause: SourceCause,
    ) -> Result<Self, NativeError> {
        let previous = self.replay(profile, state, phase)?;
        let writes = cause.replay(profile, state)?;
        writes.check(state, phase)?;
        let stamps = writes.stamps();
        if stamps.iter().all(Option::is_none) {
            return Ok(self.clone());
        }
        if self.frontier.is_some_and(|frontier| {
            writes.key <= frontier.cause
                || writes.key.action <= frontier.cause.action
                || stamps.iter().flatten().any(|stamp| *stamp <= frontier)
        }) {
            return Err(NativeError::InvalidState);
        }
        let mut next = self.clone();
        for (slot, stamp) in stamps.into_iter().enumerate() {
            if stamp.is_some() {
                next.origins[slot] = stamp;
            }
        }
        next.frontier = next.origins.iter().copied().flatten().max();
        next.causes = self
            .causes
            .iter()
            .zip(&previous)
            .filter(|(_, old)| next.origins.iter().flatten().any(|s| s.cause == old.key))
            .map(|(cause, _)| cause.clone())
            .collect();
        next.causes.push(cause);
        next.replay(profile, state, phase)?;
        Ok(next)
    }

    pub(super) fn expected(
        &self,
        profile: &Profile,
        state: &State,
        phase: Phase,
    ) -> Result<MeleeFields, NativeError> {
        let causes = self.replay(profile, state, phase)?;
        let writer = |slot: usize| {
            self.origins[slot]
                .and_then(|origin| causes.iter().find(|cause| cause.key == origin.cause))
        };
        let lifecycle = writer(0).and_then(|cause| cause.lifecycle).map_or(
            Lifecycle {
                melee_attacking: false,
                victim: None,
            },
            |write| write.value,
        );
        let mut remaining_ms = [0; 2];
        for (hand, slot) in [(0, 1), (1, 2)] {
            if let Some(cause) = writer(slot) {
                let write = if hand == 0 { cause.base } else { cause.off }
                    .ok_or(NativeError::InvalidCheckpoint)?;
                remaining_ms[hand] = remaining(write, profile.tick_ms, state.tick, phase)?;
            }
        }
        let last_error = writer(3)
            .and_then(|cause| cause.last_error)
            .map_or(0, |write| write.value);
        Ok(MeleeFields {
            lifecycle,
            remaining_ms,
            last_error,
        })
    }

    pub(super) fn validate(
        &self,
        profile: &Profile,
        state: &State,
        phase: Phase,
        actual: MeleeFields,
    ) -> Result<(), NativeError> {
        if self.expected(profile, state, phase)? != actual {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok(())
    }
}

fn key_within_frontier(key: CauseKey, frontier: CauseKey) -> bool {
    key.action != 0 && key.tick != 0 && key <= frontier && key.action <= frontier.action
}

fn validate_joint(
    origins: &[Option<WriteStamp>; SLOTS],
    frontier: Option<WriteStamp>,
    replayed: &[ReplayedCause],
) -> Result<(), NativeError> {
    // Every origin is an actual write of that same slot by its exact cause.
    for (slot, origin) in origins.iter().enumerate() {
        if let Some(origin) = origin {
            let cause = replayed
                .iter()
                .find(|cause| cause.key == origin.cause)
                .ok_or(NativeError::InvalidCheckpoint)?;
            if cause.stamps()[slot] != Some(*origin) {
                return Err(NativeError::InvalidCheckpoint);
            }
        }
    }
    // Joint frontier: a retained retry/swing cannot lose one paired write.
    // A newer origin is valid only because its writer was checked above.
    for cause in replayed {
        if !origins.iter().flatten().any(|s| s.cause == cause.key) {
            return Err(NativeError::InvalidCheckpoint);
        }
        for (slot, write) in cause.stamps().iter().enumerate() {
            if let Some(write) = write {
                if origins[slot].is_none_or(|origin| origin < *write) {
                    return Err(NativeError::InvalidCheckpoint);
                }
            }
        }
    }
    if frontier != origins.iter().copied().flatten().max() {
        return Err(NativeError::InvalidCheckpoint);
    }
    Ok(())
}

fn remaining(write: Write<u32>, diff: u32, tick: u64, phase: Phase) -> Result<u32, NativeError> {
    // Count completed Unit updates before the write and at the observation.
    // PreUnit spell writes include that tick's subtraction; PostUnit swing
    // resets do not. No wall clock or reconstructed fresh deadline participates.
    let completed = match phase {
        Phase::PreUnit => tick.checked_sub(1),
        Phase::PostUnit => Some(tick),
    }
    .ok_or(NativeError::InvalidCheckpoint)?;
    let before_write = match write.stamp.cause.phase {
        Phase::PreUnit => write.stamp.cause.tick.checked_sub(1),
        Phase::PostUnit => Some(write.stamp.cause.tick),
    }
    .ok_or(NativeError::InvalidCheckpoint)?;
    let elapsed = completed
        .checked_sub(before_write)
        .and_then(|ticks| ticks.checked_mul(u64::from(diff)))
        .ok_or(NativeError::InvalidCheckpoint)?;
    Ok(u64::from(write.value).saturating_sub(elapsed) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp(action: u64, subphase: u8) -> WriteStamp {
        WriteStamp {
            cause: CauseKey {
                tick: action,
                phase: Phase::PostUnit,
                action,
            },
            subphase,
        }
    }

    fn retry(action: u64) -> ReplayedCause {
        ReplayedCause {
            key: stamp(action, 1).cause,
            lifecycle: None,
            base: Some(Write {
                stamp: stamp(action, 1),
                value: 100,
            }),
            off: Some(Write {
                stamp: stamp(action, 2),
                value: 100,
            }),
            last_error: Some(Write {
                stamp: stamp(action, 3),
                value: 2,
            }),
        }
    }

    fn check(origins: [Option<WriteStamp>; SLOTS], causes: &[ReplayedCause]) -> bool {
        validate_joint(&origins, origins.into_iter().flatten().max(), causes).is_ok()
    }

    // These exercise the actual compact-origin algebra, not source authority.
    // Integration tests must also replay genuine captured SourceCause contexts.
    #[test]
    fn paired_retry_writes_cannot_be_omitted_or_replaced_with_stale_origins() {
        let old = retry(1);
        let current = retry(2);
        let origins = current.stamps();
        assert!(check(origins, std::slice::from_ref(&current)));
        for slot in 1..SLOTS {
            let mut missing = origins;
            missing[slot] = None;
            assert!(!check(missing, std::slice::from_ref(&current)));
            let mut stale = origins;
            stale[slot] = old.stamps()[slot];
            assert!(!check(stale, &[old.clone(), current.clone()]));
        }
        let mut wrong_slot = origins;
        wrong_slot[1] = origins[2];
        assert!(!check(wrong_slot, std::slice::from_ref(&current)));
        assert!(
            validate_joint(&origins, Some(stamp(2, 1)), std::slice::from_ref(&current)).is_err()
        );
        assert!(!check(origins, &[old, current]));
    }

    #[test]
    fn later_swing_supersedes_base_and_error_but_keeps_retry_off_origin() {
        let retry = retry(1);
        let swing = ReplayedCause {
            key: stamp(2, 1).cause,
            lifecycle: None,
            base: Some(Write {
                stamp: stamp(2, 2),
                value: 2500,
            }),
            off: None,
            last_error: Some(Write {
                stamp: stamp(2, 1),
                value: 0,
            }),
        };
        let origins = [
            None,
            Some(stamp(2, 2)),
            Some(stamp(1, 2)),
            Some(stamp(2, 1)),
        ];
        assert!(check(origins, &[retry.clone(), swing.clone()]));
        let mut omitted = origins;
        omitted[2] = None;
        assert!(!check(omitted, &[retry, swing]));
    }

    #[test]
    fn lethal_swing_stop_and_reset_share_one_ordered_cause() {
        let lethal = ReplayedCause {
            key: stamp(3, 1).cause,
            lifecycle: Some(Write {
                stamp: stamp(3, 1),
                value: Lifecycle {
                    melee_attacking: false,
                    victim: None,
                },
            }),
            base: Some(Write {
                stamp: stamp(3, 2),
                value: 2500,
            }),
            off: None,
            last_error: Some(Write {
                stamp: stamp(3, 3),
                value: 0,
            }),
        };
        let origins = lethal.stamps();
        assert!(check(origins, std::slice::from_ref(&lethal)));
        for slot in [0, 1, 3] {
            let mut omitted = origins;
            omitted[slot] = None;
            assert!(!check(omitted, std::slice::from_ref(&lethal)));
        }
    }

    #[test]
    fn reactive_origin_cannot_name_a_future_action_or_tick() {
        let frontier = stamp(3, 1).cause;
        assert!(key_within_frontier(frontier, frontier));
        assert!(key_within_frontier(stamp(2, 1).cause, frontier));
        let mut future_action = stamp(2, 1).cause;
        future_action.action = 4;
        assert!(!key_within_frontier(future_action, frontier));
        let mut future_tick = stamp(2, 1).cause;
        future_tick.tick = 4;
        assert!(!key_within_frontier(future_tick, frontier));
        let mut genesis = stamp(1, 1).cause;
        genesis.action = 0;
        assert!(!key_within_frontier(genesis, frontier));
    }

    #[test]
    fn explicit_genesis_and_frontier_are_exact() {
        assert!(validate_joint(&[None; SLOTS], None, &[]).is_ok());
        assert!(validate_joint(&[None; SLOTS], Some(stamp(1, 1)), &[]).is_err());
        assert!(validate_joint(&[None; SLOTS], None, &[retry(1)]).is_err());
        let mut missing_cause = [None; SLOTS];
        missing_cause[1] = Some(stamp(1, 1));
        assert!(!check(missing_cause, &[]));
    }

    #[test]
    fn unit_countdown_distinguishes_pre_unit_spell_and_post_unit_swing() {
        let mut pre = Write {
            stamp: stamp(4, 1),
            value: 2500,
        };
        pre.stamp.cause.phase = Phase::PreUnit;
        let post = Write {
            stamp: stamp(4, 1),
            value: 2500,
        };
        assert_eq!(remaining(pre, 50, 4, Phase::PreUnit).unwrap(), 2500);
        assert_eq!(remaining(pre, 50, 4, Phase::PostUnit).unwrap(), 2450);
        assert_eq!(remaining(post, 50, 4, Phase::PostUnit).unwrap(), 2500);
        assert_eq!(remaining(post, 50, 5, Phase::PreUnit).unwrap(), 2500);
        assert_eq!(remaining(post, 50, 5, Phase::PostUnit).unwrap(), 2450);
        assert_eq!(remaining(post, 50, 100, Phase::PostUnit).unwrap(), 0);
        assert!(remaining(post, 50, 4, Phase::PreUnit).is_err());
        assert!(remaining(post, 50, 0, Phase::PreUnit).is_err());
        assert!(remaining(post, u32::MAX, u64::MAX, Phase::PostUnit).is_err());
    }
}
