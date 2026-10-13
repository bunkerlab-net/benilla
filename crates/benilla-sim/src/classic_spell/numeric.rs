// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1: Object.cpp, Spell.cpp, SpellMgr.cpp, Unit.cpp.
// Attribution, source grants, numeric domains and exclusions: ../../SPELLS.md.
use super::{SpellError, MAX_AMOUNT, MAX_TIME};
use rustc_apfloat::{
    ieee::{Double, Single},
    Float, FloatConvert, Round, Status, StatusAnd,
};
const R: Round = Round::NearestTiesToEven;

fn checked<T>(value: StatusAnd<T>) -> Result<T, SpellError> {
    if !(value.status & !Status::INEXACT).is_empty() {
        return Err(SpellError::Domain("binary32 exception"));
    }
    Ok(value.value)
}
fn integer(value: i64) -> Result<Single, SpellError> {
    checked(Single::from_i128_r(i128::from(value), R))
}
fn signed(value: Single) -> Result<i32, SpellError> {
    let n = checked(value.to_i128_r(32, Round::TowardZero, &mut false))?;
    i32::try_from(n).map_err(|_| SpellError::Domain("integer conversion"))
}
/// A finite normal binary32 or either signed zero; absolute value <= 1,000,000.
/// Encodings, rather than decimal/host floats, belong in simulation replay input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpellFloat(u32);
impl SpellFloat {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(0x3f80_0000);
    pub fn from_bits(bits: u32) -> Result<Self, SpellError> {
        let magnitude = bits & 0x7fff_ffff;
        if magnitude > 0x4974_2400 || (magnitude != 0 && magnitude < 0x0080_0000) {
            return Err(SpellError::Domain("float encoding"));
        }
        Ok(Self(bits))
    }
    pub const fn bits(self) -> u32 {
        self.0
    }
    fn value(self) -> Single {
        Single::from_bits(u128::from(self.0))
    }
}
/// Original effect-value fields, before base-dice addition. No combo points,
/// spellmods, creature scaling, overridden base points or scripted values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectValue {
    pub base_points: i32,
    pub base_dice: i32,
    pub die_sides: i32,
    pub points_per_level: SpellFloat,
    pub dice_per_level: SpellFloat,
    pub base_level: u8,
    pub spell_level: u8,
    /// Zero means uncapped. The source uses if/else-if, not two independent clamps.
    pub max_level: u8,
}
impl EffectValue {
    fn scaled(self, rank: u16) -> Result<(Single, i32), SpellError> {
        if rank == 0
            || rank > 315
            || [self.base_points, self.base_dice, self.die_sides]
                .into_iter()
                .any(|v| v.unsigned_abs() > MAX_AMOUNT)
        {
            return Err(SpellError::Domain("effect inputs"));
        }
        let mut base = integer(i64::from(self.base_points))?;
        let mut sides = self.die_sides;
        if self.points_per_level.value() != Single::ZERO {
            let mut level = i32::from(rank / 5);
            if self.max_level > 0 && level > i32::from(self.max_level) {
                level = i32::from(self.max_level);
            } else if level < i32::from(self.base_level) {
                level = i32::from(self.base_level);
            }
            level -= i32::from(self.spell_level);
            let level = integer(i64::from(level))?;
            base = checked(base.add_r(checked(level.mul_r(self.points_per_level.value(), R))?, R))?;
            sides = sides
                .checked_add(signed(checked(
                    level.mul_r(self.dice_per_level.value(), R),
                )?)?)
                .ok_or(SpellError::Domain("effect die overflow"))?;
        }
        Ok((base, sides))
    }
    /// Exact inclusive irand range; None means upstream consumes no effect die.
    pub fn roll_range(self, rank: u16) -> Result<Option<(i32, i32)>, SpellError> {
        let (_, sides) = self.scaled(rank)?;
        Ok(if sides == 0 || sides == 1 {
            None
        } else {
            Some((self.base_dice.min(sides), self.base_dice.max(sides)))
        })
    }
    /// Source CalculateSpellEffectValue sequence, nearest-even after each binary32
    /// operation and toward-zero at the returned int32. Extra/missing rolls fail.
    pub fn evaluate(self, rank: u16, roll: Option<i32>) -> Result<i32, SpellError> {
        let (base, _) = self.scaled(rank)?;
        let addition = match self.roll_range(rank)? {
            None if roll.is_none() => self.base_dice,
            Some((lo, hi)) => match roll {
                Some(value) if (lo..=hi).contains(&value) => value,
                _ => return Err(SpellError::Roll("effect inclusive die")),
            },
            _ => return Err(SpellError::Roll("unexpected effect die")),
        };
        let value = signed(checked(base.add_r(integer(i64::from(addition))?, R))?)?;
        if value.unsigned_abs() > MAX_AMOUNT {
            return Err(SpellError::Domain("effect result"));
        }
        Ok(value)
    }
}
/// GetSpellCastTime's unmodified, non-ranged player branch. Missing rows are an
/// error at the compiler boundary, never substituted by the source's zero fallback.
pub fn cast_time(
    base_ms: i32,
    per_level_ms: i32,
    minimum_ms: i32,
    rank: u16,
    base_level: u8,
) -> Result<u32, SpellError> {
    if rank == 0 || rank > 315 {
        return Err(SpellError::Domain("spell rank"));
    }
    let value = i64::from(base_ms)
        + i64::from(per_level_ms) * (i64::from(rank / 5) - i64::from(base_level));
    let value = value.max(i64::from(minimum_ms)).max(0);
    if value > i64::from(MAX_TIME) {
        return Err(SpellError::Domain("cast time"));
    }
    Ok(value as u32)
}
/// CalculatePowerCost's mana branch with no school/spell/aura modifiers. Percent
/// uses create mana, not maximum mana; the final source float multiplication by
/// 1.0 still rounds to binary32 before truncation.
pub fn mana_cost(
    flat: u32,
    per_level: u32,
    percent: u32,
    create_mana: u32,
    rank: u16,
    base_level: u8,
) -> Result<u32, SpellError> {
    if [flat, per_level, create_mana]
        .into_iter()
        .any(|v| v > MAX_AMOUNT)
        || percent > 100
        || rank == 0
        || rank > 315
    {
        return Err(SpellError::Domain("power cost"));
    }
    let pct = percent
        .checked_mul(create_mana)
        .ok_or(SpellError::Domain("power percent overflow"))?
        / 100;
    let n = i64::from(flat)
        + i64::from(per_level) * (i64::from(rank / 5) - i64::from(base_level))
        + i64::from(pct);
    if !(-i64::from(MAX_AMOUNT)..=i64::from(MAX_AMOUNT)).contains(&n) {
        return Err(SpellError::Domain("power arithmetic"));
    }
    Ok(signed(integer(n)?)?.max(0) as u32)
}
fn widen(v: Single) -> Double {
    let value: StatusAnd<Double> = v.convert_r(R, &mut false);
    debug_assert!(value.status.is_empty());
    value.value
}
// Adjacent binary32 midpoint has <=25 significant bits. Its square is exact in
// binary64 (<=50 bits). Integer software APFloat gives correctly rounded sqrt
// without a platform libm or an unchecked host-float intermediate.
fn sqrt_binary32(x: Single) -> Single {
    if x == Single::ZERO {
        return Single::ZERO;
    }
    let xwide = widen(x);
    let mut lo = 0u32;
    let mut hi = 0x4b00_0000u32; // 2^23 > sqrt(max accepted squared distance)
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        let y = widen(Single::from_bits(u128::from(mid)));
        if y.mul_r(y, R).value <= xwide {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let low = widen(Single::from_bits(u128::from(lo)));
    let high = widen(Single::from_bits(u128::from(hi)));
    let half = Double::from_bits(0x3fe0_0000_0000_0000);
    let mid = low.add_r(high, R).value.mul_r(half, R).value;
    let midpoint_squared = mid.mul_r(mid, R).value;
    let result = if xwide < midpoint_squared || (xwide == midpoint_squared && lo & 1 == 0) {
        lo
    } else {
        hi
    };
    Single::from_bits(u128::from(result))
}
/// AddUnitTarget: sqrt(raw squared distance), clamp to 5 yards, divide by speed,
/// multiply by binary32 1000, then floor. Nonzero positive speed <=1,000,000;
/// squared distance 0..=1,000,000. Negative zero is accepted as zero.
pub fn projectile_delay(
    distance_squared: SpellFloat,
    speed: SpellFloat,
) -> Result<u32, SpellError> {
    if distance_squared.value() < Single::ZERO || speed.value() <= Single::ZERO {
        return Err(SpellError::Domain("projectile geometry"));
    }
    let distance = sqrt_binary32(distance_squared.value()).max(integer(5)?);
    let ms = checked(checked(distance.div_r(speed.value(), R))?.mul_r(integer(1000)?, R))?;
    let ms = signed(ms)?;
    if !(0..=MAX_TIME as i32).contains(&ms) {
        return Err(SpellError::Domain("travel time"));
    }
    Ok(ms as u32)
}
/// The no-modifier magic crit amount: source uint32(float(amount) * 1.5f).
pub fn critical_damage(amount: u32) -> Result<u32, SpellError> {
    if amount > MAX_AMOUNT {
        return Err(SpellError::Domain("damage"));
    }
    let value = signed(checked(
        integer(i64::from(amount))?.mul_r(Single::from_bits(0x3fc0_0000), R),
    )?)?;
    if value < 0 || value as u32 > MAX_AMOUNT {
        return Err(SpellError::Domain("critical damage"));
    }
    Ok(value as u32)
}
/// Util::chance_u: multiply in binary32, then roundf (half away from zero).
/// This is deliberately not the strict > comparison used by roll_chance_combat.
pub fn combat_chance(percent: SpellFloat) -> Result<u16, SpellError> {
    let value = percent.value();
    if value < Single::ZERO || value > integer(100)? {
        return Err(SpellError::Domain("combat chance"));
    }
    let scaled = checked(value.mul_r(integer(100)?, R))?;
    let rounded = checked(scaled.round_to_integral(Round::NearestTiesToAway))?;
    Ok(signed(rounded)? as u16)
}
/// CalculateSpellMissChance/CalculateSpellResistChance for an ordinary NPC with
/// zero school resistance and no modifiers, mechanic/dispel resistance or binary
/// behavior. Levels <=20 make source level-resistance contribution zero because
/// its skill denominator has a 100-point floor. The resolved target base miss
/// percentage remains mandatory; this helper never invents it from the spell ID.
pub fn zero_resistance_npc_miss(
    caster_level: u8,
    target_level: u8,
    base_miss: SpellFloat,
) -> Result<u16, SpellError> {
    if !(1..=20).contains(&caster_level) || !(1..=20).contains(&target_level) {
        return Err(SpellError::Domain("zero-resistance level domain"));
    }
    let mut chance = base_miss.value();
    if chance < Single::ZERO || chance > integer(100)? {
        return Err(SpellError::Domain("base miss"));
    }
    if chance < Single::from_bits(0x3ba3_d70a) {
        return Ok(0);
    } // 0.005f
    let difference = i32::from(target_level) - i32::from(caster_level);
    if difference > 2 {
        chance = checked(chance.add_r(integer(2)?, R))?;
        chance = checked(chance.add_r(integer(i64::from((difference - 2) * 11))?, R))?;
    } else {
        chance = checked(chance.add_r(integer(i64::from(difference))?, R))?;
    }
    let chance = chance.max(integer(1)?).min(integer(100)?);
    combat_chance(SpellFloat(chance.to_bits() as u32))
}
