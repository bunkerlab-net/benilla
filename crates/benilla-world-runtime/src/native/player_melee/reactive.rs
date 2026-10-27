// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 UnitAuraProcHandler.cpp ProcSkillsAndReactives.
//! A bounded source cause for an ordinary NPC's REACTIVE_DEFENSE timer.
//! No timestamp or outcome supplied by a caller grants a reactive state.
use super::context::{ReplayedContext, SourceContext};
use super::source::replay_numeric;
use super::state::{CauseKey, Phase};
use crate::native::{Draw, NativeError, Profile, State};
use crate::NpcId;
use benilla_sim::player_melee::{MeleeOutcome, MeleeResolution};
use serde::{Deserialize, Serialize};

const REACTIVE_MS: u32 = 4000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::native) struct NpcDefenseReactive {
    context: Box<SourceContext>,
    key: CauseKey,
    #[serde(deserialize_with = "deserialize_draws")]
    draws: Vec<Draw>,
}

fn deserialize_draws<'de, D>(deserializer: D) -> Result<Vec<Draw>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Draws;
    impl<'de> serde::de::Visitor<'de> for Draws {
        type Value = Vec<Draw>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("exactly two or three source melee draws")
        }
        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            if sequence
                .size_hint()
                .is_some_and(|size| !(2..=3).contains(&size))
            {
                return Err(serde::de::Error::custom("reactive draw count"));
            }
            let mut draws = Vec::new();
            while draws.len() < 3 {
                let Some(draw) = sequence.next_element()? else {
                    break;
                };
                draws.push(draw);
            }
            if draws.len() < 2 || sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(serde::de::Error::custom("reactive draw count"));
            }
            Ok(draws)
        }
    }
    deserializer.deserialize_seq(Draws)
}

impl NpcDefenseReactive {
    /// Called after the genuine damage/skill callbacks, before this NPC's Unit
    /// update. A refresh replaces the old cause; the caller clears it on death
    /// or respawn. No arbitrary public state setter accompanies this type.
    pub(super) fn capture(
        profile: &Profile,
        state: &State,
        context: SourceContext,
        key: CauseKey,
        draws: Vec<Draw>,
        actual: &MeleeResolution,
    ) -> Result<Option<Self>, NativeError> {
        let proof = Self {
            context: Box::new(context),
            key,
            draws,
        };
        let (replayed, result, defense_class) = proof.replay(profile, state)?;
        if result != *actual {
            return Err(NativeError::InvalidState);
        }
        let target = state
            .npcs
            .get(&proof.context.target().spawn)
            .filter(|target| target.id == proof.context.target())
            .ok_or(NativeError::InvalidState)?;
        if target.health != replayed.target_health.saturating_sub(result.total_damage()) {
            return Err(NativeError::InvalidState);
        }
        if !writes_defense(defense_class, result.outcome(), target.health != 0)? {
            return Ok(None);
        }
        Ok(Some(proof))
    }

    pub(in crate::native) fn replay_work(&self) -> u64 {
        self.context.replay_work().saturating_mul(2).saturating_add(3)
    }

    /// End-of-NPC-Unit-update value only. Before that update a newly minted
    /// source reactive is still 4000; this method already includes its diff.
    pub(in crate::native) fn remaining(&self, now: u64, diff: u32) -> Result<u32, NativeError> {
        remaining_from(self.context.captured_ms(), now, diff)
    }

    fn replay(
        &self,
        profile: &Profile,
        state: &State,
    ) -> Result<(ReplayedContext, MeleeResolution, u32), NativeError> {
        let context = self.context.replay(profile, state)?;
        let start = self.context.captured_ms();
        if profile.tick_ms == 0
            || self.key.tick == 0
            || self.key.action == 0
            || self.key.phase != Phase::PostUnit
            || start.checked_sub(profile.clock_origin_ms)
                != self.key.tick.checked_mul(u64::from(profile.tick_ms))
            || self.key.tick > state.tick
            || !(2..=3).contains(&self.draws.len())
            || context.predecessor.lifecycle.victim != Some(self.context.target())
            || !context.predecessor.lifecycle.melee_attacking
            || context.predecessor.remaining_ms[0] != 0
            || context.player_health == 0
            || context.target_health == 0
            || context.target_evading
            || !context.in_range
            || !context.facing
            || !context.can_attack_in_combat
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        let result = replay_numeric(profile, state, self.key, &self.context, &self.draws)?;
        let class = profile
            .creature_generations
            .as_ref()
            .and_then(|catalog| catalog.source(self.context.target().spawn))
            .ok_or(NativeError::InvalidProfile)?
            .input
            .template
            .unit_class;
        Ok((context, result, class))
    }

    pub(in crate::native) fn validate(
        &self,
        profile: &Profile,
        state: &State,
        target: NpcId,
    ) -> Result<(), NativeError> {
        let current = state
            .npcs
            .get(&target.spawn)
            .filter(|npc| npc.id == target && npc.health != 0)
            .ok_or(NativeError::InvalidCheckpoint)?;
        if self.context.target() != current.id {
            return Err(NativeError::InvalidCheckpoint);
        }
        state
            .player_melee
            .as_ref()
            .ok_or(NativeError::InvalidCheckpoint)?
            .proof
            .validate_reactive_origin(self.key, &self.context, &self.draws)?;
        let (context, result, class) = self.replay(profile, state)?;
        let survived = context.target_health > result.total_damage();
        if !writes_defense(class, result.outcome(), survived)? {
            return Err(NativeError::InvalidCheckpoint);
        }
        self.remaining(state.now_ms, profile.tick_ms)?;
        Ok(())
    }
}

fn writes_defense(class: u32, outcome: MeleeOutcome, survived: bool) -> Result<bool, NativeError> {
    if !matches!(class, 1 | 2 | 4 | 8) {
        return Err(NativeError::UnsupportedMechanic("NPC defense reactive class"));
    }
    Ok(survived
        && match outcome {
            MeleeOutcome::Dodge => class != 4, // rogue: skip Riposte state
            // Hunter's distinct REACTIVE_HUNTER_PARRY is outside this domain.
            MeleeOutcome::Parry | MeleeOutcome::Block => true,
            _ => false,
        })
}

fn remaining_from(start: u64, now: u64, diff: u32) -> Result<u32, NativeError> {
    if diff == 0 {
        return Err(NativeError::InvalidCheckpoint);
    }
    let since_start = now
        .checked_sub(start)
        .ok_or(NativeError::InvalidCheckpoint)?;
    if !since_start.is_multiple_of(u64::from(diff)) {
        return Err(NativeError::InvalidCheckpoint);
    }
    // Player melee callback precedes this NPC's Unit::UpdateReactiveTimers.
    // The source refresh therefore loses this same update's diff immediately.
    let elapsed = since_start
        .checked_add(u64::from(diff))
        .ok_or(NativeError::InvalidCheckpoint)?;
    Ok(u64::from(REACTIVE_MS).saturating_sub(elapsed) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defense_reactive_matches_source_class_outcome_and_survival_branches() {
        for class in [1, 2, 4, 8] {
            for outcome in [
                MeleeOutcome::Miss,
                MeleeOutcome::Dodge,
                MeleeOutcome::Parry,
                MeleeOutcome::Block,
                MeleeOutcome::Glancing,
                MeleeOutcome::Critical,
                MeleeOutcome::Normal,
            ] {
                let expected = matches!(outcome, MeleeOutcome::Parry | MeleeOutcome::Block)
                    || outcome == MeleeOutcome::Dodge && class != 4;
                assert_eq!(writes_defense(class, outcome, true).unwrap(), expected);
                assert!(!writes_defense(class, outcome, false).unwrap());
            }
        }
        for class in [0, 3, 5, 6, 7, 9] {
            assert!(writes_defense(class, MeleeOutcome::Parry, true).is_err());
        }
    }

    #[test]
    fn serialized_draw_count_is_bounded_before_numeric_validation() {
        #[derive(Deserialize)]
        struct List(#[serde(deserialize_with = "deserialize_draws")] Vec<Draw>);
        // These are count-only fixtures, never accepted as source RNG proof.
        let draw = Draw {
            cast_id: 0,
            purpose: crate::native::DrawPurpose::Hit,
            first_counter: 0,
            consumed: 1,
            low: 0,
            high: 1,
            value: 0,
        };
        for count in 0..=5 {
            let value = serde_json::to_value(vec![draw.clone(); count]).unwrap();
            let decoded = serde_json::from_value::<List>(value);
            if (2..=3).contains(&count) {
                assert_eq!(decoded.unwrap().0.len(), count);
            } else {
                assert!(decoded.is_err());
            }
        }
    }

    #[test]
    fn reactive_refresh_uses_original_npc_same_tick_countdown() {
        assert_eq!(remaining_from(1000, 1000, 50).unwrap(), 3950);
        assert_eq!(remaining_from(1000, 1050, 50).unwrap(), 3900);
        assert_eq!(remaining_from(1000, 4950, 50).unwrap(), 0);
        assert_eq!(remaining_from(1050, 1050, 50).unwrap(), 3950);
        assert_eq!(remaining_from(1000, 9000, 50).unwrap(), 0);
        assert!(remaining_from(1000, 999, 50).is_err());
        assert!(remaining_from(1000, 1001, 50).is_err());
        assert!(remaining_from(1000, 1000, 0).is_err());
        assert!(remaining_from(0, u64::MAX, 1).is_err());
    }
}
