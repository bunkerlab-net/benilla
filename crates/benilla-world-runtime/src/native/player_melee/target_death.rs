// SPDX-License-Identifier: GPL-3.0-or-later
//! CMaNGOS 8ec338a1 Unit::SetDeathState -> CombatStop -> RemoveAllAttackers
//! -> attacker's AttackStop (Unit.cpp:8564,6369,6422-6428,6339-6363).
//! Called after the synchronous target death receipt installs its retained life
//! proof. Unlike a melee kill, a spell kill does not reset the attacker's timer.
use super::source::SourceCause;
use super::state::{CauseKey, Lifecycle, Phase, ReplayedCause, Write, WriteStamp};
use crate::native::{ai, Event, NativeError, Profile, State};
use crate::NpcId;
use serde::{Deserialize, Serialize};

/// Reachable owner context for RemoveAllAttackers; deliberately independent of
/// the SpellEvent's temporarily detached executing cast and event queue.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TargetDeathContext {
    target: NpcId,
    previous: NpcId,
    captured_ms: u64,
}

pub(in crate::native) fn after_spell_kill(
    profile: &Profile,
    current: &mut State,
    target: NpcId,
    unit_updated: bool,
    scheduler: Option<&benilla_sim::event_processor::EventCallbackSnapshot>,
) -> Result<Vec<Event>, NativeError> {
    if profile.player_melee.is_none() {
        return Ok(vec![]);
    }
    // During a player SpellEvent the queue is owned by its transaction. Validate
    // older reset writers against that actual callback, not State's empty slot.
    if let Some(scheduler) = scheduler {
        let mut view = current.clone();
        view.player_events.queue =
            benilla_sim::event_processor::EventProcessor::restore(scheduler.queue.clone())?;
        let events = after_spell_kill(profile, &mut view, target, unit_updated, None)?;
        current.player_melee = view.player_melee;
        if let (Some(current), Some(view)) = (&mut current.combat, view.combat) {
            current.melee_timers = view.melee_timers;
        }
        return Ok(events);
    }

    let phase = if unit_updated {
        Phase::PostUnit
    } else {
        Phase::PreUnit
    };
    super::validate_fields(profile, current, phase)?;
    let fields = super::current_fields(current)?;
    // RemoveAllAttackers only calls this player's AttackStop when the player
    // actually attacks this exact life. A different spell target cannot cancel it.
    if fields.lifecycle.victim != Some(target) {
        return Ok(vec![]);
    }
    let npc = current
        .npcs
        .get(&target.spawn)
        .filter(|npc| npc.id == target)
        .ok_or(NativeError::InvalidState)?;
    if !fields.lifecycle.melee_attacking
        || npc.health != 0
        || npc.ai.phase() != ai::Phase::Corpse
        || npc.ai.death_at_ms() != Some(current.now_ms)
    {
        return Err(NativeError::InvalidState);
    }
    let context = TargetDeathContext {
        target,
        previous: fields.lifecycle.victim.ok_or(NativeError::InvalidState)?,
        captured_ms: current.now_ms,
    };
    let key = super::key(current, phase)?;
    super::commit_cause(
        profile,
        current,
        phase,
        SourceCause::TargetDeath {
            key,
            context: Box::new(context),
        },
    )?;
    Ok(vec![
        Event::PlayerAttackCancel,
        Event::PlayerAttackStop { target },
    ])
}

pub(super) fn replay_stop(
    profile: &Profile,
    current: &State,
    key: CauseKey,
    context: &TargetDeathContext,
) -> Result<ReplayedCause, NativeError> {
    let target = context.target;
    let captured_ms = key
        .tick
        .checked_mul(u64::from(profile.tick_ms))
        .and_then(|elapsed| profile.clock_origin_ms.checked_add(elapsed))
        .ok_or(NativeError::InvalidCheckpoint)?;
    if context.captured_ms != captured_ms || context.previous != target {
        return Err(NativeError::InvalidCheckpoint);
    }
    validate_death(profile, current, key, target)?;
    Ok(ReplayedCause {
        key,
        lifecycle: Some(Write {
            stamp: WriteStamp {
                cause: key,
                subphase: 1,
            },
            value: Lifecycle {
                melee_attacking: false,
                victim: None,
            },
        }),
        base: None,
        off: None,
        last_error: None,
    })
}

fn validate_death(
    profile: &Profile,
    current: &State,
    key: CauseKey,
    target: NpcId,
) -> Result<(), NativeError> {
    let owner = current
        .npcs
        .get(&target.spawn)
        .ok_or(NativeError::InvalidCheckpoint)?;
    let life = owner
        .source_generation
        .as_ref()
        .and_then(|owner| owner.historical(target))
        .ok_or(NativeError::InvalidCheckpoint)?;
    // A bounded source death record, not a caller-supplied dead flag/timestamp.
    // Exact generation history remains available after this spawn respawns.
    if key.tick == 0
        || key.action == 0
        || key.tick > current.tick
        || life.death_tick() != Some(key.tick)
    {
        return Err(NativeError::InvalidCheckpoint);
    }
    let captured_ms = key
        .tick
        .checked_mul(u64::from(profile.tick_ms))
        .and_then(|elapsed| profile.clock_origin_ms.checked_add(elapsed))
        .ok_or(NativeError::InvalidCheckpoint)?;
    crate::native::interactions::spell_authority::historical_target_profile(
        profile,
        owner,
        target,
        captured_ms,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::{
        synthetic_quest_fixture, synthetic_two_owner_fixture::CompleteAuthority, Command,
    };

    #[test]
    fn target_stop_requires_the_retained_death_of_the_exact_life() {
        let mut world = crate::native::player_melee::tests::world();
        let target = world.state.npcs[&2].id;
        let input = synthetic_quest_fixture::command(&world, Command::AttackStart { target });
        let mut authority = CompleteAuthority::default();
        world.step(1, Some(input), &mut authority).unwrap();
        while world.state.npcs[&2].health != 0 && world.state.tick < 101 {
            world
                .step(world.state.tick + 1, None, &mut authority)
                .unwrap();
        }
        assert_eq!(world.state.npcs[&2].health, 0);
        let key = CauseKey {
            tick: world.state.tick,
            phase: Phase::PostUnit,
            action: 1,
        };
        validate_death(&world.profile, &world.state, key, target).unwrap();
        assert!(validate_death(
            &world.profile,
            &world.state,
            CauseKey {
                tick: key.tick + 1,
                ..key
            },
            target
        )
        .is_err());
        assert!(validate_death(
            &world.profile,
            &world.state,
            key,
            NpcId {
                generation: target.generation + 1,
                ..target
            }
        )
        .is_err());
        let fields = super::super::current_fields(&world.state).unwrap();
        // Melee death already stopped this attacker: no duplicate cancel/stop.
        assert!(
            after_spell_kill(&world.profile, &mut world.state, target, true, None)
                .unwrap()
                .is_empty()
        );
        assert_eq!(super::super::current_fields(&world.state).unwrap(), fields);
    }
}
