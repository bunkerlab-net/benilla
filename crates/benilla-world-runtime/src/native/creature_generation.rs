// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Creature.cpp:
// 338-366/506-538/636-654/1265-1462/1690-1693/1866-1873/1912-1945.
// Upstream GPL-2.0-or-later; Rust translation GPL-3.0-or-later. No warranty.
//! Source-bound ordinary DB-spawn lives. Retained inputs are a capture contract,
//! not authenticated database origin or native admission. No independent RNG exists.
use super::*;
use benilla_sim::{creature_init as init, creature_regeneration as regeneration};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const CREATURE_GENERATION_POLICY: &str = "cmangos-8ec338a1-native-creature-generation-v4";
pub const MAX_CREATURE_GENERATIONS: usize = 256;
const MAX_CORPSE_TIMER_CALLS: usize = 256;
const MAX_SOURCES: usize = 4096;
const MAX_SOURCE_ROWS: usize = 65_536;
const MAX_SOURCE_BYTES: usize = 16 * 1024 * 1024;
const MAX_OWNER_BYTES: usize = 256 * 1024;
/// The initial legacy cap multiplies uint32 by nine before division. Overflow is
/// outside this host; the raw database endpoints remain distinct, including equality.
const MAX_RESPAWN_SECONDS: u32 = u32::MAX / 9;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CreatureGenerationSource {
    pub spawn_id: u64,
    pub spawn_entry: u32,
    pub requested_spawn_template_entry: u32,
    pub respawn_delay_min_seconds: u32,
    pub respawn_delay_max_seconds: u32,
    pub input: init::Input,
}
impl CreatureGenerationSource {
    pub fn identity(&self) -> crate::Digest {
        crate::digest(
            b"benilla-native-creature-generation-source-v4\0",
            &(
                CREATURE_GENERATION_POLICY,
                init::CREATURE_INIT_POLICY_ID,
                self,
            ),
        )
    }
    fn validate(&self) -> Result<(), NativeError> {
        if self.input.spawn_id != self.spawn_id
            || self.spawn_id <= SOLO_UNIT
            || self.spawn_entry == 0
            || self.spawn_entry > 0xff_ffff
            || self.spawn_entry != self.input.template.entry
            || self.respawn_delay_min_seconds > self.respawn_delay_max_seconds
            || self.respawn_delay_max_seconds > MAX_RESPAWN_SECONDS
        {
            return Err(NativeError::InvalidProfile);
        }
        let [spawn] = rows(&self.input.spawn_template)? else {
            return Err(NativeError::InvalidProfile);
        };
        if spawn.entry != self.requested_spawn_template_entry {
            return Err(NativeError::UnsupportedMechanic(
                "creature generation spawn-template substitution",
            ));
        }
        // JUST_ALIVED restores template NPC flags after UpdateEntry. A differing
        // spawn override needs a separate post-initialization flag transition.
        if spawn.npc_flags != -1 && spawn.npc_flags as u32 != self.input.template.npc_flags {
            return Err(NativeError::UnsupportedMechanic(
                "respawn NPC-flag override",
            ));
        }
        // prepare validates every reachable stat and model row before any request.
        let plan = init::prepare(&self.input)?;
        let model = plan.model_roll_range().map(|range| range.0);
        let gender = plan.gender_roll_range(model)?.map(|range| range.1);
        let levels = self.input.template.minimum_level..=self.input.template.maximum_level;
        for level in levels {
            let result = init::prepare(&self.input)?.resolve(init::SelectionRolls {
                model,
                gender,
                level: plan.level_roll_range().map(|_| u32::from(level)),
            })?;
            validate_projection(&self.input, &result)?;
        }
        // Model-dependent host scalars must also succeed for every reachable
        // alternative. A canonical low endpoint represents each weighted bucket.
        let mut model_rolls = vec![model];
        if plan.model_roll_range().is_some() {
            model_rolls.clear();
            let mut first = 0_u32;
            for display in self.input.template.displays {
                if display.id != 0 && display.probability != 0 {
                    model_rolls.push(Some(first));
                    first = first
                        .checked_add(display.probability)
                        .ok_or(NativeError::Limit)?;
                }
            }
        }
        for model in model_rolls {
            let genders = if plan.gender_roll_range(model)?.is_some() {
                vec![Some(0), Some(1)]
            } else {
                vec![None]
            };
            for gender in genders {
                let result = init::prepare(&self.input)?.resolve(init::SelectionRolls {
                    model,
                    gender,
                    level: plan.level_roll_range().map(|range| range.0),
                })?;
                validate_projection(&self.input, &result)?;
            }
        }
        Ok(())
    }
}

/// An authored scheduler for this synthetic component host. Player first-entry
/// draws precede these spawn keys; this is not universal CMaNGOS map-load order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum CreatureColdEntrySchedule {
    SyntheticPlayerThenAuthoredSpawns(Vec<u64>),
}

/// Raw source ownership cannot be forged through a deserialized arithmetic result.
/// ```compile_fail
/// use benilla_world_runtime::native::CreatureGenerationCatalog;
/// let _: CreatureGenerationCatalog = serde_json::from_str("{}").unwrap();
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CreatureGenerationCatalog {
    policy: &'static str,
    initialization_policy: &'static str,
    schedule: CreatureColdEntrySchedule,
    spawns: BTreeMap<u64, CreatureGenerationSource>,
}
impl CreatureGenerationCatalog {
    pub fn validate_sources(
        sources: init::Rows<CreatureGenerationSource>,
        cold_entry_order: Vec<u64>,
    ) -> Result<Self, NativeError> {
        let sources = rows(&sources)?;
        if sources.len() > MAX_SOURCES || cold_entry_order.len() != sources.len() {
            return Err(NativeError::TooLarge);
        }
        let mut row_count = 0_usize;
        for source in sources {
            source.input.validate_retention_bounds()?;
            for count in [
                rows(&source.input.class_levels)?.len(),
                rows(&source.input.models)?.len(),
                rows(&source.input.displays)?.len(),
                rows(&source.input.spawn_template)?.len(),
                rows(&source.input.external_features)?.len(),
                rows(&source.input.equipment_templates)?.len(),
                rows(&source.input.item_templates)?.len(),
                rows(&source.input.spawn_addons)?.len(),
                rows(&source.input.template_addons)?.len(),
            ] {
                row_count = row_count.checked_add(count).ok_or(NativeError::TooLarge)?;
            }
            if row_count > MAX_SOURCE_ROWS {
                return Err(NativeError::TooLarge);
            }
        }
        bounded_encoded_size(&sources, MAX_SOURCE_BYTES)?;
        let mut spawns = BTreeMap::new();
        let mut templates = BTreeMap::new();
        let mut levels = BTreeMap::new();
        let mut models = BTreeMap::new();
        let mut displays = BTreeMap::new();
        let mut spawn_templates = BTreeMap::new();
        let mut equipment_templates = BTreeMap::new();
        let mut item_templates = BTreeMap::new();
        let mut template_addons = BTreeMap::new();
        let expected_rates = sources.first().map(|source| source.input.rates);
        for source in sources {
            source.validate()?;
            let input = &source.input;
            if Some(input.rates) != expected_rates {
                return Err(NativeError::InvalidProfile);
            }
            consistent(&mut templates, input.template.entry, &input.template)?;
            // A complete empty query and a present row cannot both describe the
            // same template. Compare query closure, not only overlapping rows.
            consistent(
                &mut template_addons,
                input.template.entry,
                &input.template_addons,
            )?;
            for row in rows(&input.class_levels)? {
                consistent(&mut levels, (row.class, row.level), row)?;
            }
            for row in rows(&input.models)? {
                consistent(&mut models, row.id, row)?;
            }
            for row in rows(&input.displays)? {
                consistent(&mut displays, row.id, row)?;
            }
            for row in rows(&input.spawn_template)? {
                consistent(&mut spawn_templates, row.entry, row)?;
            }
            for row in rows(&input.equipment_templates)? {
                consistent(&mut equipment_templates, row.entry, row)?;
            }
            for row in rows(&input.item_templates)? {
                consistent(&mut item_templates, row.entry, row)?;
            }
            if spawns.insert(source.spawn_id, source.clone()).is_some() {
                return Err(NativeError::InvalidProfile);
            }
        }
        let selected: BTreeSet<_> = cold_entry_order.iter().copied().collect();
        if selected.len() != cold_entry_order.len() || selected.iter().ne(spawns.keys()) {
            return Err(NativeError::InvalidProfile);
        }
        Ok(Self {
            policy: CREATURE_GENERATION_POLICY,
            initialization_policy: init::CREATURE_INIT_POLICY_ID,
            schedule: CreatureColdEntrySchedule::SyntheticPlayerThenAuthoredSpawns(
                cold_entry_order,
            ),
            spawns,
        })
    }
    pub fn source(&self, spawn: u64) -> Option<&CreatureGenerationSource> {
        self.spawns.get(&spawn)
    }
    pub fn cold_entry_order(&self) -> &[u64] {
        match &self.schedule {
            CreatureColdEntrySchedule::SyntheticPlayerThenAuthoredSpawns(order) => order,
        }
    }
    pub fn schedule(&self) -> &CreatureColdEntrySchedule {
        &self.schedule
    }
    pub(super) fn validate(&self, profile: &Profile) -> Result<(), NativeError> {
        if self.policy != CREATURE_GENERATION_POLICY
            || self.initialization_policy != init::CREATURE_INIT_POLICY_ID
            || self.spawns.len() != profile.npcs.len()
        {
            return Err(NativeError::InvalidProfile);
        }
        for npc in &profile.npcs {
            let source = self
                .source(npc.ai.creature_id)
                .ok_or(NativeError::InvalidProfile)?;
            let [spawn] = rows(&source.input.spawn_template)? else {
                return Err(NativeError::InvalidProfile);
            };
            creature_equipment::validate_item_overlaps(profile, &source.input)?;
            let template = &source.input.template;
            // CreatureAI.cpp:39-40 derives the initial reaction from ExtraFlags.
            // Other controls require their own owner, even without combat callbacks.
            let has_combat_authority = profile
                .interactions
                .as_ref()
                .is_some_and(|catalog| catalog.has_combat_authority());
            let owned_extra_flags = 0x2 | if has_combat_authority { 0x0008_0000 } else { 0 };
            if template.extra_flags & !owned_extra_flags != 0 {
                return Err(NativeError::UnsupportedMechanic(
                    "unowned creature generation extra flags",
                ));
            }
            let reaction = if template.extra_flags & 0x2 != 0 {
                ai::Reaction::Defensive
            } else {
                ai::Reaction::Aggressive
            };
            let faction = if spawn.faction == 0 {
                template.faction
            } else {
                spawn.faction
            };
            let unit_flags = if spawn.unit_flags == -1 {
                template.unit_flags
            } else {
                spawn.unit_flags as u32
            };
            if source.spawn_entry != npc.entry
                || npc.ai.reaction != reaction
                || faction != npc.faction_template
                || npc.health_regeneration.regenerate_stats != template.regenerate_stats
                || npc.health_regeneration.creature_type_flags != template.type_flags
                || npc.health_regeneration.unit_flags != unit_flags
                || npc.combat.as_ref().is_some_and(|combat| {
                    combat.no_crushing_blows != (template.static_flags[1] & 0x10 != 0)
                        || combat.creature_is_critter != (template.creature_type == 8)
                })
            {
                return Err(NativeError::InvalidProfile);
            }
        }
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn initial(
        &self,
        world: crate::WorldId,
        profile: crate::Digest,
        npc: crate::NpcId,
        seed: u64,
        cursor: &mut u64,
    ) -> Result<(CreatureGenerationOwner, Vec<Draw>), NativeError> {
        if npc.generation != 0 {
            return Err(NativeError::InvalidState);
        }
        let source = self.source(npc.spawn).ok_or(NativeError::InvalidProfile)?;
        let mut next_counter = *cursor;
        let generation = realize(
            source,
            world,
            npc,
            0,
            seed,
            &mut next_counter,
            None,
            None,
            &mut creature_initialization::unavailable_initialization,
        )?;
        let draws = generation.history.draws.clone();
        let owner = CreatureGenerationOwner {
            policy: CREATURE_GENERATION_POLICY,
            world,
            profile,
            source: source.identity(),
            spawn: npc.spawn,
            generations: vec![generation],
        };
        owner.checkpoint().check_budget()?;
        *cursor = next_counter;
        Ok((owner, draws))
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn initial_with_initialization(
        &self,
        profile: &Profile,
        world: crate::WorldId,
        npc: crate::NpcId,
        seed: u64,
        cursor: &mut u64,
        observe: &mut dyn FnMut(
            &CreatureInitializationRequest,
        ) -> Result<CreatureInitializationFacts, NativeError>,
    ) -> Result<(CreatureGenerationOwner, Vec<Draw>), NativeError> {
        if npc.generation != 0 {
            return Err(NativeError::InvalidState);
        }
        let source = self.source(npc.spawn).ok_or(NativeError::InvalidProfile)?;
        let mut next_counter = *cursor;
        let generation = realize(
            source,
            world,
            npc,
            0,
            seed,
            &mut next_counter,
            None,
            Some(profile),
            observe,
        )?;
        let draws = generation.history.draws.clone();
        let owner = CreatureGenerationOwner {
            policy: CREATURE_GENERATION_POLICY,
            world,
            profile: profile.digest(),
            source: source.identity(),
            spawn: npc.spawn,
            generations: vec![generation],
        };
        owner.checkpoint().check_budget()?;
        *cursor = next_counter;
        Ok((owner, draws))
    }
}

fn rows<T>(rows: &init::Rows<T>) -> Result<&[T], NativeError> {
    match rows {
        init::Rows::Missing => Err(NativeError::AuthorityUnavailable(
            "creature generation raw sources",
        )),
        init::Rows::Complete(rows) => Ok(rows),
    }
}
fn consistent<'a, K: Ord, V: PartialEq>(
    entries: &mut BTreeMap<K, &'a V>,
    key: K,
    value: &'a V,
) -> Result<(), NativeError> {
    if entries
        .insert(key, value)
        .is_some_and(|previous| previous != value)
    {
        return Err(NativeError::InvalidProfile);
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeathHistory {
    tick: u64,
    draw: Draw,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GenerationHistory {
    npc: crate::NpcId,
    created_tick: u64,
    first_counter: u64,
    end_counter: u64,
    draws: Vec<Draw>,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    initialization: Option<CreatureInitializationFacts>,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    death: Option<DeathHistory>,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    corpse_removed_tick: Option<u64>,
    corpse_timer_calls: Vec<ai::CorpseTimerCall>,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    corpse_removal_ai: Option<ai::AiCheckpoint>,
}
/// A decoded checkpoint is untrusted compact input, never a generation owner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureGenerationCheckpoint {
    policy: String,
    world: crate::WorldId,
    profile: crate::Digest,
    source: crate::Digest,
    spawn: u64,
    generations: Vec<GenerationHistory>,
}
impl CreatureGenerationCheckpoint {
    /// Cheap structural validation precedes replay. Actual rejected words count
    /// individually, even when a semantic request has equal endpoints.
    pub(super) fn replay_work(&self) -> Result<u64, NativeError> {
        self.check_budget()?;
        self.generations
            .iter()
            .try_fold(self.generations.len() as u64, |total, life| {
                let snapshot_work = life
                    .corpse_removal_ai
                    .as_ref()
                    .map(|snapshot| {
                        bounded_encoded_size(snapshot, MAX_OWNER_BYTES).map(|n| n as u64)
                    })
                    .transpose()?
                    .unwrap_or(0);
                let total = total
                    .checked_add(snapshot_work)
                    .ok_or(NativeError::TooLarge)?;
                life.draws
                    .iter()
                    .chain(life.death.iter().map(|death| &death.draw))
                    .try_fold(
                        total
                            .checked_add(life.corpse_timer_calls.len() as u64)
                            .ok_or(NativeError::TooLarge)?,
                        |total, draw| {
                            total
                                .checked_add(u64::from(draw.consumed))
                                .ok_or(NativeError::TooLarge)
                        },
                    )
            })
    }
    /// Conservative aggregate-work units for the host's pre-replay budget.
    /// Includes source scans/hashing, level/model preparation, receipt lookups,
    /// and each bounded timer/snapshot history once. Reconstruction is per-life.
    pub(super) fn source_replay_work(
        &self,
        catalog: &CreatureGenerationCatalog,
    ) -> Result<u64, NativeError> {
        let base = self.replay_work()?;
        let source = catalog
            .source(self.spawn)
            .ok_or(NativeError::InvalidCheckpoint)?;
        let input = &source.input;
        let models = rows(&input.models)?.len() as u64;
        let displays = rows(&input.displays)?.len() as u64;
        let raw_rows = (rows(&input.class_levels)?.len() as u64)
            .checked_add(models)
            .and_then(|n| n.checked_add(displays))
            .and_then(|n| n.checked_add(rows(&input.spawn_template).ok()?.len() as u64))
            .and_then(|n| n.checked_add(rows(&input.external_features).ok()?.len() as u64))
            .and_then(|n| n.checked_add(rows(&input.equipment_templates).ok()?.len() as u64))
            .and_then(|n| n.checked_add(rows(&input.item_templates).ok()?.len() as u64))
            .and_then(|n| n.checked_add(rows(&input.spawn_addons).ok()?.len() as u64))
            .and_then(|n| n.checked_add(rows(&input.template_addons).ok()?.len() as u64))
            .ok_or(NativeError::TooLarge)?;
        let levels = u64::from(input.template.maximum_level)
            .checked_sub(u64::from(input.template.minimum_level))
            .and_then(|n| n.checked_add(1))
            .ok_or(NativeError::InvalidProfile)?;
        // Four template roots plus one spawn override, each with one gender row.
        let reachable_models = models.min(10);
        let per_life = raw_rows
            .checked_mul(3)
            .and_then(|n| n.checked_add(levels))
            .and_then(|n| {
                n.checked_add(reachable_models.checked_mul(models.checked_add(displays)?)?)
            })
            .and_then(|n| n.checked_add(1))
            .ok_or(NativeError::TooLarge)?;
        let lives = self.generations.len() as u64;
        base.checked_add(lives.checked_mul(per_life).ok_or(NativeError::TooLarge)?)
            .and_then(|n| n.checked_add(lives.checked_mul(6 + reachable_models)?))
            .ok_or(NativeError::TooLarge)
    }
    fn check_budget(&self) -> Result<(), NativeError> {
        if self.policy.len() > 128
            || self.generations.is_empty()
            || self.generations.len() > MAX_CREATURE_GENERATIONS
            || self.generations.iter().any(|life| {
                life.draws.len() > 4
                    || life.corpse_timer_calls.len() > MAX_CORPSE_TIMER_CALLS
                    || life.corpse_removed_tick.is_some() != life.corpse_removal_ai.is_some()
                    || life
                        .draws
                        .iter()
                        .chain(life.death.iter().map(|death| &death.draw))
                        .any(|draw| draw.consumed == 0 || draw.consumed > 64)
            })
        {
            return Err(NativeError::TooLarge);
        }
        bounded_encoded_size(self, MAX_OWNER_BYTES).map(|_| ())
    }
}

/// One immutable selected life, including all initialized stats and its model
/// receipt. Callers can borrow results but cannot install caller-authored results.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CreatureGeneration {
    history: GenerationHistory,
    initialized: init::InitializedCreature,
    model_selection: init::ModelSelectionReceipt,
    regeneration: regeneration::Policy,
    respawn_delay_seconds: u32,
    corpse_delay_seconds: u32,
}
impl CreatureGeneration {
    pub fn initialization(&self) -> Option<&CreatureInitializationFacts> {
        self.history.initialization.as_ref()
    }

    pub fn npc_id(&self) -> crate::NpcId {
        self.history.npc
    }
    pub fn result(&self) -> &init::InitializedCreature {
        &self.initialized
    }
    pub fn model_selection(&self) -> &init::ModelSelectionReceipt {
        &self.model_selection
    }
    pub fn creation_tick(&self) -> u64 {
        self.history.created_tick
    }
    pub fn death_tick(&self) -> Option<u64> {
        self.history.death.as_ref().map(|death| death.tick)
    }
    pub fn corpse_removed_tick(&self) -> Option<u64> {
        self.history.corpse_removed_tick
    }
    pub fn corpse_timer_calls(&self) -> &[ai::CorpseTimerCall] {
        &self.history.corpse_timer_calls
    }
    pub fn creation_first_counter(&self) -> u64 {
        self.history.first_counter
    }
    pub fn creation_end_counter(&self) -> u64 {
        self.history.end_counter
    }
    pub fn creation_draws(&self) -> &[Draw] {
        &self.history.draws
    }
    pub fn death_draw(&self) -> Option<&Draw> {
        self.history.death.as_ref().map(|death| &death.draw)
    }
    pub fn respawn_delay_seconds(&self) -> u32 {
        self.respawn_delay_seconds
    }
    pub fn corpse_delay_seconds(&self) -> u32 {
        self.corpse_delay_seconds
    }
    pub fn project_profile(&self, base: &NpcProfile) -> Result<NpcProfile, NativeError> {
        if base.ai.creature_id != self.history.npc.spawn
            || base.entry != self.initialized.entry
            || base.faction_template != self.initialized.faction
        {
            return Err(NativeError::AuthorityBinding);
        }
        let result = &self.initialized;
        let mut projected = base.clone();
        projected.ai = self.project_ai(&base.ai)?;
        projected.max_health = result.stats.resources.max_health;
        projected.display_id = result.model.display_id;
        projected.health_regeneration = self.regeneration;
        // Presence selects the host's native melee component; absent components
        // do not become supported merely because initialization supplied damage.
        if projected.combat.is_some() {
            projected.combat = Some(CreatureCombatProfile {
                minimum_damage: result.stats.damage.minimum,
                maximum_damage: result.stats.damage.maximum,
                no_crushing_blows: result.no_crushing_blows,
                creature_is_critter: result.creature_is_critter,
            });
        }
        Ok(projected)
    }
    fn project_ai(&self, base: &ai::Profile) -> Result<ai::Profile, NativeError> {
        if base.creature_id != self.npc_id().spawn {
            return Err(NativeError::AuthorityBinding);
        }
        let result = &self.initialized;
        let mut projected = base.clone();
        projected.level = result.level;
        projected.combat_reach = ai::Real::from_bits(result.model.combat_reach.bits())?;
        projected.bounding_radius = ai::Real::from_bits(result.model.bounding_radius.bits())?;
        projected.run_speed = ai::Real::from_bits(result.model.run_speed.bits())?;
        projected.home_speed = projected.run_speed;
        projected.mainhand_interval_ms = result.mainhand_interval_ms;
        projected.corpse_decay_ms = self
            .corpse_delay_seconds
            .checked_mul(1000)
            .ok_or(NativeError::Limit)?;
        projected.respawn_delay_seconds = self.respawn_delay_seconds;
        Ok(projected)
    }
}

/// The profile/world/source binding and all retained lives are private. Checkpoint
/// restore rederives every result from source and exact shared-counter samples.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CreatureGenerationOwner {
    policy: &'static str,
    world: crate::WorldId,
    profile: crate::Digest,
    source: crate::Digest,
    spawn: u64,
    generations: Vec<CreatureGeneration>,
}
impl CreatureGenerationOwner {
    pub fn initialization(&self) -> Option<&CreatureInitializationFacts> {
        self.current().initialization()
    }

    pub(super) fn validate_initializations(&self, profile: &Profile) -> Result<(), NativeError> {
        let source = profile
            .creature_generations
            .as_ref()
            .and_then(|catalog| catalog.source(self.spawn))
            .ok_or(NativeError::AuthorityBinding)?;
        if source.identity() != self.source || self.profile != profile.digest() {
            return Err(NativeError::AuthorityBinding);
        }
        for life in &self.generations {
            let selected = life.model_selection.draws();
            let model = init::prepare(&source.input)?
                .select_model(selected[0].value(), selected[1].value())?;
            let mut before_level = life.history.first_counter;
            let mut prefix_count = 0;
            for draw in &life.history.draws {
                match draw.purpose {
                    DrawPurpose::CreatureModel { npc } | DrawPurpose::CreatureGender { npc }
                        if npc == life.npc_id() =>
                    {
                        if draw.first_counter != before_level {
                            return Err(NativeError::AuthorityBinding);
                        }
                        before_level = draw_end(draw)?;
                        prefix_count += 1;
                    }
                    _ => break,
                }
            }
            let expected = CreatureInitializationRequest::new(
                profile,
                self.world,
                life.npc_id(),
                life.creation_tick(),
                life.creation_first_counter(),
                before_level,
                &model,
                &life.history.draws[..prefix_count],
            )?;
            match (&expected, life.initialization()) {
                (None, None) => {}
                (Some(request), Some(facts)) => facts.validate_for(request)?,
                _ => return Err(NativeError::AuthorityBinding),
            }
        }
        Ok(())
    }
    fn current(&self) -> &CreatureGeneration {
        self.generations.last().expect("owned initial generation")
    }
    pub fn npc_id(&self) -> crate::NpcId {
        self.current().npc_id()
    }
    pub fn result(&self) -> &init::InitializedCreature {
        self.current().result()
    }
    pub fn model_selection(&self) -> &init::ModelSelectionReceipt {
        self.current().model_selection()
    }
    pub fn respawn_delay_seconds(&self) -> u32 {
        self.current().respawn_delay_seconds()
    }
    pub fn corpse_delay_seconds(&self) -> u32 {
        self.current().corpse_delay_seconds()
    }
    pub fn current_creation_tick(&self) -> u64 {
        self.current().creation_tick()
    }
    pub fn current_death_tick(&self) -> Option<u64> {
        self.current().death_tick()
    }
    pub fn current_corpse_removed_tick(&self) -> Option<u64> {
        self.current().corpse_removed_tick()
    }
    pub fn source_identity(&self) -> crate::Digest {
        self.source
    }
    pub fn profile_identity(&self) -> crate::Digest {
        self.profile
    }
    pub fn world(&self) -> crate::WorldId {
        self.world
    }
    pub fn history(&self) -> &[CreatureGeneration] {
        &self.generations
    }
    pub fn genesis_draws(&self) -> &[Draw] {
        self.generations[0].creation_draws()
    }
    pub fn all_draws(&self) -> impl Iterator<Item = (u64, &Draw)> {
        self.generations.iter().flat_map(|life| {
            life.history
                .draws
                .iter()
                .map(move |draw| (life.history.created_tick, draw))
                .chain(
                    life.history
                        .death
                        .iter()
                        .map(|death| (death.tick, &death.draw)),
                )
        })
    }
    pub fn historical(&self, npc: crate::NpcId) -> Option<&CreatureGeneration> {
        if npc.spawn != self.spawn {
            return None;
        }
        self.generations
            .get(usize::try_from(npc.generation).ok()?)
            .filter(|life| life.npc_id() == npc)
    }
    pub fn project_profile(&self, base: &NpcProfile) -> Result<NpcProfile, NativeError> {
        self.current().project_profile(base)
    }
    fn bound_source<'a>(
        &self,
        catalog: &'a CreatureGenerationCatalog,
    ) -> Result<&'a CreatureGenerationSource, NativeError> {
        let source = catalog
            .source(self.spawn)
            .ok_or(NativeError::AuthorityBinding)?;
        if self.policy != CREATURE_GENERATION_POLICY || source.identity() != self.source {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(source)
    }
    /// Invoke after body loot's draws and immediately before SetDeathState.
    /// Repeated death or failure leaves both this owner and the caller cursor intact.
    pub(super) fn redraw_death_delay(
        &self,
        catalog: &CreatureGenerationCatalog,
        tick: u64,
        seed: u64,
        cursor: &mut u64,
    ) -> Result<(Self, Vec<Draw>), NativeError> {
        let source = self.bound_source(catalog)?;
        if self.current_death_tick().is_some()
            || tick < self.current_creation_tick()
            || *cursor < self.current().creation_end_counter()
        {
            return Err(NativeError::InvalidState);
        }
        let mut next_counter = *cursor;
        let draw = sample(
            source,
            self.world,
            seed,
            &mut next_counter,
            DrawPurpose::CreatureDeathRespawnDelay { npc: self.npc_id() },
            (
                source.respawn_delay_min_seconds,
                source.respawn_delay_max_seconds,
            ),
        )?;
        let mut next = self.clone();
        let life = next
            .generations
            .last_mut()
            .ok_or(NativeError::InvalidState)?;
        life.respawn_delay_seconds = draw
            .value
            .try_into()
            .map_err(|_| NativeError::InvalidState)?;
        life.history.death = Some(DeathHistory {
            tick,
            draw: draw.clone(),
        });
        next.checkpoint().check_budget()?;
        *cursor = next_counter;
        Ok((next, vec![draw]))
    }
    /// Record every supported callback, including no-op min/max timer updates.
    pub(super) fn record_corpse_timer_call(
        &self,
        call: ai::CorpseTimerCall,
    ) -> Result<Self, NativeError> {
        if self.current_death_tick().is_none()
            || self.current_corpse_removed_tick().is_some()
            || call.delay_ms == 0
            || call.at_ms.checked_add(u64::from(call.delay_ms)).is_none()
            || self
                .current()
                .history
                .corpse_timer_calls
                .last()
                .is_some_and(|previous| call.at_ms < previous.at_ms)
        {
            return Err(NativeError::InvalidState);
        }
        if self.current().history.corpse_timer_calls.len() >= MAX_CORPSE_TIMER_CALLS {
            return Err(NativeError::TooLarge);
        }
        let mut next = self.clone();
        next.generations
            .last_mut()
            .ok_or(NativeError::InvalidState)?
            .history
            .corpse_timer_calls
            .push(call);
        next.checkpoint().check_budget()?;
        Ok(next)
    }
    /// Full callback replay supplies the timer basis omitted by AI's last-call
    /// checkpoint. A consistent captured history is still not authenticated origin.
    pub(super) fn validate_corpse_timer(
        &self,
        ai: &ai::SoloCreatureAi,
        tick_ms: u32,
        now_ms: u64,
    ) -> Result<(), NativeError> {
        validate_life_corpse_timer(self.current(), ai, tick_ms, now_ms)
    }
    /// Retain the actual validated removal boundary before respawn clears AI history.
    pub(super) fn record_corpse_removed(
        &self,
        tick: u64,
        ai: &ai::SoloCreatureAi,
    ) -> Result<Self, NativeError> {
        let death = self.current_death_tick().ok_or(NativeError::InvalidState)?;
        let removed_ms = ai.despawned_at_ms().ok_or(NativeError::InvalidState)?;
        let origin = ai.effective_profile().clock_origin_ms;
        let elapsed = removed_ms
            .checked_sub(origin)
            .ok_or(NativeError::InvalidState)?;
        let tick_ms: u32 = elapsed
            .checked_div(tick)
            .filter(|step| *step != 0)
            .and_then(|step| u32::try_from(step).ok())
            .ok_or(NativeError::InvalidState)?;
        if tick < death
            || self.current_corpse_removed_tick().is_some()
            || at_ms(origin, tick_ms, tick)? != removed_ms
            || ai.phase() != ai::Phase::Despawned
            || ai.generation() != Some(self.npc_id().generation)
            || self.current().project_ai(ai.effective_profile())? != *ai.effective_profile()
            || ai.mainhand_timer_limit_ms() != self.result().mainhand_interval_ms.max(100)
        {
            return Err(NativeError::InvalidState);
        }
        self.validate_corpse_timer(ai, tick_ms, removed_ms)?;
        ai.validate_fixed_step(tick_ms)?;
        let mut next = self.clone();
        let life = next
            .generations
            .last_mut()
            .ok_or(NativeError::InvalidState)?;
        life.history.corpse_removed_tick = Some(tick);
        life.history.corpse_removal_ai = Some(ai.checkpoint());
        next.checkpoint().check_budget()?;
        Ok(next)
    }
    #[cfg(test)]
    pub(super) fn respawn(
        &self,
        catalog: &CreatureGenerationCatalog,
        npc: crate::NpcId,
        tick: u64,
        seed: u64,
        cursor: &mut u64,
    ) -> Result<(Self, Vec<Draw>), NativeError> {
        self.respawn_with_initialization(
            catalog,
            npc,
            tick,
            seed,
            cursor,
            None,
            &mut creature_initialization::unavailable_initialization,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn respawn_with_initialization(
        &self,
        catalog: &CreatureGenerationCatalog,
        npc: crate::NpcId,
        tick: u64,
        seed: u64,
        cursor: &mut u64,
        profile: Option<&Profile>,
        observe: &mut dyn FnMut(
            &CreatureInitializationRequest,
        ) -> Result<CreatureInitializationFacts, NativeError>,
    ) -> Result<(Self, Vec<Draw>), NativeError> {
        let source = self.bound_source(catalog)?;
        if self.generations.len() >= MAX_CREATURE_GENERATIONS {
            return Err(NativeError::TooLarge);
        }
        let death = self
            .current()
            .history
            .death
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let removed_tick = self
            .current_corpse_removed_tick()
            .ok_or(NativeError::InvalidState)?;
        if npc.spawn != self.spawn
            || npc.generation
                != self
                    .npc_id()
                    .generation
                    .checked_add(1)
                    .ok_or(NativeError::Limit)?
            || tick <= removed_tick
            || *cursor < draw_end(&death.draw)?
        {
            return Err(NativeError::InvalidState);
        }
        let mut next_counter = *cursor;
        let life = realize(
            source,
            self.world,
            npc,
            tick,
            seed,
            &mut next_counter,
            Some(self.respawn_delay_seconds()),
            profile,
            observe,
        )?;
        let draws = life.history.draws.clone();
        let mut next = self.clone();
        next.generations.push(life);
        next.checkpoint().check_budget()?;
        *cursor = next_counter;
        Ok((next, draws))
    }
    pub fn checkpoint(&self) -> CreatureGenerationCheckpoint {
        CreatureGenerationCheckpoint {
            policy: self.policy.into(),
            world: self.world,
            profile: self.profile,
            source: self.source,
            spawn: self.spawn,
            generations: self
                .generations
                .iter()
                .map(|life| life.history.clone())
                .collect(),
        }
    }
    /// Deterministic consistency and causal bounds, not an authenticated history.
    /// The host additionally validates global interleaving and exact record replay.
    #[allow(clippy::too_many_arguments)]
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn restore(
        catalog: &CreatureGenerationCatalog,
        checkpoint: CreatureGenerationCheckpoint,
        world: crate::WorldId,
        profile: crate::Digest,
        npc: crate::NpcId,
        seed: u64,
        current_tick: u64,
        counter: u64,
        tick_ms: u32,
        clock_origin_ms: u64,
        base: &NpcProfile,
        runtime_profile: Option<&Profile>,
    ) -> Result<Self, NativeError> {
        checkpoint.check_budget()?;
        let source = catalog
            .source(npc.spawn)
            .ok_or(NativeError::InvalidCheckpoint)?;
        if checkpoint.policy != CREATURE_GENERATION_POLICY
            || checkpoint.world != world
            || checkpoint.profile != profile
            || checkpoint.source != source.identity()
            || checkpoint.spawn != npc.spawn
            || tick_ms == 0
            || checkpoint.generations.len() as u64
                != npc.generation.checked_add(1).ok_or(NativeError::Limit)?
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        let mut generations =
            Vec::<CreatureGeneration>::with_capacity(checkpoint.generations.len());
        for (generation, history) in checkpoint.generations.iter().enumerate() {
            if history.npc
                != (crate::NpcId {
                    spawn: npc.spawn,
                    generation: generation as u64,
                })
                || history.created_tick > current_tick
                || history.end_counter > counter
                || history.first_counter > history.end_counter
            {
                return Err(NativeError::InvalidCheckpoint);
            }
            let inherited_delay = match generations.last() {
                None if history.created_tick == 0 => None,
                Some(previous) => {
                    let death = previous
                        .history
                        .death
                        .as_ref()
                        .ok_or(NativeError::InvalidCheckpoint)?;
                    let removed_tick = previous
                        .corpse_removed_tick()
                        .ok_or(NativeError::InvalidCheckpoint)?;
                    let death_ms = at_ms(clock_origin_ms, tick_ms, death.tick)?;
                    let removed_ms = at_ms(clock_origin_ms, tick_ms, removed_tick)?;
                    let due_ms = (death_ms / 1000)
                        .checked_add(u64::from(previous.respawn_delay_seconds()))
                        .and_then(|seconds| seconds.checked_mul(1000))
                        .ok_or(NativeError::Limit)?;
                    let first_birth = ceil_tick(clock_origin_ms, tick_ms, due_ms)?.max(
                        removed_ms
                            .checked_add(u64::from(tick_ms))
                            .ok_or(NativeError::Limit)?,
                    );
                    if at_ms(clock_origin_ms, tick_ms, history.created_tick)? != first_birth
                        || history.first_counter < draw_end(&death.draw)?
                    {
                        return Err(NativeError::InvalidCheckpoint);
                    }
                    Some(previous.respawn_delay_seconds())
                }
                None => return Err(NativeError::InvalidCheckpoint),
            };
            let mut cursor = history.first_counter;
            let mut life = realize(
                source,
                world,
                history.npc,
                history.created_tick,
                seed,
                &mut cursor,
                inherited_delay,
                runtime_profile,
                &mut |request| {
                    let facts = history
                        .initialization
                        .as_ref()
                        .ok_or(NativeError::InvalidCheckpoint)?;
                    facts.validate_for(request)?;
                    Ok(facts.clone())
                },
            )?;
            if cursor != history.end_counter
                || life.history.draws != history.draws
                || life.history.initialization != history.initialization
            {
                return Err(NativeError::InvalidCheckpoint);
            }
            if let Some(death) = &history.death {
                if death.tick < history.created_tick
                    || death.tick > current_tick
                    || death.draw.first_counter < history.end_counter
                    || draw_end(&death.draw)? > counter
                {
                    return Err(NativeError::InvalidCheckpoint);
                }
                cursor = death.draw.first_counter;
                let draw = sample(
                    source,
                    world,
                    seed,
                    &mut cursor,
                    DrawPurpose::CreatureDeathRespawnDelay { npc: history.npc },
                    (
                        source.respawn_delay_min_seconds,
                        source.respawn_delay_max_seconds,
                    ),
                )?;
                if draw != death.draw {
                    return Err(NativeError::InvalidCheckpoint);
                }
                life.respawn_delay_seconds = draw
                    .value
                    .try_into()
                    .map_err(|_| NativeError::InvalidCheckpoint)?;
                life.history.death = Some(death.clone());
            }
            life.history.corpse_timer_calls = history.corpse_timer_calls.clone();
            if !life.corpse_timer_calls().is_empty() && life.death_tick().is_none() {
                return Err(NativeError::InvalidCheckpoint);
            }
            if let Some(removed_tick) = history.corpse_removed_tick {
                if removed_tick > current_tick
                    || life.death_tick().is_none_or(|tick| tick > removed_tick)
                {
                    return Err(NativeError::InvalidCheckpoint);
                }
                let removed_ms = at_ms(clock_origin_ms, tick_ms, removed_tick)?;
                let snapshot = history
                    .corpse_removal_ai
                    .as_ref()
                    .ok_or(NativeError::InvalidCheckpoint)?;
                let ai = ai::SoloCreatureAi::restore_generation_owned(
                    snapshot.clone(),
                    life.project_profile(base)?.ai,
                    removed_ms,
                    removed_tick,
                    SOLO_UNIT,
                    history.npc.generation,
                )?;
                if ai.phase() != ai::Phase::Despawned
                    || ai.despawned_at_ms() != Some(removed_ms)
                    || ai.mainhand_timer_limit_ms() != life.result().mainhand_interval_ms.max(100)
                {
                    return Err(NativeError::InvalidCheckpoint);
                }
                validate_life_corpse_timer(&life, &ai, tick_ms, removed_ms)?;
                ai.validate_fixed_step(tick_ms)?;
                life.history.corpse_removed_tick = Some(removed_tick);
                life.history.corpse_removal_ai = Some(ai.checkpoint());
            }
            if life.history != *history {
                return Err(NativeError::InvalidCheckpoint);
            }
            generations.push(life);
        }
        Ok(Self {
            policy: CREATURE_GENERATION_POLICY,
            world,
            profile,
            source: checkpoint.source,
            spawn: npc.spawn,
            generations,
        })
    }
}

fn bounded_encoded_size<T: Serialize>(value: &T, limit: usize) -> Result<usize, NativeError> {
    struct Counter {
        count: usize,
        limit: usize,
    }
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.count = self
                .count
                .checked_add(bytes.len())
                .filter(|n| *n <= self.limit)
                .ok_or_else(|| std::io::Error::other("creature generation byte budget"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter { count: 0, limit };
    serde_json::to_writer(&mut counter, value).map_err(|_| NativeError::TooLarge)?;
    Ok(counter.count)
}

fn validate_life_corpse_timer(
    life: &CreatureGeneration,
    ai: &ai::SoloCreatureAi,
    tick_ms: u32,
    now_ms: u64,
) -> Result<(), NativeError> {
    let Some(death_tick) = life.death_tick() else {
        if life.corpse_timer_calls().is_empty() && ai.death_at_ms().is_none() {
            return Ok(());
        }
        return Err(NativeError::InvalidState);
    };
    let origin = ai.effective_profile().clock_origin_ms;
    let death_ms = at_ms(origin, tick_ms, death_tick)?;
    let (deadline, last_change, previous_deadline, last_ms) =
        corpse_deadline(life, death_ms, now_ms, origin, tick_ms, ai.death_deferred())?;
    if ai.death_at_ms() != Some(death_ms)
        || ai.corpse_expiration_ms() != deadline
        || ai.corpse_timer_calls() != last_change.as_slice()
        || ai.corpse_timer_previous_ms() != previous_deadline
        || ai.corpse_timer_last_ms() != last_ms
    {
        return Err(NativeError::InvalidCheckpoint);
    }
    Ok(())
}

fn corpse_deadline(
    life: &CreatureGeneration,
    death_ms: u64,
    now_ms: u64,
    origin: u64,
    tick_ms: u32,
    deferred: bool,
) -> Result<(u64, Vec<ai::CorpseTimerCall>, u64, u64), NativeError> {
    let mut deadline = death_ms
        .checked_add(
            u64::from(life.corpse_delay_seconds)
                .checked_mul(1000)
                .ok_or(NativeError::Limit)?,
        )
        .ok_or(NativeError::Limit)?;
    let mut last_ms = death_ms;
    let mut previous_deadline = 0;
    let mut last_change = Vec::new();
    let first_corpse_update = if deferred {
        death_ms
            .checked_add(u64::from(tick_ms))
            .ok_or(NativeError::Limit)?
    } else {
        death_ms
    };
    for call in life.corpse_timer_calls() {
        // Player loot callbacks precede this NPC's update, so equality with the
        // first removal opportunity is reachable; any later callback is not.
        let removal = ceil_tick(origin, tick_ms, deadline)?.max(first_corpse_update);
        if call.delay_ms == 0
            || call.at_ms < last_ms
            || call.at_ms > now_ms
            || call.at_ms > removal
            || call.at_ms < origin
            || !(call.at_ms - origin).is_multiple_of(u64::from(tick_ms))
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        let until = call
            .at_ms
            .checked_add(u64::from(call.delay_ms))
            .ok_or(NativeError::Limit)?;
        let next = if call.inspecting {
            deadline.max(until)
        } else {
            deadline.min(until)
        };
        if next != deadline {
            previous_deadline = deadline;
            last_change.clear();
            last_change.push(*call);
        }
        deadline = next;
        last_ms = call.at_ms;
    }
    Ok((deadline, last_change, previous_deadline, last_ms))
}
fn ceil_tick(origin: u64, tick_ms: u32, at: u64) -> Result<u64, NativeError> {
    if tick_ms == 0 {
        return Err(NativeError::InvalidCheckpoint);
    }
    let tick = u64::from(tick_ms);
    at.saturating_sub(origin)
        .checked_add(tick - 1)
        .and_then(|elapsed| (elapsed / tick).checked_mul(tick))
        .and_then(|elapsed| origin.checked_add(elapsed))
        .ok_or(NativeError::Limit)
}

fn at_ms(origin: u64, tick_ms: u32, tick: u64) -> Result<u64, NativeError> {
    tick.checked_mul(u64::from(tick_ms))
        .and_then(|elapsed| origin.checked_add(elapsed))
        .ok_or(NativeError::Limit)
}
fn draw_end(draw: &Draw) -> Result<u64, NativeError> {
    draw.first_counter
        .checked_add(u64::from(draw.consumed))
        .ok_or(NativeError::Limit)
}
#[allow(clippy::too_many_arguments)]
fn realize(
    source: &CreatureGenerationSource,
    world: crate::WorldId,
    npc: crate::NpcId,
    tick: u64,
    seed: u64,
    cursor: &mut u64,
    inherited_delay: Option<u32>,
    profile: Option<&Profile>,
    observe: &mut dyn FnMut(
        &CreatureInitializationRequest,
    ) -> Result<CreatureInitializationFacts, NativeError>,
) -> Result<CreatureGeneration, NativeError> {
    let first_counter = *cursor;
    let plan = init::prepare(&source.input)?;
    let mut draws = Vec::with_capacity(4);
    let model = selection_sample(
        source,
        world,
        seed,
        cursor,
        DrawPurpose::CreatureModel { npc },
        plan.model_roll_range(),
        &mut draws,
    )?;
    let gender = selection_sample(
        source,
        world,
        seed,
        cursor,
        DrawPurpose::CreatureGender { npc },
        plan.gender_roll_range(model)?,
        &mut draws,
    )?;
    let model = plan.select_model(model, gender)?;
    let initialization = profile
        .map(|profile| {
            CreatureInitializationRequest::new(
                profile,
                world,
                npc,
                tick,
                first_counter,
                *cursor,
                &model,
                &draws,
            )
        })
        .transpose()?
        .flatten()
        .map(|request| {
            let facts = observe(&request)?;
            facts.validate_for(&request)?;
            Ok::<_, NativeError>(facts)
        })
        .transpose()?;
    let level = selection_sample(
        source,
        world,
        seed,
        cursor,
        DrawPurpose::CreatureLevel { npc },
        model.level_roll_range(),
        &mut draws,
    )?;
    let (initialized, model_selection) = model.resolve_with_model_receipt(level)?;
    let regeneration = validate_projection(&source.input, &initialized)?;
    let respawn_delay_seconds = match inherited_delay {
        Some(delay) => delay,
        None => {
            let draw = sample(
                source,
                world,
                seed,
                cursor,
                DrawPurpose::CreatureInitialRespawnDelay { npc },
                (
                    source.respawn_delay_min_seconds,
                    source.respawn_delay_max_seconds,
                ),
            )?;
            let delay = draw
                .value
                .try_into()
                .map_err(|_| NativeError::InvalidState)?;
            draws.push(draw);
            delay
        }
    };
    let entry_delay = initialized.entry_corpse_delay_seconds;
    let corpse_delay_seconds = if npc.generation == 0 {
        entry_delay.min(respawn_delay_seconds * 9 / 10)
    } else {
        entry_delay
    };
    Ok(CreatureGeneration {
        history: GenerationHistory {
            npc,
            created_tick: tick,
            first_counter,
            end_counter: *cursor,
            draws,
            initialization,
            death: None,
            corpse_removed_tick: None,
            corpse_timer_calls: Vec::new(),
            corpse_removal_ai: None,
        },
        initialized,
        model_selection,
        regeneration,
        respawn_delay_seconds,
        corpse_delay_seconds,
    })
}
#[allow(clippy::too_many_arguments)]
fn selection_sample(
    source: &CreatureGenerationSource,
    world: crate::WorldId,
    seed: u64,
    cursor: &mut u64,
    purpose: DrawPurpose,
    range: Option<(u32, u32)>,
    draws: &mut Vec<Draw>,
) -> Result<Option<u32>, NativeError> {
    let Some(range) = range else {
        return Ok(None);
    };
    let draw = sample(source, world, seed, cursor, purpose, range)?;
    let value = draw
        .value
        .try_into()
        .map_err(|_| NativeError::InvalidState)?;
    draws.push(draw);
    Ok(Some(value))
}
fn sample(
    _source: &CreatureGenerationSource,
    world: crate::WorldId,
    seed: u64,
    cursor: &mut u64,
    purpose: DrawPurpose,
    range: (u32, u32),
) -> Result<Draw, NativeError> {
    let low = i64::from(range.0);
    let high = i64::from(range.1);
    let span = rng::sample_inclusive_v2(world, seed, cursor, low, high)?;
    Ok(Draw {
        cast_id: 0,
        purpose,
        first_counter: span.first_counter,
        consumed: span.consumed_words,
        low,
        high,
        value: span.value,
    })
}
fn validate_projection(
    input: &init::Input,
    result: &init::InitializedCreature,
) -> Result<regeneration::Policy, NativeError> {
    for value in [
        result.model.bounding_radius,
        result.model.combat_reach,
        result.model.walk_speed,
        result.model.run_speed,
    ] {
        ai::Real::from_bits(value.bits())?;
    }
    result
        .entry_corpse_delay_seconds
        .checked_mul(1000)
        .ok_or(NativeError::Limit)?;
    // Creature.cpp:1437/1439 sets ordinary rage and energy maxima to zero.
    let (power_type, max_power) = match result.power_type {
        init::PowerType::Mana => (0, result.stats.resources.max_mana),
        init::PowerType::Rage => (1, 0),
        init::PowerType::Energy => (3, 0),
    };
    let policy = regeneration::Policy {
        kind: regeneration::CreatureKind::Ordinary,
        regenerate_stats: input.template.regenerate_stats,
        creature_type_flags: input.template.type_flags,
        unit_flags: result.unit_flags,
        power: regeneration::PowerEvidence::Known {
            power_type,
            max_power,
        },
    };
    policy.validate()?;
    Ok(policy)
}

#[cfg(test)]
mod tests;
