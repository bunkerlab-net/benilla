// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, Player.cpp, Spell.cpp, SpellMgr.cpp and SpellAuras.cpp.
//! Bounded source closure for the complete ordered recursive initial-cast trace.
//! Caller-supplied resolutions are evidence inputs, never a capture certificate.
use super::*;

const INITIAL_CAST_SOURCE_POLICY: &str = "cmangos-8ec338a1-initial-cast-source-v1";
const MAX_INITIAL_SPELLS: usize = 256;
const MAX_SOURCE_ROWS: usize = 4096;

/// An actual indexed store lookup. Missing means the complete queried store had
/// no row at this exact index; neither index0 nor index1 implies absence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum InitialSpellLookup<T> {
    Unknown,
    Missing { index: u32 },
    Found(T),
}
/// Complete source dependencies for one actual initial cast. Spell-script and
/// other modifier hooks are supplied by the catalog's exact queried-ID union.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InitialCastDependencies {
    pub spell_id: u32,
    pub cast_time: InitialSpellLookup<SpellCastTimeSource>,
    pub duration: InitialSpellLookup<SpellDurationSource>,
    pub aura_script_names: SpellSourceRows<String>,
    pub server_side: u32,
    pub server_attributes: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(super) struct InitialCastProof {
    policy: &'static str,
    // No deserializer: only initialization retains the complete immutable input.
    #[serde(serialize_with = "serialize_input")]
    input: init::StarterInput,
    source_identity: [u8; 32],
    cast_trace: Vec<u32>,
    source_rows: Vec<[u8; 32]>,
    dependencies: SpellSourceRows<InitialCastDependencies>,
}
fn serialize_input<S: serde::Serializer>(
    input: &init::StarterInput,
    s: S,
) -> Result<S::Ok, S::Error> {
    input.source_identity().serialize(s)
}
fn rows_len<T>(rows: &init::Rows<T>) -> usize {
    match rows {
        init::Rows::Missing => 0,
        init::Rows::Complete(rows) => rows.len(),
    }
}
fn trace(mage: &init::InitializedMage) -> Vec<u32> {
    mage.trace
        .iter()
        .filter_map(|event| match event {
            init::InitEvent::InitialCast { spell } => Some(*spell),
            _ => None,
        })
        .collect()
}
impl InitialCastProof {
    pub(super) fn check_bound(input: &init::StarterInput) -> Result<(), NativeError> {
        // Checked before cloning/replay. SpellEntry rows, auxiliary records and
        // text are bounded independently of the initializer's execution budget.
        let row_counts = [
            input.spells.len(),
            input.items.len(),
            input.skill_lines.len(),
            input.skill_tiers.len(),
            input.active_support.len(),
            rows_len(&input.default_skills),
            rows_len(&input.default_spells),
            rows_len(&input.skill_race_class),
            rows_len(&input.skill_abilities),
            rows_len(&input.item_grants),
        ];
        let mut count = row_counts
            .into_iter()
            .try_fold(0usize, usize::checked_add)
            .ok_or(NativeError::TooLarge)?;
        for row in &input.spells {
            count = count
                .checked_add(rows_len(&row.learned_spells))
                .ok_or(NativeError::TooLarge)?;
        }
        let text = input
            .active_support
            .iter()
            .try_fold(0usize, |sum, row| {
                sum.checked_add(row.unavailable_reason.as_ref().map_or(0, String::len))
            })
            .ok_or(NativeError::TooLarge)?;
        if input.spells.len() > MAX_INITIAL_SPELLS || count > MAX_SOURCE_ROWS || text > 64 * 1024 {
            return Err(NativeError::TooLarge);
        }
        Ok(())
    }
    pub(super) fn new(
        input: &init::StarterInput,
        mage: &init::InitializedMage,
        dependencies: SpellSourceRows<InitialCastDependencies>,
    ) -> Result<Self, NativeError> {
        Self::check_bound(input)?;
        dependencies.complete("initial cast dependency scope")?;
        let cast_trace = trace(mage);
        if cast_trace.len() > MAX_SOURCE_ROWS {
            return Err(NativeError::TooLarge);
        }
        Ok(Self {
            policy: INITIAL_CAST_SOURCE_POLICY,
            input: input.clone(),
            source_identity: input.source_identity(),
            cast_trace,
            source_rows: input
                .spells
                .iter()
                .map(init::SpellRow::source_hash)
                .collect(),
            dependencies,
        })
    }
    pub(super) fn cast_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.cast_trace.iter().copied()
    }
    pub(super) fn raw_rows(&self) -> impl Iterator<Item = &[u32; 173]> {
        self.input.spells.iter().map(|row| &row.words)
    }
    pub(super) fn validate<'a>(
        &self,
        profile: &Profile,
        hooks: impl IntoIterator<Item = &'a SpellHookSources>,
        active_spells: impl IntoIterator<Item = &'a SpellAuthoritySource>,
    ) -> Result<(), NativeError> {
        Self::check_bound(&self.input)?;
        if self.policy != INITIAL_CAST_SOURCE_POLICY
            || self.source_identity != self.input.source_identity()
            || self.source_rows
                != self
                    .input
                    .spells
                    .iter()
                    .map(init::SpellRow::source_hash)
                    .collect::<Vec<_>>()
            || profile
                .quest_progression
                .as_ref()
                .is_some_and(|q| q.initial_training_identity() != self.source_identity)
        {
            return Err(NativeError::InvalidProfile);
        }
        let mage = init::initialize_mage(&self.input)?;
        if self.cast_trace != trace(&mage) {
            return Err(NativeError::InvalidProfile);
        }
        let combat = profile.combat.as_ref().ok_or(NativeError::InvalidProfile)?;
        if mage.initial_auras.len() != combat.initial_auras.len()
            || mage
                .initial_auras
                .iter()
                .zip(&combat.initial_auras)
                .any(|(a, b)| {
                    a.spell_id != b.spell_id
                        || a.effect_index != b.effect_index
                        || a.aura_type != b.aura_type
                        || a.misc_value != b.misc_value
                        || a.amount != b.amount
                })
            || mage.stat_recipe.snapshot() != combat.recipe.snapshot()
            || (mage.weapon_proficiency, mage.armor_proficiency) != combat.proficiencies
        {
            return Err(NativeError::InvalidProfile);
        }
        let ids = self.cast_ids().collect::<BTreeSet<_>>();
        self.dependencies
            .complete("initial cast dependency scope")?;
        let SpellSourceRows::Complete(dependencies) = &self.dependencies else {
            unreachable!()
        };
        if !dependencies
            .iter()
            .map(|row| row.spell_id)
            .eq(ids.iter().copied())
        {
            return Err(NativeError::AuthorityBinding);
        }
        // All spells resolve against the same indexed DBC stores. Equal
        // derived values cannot reconcile Missing with an observed Found row.
        let mut cast_times = BTreeMap::new();
        let mut durations = BTreeMap::new();
        for row in dependencies {
            match row.cast_time {
                InitialSpellLookup::Unknown => {
                    return Err(NativeError::AuthorityUnavailable(
                        "initial cast-time lookup",
                    ));
                }
                InitialSpellLookup::Missing { index } => {
                    consistent_index(&mut cast_times, index, None)?;
                }
                InitialSpellLookup::Found(raw) => {
                    consistent_index(&mut cast_times, raw.id, Some(raw))?;
                }
            }
            match row.duration {
                InitialSpellLookup::Unknown => {
                    return Err(NativeError::AuthorityUnavailable("initial duration lookup"));
                }
                InitialSpellLookup::Missing { index } => {
                    consistent_index(&mut durations, index, None)?;
                }
                InitialSpellLookup::Found(raw) => {
                    consistent_index(&mut durations, raw.id, Some(raw))?;
                }
            }
        }
        for row in active_spells {
            consistent_index(&mut cast_times, row.cast_time.id, Some(row.cast_time))?;
            // None carries no resolved duration row, not a proven missing lookup.
            if let Some(raw) = row.duration {
                consistent_index(&mut durations, raw.id, Some(raw))?;
            }
        }
        for hook in hooks {
            let SpellSourceRows::Complete(ranks) = &hook.spell_rank_chain else {
                return Err(NativeError::AuthorityUnavailable(
                    "initial cast rank closure",
                ));
            };
            // No applicable chain relation may contradict the initializer's
            // previous_rank=None or enter passive cross-rank holder comparison.
            if ranks.iter().any(|rank| {
                [
                    rank.spell_id,
                    rank.previous_spell,
                    rank.first_spell,
                    rank.required_spell,
                ]
                .iter()
                .any(|id| ids.contains(id))
            }) {
                return Err(NativeError::UnsupportedMechanic(
                    "initial cast rank relation",
                ));
            }
        }
        for &id in &self.cast_trace {
            let row = self
                .input
                .spells
                .iter()
                .find(|row| row.id == id)
                .ok_or(NativeError::InvalidProfile)?;
            let dependency = dependencies
                .iter()
                .find(|row| row.spell_id == id)
                .ok_or(NativeError::AuthorityBinding)?;
            validate_cast(row, dependency)?;
        }
        // A future no-op/nonholder cast is observable even if its final stats
        // are unchanged. Native forbids known-spell active-state transitions,
        // removal and disabling; every possible cast-on-learning must already
        // have executed in this exact genesis source pool.
        for row in &self.input.spells {
            let w = &row.words;
            let potential = w[7] & 0x8000_0000 != 0
                || w[6] & 0x40 != 0
                || w[61..64].contains(&44)
                || row.talent;
            if potential
                && (!ids.contains(&row.id)
                    || mage
                        .abilities
                        .get(&row.id)
                        .is_none_or(|ability| !ability.active))
            {
                return Err(NativeError::UnsupportedMechanic(
                    "future initial-cast source closure",
                ));
            }
        }
        Ok(())
    }
}
fn consistent_index<T: Eq>(
    store: &mut BTreeMap<u32, Option<T>>,
    index: u32,
    value: Option<T>,
) -> Result<(), NativeError> {
    match store.entry(index) {
        std::collections::btree_map::Entry::Vacant(entry) => {
            entry.insert(value);
        }
        std::collections::btree_map::Entry::Occupied(entry) if entry.get() != &value => {
            return Err(NativeError::AuthorityBinding);
        }
        std::collections::btree_map::Entry::Occupied(_) => {}
    }
    Ok(())
}
fn validate_cast(
    row: &init::SpellRow,
    dependency: &InitialCastDependencies,
) -> Result<(), NativeError> {
    let w = &row.words;
    let (aura, misc) = match row.id {
        20598 => (137, 4),
        20599 => (156, 0),
        60100 => (29, 3), // Authored test ID, never a production admission.
        _ => {
            return Err(NativeError::UnsupportedMechanic(
                "unaudited initial cast ID",
            ));
        }
    };
    if w[0] != row.id
        || row.talent
        || row.previous_rank.is_some()
        || row.external_initial_hooks
        || w[1] > 6
        || w[6] & 0x40 == 0
        || w[6] & !0x8000_01d0 != 0
        || w[7] & !0x9000_0400 != 0
        || w[8] & !0x0008_0000 != 0
        || w[58] != u32::MAX
        || w[61] != 6
        || !matches!(w[64], 0 | 1)
        || w[82] != 1
        || w[91] != aura
        || w[106] != misc
        || dependency.spell_id != row.id
        || dependency.server_side != 0
        || dependency.server_attributes != 0
    {
        return Err(NativeError::UnsupportedMechanic(
            "audited initial cast shape",
        ));
    }
    // Every remaining semantic cell is zero, including all inactive targets,
    // Mechanics, cooldown/proc/charge/stack/trigger/periodic fields and speed.
    // Only visual/text words and the explicitly checked numeric inputs vary.
    for (index, word) in w.iter().enumerate() {
        if !matches!(index, 0 | 1 | 3 | 6 | 7 | 8 | 18 | 27..=30 | 38 | 58 | 61 | 64 | 67 | 76 | 82 | 91 | 106 | 115..=155 | 166)
            && *word != 0
        {
            return Err(NativeError::UnsupportedMechanic(
                "initial cast unproved source field",
            ));
        }
    }
    dependency
        .aura_script_names
        .empty("initial aura script source closure")?;
    match &dependency.cast_time {
        InitialSpellLookup::Unknown => {
            return Err(NativeError::AuthorityUnavailable(
                "initial cast-time lookup",
            ));
        }
        InitialSpellLookup::Missing { index } if *index == w[18] => {}
        InitialSpellLookup::Found(raw)
            if raw.id == w[18]
                && raw.base_ms == 0
                && raw.per_level_ms == 0
                && raw.minimum_ms == 0 => {}
        _ => {
            return Err(NativeError::UnsupportedMechanic(
                "initial cast-time resolution",
            ));
        }
    }
    // SpellMgr.cpp:56: missing=>0, raw -1=>-1, otherwise abs(base). The
    // bounded tuples make maxduration equal duration, excluding combo scaling;
    // explicit absent aura scripts and neutral holders close duration modifiers.
    let duration = match &dependency.duration {
        InitialSpellLookup::Unknown => {
            return Err(NativeError::AuthorityUnavailable("initial duration lookup"));
        }
        InitialSpellLookup::Missing { index } if *index == w[30] => 0,
        InitialSpellLookup::Found(raw)
            if raw.id == w[30]
                && raw.per_level_ms == 0
                && matches!(raw.base_ms, -1 | 0)
                && raw.maximum_ms == raw.base_ms =>
        {
            raw.base_ms
        }
        _ => {
            return Err(NativeError::UnsupportedMechanic(
                "initial duration resolution",
            ));
        }
    };
    if row.duration_ms != Some(duration) || !(duration == -1 || (duration == 0 && w[30] == 0)) {
        return Err(NativeError::UnsupportedMechanic(
            "initial permanent passive duration",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diplomacy_holder_shape_and_amount_are_source_bound_before_consumer_binding() {
        let mut words = [0; 173];
        words[0] = 20599;
        words[6] = 0x40;
        words[58] = u32::MAX;
        words[61] = 6;
        words[67] = 1;
        words[76] = 4;
        words[82] = 1;
        words[91] = 156;
        let row = init::SpellRow {
            id: 20599,
            words,
            duration_ms: Some(0),
            previous_rank: None,
            talent: false,
            learned_spells: init::Rows::Complete(vec![]),
            external_initial_hooks: false,
        };
        let dependency = InitialCastDependencies {
            spell_id: 20599,
            cast_time: InitialSpellLookup::Missing { index: 0 },
            duration: InitialSpellLookup::Missing { index: 0 },
            aura_script_names: SpellSourceRows::Complete(vec![]),
            server_side: 0,
            server_attributes: 0,
        };
        validate_cast(&row, &dependency).unwrap();
        let (_, mut initial, _) = crate::native::synthetic_spell_fixture::inputs();
        let init::Rows::Complete(defaults) = &mut initial.default_spells else {
            unreachable!()
        };
        defaults.push(row.id);
        initial.spells.push(row.clone());
        let mage = init::initialize_mage(&initial).unwrap();
        let holder = mage
            .initial_auras
            .iter()
            .find(|aura| aura.spell_id == row.id)
            .unwrap();
        assert_eq!(
            (holder.aura_type, holder.misc_value, holder.amount),
            (156, 0, 5)
        );
        let mut wrong = row;
        wrong.words[91] = 137;
        assert!(validate_cast(&wrong, &dependency).is_err());
    }
}
