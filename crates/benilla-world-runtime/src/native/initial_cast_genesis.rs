// SPDX-License-Identifier: GPL-3.0-or-later
//! First-world-entry RNG realization. Creation is a retained unsampled prerequisite.
use super::*;
use benilla_sim::starter_init::initial_cast_phases::{InitialEffectPhase, InitialPhaseEvent};
use serde::{Deserialize, Serialize};

pub const INITIAL_CAST_GENESIS_POLICY: &str = "benilla-native-first-entry-initial-cast-genesis-v1";
const MAX_GENESIS_WORDS: u64 = 64;
const MAX_GENESIS_BYTES: usize = 64 * 1024;
const MAX_GENESIS_EVENTS: usize = 256 * 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum InitialCastBoundary {
    FirstWorldEntryOnly,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum GenesisEpoch {
    FirstWorldEntry,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum InitialCastSourceBinding {
    NoSourceInitializer,
    DeterministicInitializer {
        source_identity: crate::Digest,
    },
    Prepared {
        input: crate::Digest,
        preparation: crate::Digest,
        source_seal: crate::Digest,
        creation_plan: crate::Digest,
        entry_plan: crate::Digest,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenesisDraw {
    pub epoch: GenesisEpoch,
    pub occurrence: u32,
    pub spell_id: u32,
    pub effect_slot: u8,
    pub source_hash: crate::Digest,
    pub first_counter: u64,
    pub consumed_words: u32,
    pub low: i32,
    pub high: i32,
    pub value: i32,
    pub evaluated_amount: i32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InitialCastGenesis {
    version: u32,
    policy: String,
    rng_policy: String,
    boundary: InitialCastBoundary,
    epoch: GenesisEpoch,
    domain: Domain,
    world: crate::WorldId,
    player: crate::PlayerId,
    seed: u64,
    profile: crate::Digest,
    source_binding: InitialCastSourceBinding,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    entry_location: Option<InitialCastEntryLocationFacts>,
    first_counter: u64,
    end_counter: u64,
    semantic_requests: u32,
    draws: Vec<GenesisDraw>,
    realized_trace: crate::Digest,
}
impl InitialCastGenesis {
    pub fn end_counter(&self) -> u64 {
        self.end_counter
    }
    pub fn draws(&self) -> &[GenesisDraw] {
        &self.draws
    }
    pub fn source_binding(&self) -> &InitialCastSourceBinding {
        &self.source_binding
    }
    pub fn realized_trace(&self) -> crate::Digest {
        self.realized_trace
    }
    pub(super) fn realize(
        profile: &Profile,
        world: crate::WorldId,
        player: crate::PlayerId,
        seed: u64,
        entry_location: Option<&InitialCastEntryLocationFacts>,
    ) -> Result<Self, NativeError> {
        profile.require_realizable_initial_casts()?;
        match profile.initial_cast_source_input() {
            Some(source) => initial_cast_location::validate_entry(
                profile,
                world,
                player,
                source,
                entry_location,
            )?,
            None if entry_location.is_some() => return Err(NativeError::AuthorityBinding),
            None => {}
        }
        let mut cursor = 0;
        let mut draws = vec![];
        let (source_binding, realized_trace) = match &profile.combat {
            None => (
                InitialCastSourceBinding::NoSourceInitializer,
                crate::digest(b"benilla-native-initial-cast-empty-trace-v1\0", &()),
            ),
            Some(combat) => match &combat.prepared {
                None => (
                    InitialCastSourceBinding::DeterministicInitializer {
                        source_identity: combat.input_identity,
                    },
                    crate::digest(
                        b"benilla-native-initial-cast-deterministic-trace-v1\0",
                        &combat.input_identity,
                    ),
                ),
                Some(prepared) => {
                    let seal = combat
                        .initial_cast_source
                        .as_ref()
                        .ok_or(NativeError::InvalidProfile)?;
                    let plan = prepared.first_entry_plan();
                    if plan.occurrences().len() > 256 {
                        return Err(NativeError::TooLarge);
                    }
                    let requests = plan.draw_requests();
                    if requests.len() > 1 {
                        return Err(NativeError::TooLarge);
                    }
                    let mut rolls = Vec::with_capacity(requests.len());
                    for request in &requests {
                        let cast = plan
                            .occurrences()
                            .get(request.occurrence as usize)
                            .ok_or(NativeError::InvalidProfile)?;
                        if cast.ordinal() != request.occurrence || cast.row().id != request.spell_id
                        {
                            return Err(NativeError::InvalidProfile);
                        }
                        let sample = rng::sample_inclusive_v2(
                            world,
                            seed,
                            &mut cursor,
                            i64::from(request.low),
                            i64::from(request.high),
                        )?;
                        if cursor > MAX_GENESIS_WORDS {
                            return Err(NativeError::TooLarge);
                        }
                        let value =
                            i32::try_from(sample.value).map_err(|_| NativeError::InvalidState)?;
                        rolls.push(value);
                        draws.push(GenesisDraw {
                            epoch: GenesisEpoch::FirstWorldEntry,
                            occurrence: request.occurrence,
                            spell_id: request.spell_id,
                            effect_slot: request.effect_slot,
                            source_hash: cast.source_hash(),
                            first_counter: sample.first_counter,
                            consumed_words: sample.consumed_words,
                            low: request.low,
                            high: request.high,
                            value,
                            evaluated_amount: 0,
                        });
                    }
                    let evaluated = plan.evaluate(&rolls, prepared.first_entry_initial_masks())?;
                    if evaluated.events().len() > MAX_GENESIS_EVENTS
                        || (
                            evaluated.final_masks().weapon,
                            evaluated.final_masks().armor,
                        ) != combat.proficiencies
                    {
                        return Err(NativeError::InvalidProfile);
                    }
                    for draw in &mut draws {
                        let amounts: Vec<_> = evaluated
                            .events()
                            .iter()
                            .filter_map(|event| match event {
                                InitialPhaseEvent::Amount {
                                    occurrence,
                                    slot,
                                    phase: InitialEffectPhase::Targetless,
                                    value,
                                } if *occurrence == draw.occurrence
                                    && *slot == draw.effect_slot =>
                                {
                                    Some(*value)
                                }
                                _ => None,
                            })
                            .collect();
                        let [amount] = amounts.as_slice() else {
                            return Err(NativeError::InvalidProfile);
                        };
                        draw.evaluated_amount = *amount;
                    }
                    (
                        InitialCastSourceBinding::Prepared {
                            input: prepared.source_identity(),
                            preparation: prepared.identity(),
                            source_seal: seal.identity(),
                            creation_plan: prepared.creation_plan().identity(),
                            entry_plan: plan.identity(),
                        },
                        crate::digest(
                            b"benilla-native-initial-cast-realized-trace-v1\0",
                            &evaluated,
                        ),
                    )
                }
            },
        };
        let receipt = Self {
            version: 1,
            policy: INITIAL_CAST_GENESIS_POLICY.into(),
            rng_policy: RNG_POLICY.into(),
            boundary: InitialCastBoundary::FirstWorldEntryOnly,
            epoch: GenesisEpoch::FirstWorldEntry,
            domain: Domain::SyntheticComponentTest,
            world,
            player,
            seed,
            profile: profile.digest(),
            source_binding,
            entry_location: entry_location.cloned(),
            first_counter: 0,
            end_counter: cursor,
            semantic_requests: draws.len() as u32,
            draws,
            realized_trace,
        };
        receipt.check_budget()?;
        Ok(receipt)
    }
    fn check_budget(&self) -> Result<(), NativeError> {
        if self.draws.len() > 1
            || self.end_counter > MAX_GENESIS_WORDS
            || serde_json::to_vec(self)
                .map_err(|_| NativeError::InvalidCheckpoint)?
                .len()
                > MAX_GENESIS_BYTES
        {
            return Err(NativeError::TooLarge);
        }
        Ok(())
    }
    pub(super) fn verify(
        &self,
        profile: &Profile,
        world: crate::WorldId,
        player: crate::PlayerId,
        seed: u64,
    ) -> Result<u64, NativeError> {
        self.check_budget()?;
        let expected = Self::realize(profile, world, player, seed, self.entry_location.as_ref())?;
        if self != &expected {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok(expected.end_counter)
    }
}
pub(super) fn validate_counter(baseline: u64, tick: u64, counter: u64) -> Result<(), NativeError> {
    let upper = tick
        .checked_mul(MAX_RNG_WORDS_PER_TICK)
        .and_then(|budget| baseline.checked_add(budget))
        .ok_or(NativeError::InvalidCheckpoint)?;
    if counter < baseline || counter > upper {
        return Err(NativeError::InvalidCheckpoint);
    }
    Ok(())
}
pub(super) fn validate_state(profile: &Profile, state: &State) -> Result<u64, NativeError> {
    let baseline =
        state
            .initial_cast_genesis
            .verify(profile, state.world, state.player, state.seed)?;
    let baseline = creature_generation_binding::validate_prefix(profile, state, baseline)?;
    validate_counter(baseline, state.tick, state.rng_counter)?;
    Ok(baseline)
}
