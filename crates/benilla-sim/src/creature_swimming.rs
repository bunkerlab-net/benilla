// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f,
// GridMap.cpp:797-803,1147-1161; GridMap.h:154,157;
// Creature.cpp:438-442,575-577; Object.h:761,763.
// Copyright CMaNGOS contributors; upstream GPL-2.0-or-later.
//! Scalar swimming initialization prerequisites, not geometry or live authority.
//! A false query result never clears the movement bit from an earlier life.
use crate::{
    classic_spell::{SpellError, SpellFloat},
    creature_capabilities::{InhabitType, MOVEFLAG_SWIMMING},
};
use rustc_apfloat::{ieee::Single, Float, Round, Status};

pub const POLICY: &str = "cmangos-8ec338a1-creature-swimming-soft-rne-v1";
/// LF-normalized GridMap.cpp, GridMap.h, Creature.cpp and Object.h.
pub const SOURCE_SHA256: [&str; 4] = [
    "52c6d22fbe9f275196a44bf4f9b39da527071e3c81f7460a4ac03a3c9107b85d",
    "accbf9ecc6f436b952d3d38ee2e506fe3d331a7367c048745334160705760c4d",
    "27ae75c9e90febd0827fd60f9c9fe27bfeeab34e476d10ac784ab1039d4b8302",
    "259cdb1a061258d6a8c3e977c743c8c83848c763353b504ca3e617580f58c128",
];
pub const LIQUID_MASK: u8 = 15;
/// Nested getLiquidStatus default classification height, distinct from radius.
pub const LIQUID_COLLISION_HEIGHT_BITS: u32 = 0x4002_007e; // 2.03128f
pub const WALK_IN_WATER: u32 = 0x0000_8000;
pub const UNIT_FLAG_SWIMMING: u32 = 0x0000_8000;

/// Exact scalar branch after a separately bound CanCheckLiquidLevel evaluation.
/// Every nonzero status takes the depth comparison, including ABOVE_WATER and
/// WATER_WALK. The inputs are bounded finite SpellFloat values; a subnormal
/// subtraction result remains valid and is compared without narrowing its domain.
/// This function cannot establish liquid availability or source-query provenance.
pub fn is_swimmable(
    can_check: bool,
    status: u32,
    level: SpellFloat,
    depth_level: SpellFloat,
    radius: SpellFloat,
) -> Result<bool, SpellError> {
    if !can_check || status == 0 {
        return Ok(false);
    }
    let value = |v: SpellFloat| Single::from_bits(u128::from(v.bits()));
    let depth = value(level).sub_r(value(depth_level), Round::NearestTiesToEven);
    if !(depth.status & !(Status::INEXACT | Status::UNDERFLOW)).is_empty() {
        return Err(SpellError::Domain("swimming depth subtraction"));
    }
    Ok(depth.value > value(radius))
}

/// InitEntry adds SWIMMING only after the habitat/ExtraFlags/query conditions.
/// The host must preserve short-circuit query ordering; passing this scalar bool
/// is not proof that any query occurred. No current bit is ever cleared here.
pub const fn initialize_movement_flags(
    current: u32,
    inhabit_type: InhabitType,
    extra_flags: u32,
    swimmable: bool,
) -> u32 {
    if inhabit_type.raw() & 2 != 0 && extra_flags & WALK_IN_WATER == 0 && swimmable {
        current | MOVEFLAG_SWIMMING
    } else {
        current
    }
}

/// UpdateEntry copies the owned movement bit into its already-selected unit flags.
/// A template's pre-existing UNIT_FLAG_SWIMMING also remains untouched.
pub const fn project_unit_flags(base: u32, movement: u32) -> u32 {
    if movement & MOVEFLAG_SWIMMING != 0 {
        base | UNIT_FLAG_SWIMMING
    } else {
        base
    }
}
