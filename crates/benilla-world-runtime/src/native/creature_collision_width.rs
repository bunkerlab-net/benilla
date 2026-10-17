// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Creature.cpp
// InitEntry/Respawn and Unit.cpp GetCollisionWidth. Upstream GPL-2.0-or-later.
//! Retained loaded model rows and per-life selection ownership; no native admission.
use super::*;
use benilla_formats::CreatureCatalog;
use benilla_sim::creature_init::{self as source, InitialCollisionWidth, ModelSelectionReceipt};
use serde::{Deserialize, Serialize};

pub const CREATURE_COLLISION_WIDTH_POLICY: &str =
    "cmangos-8ec338a1-native-generation-model-width-v3";

/// Complete is an explicit capture contract, not proof of producer completeness.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum CreatureCollisionWidthRows<T> {
    Missing,
    Complete(Vec<T>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct LoadedModelRow {
    display_id: u32,
    model_id: u32,
    display_scale_bits: u32,
    model_scale_bits: u32,
    raw_width_bits: u32,
    raw_height_bits: u32,
}

/// Only a validated initializer receipt and the actual loaded catalog can create
/// this retained binding. Caller-writable Model or a free width is insufficient.
/// ```compile_fail
/// use benilla_world_runtime::native::CreatureCollisionWidthSource;
/// let _: CreatureCollisionWidthSource = serde_json::from_str("{}").unwrap();
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CreatureCollisionWidthSource {
    spawn_id: u64,
    selection: ModelSelectionReceipt,
    loaded_rows: BTreeMap<u32, LoadedModelRow>,
    initial_width: InitialCollisionWidth,
}
impl CreatureCollisionWidthSource {
    pub fn from_initial_model(
        spawn_id: u64,
        selection: ModelSelectionReceipt,
        catalog: &CreatureCatalog,
    ) -> Result<Self, NativeError> {
        if spawn_id <= SOLO_UNIT {
            return Err(NativeError::InvalidProfile);
        }
        let mut loaded_rows = BTreeMap::new();
        for display in selection.display_scales() {
            let row = catalog.collision_width_row(display.id).ok_or(
                NativeError::AuthorityUnavailable("complete loaded creature model width row"),
            )?;
            let height = catalog.collision_height_row(display.id).ok_or(
                NativeError::AuthorityUnavailable("complete loaded creature model height row"),
            )?;
            if row.display_id != display.id
                || row.display_scale.to_bits() != display.scale.bits()
                || height.display_id != row.display_id
                || height.model_id != row.model_id
                || height.display_scale.to_bits() != row.display_scale.to_bits()
                || height.model_scale.to_bits() != row.model_scale.to_bits()
            {
                return Err(NativeError::AuthorityBinding);
            }
            // Retain raw bits, including model/render scale, without folding those
            // DBC scales into OBJECT_FIELD_SCALE_X or the source width product.
            let raw_width = spell::SpellFloat::from_bits(row.collision_width.to_bits())?;
            if row.model_id == 0 || row.collision_width < 0.0 {
                return Err(NativeError::InvalidProfile);
            }
            loaded_rows.insert(
                display.id,
                LoadedModelRow {
                    display_id: row.display_id,
                    model_id: row.model_id,
                    display_scale_bits: row.display_scale.to_bits(),
                    model_scale_bits: row.model_scale.to_bits(),
                    raw_width_bits: raw_width.bits(),
                    // Capture only: height validation is explicitly opt-in and
                    // must not change the width-only raw numeric domain.
                    raw_height_bits: height.collision_height.to_bits(),
                },
            );
        }
        let row = loaded_rows.get(&selection.native_display_id()).ok_or(
            NativeError::AuthorityUnavailable("selected creature model width row"),
        )?;
        let initial_width = selection
            .collision_width(
                row.display_id,
                row.model_id,
                spell::SpellFloat::from_bits(row.raw_width_bits)?,
            )
            .map_err(|_| NativeError::InvalidProfile)?;
        let result = Self {
            spawn_id,
            selection,
            loaded_rows,
            initial_width,
        };
        result.validate()?;
        Ok(result)
    }
    pub fn spawn_id(&self) -> u64 {
        self.spawn_id
    }
    pub fn selection(&self) -> &ModelSelectionReceipt {
        &self.selection
    }
    pub fn initial_width(&self) -> &InitialCollisionWidth {
        &self.initial_width
    }
    fn validate_selection_domain(
        &self,
        selection: &ModelSelectionReceipt,
    ) -> Result<(), NativeError> {
        let retained = &self.selection;
        if selection.policy() != retained.policy()
            || selection.initialization_policy() != retained.initialization_policy()
            || selection.entry() != retained.entry()
            || selection.template_displays() != retained.template_displays()
            || selection.raw_template_scale() != retained.raw_template_scale()
            || selection.spawn_template_entry() != retained.spawn_template_entry()
            || selection.spawn_model_id() != retained.spawn_model_id()
            || selection.display_scales() != retained.display_scales()
            || selection.gender_rows() != retained.gender_rows()
            || (selection.native_display_id() == retained.native_display_id()
                && (selection.bounding_radius() != retained.bounding_radius()
                    || selection.combat_reach() != retained.combat_reach()))
        {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
    fn selected_width(
        &self,
        selection: &ModelSelectionReceipt,
    ) -> Result<InitialCollisionWidth, NativeError> {
        self.validate_selection_domain(selection)?;
        let row = self
            .loaded_rows
            .get(&selection.native_display_id())
            .ok_or(NativeError::AuthorityBinding)?;
        selection
            .collision_width(
                row.display_id,
                row.model_id,
                spell::SpellFloat::from_bits(row.raw_width_bits)?,
            )
            .map_err(|_| NativeError::InvalidProfile)
    }
    pub(super) fn selected_height(
        &self,
        selection: &ModelSelectionReceipt,
    ) -> Result<source::InitialCollisionHeight, NativeError> {
        self.validate_selection_domain(selection)?;
        let row = self
            .loaded_rows
            .get(&selection.native_display_id())
            .ok_or(NativeError::AuthorityBinding)?;
        selection
            .collision_height(
                row.display_id,
                row.model_id,
                spell::SpellFloat::from_bits(row.raw_height_bits)?,
            )
            .map_err(|_| NativeError::InvalidProfile)
    }
    pub(super) fn selected_height_before_level(
        &self,
        selection: &source::ModelPlan<'_>,
    ) -> Result<source::InitialCollisionHeight, NativeError> {
        let row = self
            .loaded_rows
            .get(&selection.model().display_id)
            .ok_or(NativeError::AuthorityBinding)?;
        selection
            .collision_height(
                row.display_id,
                row.model_id,
                spell::SpellFloat::from_bits(row.raw_height_bits)?,
            )
            .map_err(|_| NativeError::InvalidProfile)
    }
    /// Validate all possible model products before the shared RNG is advanced.
    pub(super) fn generation_selections(
        &self,
        profile: &Profile,
    ) -> Result<Vec<ModelSelectionReceipt>, NativeError> {
        let source = profile
            .creature_generations
            .as_ref()
            .and_then(|catalog| catalog.source(self.spawn_id))
            .ok_or(NativeError::InvalidProfile)?;
        let input = &source.input;
        let plan = source::prepare(input)?;
        let mut model_rolls = vec![None];
        if plan.model_roll_range().is_some() {
            model_rolls.clear();
            let mut first = 0u32;
            for choice in input.template.displays {
                if choice.id != 0 {
                    if choice.probability != 0 {
                        model_rolls.push(Some(first));
                    }
                    first = first
                        .checked_add(choice.probability)
                        .ok_or(NativeError::Limit)?;
                }
            }
        }
        let mut selections = Vec::new();
        for model in model_rolls {
            let genders = match plan.gender_roll_range(model)? {
                Some(_) => vec![Some(0), Some(1)],
                None => vec![None],
            };
            for gender in genders {
                let (_, selection) =
                    source::prepare(input)?.resolve_with_model_receipt(source::SelectionRolls {
                        model,
                        gender,
                        level: plan.level_roll_range().map(|range| range.0),
                    })?;
                self.validate_selection_domain(&selection)?;
                self.selected_width(&selection)?;
                selections.push(selection);
            }
        }
        Ok(selections)
    }
    fn validate(&self) -> Result<(), NativeError> {
        if self.spawn_id <= SOLO_UNIT
            || self.selection.policy() != source::CREATURE_MODEL_SELECTION_POLICY_ID
            || self.selection.initialization_policy() != source::CREATURE_INIT_POLICY_ID
            || self.initial_width.policy() != source::CREATURE_COLLISION_WIDTH_POLICY_ID
            || self.loaded_rows.len() != self.selection.display_scales().len()
        {
            return Err(NativeError::InvalidProfile);
        }
        for display in self.selection.display_scales() {
            let row = self
                .loaded_rows
                .get(&display.id)
                .ok_or(NativeError::InvalidProfile)?;
            let width = spell::SpellFloat::from_bits(row.raw_width_bits)?;
            if row.display_id != display.id
                || row.model_id == 0
                || row.display_scale_bits != display.scale.bits()
                || f32::from_bits(width.bits()) < 0.0
            {
                return Err(NativeError::InvalidProfile);
            }
        }
        let row = self
            .loaded_rows
            .get(&self.selection.native_display_id())
            .ok_or(NativeError::InvalidProfile)?;
        let expected = self
            .selection
            .collision_width(
                row.display_id,
                row.model_id,
                spell::SpellFloat::from_bits(row.raw_width_bits)?,
            )
            .map_err(|_| NativeError::InvalidProfile)?;
        if expected != self.initial_width {
            return Err(NativeError::InvalidProfile);
        }
        Ok(())
    }
    pub(super) fn initial_height(&self) -> Result<source::InitialCollisionHeight, NativeError> {
        self.validate()?;
        let row = self
            .loaded_rows
            .get(&self.selection.native_display_id())
            .ok_or(NativeError::InvalidProfile)?;
        self.selection
            .collision_height(
                row.display_id,
                row.model_id,
                spell::SpellFloat::from_bits(row.raw_height_bits)?,
            )
            .map_err(|_| NativeError::InvalidProfile)
    }
    pub(super) fn binding(&self) -> crate::Digest {
        crate::digest(
            b"benilla-generation-model-width-source-v3\0",
            &(CREATURE_COLLISION_WIDTH_POLICY, self),
        )
    }
}

/// Private retained initializer/DBC projections are part of Profile::digest.
/// No Deserialize, mutable entries, database authentication or admission token.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CreatureCollisionWidthCatalog {
    policy: &'static str,
    spawns: BTreeMap<u64, CreatureCollisionWidthSource>,
}
impl CreatureCollisionWidthCatalog {
    pub fn validate_sources(
        rows: CreatureCollisionWidthRows<CreatureCollisionWidthSource>,
    ) -> Result<Self, NativeError> {
        let CreatureCollisionWidthRows::Complete(rows) = rows else {
            return Err(NativeError::AuthorityUnavailable(
                "initial creature model selections",
            ));
        };
        if rows.len() > 4096 {
            return Err(NativeError::TooLarge);
        }
        let mut spawns = BTreeMap::new();
        let mut displays = BTreeMap::new();
        let mut models = BTreeMap::new();
        let mut templates = BTreeMap::new();
        let mut spawn_templates = BTreeMap::new();
        let mut selection_models = BTreeMap::new();
        let mut selected_bounds = BTreeMap::new();
        for row in rows {
            row.validate()?;
            let template = (
                row.selection.template_displays(),
                row.selection.raw_template_scale(),
            );
            let template_binding =
                crate::digest(b"benilla-model-selection-template-v1\0", &template);
            if templates
                .insert(row.selection.entry(), template_binding)
                .is_some_and(|old| old != template_binding)
                || spawn_templates
                    .insert(
                        row.selection.spawn_template_entry(),
                        row.selection.spawn_model_id(),
                    )
                    .is_some_and(|old| old != row.selection.spawn_model_id())
            {
                return Err(NativeError::InvalidProfile);
            }
            let bounds = (
                row.selection.bounding_radius(),
                row.selection.combat_reach(),
            );
            if selected_bounds
                .insert(row.selection.native_display_id(), bounds)
                .is_some_and(|old| old != bounds)
            {
                return Err(NativeError::InvalidProfile);
            }
            for model in row.selection.gender_rows() {
                if selection_models
                    .insert(model.display_id(), *model)
                    .is_some_and(|old| old != *model)
                {
                    return Err(NativeError::InvalidProfile);
                }
            }
            for (&id, loaded) in &row.loaded_rows {
                let model = (
                    loaded.model_scale_bits,
                    loaded.raw_width_bits,
                    loaded.raw_height_bits,
                );
                if models
                    .insert(loaded.model_id, model)
                    .is_some_and(|old| old != model)
                    || displays
                        .insert(id, loaded.clone())
                        .is_some_and(|old| old != *loaded)
                {
                    return Err(NativeError::InvalidProfile);
                }
            }
            if spawns.insert(row.spawn_id, row).is_some() {
                return Err(NativeError::InvalidProfile);
            }
        }
        Ok(Self {
            policy: CREATURE_COLLISION_WIDTH_POLICY,
            spawns,
        })
    }
    pub fn source(&self, spawn_id: u64) -> Option<&CreatureCollisionWidthSource> {
        self.spawns.get(&spawn_id)
    }
    pub(super) fn validate(&self, profile: &Profile) -> Result<(), NativeError> {
        if self.policy != CREATURE_COLLISION_WIDTH_POLICY
            || profile.source_motion.is_none()
            || profile.creature_capabilities.is_none()
        {
            return Err(NativeError::InvalidProfile);
        }
        if self.spawns.len() != profile.npcs.len() {
            return Err(NativeError::AuthorityUnavailable(
                "complete initial model spawn bindings",
            ));
        }
        for npc in &profile.npcs {
            let row = self
                .source(npc.ai.creature_id)
                .ok_or(NativeError::AuthorityUnavailable(
                    "initial model spawn binding",
                ))?;
            row.validate()?;
            let capabilities = profile
                .creature_capabilities
                .as_ref()
                .and_then(|c| c.source(npc.ai.creature_id))
                .ok_or(NativeError::InvalidProfile)?;
            if row.selection.entry() != npc.entry
                || row.selection.spawn_template_entry()
                    != capabilities.requested_spawn_template_entry
            {
                return Err(NativeError::InvalidProfile);
            }
            if profile.creature_generations.is_some() {
                row.generation_selections(profile)?;
            } else if row.selection.native_display_id() != npc.display_id
                || row.selection.bounding_radius().bits() != npc.ai.bounding_radius.bits()
                || row.selection.combat_reach().bits() != npc.ai.combat_reach.bits()
            {
                return Err(NativeError::InvalidProfile);
            }
        }
        Ok(())
    }
}
impl Profile {
    pub fn initialize_creature_collision_widths(
        &mut self,
        rows: CreatureCollisionWidthRows<CreatureCollisionWidthSource>,
    ) -> Result<(), NativeError> {
        let catalog = CreatureCollisionWidthCatalog::validate_sources(rows)?;
        let mut next = self.clone();
        next.creature_collision_widths = Some(catalog);
        next.validate()?;
        *self = next;
        Ok(())
    }
}

/// Compact creation evidence, rederived from retained source rows. Provider-side
/// validation proves arithmetic and request shape, not live RNG ownership. The
/// frame additionally compares these exact fields with its actual generation owner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GenerationModelSelection {
    world: crate::WorldId,
    npc: crate::NpcId,
    profile: crate::Digest,
    source: crate::Digest,
    created_tick: u64,
    first_counter: u64,
    end_counter: u64,
    draws: Vec<Draw>,
}
impl GenerationModelSelection {
    fn from_owner(
        profile: &Profile,
        npc: crate::NpcId,
        owner: &CreatureGenerationOwner,
    ) -> Result<Self, NativeError> {
        let source = profile
            .creature_generations
            .as_ref()
            .and_then(|catalog| catalog.source(npc.spawn))
            .ok_or(NativeError::AuthorityBinding)?;
        if owner.npc_id() != npc
            || owner.source_identity() != source.identity()
            || owner.profile_identity() != profile.digest()
        {
            return Err(NativeError::AuthorityBinding);
        }
        let life = owner.historical(npc).ok_or(NativeError::AuthorityBinding)?;
        let result = Self {
            world: owner.world(),
            npc,
            profile: owner.profile_identity(),
            source: owner.source_identity(),
            created_tick: life.creation_tick(),
            first_counter: life.creation_first_counter(),
            end_counter: life.creation_end_counter(),
            draws: life.creation_draws().to_vec(),
        };
        if result.selection(profile, npc)? != *owner.model_selection() {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(result)
    }
    pub(super) fn selection(
        &self,
        profile: &Profile,
        npc: crate::NpcId,
    ) -> Result<ModelSelectionReceipt, NativeError> {
        let source = profile
            .creature_generations
            .as_ref()
            .and_then(|catalog| catalog.source(npc.spawn))
            .ok_or(NativeError::AuthorityBinding)?;
        if self.npc != npc
            || self.profile != profile.digest()
            || self.source != source.identity()
            || self.draws.len() > 4
            || (npc.generation == 0) != (self.created_tick == 0)
            || self.first_counter > self.end_counter
        {
            return Err(NativeError::AuthorityBinding);
        }
        let plan = source::prepare(&source.input)?;
        let mut draws = self.draws.iter();
        let mut cursor = self.first_counter;
        let mut take = |purpose, range: Option<(u32, u32)>| -> Result<Option<u32>, NativeError> {
            let Some((low, high)) = range else {
                return Ok(None);
            };
            let draw = draws.next().ok_or(NativeError::AuthorityBinding)?;
            if draw.cast_id != 0
                || draw.purpose != purpose
                || draw.first_counter != cursor
                || draw.consumed == 0
                || draw.consumed > 64
                || draw.low != i64::from(low)
                || draw.high != i64::from(high)
                || !(draw.low..=draw.high).contains(&draw.value)
            {
                return Err(NativeError::AuthorityBinding);
            }
            cursor = cursor
                .checked_add(u64::from(draw.consumed))
                .ok_or(NativeError::Limit)?;
            Ok(Some(
                draw.value
                    .try_into()
                    .map_err(|_| NativeError::AuthorityBinding)?,
            ))
        };
        let model = take(DrawPurpose::CreatureModel { npc }, plan.model_roll_range())?;
        let gender = take(
            DrawPurpose::CreatureGender { npc },
            plan.gender_roll_range(model)?,
        )?;
        let level = take(DrawPurpose::CreatureLevel { npc }, plan.level_roll_range())?;
        if npc.generation == 0 {
            take(
                DrawPurpose::CreatureInitialRespawnDelay { npc },
                Some((
                    source.respawn_delay_min_seconds,
                    source.respawn_delay_max_seconds,
                )),
            )?;
        }
        if draws.next().is_some() || cursor != self.end_counter {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(plan
            .resolve_with_model_receipt(source::SelectionRolls {
                model,
                gender,
                level,
            })?
            .1)
    }
}

/// Serialized witnesses are untrusted transcript data. Validation checks the
/// immutable source and selection; the frame must additionally bind its live owner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureCollisionWidthWitness {
    npc: crate::NpcId,
    content: crate::Digest,
    source_binding: crate::Digest,
    native_display_id: u32,
    model_id: u32,
    object_scale: spell::SpellFloat,
    raw_width: spell::SpellFloat,
    width: spell::SpellFloat,
    #[serde(deserialize_with = "required_option")]
    generation_selection: Option<GenerationModelSelection>,
}
impl CreatureCollisionWidthWitness {
    pub fn npc(&self) -> crate::NpcId {
        self.npc
    }
    pub fn source_binding(&self) -> crate::Digest {
        self.source_binding
    }
    pub fn native_display_id(&self) -> u32 {
        self.native_display_id
    }
    pub fn model_id(&self) -> u32 {
        self.model_id
    }
    pub fn object_scale(&self) -> spell::SpellFloat {
        self.object_scale
    }
    pub fn raw_width(&self) -> spell::SpellFloat {
        self.raw_width
    }
    pub fn width(&self) -> spell::SpellFloat {
        self.width
    }
    pub(super) fn generation_selection(&self) -> Option<&GenerationModelSelection> {
        self.generation_selection.as_ref()
    }
    /// Reconstruct the current selected model after validating this width receipt.
    /// A dry initializer must describe the very same world, creation clock and
    /// model/gender draw prefix, not merely an equal display or collision scalar.
    /// This is transcript consistency; the emitting runtime binds the live owner.
    pub fn validated_model_selection(
        &self,
        profile: &Profile,
        npc: crate::NpcId,
        initialization: Option<&CreatureInitializationFacts>,
    ) -> Result<ModelSelectionReceipt, NativeError> {
        self.validate_for(profile, npc)?;
        let Some(evidence) = &self.generation_selection else {
            if initialization.is_some() {
                return Err(NativeError::AuthorityBinding);
            }
            return profile
                .creature_collision_widths
                .as_ref()
                .and_then(|catalog| catalog.source(npc.spawn))
                .map(|source| source.selection().clone())
                .ok_or(NativeError::AuthorityBinding);
        };
        let selection = evidence.selection(profile, npc)?;
        let source = profile
            .creature_generations
            .as_ref()
            .and_then(|catalog| catalog.source(npc.spawn))
            .ok_or(NativeError::AuthorityBinding)?;
        let decisions = selection.draws();
        let model = source::prepare(&source.input)?
            .select_model(decisions[0].value(), decisions[1].value())?;
        let mut before_level = evidence.first_counter;
        let mut prefix_count = 0;
        for draw in &evidence.draws {
            match draw.purpose {
                DrawPurpose::CreatureModel { npc: owner }
                | DrawPurpose::CreatureGender { npc: owner }
                    if owner == npc =>
                {
                    if draw.first_counter != before_level {
                        return Err(NativeError::AuthorityBinding);
                    }
                    before_level = before_level
                        .checked_add(u64::from(draw.consumed))
                        .ok_or(NativeError::Limit)?;
                    prefix_count += 1;
                }
                _ => break,
            }
        }
        let expected = CreatureInitializationRequest::new(
            profile,
            evidence.world,
            npc,
            evidence.created_tick,
            evidence.first_counter,
            before_level,
            &model,
            &evidence.draws[..prefix_count],
        )?;
        match (expected, initialization) {
            (None, None) => {}
            (Some(request), Some(facts)) => facts.validate_for(&request)?,
            _ => return Err(NativeError::AuthorityBinding),
        }
        Ok(selection)
    }
    /// Bind a historical model selection to its actual world and creation clock.
    /// This checks transcript consistency; only the live owner supplies the witness.
    pub fn validate_context(
        &self,
        profile: &Profile,
        binding: &Binding,
    ) -> Result<(), NativeError> {
        let npc = binding.npc.ok_or(NativeError::AuthorityBinding)?;
        self.validate_for(profile, npc)?;
        if binding.world == [0; 16]
            || binding.profile != profile.digest()
            || binding.authority_policy != profile.authority_policy
            || binding.geometry != profile.geometry_revision
            || self.generation_selection.as_ref().is_some_and(|selection| {
                selection.world != binding.world || selection.created_tick > binding.tick
            })
        {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
    pub fn validate_for(&self, profile: &Profile, npc: crate::NpcId) -> Result<(), NativeError> {
        let expected = match &self.generation_selection {
            Some(selection) => CreatureCollisionWidthState::from_generation_selection(
                profile,
                npc,
                selection.clone(),
            )?,
            None => CreatureCollisionWidthState::new(profile, npc)?,
        }
        .ok_or(NativeError::AuthorityUnavailable(
            "current creature model selection receipt",
        ))?;
        if self != expected.witness() {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatureCollisionWidthState {
    witness: CreatureCollisionWidthWitness,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CreatureCollisionWidthSnapshot {
    witness: CreatureCollisionWidthWitness,
}
impl CreatureCollisionWidthState {
    pub fn witness(&self) -> &CreatureCollisionWidthWitness {
        &self.witness
    }
    pub(super) fn new(profile: &Profile, npc: crate::NpcId) -> Result<Option<Self>, NativeError> {
        if profile.creature_generations.is_some() {
            return Err(NativeError::AuthorityUnavailable(
                "current creature generation width owner",
            ));
        }
        // Creature::Respawn reruns InitEntry/model selection. Generation-zero
        // source data cannot be relabeled as a new selection for a later life.
        if npc.generation != 0 {
            return Ok(None);
        }
        let Some(catalog) = &profile.creature_collision_widths else {
            return Ok(None);
        };
        catalog.validate(profile)?;
        let row = catalog.source(npc.spawn).ok_or(NativeError::InvalidState)?;
        let value = row.initial_width;
        Ok(Some(Self {
            witness: CreatureCollisionWidthWitness {
                npc,
                content: profile.content_digest,
                source_binding: row.binding(),
                native_display_id: value.native_display_id(),
                model_id: value.model_id(),
                object_scale: value.object_scale(),
                raw_width: value.raw_width(),
                width: value.width(),
                generation_selection: None,
            },
        }))
    }
    pub(super) fn new_generation(
        profile: &Profile,
        npc: crate::NpcId,
        owner: &CreatureGenerationOwner,
    ) -> Result<Option<Self>, NativeError> {
        Self::from_generation_selection(
            profile,
            npc,
            GenerationModelSelection::from_owner(profile, npc, owner)?,
        )
    }
    fn from_generation_selection(
        profile: &Profile,
        npc: crate::NpcId,
        evidence: GenerationModelSelection,
    ) -> Result<Option<Self>, NativeError> {
        let selection = evidence.selection(profile, npc)?;
        let Some(catalog) = &profile.creature_collision_widths else {
            return Ok(None);
        };
        catalog.validate(profile)?;
        let row = catalog.source(npc.spawn).ok_or(NativeError::InvalidState)?;
        let value = row.selected_width(&selection)?;
        Ok(Some(Self {
            witness: CreatureCollisionWidthWitness {
                npc,
                content: profile.content_digest,
                source_binding: crate::digest(
                    b"benilla-generation-model-width-owner-v1\0",
                    &(row.binding(), &evidence),
                ),
                native_display_id: value.native_display_id(),
                model_id: value.model_id(),
                object_scale: value.object_scale(),
                raw_width: value.raw_width(),
                width: value.width(),
                generation_selection: Some(evidence),
            },
        }))
    }
    pub(super) fn validate_generation_optional(
        state: Option<&Self>,
        profile: &Profile,
        npc: crate::NpcId,
        owner: &CreatureGenerationOwner,
    ) -> Result<(), NativeError> {
        if state != Self::new_generation(profile, npc, owner)?.as_ref() {
            return Err(NativeError::InvalidState);
        }
        Ok(())
    }
    pub(super) fn validate_optional(
        state: Option<&Self>,
        profile: &Profile,
        npc: crate::NpcId,
    ) -> Result<(), NativeError> {
        let expected = Self::new(profile, npc)?;
        if state != expected.as_ref() {
            return Err(NativeError::InvalidState);
        }
        Ok(())
    }
    pub(super) fn snapshot(&self) -> CreatureCollisionWidthSnapshot {
        CreatureCollisionWidthSnapshot {
            witness: self.witness.clone(),
        }
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn restore(
        snapshot: Option<CreatureCollisionWidthSnapshot>,
        profile: &Profile,
        npc: crate::NpcId,
    ) -> Result<Option<Self>, NativeError> {
        let state = snapshot.map(|s| Self { witness: s.witness });
        Self::validate_optional(state.as_ref(), profile, npc)
            .map_err(|_| NativeError::InvalidCheckpoint)?;
        Ok(state)
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn restore_generation(
        snapshot: Option<CreatureCollisionWidthSnapshot>,
        profile: &Profile,
        npc: crate::NpcId,
        owner: &CreatureGenerationOwner,
    ) -> Result<Option<Self>, NativeError> {
        let state = snapshot.map(|snapshot| Self {
            witness: snapshot.witness,
        });
        Self::validate_generation_optional(state.as_ref(), profile, npc, owner)
            .map_err(|_| NativeError::InvalidCheckpoint)?;
        Ok(state)
    }
}
impl Npc {
    pub fn creature_collision_width(&self) -> Option<&CreatureCollisionWidthState> {
        self.source_collision_width.as_ref()
    }
}
impl CreatureRouteRequest {
    /// Width-independent route dependencies may accept None. A width-consuming
    /// provider must use this check and cannot supply radius or render scale.
    pub fn require_collision_width(
        &self,
        profile: &Profile,
    ) -> Result<&CreatureCollisionWidthWitness, NativeError> {
        self.owner.validate_for(profile, self.owner.npc())?;
        let identity = self.request.binding.identity;
        if identity.creature != self.owner.npc().spawn
            || identity.map != profile.map_id
            || identity.content.0 != profile.content_digest
            || identity.geometry.0 != profile.geometry_revision
        {
            return Err(NativeError::AuthorityBinding);
        }
        let width = self
            .collision_width
            .as_ref()
            .ok_or(NativeError::AuthorityUnavailable(
                "current creature model collision width",
            ))?;
        width.validate_for(profile, self.owner.npc())?;
        Ok(width)
    }
}

/// Unlike serde's implicit Option default, an absent schema field is an error;
/// explicit JSON null represents the supported missing optional dependency.
pub(super) fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

#[cfg(test)]
mod generation_tests {
    use super::*;
    use crate::native::{synthetic_creature_model_fixture as models, synthetic_fixture as f};

    fn source_input() -> source::Input {
        let mut input = models::input();
        input.template.faction = 1;
        input.template.regenerate_stats = 2;
        let source::Rows::Complete(spawns) = &mut input.spawn_template else {
            unreachable!()
        };
        spawns[0].spawn_flags = 1;
        let source::Rows::Complete(rows) = &mut input.models else {
            unreachable!()
        };
        let alternate = rows.iter_mut().find(|row| row.id == 31).unwrap();
        alternate.bounding_radius = f::float(1.25);
        alternate.combat_reach = f::float(2.0);
        alternate.speed_walk = f::float(1.2);
        input
    }
    fn generation_source() -> CreatureGenerationSource {
        CreatureGenerationSource {
            spawn_id: 2,
            spawn_entry: 60001,
            requested_spawn_template_entry: 0,
            respawn_delay_min_seconds: 2,
            respawn_delay_max_seconds: 2,
            input: source_input(),
        }
    }
    fn prepared_profile(alternate_height: f32) -> Profile {
        let mut profile = models::profile();
        let mut width = CreatureCollisionWidthSource::from_initial_model(
            2,
            models::selection(),
            &models::catalog(),
        )
        .unwrap();
        // Authored retained-row mutations isolate the unselected-branch verifier.
        let alternate = width.loaded_rows.get_mut(&31).unwrap();
        alternate.raw_width_bits = 2.0f32.to_bits();
        alternate.raw_height_bits = alternate_height.to_bits();
        profile
            .initialize_creature_collision_widths(CreatureCollisionWidthRows::Complete(vec![width]))
            .unwrap();
        profile.initialize_creature_collision_heights().unwrap();
        profile
    }
    fn profile() -> Profile {
        let mut profile = prepared_profile(4.0);
        profile
            .initialize_creature_generations(
                source::Rows::Complete(vec![generation_source()]),
                vec![2],
            )
            .unwrap();
        profile
    }
    fn owner(profile: &Profile, seed: u64) -> (CreatureGenerationOwner, u64) {
        let mut cursor = 0;
        let (owner, _) = profile
            .creature_generations
            .as_ref()
            .unwrap()
            .initial(
                [8; 16],
                profile.digest(),
                crate::NpcId {
                    spawn: 2,
                    generation: 0,
                },
                seed,
                &mut cursor,
            )
            .unwrap();
        (owner, cursor)
    }
    #[test]
    fn live_generation_selection_rebinds_both_dimensions_and_rejects_other_owner_recipe() {
        let profile = profile();
        let (first, _) = owner(&profile, 1);
        let (other, _) = (2..=128)
            .map(|seed| owner(&profile, seed))
            .find(|(candidate, _)| {
                candidate.model_selection().native_display_id()
                    != first.model_selection().native_display_id()
            })
            .unwrap();
        let first_width =
            CreatureCollisionWidthState::new_generation(&profile, first.npc_id(), &first)
                .unwrap()
                .unwrap();
        let first_height =
            CreatureCollisionHeightState::new_generation(&profile, first.npc_id(), &first)
                .unwrap()
                .unwrap();
        let other_width =
            CreatureCollisionWidthState::new_generation(&profile, other.npc_id(), &other)
                .unwrap()
                .unwrap();
        let other_height =
            CreatureCollisionHeightState::new_generation(&profile, other.npc_id(), &other)
                .unwrap()
                .unwrap();
        assert_ne!(
            first_width.witness().native_display_id(),
            other_width.witness().native_display_id()
        );
        assert_ne!(first_width.witness().width(), other_width.witness().width());
        assert_ne!(
            first_height.witness().height(),
            other_height.witness().height()
        );
        for (owner, width, height) in [
            (&first, &first_width, &first_height),
            (&other, &other_width, &other_height),
        ] {
            width
                .witness()
                .validate_for(&profile, owner.npc_id())
                .unwrap();
            height
                .witness()
                .validate_for(&profile, owner.npc_id())
                .unwrap();
            assert_eq!(
                height.witness().width_source_binding(),
                width.witness().source_binding()
            );
            assert_eq!(
                CreatureCollisionWidthState::restore_generation(
                    Some(width.snapshot()),
                    &profile,
                    owner.npc_id(),
                    owner
                )
                .unwrap()
                .as_ref(),
                Some(width)
            );
            assert_eq!(
                CreatureCollisionHeightState::restore_generation(
                    Some(height.snapshot()),
                    &profile,
                    owner.npc_id(),
                    owner
                )
                .unwrap()
                .as_ref(),
                Some(height)
            );
        }
        // Both recipes are source-consistent, but only one belongs to this live owner.
        assert!(CreatureCollisionWidthState::validate_generation_optional(
            Some(&other_width),
            &profile,
            first.npc_id(),
            &first
        )
        .is_err());
        assert!(CreatureCollisionHeightState::validate_generation_optional(
            Some(&other_height),
            &profile,
            first.npc_id(),
            &first
        )
        .is_err());
        let mut other_world_cursor = 0;
        let (other_world, _) = profile
            .creature_generations
            .as_ref()
            .unwrap()
            .initial(
                [9; 16],
                profile.digest(),
                first.npc_id(),
                1,
                &mut other_world_cursor,
            )
            .unwrap();
        let other_world_width =
            CreatureCollisionWidthState::new_generation(&profile, first.npc_id(), &other_world)
                .unwrap()
                .unwrap();
        other_world_width
            .witness()
            .validate_for(&profile, first.npc_id())
            .unwrap();
        assert!(CreatureCollisionWidthState::validate_generation_optional(
            Some(&other_world_width),
            &profile,
            first.npc_id(),
            &first
        )
        .is_err());
        assert!(CreatureCollisionWidthState::new(&profile, first.npc_id()).is_err());
        assert!(CreatureCollisionHeightState::new(&profile, first.npc_id()).is_err());
    }
    #[test]
    fn genuine_respawn_uses_new_selection_evidence_and_rejects_relabeled_old_dimensions() {
        let profile = profile();
        let catalog = profile.creature_generations.as_ref().unwrap();
        let (initial, mut cursor) = owner(&profile, 3);
        let old_width =
            CreatureCollisionWidthState::new_generation(&profile, initial.npc_id(), &initial)
                .unwrap()
                .unwrap();
        let old_height =
            CreatureCollisionHeightState::new_generation(&profile, initial.npc_id(), &initial)
                .unwrap()
                .unwrap();
        let (dead, _) = initial
            .redraw_death_delay(catalog, 1, 3, &mut cursor)
            .unwrap();
        let id = crate::NpcId {
            spawn: 2,
            generation: 1,
        };
        let base = profile.npc(2).unwrap();
        let mut ai =
            ai::SoloCreatureAi::new_generation_owned(initial.project_profile(base).unwrap().ai)
                .unwrap();
        let mut removed = None;
        let mut respawned = None;
        for tick in 1..=128 {
            let observation = ai::Observation {
                sequence: ai.next_sequence().unwrap(),
                elapsed_ms: profile.tick_ms,
                geometry_revision: ai::movement::Revision(profile.geometry_revision),
                target: None,
                can_initiate_attack: true,
                casting_non_melee: false,
                stationary_melee_refresh_eligible: false,
                creature_moving_and_running: false,
                movement: None,
                receipts: if tick == 1 {
                    vec![ai::Receipt {
                        id: 1,
                        event: ai::CombatEvent::Died,
                    }]
                } else {
                    vec![]
                },
            };
            let timing = (tick == 1).then_some(ai::DeathTiming {
                receipt_id: 1,
                at_ms: profile.clock_origin_ms + u64::from(profile.tick_ms),
                respawn_delay_seconds: dead.respawn_delay_seconds(),
            });
            let token = ai
                .begin_source_receipts_in_phase_with_death_timing(&observation, false, timing)
                .unwrap();
            let effects = ai
                .finish_receipts(observation, token, &mut |_, _, _| {
                    Ok(ai::MainhandResult::Unchanged)
                })
                .unwrap();
            if effects.contains(&ai::Effect::CorpseRemoved) {
                removed = Some(dead.record_corpse_removed(tick, &ai).unwrap());
            }
            if effects
                .iter()
                .any(|effect| matches!(effect, ai::Effect::Respawned { .. }))
            {
                let (next, _) = removed
                    .as_ref()
                    .unwrap()
                    .respawn(catalog, id, tick, 3, &mut cursor)
                    .unwrap();
                ai.rebind_after_respawn(next.project_profile(base).unwrap().ai, 0, 1)
                    .unwrap();
                respawned = Some(next);
                break;
            }
        }
        let respawned =
            respawned.expect("actual removal and subsequent respawn within fixture budget");
        assert_eq!(ai.generation(), Some(1));
        let width = CreatureCollisionWidthState::new_generation(&profile, id, &respawned)
            .unwrap()
            .unwrap();
        let height = CreatureCollisionHeightState::new_generation(&profile, id, &respawned)
            .unwrap()
            .unwrap();
        assert_ne!(
            width.witness().source_binding(),
            old_width.witness().source_binding()
        );
        assert_ne!(
            height.witness().source_binding(),
            old_height.witness().source_binding()
        );
        assert_eq!(
            width.witness().native_display_id(),
            respawned.model_selection().native_display_id()
        );
        assert_eq!(
            height.witness().native_display_id(),
            respawned.model_selection().native_display_id()
        );
        assert!(CreatureCollisionWidthState::restore_generation(
            Some(old_width.snapshot()),
            &profile,
            id,
            &respawned
        )
        .is_err());
        assert!(CreatureCollisionHeightState::restore_generation(
            Some(old_height.snapshot()),
            &profile,
            id,
            &respawned
        )
        .is_err());
        let mut forged = serde_json::to_value(width.witness()).unwrap();
        forged["generation_selection"]["draws"][0]["first_counter"] = serde_json::json!(0);
        let forged: CreatureCollisionWidthWitness = serde_json::from_value(forged).unwrap();
        assert!(forged.validate_for(&profile, id).is_err());
        let mut missing = serde_json::to_value(width.witness()).unwrap();
        missing
            .as_object_mut()
            .unwrap()
            .remove("generation_selection");
        assert!(serde_json::from_value::<CreatureCollisionWidthWitness>(missing).is_err());
    }
    #[test]
    fn all_reachable_heights_are_checked_before_generation_installation() {
        for height in [f32::NAN, -4.0] {
            let mut profile = prepared_profile(height);
            let before = profile.clone();
            assert!(profile
                .initialize_creature_generations(
                    source::Rows::Complete(vec![generation_source()]),
                    vec![2]
                )
                .is_err());
            assert_eq!(profile, before);
        }
    }
    #[test]
    fn conflicting_bounds_for_the_retained_selected_display_reject_atomically() {
        for change_radius in [false, true] {
            let mut profile = prepared_profile(4.0);
            let before = profile.clone();
            let mut retained = generation_source();
            let source::Rows::Complete(rows) = &mut retained.input.models else {
                unreachable!()
            };
            let row = rows.iter_mut().find(|row| row.id == 34).unwrap();
            if change_radius {
                row.bounding_radius = f::float(2.5);
            } else {
                row.combat_reach = f::float(3.5);
            }
            assert!(profile
                .initialize_creature_generations(source::Rows::Complete(vec![retained]), vec![2])
                .is_err());
            assert_eq!(profile, before);
        }
    }
}
