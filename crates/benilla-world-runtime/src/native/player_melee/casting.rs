// SPDX-License-Identifier: GPL-3.0-or-later
//! Unit::IsNonMeleeSpellCasted(false), pinned CMaNGOS Classic 8ec338a1.
//! A retained reachable slot context, not an authenticated command trajectory.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GenericSlot {
    id: u64,
    target: NpcId,
    prepared: ExecutionBinding,
    released: Option<ExecutionBinding>,
    execution: spell::CastCheckpoint,
    #[serde(deserialize_with = "crate::native::creature_collision_width::required_option")]
    callback_ms: Option<u64>,
}

pub(super) fn blocks(current: &State) -> Result<bool, NativeError> {
    let Some(id) = current.player_events.current_spell() else {
        return Ok(false);
    };
    match current.casts.get(&id) {
        Some(cast) => Ok(matches!(
            cast.execution.state(),
            spell::CastState::Casting { .. }
        )),
        None if current
            .player_events
            .spell_lifetime(id)
            .is_some_and(|life| life.finished.is_some()) =>
        {
            Ok(false)
        }
        None => Err(NativeError::InvalidState),
    }
}

impl GenericSlot {
    pub(super) fn capture(current: &State) -> Result<Option<Self>, NativeError> {
        let Some(id) = current.player_events.current_spell() else {
            return Ok(None);
        };
        let Some(cast) = current.casts.get(&id) else {
            blocks(current)?;
            return Ok(None);
        };
        let lifetime = current
            .player_events
            .spell_lifetime(id)
            .ok_or(NativeError::InvalidState)?;
        Ok(Some(Self {
            id,
            target: cast.target,
            prepared: cast.binding.clone(),
            released: cast.release_binding.clone(),
            execution: cast.execution.checkpoint(),
            // AttackStart may precede the first SpellEvent callback.
            callback_ms: lifetime.last_callback_ms,
        }))
    }

    pub(super) fn replay_work(&self) -> u64 {
        self.prepared.replay_work().saturating_add(
            self.released
                .as_ref()
                .map_or(0, ExecutionBinding::replay_work),
        )
    }

    pub(super) fn blocks(
        &self,
        profile: &Profile,
        current: &State,
        captured_ms: u64,
        phase: state::Phase,
    ) -> Result<bool, NativeError> {
        if self.id == 0 || self.id >= current.next_cast_id || captured_ms > current.now_ms {
            return Err(NativeError::InvalidCheckpoint);
        }
        #[cfg(any(test, feature = "synthetic-native-tests"))]
        let identity = (
            profile.digest(),
            current.domain,
            current.world,
            current.player,
        );
        #[cfg(any(test, feature = "synthetic-native-tests"))]
        self.prepared.validate(
            profile,
            current.quest_progression.as_ref(),
            captured_ms,
            identity,
        )?;
        let release = self
            .released
            .as_ref()
            .map(|released| {
                #[cfg(any(test, feature = "synthetic-native-tests"))]
                released.validate(
                    profile,
                    current.quest_progression.as_ref(),
                    captured_ms,
                    identity,
                )?;
                released.release_context(&self.prepared)
            })
            .transpose()?;
        let execution = spell::MageProjectileCast::restore_with_release(
            self.execution.clone(),
            self.prepared.spec()?.clone(),
            release,
            self.callback_ms.unwrap_or(self.prepared.captured_ms),
        )?;
        match self.callback_ms {
            Some(_) => execution.validate_fixed_step(profile.clock_origin_ms, profile.tick_ms)?,
            None if captured_ms == execution.created_ms() => {
                execution.validate_prepared_fixed_step(profile.clock_origin_ms, profile.tick_ms)?
            }
            None => return Err(NativeError::InvalidCheckpoint),
        }
        let owner = current
            .npcs
            .get(&self.target.spawn)
            .ok_or(NativeError::InvalidCheckpoint)?;
        interactions::spell_authority::historical_target_profile(
            profile,
            owner,
            self.target,
            self.prepared.captured_ms,
        )?;
        if execution.created_ms() != self.prepared.captured_ms
            || execution.caster_id() != SOLO_UNIT
            || execution.target_id() != self.target.spawn
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        match (execution.state(), &self.released) {
            (spell::CastState::Casting { .. }, None) => {
                // A command can observe last tick's current slot before this
                // tick's Unit event pass. Ready replay always rejects Casting;
                // this is not completed-frame scheduled restore authority.
                let callback_ms = self.callback_ms.unwrap_or(self.prepared.captured_ms);
                if callback_ms != captured_ms
                    && (phase == state::Phase::PostUnit
                        || callback_ms.checked_add(u64::from(profile.tick_ms)) != Some(captured_ms))
                {
                    return Err(NativeError::InvalidCheckpoint);
                }
                Ok(true)
            }
            (spell::CastState::Traveling { impact_at_ms }, Some(released)) => {
                let horizon = match phase {
                    state::Phase::PreUnit => captured_ms.saturating_sub(u64::from(profile.tick_ms)),
                    state::Phase::PostUnit => captured_ms,
                };
                if impact_at_ms <= horizon || self.callback_ms.is_none_or(|at| at > captured_ms) {
                    return Err(NativeError::InvalidCheckpoint);
                }
                let launch = execution
                    .launch_context()
                    .ok_or(NativeError::InvalidCheckpoint)?;
                #[cfg(any(test, feature = "synthetic-native-tests"))]
                interactions::spell_authority::validate_historical_launch(
                    profile,
                    owner,
                    released,
                    self.target,
                    &launch,
                )?;
                let delay = spell::projectile_delay(
                    launch.distance_squared,
                    self.prepared.spec()?.projectile_speed,
                )?;
                if impact_at_ms.checked_sub(u64::from(delay)) != Some(released.captured_ms) {
                    return Err(NativeError::InvalidCheckpoint);
                }
                Ok(false)
            }
            _ => Err(NativeError::InvalidCheckpoint),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::{synthetic_quest_fixture, synthetic_two_owner_fixture::CompleteAuthority};

    #[test]
    fn retained_generic_slot_rejects_identity_clock_and_phase_substitution() {
        let mut world = super::super::tests::world();
        let target = world.state.npcs[&2].id;
        let input = synthetic_quest_fixture::command(
            &world,
            Command::Cast {
                ability: 60001,
                target,
            },
        );
        world
            .step(1, Some(input), &mut CompleteAuthority::default())
            .unwrap();
        let slot = GenericSlot::capture(&world.state).unwrap().unwrap();
        let at = world.state.now_ms;
        assert!(slot
            .blocks(&world.profile, &world.state, at, state::Phase::PostUnit)
            .unwrap());
        for mutation in 0..6 {
            let mut changed = slot.clone();
            match mutation {
                0 => changed.id = 0,
                1 => changed.target.generation += 1,
                2 => changed.callback_ms = changed.callback_ms.map(|at| at + 1),
                3 => changed.prepared.captured_ms += u64::from(world.profile.tick_ms),
                4 => changed.released = Some(changed.prepared.clone()),
                _ => changed.callback_ms = None,
            }
            assert!(
                changed
                    .blocks(&world.profile, &world.state, at, state::Phase::PostUnit)
                    .is_err(),
                "mutation {mutation}"
            );
        }
        // A dangling live slot is not an empty/finished slot.
        world.state.casts.clear();
        assert!(blocks(&world.state).is_err());
        assert!(GenericSlot::capture(&world.state).is_err());
    }
}
