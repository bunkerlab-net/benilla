// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Unit.cpp Attack/AttackStop/UpdateMeleeAttackingState.
//! Source-replayed latest writers. Context is reachable, not a trajectory proof.
use super::*;
use crate::NpcId;
use super::context::{SourceContext, ReplayedContext};
use benilla_sim::player_melee as melee;
use super::state::{CauseKey, Phase, ReplayedCause, Lifecycle, Write, WriteStamp};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) enum SourceCause {
    Start {
        key: CauseKey,
        input_sequence: u64,
        context: Box<SourceContext>
    },
    Stop {
        key: CauseKey,
        input_sequence: u64,
        previous: Option<NpcId>
    },
    SpellReset {
        key: CauseKey,
        witness: Box<melee_reset::MeleeResetWitness>
    },
    PlayerDeath {
        key: CauseKey,
        previous: NpcId,
        died_at_tick: u64
    },
    Ready {
        key: CauseKey,
        context: Box<SourceContext>,
        swing: Option<Box<SwingWitness>>
    },
}
fn empty(key: CauseKey) -> ReplayedCause {
    ReplayedCause {
        key,
        lifecycle: None,
        base: None,
        off: None,
        last_error: None
    }
}
fn write<T>(key: CauseKey, subphase: u8, value: T) -> Write<T> {
    Write {
        stamp: WriteStamp { cause: key, subphase },
        value
    }
}
impl SourceCause {
    pub(super) fn retained_origin(&self) -> (CauseKey, Option<(&SourceContext, &[Draw])>) {
        match self {
            Self::Ready { key, context, swing: Some(swing) } => (*key, Some((context, &swing.draws))),
            Self::Start { key, .. }
            | Self::Stop { key, .. }
            | Self::SpellReset { key, .. }
            | Self::PlayerDeath { key, .. }
            | Self::Ready { key, .. } => (*key, None),
        }
    }
    pub(super) fn validate_closed(&self, current: &State) -> Result<(), NativeError> {
        if matches!(self, Self::Start { input_sequence, .. } | Self::Stop { input_sequence, .. } if *input_sequence >= current.next_sequence) {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok(())
    }
    pub(super) fn replay_work(&self) -> u64 {
        match self {
            Self::Start { context, .. } => context.replay_work(),
            Self::Ready { context, swing, .. } => context
                .replay_work()
                .saturating_mul(3)
                .saturating_add(swing
                .as_ref()
                .map_or(0, |s| s.after_damage
                .replay_work()
                .saturating_mul(3)
                .saturating_add(s.after
                .replay_work()
                .saturating_mul(2))
                .saturating_add(s.draws.len() as u64))),
            Self::Stop { .. } | Self::PlayerDeath { .. } => 1,
            Self::SpellReset { witness, .. } => witness.replay_work(),
        }
    }
    pub(super) fn replay(&self, profile: &Profile, current: &State) -> Result<ReplayedCause, NativeError> {
        match self {
            Self::PlayerDeath { key, previous, died_at_tick } => super::death::replay_stop(profile, current, *key, *previous, *died_at_tick),
            Self::Start { key, input_sequence, context } => {
                if key.phase != Phase::PreUnit || *input_sequence > current.next_sequence {
                    return Err(NativeError::InvalidCheckpoint);
                }
                let source = context.replay(profile, current)?;
                if !source.can_attack_now || source.target_evading
                    || context.captured_ms() != profile.clock_origin_ms
                    .checked_add(key.tick
                    .checked_mul(u64::from(profile.tick_ms))
                    .ok_or(NativeError::Limit)?)
                    .ok_or(NativeError::Limit)? {
                    return Err(NativeError::InvalidCheckpoint);
                }
                let mut result = empty(*key);
                if source.predecessor.lifecycle.victim != Some(context.target()) {
                    result.lifecycle = Some(write(*key, 1, Lifecycle { melee_attacking: true, victim: Some(context.target()) }));
                }
                Ok(result)
            }
            Self::Stop { key, input_sequence, previous } => {
                if key.phase != Phase::PreUnit || *input_sequence > current.next_sequence {
                    return Err(NativeError::InvalidCheckpoint);
                }
                let captured_ms = profile.clock_origin_ms
                    .checked_add(key.tick
                    .checked_mul(u64::from(profile.tick_ms))
                    .ok_or(NativeError::Limit)?)
                    .ok_or(NativeError::Limit)?;
                if let Some(previous) = previous {
                    let owner = current.npcs
                        .get(&previous.spawn)
                        .ok_or(NativeError::InvalidCheckpoint)?;
                    interactions::spell_authority::historical_target_profile(profile, owner, *previous, captured_ms)?;
                }
                let mut result = empty(*key);
                if previous.is_some() {
                    result.lifecycle = Some(write(*key, 1, Lifecycle { melee_attacking: false, victim: None }));
                }
                Ok(result)
            }
            Self::Ready { key, context, swing } => {
                let source = context.replay(profile, current)?;
                if key.phase != Phase::PostUnit
                    || source.predecessor.lifecycle.victim != Some(context.target())

                        || !source.predecessor.lifecycle.melee_attacking
                        || source.predecessor.remaining_ms[0] != 0

                        || context.captured_ms().checked_sub(profile.clock_origin_ms) != key.tick
                        .checked_mul(u64::from(profile.tick_ms)) {
                    return Err(NativeError::InvalidCheckpoint);
                }
                let (error, executes) = ready_decision(&source);
                if executes && source.target_evading {
                    return Err(NativeError::UnsupportedMechanic("player melee evade damage"));
                }
                if executes != swing.is_some() {
                    return Err(NativeError::InvalidCheckpoint);
                }
                let mut result = empty(*key);
                if let Some(swing) = swing {
                    let (resolved, intervals) = swing.replay(profile, current, *key, context, &source)?;
                    if resolved.total_damage() >= source.target_health
                        && source.target_health != 0 {
                        result.lifecycle = Some(write(*key, 1, Lifecycle { victim: None, melee_attacking: false }));
                    }
                    result.base = Some(write(*key, 2, intervals[0]));
                } else if error != 0 {
                    result.base = Some(write(*key, 2, 100));
                    result.off = Some(write(*key, 3, 100));
                }
                if error != source.predecessor.last_error {
                    result.last_error = Some(write(*key, 4, error));
                }
                Ok(result)
            }
            Self::SpellReset { key, witness } => {
                let (released_ms, intervals) = witness.source_origin();
                if key.phase != Phase::PreUnit
                    || released_ms.checked_sub(profile.clock_origin_ms) != key.tick
                    .checked_mul(u64::from(profile.tick_ms)) {
                    return Err(NativeError::InvalidCheckpoint);
                }
                #[cfg(any(test, feature = "synthetic-native-tests"))]
                witness.validate(profile, current)?;
                let mut result = empty(*key);
                result.base = Some(write(*key, 1, intervals[0]));
                Ok(result)
            }
        }
    }
}

/// Pinned enum: range=0, dead=1, facing=2, disallowed=3. The source tests
/// `if (swingError)`, so range does not write either retry timer.
pub(super) fn ready_decision(source: &ReplayedContext) -> (u8, bool) {
    if !source.in_range {
        (0, false)
    } else if !source.facing {
        (2, false)
    } else if source.target_health == 0 {
        (1, false)
    } else if !source.can_attack_in_combat {
        (3, false)
    } else {
        (0, true)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SwingWitness {
    #[serde(deserialize_with = "deserialize_swing_draws")]
    pub(super) draws: Vec<Draw>,
    pub(super) after_damage: MeleeExecutionBinding,
    pub(super) after: MeleeExecutionBinding,
    pub(super) after_equipment: equipment::EquipmentSnapshot,
    pub(super) loot: Option<CorpseLoot>,
}
fn deserialize_swing_draws<'de, D>(deserializer: D) -> Result<Vec<Draw>, D::Error>
where
    D: serde::Deserializer<'de>
{
    struct Draws;
    impl<'de> serde::de::Visitor<'de> for Draws {
        type Value = Vec<Draw>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("two to 4102 source callback draws")
        }
        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>
        {
            if sequence.size_hint().is_some_and(|size| !(2..=4102).contains(&size)) {
                return Err(serde::de::Error::custom("swing draw count"));
            }
            let mut draws = Vec::new();
            while draws.len() < 4102 {
                let Some(draw) = sequence.next_element()? else {
                    break;
                };
                draws.push(draw);
            }
            if draws.len() < 2 || sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(serde::de::Error::custom("swing draw count"));
            }
            Ok(draws)
        }
    }
    deserializer.deserialize_seq(Draws)
}
impl SwingWitness {
    fn replay(&self, profile: &Profile, current: &State, key: CauseKey, context: &SourceContext, source: &ReplayedContext) -> Result<(melee::MeleeResolution, [u32; 2]), NativeError> {
        if self.draws.len() < 2
            || self.draws.len() > 4102
            || self.after.captured_ms() != context.captured_ms() {
            return Err(NativeError::InvalidCheckpoint);
        }
        #[cfg(any(test, feature = "synthetic-native-tests"))]
        let after = self.after.validate(profile, current.quest_progression
            .as_ref(), current.now_ms, (profile.digest(), current.domain, current.world, current.player))?;
        #[cfg(not(any(test, feature = "synthetic-native-tests")))]
        let after = self.after.current_operands(profile)?;
        let target = context.target();
        let mut cursor = DrawCursor::new(current, &self.draws);
        let result = numeric_from_cursor(key, target, source, &mut cursor)?;
        let killed = result.total_damage() >= source.target_health && source.target_health != 0;
        let mut equipment = equipment::EquipmentState::from_snapshot(context.before_equipment().clone())?;
        let mut broken_slot = None;
        if killed {
            if context.credited_generation()
                .is_some_and(|generation| generation >= target.generation) {
                return Err(NativeError::InvalidCheckpoint);
            }
            self.replay_death_draws(profile, current, context, &mut cursor)?;
        } else {
            if self.loot.is_some() {
                return Err(NativeError::InvalidCheckpoint);
            }
            if result.total_damage() != 0 {
                let chance = cursor.take(DrawPurpose::PlayerDurabilityChance { action: key.action, npc: target }, 0, (1_i64 << 53) - 1)? as u64;
                if benilla_sim::npc_melee::PercentRoll::from_fraction53(chance)?.succeeds(profile.combat
                    .as_ref()
                    .ok_or(NativeError::InvalidProfile)?.durability_hit_chance)? {
                    let slot = cursor.take(DrawPurpose::PlayerDurabilitySlot { action: key.action, npc: target }, 0, 18)? as u8;
                    equipment.apply_hit_loss::<NativeError, _>(slot, |_, _, changes| {
                        for transition in &changes.stat_transitions {
                            let equipment::EquipmentStatTransition::Broken { slot, .. } = *transition;
                            if broken_slot.replace(slot).is_some() {
                                return Err(NativeError::InvalidCheckpoint);
                            }
                        }
                        Ok(())
                    })?;
                }
            }
        }
        if equipment.snapshot() != self.after_equipment {
            return Err(NativeError::InvalidCheckpoint);
        }
        #[cfg(any(test, feature = "synthetic-native-tests"))]
        context.binding()
            .validate_damage_callbacks(profile, current, &self.after_damage, killed.then_some(target), broken_slot)?;
        let operands = self.after_damage.current_operands(profile)?;
        let owner = current.npcs.get(&target.spawn).ok_or(NativeError::InvalidCheckpoint)?;
        let target_profile = owner.historical_profile(profile, target)?;
        let critter = target_profile.combat
            .as_ref()
            .ok_or(NativeError::InvalidProfile)?.creature_is_critter;
        let gain = profile.player_melee
            .as_ref()
            .ok_or(NativeError::InvalidProfile)?
            .weapon_skill_gain();
        let skill = melee::WeaponSkill { value: operands.pure_skill, maximum: operands.maximum_skill, gain };
        let mut weapon_callback = None;
        // Glancing also sets PROC_EX_NORMAL_HIT in CalculateMeleeDamage.
        if !critter {
            let roll = melee::weapon_gain_chance(operands.level, operands.intellect, skill)?
                .map(|_| {
                cursor.take(DrawPurpose::PlayerWeaponGain { action: key.action, npc: target }, 0, (1_i64 << 53) - 1)

                        .and_then(|fraction| Ok(benilla_sim::npc_melee::PercentRoll::from_fraction53(fraction as u64)?))
            }).transpose()?;
            let resolved = melee::resolve_weapon_gain(operands.level, operands.intellect, skill, roll)?;
            if resolved.update_weapon_skill() {
                weapon_callback = Some((operands.skill_id, gain));
            }
        }
        if cursor.index != self.draws.len() {
            return Err(NativeError::InvalidCheckpoint);
        }
        #[cfg(any(test, feature = "synthetic-native-tests"))]
        self.after_damage.validate_weapon_callback(profile, current, &self.after, weapon_callback)?;
        #[cfg(not(any(test, feature = "synthetic-native-tests")))]
        let _ = weapon_callback;
        Ok((result, after.attack_times_ms))
    }
    fn replay_death_draws(&self, profile: &Profile, current: &State, context: &SourceContext, cursor: &mut DrawCursor<'_>) -> Result<(), NativeError> {
        let target = context.target();
        let g = profile.quest_progression.as_ref().ok_or(NativeError::InvalidProfile)?;
        let target_profile = profile.npc(target.spawn).ok_or(NativeError::InvalidProfile)?;
        if let Some(loot) = &g.loot {
            let row = loot.creatures.get(&target_profile.entry).ok_or(NativeError::InvalidProfile)?;
            if !row.no_loot {
                let saved = self.loot.as_ref().ok_or(NativeError::InvalidCheckpoint)?;
                if saved.owner != current.player
                    || saved.corpse != target
                    || saved.death_at_ms != context.captured_ms()
                    || saved.opened || saved.money_taken || !saved.collected.is_empty() {
                    return Err(NativeError::InvalidCheckpoint);
                }
                let mut probe = current.clone();
                probe.rng_counter = cursor.end_counter()?;
                if saved.first_counter != probe.rng_counter {
                    return Err(NativeError::InvalidCheckpoint);
                }
                let mut draws = vec![];
                let generated = loot.catalog
                    .generate(row.loot_id, row.minimum_money, row.maximum_money, &loot.rules,
                    &mut crate::native::loot::RandomHost { state: &mut probe, corpse: target, draws: &mut draws })?;
                if saved.generated != generated
                    || saved.draws != draws
                    || saved.end_counter != probe.rng_counter

                        || saved.looted != (generated.items.is_empty()
                        && generated.money == 0
                        && !generated.fake) {
                        return Err(NativeError::InvalidCheckpoint);
                    }
                for draw in &draws { cursor.exact(draw)?; }
            } else if self.loot.is_some() {
                return Err(NativeError::InvalidCheckpoint);
            }
        } else if self.loot.is_some() {
            return Err(NativeError::InvalidCheckpoint);
        }
        let generation = current.npcs
            .get(&target.spawn)
            .and_then(|npc| npc.source_generation
            .as_ref())
            .and_then(|owner| owner.historical(target))
            .ok_or(NativeError::InvalidCheckpoint)?;
        let death = generation.death_draw().ok_or(NativeError::InvalidCheckpoint)?;
        cursor.exact(death)?;
        Ok(())
    }

}
pub(super) fn replay_numeric(
    profile: &Profile,
    current: &State,
    key: CauseKey,
    context: &SourceContext,
    draws: &[Draw],
) -> Result<melee::MeleeResolution, NativeError> {
    if !(2..=3).contains(&draws.len()) {
        return Err(NativeError::InvalidCheckpoint);
    }
    let source = context.replay(profile, current)?;
    let mut cursor = DrawCursor::new(current, draws);
    let result = numeric_from_cursor(key, context.target(), &source, &mut cursor)?;
    if cursor.index != draws.len() {
        return Err(NativeError::InvalidCheckpoint);
    }
    Ok(result)
}

fn numeric_from_cursor(
    key: CauseKey,
    target: NpcId,
    source: &ReplayedContext,
    cursor: &mut DrawCursor<'_>,
) -> Result<melee::MeleeResolution, NativeError> {
    let plan = melee::prepare(source.input)?;
    let (low, high) = plan.damage_range();
    let damage = cursor.take(DrawPurpose::PlayerMeleeDamage { action: key.action, npc: target }, i64::from(low), i64::from(high))? as u32;
    let outcome = cursor.take(DrawPurpose::PlayerMeleeOutcome { action: key.action, npc: target }, 1, 10000)? as u16;
    let glance = plan
        .glance_range(outcome)?
        .map(|(low,high)| cursor.take(DrawPurpose::PlayerMeleeGlance { action: key.action, npc: target }, i64::from(low), i64::from(high))
        .map(|v| v as u32))
        .transpose()?;
    let result = melee::resolve(plan, melee::MeleeRolls { damage, outcome, glance })?;
    Ok(result)
}

struct DrawCursor<'a> {
    state: &'a State,
    draws: &'a [Draw],
    index: usize
}
impl<'a> DrawCursor<'a> {
    fn new(state: &'a State, draws: &'a [Draw]) -> Self {
        Self { state, draws, index: 0 }
    }
    fn end_counter(&self) -> Result<u64, NativeError> {
        let draw = self.draws
            .get(self.index.checked_sub(1)
            .ok_or(NativeError::InvalidCheckpoint)?)
            .ok_or(NativeError::InvalidCheckpoint)?;
        draw.first_counter
            .checked_add(u64::from(draw.consumed))
            .ok_or(NativeError::InvalidCheckpoint)
    }
    fn exact(&mut self, expected: &Draw) -> Result<(), NativeError> {
        if self.draws.get(self.index) != Some(expected) {
            return Err(NativeError::InvalidCheckpoint);
        }
        self.take(expected.purpose, expected.low, expected.high)?;
        Ok(())
    }
    fn take(&mut self, purpose: DrawPurpose, low: i64, high: i64) -> Result<i64, NativeError> {
        let draw = self.draws.get(self.index).ok_or(NativeError::InvalidCheckpoint)?;
        if draw.purpose != purpose || draw.low != low || draw.high != high || draw.cast_id != 0 {
            return Err(NativeError::InvalidCheckpoint);
        }
        if self.index != 0 && draw.first_counter != self.end_counter()? {
            return Err(NativeError::InvalidCheckpoint);
        }
        validate_draw(self.state, draw)?;
        self.index += 1;
        Ok(draw.value)
    }
}
fn validate_draw(state: &State, draw: &Draw) -> Result<(), NativeError> {
    let mut counter = draw.first_counter;
    let result = rng::sample_inclusive_v2(state.world, state.seed, &mut counter, draw.low, draw.high)?;
    if result.first_counter != draw.first_counter
        || result.consumed_words != draw.consumed
        || result.value != draw.value
        || counter > state.rng_counter {
        return Err(NativeError::InvalidCheckpoint);
    }
    Ok(())
}
