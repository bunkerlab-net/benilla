// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
// Source grant GPL-2.0-or-later; Rust translation GPL-3.0-or-later.
//! Bounded initial player scale/height arithmetic, not loaded-data provenance.
//! Source, oracle profile and exclusions: ../PLAYER-MODEL.md.
use crate::classic_spell::SpellFloat;
use rustc_apfloat::{ieee::Single, Float, Round, Status};
use serde::{Deserialize, Serialize};

pub const PLAYER_MODEL_POLICY_ID: &str = "cmangos-8ec338a1-initial-player-model-softfloat-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerModelError {
    InvalidIdentity,
    Numeric(&'static str),
}
impl std::fmt::Display for PlayerModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bounded initial player model: {self:?}")
    }
}
impl std::error::Error for PlayerModelError {}

/// Raw fields for an already selected native player display and its model.
/// The caller must bind these fields to genuine loaded rows and player identity;
/// supplying IDs or deserializing these inputs proves neither ownership nor data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InitialPlayerModelInputs {
    pub native_display_id: u32,
    pub model_id: u32,
    /// CreatureDisplayInfo field 4, including either signed zero.
    pub display_scale: SpellFloat,
    /// CreatureModelData field 4, including either signed zero.
    pub model_scale: SpellFloat,
    /// CreatureModelData field 15; width is not a substitute.
    pub raw_height: SpellFloat,
}

/// Immutable arithmetic receipt; not an initial-player owner or live-state proof.
/// It can only be constructed by evaluating all raw inputs, never a final scalar.
///
/// ```compile_fail
/// use benilla_sim::player_model::InitialPlayerCollisionHeight;
/// fn transcript<T: for<'de> serde::Deserialize<'de>>() {}
/// transcript::<InitialPlayerCollisionHeight>();
/// ```
/// ```compile_fail
/// use benilla_sim::{classic_spell::SpellFloat, player_model::InitialPlayerCollisionHeight};
/// fn replace(receipt: &mut InitialPlayerCollisionHeight) {
///     receipt.height = SpellFloat::ONE;
/// }
/// ```
/// ```compile_fail
/// use benilla_sim::{classic_spell::SpellFloat, player_model::InitialPlayerCollisionHeight};
/// let receipt: InitialPlayerCollisionHeight = SpellFloat::ONE.into();
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct InitialPlayerCollisionHeight {
    policy: &'static str,
    native_display_id: u32,
    model_id: u32,
    display_scale: SpellFloat,
    model_scale: SpellFloat,
    raw_height: SpellFloat,
    raw_scale_product: SpellFloat,
    stored_object_scale: SpellFloat,
    effective_object_scale: SpellFloat,
    height: SpellFloat,
}
impl InitialPlayerCollisionHeight {
    pub fn policy(&self) -> &'static str {
        self.policy
    }
    pub fn native_display_id(&self) -> u32 {
        self.native_display_id
    }
    pub fn model_id(&self) -> u32 {
        self.model_id
    }
    pub fn display_scale(&self) -> SpellFloat {
        self.display_scale
    }
    pub fn model_scale(&self) -> SpellFloat {
        self.model_scale
    }
    pub fn raw_height(&self) -> SpellFloat {
        self.raw_height
    }
    pub fn raw_scale_product(&self) -> SpellFloat {
        self.raw_scale_product
    }
    pub fn stored_object_scale(&self) -> SpellFloat {
        self.stored_object_scale
    }
    pub fn effective_object_scale(&self) -> SpellFloat {
        self.effective_object_scale
    }
    pub fn height(&self) -> SpellFloat {
        self.height
    }
}

fn multiply(
    left: SpellFloat,
    right: SpellFloat,
    stage: &'static str,
) -> Result<SpellFloat, PlayerModelError> {
    let product = Single::from_bits(u128::from(left.bits())).mul_r(
        Single::from_bits(u128::from(right.bits())),
        Round::NearestTiesToEven,
    );
    if !(product.status & !Status::INEXACT).is_empty() {
        return Err(PlayerModelError::Numeric(stage));
    }
    SpellFloat::from_bits(product.value.to_bits() as u32)
        .map_err(|_| PlayerModelError::Numeric(stage))
}

/// Player.cpp:17491-17526 multiplies display.scale * model.Scale in binary32;
/// Object.cpp:107-113,872-883 leaves fresh +0 storage unchanged for either zero.
/// Object.h:364-367 repairs either stored zero to one; Unit.cpp:11930-11957
/// multiplies that getter by native-model height, repairing only exact ±0.
/// Each multiply rounds separately. Unsupported arithmetic (including underflow)
/// fails closed before a receipt is returned; no runtime admission is implied.
pub fn initial_collision_height(
    inputs: InitialPlayerModelInputs,
) -> Result<InitialPlayerCollisionHeight, PlayerModelError> {
    if inputs.native_display_id == 0 || inputs.model_id == 0 {
        return Err(PlayerModelError::InvalidIdentity);
    }
    let raw_scale_product = multiply(inputs.display_scale, inputs.model_scale, "stored scale")?;
    let stored_object_scale = if raw_scale_product.bits() & 0x7fff_ffff == 0 {
        SpellFloat::ZERO
    } else {
        raw_scale_product
    };
    let effective_object_scale = if stored_object_scale.bits() & 0x7fff_ffff == 0 {
        SpellFloat::ONE
    } else {
        stored_object_scale
    };
    let product = multiply(
        effective_object_scale,
        inputs.raw_height,
        "collision height",
    )?;
    let height = if product.bits() & 0x7fff_ffff == 0 {
        SpellFloat::from_bits(0x4002_007e)
            .map_err(|_| PlayerModelError::Numeric("default collision height"))?
    } else {
        product
    };
    Ok(InitialPlayerCollisionHeight {
        policy: PLAYER_MODEL_POLICY_ID,
        native_display_id: inputs.native_display_id,
        model_id: inputs.model_id,
        display_scale: inputs.display_scale,
        model_scale: inputs.model_scale,
        raw_height: inputs.raw_height,
        raw_scale_product,
        stored_object_scale,
        effective_object_scale,
        height,
    })
}
