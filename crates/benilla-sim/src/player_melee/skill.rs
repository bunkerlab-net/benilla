// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Player::UpdateCombatSkills, weapon branch.
use crate::classic_spell::SpellFloat;
use crate::npc_melee::{combat_skill_base_chance, MeleeError, PercentRoll};
use rustc_apfloat::{
    ieee::{Double, Single},
    Float, FloatConvert, Round, Status, StatusAnd,
};
const R: Round = Round::NearestTiesToEven;

fn checked<T>(value: StatusAnd<T>) -> Result<T, MeleeError> {
    if !(value.status & !Status::INEXACT).is_empty() {
        return Err(MeleeError::Domain("weapon skill floating exception"));
    }
    Ok(value.value)
}
fn integer(value: u32) -> Result<Double, MeleeError> {
    checked(Double::from_u128_r(u128::from(value), R))
}

/// PURE GetSkillValuePure/GetSkillMaxPure and explicitly configured
/// CONFIG_UINT32_SKILL_GAIN_WEAPON, admitted here through a uint16 bound.
/// This numeric projection does not execute UpdateWeaponSkill's victim,
/// shapeshift, weapon/fishing-pole gates or UpdateAllCritPercentages callback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeaponSkill {
    pub value: u16,
    pub maximum: u16,
    pub gain: u16,
}

/// Source levels 1..60, canonical shared binary64 log table for room>=7.
/// Errors/PercentRoll deliberately share the existing combat-skill contract.
/// None consumes no draw. Some consumes one even for zero gain or zero value.
/// Intellect is the current resolved STAT_INTELLECT, never a default bonus.
pub fn weapon_gain_chance(
    level: u8,
    intellect: u32,
    skill: WeaponSkill,
) -> Result<Option<SpellFloat>, MeleeError> {
    if !(1..=60).contains(&level) || intellect > 1_000_000 {
        return Err(MeleeError::Domain("weapon skill level/intellect"));
    }
    let Some(base) = combat_skill_base_chance(level, skill.value, skill.maximum)? else {
        return Ok(None);
    };
    let tenth = Double::from_bits(0x3fb999999999999a);
    let ratio = checked(integer(intellect)?.div_r(integer(750)?, R))?;
    let bonus = checked(tenth.mul_r(ratio, R))?;
    let level_ratio = checked(integer(60)?.div_r(integer(u32::from(level))?, R))?;
    let bonus = checked(bonus.mul_r(level_ratio, R))?
        .max(Double::ZERO)
        .min(tenth);
    let chance = checked(base.add_r(bonus, R))?
        .max(Double::ZERO)
        .min(integer(1)?);
    // The source's 100.0f is promoted to double by the left operand. Narrow
    // only after multiplication, not before adding the intellect contribution.
    let chance = checked(chance.mul_r(integer(100)?, R))?;
    let chance: Single = checked(chance.convert_r(R, &mut false))?;
    Ok(Some(
        SpellFloat::from_bits(chance.to_bits() as u32)
            .map_err(|_| MeleeError::Domain("weapon skill chance"))?,
    ))
}

/// Numeric callback decision. A successful roll requires UpdateWeaponSkill
/// even when UpdateSkill leaves its value unchanged (zero gain/value).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeaponSkillResolution {
    value: u16,
    update_weapon_skill: bool,
}
impl WeaponSkillResolution {
    pub fn value(self) -> u16 {
        self.value
    }
    pub fn update_weapon_skill(self) -> bool {
        self.update_weapon_skill
    }
}

/// Health-independent UpdateSkill value projection; caller must establish the
/// actual attack proc gate and execute UpdateAllCritPercentages in source order.
/// In particular, lethal attacks do not suppress the attacker's skill callback.
pub fn resolve_weapon_gain(
    level: u8,
    intellect: u32,
    skill: WeaponSkill,
    roll: Option<PercentRoll>,
) -> Result<WeaponSkillResolution, MeleeError> {
    let success = match (weapon_gain_chance(level, intellect, skill)?, roll) {
        (None, None) => false,
        (Some(chance), Some(roll)) => roll.succeeds(chance)?,
        _ => return Err(MeleeError::Roll("missing/unexpected weapon gain draw")),
    };
    let value = if success && skill.gain != 0 && skill.value != 0 {
        (u32::from(skill.value) + u32::from(skill.gain)).min(u32::from(skill.maximum)) as u16
    } else {
        skill.value
    };
    Ok(WeaponSkillResolution {
        value,
        update_weapon_skill: success,
    })
}
