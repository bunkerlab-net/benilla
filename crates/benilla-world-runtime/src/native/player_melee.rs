// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 CombatHandler.cpp and Unit/Player melee callbacks.
//! Owner-side mainhand policy. Selecting source operands does not grant admission.
use super::*;
use crate::NpcId;
use serde::{Deserialize, Serialize};
pub(in crate::native) mod casting;
mod context;
pub(in crate::native) mod death;
mod reactive;
mod source;
mod state;
pub(in crate::native) mod target_death;
pub(in crate::native) use death::PendingPlayerMeleeDeath;
pub(in crate::native) use reactive::NpcDefenseReactive;

pub const PLAYER_MELEE_POLICY: &str = "cmangos-8ec338a1-native-mage-mainhand-v2";

/// Explicit source configuration plus existing retained dependency closures.
/// No deserializer or constructor accepts resolved damage or target eligibility.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlayerMeleePolicy {
    policy: &'static str,
    weapon_skill_gain: u16,
}
impl PlayerMeleePolicy {
    pub(in crate::native) fn weapon_skill_gain(&self) -> u16 {
        self.weapon_skill_gain
    }

    pub(in crate::native) fn validate(&self, profile: &Profile) -> Result<(), NativeError> {
        if self.policy != PLAYER_MELEE_POLICY {
            return Err(NativeError::InvalidProfile);
        }
        let combat = profile
            .combat
            .as_ref()
            .ok_or(NativeError::UnsupportedMechanic(
                "player melee retained starter operands",
            ))?;
        if combat
            .equipment_weapons
            .iter()
            .any(|weapon| weapon.slot == 15 && weapon.subclass == 20)
        {
            return Err(NativeError::UnsupportedMechanic(
                "player melee fishing-pole callback",
            ));
        }
        if combat.starter_identity.class != 8 {
            return Err(NativeError::UnsupportedMechanic("player melee non-mage"));
        }
        let _progression =
            profile
                .quest_progression
                .as_ref()
                .ok_or(NativeError::UnsupportedMechanic(
                    "player melee live derived-stat source",
                ))?;
        let generations =
            profile
                .creature_generations
                .as_ref()
                .ok_or(NativeError::UnsupportedMechanic(
                    "player melee retained creature initialization",
                ))?;
        if profile.source_motion.is_none()
            || !profile.source_facing_authority()
            || !profile.source_combat_authority()
        {
            return Err(NativeError::UnsupportedMechanic(
                "player melee source movement/relations/facing",
            ));
        }
        interactions::player_melee::validate_policy(profile)?;
        if combat
            .initial_auras
            .iter()
            .any(|aura| aura.aura_interrupt_flags & 0x1000 != 0)
        {
            return Err(NativeError::UnsupportedMechanic(
                "player melee attack-interrupted passive aura",
            ));
        }
        if combat
            .source_attack_times_for_recipe(&combat.recipe.snapshot())?
            .into_iter()
            .any(|time| time > 1_000_000)
        {
            return Err(NativeError::UnsupportedMechanic(
                "player melee bounded attack interval",
            ));
        }
        for npc in &profile.npcs {
            let source = generations
                .source(npc.ai.creature_id)
                .ok_or(NativeError::InvalidProfile)?;
            let raw = &source.input.template;
            // Existing ordinary initialization owns only these classes/static
            // flags. Do not reinterpret unsupported controls as absent defenses.
            if !matches!(raw.unit_class, 1 | 2 | 4 | 8)
                || raw.extra_flags & !(0x2 | 0x0008_0000) != 0
                || raw.static_flags[0] != 0
                || raw.static_flags[1] & !0x10 != 0
                || raw.static_flags[2] != 0
                || raw.static_flags[3] != 0
            {
                return Err(NativeError::UnsupportedMechanic(
                    "player melee ordinary creature callback closure",
                ));
            }
        }
        Ok(())
    }
}
impl Profile {
    /// CONFIG_UINT32_SKILL_GAIN_WEAPON is separate from defense gain. The
    /// selected bounded reducer rejects an unrepresentable value, never clamps.
    pub fn initialize_player_melee(&mut self, weapon_skill_gain: u32) -> Result<(), NativeError> {
        let mut next = self.clone();
        next.player_melee = Some(PlayerMeleePolicy {
            policy: PLAYER_MELEE_POLICY,
            weapon_skill_gain: weapon_skill_gain.try_into().map_err(|_| {
                NativeError::UnsupportedMechanic("player weapon skill gain above uint16")
            })?,
        });
        next.validate()?;
        *self = next;
        Ok(())
    }
}

/// Configuration authority for every retained weapon callback, including old
/// prefixes reached through spell/melee operand bindings.
pub(in crate::native) fn validate_weapon_history(
    profile: &Profile,
    state: &State,
) -> Result<(), NativeError> {
    if let Some(progression) = &state.quest_progression {
        progression.validate_weapon_policy(
            profile
                .player_melee
                .as_ref()
                .map(PlayerMeleePolicy::weapon_skill_gain),
        )?;
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::native) struct PlayerMeleeState {
    proof: state::PlayerMeleeProof,
    last_error: u8,
}
impl PlayerMeleeState {
    pub(in crate::native) fn replay_work(&self) -> u64 {
        self.proof.replay_work()
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(in crate::native) fn new(profile: &Profile, current: &State) -> Self {
        Self {
            proof: state::PlayerMeleeProof::genesis(profile, current),
            last_error: 0,
        }
    }
}
fn current_fields(current: &State) -> Result<state::MeleeFields, NativeError> {
    let selected = current
        .player_melee
        .as_ref()
        .ok_or(NativeError::InvalidState)?;
    let timers = &current
        .combat
        .as_ref()
        .ok_or(NativeError::InvalidState)?
        .melee_timers;
    Ok(state::MeleeFields {
        lifecycle: state::Lifecycle {
            victim: timers.victim(),
            melee_attacking: timers.melee_attacking(),
        },
        remaining_ms: timers.remaining_ms(),
        last_error: selected.last_error,
    })
}
// NPC spells and player control effects remain outside the selected closure.
#[cfg(any(test, feature = "synthetic-native-tests"))]
pub(in crate::native) fn refuse_spell_concurrency(
    profile: &Profile,
    current: &State,
) -> Result<(), NativeError> {
    if profile.player_melee.is_some()
        && current
            .combat
            .as_ref()
            .is_some_and(|combat| combat.melee_timers.melee_attacking())
    {
        return Err(NativeError::UnsupportedMechanic(
            "player melee concurrent spell/projectile/aura",
        ));
    }
    Ok(())
}

pub(in crate::native) fn validate_generic_concurrency(
    profile: &Profile,
    current: &State,
) -> Result<(), NativeError> {
    if profile.player_melee.is_some() {
        validate_supported_concurrency(profile, current)?;
    }
    Ok(())
}
fn validate_supported_concurrency(_profile: &Profile, current: &State) -> Result<(), NativeError> {
    let combat = current.combat.as_ref().ok_or(NativeError::InvalidState)?;
    if combat.has_channeled_spell
        || combat.has_auto_repeat_spell
        || current
            .npcs
            .values()
            .any(|npc| npc.events.spells().next().is_some())
        || current
            .player_slow
            .as_ref()
            .is_some_and(|slow| slow.holders().next().is_some())
    {
        return Err(NativeError::UnsupportedMechanic(
            "player melee concurrent spell/projectile/aura",
        ));
    }
    Ok(())
}

fn key(current: &State, phase: state::Phase) -> Result<state::CauseKey, NativeError> {
    Ok(state::CauseKey {
        tick: current.tick,
        phase,
        action: current
            .player_melee
            .as_ref()
            .ok_or(NativeError::InvalidState)?
            .proof
            .next_action()?,
    })
}
fn commit_cause(
    profile: &Profile,
    current: &mut State,
    phase: state::Phase,
    cause: source::SourceCause,
) -> Result<(), NativeError> {
    let proof = current
        .player_melee
        .as_ref()
        .ok_or(NativeError::InvalidState)?
        .proof
        .apply(profile, current, phase, cause)?;
    let fields = proof.expected(profile, current, phase)?;
    current
        .combat
        .as_mut()
        .ok_or(NativeError::InvalidState)?
        .melee_timers
        .write_source_fields(fields.lifecycle.victim, fields.remaining_ms)?;
    current.player_melee = Some(PlayerMeleeState {
        proof,
        last_error: fields.last_error,
    });
    Ok(())
}
pub(in crate::native) fn start(
    profile: &Profile,
    current: &mut State,
    target: NpcId,
) -> Result<(Vec<Event>, Option<Rejection>), NativeError> {
    profile
        .player_melee
        .as_ref()
        .ok_or(NativeError::UnsupportedMechanic(
            "player melee policy not selected",
        ))?
        .validate(profile)?;
    validate_supported_concurrency(profile, current)?;
    validate_fields(profile, current, state::Phase::PreUnit)?;
    let before = current_fields(current)?;
    let npc = current
        .npcs
        .get(&target.spawn)
        .filter(|npc| npc.id == target)
        .ok_or(NativeError::UnknownTarget)?;
    let context =
        context::SourceContext::capture(profile, current, npc, before, state::Phase::PreUnit)?;
    let resolved = context.replay(profile, current)?;
    if !resolved.can_attack_now {
        return Ok((
            vec![Event::PlayerAttackStop { target }],
            Some(Rejection::AttackDenied),
        ));
    }
    // Unit::Attack rejects an evading victim after the handler check, without
    // sending that handler's MeleeAttackStop or switching the current victim.
    if resolved.target_evading {
        return Ok((vec![], None));
    }
    let mut events = vec![];
    if before.lifecycle.victim != Some(target) {
        if let Some(previous) = before.lifecycle.victim {
            events.push(Event::PlayerAttackCancel);
            events.push(Event::PlayerAttackStop { target: previous });
        }
        events.push(Event::PlayerAttackStart { target });
    }
    let cause = source::SourceCause::Start {
        key: key(current, state::Phase::PreUnit)?,
        input_sequence: current.next_sequence,
        context: Box::new(context),
    };
    commit_cause(profile, current, state::Phase::PreUnit, cause)?;
    Ok((events, None))
}
pub(in crate::native) fn stop(
    profile: &Profile,
    current: &mut State,
) -> Result<Vec<Event>, NativeError> {
    if profile.player_melee.is_none() {
        return Err(NativeError::UnsupportedMechanic(
            "player melee policy not selected",
        ));
    }
    validate_fields(profile, current, state::Phase::PreUnit)?;
    let previous = current_fields(current)?.lifecycle.victim;
    let cause = source::SourceCause::Stop {
        key: key(current, state::Phase::PreUnit)?,
        input_sequence: current.next_sequence,
        previous,
    };
    commit_cause(profile, current, state::Phase::PreUnit, cause)?;
    let mut events = vec![];
    if let Some(target) = previous {
        events.push(Event::PlayerAttackStop { target });
    }
    Ok(events)
}
fn validate_fields(
    profile: &Profile,
    current: &State,
    phase: state::Phase,
) -> Result<(), NativeError> {
    let owner = current
        .player_melee
        .as_ref()
        .ok_or(NativeError::InvalidState)?;
    current
        .combat
        .as_ref()
        .ok_or(NativeError::InvalidState)?
        .validate_melee_timer_fields(profile.combat.as_ref().ok_or(NativeError::InvalidState)?)?;
    owner
        .proof
        .validate(profile, current, phase, current_fields(current)?)
}
pub(in crate::native) fn validate_current(
    profile: &Profile,
    current: &State,
) -> Result<(), NativeError> {
    validate_weapon_history(profile, current)?;
    match (&profile.player_melee, &current.player_melee) {
        (None, None) => Ok(()),
        (Some(policy), Some(_)) => {
            policy.validate(profile)?;
            validate_fields(profile, current, state::Phase::PostUnit)?;
            current
                .player_melee
                .as_ref()
                .ok_or(NativeError::InvalidState)?
                .proof
                .validate_closed(current)?;
            if let Some(reset) = current
                .combat
                .as_ref()
                .and_then(|combat| combat.melee_reset.as_ref())
            {
                let (at, _) = reset.source_origin();
                let tick = at
                    .checked_sub(profile.clock_origin_ms)
                    .ok_or(NativeError::InvalidState)?
                    / u64::from(profile.tick_ms);
                current
                    .player_melee
                    .as_ref()
                    .unwrap()
                    .proof
                    .validate_latest_reset(reset, tick)?;
            }
            if current
                .combat
                .as_ref()
                .is_some_and(|combat| combat.melee_timers.melee_attacking())
            {
                validate_supported_concurrency(profile, current)?;
            }
            Ok(())
        }
        _ => Err(NativeError::InvalidState),
    }
}

pub(in crate::native) fn record_spell_reset(
    profile: &Profile,
    current: &mut State,
    scheduler: &benilla_sim::event_processor::EventCallbackSnapshot,
) -> Result<(), NativeError> {
    if profile.player_melee.is_none() {
        return Ok(());
    }
    let witness = current
        .combat
        .as_ref()
        .and_then(|combat| combat.melee_reset.as_ref())
        .ok_or(NativeError::InvalidState)?
        .clone();
    // player_event_update temporarily owns the real queue. Replay the new
    // writer against its source-owned callback snapshot, never an empty queue.
    // Only melee fields return to the live owner; final validation uses the
    // actual committed scheduler, not this transient view.
    let mut validation_view = current.clone();
    validation_view.player_events.queue = witness.release_queue(current.now_ms, scheduler)?;
    let cause = source::SourceCause::SpellReset {
        key: key(current, state::Phase::PreUnit)?,
        witness: Box::new(witness),
    };
    commit_cause(profile, &mut validation_view, state::Phase::PreUnit, cause)?;
    current.player_melee = validation_view.player_melee;
    current
        .combat
        .as_mut()
        .ok_or(NativeError::InvalidState)?
        .melee_timers = validation_view
        .combat
        .ok_or(NativeError::InvalidState)?
        .melee_timers;
    Ok(())
}

pub(in crate::native) struct PendingSwing {
    key: state::CauseKey,
    context: context::SourceContext,
    resolved: context::ReplayedContext,
}
impl PendingSwing {
    pub(in crate::native) fn action(&self) -> u64 {
        self.key.action
    }
    pub(in crate::native) fn target(&self) -> NpcId {
        self.context.target()
    }
    pub(in crate::native) fn input(&self) -> benilla_sim::player_melee::MeleeInput {
        self.resolved.input
    }
    pub(in crate::native) fn target_health(&self) -> u32 {
        self.resolved.target_health
    }
    pub(in crate::native) fn victim_can_attack(&self) -> bool {
        self.resolved.victim_can_attack
    }
}
pub(in crate::native) fn prepare_tick(
    profile: &Profile,
    current: &mut State,
) -> Result<(Option<PendingSwing>, Vec<Event>), NativeError> {
    if profile.player_melee.is_none() {
        return Ok((None, vec![]));
    }
    validate_fields(profile, current, state::Phase::PostUnit)?;
    let before = current_fields(current)?;
    let Some(target) = before.lifecycle.victim else {
        return Ok((None, vec![]));
    };
    validate_supported_concurrency(profile, current)?;
    if !current.player_alive() {
        return Err(NativeError::InvalidState);
    }
    // Unit spell events and finished-slot cleanup precede this check. Casting
    // pauses execution only: countdown, victim and last error are retained.
    if casting::blocks(current)? || before.remaining_ms[0] != 0 {
        return Ok((None, vec![]));
    }
    let npc = current
        .npcs
        .get(&target.spawn)
        .filter(|npc| npc.id == target)
        .ok_or(NativeError::InvalidState)?;
    let context =
        context::SourceContext::capture(profile, current, npc, before, state::Phase::PostUnit)?;
    let resolved = context.replay(profile, current)?;
    let key = key(current, state::Phase::PostUnit)?;
    let (error, executes) = source::ready_decision(&resolved);
    if executes && resolved.target_evading {
        return Err(NativeError::UnsupportedMechanic(
            "player melee evade damage",
        ));
    }
    if executes {
        return Ok((
            Some(PendingSwing {
                key,
                context,
                resolved,
            }),
            vec![],
        ));
    }
    let events = if error != before.last_error {
        vec![Event::PlayerSwingError { code: error }]
    } else {
        vec![]
    };
    commit_cause(
        profile,
        current,
        state::Phase::PostUnit,
        source::SourceCause::Ready {
            key,
            context: Box::new(context),
            swing: None,
        },
    )?;
    Ok((None, events))
}
pub(in crate::native) fn record_npc_reactive(
    profile: &Profile,
    current: &mut State,
    pending: &PendingSwing,
    draws: Vec<Draw>,
    result: &benilla_sim::player_melee::MeleeResolution,
) -> Result<(), NativeError> {
    if let Some(proof) = NpcDefenseReactive::capture(
        profile,
        current,
        pending.context.clone(),
        pending.key,
        draws,
        result,
    )? {
        current
            .npcs
            .get_mut(&pending.target().spawn)
            .filter(|npc| npc.id == pending.target())
            .ok_or(NativeError::InvalidState)?
            .defense_reactive = Some(proof);
    }
    Ok(())
}

pub(in crate::native) fn finish_swing(
    profile: &Profile,
    current: &mut State,
    pending: PendingSwing,
    draws: Vec<Draw>,
    after_damage: MeleeExecutionBinding,
    loot: Option<CorpseLoot>,
) -> Result<Vec<Event>, NativeError> {
    let old_error = pending.resolved.predecessor.last_error;
    let after = MeleeExecutionBinding::capture(profile, current)?;
    let after_equipment = current
        .combat
        .as_ref()
        .ok_or(NativeError::InvalidState)?
        .equipment
        .snapshot();
    let witness = source::SwingWitness {
        draws,
        after_damage,
        after,
        after_equipment,
        loot,
    };
    commit_cause(
        profile,
        current,
        state::Phase::PostUnit,
        source::SourceCause::Ready {
            key: pending.key,
            context: Box::new(pending.context),
            swing: Some(Box::new(witness)),
        },
    )?;
    // Source value0 is also NOT_IN_RANGE, even following a successful swing.
    Ok(if old_error != 0 {
        vec![Event::PlayerSwingError { code: 0 }]
    } else {
        vec![]
    })
}

#[cfg(test)]
pub(in crate::native) mod tests;

#[cfg(test)]
mod cast_source_tests;
