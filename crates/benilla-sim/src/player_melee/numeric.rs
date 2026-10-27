// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
use super::{CreatureDefense, MageAttack, MeleeChances, MeleeError};
use crate::classic_spell::{combat_chance, SpellFloat};
use rustc_apfloat::{ieee::Single, Float, Round, Status, StatusAnd};
const R: Round = Round::NearestTiesToEven;
pub(super) fn checked<T>(value: StatusAnd<T>) -> Result<T, MeleeError> {
    if !(value.status & !Status::INEXACT).is_empty() {
        return Err(MeleeError::Domain("software floating exception"));
    }
    Ok(value.value)
}
pub(super) fn integer(value: i64) -> Result<Single, MeleeError> {
    checked(Single::from_i128_r(i128::from(value), R))
}
pub(super) fn sf(value: SpellFloat) -> Single {
    Single::from_bits(u128::from(value.bits()))
}
pub(super) fn output(value: Single) -> Result<SpellFloat, MeleeError> {
    SpellFloat::from_bits(value.to_bits() as u32).map_err(|_| MeleeError::Domain("binary32 result"))
}
fn uint(value: Single) -> Result<u32, MeleeError> {
    let n = checked(value.to_u128_r(32, Round::TowardZero, &mut false))?;
    u32::try_from(n).map_err(|_| MeleeError::Domain("uint32 conversion"))
}
fn add(a: Single, b: Single) -> Result<Single, MeleeError> {
    checked(a.add_r(b, R))
}
fn sub(a: Single, b: Single) -> Result<Single, MeleeError> {
    checked(a.sub_r(b, R))
}
fn mul(a: Single, b: Single) -> Result<Single, MeleeError> {
    checked(a.mul_r(b, R))
}
fn clamp(v: Single) -> Result<Single, MeleeError> {
    Ok(v.max(Single::ZERO).min(integer(100)?))
}
fn die(v: Single) -> Result<u16, MeleeError> {
    combat_chance(output(v)?).map_err(|_| MeleeError::Domain("chance_u"))
}
pub(super) fn roll_range(min: SpellFloat, max: SpellFloat) -> Result<(u32, u32), MeleeError> {
    let (mut min, mut max) = (sf(min), sf(max));
    if min < Single::ZERO || max < Single::ZERO {
        return Err(MeleeError::Domain("negative damage range"));
    }
    if min > max {
        std::mem::swap(&mut min, &mut max);
    }
    // CalculateDamage explicitly substitutes five only for an actual zero maximum.
    if max == Single::ZERO {
        max = integer(5)?;
    }
    Ok((uint(min)?, uint(max)?))
}
pub(super) fn critical_damage(amount: u32) -> Result<u32, MeleeError> {
    // Source converts to float twice; accepted amount is exactly representable.
    let outgoing = uint(mul(integer(i64::from(amount))?, integer(2)?)?)?;
    Ok(amount.max(uint(mul(integer(i64::from(outgoing))?, integer(1)?)?)?))
}

pub(super) fn chances(a: MageAttack, d: CreatureDefense) -> Result<MeleeChances, MeleeError> {
    let defense = i64::from(d.level) * 5;
    let skill = i64::from(a.weapon_skill);
    let difference = defense - skill;
    let mut miss_difference = difference;
    let mut miss = integer(5)?;
    let factor = if miss_difference > 10 {
        miss = add(miss, mul(integer(10)?, Single::from_bits(0x3dcccccd))?)?;
        miss_difference -= 10;
        miss = add(
            miss,
            mul(integer(miss_difference)?, Single::from_bits(0x3e4ccccd))?,
        )?;
        Single::from_bits(0x3ecccccd)
    } else if miss_difference > 0 {
        Single::from_bits(0x3dcccccd)
    } else {
        Single::from_bits(0x3d23d70a)
    };
    miss = add(miss, mul(integer(miss_difference)?, factor)?)?;
    miss = sub(miss, sf(a.hit_chance))?;
    // Source tests the mutated difference after subtracting the first ten.
    miss = miss
        .max(integer(if miss_difference > 10 { 1 } else { 0 })?)
        .min(integer(100)?);
    let avoidance = |factor| -> Result<u16, MeleeError> {
        die(clamp(add(integer(5)?, mul(integer(difference)?, factor)?)?)?)
    };
    let active = d.can_react && !d.casting_non_melee;
    let dodge = if active && d.can_dodge {
        avoidance(Single::from_bits(if difference > 0 {
            0x3dcccccd
        } else {
            0x3d23d70a
        }))?
    } else {
        0
    };
    let parry = if active && d.can_parry && d.front_facing {
        avoidance(Single::from_bits(if difference > 10 {
            0x3f19999a
        } else if difference > 0 {
            0x3dcccccd
        } else {
            0x3d23d70a
        }))?
    } else {
        0
    };
    let block = if active && d.can_block && d.front_facing {
        avoidance(if difference > 0 {
            Single::ZERO
        } else {
            Single::from_bits(0x3d23d70a)
        })?
    } else {
        0
    };
    let glance = if d.level > 10 {
        let capped_skill = skill.min(i64::from(a.level) * 5);
        die(clamp(integer(
            i64::from(a.level.min(30)) + 2 * (defense - capped_skill),
        )?)?)?
    } else {
        0
    };
    let critical = if sf(a.critical_chance) < Single::from_bits(0x3ba3d70a) {
        0
    } else {
        die(clamp(add(
            sf(a.critical_chance),
            mul(integer(-difference)?, Single::from_bits(0x3e4ccccd))?,
        )?)?)?
    };
    Ok(MeleeChances {
        miss: die(miss)?,
        dodge,
        parry,
        block,
        glance,
        critical,
    })
}

pub(super) fn glance_range(
    a: MageAttack,
    d: CreatureDefense,
) -> Result<Option<(u32, u32)>, MeleeError> {
    let difference = i64::from(d.level) * 5 - i64::from(a.weapon_skill);
    if difference < 0 {
        return Ok(None);
    }
    let high = sub(
        Single::from_bits(0x3f666666),
        mul(Single::from_bits(0x3cf5c28f), integer(difference)?)?,
    )?
    .max(Single::from_bits(0x3e4ccccd))
    .min(Single::from_bits(0x3f7d70a4));
    let low = sub(
        Single::from_bits(0x3f19999a),
        mul(Single::from_bits(0x3d4ccccd), integer(difference)?)?,
    )?
    .max(Single::from_bits(0x3c23d70a))
    .min(Single::from_bits(0x3f19999a).min(high));
    Ok(Some((
        uint(mul(low, integer(100)?)?)?,
        uint(mul(high, integer(100)?)?)?,
    )))
}

pub(super) fn glancing_damage(amount: u32, roll: u32) -> Result<u32, MeleeError> {
    let multiplier = mul(integer(i64::from(roll))?, Single::from_bits(0x3c23d70a))?;
    uint(mul(integer(i64::from(amount))?, multiplier)?)
}
