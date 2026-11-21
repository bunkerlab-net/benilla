// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Unit::CalcArmorReducedDamage port; attribution/grant in ../classic.rs
// and ../../PROVENANCE.md. Numerical machinery: rust-lang/rustc_apfloat,
// Apache-2.0 WITH LLVM-exception; exact dependency and checksum in Cargo.lock.
//! The source's binary32 steps executed using integer limbs, not host f32/f64.
use super::{ArmorReduction, ClassicError, MAX_VALUE};
use rustc_apfloat::{ieee::Single, Float, Round, Status, StatusAnd};

/// Include this exact policy identity in a runtime's rules/content compatibility hash.
/// It is deliberately different from the existing rational policy version.
pub const SOFTWARE_ARMOR_POLICY_ID: &str = "cmangos-8ec338a1-armor-binary32-rne-v1";

// INEXACT is ordinary IEEE rounding. Other exceptions are outside this input domain.
fn checked<T>(result: StatusAnd<T>) -> Result<T, ClassicError> {
    if !(result.status & !Status::INEXACT).is_empty() {
        Err(ClassicError {
            field: "software_binary32_exception",
        })
    } else {
        Ok(result.value)
    }
}

/// Source-exact bounded armor helper and melee cast, using software binary32.
/// Accepts the same validated integer domain as `armor_reduced_damage`; rounding
/// after EVERY operation is nearest-ties-even, final integer conversion toward zero.
/// Constants are source binary32 bit patterns; no parsing or host float arithmetic.
/// This is a physical-armor primitive, not the full melee or native-creature pipeline.
pub fn software_armor_reduction(
    level: u8,
    armor: i32,
    penetration: i32,
    damage: u32,
) -> Result<ArmorReduction, ClassicError> {
    if !(1..=63).contains(&level) {
        return Err(ClassicError {
            field: "attacker_level",
        });
    }
    if !(-(MAX_VALUE as i32)..=MAX_VALUE as i32).contains(&armor) {
        return Err(ClassicError { field: "armor" });
    }
    if !(-(MAX_VALUE as i32)..=MAX_VALUE as i32).contains(&penetration) {
        return Err(ClassicError {
            field: "armor_penetration",
        });
    }
    if damage > MAX_VALUE {
        return Err(ClassicError { field: "damage" });
    }
    let round = Round::NearestTiesToEven;
    let one = Single::from_bits(0x3f80_0000);
    let cap = Single::from_bits(0x3f40_0000); // 0.75f
    let tenth = Single::from_bits(0x3dcc_cccd); // 0.1f
    let eight_half = Single::from_bits(0x4108_0000); // 8.5f
    let forty = Single::from_bits(0x4220_0000); // 40.0f
                                                // Convert the two signed operands separately before subtraction, as upstream does.
    let armor = checked(Single::from_i128_r(i128::from(armor), round))?;
    let penetration = checked(Single::from_i128_r(i128::from(penetration), round))?;
    let mut armor = checked(armor.sub_r(penetration, round))?;
    if armor < Single::ZERO {
        armor = Single::ZERO;
    }
    let level = checked(Single::from_u128_r(u128::from(level), round))?;
    let numerator = checked(tenth.mul_r(armor, round))?;
    let level_term = checked(eight_half.mul_r(level, round))?;
    let denominator = checked(level_term.add_r(forty, round))?;
    let reduction = checked(numerator.div_r(denominator, round))?;
    let denominator = checked(one.add_r(reduction, round))?;
    let mut reduction = checked(reduction.div_r(denominator, round))?;
    if reduction < Single::ZERO {
        reduction = Single::ZERO;
    }
    if reduction > cap {
        reduction = cap;
    }
    let damage = checked(Single::from_u128_r(u128::from(damage), round))?;
    let mitigated = checked(damage.mul_r(reduction, round))?;
    let mut remaining = checked(damage.sub_r(mitigated, round))?;
    if remaining <= one {
        remaining = one;
    }
    let mut exact = false;
    let integer = checked(remaining.to_u128_r(32, Round::TowardZero, &mut exact))?;
    if integer == 0 || integer > u128::from(MAX_VALUE) {
        return Err(ClassicError { field: "result" });
    }
    Ok(ArmorReduction {
        remaining_damage_bits: remaining.to_bits() as u32,
        melee_damage: integer as u32,
    })
}

/// Software-binary32 source-exact physical-melee result. No host floating point.
pub fn software_armor_reduced_damage(
    level: u8,
    armor: i32,
    penetration: i32,
    damage: u32,
) -> Result<u32, ClassicError> {
    Ok(software_armor_reduction(level, armor, penetration, damage)?.melee_damage)
}
