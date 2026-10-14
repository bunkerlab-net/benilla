// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-derived phase contexts for preparation, release and applied auras.
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
    /// SpellEntry metadata stays fixed; only current GetSpellRank/CheckPower
    /// values may differ. Both witnesses are separately source-validated on load.
    pub(in crate::native) fn release_context(
        &self,
        prepared: &Self,
    ) -> Result<spell::MageProjectileReleaseContext, NativeError> {
        if self.ability.id != prepared.ability.id
            || self.ability.school != prepared.ability.school
            || self.captured_ms <= prepared.captured_ms
            || self.level < prepared.level
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        match (&prepared.progression, &self.progression) {
            (Some(a), Some(b)) => {
                if !b.training.calls.starts_with(&a.training.calls)
                    || !b.derived_calls.starts_with(&a.derived_calls)
                    || a.player.skills.len() != b.player.skills.len()
                    || a.player
                        .skills
                        .iter()
                        .zip(&b.player.skills)
                        .any(|(old, new)| {
                            old.id != new.id
                                || old.position != new.position
                                || old.value > new.value
                                || old.maximum > new.maximum
                                || old.step != new.step
                                || old.permanent_bonus != new.permanent_bonus
                                || old.temporary_bonus != new.temporary_bonus
                        })
                {
                    return Err(NativeError::InvalidCheckpoint);
                }
            }
            (None, None) => {}
            _ => return Err(NativeError::InvalidCheckpoint),
        }
        Ok(spell::MageProjectileReleaseContext::from_specs(
            prepared.spec()?,
            self.spec()?,
        )?)
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
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

    #[test]
    fn release_cannot_reverse_independently_valid_same_level_item_break_history() {
        let mut world = fixture::world();
        let before = ExecutionBinding::capture(&world.profile, &world.state, 60001).unwrap();
        let g = world.profile.quest_progression.as_ref().unwrap();
        let before_combat = world.state.combat.as_ref().unwrap().clone();
        let current = world.state.quest_progression.as_mut().unwrap();
        let view = current.player.view();
        let live = init::Resources {
            health: view.numeric.health,
            max_health: view.numeric.max_health,
            mana: view.numeric.powers[0],
            max_mana: view.numeric.max_powers[0],
        };
        let mut after_combat = before_combat.clone();
        let recompute = after_combat.apply_item_break(4, live).unwrap();
        current
            .item_break(
                g,
                &before_combat,
                &after_combat,
                4,
                &recompute,
                live.health,
                live.mana,
            )
            .unwrap();
        world.state.combat = Some(after_combat);
        world.state.now_ms += u64::from(world.profile.tick_ms);
        let prepared = ExecutionBinding::capture(&world.profile, &world.state, 60001).unwrap();
        assert_eq!(prepared.level, before.level);
        assert_eq!(prepared.ability, before.ability);
        let old = before.progression.as_ref().unwrap();
        let new = prepared.progression.as_ref().unwrap();
        assert_eq!(old.training, new.training);
        assert_eq!(old.player.skills, new.player.skills);
        assert_eq!(new.derived_calls.len(), old.derived_calls.len() + 1);
        assert!(matches!(
            new.derived_calls.last(),
            Some(DerivedCall::ItemBreak { .. })
        ));
        prepared.release_context(&before).unwrap();

        world.state.now_ms += u64::from(world.profile.tick_ms);
        let mut stale_release = before;
        stale_release.captured_ms = world.state.now_ms;
        for binding in [&prepared, &stale_release] {
            binding
                .validate(
                    &world.profile,
                    world.state.quest_progression.as_ref(),
                    world.state.now_ms,
                )
                .unwrap();
        }
        assert!(stale_release.captured_ms > prepared.captured_ms);
        assert_eq!(
            spell::MageProjectileReleaseContext::from_specs(
                prepared.spec().unwrap(),
                stale_release.spec().unwrap(),
            )
            .unwrap(),
            spell::MageProjectileReleaseContext {
                spell_rank: prepared.spec().unwrap().spell_rank,
                mana_cost: prepared.spec().unwrap().mana_cost,
            }
        );
        assert_eq!(
            stale_release.release_context(&prepared).unwrap_err(),
            NativeError::InvalidCheckpoint,
        );
        let mut current_release = prepared.clone();
        current_release.captured_ms = world.state.now_ms;
        current_release.release_context(&prepared).unwrap();
    }
}
