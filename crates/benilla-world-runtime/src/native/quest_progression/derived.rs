// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
//! Source callback caches, authenticated by replaying real progression and item recipes.
use super::*;
use pg::derived::DerivedWeapon;

const MAX_DERIVED_CALLS: usize = 4096;

/// Callback-time snapshots deliberately include transient states: GiveLevel runs
/// defense hooks with the new level and old attributes before UpdateAllStats.
/// Level witnesses authorize those snapshots only through source reducer replay.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum DerivedCall {
    Level {
        before: Box<pg::Snapshot>,
        level: u32,
        inputs: DerivedInputs,
    },
    Hook {
        event: pg::Event,
        view: Box<pg::Snapshot>,
        inputs: DerivedInputs,
    },
    Defense {
        view: Box<pg::Snapshot>,
        inputs: DerivedInputs,
    },
    ItemBreak {
        before: Box<pg::Snapshot>,
        before_recipe: Box<init::StatRecipeSnapshot>,
        after_recipe: Box<init::StatRecipeSnapshot>,
        before_inputs: DerivedInputs,
        after_inputs: DerivedInputs,
        slot: u8,
        recompute: Box<init::EquipmentRecompute>,
    },
}

pub(super) fn deserialize_calls<'de, D>(deserializer: D) -> Result<Vec<DerivedCall>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Calls;
    impl<'de> serde::de::Visitor<'de> for Calls {
        type Value = Vec<DerivedCall>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "at most {MAX_DERIVED_CALLS} derived callbacks")
        }
        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            if seq.size_hint().is_some_and(|n| n > MAX_DERIVED_CALLS) {
                return Err(serde::de::Error::custom("derived callback limit"));
            }
            let mut calls = Vec::new();
            while let Some(call) = seq.next_element()? {
                if calls.len() == MAX_DERIVED_CALLS {
                    return Err(serde::de::Error::custom("derived callback limit"));
                }
                calls.push(call);
            }
            Ok(calls)
        }
    }
    deserializer.deserialize_seq(Calls)
}

fn reserve_call(calls: &[DerivedCall]) -> pg::Result<()> {
    if calls.len() >= MAX_DERIVED_CALLS {
        return Err(pg::Error::Host("derived callback limit"));
    }
    Ok(())
}

fn recipe_inputs(
    combat: &CombatGenesis,
    audit: DerivedAudit,
    recipe: &init::StatRecipeSnapshot,
) -> DerivedInputs {
    // The initializer's equipped metadata also contains armor. IsWeapon from
    // the actual item recipe is authoritative; slot or nonzero delay is not.
    let initial = combat.recipe.snapshot();
    DerivedInputs {
        weapons: combat
            .equipment_weapons
            .iter()
            .filter(|w| initial.items.iter().any(|i| i.slot == w.slot && i.weapon))
            .map(|w| DerivedWeapon {
                slot: w.slot,
                skill: w.skill,
                delay_ms: w.delay_ms,
                damage: w.damage.clone(),
                // DurabilityPointsLoss calls _ApplyItemBonuses(false) BEFORE
                // writing zero durability. The recipe tracks that transition.
                broken: recipe.broken_slots.contains(&w.slot),
            })
            .collect(),
        audit,
    }
}

pub(super) fn initial_inputs(combat: &CombatGenesis, audit: DerivedAudit) -> DerivedInputs {
    recipe_inputs(combat, audit, &combat.recipe.snapshot())
}

impl QuestProgressionGenesis {
    pub(super) fn derived_inputs(&self, combat: &PlayerCombatState) -> DerivedInputs {
        self.inputs_for_recipe(&combat.recipe.snapshot())
    }

    fn inputs_for_recipe(&self, recipe: &init::StatRecipeSnapshot) -> DerivedInputs {
        recipe_inputs(
            &self.initial_combat,
            self.initial_derived_inputs.audit.clone(),
            recipe,
        )
    }
}

impl ProgressionHost {
    pub(super) fn apply_derived(
        &mut self,
        event: &pg::Event,
        view: &pg::Snapshot,
    ) -> pg::Result<()> {
        match event {
            pg::Event::LevelUpPacket { words } => {
                reserve_call(&self.derived_calls)?;
                if view.level.checked_add(1) != Some(words[0]) {
                    return Err(pg::Error::Host("nonconsecutive derived level"));
                }
                self.derived_calls.push(DerivedCall::Level {
                    before: Box::new(view.clone()),
                    level: words[0],
                    inputs: self.inputs.clone(),
                });
            }
            pg::Event::Hook(hook) => self.derived_hook(event, *hook, view)?,
            pg::Event::SkillDefenseBonuses => {
                self.derived_hook(event, pg::StatHook::DefenseBonuses, view)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn derived_hook(
        &mut self,
        event: &pg::Event,
        hook: pg::StatHook,
        view: &pg::Snapshot,
    ) -> pg::Result<()> {
        reserve_call(&self.derived_calls)?;
        // apply_hook deep-clones its fields. Never replace a partial source
        // callback by from_snapshot, which would overwrite unrelated caches.
        self.derived.apply_hook(hook, view, &self.inputs)?;
        self.derived_calls.push(DerivedCall::Hook {
            event: event.clone(),
            view: Box::new(view.clone()),
            inputs: self.inputs.clone(),
        });
        Ok(())
    }
}

fn resources(view: &pg::Snapshot) -> init::Resources {
    init::Resources {
        health: view.numeric.health,
        max_health: view.numeric.max_health,
        mana: view.numeric.powers[0],
        max_mana: view.numeric.max_powers[0],
    }
}

/// Full validation is appropriate only at a completed-operation boundary.
#[cfg(any(test, feature = "synthetic-native-tests"))]
fn boundary(
    g: &QuestProgressionGenesis,
    view: &pg::Snapshot,
    previous: &pg::Snapshot,
    recipe: &init::StatRecipe,
) -> Result<pg::State, NativeError> {
    let state = pg::State::restore(&g.levels, view.clone())?;
    g.validate_live_skills(view)?;
    if view.level != previous.level
        || view.audit != g.initial.view().audit
        || view.skills != previous.skills
        || view.numeric.powers[1..] != previous.numeric.powers[1..]
        || view.modifiers != recipe.snapshot().modifiers
    {
        return Err(NativeError::InvalidCheckpoint);
    }
    Ok(state)
}

fn authoritative_recipe(
    g: &QuestProgressionGenesis,
    view: &pg::Snapshot,
    recipe: &init::StatRecipe,
) -> Result<(), NativeError> {
    pg::State::restore(&g.levels, view.clone())?;
    let fresh = g.initial_combat.recipe.rebase(
        view.level,
        view.modifiers.create,
        view.modifiers.create_health,
        view.modifiers.create_mana,
    )?;
    init::StatRecipe::restore(&fresh, &recipe.snapshot())?;
    if recipe.snapshot().modifiers != view.modifiers {
        return Err(NativeError::InvalidState);
    }
    Ok(())
}

fn defense_skills(
    g: &QuestProgressionGenesis,
    level: u32,
    before: &[pg::SkillState],
) -> Result<Vec<pg::SkillState>, NativeError> {
    let mut expected = before.to_vec();
    let skill = expected
        .iter_mut()
        .find(|s| s.id == combat::DEFENSE_SKILL)
        .ok_or(NativeError::InvalidState)?;
    let chance = benilla_sim::npc_melee::defense_gain_chance(
        u8::try_from(level).map_err(|_| NativeError::InvalidState)?,
        benilla_sim::npc_melee::DefenseSkill {
            value: skill.value,
            maximum: skill.maximum,
            gain: g.initial_combat.defense_gain,
        },
    )?;
    if chance.is_none() {
        return Err(NativeError::InvalidState);
    }
    let value = (u32::from(skill.value) + u32::from(g.initial_combat.defense_gain))
        .min(u32::from(skill.maximum)) as u16;
    if skill.value == 0 || value <= skill.value {
        return Err(NativeError::InvalidState);
    }
    skill.value = value;
    skill.status = pg::SkillStatus::Changed;
    Ok(expected)
}

impl QuestProgressionState {
    pub(in crate::native) fn item_break(
        &mut self,
        g: &QuestProgressionGenesis,
        before_combat: &PlayerCombatState,
        after_combat: &PlayerCombatState,
        slot: u8,
        recompute: &init::EquipmentRecompute,
        health: u32,
        mana: u32,
    ) -> Result<(), NativeError> {
        reserve_call(&self.derived_calls)?;
        let mut next = self.clone();
        next.synchronize(g, before_combat, health, mana)?;
        let before = next.player.snapshot();
        authoritative_recipe(g, &before, &before_combat.recipe)?;
        let before_recipe = before_combat.recipe.snapshot();
        if before_recipe.broken_slots.contains(&slot) {
            return Err(NativeError::InvalidState);
        }
        let mut actual_recipe = before_combat.recipe.clone();
        let actual = actual_recipe.set_item_broken(slot, true, resources(&before))?;
        let mut expected_combat = before_combat.clone();
        expected_combat.recipe = actual_recipe;
        expected_combat.stats = combat::CharacterStats::from_result(&actual);
        if actual != *recompute || expected_combat != *after_combat {
            return Err(NativeError::InvalidState);
        }
        let before_inputs = g.derived_inputs(before_combat);
        let after_inputs = g.derived_inputs(after_combat);
        next.derived
            .apply_item_break(&before, &before_inputs, &after_inputs, slot, &actual)?;
        next.synchronize(
            g,
            after_combat,
            actual.resources.health,
            actual.resources.mana,
        )?;
        next.derived_calls.push(DerivedCall::ItemBreak {
            before: Box::new(before),
            before_recipe: Box::new(before_recipe),
            after_recipe: Box::new(after_combat.recipe.snapshot()),
            before_inputs,
            after_inputs,
            slot,
            recompute: Box::new(actual),
        });
        *self = next;
        Ok(())
    }

    pub(in crate::native) fn defense_change(
        &mut self,
        g: &QuestProgressionGenesis,
        combat: &PlayerCombatState,
        health: u32,
        mana: u32,
    ) -> Result<(), NativeError> {
        reserve_call(&self.derived_calls)?;
        let expected_skills =
            defense_skills(g, self.player.view().level, &self.player.view().skills)?;
        let mut next = self.clone();
        next.synchronize(g, combat, health, mana)?;
        let view = next.player.view();
        if view.skills != expected_skills {
            return Err(NativeError::InvalidState);
        }
        authoritative_recipe(g, view, &combat.recipe)?;
        let inputs = g.derived_inputs(combat);
        next.derived
            .apply_hook(pg::StatHook::DefenseBonuses, view, &inputs)?;
        next.derived_calls.push(DerivedCall::Defense {
            view: Box::new(view.clone()),
            inputs,
        });
        *self = next;
        Ok(())
    }

    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn validate_derived(
        &self,
        g: &QuestProgressionGenesis,
        combat: &PlayerCombatState,
    ) -> Result<BTreeMap<usize, u32>, NativeError> {
        if self.genesis != g.identity {
            return Err(NativeError::InvalidCheckpoint);
        }
        let boundaries = validate_history(
            g,
            self.player.view(),
            &self.training,
            &self.derived,
            &self.derived_calls,
            &combat.recipe,
        )?;
        let expected_broken: Vec<_> = combat
            .equipment
            .items()
            .filter(|i| i.is_broken())
            .map(|i| i.slot)
            .collect();
        if expected_broken != combat.recipe.snapshot().broken_slots {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok(boundaries)
    }
}

/// Validate a completed current or historical snapshot against its source-call
/// prefix. Historical bindings provide their actual captured recipe, never a
/// fabricated equipment state. The owner separately binds this prefix to its
/// current journal and validates current physical equipment durability.
#[cfg(any(test, feature = "synthetic-native-tests"))]
pub(super) fn validate_history(
    g: &QuestProgressionGenesis,
    player: &pg::Snapshot,
    expected_training: &init::TrainingSnapshot,
    expected_derived: &DerivedStats,
    calls: &[DerivedCall],
    expected_recipe: &init::StatRecipe,
) -> Result<BTreeMap<usize, u32>, NativeError> {
    if calls.len() > MAX_DERIVED_CALLS {
        return Err(NativeError::InvalidCheckpoint);
    }
    let mut source = g.initial.clone();
    let mut recipe = g.initial_combat.recipe.clone();
    let mut training = g.training.clone();
    let mut derived = DerivedStats::from_snapshot(source.view(), &g.initial_derived_inputs)?;
    let mut index = 0;
    let mut boundaries = BTreeMap::from([(0, source.view().level)]);
    while let Some(call) = calls.get(index) {
        match call {
            DerivedCall::Level {
                before,
                level,
                inputs,
            } => {
                source = boundary(g, before, source.view(), &recipe)?;
                if source.view().level.checked_add(1) != Some(*level)
                    || *level > player.level
                    || *inputs != g.inputs_for_recipe(&recipe.snapshot())
                {
                    return Err(NativeError::InvalidCheckpoint);
                }
                let host = ProgressionHost {
                    training,
                    derived,
                    derived_calls: vec![],
                    inputs: inputs.clone(),
                    audit: source.view().audit,
                };
                // This real host executes training as well as every mandatory
                // derived hook. Compare all intermediate source snapshots;
                // State::restore would incorrectly reject many of them.
                let tx = pg::give_level(&g.levels, &source, &host, *level)?;
                let end = index
                    .checked_add(tx.host.derived_calls.len())
                    .ok_or(NativeError::InvalidCheckpoint)?;
                if end == index || calls.get(index..end) != Some(tx.host.derived_calls.as_slice()) {
                    return Err(NativeError::InvalidCheckpoint);
                }
                recipe = recipe.rebase(
                    *level,
                    tx.state.view().modifiers.create,
                    tx.state.view().modifiers.create_health,
                    tx.state.view().modifiers.create_mana,
                )?;
                source = tx.state;
                training = tx.host.training;
                derived = tx.host.derived;
                index = end;
            }
            DerivedCall::Hook { .. } => return Err(NativeError::InvalidCheckpoint),
            DerivedCall::Defense { view, inputs } => {
                let mut expected = source.snapshot();
                expected.skills = defense_skills(g, expected.level, &expected.skills)?;
                source = boundary(g, view, &expected, &recipe)?;
                if *inputs != g.inputs_for_recipe(&recipe.snapshot()) {
                    return Err(NativeError::InvalidCheckpoint);
                }
                derived.apply_hook(pg::StatHook::DefenseBonuses, view, inputs)?;
                index += 1;
            }
            DerivedCall::ItemBreak {
                before,
                before_recipe,
                after_recipe,
                before_inputs,
                after_inputs,
                slot,
                recompute,
            } => {
                source = boundary(g, before, source.view(), &recipe)?;
                if **before_recipe != recipe.snapshot()
                    || before_recipe.broken_slots.contains(slot)
                    || *before_inputs != g.inputs_for_recipe(before_recipe)
                {
                    return Err(NativeError::InvalidCheckpoint);
                }
                // Never admit persisted attribute_steps as authority. This
                // operation regenerates their order, resources, and values
                // from immutable actual item rows and current create rows.
                let actual = recipe.set_item_broken(*slot, true, resources(before))?;
                if actual != **recompute
                    || **after_recipe != recipe.snapshot()
                    || *after_inputs != g.inputs_for_recipe(after_recipe)
                {
                    return Err(NativeError::InvalidCheckpoint);
                }
                derived.apply_item_break(before, before_inputs, after_inputs, *slot, &actual)?;
                let mut view = source.snapshot();
                view.modifiers = recipe.snapshot().modifiers;
                let mut powers = view.numeric.powers;
                powers[0] = actual.resources.mana;
                view.numeric = pg::NumericState::from_modifiers(
                    view.level,
                    &view.modifiers,
                    actual.resources.health,
                    powers,
                )?;
                view.alive = actual.resources.health != 0;
                source = pg::State::restore(&g.levels, view)?;
                index += 1;
            }
        }
        // A Level packet alone is not an attained level: all mandatory source
        // hooks must replay before exposing this completed-operation boundary.
        boundaries.insert(index, source.view().level);
    }
    boundary(g, player, source.view(), &recipe)?;
    if *expected_derived != derived
        || *expected_recipe != recipe
        || *expected_training != training.snapshot()
    {
        return Err(NativeError::InvalidCheckpoint);
    }
    Ok(boundaries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (
        QuestProgressionGenesis,
        QuestProgressionState,
        PlayerCombatState,
    ) {
        let profile = crate::native::synthetic_quest_fixture::profile();
        let g = profile.quest_progression.unwrap();
        let state = QuestProgressionState::new(&g, 0).unwrap();
        let combat = PlayerCombatState::new(&g.initial_combat, 0).unwrap();
        (g, state, combat)
    }

    fn mainhand_fixture(
        delay_ms: u32,
    ) -> (
        QuestProgressionGenesis,
        QuestProgressionState,
        PlayerCombatState,
    ) {
        let (mut profile, mut input, mut rows) = crate::native::synthetic_quest_fixture::setup();
        let init::Rows::Complete(skills) = &mut input.default_skills else {
            unreachable!()
        };
        skills.push(init::DefaultSkill { id: 136, step: 0 });
        input.skill_lines.push(init::SkillLineRow {
            id: 136,
            category: 6,
        });
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
        weapon.delay_ms = delay_ms;
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
        let skill = &profile.combat.as_ref().unwrap().skills[&136];
        rows.skills.push(pg::SkillState {
            id: 136,
            position: 2,
            value: skill.value,
            maximum: skill.maximum,
            step: skill.step,
            permanent_bonus: skill.permanent_bonus,
            temporary_bonus: skill.temporary_bonus,
            status: pg::SkillStatus::Unchanged,
        });
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
        let g = profile.quest_progression.unwrap();
        let state = QuestProgressionState::new(&g, 0).unwrap();
        let combat = PlayerCombatState::new(&g.initial_combat, 0).unwrap();
        (g, state, combat)
    }

    fn level_up(
        g: &QuestProgressionGenesis,
        state: &mut QuestProgressionState,
        combat: &mut PlayerCombatState,
    ) {
        let host = ProgressionHost {
            training: init::LiveTraining::restore_authoritative(
                &g.training,
                &state.training,
                state.player.view(),
            )
            .unwrap(),
            derived: state.derived.clone(),
            derived_calls: state.derived_calls.clone(),
            inputs: g.derived_inputs(combat),
            audit: state.player.view().audit,
        };
        let tx = pg::give_level(
            &g.levels,
            &state.player,
            &host,
            state.player.view().level + 1,
        )
        .unwrap();
        let view = tx.state.view();
        combat.recipe = combat
            .recipe
            .rebase(
                view.level,
                view.modifiers.create,
                view.modifiers.create_health,
                view.modifiers.create_mana,
            )
            .unwrap();
        combat.stats =
            combat::CharacterStats::from_result(&combat.recipe.current(resources(view)).unwrap());
        combat.skills = view
            .skills
            .iter()
            .map(|s| {
                (
                    s.id,
                    SkillState {
                        id: s.id,
                        value: s.value,
                        maximum: s.maximum,
                        step: s.step,
                        permanent_bonus: s.permanent_bonus,
                        temporary_bonus: s.temporary_bonus,
                    },
                )
            })
            .collect();
        state.abilities = g.resolve_abilities(&tx.host.training, view).unwrap();
        state.player = tx.state;
        state.training = tx.host.training.snapshot();
        state.derived = tx.host.derived;
        state.derived_calls = tx.host.derived_calls;
    }

    fn break_armor(
        g: &QuestProgressionGenesis,
        state: &mut QuestProgressionState,
        combat: &mut PlayerCombatState,
    ) {
        let before = combat.clone();
        let live = resources(state.player.view());
        let recompute = combat.apply_item_break(4, live).unwrap();
        // The runtime callback precedes equipment durability commitment.
        assert!(!combat.equipment.item(4).unwrap().is_broken());
        state
            .item_break(g, &before, combat, 4, &recompute, live.health, live.mana)
            .unwrap();
        let mut equipment = combat.equipment.snapshot();
        equipment
            .items
            .iter_mut()
            .find(|i| i.slot == 4)
            .unwrap()
            .current_durability = 0;
        combat.equipment = equipment::EquipmentState::from_snapshot(equipment).unwrap();
    }

    #[test]
    fn armor_metadata_is_not_a_derived_weapon() {
        let (g, state, combat) = fixture();
        assert!(!g.initial_combat.equipment_weapons.is_empty());
        assert!(g.initial_derived_inputs.weapons.is_empty());
        assert!(g.derived_inputs(&combat).weapons.is_empty());
        state.validate_derived(&g, &combat).unwrap();
    }

    #[test]
    fn level_history_replays_transient_source_hooks_and_rejects_corruption() {
        let (g, mut state, mut combat) = fixture();
        level_up(&g, &mut state, &mut combat);
        let transient = state
            .derived_calls
            .iter()
            .find_map(|call| match call {
                DerivedCall::Hook {
                    event: pg::Event::SkillDefenseBonuses,
                    view,
                    ..
                } => Some(view),
                _ => None,
            })
            .unwrap();
        assert_eq!(transient.level, 2);
        assert_eq!(
            transient.modifiers.create,
            g.initial.view().modifiers.create
        );
        assert!(pg::State::restore(&g.levels, (**transient).clone()).is_err());
        state.validate_derived(&g, &combat).unwrap();
        let mut forged = state.clone();
        let DerivedCall::Hook { view, .. } = forged
            .derived_calls
            .iter_mut()
            .find(|c| matches!(c, DerivedCall::Hook { .. }))
            .unwrap()
        else {
            unreachable!()
        };
        view.numeric.attributes.0[1] += 1;
        assert!(forged.validate_derived(&g, &combat).is_err());
        let mut missing = state.clone();
        missing.derived_calls.pop();
        assert!(missing.validate_derived(&g, &combat).is_err());
        let mut isolated = state.clone();
        isolated.derived_calls.remove(0);
        assert!(isolated.validate_derived(&g, &combat).is_err());
    }

    #[test]
    fn item_break_replays_actual_witness_and_keeps_rejection_atomic() {
        let (g, mut state, mut combat) = fixture();
        level_up(&g, &mut state, &mut combat);
        let before = combat.clone();
        let live = resources(state.player.view());
        let actual = combat.apply_item_break(4, live).unwrap();
        let mut forged = actual.clone();
        forged.attribute_steps.swap(0, 1);
        let original = state.clone();
        assert!(state
            .item_break(&g, &before, &combat, 4, &forged, live.health, live.mana)
            .is_err());
        assert_eq!(state, original);
        combat = before;
        break_armor(&g, &mut state, &mut combat);
        state.validate_derived(&g, &combat).unwrap();
        let mut forged = state.clone();
        let DerivedCall::ItemBreak { recompute, .. } = forged.derived_calls.last_mut().unwrap()
        else {
            unreachable!()
        };
        recompute.attribute_steps[0].1 .0[3] += 1;
        assert!(forged.validate_derived(&g, &combat).is_err());
        let mut forged = state.clone();
        let DerivedCall::ItemBreak { after_recipe, .. } = forged.derived_calls.last_mut().unwrap()
        else {
            unreachable!()
        };
        after_recipe.broken_slots.clear();
        assert!(forged.validate_derived(&g, &combat).is_err());
        level_up(&g, &mut state, &mut combat);
        state.validate_derived(&g, &combat).unwrap();
    }

    #[test]
    fn defense_updates_only_defense_and_journal_limit_is_atomic() {
        let (g, mut state, mut combat) = fixture();
        let before = state.derived.clone();
        let live = resources(state.player.view());
        combat.skills.get_mut(&combat::DEFENSE_SKILL).unwrap().value += 1;
        state
            .defense_change(&g, &combat, live.health, live.mana)
            .unwrap();
        let mut expected = before;
        expected.defense = state.derived.defense;
        assert_eq!(state.derived, expected);
        state.validate_derived(&g, &combat).unwrap();
        let mut forged = state.clone();
        let DerivedCall::Defense { view, .. } = forged.derived_calls.last_mut().unwrap() else {
            unreachable!()
        };
        view.skills
            .iter_mut()
            .find(|s| s.id == combat::DEFENSE_SKILL)
            .unwrap()
            .value += 1;
        assert!(forged.validate_derived(&g, &combat).is_err());
        state.derived_calls = vec![state.derived_calls[0].clone(); MAX_DERIVED_CALLS];
        let original = state.clone();
        combat.skills.get_mut(&combat::DEFENSE_SKILL).unwrap().value += 1;
        assert!(state
            .defense_change(&g, &combat, live.health, live.mana)
            .is_err());
        assert_eq!(state, original);
    }

    #[test]
    fn mainhand_break_uses_precommit_recipe_and_preserves_partial_caches() {
        for delay in [0, 1700] {
            let (g, mut state, mut combat) = mainhand_fixture(delay);
            assert_eq!(g.derived_inputs(&combat).weapons.len(), 1);
            let before = combat.clone();
            let before_damage = state.derived.damage[0];
            let live = resources(state.player.view());
            let actual = combat.apply_item_break(15, live).unwrap();
            assert!(!combat.equipment.item(15).unwrap().is_broken());
            assert!(!g.derived_inputs(&before).weapons[0].broken);
            assert!(g.derived_inputs(&combat).weapons[0].broken);
            state
                .item_break(&g, &before, &combat, 15, &actual, live.health, live.mana)
                .unwrap();
            let full = DerivedStats::from_snapshot(state.player.view(), &g.derived_inputs(&combat))
                .unwrap();
            // The source agility callback sees the staff skill before damage
            // clearing; a full recomputation would use missing unarmed skill.
            assert_ne!(
                state.derived.crit_display_percent[0],
                full.crit_display_percent[0]
            );
            if delay == 0 {
                assert_eq!(state.derived.damage[0], before_damage);
                assert_ne!(state.derived.damage[0], full.damage[0]);
            } else {
                assert_eq!(state.derived.damage[0], full.damage[0]);
            }
            let mut equipment = combat.equipment.snapshot();
            equipment
                .items
                .iter_mut()
                .find(|i| i.slot == 15)
                .unwrap()
                .current_durability = 0;
            combat.equipment = equipment::EquipmentState::from_snapshot(equipment).unwrap();
            state.validate_derived(&g, &combat).unwrap();
            let mut forged = state.clone();
            let DerivedCall::ItemBreak { after_inputs, .. } =
                forged.derived_calls.last_mut().unwrap()
            else {
                unreachable!()
            };
            after_inputs.weapons[0].broken = false;
            assert!(forged.validate_derived(&g, &combat).is_err());
        }
    }
}
