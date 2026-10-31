// SPDX-License-Identifier: GPL-3.0-or-later
//! Historical player operands retain the actual progression/recipe source prefix.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::native) struct MeleeExecutionBinding {
    level: u8,
    captured_ms: u64,
    progression: ProgressionBinding,
    kill: kills::MeleeKillContext,
}

/// Resolved operands, not native admission or a current-target permission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::native) struct ValidatedMeleeOperands {
    pub level: u8,
    pub maximum_health: u32,
    pub minimum_damage: spell::SpellFloat,
    pub maximum_damage: spell::SpellFloat,
    pub critical_chance: spell::SpellFloat,
    pub hit_chance: spell::SpellFloat,
    pub skill_id: u16,
    pub pure_skill: u16,
    pub effective_skill: u16,
    pub maximum_skill: u16,
    pub intellect: u32,
    pub attack_times_ms: [u32; 2],
    /// None means actual absent/broken mainhand, not an invented weapon.
    pub weapon_item: Option<u32>,
    pub weapon_subclass: Option<u32>,
}

impl MeleeExecutionBinding {
    pub(in crate::native) fn capture(
        profile: &Profile,
        state: &State,
    ) -> Result<Self, NativeError> {
        let g = profile
            .quest_progression
            .as_ref()
            .ok_or(NativeError::UnsupportedMechanic("melee progression source"))?;
        let q = state
            .quest_progression
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let c = state.combat.as_ref().ok_or(NativeError::InvalidState)?;
        // Same authoritative live-training validation as the spell capture,
        // without selecting or manufacturing a spell ability.
        init::LiveTraining::restore_authoritative(&g.training, &q.training, q.player.view())?;
        Ok(Self {
            level: state.level(profile),
            captured_ms: state.now_ms,
            progression: ProgressionBinding::capture(profile, state, g, q, c)?,
            kill: kills::MeleeKillContext::capture(q),
        })
    }

    pub(in crate::native) fn captured_ms(&self) -> u64 {
        self.captured_ms
    }

    pub(in crate::native) fn recipe(&self) -> &init::StatRecipeSnapshot {
        &self.progression.recipe
    }

    pub(in crate::native) fn replay_work(&self) -> u64 {
        64 + 2 * u64::from(self.progression.training_prefix.count)
            + 2 * u64::from(self.progression.derived_prefix.count)
    }

    /// Only the immediate owner-capture path may use this without validate.
    /// This computes source operands; it does NOT prove historical ancestry.
    pub(in crate::native) fn current_operands(
        &self,
        profile: &Profile,
    ) -> Result<ValidatedMeleeOperands, NativeError> {
        let g = profile
            .quest_progression
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let combat = profile.combat.as_ref().ok_or(NativeError::InvalidState)?;
        let p = &self.progression.player;
        if self.progression.owner.profile != profile.digest()
            || self.progression.owner.genesis != g.identity
            || p.class != 8
            || p.level != u32::from(self.level)
            || !(1..=60).contains(&self.level)
        {
            return Err(NativeError::InvalidState);
        }
        g.validate_live_skills(p)?;
        let inputs = g.inputs_for_recipe(self.recipe());
        inputs.validate(p)?;
        let active_weapon = inputs
            .weapons
            .first()
            .filter(|weapon| !weapon.broken && !weapon.inactive);
        let metadata = match active_weapon {
            Some(weapon) => Some(
                combat
                    .equipment_weapons
                    .iter()
                    .find(|item| item.slot == weapon.slot && item.class == 2)
                    .filter(|item| item.skill == weapon.skill)
                    .ok_or(NativeError::InvalidState)?,
            ),
            None => None,
        };
        // Unit::GetWeaponSkillValue / Player::GetWeaponSkillIdForAttack use
        // SKILL_UNARMED when GetWeaponForAttack rejects absent/broken equipment.
        let skill_id = active_weapon.map_or(162, |weapon| weapon.skill);
        let skill = p
            .skills
            .iter()
            .find(|skill| skill.id == skill_id && skill.status != pg::SkillStatus::Deleted)
            .ok_or(NativeError::UnsupportedMechanic("retained melee skill"))?;
        // Player.cpp GetSkill returns uint16(max(0, pure + signed bonuses));
        // Player.h GetSkillValue enables both bonuses (no upper saturation).
        let effective_skill = (i32::from(skill.value)
            + i32::from(skill.permanent_bonus)
            + i32::from(skill.temporary_bonus))
        .max(0) as u16;
        let intellect =
            u32::try_from(p.numeric.attributes.0[3]).map_err(|_| NativeError::InvalidState)?;
        let damage = self.progression.derived.damage[0];
        Ok(ValidatedMeleeOperands {
            level: self.level,
            maximum_health: p.numeric.max_health,
            minimum_damage: damage.minimum,
            maximum_damage: damage.maximum,
            critical_chance: self.progression.derived.crit_percent[0],
            hit_chance: integer_float(self.progression.derived.weapon_hit_chance[0])?,
            skill_id,
            pure_skill: skill.value,
            effective_skill,
            maximum_skill: skill.maximum,
            intellect,
            attack_times_ms: combat.source_attack_times_for_recipe(self.recipe())?,
            weapon_item: metadata.map(|item| item.item_id),
            weapon_subclass: metadata.map(|item| item.subclass),
        })
    }

    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(in crate::native) fn validate(
        &self,
        profile: &Profile,
        current: Option<&QuestProgressionState>,
        now: u64,
        identity: (crate::Digest, Domain, crate::WorldId, crate::PlayerId),
    ) -> Result<ValidatedMeleeOperands, NativeError> {
        if self.captured_ms < profile.clock_origin_ms
            || self.captured_ms > now
            || !(self.captured_ms - profile.clock_origin_ms)
                .is_multiple_of(u64::from(profile.tick_ms))
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        let g = profile
            .quest_progression
            .as_ref()
            .ok_or(NativeError::InvalidCheckpoint)?;
        let current = current.ok_or(NativeError::InvalidCheckpoint)?;
        self.kill.validate_prior(current)?;
        current.validate_weapon_policy(
            profile
                .player_melee
                .as_ref()
                .map(PlayerMeleePolicy::weapon_skill_gain),
        )?;
        self.progression
            .validate(g, current, self.level, identity)?;
        self.current_operands(profile)
    }
}

#[cfg(any(test, feature = "synthetic-native-tests"))]
struct TransitionPair<'a> {
    genesis: &'a QuestProgressionGenesis,
    current: &'a QuestProgressionState,
    prior: pg::State,
    training: init::LiveTraining,
}

#[cfg(any(test, feature = "synthetic-native-tests"))]
impl MeleeExecutionBinding {
    fn transition_pair<'a>(
        &self,
        profile: &'a Profile,
        state: &'a State,
        after: &Self,
    ) -> Result<TransitionPair<'a>, NativeError> {
        if self.captured_ms != after.captured_ms
            || self.progression.owner != after.progression.owner
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        let identity = (profile.digest(), state.domain, state.world, state.player);
        self.validate(
            profile,
            state.quest_progression.as_ref(),
            state.now_ms,
            identity,
        )?;
        after.validate(
            profile,
            state.quest_progression.as_ref(),
            state.now_ms,
            identity,
        )?;
        let g = profile
            .quest_progression
            .as_ref()
            .ok_or(NativeError::InvalidCheckpoint)?;
        let current = state
            .quest_progression
            .as_ref()
            .ok_or(NativeError::InvalidCheckpoint)?;
        let (prior, training) = self
            .progression
            .validate(g, current, self.level, identity)?;
        Ok(TransitionPair {
            genesis: g,
            current,
            prior,
            training,
        })
    }

    /// Reconstruct this binding's exact source prefix, not the final loadout.
    /// Callers must first validate the binding against the containing owner.
    pub(in crate::native) fn recipe_state(
        &self,
        g: &QuestProgressionGenesis,
        current: &QuestProgressionState,
    ) -> Result<init::StatRecipe, NativeError> {
        self.progression.reconstruct_recipe(g, current)
    }

    fn append_exact_callbacks(
        &mut self,
        current: &QuestProgressionState,
        calls: &[DerivedCall],
    ) -> Result<(), NativeError> {
        let before = self.progression.derived_prefix.resolve(
            self.progression.owner,
            "derived",
            &current.derived_calls,
        )?;
        let end = before
            .len()
            .checked_add(calls.len())
            .ok_or(NativeError::InvalidCheckpoint)?;
        if current.derived_calls.get(before.len()..end) != Some(calls) {
            return Err(NativeError::InvalidCheckpoint);
        }
        self.progression.derived_prefix = HistoryPrefix::capture(
            self.progression.owner,
            "derived",
            &current.derived_calls[..end],
        )?;
        Ok(())
    }

    /// Exact callbacks of DealDamage, before ProcSkillsAndReactives. Caller
    /// supplies only the source-resolved lethal target or actual broken slot;
    /// source damage/durability proof is owned by the enclosing completed swing.
    pub(in crate::native) fn validate_damage_callbacks(
        &self,
        profile: &Profile,
        state: &State,
        after: &Self,
        killed: Option<crate::NpcId>,
        broken_slot: Option<u8>,
    ) -> Result<(), NativeError> {
        let TransitionPair {
            genesis: g,
            current,
            prior,
            training,
        } = self.transition_pair(profile, state, after)?;
        if killed.is_some() && broken_slot.is_some() {
            return Err(NativeError::InvalidCheckpoint);
        }
        let mut expected = self.clone();
        if let Some(target) = killed {
            self.kill
                .validate_transition(&after.kill, current, target)?;
            let owner = state
                .npcs
                .get(&target.spawn)
                .ok_or(NativeError::InvalidCheckpoint)?;
            let target_profile = interactions::spell_authority::historical_target_profile(
                profile,
                owner,
                target,
                self.captured_ms,
            )?;
            let tx = kills::replay_melee_award(
                g,
                kills::MeleeAwardSeed {
                    player: prior,
                    training,
                    derived: self.progression.derived.clone(),
                    inputs: g.inputs_for_recipe(self.recipe()),
                },
                &self.kill,
                state.world,
                &target_profile,
                target,
            )?;
            if tx.rest != kill::RestState::fresh_player() {
                return Err(NativeError::InvalidCheckpoint);
            }
            expected.append_exact_callbacks(current, &tx.calls)?;
            let training = tx.training.snapshot();
            expected.progression.training = TrainingBinding {
                version: training.version,
                identity: training.identity,
                abilities: training.abilities.clone(),
            };
            expected.progression.training_prefix =
                HistoryPrefix::capture(self.progression.owner, "training", &training.calls)?;
            let p = tx.player.view();
            let recipe = self.recipe_state(g, current)?.rebase(
                p.level,
                p.modifiers.create,
                p.modifiers.create_health,
                p.modifiers.create_mana,
            )?;
            expected.level = p
                .level
                .try_into()
                .map_err(|_| NativeError::InvalidCheckpoint)?;
            expected.progression.recipe = recipe.snapshot();
            expected.progression.player = tx.player.snapshot();
            expected.progression.derived = tx.derived;
            expected.kill = after.kill.clone(); // exact increment/unchanged inputs checked above
        } else if let Some(slot) = broken_slot {
            let before = prior.snapshot();
            let mut recipe = self.recipe_state(g, current)?;
            let before_recipe = recipe.snapshot();
            if before_recipe.broken_slots.contains(&slot) {
                return Err(NativeError::InvalidCheckpoint);
            }
            let result = recipe.set_item_broken(
                slot,
                true,
                init::Resources {
                    health: before.numeric.health,
                    max_health: before.numeric.max_health,
                    mana: before.numeric.powers[0],
                    max_mana: before.numeric.max_powers[0],
                },
            )?;
            let after_recipe = recipe.snapshot();
            let before_inputs = g.inputs_for_recipe(&before_recipe);
            let after_inputs = g.inputs_for_recipe(&after_recipe);
            expected.progression.derived.apply_item_break(
                &before,
                &before_inputs,
                &after_inputs,
                slot,
                &result,
            )?;
            let mut view = before.clone();
            view.modifiers = after_recipe.modifiers.clone();
            let mut powers = view.numeric.powers;
            powers[0] = result.resources.mana;
            view.numeric = pg::NumericState::from_modifiers(
                view.level,
                &view.modifiers,
                result.resources.health,
                powers,
            )?;
            view.alive = result.resources.health != 0;
            expected.progression.player = pg::State::restore(&g.levels, view)?.snapshot();
            expected.progression.recipe = after_recipe.clone();
            expected.append_exact_callbacks(
                current,
                &[DerivedCall::ItemBreak {
                    before: Box::new(before),
                    before_recipe: Box::new(before_recipe),
                    after_recipe: Box::new(after_recipe),
                    before_inputs,
                    after_inputs,
                    slot,
                    recompute: Box::new(result),
                }],
            )?;
        }
        if expected != *after {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok(())
    }

    /// Runs after damage/kill callbacks, using their actual post-level-up skill,
    /// attributes and current broken/unarmed recipe, never pre-swing operands.
    pub(in crate::native) fn validate_weapon_callback(
        &self,
        profile: &Profile,
        state: &State,
        after: &Self,
        weapon: Option<(u16, u16)>,
    ) -> Result<(), NativeError> {
        let TransitionPair {
            genesis: g,
            current,
            prior,
            ..
        } = self.transition_pair(profile, state, after)?;
        let mut expected = self.clone();
        if let Some((skill, gain)) = weapon {
            if profile
                .player_melee
                .as_ref()
                .map(|policy| policy.weapon_skill_gain())
                != Some(gain)
            {
                return Err(NativeError::InvalidCheckpoint);
            }
            let inputs = g.inputs_for_recipe(self.recipe());
            let mut view = prior.snapshot();
            view.skills = derived::weapon_skills(g, &view, &inputs, skill, gain)?;
            expected.progression.derived.apply_hook(
                pg::StatHook::AllCritPercentages,
                &view,
                &inputs,
            )?;
            expected.progression.player = pg::State::restore(&g.levels, view.clone())?.snapshot();
            expected.append_exact_callbacks(
                current,
                &[DerivedCall::Weapon {
                    view: Box::new(view),
                    inputs,
                    skill,
                    gain,
                }],
            )?;
        }
        if expected != *after {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok(())
    }
}

/// Every accepted integer is exactly representable as binary32. Construct its
/// bits without host floating point, including negative hit-chance modifiers.
fn integer_float(value: i32) -> Result<spell::SpellFloat, NativeError> {
    let magnitude = value.unsigned_abs();
    if magnitude > 1_000_000 {
        return Err(NativeError::InvalidState);
    }
    if magnitude == 0 {
        return Ok(spell::SpellFloat::ZERO);
    }
    let exponent = 31 - magnitude.leading_zeros();
    let fraction = (magnitude << (23 - exponent)) & 0x007f_ffff;
    let sign = if value < 0 { 0x8000_0000 } else { 0 };
    Ok(spell::SpellFloat::from_bits(
        sign | ((exponent + 127) << 23) | fraction,
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::synthetic_quest_fixture as fixture;

    fn mainhand_world() -> NativeWorld {
        let (mut profile, mut input, mut rows) = crate::native::synthetic_quest_fixture::setup();
        let init::Rows::Complete(skills) = &mut input.default_skills else {
            unreachable!()
        };
        for id in [136, 162] {
            skills.push(init::DefaultSkill { id, step: 0 });
            input
                .skill_lines
                .push(init::SkillLineRow { id, category: 6 });
        }
        let mut weapon = input.items[0].clone();
        weapon.id = 900002;
        weapon.class = 2;
        weapon.subclass = 10;
        weapon.inventory_type = 17;
        weapon.armor = 0;
        weapon.stats = [init::ItemStat { kind: 0, value: 0 }; 10];
        weapon.stats[0] = init::ItemStat { kind: 3, value: 3 };
        weapon.damage[0] = init::ItemDamage {
            min_bits: 2.0f32.to_bits(),
            max_bits: 4.0f32.to_bits(),
            school: 0,
        };
        weapon.delay_ms = 2500;
        input.items.push(weapon);
        let init::Rows::Complete(grants) = &mut input.item_grants else {
            unreachable!()
        };
        grants.push(init::ItemGrant {
            source: init::GrantSource::Custom,
            source_order: 1,
            item: 900002,
            count: 1,
        });
        profile
            .initialize_player(&input, crate::native::synthetic_fixture::float(100.0), 1)
            .unwrap();
        for (position, id) in [(2, 136), (3, 162)] {
            let skill = &profile.combat.as_ref().unwrap().skills[&id];
            rows.skills.push(pg::SkillState {
                id,
                position,
                value: skill.value,
                maximum: skill.maximum,
                step: skill.step,
                permanent_bonus: skill.permanent_bonus,
                temporary_bonus: skill.temporary_bonus,
                status: pg::SkillStatus::Unchanged,
            });
        }
        // The existing fixture's authored level rows, extended with an actual
        // staff SkillLine lookup. These are test rows, never native admission.
        rows.levels = pg::Catalog::new(
            1,
            8,
            rows.levels.rules(),
            (1..=3)
                .map(|level| pg::LevelStatsRow {
                    race: 1,
                    class: 8,
                    level,
                    attributes: init::Attributes(
                        [17, 23, 29, 31, 37].map(|v| v + level as i32 - 1),
                    ),
                })
                .collect(),
            (1..=3)
                .map(|level| pg::ClassLevelStatsRow {
                    class: 8,
                    level,
                    base_health: 5000 + (level - 1) * 10,
                    base_mana: 59 + (level - 1) * 6,
                })
                .collect(),
            (1..=3)
                .map(|level| pg::XpRow {
                    level,
                    xp_for_next_level: level * 40,
                })
                .collect(),
            input
                .skill_lines
                .iter()
                .map(|line| pg::SkillDependency {
                    skill: line.id,
                    line: Some(*line),
                    race_class: init::Rows::Complete(vec![]),
                })
                .collect(),
        )
        .unwrap();
        profile.initialize_quest_progression(&input, rows).unwrap();
        NativeWorld::synthetic_component_test(
            profile,
            [1; 16],
            crate::native::synthetic_fixture::player(),
            42,
        )
        .unwrap()
    }

    fn check(
        binding: &MeleeExecutionBinding,
        world: &NativeWorld,
    ) -> Result<ValidatedMeleeOperands, NativeError> {
        binding.validate(
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
    }

    #[test]
    fn retained_mainhand_operands_and_historical_prefix_are_exact() {
        let world = mainhand_world();
        let binding = MeleeExecutionBinding::capture(&world.profile, &world.state).unwrap();
        let operands = check(&binding, &world).unwrap();
        let current = world.state.quest_progression.as_ref().unwrap();
        assert_eq!(operands, binding.current_operands(&world.profile).unwrap());
        assert_eq!(operands.weapon_item, Some(900002));
        assert_eq!(operands.weapon_subclass, Some(10));
        assert_eq!(operands.skill_id, 136);
        assert_eq!(operands.attack_times_ms, [2500, 2000]);
        assert_eq!(operands.minimum_damage, current.derived.damage[0].minimum);
        assert_eq!(operands.maximum_damage, current.derived.damage[0].maximum);
        assert_eq!(operands.critical_chance, current.derived.crit_percent[0]);
        assert_eq!(
            operands.intellect,
            current.player.view().numeric.attributes.0[3] as u32
        );
        assert_eq!(
            binding.recipe(),
            &world.state.combat.as_ref().unwrap().recipe.snapshot()
        );
        assert_eq!(binding.captured_ms(), world.state.now_ms);
        assert!(binding.replay_work() >= 64);
        let checkpoint = world.checkpoint().unwrap();
        let mut reopened = NativeWorld::restore_synthetic_component_test(
            world.profile.clone(),
            checkpoint.clone(),
        )
        .unwrap();
        reopened.checkpoint_committed(&checkpoint).unwrap();
        assert!(reopened.records().is_empty());
        assert_eq!(check(&binding, &reopened).unwrap(), operands);
        for mutation in 0..7 {
            let mut changed = binding.clone();
            match mutation {
                0 => changed.progression.owner.world[0] ^= 1,
                1 => changed.progression.owner.player.character[0] ^= 1,
                2 => changed.progression.derived.damage[0].minimum = spell::SpellFloat::ZERO,
                3 => changed.progression.derived_prefix.digest[0] ^= 1,
                4 => changed.captured_ms += 1,
                5 => changed.level += 1,
                _ => changed.progression.player.numeric.attributes.0[3] += 1,
            }
            assert!(check(&changed, &world).is_err(), "mutation {mutation}");
        }
    }

    #[test]
    fn actual_broken_mainhand_selects_retained_unarmed_skill() {
        let mut world = mainhand_world();
        let before = MeleeExecutionBinding::capture(&world.profile, &world.state).unwrap();
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
        let recompute = after_combat.apply_item_break(15, live).unwrap();
        current
            .item_break(
                g,
                &before_combat,
                &after_combat,
                15,
                &recompute,
                live.health,
                live.mana,
            )
            .unwrap();
        world.state.combat = Some(after_combat);
        let after = MeleeExecutionBinding::capture(&world.profile, &world.state).unwrap();
        before
            .validate_damage_callbacks(&world.profile, &world.state, &after, None, Some(15))
            .unwrap();
        assert!(before
            .validate_damage_callbacks(&world.profile, &world.state, &after, None, None)
            .is_err());
        assert!(before
            .validate_damage_callbacks(&world.profile, &world.state, &after, None, Some(14))
            .is_err());
        assert!(before
            .validate_weapon_callback(&world.profile, &world.state, &after, None)
            .is_err());
        let operands = check(&after, &world).unwrap();
        assert_eq!(operands.weapon_item, None);
        assert_eq!(operands.weapon_subclass, None);
        assert_eq!(operands.skill_id, 162);
        assert_eq!(operands.attack_times_ms, [2000, 2000]);
        assert_eq!(check(&before, &world).unwrap().skill_id, 136);
        let mut missing = after;
        missing
            .progression
            .player
            .skills
            .retain(|skill| skill.id != 162);
        assert!(missing.current_operands(&world.profile).is_err());
        assert!(check(&missing, &world).is_err());
    }

    // Test-only projection of a genuine source progression transaction, matching
    // Frame::commit_quest_progression's recipe/skill/resource writes. This does
    // not fabricate damage, a native target, or a gameplay admission receipt.
    fn commit_progression(world: &mut NativeWorld, next: QuestProgressionState) {
        let view = next.player.view();
        let combat = world.state.combat.as_mut().unwrap();
        let recipe = combat
            .recipe
            .rebase(
                view.level,
                view.modifiers.create,
                view.modifiers.create_health,
                view.modifiers.create_mana,
            )
            .unwrap();
        let result = recipe
            .current(init::Resources {
                health: view.numeric.health,
                max_health: view.numeric.max_health,
                mana: view.numeric.powers[0],
                max_mana: view.numeric.max_powers[0],
            })
            .unwrap();
        combat.recipe = recipe;
        combat.stats = crate::native::combat::CharacterStats::from_result(&result);
        combat.skills = view
            .skills
            .iter()
            .filter(|skill| skill.status != pg::SkillStatus::Deleted)
            .map(|skill| {
                (
                    skill.id,
                    SkillState {
                        id: skill.id,
                        value: skill.value,
                        maximum: skill.maximum,
                        step: skill.step,
                        permanent_bonus: skill.permanent_bonus,
                        temporary_bonus: skill.temporary_bonus,
                    },
                )
            })
            .collect();
        world.state.health = view.numeric.health;
        world.state.resources.caster.mana = view.numeric.powers[0];
        world.state.quest_progression = Some(next);
    }

    #[test]
    fn actual_kill_award_level_hooks_are_exact_not_arbitrary_valid_level_history() {
        let mut world = mainhand_world();
        let before = MeleeExecutionBinding::capture(&world.profile, &world.state).unwrap();
        let npc = world.state.npcs.values().next().unwrap();
        let target = npc.id;
        let target_profile = npc.effective_profile(&world.profile).unwrap();
        let (awarded, _) = world
            .state
            .quest_progression
            .as_ref()
            .unwrap()
            .killed(
                world.profile.quest_progression.as_ref().unwrap(),
                world.state.combat.as_ref().unwrap(),
                world.state.world,
                &target_profile,
                target,
            )
            .unwrap();
        assert_eq!(awarded.player.view().level, 2);
        assert!(awarded
            .derived_calls
            .iter()
            .any(|call| matches!(call, DerivedCall::Level { .. })));
        assert!(awarded
            .derived_calls
            .iter()
            .any(|call| matches!(call, DerivedCall::Hook { .. })));
        commit_progression(&mut world, awarded);
        let after_kill = MeleeExecutionBinding::capture(&world.profile, &world.state).unwrap();
        before
            .validate_damage_callbacks(
                &world.profile,
                &world.state,
                &after_kill,
                Some(target),
                None,
            )
            .unwrap();
        assert!(before
            .validate_damage_callbacks(&world.profile, &world.state, &after_kill, None, None)
            .is_err());
        let (unrelated, _) = world
            .state
            .quest_progression
            .as_ref()
            .unwrap()
            .admin_give_level(
                world.profile.quest_progression.as_ref().unwrap(),
                world.state.combat.as_ref().unwrap(),
                3,
            )
            .unwrap();
        commit_progression(&mut world, unrelated);
        let after_unrelated = MeleeExecutionBinding::capture(&world.profile, &world.state).unwrap();
        check(&after_unrelated, &world).unwrap(); // independently source-valid, but not this kill
        assert!(before
            .validate_damage_callbacks(
                &world.profile,
                &world.state,
                &after_unrelated,
                Some(target),
                None
            )
            .is_err());
        before
            .validate_damage_callbacks(
                &world.profile,
                &world.state,
                &after_kill,
                Some(target),
                None,
            )
            .unwrap();
    }

    #[test]
    fn unchanged_callback_boundary_is_exact_not_just_valid_ancestry() {
        let world = mainhand_world();
        let before = MeleeExecutionBinding::capture(&world.profile, &world.state).unwrap();
        before
            .validate_damage_callbacks(&world.profile, &world.state, &before, None, None)
            .unwrap();
        before
            .validate_weapon_callback(&world.profile, &world.state, &before, None)
            .unwrap();
        for mutation in 0..4 {
            let mut after = before.clone();
            match mutation {
                0 => after.captured_ms += 1,
                1 => after.progression.player.xp += 1,
                2 => after.progression.training_prefix.digest[0] ^= 1,
                _ => after.progression.derived_prefix.count += 1,
            }
            assert!(before
                .validate_damage_callbacks(&world.profile, &world.state, &after, None, None)
                .is_err());
            assert!(before
                .validate_weapon_callback(&world.profile, &world.state, &after, None)
                .is_err());
        }
    }

    #[test]
    fn absent_sources_and_integer_float_bounds_refuse_without_defaults() {
        let mut world = fixture::world();
        world.state.quest_progression = None;
        assert!(MeleeExecutionBinding::capture(&world.profile, &world.state).is_err());
        for value in [-1_000_000, -100, -1, 0, 1, 100, 1_000_000] {
            assert_eq!(
                integer_float(value).unwrap().bits(),
                (value as f32).to_bits()
            );
        }
        assert!(integer_float(1_000_001).is_err());
        assert!(integer_float(i32::MIN).is_err());
    }
}
