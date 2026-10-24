// SPDX-License-Identifier: GPL-3.0-or-later
//! Seed-independent projection of actual creation and first-entry learning.
//! Supplied source identities are content bindings, never capture certificates.
//! Native execution must independently verify its complete source/hook closure.
use super::{
    initial_cast_phases::{
        InitialAmountOperation, InitialCastPlan, InitialEffectPhase, InitialProficiencyMasks,
        SourcePhaseFields, SourcePhaseResolutions, SourcePlanBuilder,
    },
    FirstEntryState, Init, InitError, InitEvent, InitialAura, InitializedMage, Result, Rows,
    SpellRow, StarterInput,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const STARTER_PREPARATION_POLICY: &str = "cmangos-8ec338a1-source-starter-preparation-v1";

/// Missing is a witnessed lookup at this exact index, not a default zero.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub enum InitialSpellLookup<T> {
    Unknown,
    Missing { index: u32 },
    Found(T),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct SpellCastTimeSource {
    pub id: u32,
    pub base_ms: i32,
    pub per_level_ms: i32,
    pub minimum_ms: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct SpellDurationSource {
    pub id: u32,
    pub base_ms: i32,
    pub per_level_ms: i32,
    pub maximum_ms: i32,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct StarterInitialDependency {
    pub spell_id: u32,
    /// The unchanged full SpellRow::source_hash contract, not a raw-only digest.
    pub source_hash: [u8; 32],
    pub cast_time: InitialSpellLookup<SpellCastTimeSource>,
    pub duration: InitialSpellLookup<SpellDurationSource>,
    pub server_side: u32,
    pub server_attributes: u32,
    pub bonus_coefficients: [u32; 3],
    pub attack_power_coefficients: [u32; 3],
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct StarterInitialDependencies {
    pub source_identity: [u8; 32],
    pub store_identity: [u8; 32],
    /// Binds the caller's separate complete hook/area/query evidence. This digest
    /// is not itself proof of hook absence; native must revalidate that evidence.
    pub hook_scope_identity: [u8; 32],
    /// Exact sorted union of initial cast IDs reached in both actual traversals.
    pub queried_spell_ids: Vec<u32>,
    pub rows: Rows<StarterInitialDependency>,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub enum InitialCastOrderEvidence {
    Unknown,
    /// Synthetic order contract only. Never promoted to a loaded observation.
    Authored {
        identity: [u8; 32],
    },
    ObservedLoadedOrder {
        capture_identity: [u8; 32],
        loader_identity: [u8; 32],
    },
    /// Reserved: no exact stochastic traversal equivalence proof is implemented.
    ProvenProjectionEquivalent {
        proof_identity: [u8; 32],
    },
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct StarterCastOrder {
    pub creation: InitialCastOrderEvidence,
    pub first_entry: InitialCastOrderEvidence,
    /// Must drive the actual traversal and be an exact normalized-skill permutation.
    pub entry_skill_order: Option<Vec<u16>>,
}

/// No constructor or deserializer can replace the actual learning traversal.
/// The projection has unresolved Dodge amounts; it is not a completed cast state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedStarter {
    source_identity: [u8; 32],
    dependencies: StarterInitialDependencies,
    order: StarterCastOrder,
    created: InitializedMage,
    entry: FirstEntryState,
    creation_plan: InitialCastPlan,
    entry_plan: InitialCastPlan,
    identity: [u8; 32],
}
impl serde::Serialize for PreparedStarter {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serde::Serialize::serialize(&self.identity, serializer)
    }
}
impl PreparedStarter {
    pub fn source_identity(&self) -> [u8; 32] {
        self.source_identity
    }
    pub fn identity(&self) -> [u8; 32] {
        self.identity
    }
    pub fn dependencies(&self) -> &StarterInitialDependencies {
        &self.dependencies
    }
    pub fn numeric_dependencies(&self) -> &StarterInitialDependencies {
        &self.dependencies
    }
    pub fn order(&self) -> &StarterCastOrder {
        &self.order
    }
    pub fn created(&self) -> &InitializedMage {
        &self.created
    }
    pub fn first_entry(&self) -> &FirstEntryState {
        &self.entry
    }
    pub fn creation_plan(&self) -> &InitialCastPlan {
        &self.creation_plan
    }
    pub fn first_entry_plan(&self) -> &InitialCastPlan {
        &self.entry_plan
    }
    /// Fresh source Unit construction begins both epochs with zero proficiency masks.
    pub fn first_entry_initial_masks(&self) -> InitialProficiencyMasks {
        InitialProficiencyMasks {
            weapon: 0,
            armor: 0,
        }
    }
    pub fn validate_against(&self, input: &StarterInput) -> Result<()> {
        if self.source_identity != input.source_identity()
            || prepare_starter(input, &self.dependencies, &self.order)? != *self
        {
            return Err(InitError::Invalid("starter preparation binding"));
        }
        Ok(())
    }
}

/// Runs the real recursive learning reducers. No occurrence/request list or RNG
/// is accepted; pending operations are emitted only at their actual cast point.
pub fn prepare_starter(
    input: &StarterInput,
    dependencies: &StarterInitialDependencies,
    order: &StarterCastOrder,
) -> Result<PreparedStarter> {
    dependencies.validate(input)?;
    validate_order(&order.creation)?;
    validate_order(&order.first_entry)?;
    let entry_order = order.entry_skill_order.as_ref().ok_or(InitError::Invalid(
        "missing explicit first-entry skill order",
    ))?;
    let context = |epoch: u8| {
        let mut h = Sha256::new();
        h.update(STARTER_PREPARATION_POLICY.as_bytes());
        h.update([0, epoch]);
        h.update(input.source_identity());
        h.update(dependencies.identity());
        hash_order(&mut h, order);
        <[u8; 32]>::from(h.finalize())
    };
    let (created, creation) =
        super::initialize_mage_inner(input, Some(CastCollector::new(dependencies, context(0))))?;
    let (entry, first_entry) = super::skills::first_entry_inner(
        input,
        &created,
        Some(CastCollector::new(dependencies, context(1))),
        Some(entry_order),
    )?;
    let creation_plan = creation
        .ok_or(InitError::Invalid("missing creation collector"))?
        .finish();
    let entry_plan = first_entry
        .ok_or(InitError::Invalid("missing entry collector"))?
        .finish();
    let ids: BTreeSet<_> = creation_plan
        .occurrences()
        .iter()
        .chain(entry_plan.occurrences())
        .map(|cast| cast.row().id)
        .collect();
    if !ids
        .iter()
        .copied()
        .eq(dependencies.queried_spell_ids.iter().copied())
    {
        return Err(InitError::Invalid("initial dependency exact reached scope"));
    }
    let mut h = Sha256::new();
    h.update(STARTER_PREPARATION_POLICY.as_bytes());
    h.update([0]);
    h.update(context(0));
    h.update(context(1));
    h.update(creation_plan.identity());
    h.update(entry_plan.identity());
    Ok(PreparedStarter {
        source_identity: input.source_identity(),
        dependencies: dependencies.clone(),
        order: order.clone(),
        created,
        entry,
        creation_plan,
        entry_plan,
        identity: h.finalize().into(),
    })
}
fn validate_order(order: &InitialCastOrderEvidence) -> Result<()> {
    match order {
        InitialCastOrderEvidence::Authored { identity } if *identity != [0; 32] => Ok(()),
        InitialCastOrderEvidence::ObservedLoadedOrder {
            capture_identity,
            loader_identity,
        } if *capture_identity != [0; 32] && *loader_identity != [0; 32] => Ok(()),
        _ => Err(InitError::Invalid("unproven exact initial cast order")),
    }
}
fn hash_order(h: &mut Sha256, order: &StarterCastOrder) {
    for evidence in [&order.creation, &order.first_entry] {
        match evidence {
            InitialCastOrderEvidence::Unknown => h.update([0]),
            InitialCastOrderEvidence::Authored { identity } => {
                h.update([1]);
                h.update(identity);
            }
            InitialCastOrderEvidence::ObservedLoadedOrder {
                capture_identity,
                loader_identity,
            } => {
                h.update([2]);
                h.update(capture_identity);
                h.update(loader_identity);
            }
            InitialCastOrderEvidence::ProvenProjectionEquivalent { proof_identity } => {
                h.update([3]);
                h.update(proof_identity);
            }
        }
    }
    if let Some(ids) = &order.entry_skill_order {
        h.update([1]);
        h.update((ids.len() as u64).to_le_bytes());
        for id in ids {
            h.update(id.to_le_bytes());
        }
    } else {
        h.update([0]);
    }
}
fn hash_lookup<T>(h: &mut Sha256, lookup: &InitialSpellLookup<T>, words: impl Fn(&T) -> [u32; 4]) {
    match lookup {
        InitialSpellLookup::Unknown => h.update([0]),
        InitialSpellLookup::Missing { index } => {
            h.update([1]);
            h.update(index.to_le_bytes());
        }
        InitialSpellLookup::Found(row) => {
            h.update([2]);
            for word in words(row) {
                h.update(word.to_le_bytes());
            }
        }
    }
}
impl StarterInitialDependency {
    pub fn identity(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(b"cmangos-8ec338a1-starter-numeric-dependency-v1\0");
        h.update(self.spell_id.to_le_bytes());
        h.update(self.source_hash);
        hash_lookup(&mut h, &self.cast_time, |r| {
            [
                r.id,
                r.base_ms as u32,
                r.per_level_ms as u32,
                r.minimum_ms as u32,
            ]
        });
        hash_lookup(&mut h, &self.duration, |r| {
            [
                r.id,
                r.base_ms as u32,
                r.per_level_ms as u32,
                r.maximum_ms as u32,
            ]
        });
        for word in [self.server_side, self.server_attributes]
            .into_iter()
            .chain(self.bonus_coefficients)
            .chain(self.attack_power_coefficients)
        {
            h.update(word.to_le_bytes());
        }
        h.finalize().into()
    }
    fn resolutions(&self, row: &SpellRow) -> Result<SourcePhaseResolutions> {
        if self.spell_id != row.id || self.source_hash != row.source_hash() {
            return Err(InitError::Invalid("initial dependency source row binding"));
        }
        let w = &row.words;
        // SpellMgr::GetSpellCastTime still queries the index for old-triggered casts.
        let cast_time_ms = match &self.cast_time {
            InitialSpellLookup::Missing { index } if *index == w[18] => 0,
            InitialSpellLookup::Found(r)
                if r.id == w[18] && (r.base_ms, r.per_level_ms, r.minimum_ms) == (0, 0, 0) =>
            {
                0
            }
            _ => return Err(InitError::Invalid("initial indexed cast-time resolution")),
        };
        // Bounded equal base/max tuples exclude duration/rank/combo modifiers.
        let duration_ms = match &self.duration {
            InitialSpellLookup::Missing { index } if *index == w[30] => 0,
            InitialSpellLookup::Found(r)
                if r.id == w[30]
                    && r.per_level_ms == 0
                    && matches!(r.base_ms, -1 | 0)
                    && r.maximum_ms == r.base_ms =>
            {
                r.base_ms
            }
            _ => return Err(InitError::Invalid("initial indexed duration resolution")),
        };
        if row.duration_ms != Some(duration_ms) {
            return Err(InitError::Invalid("initial indexed duration projection"));
        }
        Ok(SourcePhaseResolutions {
            cast_time_index: w[18],
            cast_time_ms,
            duration_index: w[30],
            duration_ms,
        })
    }
}
impl StarterInitialDependencies {
    pub fn identity(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(b"cmangos-8ec338a1-starter-numeric-scope-v1\0");
        h.update(self.source_identity);
        h.update(self.store_identity);
        h.update(self.hook_scope_identity);
        h.update((self.queried_spell_ids.len() as u64).to_le_bytes());
        for id in &self.queried_spell_ids {
            h.update(id.to_le_bytes());
        }
        match &self.rows {
            Rows::Missing => h.update([0]),
            Rows::Complete(rows) => {
                h.update([1]);
                h.update((rows.len() as u64).to_le_bytes());
                for row in rows {
                    h.update(row.identity());
                }
            }
        }
        h.finalize().into()
    }
    fn validate(&self, input: &StarterInput) -> Result<()> {
        if self.source_identity != input.source_identity()
            || self.store_identity == [0; 32]
            || self.hook_scope_identity == [0; 32]
            || self.queried_spell_ids.len() > 256
            || self
                .queried_spell_ids
                .windows(2)
                .any(|ids| ids[0] >= ids[1])
        {
            return Err(InitError::Invalid("initial dependency identity/scope"));
        }
        let rows = self.rows.require("initial numeric dependencies", 0)?;
        if !rows
            .iter()
            .map(|r| r.spell_id)
            .eq(self.queried_spell_ids.iter().copied())
        {
            return Err(InitError::Invalid("initial dependency exact row scope"));
        }
        let mut cast_times = BTreeMap::new();
        let mut durations = BTreeMap::new();
        for dependency in rows {
            let row = input
                .spells
                .iter()
                .find(|r| r.id == dependency.spell_id)
                .ok_or(super::missing(
                    "initial dependency SpellEntry",
                    dependency.spell_id,
                ))?;
            dependency.resolutions(row)?;
            consistent(&mut cast_times, &dependency.cast_time, |r| r.id)?;
            consistent(&mut durations, &dependency.duration, |r| r.id)?;
        }
        Ok(())
    }
}
fn consistent<T: Clone + Eq>(
    store: &mut BTreeMap<u32, Option<T>>,
    lookup: &InitialSpellLookup<T>,
    id: impl Fn(&T) -> u32,
) -> Result<()> {
    let (index, value) = match lookup {
        InitialSpellLookup::Unknown => {
            return Err(InitError::Invalid("unknown initial indexed lookup"));
        }
        InitialSpellLookup::Missing { index } => (*index, None),
        InitialSpellLookup::Found(row) => (id(row), Some(row.clone())),
    };
    match store.entry(index) {
        std::collections::btree_map::Entry::Vacant(entry) => {
            entry.insert(value);
        }
        std::collections::btree_map::Entry::Occupied(entry) if entry.get() != &value => {
            return Err(InitError::Invalid("inconsistent initial indexed store"));
        }
        _ => {}
    }
    Ok(())
}

pub(super) struct CastCollector<'a> {
    dependencies: &'a StarterInitialDependencies,
    builder: SourcePlanBuilder,
}
impl<'a> CastCollector<'a> {
    fn new(dependencies: &'a StarterInitialDependencies, context_identity: [u8; 32]) -> Self {
        Self {
            dependencies,
            builder: SourcePlanBuilder::new(context_identity),
        }
    }
    fn finish(self) -> InitialCastPlan {
        self.builder.finish()
    }
}
impl Init<'_> {
    pub(super) fn cast_initial_prepared(&mut self, row: &SpellRow) -> Result<()> {
        self.step()?;
        let collector = self
            .initial_casts
            .as_mut()
            .ok_or(InitError::Invalid("missing initial collector"))?;
        let dependency = collector
            .dependencies
            .rows
            .require("initial numeric dependencies", row.id)?
            .iter()
            .find(|r| r.spell_id == row.id)
            .ok_or(super::missing("initial numeric dependency", row.id))?;
        let effects = collector.builder.append(
            row,
            SourcePhaseFields {
                server_side: dependency.server_side,
                server_attributes: dependency.server_attributes,
                bonus_coefficients: dependency.bonus_coefficients,
                attack_power_coefficients: dependency.attack_power_coefficients,
            },
            dependency.resolutions(row)?,
            dependency.identity(),
        )?;
        self.trace.push(InitEvent::InitialCast { spell: row.id });
        for effect in effects
            .iter()
            .filter(|e| e.phase == InitialEffectPhase::Targetless)
        {
            match effect.amount {
                Some(InitialAmountOperation::PendingRandomNoOp { low, high }) => {
                    // The sealed classifier proves exact Dodge's amount has no state consumer.
                    self.trace.push(InitEvent::PendingInitialAmount {
                        spell: row.id,
                        index: effect.slot,
                        low,
                        high,
                    });
                    continue;
                }
                Some(InitialAmountOperation::Constant { .. }) => {}
                None => return Err(InitError::Invalid("scheduled initial amount missing")),
            }
            if effect.opcode == 60 {
                let mask = if row.words[58] == 2 {
                    &mut self.weapon_proficiency
                } else {
                    &mut self.armor_proficiency
                };
                if *mask & row.words[59] == 0 {
                    *mask |= row.words[59];
                }
            }
            self.trace.push(InitEvent::EffectApplied {
                spell: row.id,
                index: effect.slot,
                effect: effect.opcode,
                aura: effect.aura,
            });
        }
        // Register every aura in the holder before applying any modifier.
        let mut holder = vec![];
        for effect in effects
            .iter()
            .filter(|e| e.phase == InitialEffectPhase::SelfHolder)
        {
            let Some(InitialAmountOperation::Constant { value: amount }) = effect.amount else {
                return Err(InitError::Invalid("nonconstant initial holder"));
            };
            if !self.applied_auras.insert((effect.aura, effect.misc)) {
                return Err(InitError::Invalid("initial aura stacking"));
            }
            let aura = InitialAura {
                spell_id: row.id,
                effect_index: effect.slot,
                aura_type: effect.aura,
                misc_value: effect.misc,
                amount,
            };
            self.initial_auras.push(aura);
            holder.push(aura);
        }
        for aura in holder {
            self.apply_aura_modifier(row, aura.aura_type, aura.misc_value, aura.amount)?;
            self.trace.push(InitEvent::EffectApplied {
                spell: row.id,
                index: aura.effect_index,
                effect: 6,
                aura: aura.aura_type,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
