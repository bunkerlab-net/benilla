// SPDX-License-Identifier: GPL-3.0-or-later
//! Reconstructed owner-side integration of pinned ordinary quest and mage progression reducers.
//! Genesis requires actual rows and complete absence proofs; it grants no native capability.
use super::*;
use benilla_sim::{
    inventory as inv, player_progression as pg, quest as arithmetic, quest_lifecycle as q,
    starter_init as init,
};
use pg::{
    derived::{DerivedAudit, DerivedInputs, DerivedStats},
    kill_xp as kill,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
mod bindings;
mod capacity;
mod derived;
pub(in crate::native) mod equipment_movement;
pub use equipment_movement::{
    EquipmentAutoStoreReceipt, EquipmentDestroyReceipt, EquipmentMoveReceipt, EquipmentMoveRequest,
    EquipmentRelocationReceipt,
};
mod kills;
mod reputation;
pub use bindings::ExecutionBinding;
pub(in crate::native) use bindings::MeleeExecutionBinding;
pub use derived::DerivedCall;
pub use kills::{creature_guid, KillCreatureRow, KillDependencyAudit, NativeKillPolicy};
pub use reputation::NativeReputationState;

pub const QUEST_PROGRESSION_POLICY: &str =
    "benilla-native-ordinary-quest-reputation-reconstruction-v14";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestNpcRow {
    pub entry: u32,
    pub npc_flags: u32,
    pub unit_flags: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastTimeRow {
    pub id: u32,
    pub base_ms: i32,
    pub per_level_ms: i32,
    pub minimum_ms: i32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityRecipe {
    pub ability: Ability,
    pub cast_time: Option<CastTimeRow>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestDialogRules {
    pub always_show_quest_greeting: bool,
    pub low_level_hide_diff: i32,
    pub high_level_hide_diff: i32,
}
#[derive(Clone, Debug)]
pub struct QuestProgressionInput {
    /// Explicit source-resolved closure. None leaves reputation unavailable.
    pub reputation: Option<benilla_sim::reputation::Catalog>,
    pub loot: Option<NativeLootInput>,
    pub quests: Vec<q::QuestRow>,
    pub relations: q::QuestRelations,
    pub quest_audit: q::DependencyAudit,
    pub levels: pg::Catalog,
    pub progression_audit: pg::Audit,
    pub derived_audit: DerivedAudit,
    pub skills: Vec<pg::SkillState>,
    pub initial_next_level_xp: u32,
    pub start_player_money: u32,
    pub quest_xp_rate: arithmetic::Binary32Rate,
    pub money_rate: arithmetic::Binary32Rate,
    pub npc_rows: Vec<QuestNpcRow>,
    pub abilities: Vec<AbilityRecipe>,
    pub kill: NativeKillPolicy,
    pub dialog_rules: QuestDialogRules,
}
#[derive(Clone, Debug)]
pub struct QuestProgressionGenesis {
    reputation: Option<benilla_sim::reputation::Catalog>,
    pub loot: Option<NativeLootGenesis>,
    loot_identity: Option<crate::Digest>,
    identity: crate::Digest,
    tick_ms: u32,
    quests: q::Catalog,
    presentation_relations: q::QuestRelations,
    levels: pg::Catalog,
    initial: pg::State,
    training: init::LiveTraining,
    initial_combat: CombatGenesis,
    initial_derived_inputs: DerivedInputs,
    abilities: BTreeMap<u32, AbilityRecipe>,
    pub(in crate::native) npc_rows: BTreeMap<u32, QuestNpcRow>,
    skill_bounds: BTreeMap<u32, Vec<pg::SkillState>>,
    start_money: u32,
    quest_xp_rate: arithmetic::Binary32Rate,
    money_rate: arithmetic::Binary32Rate,
    kill: NativeKillPolicy,
    dialog_rules: QuestDialogRules,
}
impl PartialEq for QuestProgressionGenesis {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity
    }
}
impl Eq for QuestProgressionGenesis {}
impl Serialize for QuestProgressionGenesis {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.identity.serialize(serializer)
    }
}
impl Profile {
    pub fn initialize_quest_progression(
        &mut self,
        input: &init::StarterInput,
        rows: QuestProgressionInput,
    ) -> Result<(), NativeError> {
        let mut next = self.clone();
        next.quest_progression = None;
        let genesis = QuestProgressionGenesis::build(&next, input, rows)?;
        next.quest_progression = Some(genesis);
        next.validate()?;
        *self = next;
        Ok(())
    }
}
impl QuestProgressionGenesis {
    fn build(
        profile: &Profile,
        input: &init::StarterInput,
        rows: QuestProgressionInput,
    ) -> Result<Self, NativeError> {
        let initial_combat = profile
            .combat
            .as_ref()
            .ok_or(NativeError::UnsupportedMechanic(
                "quest progression requires source initializer",
            ))?;
        if input.level != 1
            || rows.levels.race() != input.race
            || rows.levels.class() != input.class
            || rows.start_player_money > q::MAX_MONEY
            || rows.quests.len() > 1024
            || rows.relations.starters.len() > 8192
            || rows.relations.finishers.len() > 8192
            || rows.npc_rows.len() > 4096
            || rows.abilities.len() > 64
            || !(-1..=100).contains(&rows.dialog_rules.low_level_hide_diff)
            || !(-1..=100).contains(&rows.dialog_rules.high_level_hide_diff)
        {
            return Err(NativeError::UnsupportedMechanic(
                "ordinary bounded quest progression input",
            ));
        }
        rows.derived_audit.validate()?;
        rows.kill.validate(profile, rows.loot.is_some())?;
        let mage = initial_combat.initialized_projection(input)?;
        reputation::validate_input(
            rows.reputation.as_ref(),
            &rows.quests,
            input,
            &mage,
            initial_combat,
        )?;
        let loot = rows
            .loot
            .map(|r| NativeLootGenesis::build(profile, input, &mage, r))
            .transpose()?;
        if loot.is_none() && !mage.inventory.is_empty() {
            return Err(NativeError::UnsupportedMechanic(
                "starter carried items require inventory host",
            ));
        }
        for row in &rows.quests {
            for item in row
                .required_item
                .into_iter()
                .chain(row.excluded.reward_choice_item)
                .chain(row.excluded.reward_item)
                .filter(|i| *i != 0)
            {
                let l = loot.as_ref().ok_or(NativeError::UnsupportedMechanic(
                    "quest items require inventory host",
                ))?;
                l.inventory.item(item)?;
            }
            // Search visibility and admin creation do not widen ordinary reward readiness.
            for item in row
                .excluded
                .reward_choice_item
                .into_iter()
                .chain(row.excluded.reward_item)
                .filter(|i| *i != 0)
            {
                loot.as_ref()
                    .ok_or(NativeError::UnsupportedMechanic(
                        "quest rewards require inventory host",
                    ))?
                    .inventory
                    .ordinary_loot_ready(item)?;
            }
            if row.required_item.iter().any(|item| {
                loot.as_ref()
                    .is_some_and(|l| l.initial_inventory.equipped.contains_key(item))
            }) {
                return Err(NativeError::UnsupportedMechanic(
                    "quest removes equipped starter item",
                ));
            }
        }
        let training = match &initial_combat.prepared {
            Some(prepared) => init::LiveTraining::from_prepared(input, prepared)?,
            None => init::LiveTraining::from_initializer(input, &mage)?,
        };
        let mut verified = profile.clone();
        verified.quest_progression = None;
        verified.reinitialize_player(input, initial_combat)?;
        let mut initial_combat = initial_combat.clone();
        initial_combat.death = None;
        if verified.combat.as_ref() != Some(&initial_combat) {
            return Err(NativeError::InvalidProfile);
        }
        let expected_skills = &initial_combat.skills;
        let mut seen = BTreeSet::new();
        if rows.skills.len() != expected_skills.len()
            || rows.skills.iter().any(|s| {
                !seen.insert(s.id)
                    || s.status != pg::SkillStatus::Unchanged
                    || expected_skills.get(&s.id)
                        != Some(&SkillState {
                            id: s.id,
                            value: s.value,
                            maximum: s.maximum,
                            step: s.step,
                            permanent_bonus: s.permanent_bonus,
                            temporary_bonus: s.temporary_bonus,
                        })
            })
        {
            return Err(NativeError::InvalidProfile);
        }
        let modifiers = initial_combat.recipe.snapshot().modifiers;
        let numeric = pg::NumericState::from_modifiers(
            1,
            &modifiers,
            initial_combat.initial_health,
            [initial_combat.initial_mana, 0, 0, 100, 0],
        )?;
        let initial = pg::State::restore(
            &rows.levels,
            pg::Snapshot {
                policy: pg::POLICY_ID.into(),
                catalog_identity: rows.levels.identity(),
                race: input.race,
                class: input.class,
                level: 1,
                alive: true,
                xp: 0,
                next_level_xp: rows.initial_next_level_xp,
                played_time_level: 0,
                session_level: 1,
                free_talent_points: 0,
                used_talent_points: 0,
                modifiers,
                numeric,
                skills: rows.skills,
                audit: rows.progression_audit,
            },
        )?;
        training.preflight(initial.view())?;
        // Source multimap preserves the captured query order among equivalent NPC keys.
        let presentation_relations = rows.relations.clone();
        let quests = q::Catalog::new(rows.quests, rows.relations, rows.quest_audit)?;
        let mut npc_rows = BTreeMap::new();
        for row in rows.npc_rows {
            if row.entry == 0
                || row.entry > 0xff_ffff
                || !profile.npcs.iter().any(|n| n.entry == row.entry)
                || npc_rows.insert(row.entry, row).is_some()
            {
                return Err(NativeError::InvalidProfile);
            }
        }
        for r in quests
            .relations()
            .starters
            .iter()
            .chain(&quests.relations().finishers)
        {
            if !npc_rows.contains_key(&r.creature) {
                return Err(NativeError::InvalidProfile);
            }
        }
        let mut abilities = BTreeMap::new();
        for row in rows.abilities {
            if row.ability.id == 0
                || row.ability.id > u16::MAX.into()
                || abilities.insert(row.ability.id, row).is_some()
            {
                return Err(NativeError::InvalidProfile);
            }
        }
        let initial_derived_inputs = derived::initial_inputs(&initial_combat, rows.derived_audit);
        let mut out = Self {
            reputation: rows.reputation,
            loot_identity: loot.as_ref().map(NativeLootGenesis::identity),
            loot,
            identity: [0; 32],
            tick_ms: profile.tick_ms,
            quests,
            presentation_relations,
            levels: rows.levels,
            skill_bounds: BTreeMap::from([(1, initial.view().skills.clone())]),
            initial,
            training,
            initial_combat,
            initial_derived_inputs,
            abilities,
            npc_rows,
            start_money: rows.start_player_money,
            quest_xp_rate: rows.quest_xp_rate,
            money_rate: rows.money_rate,
            kill: rows.kill,
            dialog_rules: rows.dialog_rules,
        };
        if out.resolve_abilities(&out.training, out.initial.view())? != profile.abilities {
            return Err(NativeError::InvalidProfile);
        }
        let mut probe = out.initial.clone();
        let mut host = ProgressionHost {
            training: out.training.clone(),
            derived: DerivedStats::from_snapshot(probe.view(), &out.initial_derived_inputs)?,
            derived_calls: vec![],
            inputs: out.initial_derived_inputs.clone(),
            audit: probe.view().audit,
        };
        let levels: Vec<_> = out.levels.supplied_levels().collect();
        if levels.first() != Some(&1) || levels.windows(2).any(|w| w[1] != w[0] + 1) {
            return Err(NativeError::InvalidProfile);
        }
        for level in levels.into_iter().skip(1) {
            let tx = pg::give_level(&out.levels, &probe, &host, level)?;
            out.resolve_abilities(&tx.host.training, tx.state.view())?;
            probe = tx.state;
            out.skill_bounds.insert(level, probe.view().skills.clone());
            host = tx.host;
        }
        capacity::validate(&out, &host, probe.view())?;
        out.identity = crate::digest(
            b"benilla-native-quest-progression-genesis-v6\0",
            &(
                (
                    QUEST_PROGRESSION_POLICY,
                    q::POLICY_ID,
                    pg::POLICY_ID,
                    pg::derived::POLICY_ID,
                    init::LIVE_TRAINING_POLICY_ID,
                    reputation::POLICY_ID,
                    out.reputation
                        .as_ref()
                        .map(benilla_sim::reputation::Catalog::hash),
                ),
                (
                    out.quests.hash(),
                    (
                        out.presentation_relations
                            .starters
                            .iter()
                            .map(|r| (r.creature, r.quest))
                            .collect::<Vec<_>>(),
                        out.presentation_relations
                            .finishers
                            .iter()
                            .map(|r| (r.creature, r.quest))
                            .collect::<Vec<_>>(),
                    ),
                    out.tick_ms,
                ),
                out.levels.identity(),
                out.initial.view(),
                (out.training.identity(), out.training.preparation_identity()),
                &out.initial_derived_inputs,
                &out.abilities,
                &out.npc_rows,
                out.start_money,
                (out.quest_xp_rate.to_bits(), out.money_rate.to_bits()),
                &out.kill,
                &out.dialog_rules,
                &out.initial_combat,
                &out.loot,
            ),
        );
        Ok(out)
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(in crate::native) fn restore_inputs(
        &self,
        s: &QuestProgressionSnapshot,
        combat: &CombatGenesis,
    ) -> Result<(Vec<Ability>, CombatGenesis, init::StatRecipe), NativeError> {
        if s.policy != QUEST_PROGRESSION_POLICY || s.genesis != self.identity {
            return Err(NativeError::InvalidCheckpoint);
        }
        let p = pg::State::restore(&self.levels, s.player.clone())?;
        self.validate_live_skills(p.view())?;
        let training =
            init::LiveTraining::restore_authoritative(&self.training, &s.training, p.view())?;
        let (_, replayed_training, recipe) = derived::reconstruct_history(
            self,
            p.view(),
            s.training.as_view(),
            &s.derived,
            &s.derived_calls,
        )?;
        if replayed_training.snapshot() != training.snapshot() {
            return Err(NativeError::InvalidCheckpoint);
        }
        let abilities = self.resolve_abilities(&training, p.view())?;
        if abilities != s.abilities {
            return Err(NativeError::InvalidCheckpoint);
        }
        let view = p.view();
        let mut combat = combat.clone();
        combat.recipe = combat.recipe.rebase(
            view.level,
            view.modifiers.create,
            view.modifiers.create_health,
            view.modifiers.create_mana,
        )?;
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
        Ok((abilities, combat, recipe))
    }
    pub(in crate::native) fn retained_starter_items(&self) -> &[init::ItemRow] {
        &self.training.input().items
    }

    pub(in crate::native) fn validate(&self, profile: &Profile) -> Result<(), NativeError> {
        let mut combat = profile.combat.clone().ok_or(NativeError::InvalidProfile)?;
        combat.death = None;
        if profile.tick_ms != self.tick_ms
            || combat != self.initial_combat
            || profile.abilities != self.resolve_abilities(&self.training, self.initial.view())?
        {
            return Err(NativeError::InvalidProfile);
        }
        if self.loot.as_ref().map(NativeLootGenesis::identity) != self.loot_identity {
            return Err(NativeError::InvalidProfile);
        }
        let initialized = combat.initialized_projection(self.training.input())?;
        if let Some(l) = &self.loot {
            l.validate_immutable(self.training.input(), &initialized)?;
        }
        reputation::validate_binding(
            self.reputation.as_ref(),
            self.training.input(),
            &initialized,
            &combat,
        )?;
        if let Some(catalog) = &profile.creature_generations {
            for npc in &profile.npcs {
                let Some(row) = self.npc_rows.get(&npc.entry) else {
                    continue;
                };
                let source = catalog
                    .source(npc.ai.creature_id)
                    .ok_or(NativeError::AuthorityBinding)?;
                let benilla_sim::creature_init::Rows::Complete(spawns) =
                    &source.input.spawn_template
                else {
                    return Err(NativeError::AuthorityBinding);
                };
                let [spawn] = spawns.as_slice() else {
                    return Err(NativeError::AuthorityBinding);
                };
                let unit_flags = if spawn.unit_flags == -1 {
                    source.input.template.unit_flags
                } else {
                    u32::try_from(spawn.unit_flags).map_err(|_| NativeError::AuthorityBinding)?
                };
                let npc_flags = if spawn.npc_flags == -1 {
                    source.input.template.npc_flags
                } else {
                    u32::try_from(spawn.npc_flags).map_err(|_| NativeError::AuthorityBinding)?
                };
                if row.unit_flags != unit_flags || row.npc_flags != npc_flags {
                    return Err(NativeError::AuthorityBinding);
                }
            }
        }
        self.kill.validate(profile, self.loot.is_some())
    }
    pub(in crate::native) fn initial_training_source_identity(&self) -> [u8; 32] {
        self.training.source_identity()
    }
    pub(in crate::native) fn admin_supported_maximum(&self) -> u32 {
        self.levels
            .supplied_levels()
            .max()
            .unwrap_or(1)
            .min(self.levels.rules().max_player_level)
            .min(pg::MAX_SUPPORTED_LEVEL)
            .min(CLASSIC_LEVEL_CEILING)
    }
    pub(in crate::native) fn maximum_player_level(&self) -> u32 {
        self.levels.rules().max_player_level
    }
    pub fn quest_catalog(&self) -> &q::Catalog {
        &self.quests
    }
    pub fn quest(&self, id: u32) -> Result<&q::QuestRow, NativeError> {
        Ok(self.quests.row(id)?)
    }
    pub fn quest_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.quests.ids()
    }
    pub fn quest_relations(&self) -> &q::QuestRelations {
        &self.presentation_relations
    }
    pub(in crate::native) fn damage_cancels(&self, id: u32) -> bool {
        self.training
            .input()
            .spells
            .iter()
            .find(|s| s.id == id)
            .is_some_and(|s| s.words[21] & 0x10 != 0)
    }
    pub(in crate::native) fn school(&self, id: u32) -> Option<u8> {
        self.abilities.get(&id).map(|r| r.ability.school)
    }
    /// Full caller-supplied source closure, including passives and recursive
    /// learning dependencies that have no active execution recipe.
    pub fn spell_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.training
            .input()
            .spells
            .iter()
            .map(|row| row.id)
            .chain(self.abilities.keys().copied())
    }
    pub fn npc_flags(&self, entry: u32) -> Option<u32> {
        self.npc_rows.get(&entry).map(|r| r.npc_flags)
    }
    pub fn dialog_rules(&self) -> &QuestDialogRules {
        &self.dialog_rules
    }
    pub fn reward_quote(
        &self,
        id: u32,
        level: u32,
    ) -> Result<arithmetic::QuestRewardQuote, NativeError> {
        Ok(
            arithmetic::reward_quote(self.quests.row(id)?.reward, self.reward_context(level))
                .map_err(q::Error::from)?,
        )
    }
    fn reward_context(&self, level: u32) -> arithmetic::RewardContext {
        arithmetic::RewardContext {
            player_level: level as u8,
            max_player_level: self.levels.rules().max_player_level as u8,
            xp_rate: self.quest_xp_rate,
            money_rate: self.money_rate,
        }
    }
    fn validate_live_skills(&self, view: &pg::Snapshot) -> Result<(), NativeError> {
        let bounds = self
            .skill_bounds
            .get(&view.level)
            .ok_or(NativeError::InvalidCheckpoint)?;
        if view.skills.len() != bounds.len()
            || view
                .skills
                .iter()
                .zip(bounds)
                .zip(&self.initial.view().skills)
                .any(|((s, b), initial)| {
                    s.id != b.id
                        || s.position != b.position
                        || s.maximum != b.maximum
                        || s.step != b.step
                        || s.permanent_bonus != b.permanent_bonus
                        || s.temporary_bonus != b.temporary_bonus
                        || s.status == pg::SkillStatus::Deleted
                        || s.value < initial.value
                        || s.value > s.maximum
                })
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok(())
    }
    fn resolve_abilities(
        &self,
        training: &init::LiveTraining,
        view: &pg::Snapshot,
    ) -> Result<Vec<Ability>, NativeError> {
        self.resolve_ability_set(training, view, None)
    }
    pub(super) fn validate_prepare_transition(
        &self,
        next: &QuestProgressionState,
        prepared: &ExecutionBinding,
    ) -> Result<(), NativeError> {
        let training = init::LiveTraining::restore_authoritative(
            &self.training,
            &next.training,
            next.player.view(),
        )?;
        let ability =
            self.resolve_execution_ability(&training, next.player.view(), prepared.ability.id)?;
        let AbilityAdmission::Available(spec) = &ability.admission else {
            return Err(NativeError::UnsupportedAbility(prepared.ability.id));
        };
        if ability.id != prepared.ability.id || ability.school != prepared.ability.school {
            return Err(NativeError::InvalidState);
        }
        spell::MageProjectileReleaseContext::from_specs(prepared.spec()?, spec)?;
        Ok(())
    }
    fn resolve_execution_ability(
        &self,
        training: &init::LiveTraining,
        view: &pg::Snapshot,
        id: u32,
    ) -> Result<Ability, NativeError> {
        self.resolve_ability_set(training, view, Some(id))?
            .into_iter()
            .find(|a| a.id == id)
            .ok_or(NativeError::UnsupportedAbility(id))
    }
    fn resolve_ability_set(
        &self,
        training: &init::LiveTraining,
        view: &pg::Snapshot,
        historical: Option<u32>,
    ) -> Result<Vec<Ability>, NativeError> {
        let input = training.input();
        let init::Rows::Complete(rows) = &input.skill_abilities else {
            return Err(NativeError::InvalidProfile);
        };
        let links = mana::SkillLineAbilities::new(
            rows.iter()
                .map(|r| mana::SkillLineAbilityRow {
                    row_id: r.row_id,
                    spell_id: r.spell,
                    skill_id: r.skill.into(),
                })
                .collect(),
        )?;
        let skills = mana::CurrentSkills::new(
            view.skills
                .iter()
                .map(|s| mana::CurrentSkill {
                    id: s.id,
                    value: s.value,
                    permanent_bonus: s.permanent_bonus,
                    temporary_bonus: s.temporary_bonus,
                    deleted: s.status == pg::SkillStatus::Deleted,
                })
                .collect(),
        )?;
        let mut out = vec![];
        for known in training
            .abilities()
            .values()
            .filter(|a| a.active || historical == Some(a.spell_id))
        {
            if !matches!(
                known.availability,
                init::AbilityAvailability::Available | init::AbilityAvailability::Unavailable(_)
            ) {
                continue;
            }
            let recipe = self
                .abilities
                .get(&known.spell_id)
                .ok_or(NativeError::UnsupportedAbility(known.spell_id))?;
            let mut ability = recipe.ability.clone();
            match (&known.availability, &mut ability.admission) {
                (init::AbilityAvailability::Available, AbilityAdmission::Available(spec)) => {
                    let raw = input
                        .spells
                        .iter()
                        .find(|r| r.id == known.spell_id)
                        .ok_or(NativeError::InvalidProfile)?;
                    let time = recipe
                        .cast_time
                        .as_ref()
                        .ok_or(NativeError::InvalidProfile)?;
                    if time.id == 0
                        || raw.words[18] != time.id
                        || raw.words[31] != 0
                        || raw.words[28] > u8::MAX.into()
                        || raw.words[1] > 6
                        || raw.words[1] != u32::from(ability.school)
                        || spec.damage_pushback != (raw.words[21] & 2 != 0)
                    {
                        return Err(NativeError::InvalidProfile);
                    }
                    let rank = u16::try_from(mana::spell_rank(
                        known.spell_id,
                        raw.words[27],
                        &links,
                        &skills,
                    )?)
                    .map_err(|_| NativeError::Limit)?;
                    spec.spell_rank = rank;
                    spec.cast_ms = spell::cast_time(
                        time.base_ms,
                        time.per_level_ms,
                        time.minimum_ms,
                        rank,
                        raw.words[28] as u8,
                    )?;
                    spec.mana_cost = spell::mana_cost(
                        raw.words[32],
                        raw.words[33],
                        raw.words[156],
                        view.modifiers.create_mana,
                        rank,
                        raw.words[28] as u8,
                    )?;
                    spec.validate()?;
                    if spec.cast_ms == 0 {
                        return Err(NativeError::UnsupportedMechanic(
                            "instant active progression ability",
                        ));
                    }
                }
                (
                    init::AbilityAvailability::Unavailable(a),
                    AbilityAdmission::Unavailable { reason: b },
                ) if a == b => {}
                _ => return Err(NativeError::InvalidProfile),
            }
            ability.validate(self.tick_ms)?;
            out.push(ability);
        }
        Ok(out)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum QuestProgressionEvent {
    Reputation(Box<benilla_sim::reputation::Event>),
    RewardItem(Box<inv::Stored>),
    InventoryDestroyed(Box<inv::DestroyReceipt>),
    RewardRefused {
        quest: u32,
        giver: crate::NpcId,
        level: u8,
        result: Option<inv::InventoryResult>,
    },
    Quest {
        event: q::Event,
        level: u8,
        money: u32,
    },
    Progression(Box<pg::Observation>),
    KillExperience(Box<kill::Observation>),
    ChainDetails {
        quest: u32,
        giver: crate::NpcId,
        level: u8,
    },
    PartyKill {
        victim: crate::NpcId,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestProgressionState {
    pub xp_enabled: bool,
    pub reputation: Option<NativeReputationState>,
    pub inventory: Option<inv::State>,
    pub inventory_receipts: BTreeMap<u64, ItemReceipt>,
    pub loot_target: Option<crate::NpcId>,
    pub quests: q::State,
    pub player: pg::State,
    pub training: init::TrainingSnapshot,
    pub derived: DerivedStats,
    pub derived_calls: Vec<DerivedCall>,
    pub abilities: Vec<Ability>,
    pub money: u32,
    pub rest: kill::RestState,
    pub next_death_event: u64,
    pub credited_generations: BTreeMap<u64, u64>,
    pub played_clock_seconds: u64,
    pub played_time_total: u32,
    genesis: crate::Digest,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::native) struct QuestProgressionSnapshot {
    xp_enabled: bool,
    reputation: Option<NativeReputationState>,
    inventory: Option<inv::State>,
    inventory_receipts: BTreeMap<u64, ItemReceipt>,
    loot_target: Option<crate::NpcId>,
    policy: String,
    genesis: crate::Digest,
    quests: q::Snapshot,
    player: pg::Snapshot,
    pub(in crate::native) training: init::TrainingSnapshot,
    derived: DerivedStats,
    #[serde(deserialize_with = "derived::deserialize_calls")]
    pub(in crate::native) derived_calls: Vec<DerivedCall>,
    abilities: Vec<Ability>,
    money: u32,
    rest: kill::RestState,
    next_death_event: u64,
    credited_generations: BTreeMap<u64, u64>,
    played_clock_seconds: u64,
    played_time_total: u32,
}
#[derive(Clone)]
struct ProgressionHost {
    training: init::LiveTraining,
    derived: DerivedStats,
    derived_calls: Vec<DerivedCall>,
    inputs: DerivedInputs,
    audit: pg::Audit,
}
impl pg::Host for ProgressionHost {
    fn preflight(&self, catalog: &pg::Catalog, view: &pg::Snapshot) -> pg::Result<()> {
        if view.catalog_identity != catalog.identity() || view.audit != self.audit {
            return Err(pg::Error::Host("progression host identity"));
        }
        self.training
            .preflight(view)
            .map_err(|_| pg::Error::Host("actual live training preflight"))?;
        self.inputs.validate(view)
    }
    fn apply(&mut self, event: &pg::Event, view: &pg::Snapshot) -> pg::Result<()> {
        self.training
            .apply(event, view)
            .map_err(|_| pg::Error::Host("actual live training callback"))?;
        self.apply_derived(event, view)
    }
}
#[derive(Clone)]
struct QuestHost<'a> {
    selected_destroy: Option<inv::DestroyReceipt>,
    selected_original_destroy: Option<EquipmentDestroyReceipt>,
    xp_enabled: bool,
    reputation: Option<NativeReputationState>,
    reputation_calls: VecDeque<(reputation::Marker, Vec<QuestProgressionEvent>)>,
    inventory: Option<inv::State>,
    genesis: &'a QuestProgressionGenesis,
    player: pg::State,
    progression: ProgressionHost,
    money: u32,
    calls: VecDeque<(q::HostEffect, Vec<QuestProgressionEvent>, u8, u32)>,
}
impl q::Host for QuestHost<'_> {
    fn reputation(&self, faction: u32) -> Result<i32, q::Error> {
        self.current_reputation(faction)
    }
    fn reputation_visible(&mut self, faction: u32) -> Result<(), q::Error> {
        self.show_reputation(faction)
    }
    fn reward_reputation(
        &mut self,
        catalog: &q::Catalog,
        state: &mut q::State,
        quest: u32,
        _events: &mut Vec<q::Event>,
    ) -> Result<(), q::Error> {
        self.grant_reputation(catalog, state, quest)
    }
    fn participation(&self) -> q::Participation {
        q::Participation::Solo
    }
    fn identity(&self) -> arithmetic::QuestPlayerIdentity {
        let p = self.player.view();
        arithmetic::QuestPlayerIdentity {
            race: p.race,
            class: p.class,
            level: p.level as u8,
        }
    }
    fn reward_context(&self) -> arithmetic::RewardContext {
        let mut context = self.genesis.reward_context(self.player.view().level);
        if !self.xp_enabled {
            context.xp_rate = arithmetic::Binary32Rate::ZERO;
        }
        context
    }
    fn money(&self) -> u32 {
        self.money
    }
    fn item_count(&self, item: u32, _include_bank: bool) -> Result<u32, q::Error> {
        self.inventory
            .as_ref()
            .ok_or(q::Error::Unsupported("native inventory host"))?
            .item_count(item)
            .map_err(|_| q::Error::Host("inventory count"))
    }
    fn removal_steps(&self, item: u32, count: u32) -> Result<Vec<u32>, q::Error> {
        if let Some(receipt) = &self.selected_destroy {
            if receipt.item() != Some(item)
                || receipt.removed != count
                || self.inventory.as_ref() != Some(&receipt.before)
                || receipt.result != inv::InventoryResult::Ok
                || count == 0
            {
                return Err(q::Error::Host("selected inventory removal identity"));
            }
            return Ok(vec![count]);
        }
        let catalog = &self
            .genesis
            .loot
            .as_ref()
            .ok_or(q::Error::Unsupported("native inventory host"))?
            .inventory;
        self.inventory
            .as_ref()
            .ok_or(q::Error::Unsupported("native inventory host"))?
            .removal_steps(catalog, item, count)
            .map_err(|_| q::Error::Host("inventory removal"))
    }
    fn can_store_reward(
        &self,
        item: u32,
        count: u32,
        destinations: &mut Vec<inv::Destination>,
    ) -> Result<inv::InventoryResult, q::Error> {
        let catalog = &self
            .genesis
            .loot
            .as_ref()
            .ok_or(q::Error::Unsupported("native inventory host"))?
            .inventory;
        Ok(self
            .inventory
            .as_ref()
            .ok_or(q::Error::Unsupported("native inventory host"))?
            .can_store_with_destinations(catalog, item, count, destinations)
            .map_err(|_| q::Error::Host("reward storage plan"))?
            .result)
    }
    fn apply(&mut self, effect: &q::HostEffect) -> Result<(), q::Error> {
        let events = match effect {
            q::HostEffect::GiveXp(_) if !self.xp_enabled => vec![],
            q::HostEffect::GiveXp(xp) => {
                let tx = pg::give_xp(&self.genesis.levels, &self.player, &self.progression, *xp)
                    .map_err(|_| q::Error::Host("source progression transaction"))?;
                self.player = tx.state;
                self.progression = tx.host;
                tx.events
                    .into_iter()
                    .map(|e| QuestProgressionEvent::Progression(Box::new(e)))
                    .collect()
            }
            q::HostEffect::ModifyMoney(value) => {
                self.money = (i64::from(self.money) + i64::from(*value))
                    .clamp(0, i64::from(q::MAX_MONEY)) as u32;
                vec![]
            }
            q::HostEffect::DestroyCarriedStack { item, count } => {
                let catalog = &self
                    .genesis
                    .loot
                    .as_ref()
                    .ok_or(q::Error::Unsupported("native inventory host"))?
                    .inventory;
                if let Some(receipt) = &self.selected_destroy {
                    receipt
                        .validate(catalog)
                        .map_err(|_| q::Error::Host("selected inventory removal proof"))?;
                    if receipt.item() != Some(*item)
                        || receipt.removed != *count
                        || receipt.result != inv::InventoryResult::Ok
                        || *count == 0
                        || self.inventory.as_ref() != Some(&receipt.before)
                    {
                        return Err(q::Error::Host("selected inventory removal callback"));
                    }
                    let l = self
                        .genesis
                        .loot
                        .as_ref()
                        .ok_or(q::Error::Unsupported("native inventory host"))?;
                    let selected = receipt
                        .before
                        .slots
                        .get(&receipt.slot)
                        .ok_or(q::Error::Host("selected inventory removal identity"))?;
                    let original = l.equipment_origin(selected.id).is_some();
                    if original {
                        let permit = self
                            .selected_original_destroy
                            .as_ref()
                            .ok_or(q::Error::Unsupported("registered armor consumption"))?;
                        if permit.instance != selected.id || permit.destruction != *receipt {
                            return Err(q::Error::Host("selected original destruction proof"));
                        }
                    } else if self.selected_original_destroy.is_some() {
                        return Err(q::Error::Host("unexpected original destruction proof"));
                    }
                    // Sharing a template does not make an ordinary object the
                    // registered original. Only this exact selected reducer may
                    // bypass the conservative entry-wide removal hold.
                    let registered = |inventory: &inv::State| {
                        inventory
                            .slots
                            .iter()
                            .filter(|(_, instance)| l.equipment_origin(instance.id).is_some())
                            .map(|(&slot, instance)| (slot, instance.clone()))
                            .collect::<BTreeMap<_, _>>()
                    };
                    let mut expected_registered = registered(&receipt.before);
                    if original {
                        expected_registered.remove(&receipt.slot);
                    }
                    if expected_registered != registered(&receipt.after)
                        || receipt.before.equipped != receipt.after.equipped
                        || receipt.before.next_item != receipt.after.next_item
                    {
                        return Err(q::Error::Host("selected inventory registered custody"));
                    }
                    let receipt = self.selected_destroy.take().unwrap();
                    if original {
                        self.selected_original_destroy
                            .take()
                            .ok_or(q::Error::Host("original destruction permit consumed twice"))?;
                    }
                    self.inventory = Some(receipt.after.clone());
                    vec![QuestProgressionEvent::InventoryDestroyed(Box::new(receipt))]
                } else {
                    if let (Some(l), Some(inventory)) = (&self.genesis.loot, &self.inventory) {
                        if inventory.slots.values().any(|instance| {
                            instance.item == *item && l.equipment_origin(instance.id).is_some()
                        }) {
                            return Err(q::Error::Unsupported("registered armor consumption"));
                        }
                    }
                    self.inventory
                        .as_mut()
                        .ok_or(q::Error::Unsupported("native inventory host"))?
                        .remove_first_stack(catalog, *item, *count)
                        .map_err(|_| q::Error::Host("inventory removal"))?;
                    vec![]
                }
            }
            q::HostEffect::StoreRewardItem { item, count } => {
                let catalog = &self
                    .genesis
                    .loot
                    .as_ref()
                    .ok_or(q::Error::Unsupported("native inventory host"))?
                    .inventory;
                let stored = self
                    .inventory
                    .as_mut()
                    .ok_or(q::Error::Unsupported("native inventory host"))?
                    .store_new(catalog, *item, *count, |_, _, _| Ok(()))
                    .map_err(|_| q::Error::Host("source reward store"))?;
                vec![QuestProgressionEvent::RewardItem(Box::new(stored))]
            }
        };
        self.calls.push_back((
            effect.clone(),
            events,
            self.player.view().level as u8,
            self.money,
        ));
        Ok(())
    }
}
impl QuestProgressionState {
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(in crate::native) fn new(
        g: &QuestProgressionGenesis,
        now_ms: u64,
    ) -> Result<Self, NativeError> {
        Ok(Self {
            xp_enabled: true,
            reputation: reputation::initialize(g)?,
            inventory: g.loot.as_ref().map(|l| l.initial_inventory.clone()),
            inventory_receipts: g.loot.as_ref().map_or_else(BTreeMap::new, |l| {
                loot::initial_inventory_receipts(&l.initial_inventory)
            }),
            loot_target: None,
            quests: q::State::new(&g.quests),
            player: g.initial.clone(),
            training: g.training.snapshot(),
            derived: DerivedStats::from_snapshot(g.initial.view(), &g.initial_derived_inputs)?,
            derived_calls: vec![],
            abilities: g.resolve_abilities(&g.training, g.initial.view())?,
            money: g.start_money,
            rest: kill::RestState::fresh_player(),
            next_death_event: 1,
            credited_generations: BTreeMap::new(),
            played_clock_seconds: now_ms / 1000,
            played_time_total: 0,
            genesis: g.identity,
        })
    }
    pub(in crate::native) fn snapshot(&self) -> QuestProgressionSnapshot {
        QuestProgressionSnapshot {
            xp_enabled: self.xp_enabled,
            reputation: self.reputation.clone(),
            inventory: self.inventory.clone(),
            inventory_receipts: self.inventory_receipts.clone(),
            loot_target: self.loot_target,
            policy: QUEST_PROGRESSION_POLICY.into(),
            genesis: self.genesis,
            quests: self.quests.snapshot(),
            player: self.player.snapshot(),
            training: self.training.clone(),
            derived: self.derived.clone(),
            derived_calls: self.derived_calls.clone(),
            abilities: self.abilities.clone(),
            money: self.money,
            rest: self.rest,
            next_death_event: self.next_death_event,
            credited_generations: self.credited_generations.clone(),
            played_clock_seconds: self.played_clock_seconds,
            played_time_total: self.played_time_total,
        }
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(in crate::native) fn restore(
        g: &QuestProgressionGenesis,
        s: QuestProgressionSnapshot,
        c: &PlayerCombatState,
        health: u32,
        mana: u32,
        now_ms: u64,
    ) -> Result<Self, NativeError> {
        if s.policy != QUEST_PROGRESSION_POLICY
            || s.genesis != g.identity
            || s.money > q::MAX_MONEY
            || s.rest != kill::RestState::fresh_player()
            || s.next_death_event == 0
            || s.credited_generations.len() > 4096
            || s.played_clock_seconds != now_ms / 1000
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        let player = pg::State::restore(&g.levels, s.player)?;
        g.validate_live_skills(player.view())?;
        let training =
            init::LiveTraining::restore_authoritative(&g.training, &s.training, player.view())?;
        if g.resolve_abilities(&training, player.view())? != s.abilities {
            return Err(NativeError::InvalidCheckpoint);
        }
        let out = Self {
            xp_enabled: s.xp_enabled,
            reputation: s.reputation,
            inventory: s.inventory,
            inventory_receipts: s.inventory_receipts,
            loot_target: s.loot_target,
            quests: q::State::restore(&g.quests, s.quests)?,
            player,
            training: s.training,
            derived: s.derived,
            derived_calls: s.derived_calls,
            abilities: s.abilities,
            money: s.money,
            rest: s.rest,
            next_death_event: s.next_death_event,
            credited_generations: s.credited_generations,
            played_clock_seconds: s.played_clock_seconds,
            played_time_total: s.played_time_total,
            genesis: s.genesis,
        };
        match (&g.loot, &out.inventory) {
            (Some(l), Some(i)) => {
                let custody = equipment_movement::placements(g, &out.derived_calls)?;
                equipment_movement::validate_inventory(g, i, &custody)?;
                if i.next_item < l.initial_inventory.next_item
                    || out.inventory_receipts.len() != i.slots.len()
                    || i.slots.iter().any(|(&slot, item)| {
                        out.inventory_receipts
                            .get(&item.id)
                            .is_none_or(|r| !r.validates(slot, item))
                    })
                {
                    return Err(NativeError::InvalidCheckpoint);
                }
                i.validate(&l.inventory)?;
            }
            (None, None) if out.inventory_receipts.is_empty() && out.loot_target.is_none() => {}
            _ => return Err(NativeError::InvalidCheckpoint),
        }
        let progression_boundaries = out.validate_derived(g, c)?;
        reputation::validate_saved(g, &out, &progression_boundaries)?;
        let mut check = out.clone();
        check.synchronize(g, c, health, mana)?;
        if check != out {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok(out)
    }
    pub(in crate::native) fn synchronize(
        &mut self,
        g: &QuestProgressionGenesis,
        c: &PlayerCombatState,
        health: u32,
        mana: u32,
    ) -> Result<(), NativeError> {
        let mut view = self.player.snapshot();
        view.modifiers = c.recipe.snapshot().modifiers;
        let mut powers = view.numeric.powers;
        powers[0] = mana;
        view.numeric =
            pg::NumericState::from_modifiers(view.level, &view.modifiers, health, powers)?;
        view.alive = health != 0;
        if c.skills.len() != view.skills.len() {
            return Err(NativeError::InvalidState);
        }
        for skill in &mut view.skills {
            let current = c.skills.get(&skill.id).ok_or(NativeError::InvalidState)?;
            if current.maximum != skill.maximum
                || current.step != skill.step
                || current.permanent_bonus != skill.permanent_bonus
                || current.temporary_bonus != skill.temporary_bonus
                || current.value < skill.value
            {
                return Err(NativeError::InvalidState);
            }
            if current.value != skill.value {
                skill.value = current.value;
                skill.status = pg::SkillStatus::Changed;
            }
        }
        let player = pg::State::restore(&g.levels, view)?;
        g.validate_live_skills(player.view())?;
        let training =
            init::LiveTraining::restore_authoritative(&g.training, &self.training, player.view())?;
        let abilities = g.resolve_abilities(&training, player.view())?;
        self.player = player;
        self.abilities = abilities;
        Ok(())
    }
    pub(in crate::native) fn played_time(
        &mut self,
        g: &QuestProgressionGenesis,
        now_ms: u64,
    ) -> Result<(), NativeError> {
        let seconds = now_ms / 1000;
        let elapsed = seconds
            .checked_sub(self.played_clock_seconds)
            .ok_or(NativeError::InvalidState)?;
        let mut view = self.player.snapshot();
        view.played_time_level = view.played_time_level.wrapping_add(elapsed as u32);
        self.player = pg::State::restore(&g.levels, view)?;
        self.played_time_total = self.played_time_total.wrapping_add(elapsed as u32);
        self.played_clock_seconds = seconds;
        Ok(())
    }
    pub(in crate::native) fn admin_give_level(
        &self,
        g: &QuestProgressionGenesis,
        combat: &PlayerCombatState,
        level: u32,
    ) -> Result<(Self, Vec<QuestProgressionEvent>), NativeError> {
        let training = init::LiveTraining::restore_authoritative(
            &g.training,
            &self.training,
            self.player.view(),
        )?;
        let host = ProgressionHost {
            training,
            derived: self.derived.clone(),
            derived_calls: self.derived_calls.clone(),
            inputs: g.derived_inputs(combat),
            audit: self.player.view().audit,
        };
        let tx = pg::give_level(&g.levels, &self.player, &host, level)?;
        let mut next = self.clone();
        next.player = tx.state;
        next.abilities = g.resolve_abilities(&tx.host.training, next.player.view())?;
        next.training = tx.host.training.snapshot();
        next.derived = tx.host.derived;
        next.derived_calls = tx.host.derived_calls;
        Ok((
            next,
            tx.events
                .into_iter()
                .map(|e| QuestProgressionEvent::Progression(Box::new(e)))
                .collect(),
        ))
    }
    pub(in crate::native) fn reduce(
        &self,
        g: &QuestProgressionGenesis,
        combat: &PlayerCombatState,
        command: q::Command,
    ) -> Result<(Self, Vec<QuestProgressionEvent>), NativeError> {
        self.reduce_inner(g, combat, command, None, None)
    }
    pub(in crate::native) fn destroy_selected(
        &self,
        g: &QuestProgressionGenesis,
        combat: &PlayerCombatState,
        receipt: inv::DestroyReceipt,
    ) -> Result<(Self, Vec<QuestProgressionEvent>), NativeError> {
        let item = receipt.item().ok_or(NativeError::InvalidState)?;
        let count = receipt.removed;
        self.reduce_inner(
            g,
            combat,
            q::Command::RemoveItems { item, count },
            Some(receipt),
            None,
        )
    }
    fn reduce_inner(
        &self,
        g: &QuestProgressionGenesis,
        combat: &PlayerCombatState,
        command: q::Command,
        selected_destroy: Option<inv::DestroyReceipt>,
        selected_original_destroy: Option<EquipmentDestroyReceipt>,
    ) -> Result<(Self, Vec<QuestProgressionEvent>), NativeError> {
        if let Some(receipt) = &selected_destroy {
            receipt.validate(&g.loot.as_ref().ok_or(NativeError::InvalidState)?.inventory)?;
            if receipt.result != inv::InventoryResult::Ok
                || receipt.removed == 0
                || self.inventory.as_ref() != Some(&receipt.before)
                || !matches!(&command, q::Command::RemoveItems { item, count } if receipt.item() == Some(*item) && receipt.removed == *count)
            {
                return Err(NativeError::InvalidState);
            }
        }
        if let Some(permit) = &selected_original_destroy {
            if selected_destroy.as_ref() != Some(&permit.destruction)
                || self.inventory_receipts != permit.before_receipts
            {
                return Err(NativeError::InvalidState);
            }
            let mut custody = equipment_movement::placements(g, &self.derived_calls)?;
            permit.replay(g, &mut custody)?;
        }
        let training = init::LiveTraining::restore_authoritative(
            &g.training,
            &self.training,
            self.player.view(),
        )?;
        let host = QuestHost {
            selected_destroy,
            selected_original_destroy,
            xp_enabled: self.xp_enabled,
            reputation: self.reputation.clone(),
            reputation_calls: VecDeque::new(),
            inventory: self.inventory.clone(),
            genesis: g,
            player: self.player.clone(),
            money: self.money,
            calls: VecDeque::new(),
            progression: ProgressionHost {
                training,
                derived: self.derived.clone(),
                derived_calls: self.derived_calls.clone(),
                inputs: g.derived_inputs(combat),
                audit: self.player.view().audit,
            },
        };
        let mut tx = q::reduce(
            &g.quests,
            &self.quests,
            &host,
            self.quests.next_sequence(),
            command,
        )?;
        let (mut level, mut money) = (self.player.view().level as u8, self.money);
        let mut events = vec![];
        for event in tx.events {
            if let Some(marker) = reputation::Marker::from_event(&event) {
                if let Some((expected, nested)) = tx.host.reputation_calls.pop_front() {
                    if marker != expected {
                        return Err(NativeError::InvalidState);
                    }
                    events.extend(nested);
                } else if tx.host.reputation.is_some() {
                    return Err(NativeError::InvalidState);
                }
            }
            if let q::Event::Host(effect) = &event {
                let (expected, nested, l, m) =
                    tx.host.calls.pop_front().ok_or(NativeError::InvalidState)?;
                if *effect != expected {
                    return Err(NativeError::InvalidState);
                }
                events.extend(nested);
                level = l;
                money = m;
            }
            events.push(QuestProgressionEvent::Quest {
                event,
                level,
                money,
            });
        }
        if !tx.host.calls.is_empty()
            || !tx.host.reputation_calls.is_empty()
            || tx.host.selected_destroy.is_some()
            || tx.host.selected_original_destroy.is_some()
        {
            return Err(NativeError::InvalidState);
        }
        let mut next = self.clone();
        next.reputation = tx.host.reputation;
        next.quests = tx.state;
        next.player = tx.host.player;
        next.abilities = g.resolve_abilities(&tx.host.progression.training, next.player.view())?;
        next.training = tx.host.progression.training.snapshot();
        next.derived = tx.host.progression.derived;
        next.derived_calls = tx.host.progression.derived_calls;
        next.money = tx.host.money;
        next.inventory = tx.host.inventory;
        reputation::record_objective_mutations(self, &mut next, g, &events)?;
        if next.inventory != self.inventory {
            let reason = crate::digest(
                b"benilla-native-quest-item-mutation-v1\0",
                &(self.quests.next_sequence(), &events),
            );
            loot::update_inventory_receipts(&mut next, reason)?;
        }
        Ok((next, events))
    }
}

#[cfg(test)]
mod level_sixty_tests;

#[cfg(test)]
mod level_sixty_capacity_tests;
