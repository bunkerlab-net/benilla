// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-derived historical context for launched payloads and applied auras.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionBinding {
    pub ability: Ability,
    pub level: u8,
    pub captured_ms: u64,
    progression: Option<ProgressionBinding>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProgressionBinding {
    player: pg::Snapshot,
    training: init::TrainingSnapshot,
    recipe: init::StatRecipeSnapshot,
    derived: DerivedStats,
    #[serde(deserialize_with = "derived::deserialize_calls")]
    derived_calls: Vec<DerivedCall>,
}
impl ExecutionBinding {
    pub(in crate::native) fn capture(
        profile: &Profile,
        state: &State,
        id: u32,
    ) -> Result<Self, NativeError> {
        let ability = match (&profile.quest_progression, &state.quest_progression) {
            (Some(g), Some(q)) => {
                let training = init::LiveTraining::restore_authoritative(
                    &g.training,
                    &q.training,
                    q.player.view(),
                )?;
                g.resolve_execution_ability(&training, q.player.view(), id)?
            }
            (None, None) => state
                .ability(profile, id)
                .ok_or(NativeError::UnsupportedAbility(id))?
                .clone(),
            _ => return Err(NativeError::InvalidState),
        };
        let progression = match (&state.quest_progression, &state.combat) {
            (Some(q), Some(c)) => Some(ProgressionBinding {
                player: q.player.view().clone(),
                training: q.training.clone(),
                recipe: c.recipe.snapshot(),
                derived: q.derived.clone(),
                derived_calls: q.derived_calls.clone(),
            }),
            (None, _) if profile.quest_progression.is_none() => None,
            _ => return Err(NativeError::InvalidState),
        };
        Ok(Self {
            ability,
            level: state.level(profile),
            captured_ms: state.now_ms,
            progression,
        })
    }
    pub(in crate::native) fn spec(&self) -> Result<&spell::MageProjectileSpec, NativeError> {
        match &self.ability.admission {
            AbilityAdmission::Available(s) => Ok(s),
            _ => Err(NativeError::InvalidState),
        }
    }
    pub(in crate::native) fn validate(
        &self,
        profile: &Profile,
        current: Option<&QuestProgressionState>,
        now: u64,
    ) -> Result<(), NativeError> {
        if self.captured_ms < profile.clock_origin_ms
            || self.captured_ms > now
            || !(self.captured_ms - profile.clock_origin_ms)
                .is_multiple_of(u64::from(profile.tick_ms))
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        match (&profile.quest_progression, current, &self.progression) {
            (None, None, None)
                if profile.ability(self.ability.id) == Some(&self.ability)
                    && self.level == profile.player.level => {}
            (Some(g), Some(current), Some(binding)) => {
                let prior = pg::State::restore(&g.levels, binding.player.clone())?;
                let p = prior.view();
                g.validate_live_skills(p)?;
                let c = current.player.view();
                if p.level != u32::from(self.level)
                    || p.level > c.level
                    || p.skills.len() != c.skills.len()
                    || !current.training.calls.starts_with(&binding.training.calls)
                    || !current.derived_calls.starts_with(&binding.derived_calls)
                    || p.skills.iter().zip(&c.skills).any(|(a, b)| {
                        a.id != b.id
                            || a.position != b.position
                            || a.value > b.value
                            || a.maximum > b.maximum
                            || a.step != b.step
                            || a.permanent_bonus != b.permanent_bonus
                            || a.temporary_bonus != b.temporary_bonus
                    })
                {
                    return Err(NativeError::InvalidCheckpoint);
                }
                let fresh = g.initial_combat.recipe.rebase(
                    p.level,
                    p.modifiers.create,
                    p.modifiers.create_health,
                    p.modifiers.create_mana,
                )?;
                let recipe = init::StatRecipe::restore(&fresh, &binding.recipe)?;
                if recipe.snapshot().modifiers != p.modifiers {
                    return Err(NativeError::InvalidCheckpoint);
                }
                derived::validate_history(
                    g,
                    p,
                    &binding.training,
                    &binding.derived,
                    &binding.derived_calls,
                    &recipe,
                )?;
                let training =
                    init::LiveTraining::restore_authoritative(&g.training, &binding.training, p)?;
                if g.resolve_execution_ability(&training, p, self.ability.id)? != self.ability {
                    return Err(NativeError::InvalidCheckpoint);
                }
            }
            _ => return Err(NativeError::InvalidCheckpoint),
        }
        self.spec()?.validate()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::synthetic_quest_fixture as fixture;
    #[test]
    fn historical_binding_rejects_coherent_missing_level_callback_history() {
        let mut world = fixture::world();
        let mut authority = fixture::QuestAuthority::default();
        fixture::accept(&mut world, &mut authority);
        fixture::kill_target(&mut world, &mut authority);
        let mut binding = ExecutionBinding::capture(&world.profile, &world.state, 60001).unwrap();
        assert_eq!(binding.level, 2);
        binding
            .validate(
                &world.profile,
                world.state.quest_progression.as_ref(),
                world.state.now_ms,
            )
            .unwrap();
        let g = world.profile.quest_progression.as_ref().unwrap();
        let prior = binding.progression.as_mut().unwrap();
        // This training prefix is internally replayable, and the altered skill
        // remains between initial and current bounds. It is not GiveLevel history.
        prior.training = g.training.snapshot();
        prior.derived_calls.clear();
        let skill = prior.player.skills.iter_mut().find(|s| s.id == 8).unwrap();
        skill.value = 7;
        let training =
            init::LiveTraining::restore_authoritative(&g.training, &prior.training, &prior.player)
                .unwrap();
        binding.ability = g
            .resolve_execution_ability(&training, &prior.player, 60001)
            .unwrap();
        assert!(binding
            .validate(
                &world.profile,
                world.state.quest_progression.as_ref(),
                world.state.now_ms
            )
            .is_err());
    }
}
