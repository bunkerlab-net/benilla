// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1: Player::UpdateSkillTrainedSpells / addSpell.
//! Live, transactional reuse of the admitted initializer's actual learning path.
//! Recursive skill setters remain unsupported: native SetSkill changes status
//! and invokes defense callbacks even when value, maximum, and step stay equal.
use super::{
    first_entry, initialize_mage, missing, stats::Stats, unsupported, AbilityAvailability,
    GrantSource, Init, InitError, InitEvent, InitializedMage, InitializedSkill, KnownAbility,
    Resources, Result, Rows, StarterInput,
};
use crate::player_progression::{self as progression, Event, SkillStatus, Snapshot};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const LIVE_TRAINING_POLICY_ID: &str = "cmangos-8ec338a1-live-fixed-skill-training-v2";
const MAX_CALLS: usize = 4096;
const MAX_SPELLS: usize = 4096;

/// An actual source callback, including the intermediate traversal state. A
/// defense gain alone is deliberately not a training callback.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainingCallSnapshot {
    pub id: u16,
    pub value: u16,
    pub step: u16,
    pub view: Snapshot,
    /// Actual SMSG_LEARNED_SPELL IDs in source callback order. Recursive active
    /// descendants precede their parent; inactive additions and no-ops are absent.
    pub learned_spells: Vec<u32>,
}
/// Bind this replay journal to the owner's authenticated profile envelope. The
/// journal proves the learned closure, not the authority of its source events.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainingSnapshot {
    pub version: u32,
    pub identity: [u8; 32],
    pub abilities: Vec<KnownAbility>,
    pub calls: Vec<TrainingCallSnapshot>,
}
/// Deep-cloned state for the native progression Host; no shared mutable state or
/// I/O. This component handles learning only; the Host must also execute all
/// numeric/stat hooks and validate authoritative progression/equipment inputs.
#[derive(Clone, Debug)]
pub struct LiveTraining {
    input: StarterInput,
    created: InitializedMage,
    initial_skills: BTreeMap<u16, InitializedSkill>,
    known: BTreeMap<u32, KnownAbility>,
    calls: Vec<TrainingCallSnapshot>,
    identity: [u8; 32],
}
impl LiveTraining {
    /// Public fields of InitializedMage are not authority: reinitialize and
    /// verify the complete result, then prove the fresh-login replay contract.
    pub fn from_initializer(input: &StarterInput, created: &InitializedMage) -> Result<Self> {
        if initialize_mage(input)? != *created {
            return Err(InitError::Invalid("live training initializer provenance"));
        }
        let entry = first_entry(input, created)?;
        complete_learning_closure(input, &entry.skills, &created.abilities)?;
        // first_entry proves its replayed spellbook exactly equals creation.
        Ok(Self {
            input: input.clone(),
            created: created.clone(),
            initial_skills: entry.skills,
            known: created.abilities.clone(),
            calls: vec![],
            identity: input_identity(input),
        })
    }
    pub fn identity(&self) -> [u8; 32] {
        self.identity
    }
    pub fn input(&self) -> &StarterInput {
        &self.input
    }
    pub fn abilities(&self) -> &BTreeMap<u32, KnownAbility> {
        &self.known
    }
    pub fn snapshot(&self) -> TrainingSnapshot {
        TrainingSnapshot {
            version: 2,
            identity: self.identity,
            abilities: self.known.values().cloned().collect(),
            calls: self.calls.clone(),
        }
    }
    /// Replays only recorded callbacks, in authored order. Training every final
    /// skill would invent learning after source UpdateDefense/UpdateSkill gains.
    pub fn restore_authoritative(
        fresh: &Self,
        snapshot: &TrainingSnapshot,
        final_view: &Snapshot,
    ) -> Result<Self> {
        if !fresh.calls.is_empty()
            || snapshot.version != 2
            || snapshot.identity != fresh.identity
            || snapshot.calls.len() > MAX_CALLS
            || snapshot.abilities.len() > MAX_SPELLS
            || snapshot
                .calls
                .iter()
                .any(|call| call.learned_spells.len() > MAX_SPELLS)
        {
            return Err(InitError::Invalid(
                "live training checkpoint identity/bound",
            ));
        }
        let mut restored = fresh.clone();
        restored.preflight(final_view)?;
        for call in &snapshot.calls {
            if call.view.catalog_identity != final_view.catalog_identity
                || call.view.level > final_view.level
            {
                return Err(InitError::Invalid("live training callback catalog/level"));
            }
            restored.apply(
                &Event::SkillTrainedSpells {
                    id: call.id,
                    value: call.value,
                    step: call.step,
                },
                &call.view,
            )?;
        }
        restored.preflight(final_view)?;
        if restored.snapshot() != *snapshot {
            return Err(InitError::Invalid("live training replay mismatch"));
        }
        Ok(restored)
    }
    /// Call alongside the Host's catalog and modifier-provenance checks. During
    /// GiveLevel, create rows/numeric fields legitimately still describe the old
    /// level while `level` already contains the new one.
    pub fn preflight(&self, view: &Snapshot) -> Result<()> {
        self.skills_at(view)?;
        if let Some(previous) = self.calls.last() {
            if previous.view.catalog_identity != view.catalog_identity
                || previous.view.level > view.level
            {
                return Err(InitError::Invalid(
                    "live training nonmonotonic identity/level",
                ));
            }
            let old = self.skills_at(&previous.view)?;
            for skill in &view.skills {
                let prior = &old[&skill.id];
                if skill.value < prior.value || skill.maximum < prior.maximum {
                    return Err(unsupported(
                        "live training decreasing skill",
                        skill.id.into(),
                        0,
                    ));
                }
            }
        }
        Ok(())
    }
    fn skills_at(&self, view: &Snapshot) -> Result<BTreeMap<u16, InitializedSkill>> {
        if view.policy != progression::POLICY_ID
            || view.race != self.input.race
            || view.class != self.input.class
            || !(1..=progression::MAX_SUPPORTED_LEVEL).contains(&view.level)
            || view.skills.len() != self.initial_skills.len()
        {
            return Err(InitError::Invalid("live training identity/fixed skills"));
        }
        let mut skills = BTreeMap::new();
        let mut positions = BTreeSet::new();
        for row in &view.skills {
            let initial = self
                .initial_skills
                .get(&row.id)
                .ok_or(InitError::Invalid("live training new skill"))?;
            if row.status == SkillStatus::Deleted
                || row.position >= 127
                || !positions.insert(row.position)
                || row.value < initial.value
                || row.maximum < initial.maximum
                || row.step != initial.step
                || row.permanent_bonus != initial.permanent_bonus
                || row.temporary_bonus != initial.temporary_bonus
            {
                return Err(InitError::Invalid("live training fixed skill state"));
            }
            if skills
                .insert(
                    row.id,
                    InitializedSkill {
                        id: row.id,
                        value: row.value,
                        maximum: row.maximum,
                        step: row.step,
                        permanent_bonus: row.permanent_bonus,
                        temporary_bonus: row.temporary_bonus,
                    },
                )
                .is_some()
            {
                return Err(InitError::Invalid("live training duplicate skill"));
            }
        }
        Ok(skills)
    }
    /// Actual `Init::skill_trained_spells` execution on callback-time state.
    /// Nothing is committed until every fixed-state invariant has been proved.
    pub fn apply(&mut self, event: &Event, view: &Snapshot) -> Result<()> {
        let Event::SkillTrainedSpells { id, value, step } = *event else {
            return Ok(());
        };
        self.preflight(view)?;
        if self.calls.len() == MAX_CALLS {
            return Err(InitError::Invalid("live training callback bound"));
        }
        let skills = self.skills_at(view)?;
        let selected = skills
            .get(&id)
            .ok_or(InitError::Invalid("live training callback skill"))?;
        if selected.value != value || selected.step != step {
            return Err(InitError::Invalid("live training callback value/step"));
        }
        let mut input = self.input.clone();
        input.level = view.level as u8;
        input.create_attributes = view.modifiers.create;
        input.create_health = view.modifiers.create_health;
        input.create_mana = view.modifiers.create_mana;
        let mut stats = Stats::restore(&view.modifiers)?;
        stats.effective = view.numeric.attributes;
        stats.resistances = view.numeric.resistances;
        stats.resources = Resources {
            health: view.numeric.health,
            max_health: view.numeric.max_health,
            mana: view.numeric.powers[0],
            max_mana: view.numeric.max_powers[0],
        };
        let resources = stats.resources;
        let skill_auras: Vec<_> = self
            .created
            .initial_auras
            .iter()
            .filter(|a| matches!(a.aura_type, 30 | 98))
            .map(|a| (a.misc_value as u16, a.amount as i16, a.aura_type == 98))
            .collect();
        let applied_auras: BTreeSet<_> = self
            .created
            .initial_auras
            .iter()
            .map(|a| (a.aura_type, a.misc_value))
            .collect();
        let mut replay = Init {
            input: &input,
            stats,
            skills: skills.clone(),
            known: self.known.clone(),
            trace: vec![],
            learned_spells: vec![],
            skill_auras: skill_auras.clone(),
            applied_auras: applied_auras.clone(),
            initial_auras: self.created.initial_auras.clone(),
            weapon_proficiency: self.created.weapon_proficiency,
            armor_proficiency: self.created.armor_proficiency,
            can_parry: self.created.can_parry,
            can_block: self.created.can_block,
            budget: 4096,
        };
        replay.skill_trained_spells(id, value)?;
        if replay.skills != skills
            || replay
                .trace
                .iter()
                .any(|event| matches!(event, InitEvent::SkillSet { .. }))
        {
            return Err(unsupported(
                "live training changed fixed skills",
                id.into(),
                0,
            ));
        }
        if replay.stats.snapshot() != view.modifiers
            || replay.stats.effective != view.numeric.attributes
            || replay.stats.resistances != view.numeric.resistances
            || replay.stats.resources != resources
            || replay.initial_auras != self.created.initial_auras
            || replay.skill_auras != skill_auras
            || replay.applied_auras != applied_auras
            || replay.weapon_proficiency != self.created.weapon_proficiency
            || replay.armor_proficiency != self.created.armor_proficiency
            || replay.can_parry != self.created.can_parry
            || replay.can_block != self.created.can_block
        {
            return Err(unsupported(
                "live training changed fixed modifiers/passives",
                id.into(),
                0,
            ));
        }
        if replay.known.len() > MAX_SPELLS
            || replay.learned_spells.len() > MAX_SPELLS
            || self
                .known
                .iter()
                .any(|(id, known)| replay.known.get(id) != Some(known))
            || replay.known.values().any(|known| {
                !self.known.contains_key(&known.spell_id)
                    && matches!(known.availability, AbilityAvailability::SkillStep)
            })
        {
            return Err(InitError::Invalid("live training changed spellbook domain"));
        }
        self.known = replay.known;
        self.calls.push(TrainingCallSnapshot {
            id,
            value,
            step,
            view: view.clone(),
            learned_spells: replay.learned_spells,
        });
        Ok(())
    }
}

fn complete_learning_closure(
    input: &StarterInput,
    skills: &BTreeMap<u16, InitializedSkill>,
    known: &BTreeMap<u32, KnownAbility>,
) -> Result<()> {
    let mut pending: Vec<_> = known.keys().copied().collect();
    for ability in input
        .skill_abilities
        .require("live SkillLineAbility closure", 0)?
    {
        if skills.contains_key(&ability.skill) {
            pending.push(ability.spell);
            if ability.forward_spell_id != 0 {
                pending.push(ability.forward_spell_id);
            }
        }
    }
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        if seen.len() > MAX_SPELLS {
            return Err(InitError::Invalid("live training spell closure bound"));
        }
        let spell = input
            .spells
            .iter()
            .find(|s| s.id == id)
            .ok_or(missing("live training spell closure", id))?;
        if spell.words[0] != id || id == 0 {
            return Err(InitError::Invalid("live training spell row ID"));
        }
        if let Some(previous) = spell.previous_rank {
            pending.push(previous);
        }
        for child in spell
            .learned_spells
            .require("live spell_learn_spell closure", id)?
        {
            pending.push(child.spell_id);
        }
        for i in 0..3 {
            if spell.words[61 + i] == 36 {
                pending.push(spell.words[109 + i]);
            }
        }
    }
    Ok(())
}

/// Length-delimited, fixed-width encoding of every caller-supplied field. Source
/// vector order is retained; Missing differs from a captured empty relationship.
fn input_identity(input: &StarterInput) -> [u8; 32] {
    struct Hash(Sha256);
    impl Hash {
        fn words(&mut self, values: impl IntoIterator<Item = u32>) {
            for value in values {
                self.0.update(value.to_le_bytes());
            }
        }
        fn count(&mut self, len: usize) {
            self.0.update((len as u64).to_le_bytes());
        }
        fn text(&mut self, text: &str) {
            self.count(text.len());
            self.0.update(text.as_bytes());
        }
        fn optional(&mut self, value: Option<u32>) {
            self.words([u32::from(value.is_some()), value.unwrap_or(0)]);
        }
        fn rows<T>(&mut self, rows: &Rows<T>, encode: impl Fn(&mut Self, &T)) {
            match rows {
                Rows::Missing => self.words([0]),
                Rows::Complete(rows) => {
                    self.words([1]);
                    self.count(rows.len());
                    for row in rows {
                        encode(self, row);
                    }
                }
            }
        }
    }
    let mut h = Hash(Sha256::new());
    h.text(LIVE_TRAINING_POLICY_ID);
    h.text(super::STARTER_INIT_POLICY_ID);
    h.words([
        input.race.into(),
        input.class.into(),
        input.gender.into(),
        input.level.into(),
        input.class_power_type,
    ]);
    h.words(input.create_attributes.0.map(|v| v as u32));
    h.words([input.create_health, input.create_mana]);
    h.rows(&input.default_skills, |h, r| {
        h.words([r.id.into(), r.step.into()])
    });
    h.rows(&input.default_spells, |h, r| h.words([*r]));
    h.count(input.skill_lines.len());
    for row in &input.skill_lines {
        h.words([row.id.into(), row.category as u32]);
    }
    h.rows(&input.skill_race_class, |h, r| {
        h.words([
            r.row_id,
            r.skill.into(),
            r.race_mask,
            r.class_mask,
            r.flags,
            r.tier,
        ])
    });
    h.count(input.skill_tiers.len());
    for row in &input.skill_tiers {
        h.words([row.id]);
        h.words(row.values);
        h.words(row.maxima);
    }
    h.rows(&input.skill_abilities, |h, r| {
        h.words([
            r.row_id,
            r.skill.into(),
            r.spell,
            r.forward_spell_id,
            r.race_mask,
            r.class_mask,
            r.required_value,
            r.learn_on_get_skill,
            r.max_value,
        ])
    });
    h.count(input.spells.len());
    for row in &input.spells {
        h.words([row.id]);
        h.words(row.words);
        h.optional(row.duration_ms.map(|v| v as u32));
        h.optional(row.previous_rank);
        h.words([u32::from(row.talent)]);
        h.rows(&row.learned_spells, |h, r| {
            h.words([r.spell_id, u32::from(r.active), u32::from(r.auto_learned)])
        });
        h.words([u32::from(row.external_initial_hooks)]);
    }
    h.count(input.active_support.len());
    for row in &input.active_support {
        h.words([row.spell_id, u32::from(row.unavailable_reason.is_some())]);
        if let Some(reason) = &row.unavailable_reason {
            h.text(reason);
        }
    }
    h.rows(&input.item_grants, |h, r| {
        h.words([
            match r.source {
                GrantSource::Outfit => 0,
                GrantSource::Custom => 1,
            },
            r.source_order,
            r.item,
            r.count,
        ])
    });
    h.count(input.items.len());
    for r in &input.items {
        h.words([
            r.id,
            r.class,
            r.subclass,
            r.inventory_type,
            r.allowable_class,
            r.allowable_race,
            r.required_level,
            r.required_skill.into(),
            r.required_skill_rank,
            r.required_spell,
            r.required_honor_rank,
            r.required_reputation_faction,
            r.flags,
            r.unique_max_count,
            r.stackable,
            r.bag_family,
            r.container_slots,
            r.item_set,
            r.random_property,
            r.duration as u32,
            u32::from(r.scripted),
        ]);
        for stat in r.stats {
            h.words([stat.kind, stat.value as u32]);
        }
        h.words([r.armor]);
        h.words(r.resistances);
        h.words([r.block]);
        for spell in r.spells {
            h.words([spell.id, spell.trigger]);
        }
        for damage in r.damage {
            h.words([damage.min_bits, damage.max_bits, damage.school]);
        }
        h.words([r.delay_ms, r.max_durability]);
    }
    h.words([u32::from(input.item_creation_hooks)]);
    h.0.finalize().into()
}
