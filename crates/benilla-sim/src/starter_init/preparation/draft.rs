// SPDX-License-Identifier: GPL-3.0-or-later
//! Conditional numeric execution, never hook absence or a prepared source seal.
use super::*;

/// The numeric reducers ran, but their outputs remain conditional on reached hooks.
/// This type deliberately has no conversion to PreparedStarter or a runtime profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnsealedStarterDraft {
    created: InitializedMage,
    entry: FirstEntryState,
    creation_plan: InitialCastPlan,
    entry_plan: InitialCastPlan,
    dependencies: Vec<StarterInitialDependency>,
    pending_learning_hooks: Vec<u32>,
}
impl UnsealedStarterDraft {
    pub fn projected_creation(&self) -> &InitializedMage {
        &self.created
    }
    pub fn projected_first_entry(&self) -> &FirstEntryState {
        &self.entry
    }
    /// Evaluation refuses these plans until a caller rebuilds with checked hooks.
    pub fn creation_plan(&self) -> &InitialCastPlan {
        &self.creation_plan
    }
    pub fn first_entry_plan(&self) -> &InitialCastPlan {
        &self.entry_plan
    }
    /// Exact sorted reached union, not every selected numeric row.
    pub fn reached_dependencies(&self) -> &[StarterInitialDependency] {
        &self.dependencies
    }
    /// addSpell was reached, including roots that did not cast.
    pub fn pending_learning_hooks(&self) -> &[u32] {
        &self.pending_learning_hooks
    }
}

/// Execute the existing reducers with explicit deferred hooks and unsampled RNG.
/// Numeric rows may cover the selected superset; only actually reached casts query
/// their indexed dependencies. Unknown/unsupported reached operands still refuse.
pub fn draft_starter(
    input: &StarterInput,
    store_identity: [u8; 32],
    numeric: Vec<StarterInitialDependency>,
) -> Result<UnsealedStarterDraft> {
    if store_identity == [0; 32]
        || input.spells.iter().any(|row| !row.external_initial_hooks)
        || numeric.len() > 256
        || numeric
            .windows(2)
            .any(|pair| pair[0].spell_id >= pair[1].spell_id)
    {
        return Err(InitError::Invalid("draft unresolved hook/numeric scope"));
    }
    if input
        .default_spells
        .require("playercreateinfo_spell", 0)?
        .windows(2)
        .any(|pair| pair[0] > pair[1])
    {
        return Err(InitError::Invalid("noncanonical default spell order"));
    }
    let dependencies = StarterInitialDependencies {
        source_identity: input.source_identity(),
        store_identity,
        // Deliberately unresolved. This envelope cannot pass prepare_starter.
        hook_scope_identity: [0; 32],
        queried_spell_ids: numeric.iter().map(|row| row.spell_id).collect(),
        rows: Rows::Complete(numeric),
    };
    let collector = |epoch: u8| {
        let mut h = Sha256::new();
        h.update(b"cmangos-8ec338a1-unsealed-starter-draft-v1\0");
        h.update([epoch]);
        h.update(dependencies.identity());
        h.update(crate::canonical_order::identity());
        CastCollector {
            dependencies: &dependencies,
            builder: SourcePlanBuilder::new(h.finalize().into()),
            hooks: HookDisposition::PendingReachedBinding,
        }
    };
    let (created, creation) = super::super::initialize_mage_inner(input, Some(collector(0)))?;
    let (entry, first_entry) =
        super::super::skills::first_entry_inner(input, &created, Some(collector(1)), None)?;
    let creation_plan = creation
        .ok_or(InitError::Invalid("missing draft creation collector"))?
        .finish();
    let entry_plan = first_entry
        .ok_or(InitError::Invalid("missing draft entry collector"))?
        .finish();
    let reached: BTreeSet<_> = creation_plan
        .occurrences()
        .iter()
        .chain(entry_plan.occurrences())
        .map(|cast| cast.row().id)
        .collect();
    let Rows::Complete(rows) = dependencies.rows else {
        unreachable!()
    };
    let dependencies: Vec<_> = rows
        .into_iter()
        .filter(|row| reached.contains(&row.spell_id))
        .collect();
    let mut cast_times = BTreeMap::new();
    let mut durations = BTreeMap::new();
    for dependency in &dependencies {
        consistent(&mut cast_times, &dependency.cast_time, |row| row.id)?;
        consistent(&mut durations, &dependency.duration, |row| row.id)?;
    }
    let pending_learning_hooks = created.abilities.keys().copied().collect();
    Ok(UnsealedStarterDraft {
        created,
        entry,
        creation_plan,
        entry_plan,
        dependencies,
        pending_learning_hooks,
    })
}
