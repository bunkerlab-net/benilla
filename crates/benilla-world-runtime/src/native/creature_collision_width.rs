// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Creature.cpp
// InitEntry/Respawn and Unit.cpp GetCollisionWidth. Upstream GPL-2.0-or-later.
//! Initial model ownership only; no source authenticity or native admission claim.
use super::*;
use benilla_formats::CreatureCatalog;
use benilla_sim::creature_init::{self as source, InitialCollisionWidth, ModelSelectionReceipt};
use serde::{Deserialize, Serialize};

pub const CREATURE_COLLISION_WIDTH_POLICY: &str = "cmangos-8ec338a1-native-initial-model-width-v1";

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
            if row.display_id != display.id || row.display_scale.to_bits() != display.scale.bits() {
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
    fn binding(&self) -> crate::Digest {
        crate::digest(
            b"benilla-initial-model-width-source-v1\0",
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
                let model = (loaded.model_scale_bits, loaded.raw_width_bits);
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
                || row.selection.native_display_id() != npc.display_id
                || row.selection.bounding_radius().bits() != npc.ai.bounding_radius.bits()
                || row.selection.combat_reach().bits() != npc.ai.combat_reach.bits()
                || row.selection.spawn_template_entry()
                    != capabilities.requested_spawn_template_entry
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

/// Serialized witnesses are untrusted transcript data. Validation checks the
/// immutable profile and generation; it does not authenticate current liveness.
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
    pub fn validate_for(&self, profile: &Profile, npc: crate::NpcId) -> Result<(), NativeError> {
        let expected = CreatureCollisionWidthState::new(profile, npc)?.ok_or(
            NativeError::AuthorityUnavailable("current creature model selection receipt"),
        )?;
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
            },
        }))
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
