// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
// Creature.cpp:338-366/657-702, ObjectMgr.cpp:555-560/1615-1632,
// Unit.cpp:11930-11987, ObjectDefines.h:47-48. Upstream GPL-2.0-or-later;
// Rust translation GPL-3.0-or-later. See ../../CREATURE-INIT.md.
use super::*;
use serde::Serialize;

pub const CREATURE_MODEL_SELECTION_POLICY_ID: &str =
    "cmangos-8ec338a1-initial-model-selection-receipt-v1";
pub const CREATURE_COLLISION_WIDTH_POLICY_ID: &str =
    "cmangos-8ec338a1-initial-collision-width-softfloat-v1";
pub const CREATURE_COLLISION_HEIGHT_POLICY_ID: &str =
    "cmangos-8ec338a1-initial-collision-height-softfloat-v1";

/// Ordered semantic identity only: caller-supplied draws are not an RNG transcript.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ModelSelectionDrawPurpose {
    Model,
    Gender,
    Level,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ModelSelectionDraw {
    purpose: ModelSelectionDrawPurpose,
    range: Option<(u32, u32)>,
    value: Option<u32>,
}
impl ModelSelectionDraw {
    pub fn purpose(&self) -> ModelSelectionDrawPurpose {
        self.purpose
    }
    pub fn range(&self) -> Option<(u32, u32)> {
        self.range
    }
    pub fn value(&self) -> Option<u32> {
        self.value
    }
}

/// Model-selection columns of one captured creature_model_info row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ModelSelectionGenderRow {
    display_id: u32,
    gender: u8,
    other_gender_id: u32,
}
impl ModelSelectionGenderRow {
    pub fn display_id(&self) -> u32 {
        self.display_id
    }
    pub fn gender(&self) -> u8 {
        self.gender
    }
    pub fn other_gender_id(&self) -> u32 {
        self.other_gender_id
    }
}

/// Minted only by a successful, fully validated Plan resolution. Private fields
/// prevent a mutable InitializedCreature from replacing the retained selection.
/// Serialize supports content binding; no Deserialize or public constructor can
/// create a receipt from transcript fields. Complete rows remain a capture contract,
/// not authenticated database/DBC origin or proof of applicable-source completeness.
///
/// This is the initial model only. Respawn/ResetEntry reruns InitEntry and selection;
/// native-display, object-scale or model substitutions invalidate a live owner's
/// use of this receipt unless separately modeled and rebound by that owner.
///
/// ```compile_fail
/// use benilla_sim::creature_init::ModelSelectionReceipt;
/// fn transcript<T: for<'de> serde::Deserialize<'de>>() {}
/// transcript::<ModelSelectionReceipt>();
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelSelectionReceipt {
    policy: &'static str,
    initialization_policy: &'static str,
    entry: u32,
    template_displays: [DisplayChoice; 4],
    raw_template_scale: SpellFloat,
    spawn_template_entry: u32,
    spawn_model_id: u32,
    display_scales: Vec<DisplayRow>,
    gender_rows: Vec<ModelSelectionGenderRow>,
    draws: [ModelSelectionDraw; 3],
    original_display_id: u32,
    native_display_id: u32,
    gender: u8,
    object_scale: SpellFloat,
    bounding_radius: SpellFloat,
    combat_reach: SpellFloat,
}
impl ModelSelectionReceipt {
    pub(super) fn from_resolved(
        plan: &Plan<'_>,
        rolls: SelectionRolls,
        original_display_id: u32,
        model: &Model,
    ) -> Result<Self> {
        // At most five roots (four template choices plus an override), with one
        // nonrecursive other-gender lookup each. Do not retain unrelated stat rows
        // or unbounded irrelevant model/display records in the receipt.
        let displays = plan
            .input
            .displays
            .require("CreatureDisplayInfo.dbc", plan.input.template.entry)?;
        let models = plan
            .input
            .models
            .require("creature_model_info", plan.input.template.entry)?;
        let mut display_scales = Vec::with_capacity(plan.models.len());
        let mut gender_rows = Vec::with_capacity(plan.models.len());
        for &id in plan.models.keys() {
            let display = displays
                .iter()
                .find(|row| row.id == id)
                .ok_or(InitError::Missing {
                    table: "receipt display row",
                    id,
                })?;
            let model = models
                .iter()
                .find(|row| row.id == id)
                .ok_or(InitError::Missing {
                    table: "receipt model row",
                    id,
                })?;
            display_scales.push(*display);
            gender_rows.push(ModelSelectionGenderRow {
                display_id: id,
                gender: model.gender,
                other_gender_id: model.other_gender_id,
            });
        }
        Ok(Self {
            policy: CREATURE_MODEL_SELECTION_POLICY_ID,
            initialization_policy: CREATURE_INIT_POLICY_ID,
            entry: plan.input.template.entry,
            template_displays: plan.input.template.displays,
            raw_template_scale: plan.input.template.scale,
            spawn_template_entry: plan.spawn.entry,
            spawn_model_id: plan.spawn.model_id,
            display_scales,
            gender_rows,
            draws: [
                ModelSelectionDraw {
                    purpose: ModelSelectionDrawPurpose::Model,
                    range: plan.model_roll_range(),
                    value: rolls.model,
                },
                ModelSelectionDraw {
                    purpose: ModelSelectionDrawPurpose::Gender,
                    range: plan.gender_roll_range(rolls.model)?,
                    value: rolls.gender,
                },
                ModelSelectionDraw {
                    purpose: ModelSelectionDrawPurpose::Level,
                    range: plan.level_roll_range(),
                    value: rolls.level,
                },
            ],
            original_display_id,
            native_display_id: model.display_id,
            gender: model.gender,
            object_scale: model.scale,
            bounding_radius: model.bounding_radius,
            combat_reach: model.combat_reach,
        })
    }
    pub fn policy(&self) -> &'static str {
        self.policy
    }
    pub fn initialization_policy(&self) -> &'static str {
        self.initialization_policy
    }
    pub fn entry(&self) -> u32 {
        self.entry
    }
    pub fn template_displays(&self) -> &[DisplayChoice; 4] {
        &self.template_displays
    }
    pub fn raw_template_scale(&self) -> SpellFloat {
        self.raw_template_scale
    }
    pub fn spawn_template_entry(&self) -> u32 {
        self.spawn_template_entry
    }
    pub fn spawn_model_id(&self) -> u32 {
        self.spawn_model_id
    }
    /// Sorted, reachable display identity/scale projection for a loaded-reader bridge
    /// to compare bit-for-bit against its own CreatureDisplayInfo observations.
    pub fn display_scales(&self) -> &[DisplayRow] {
        &self.display_scales
    }
    pub fn gender_rows(&self) -> &[ModelSelectionGenderRow] {
        &self.gender_rows
    }
    pub fn draws(&self) -> &[ModelSelectionDraw; 3] {
        &self.draws
    }
    pub fn original_display_id(&self) -> u32 {
        self.original_display_id
    }
    pub fn native_display_id(&self) -> u32 {
        self.native_display_id
    }
    pub fn gender(&self) -> u8 {
        self.gender
    }
    pub fn object_scale(&self) -> SpellFloat {
        self.object_scale
    }
    /// Initial normalizedScale is one, so these retain the selected SQL model's
    /// bounds, independently of effective object scale or DBC collision width.
    pub fn bounding_radius(&self) -> SpellFloat {
        self.bounding_radius
    }
    pub fn combat_reach(&self) -> SpellFloat {
        self.combat_reach
    }

    /// Exact Unit::GetCollisionWidth arithmetic for this initial model. The caller
    /// must bind the supplied ModelId/raw width to its actual loaded DBC row and
    /// bind this receipt to the current owner. IDs are checked for consistency,
    /// not authenticated here. DBC model/render scale is intentionally not an input.
    pub fn collision_width(
        &self,
        display_id: u32,
        model_id: u32,
        raw_width: SpellFloat,
    ) -> Result<InitialCollisionWidth> {
        if display_id != self.native_display_id || model_id == 0 {
            return Err(InitError::Invalid("collision-width model identity"));
        }
        Ok(InitialCollisionWidth {
            policy: CREATURE_COLLISION_WIDTH_POLICY_ID,
            native_display_id: display_id,
            model_id,
            object_scale: self.object_scale,
            raw_width,
            width: numeric::collision_width(self.object_scale, raw_width)?,
        })
    }

    /// Exact Unit::GetCollisionHeight arithmetic for this initial native model.
    /// The caller must bind ModelId and raw field-15 height to its loaded DBC row
    /// and this receipt to the current owner. Identity checks are not provenance.
    /// The validated plan retains positive effective object scale; this does not
    /// accept stored zero scale or apply later form/current-scale changes.
    pub fn collision_height(
        &self,
        display_id: u32,
        model_id: u32,
        raw_height: SpellFloat,
    ) -> Result<InitialCollisionHeight> {
        if display_id != self.native_display_id || model_id == 0 {
            return Err(InitError::Invalid("collision-height model identity"));
        }
        Ok(InitialCollisionHeight {
            policy: CREATURE_COLLISION_HEIGHT_POLICY_ID,
            native_display_id: display_id,
            model_id,
            object_scale: self.object_scale,
            raw_height,
            height: numeric::collision_height(self.object_scale, raw_height)?,
        })
    }
}

/// Immutable arithmetic result, not loaded-DBC provenance or a live-state witness.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct InitialCollisionWidth {
    policy: &'static str,
    native_display_id: u32,
    model_id: u32,
    object_scale: SpellFloat,
    raw_width: SpellFloat,
    width: SpellFloat,
}
impl InitialCollisionWidth {
    pub fn policy(&self) -> &'static str {
        self.policy
    }
    pub fn native_display_id(&self) -> u32 {
        self.native_display_id
    }
    pub fn model_id(&self) -> u32 {
        self.model_id
    }
    pub fn object_scale(&self) -> SpellFloat {
        self.object_scale
    }
    pub fn raw_width(&self) -> SpellFloat {
        self.raw_width
    }
    pub fn width(&self) -> SpellFloat {
        self.width
    }
}

/// Immutable initial-height arithmetic, not loaded-DBC provenance or live state.
/// No public constructor or Deserialize can mint a receipt from a scalar.
///
/// ```compile_fail
/// use benilla_sim::creature_init::InitialCollisionHeight;
/// fn transcript<T: for<'de> serde::Deserialize<'de>>() {}
/// transcript::<InitialCollisionHeight>();
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct InitialCollisionHeight {
    policy: &'static str,
    native_display_id: u32,
    model_id: u32,
    object_scale: SpellFloat,
    raw_height: SpellFloat,
    height: SpellFloat,
}
impl InitialCollisionHeight {
    pub fn policy(&self) -> &'static str {
        self.policy
    }
    pub fn native_display_id(&self) -> u32 {
        self.native_display_id
    }
    pub fn model_id(&self) -> u32 {
        self.model_id
    }
    pub fn object_scale(&self) -> SpellFloat {
        self.object_scale
    }
    pub fn raw_height(&self) -> SpellFloat {
        self.raw_height
    }
    pub fn height(&self) -> SpellFloat {
        self.height
    }
}
