// SPDX-License-Identifier: GPL-3.0-or-later
// Player::UpdateCombatSkills/UpdateDefense/UpdateSkill, pinned 8ec338a1.
use super::{
    numeric::{checked, output},
    MeleeError,
};
use crate::classic_spell::SpellFloat;
use rustc_apfloat::{
    ieee::{Double, Single},
    Float, FloatConvert, Round,
};
const R: Round = Round::NearestTiesToEven;
/// A semantic rand_chance draw in [0,100), encoded as IEEE binary64. Source
/// roll_chance_f promotes the f32 chance and compares STRICTLY greater-than.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PercentRoll(u64);
impl PercentRoll {
    pub fn from_bits(bits: u64) -> Result<Self, MeleeError> {
        let n = Double::from_bits(u128::from(bits));
        if n.is_nan()
            || !n.is_finite()
            || n < Double::ZERO
            || n >= Double::from_bits(0x4059000000000000)
        {
            return Err(MeleeError::Roll("binary64 percentage [0,100)"));
        }
        Ok(Self(bits))
    }
    /// Deterministic semantic distribution for callers: uniform integer
    /// 0..2^53-1, divided by 2^53, then binary64 *100. Not CMaNGOS's MT state.
    pub fn from_fraction53(n: u64) -> Result<Self, MeleeError> {
        if n >= (1u64 << 53) {
            return Err(MeleeError::Roll("53-bit uniform fraction"));
        }
        let n = checked(Double::from_u128_r(u128::from(n), R))?;
        let n = checked(n.mul_r(Double::from_bits(0x3ca0000000000000), R))?;
        let n = checked(n.mul_r(Double::from_bits(0x4059000000000000), R))?;
        Self::from_bits(n.to_bits() as u64)
    }
    pub const fn bits(self) -> u64 {
        self.0
    }
    pub fn succeeds(self, chance: SpellFloat) -> Result<bool, MeleeError> {
        let c = Single::from_bits(u128::from(chance.bits()));
        if c < Single::ZERO || c > Single::from_bits(0x42c80000) {
            return Err(MeleeError::Domain("percent chance"));
        }
        let c: Double = checked(c.convert_r(R, &mut false))?;
        Ok(c > Double::from_bits(u128::from(self.0)))
    }
}
/// PURE live defense value/max (GetSkillValuePure/GetSkillMaxPure), plus the
/// explicitly configured uint16-admitted CONFIG_UINT32_SKILL_GAIN_DEFENSE.
/// An absent/deleted skill is represented by value=max=0, as source reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DefenseSkill {
    pub value: u16,
    pub maximum: u16,
    pub gain: u16,
}
/// Minimal exact starter branch: room<7. Source gap>=7 calls std::log, whose
/// architecture-independent port is deliberately not approximated here.
/// None means no source random draw; Some still consumes a draw when gain=0.
pub fn defense_gain_chance(
    level: u8,
    skill: DefenseSkill,
) -> Result<Option<SpellFloat>, MeleeError> {
    if !(1..=60).contains(&level) {
        return Err(MeleeError::Domain("defense gain level"));
    }
    if skill.maximum == 0 || skill.value >= skill.maximum {
        return Ok(None);
    }
    let room = u32::from(skill.maximum - skill.value);
    if room >= 7 {
        return Err(MeleeError::Unsupported(
            "logarithmic defense skill catch-up",
        ));
    }
    let numerator = checked(Double::from_u128_r(u128::from(room * room), R))?;
    let maximum = checked(Double::from_u128_r(u128::from(skill.maximum), R))?;
    let denominator = checked(Double::from_bits(0x401c000000000000).mul_r(maximum, R))?; //7.0
    let base = checked(numerator.div_r(denominator, R))?
        .max(Double::ZERO)
        .min(Double::from_bits(0x3ff0000000000000));
    let final_chance = checked(base.mul_r(Double::from_bits(0x4059000000000000), R))?;
    let narrowed: Single = checked(final_chance.convert_r(R, &mut false))?;
    Ok(Some(output(narrowed)?))
}
/// Health-independent skill projection. Call only after the source proc gate
/// (surviving mage/non-critter attacker), and after durability/pushback callbacks.
/// It never mutates caller state; extra/missing draws are transactional errors.
pub fn resolve_defense_gain(
    level: u8,
    skill: DefenseSkill,
    roll: Option<PercentRoll>,
) -> Result<u16, MeleeError> {
    let success = match (defense_gain_chance(level, skill)?, roll) {
        (None, None) => false,
        (Some(chance), Some(roll)) => roll.succeeds(chance)?,
        _ => return Err(MeleeError::Roll("missing/unexpected defense gain draw")),
    };
    if success && skill.gain != 0 && skill.value != 0 {
        Ok((u32::from(skill.value) + u32::from(skill.gain)).min(u32::from(skill.maximum)) as u16)
    } else {
        Ok(skill.value)
    }
}
