// SPDX-License-Identifier: GPL-3.0-or-later
// Pinned CMaNGOS arithmetic; attribution and exact input derivation: ../../NPC-MELEE.md.
use super::{CreatureAttack, MageDefense, MeleeChances, MeleeError, MAX_AMOUNT};
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
fn div(a: Single, b: Single) -> Result<Single, MeleeError> {
    checked(a.div_r(b, R))
}
fn clamp(v: Single) -> Result<Single, MeleeError> {
    Ok(v.max(Single::ZERO).min(integer(100)?))
}
fn die(v: Single) -> Result<u16, MeleeError> {
    combat_chance(output(v)?).map_err(|_| MeleeError::Domain("chance_u"))
}
/// Player::GetDodgeFromAgility (mage) and UpdateDodgePercentage. Does not use the
/// UI percentage, which already contains a defense-skill adjustment.
pub fn mage_base_dodge(level: u8, agility: u32) -> Result<SpellFloat, MeleeError> {
    if !(1..=60).contains(&level) || agility > MAX_AMOUNT {
        return Err(MeleeError::Domain("mage dodge inputs"));
    }
    let level = integer(i64::from(level))?;
    let rate = add(
        div(
            mul(Single::from_bits(0x414e6666), sub(integer(60)?, level)?)?,
            integer(59)?,
        )?,
        div(mul(integer(20)?, sub(level, integer(1)?)?)?, integer(59)?)?,
    )?;
    output(add(
        Single::from_bits(0x40500000),
        div(integer(i64::from(agility))?, rate)?,
    )?)
}
/// CalculateEffectiveDodge/Parry/BlockChance's shared PLAYER arithmetic.
/// Eligibility (casting, facing, weapon/shield, CanParry/CanBlock) is separate.
/// No positive NPC-only skill factors or ranged branches are silently assumed.
pub fn player_avoidance_chance(
    base: SpellFloat,
    defense: u16,
    attacker_skill: u16,
) -> Result<SpellFloat, MeleeError> {
    let base = sf(base);
    if base < Single::from_bits(0x3ba3d70a) {
        return Ok(SpellFloat::ZERO);
    }
    output(clamp(add(
        base,
        mul(
            integer(i64::from(defense) - i64::from(attacker_skill))?,
            Single::from_bits(0x3d23d70a),
        )?,
    )?)?)
}
pub(super) fn chances(a: CreatureAttack, d: MageDefense) -> Result<MeleeChances, MeleeError> {
    let skill = u16::from(a.level) * 5;
    let diff = i64::from(d.defense_skill) - i64::from(skill);
    let factor = Single::from_bits(0x3d23d70a); // 0.04f
    let miss = add(integer(5)?, mul(integer(diff)?, factor)?)?
        .max(integer(if diff > 10 { 1 } else { 0 })?)
        .min(integer(100)?);
    if !d.standing {
        return Ok(MeleeChances {
            miss: die(miss)?,
            dodge: 0,
            parry: 0,
            block: 0,
            critical: 10000,
            crushing: 0,
        });
    }
    let dodge = if d.casting_non_melee {
        SpellFloat::ZERO
    } else {
        player_avoidance_chance(mage_base_dodge(d.level, d.agility)?, d.defense_skill, skill)?
    };
    let critical = clamp(add(integer(5)?, mul(integer(-diff)?, factor)?)?)?;
    let deficit = i64::from(skill) - i64::from(d.defense_skill.min(u16::from(d.level) * 5));
    let crush = if !a.no_crushing_blows && deficit >= 15 {
        clamp(integer(2 * deficit - 15)?)?
    } else {
        Single::ZERO
    };
    Ok(MeleeChances {
        miss: die(miss)?,
        dodge: die(sf(dodge))?,
        parry: 0,
        block: 0,
        critical: die(critical)?,
        crushing: die(crush)?,
    })
}
pub(super) fn roll_range(min: SpellFloat, max: SpellFloat) -> Result<(u32, u32), MeleeError> {
    let (mut min, mut max) = (sf(min), sf(max));
    if min < Single::ZERO || max < Single::ZERO {
        return Err(MeleeError::Domain("negative damage range"));
    }
    if min > max {
        std::mem::swap(&mut min, &mut max);
    }
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreatureStatClass {
    Warrior,
    Paladin,
    Rogue,
    Mage,
}
/// Explicit post-SelectLevel inputs; neutral UnitMods only. Includes current
/// effective stats and floating create stats because source subtracts them before
/// assigning/truncating its attack-power field. Rank rate is already represented
/// in damage_multiplier. Do not substitute template raw damage for weapon range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreatureDamageInputs {
    pub class: CreatureStatClass,
    pub strength: u32,
    pub agility: u32,
    pub create_strength: SpellFloat,
    pub create_agility: SpellFloat,
    pub base_attack_power: SpellFloat,
    pub damage_multiplier: SpellFloat,
    pub base_weapon_minimum: SpellFloat,
    pub base_weapon_maximum: SpellFloat,
    /// hasWeapon(BASE_ATTACK) && !hasWeaponForAttack(BASE_ATTACK).
    pub weapon_unusable: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DamageRange {
    pub minimum: SpellFloat,
    pub maximum: SpellFloat,
    pub attack_power: u32,
}
/// Creature::UpdateAttackPowerAndDamage + UpdateDamagePhysical, no UnitMods or
/// auras. The source stores/truncates uint32 AP BEFORE GetTotalAttackPowerValue.
pub fn creature_damage_range(i: CreatureDamageInputs) -> Result<DamageRange, MeleeError> {
    if i.strength > MAX_AMOUNT || i.agility > MAX_AMOUNT {
        return Err(MeleeError::Domain("creature stats"));
    }
    for v in [
        i.create_strength,
        i.create_agility,
        i.base_attack_power,
        i.damage_multiplier,
        i.base_weapon_minimum,
        i.base_weapon_maximum,
    ] {
        if sf(v) < Single::ZERO {
            return Err(MeleeError::Domain("negative creature damage dependency"));
        }
    }
    let strength = integer(i64::from(i.strength))?;
    let agility = integer(i64::from(i.agility))?;
    let stat = match i.class {
        CreatureStatClass::Mage => strength,
        CreatureStatClass::Rogue => sub(
            add(sub(strength, sf(i.create_strength))?, agility)?,
            sf(i.create_agility),
        )?,
        _ => mul(sub(strength, sf(i.create_strength))?, integer(2)?)?,
    };
    let ap = uint(add(
        stat,
        mul(sf(i.base_attack_power), sf(i.damage_multiplier))?,
    )?)?;
    if ap > MAX_AMOUNT {
        return Err(MeleeError::Domain("creature attack power"));
    }
    let base = div(integer(i64::from(ap))?, integer(14)?)?;
    let conditional = if i.weapon_unusable {
        Single::from_bits(0x3f000000)
    } else {
        integer(1)?
    };
    let evaluate = |weapon| {
        output(mul(
            add(base, mul(sf(weapon), sf(i.damage_multiplier))?)?,
            conditional,
        )?)
    };
    Ok(DamageRange {
        minimum: evaluate(i.base_weapon_minimum)?,
        maximum: evaluate(i.base_weapon_maximum)?,
        attack_power: ap,
    })
}
