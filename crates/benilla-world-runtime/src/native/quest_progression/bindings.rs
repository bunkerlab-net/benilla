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
    owner: HistoryOwner,
    training: TrainingBinding,
    training_prefix: HistoryPrefix,
    recipe: init::StatRecipeSnapshot,
    derived: DerivedStats,
    derived_prefix: HistoryPrefix,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrainingBinding {
    version: u32,
    identity: crate::Digest,
    abilities: Vec<init::KnownAbility>,
}
impl TrainingBinding {
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    fn view<'a>(&'a self, calls: &'a [init::TrainingCallSnapshot]) -> init::TrainingView<'a> {
        init::TrainingView {
            version: self.version,
            identity: self.identity,
            abilities: &self.abilities,
            calls,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryOwner {
    profile: crate::Digest,
    genesis: crate::Digest,
    domain: Domain,
    world: crate::WorldId,
    player: crate::PlayerId,
}
/// Prefixes refer to the durable owner journals, not the clearable tick outbox.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryPrefix {
    count: u32,
    digest: crate::Digest,
}
impl HistoryPrefix {
    fn capture<T: Serialize>(
        owner: HistoryOwner,
        kind: &str,
        calls: &[T],
    ) -> Result<Self, NativeError> {
        use sha2::{Digest, Sha256};
        struct Writer(Sha256);
        impl std::io::Write for Writer {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.update(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let count = u32::try_from(calls.len()).map_err(|_| NativeError::TooLarge)?;
        let mut writer = Writer(Sha256::new());
        writer
            .0
            .update(b"benilla-native-execution-history-prefix-v1\0");
        serde_json::to_writer(
            &mut writer,
            &(
                owner,
                QUEST_PROGRESSION_POLICY,
                pg::POLICY_ID,
                init::LIVE_TRAINING_POLICY_ID,
                pg::derived::POLICY_ID,
                kind,
                count,
                calls,
            ),
        )
        .map_err(|_| NativeError::InvalidState)?;
        Ok(Self {
            count,
            digest: writer.0.finalize().into(),
        })
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    fn resolve<'a, T: Serialize>(
        &self,
        owner: HistoryOwner,
        kind: &str,
        calls: &'a [T],
    ) -> Result<&'a [T], NativeError> {
        if self.count > 4096 {
            return Err(NativeError::InvalidCheckpoint);
        }
        let prefix = calls
            .get(..self.count as usize)
            .ok_or(NativeError::InvalidCheckpoint)?;
        if *self != Self::capture(owner, kind, prefix)? {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok(prefix)
    }
    // Both prefixes must each validate against the same current owner history
    // before this ordering comparison establishes source ancestry.
    fn follows(&self, previous: &Self) -> bool {
        self.count > previous.count
            || (self.count == previous.count && self.digest == previous.digest)
    }
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
            (Some(q), Some(c)) => {
                let g = profile
                    .quest_progression
                    .as_ref()
                    .ok_or(NativeError::InvalidState)?;
                let owner = HistoryOwner {
                    profile: profile.digest(),
                    genesis: g.identity,
                    domain: state.domain,
                    world: state.world,
                    player: state.player,
                };
                Some(ProgressionBinding {
                    player: q.player.view().clone(),
                    owner,
                    training: TrainingBinding {
                        version: q.training.version,
                        identity: q.training.identity,
                        abilities: q.training.abilities.clone(),
                    },
                    training_prefix: HistoryPrefix::capture(owner, "training", &q.training.calls)?,
                    recipe: c.recipe.snapshot(),
                    derived: q.derived.clone(),
                    derived_prefix: HistoryPrefix::capture(owner, "derived", &q.derived_calls)?,
                })
            }
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
    /// Work upper bound for restoring this witness. Counts include both source
    /// training and derived history; the checkpoint enforces an aggregate cap.
    pub(in crate::native) fn replay_work(&self) -> u64 {
        self.progression.as_ref().map_or(1, |p| {
            64 + u64::from(p.training_prefix.count) + u64::from(p.derived_prefix.count)
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
                if a.owner != b.owner
                    || !b.training_prefix.follows(&a.training_prefix)
                    || !b.derived_prefix.follows(&a.derived_prefix)
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
    pub(in crate::native) fn historical_create_mana(
        &self,
        profile: &Profile,
    ) -> Result<u32, NativeError> {
        match (&profile.quest_progression, &self.progression) {
            (Some(_), Some(h)) => Ok(h.recipe.modifiers.create_mana),
            (None, None) => Ok(profile
                .combat
                .as_ref()
                .ok_or(NativeError::InvalidCheckpoint)?
                .recipe
                .snapshot()
                .modifiers
                .create_mana),
            _ => Err(NativeError::InvalidCheckpoint),
        }
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(in crate::native) fn historical_spell_crit(
        &self,
        profile: &Profile,
    ) -> Result<spell::SpellFloat, NativeError> {
        match (&profile.quest_progression, &self.progression) {
            (Some(g), Some(h)) => {
                let p = &h.player;
                let fresh = g.initial_combat.recipe.rebase(
                    p.level,
                    p.modifiers.create,
                    p.modifiers.create_health,
                    p.modifiers.create_mana,
                )?;
                let recipe = init::StatRecipe::restore(&fresh, &h.recipe)?;
                Ok(recipe
                    .current(init::Resources {
                        health: p.numeric.health,
                        max_health: p.numeric.max_health,
                        mana: p.numeric.powers[0],
                        max_mana: p.numeric.max_powers[0],
                    })?
                    .spell_crit_percent)
            }
            (None, None) => {
                let c = profile
                    .combat
                    .as_ref()
                    .ok_or(NativeError::InvalidCheckpoint)?;
                // Selection forbids durability-driven stat mutation without the
                // historical recipe/callback owner above.
                if c.equipment.items.iter().any(|i| i.max_durability != 0) {
                    return Err(NativeError::InvalidCheckpoint);
                }
                Ok(c.stats.crit_percent)
            }
            _ => Err(NativeError::InvalidCheckpoint),
        }
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(in crate::native) fn validate(
        &self,
        profile: &Profile,
        current: Option<&QuestProgressionState>,
        now: u64,
        identity: (crate::Digest, Domain, crate::WorldId, crate::PlayerId),
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
                let owner = HistoryOwner {
                    profile: identity.0,
                    genesis: g.identity,
                    domain: identity.1,
                    world: identity.2,
                    player: identity.3,
                };
                if binding.owner != owner {
                    return Err(NativeError::InvalidCheckpoint);
                }
                let training_calls =
                    binding
                        .training_prefix
                        .resolve(owner, "training", &current.training.calls)?;
                let derived_calls =
                    binding
                        .derived_prefix
                        .resolve(owner, "derived", &current.derived_calls)?;
                let prior = pg::State::restore(&g.levels, binding.player.clone())?;
                let p = prior.view();
                g.validate_live_skills(p)?;
                let c = current.player.view();
                if p.level != u32::from(self.level)
                    || p.level > c.level
                    || p.skills.len() != c.skills.len()
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
                let (_, training) = derived::validate_history(
                    g,
                    p,
                    binding.training.view(training_calls),
                    &binding.derived,
                    derived_calls,
                    &recipe,
                )?;
                if g.resolve_execution_ability(&training, p, self.ability.id)? != self.ability {
                    return Err(NativeError::InvalidCheckpoint);
                }
            }
            _ => return Err(NativeError::InvalidCheckpoint),
        }
        self.spec()?.validate()?;
        interactions::spell_authority::validate_historical_binding(profile, self)?;
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
                (
                    world.profile.digest(),
                    world.state.domain,
                    world.state.world,
                    world.state.player,
                ),
            )
            .unwrap();
        let g = world.profile.quest_progression.as_ref().unwrap();
        let prior = binding.progression.as_mut().unwrap();
        // This training prefix is internally replayable, and the altered skill
        // remains between initial and current bounds. It is not GiveLevel history.
        let initial = g.training.snapshot();
        prior.training = TrainingBinding {
            version: initial.version,
            identity: initial.identity,
            abilities: initial.abilities.clone(),
        };
        let owner = prior.owner;
        prior.training_prefix = HistoryPrefix::capture(owner, "training", &initial.calls).unwrap();
        prior.derived_prefix =
            HistoryPrefix::capture(owner, "derived", &[] as &[DerivedCall]).unwrap();
        let skill = prior.player.skills.iter_mut().find(|s| s.id == 8).unwrap();
        skill.value = 7;
        let training =
            init::LiveTraining::restore_view(&g.training, prior.training.view(&[]), &prior.player)
                .unwrap();
        binding.ability = g
            .resolve_execution_ability(&training, &prior.player, 60001)
            .unwrap();
        assert!(binding
            .validate(
                &world.profile,
                world.state.quest_progression.as_ref(),
                world.state.now_ms,
                (
                    world.profile.digest(),
                    world.state.domain,
                    world.state.world,
                    world.state.player
                ),
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
        assert_eq!(new.derived_prefix.count, old.derived_prefix.count + 1);
        assert_ne!(new.derived_prefix.digest, old.derived_prefix.digest);
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
                    (
                        world.profile.digest(),
                        world.state.domain,
                        world.state.world,
                        world.state.player,
                    ),
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
    #[test]
    fn prefix_binds_exact_history_kind_policy_world_and_character() {
        let world = fixture::world();
        let binding = ExecutionBinding::capture(&world.profile, &world.state, 60001).unwrap();
        let owner = binding.progression.unwrap().owner;
        let calls = [10u32, 20, 30];
        let prefix = HistoryPrefix::capture(owner, "derived", &calls[..2]).unwrap();
        assert_eq!(
            prefix.resolve(owner, "derived", &calls).unwrap(),
            &calls[..2]
        );
        // A different tail is irrelevant to this prefix; the owner itself must
        // separately prove that tail by full replay.
        assert!(prefix.resolve(owner, "derived", &[10u32, 20, 99]).is_ok());
        assert!(prefix.resolve(owner, "derived", &[10u32, 99, 30]).is_err());
        assert!(prefix.resolve(owner, "derived", &[10u32]).is_err());
        assert!(prefix.resolve(owner, "training", &calls).is_err());
        let divergent = HistoryPrefix::capture(owner, "derived", &[10u32, 99]).unwrap();
        assert_eq!(divergent.count, prefix.count);
        assert!(!divergent.follows(&prefix));
        for part in 0..5 {
            let mut other = owner;
            match part {
                0 => other.profile[0] ^= 1,
                1 => other.genesis[0] ^= 1,
                2 => other.world[0] ^= 1,
                3 => other.player.owner[0] ^= 1,
                _ => other.player.character[0] ^= 1,
            }
            assert!(prefix.resolve(other, "derived", &calls).is_err());
        }
        let mut bad = prefix.clone();
        bad.count = 4097;
        assert!(bad.resolve(owner, "derived", &vec![10u32; 4097]).is_err());
        bad = prefix;
        bad.digest[0] ^= 1;
        assert!(bad.resolve(owner, "derived", &calls).is_err());
    }

    #[test]
    fn actual_nonempty_prefix_survives_valid_tail_but_not_divergence_or_transplant() {
        let mut world = fixture::world();
        let mut a = fixture::QuestAuthority::default();
        fixture::accept(&mut world, &mut a);
        fixture::kill_target(&mut world, &mut a);
        let binding = ExecutionBinding::capture(&world.profile, &world.state, 60001).unwrap();
        assert_eq!(binding.level, 2);
        assert!(binding.progression.as_ref().unwrap().derived_prefix.count > 0);
        let command = fixture::command(
            &world,
            Command::TurnInQuest {
                quest: 70001,
                giver: world.state.npcs[&3].id,
            },
        );
        world
            .step(world.state.tick + 1, Some(command), &mut a)
            .unwrap();
        assert_eq!(world.player_level(), 3);
        let identity = (
            world.profile.digest(),
            world.state.domain,
            world.state.world,
            world.state.player,
        );
        let check = |b: &ExecutionBinding, current: &QuestProgressionState, id| {
            b.validate(&world.profile, Some(current), world.state.now_ms, id)
        };
        let current = world.state.quest_progression.as_ref().unwrap();
        check(&binding, current, identity).unwrap();
        let mut other = identity;
        other.2[0] ^= 1;
        assert!(check(&binding, current, other).is_err());
        other = identity;
        other.3.character[0] ^= 1;
        assert!(check(&binding, current, other).is_err());
        let mut diverged = current.clone();
        let DerivedCall::Level { level, .. } = &mut diverged.derived_calls[0] else {
            unreachable!()
        };
        *level += 1;
        assert!(check(&binding, &diverged, identity).is_err());
        let mut tail = current.clone();
        let last = tail.derived_calls.len() - 1;
        let DerivedCall::Hook { view, .. } = &mut tail.derived_calls[last] else {
            unreachable!()
        };
        view.free_talent_points = 99;
        check(&binding, &tail, identity).unwrap();
        assert!(tail
            .validate_derived(
                world.profile.quest_progression.as_ref().unwrap(),
                world.state.combat.as_ref().unwrap()
            )
            .is_err());
        let mut truncated = current.clone();
        truncated
            .derived_calls
            .truncate(binding.progression.as_ref().unwrap().derived_prefix.count as usize - 1);
        assert!(check(&binding, &truncated, identity).is_err());
        let cp = world.checkpoint().unwrap();
        let mut reopened =
            NativeWorld::restore_synthetic_component_test(world.profile.clone(), cp.clone())
                .unwrap();
        reopened.checkpoint_committed(&cp).unwrap();
        assert!(reopened.records().is_empty());
        check(
            &binding,
            reopened.state.quest_progression.as_ref().unwrap(),
            identity,
        )
        .unwrap();
    }
}
