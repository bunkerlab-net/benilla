// SPDX-License-Identifier: GPL-3.0-or-later
// Source/attribution and domain: ../../CREATURE-INIT.md.
use super::*;
use crate::npc_melee::{creature_damage_range, CreatureDamageInputs};
use rustc_apfloat::{ieee::Single, Float, Round, Status, StatusAnd};
const R: Round = Round::NearestTiesToEven;
fn checked<T>(value: StatusAnd<T>) -> Result<T> {
    if !(value.status & !Status::INEXACT).is_empty() {
        return Err(InitError::Numeric("software binary32 exception"));
    }
    Ok(value.value)
}
fn n(value: u32) -> Result<Single> {
    checked(Single::from_u128_r(u128::from(value), R))
}
fn sf(value: SpellFloat) -> Single {
    Single::from_bits(u128::from(value.bits()))
}
fn out(value: Single) -> Result<SpellFloat> {
    SpellFloat::from_bits(value.to_bits() as u32)
        .map_err(|_| InitError::Numeric("bounded binary32 result"))
}
fn add(a: Single, b: Single) -> Result<Single> {
    checked(a.add_r(b, R))
}
fn sub(a: Single, b: Single) -> Result<Single> {
    checked(a.sub_r(b, R))
}
fn mul(a: Single, b: Single) -> Result<Single> {
    checked(a.mul_r(b, R))
}
fn div(a: Single, b: Single) -> Result<Single> {
    checked(a.div_r(b, R))
}
fn uint(value: Single) -> Result<u32> {
    let result = checked(value.to_u128_r(32, Round::TowardZero, &mut false))?;
    let result = u32::try_from(result).map_err(|_| InitError::Numeric("uint32 conversion"))?;
    if result > MAX_AMOUNT {
        return Err(InitError::Numeric("integer result exceeds one million"));
    }
    Ok(result)
}
fn round(value: Single) -> Result<Single> {
    checked(value.round_to_integral(Round::NearestTiesToAway))
}
fn bounded(value: u32) -> Result<()> {
    if value > MAX_AMOUNT {
        Err(InitError::Invalid("raw integer exceeds one million"))
    } else {
        Ok(())
    }
}
pub(super) fn is_positive(value: SpellFloat) -> bool {
    sf(value) > Single::ZERO
}
pub(super) fn positive(value: SpellFloat, name: &'static str) -> Result<()> {
    if !is_positive(value) {
        Err(InitError::Invalid(name))
    } else {
        Ok(())
    }
}
fn nonnegative(value: SpellFloat, name: &'static str) -> Result<()> {
    if sf(value) < Single::ZERO {
        Err(InitError::Invalid(name))
    } else {
        Ok(())
    }
}
fn bonus(value: Single, coefficient: u32) -> Result<Single> {
    let base = value.min(n(20)?);
    add(base, mul(sub(value, base)?, n(coefficient)?)?)
}
fn interpolate(level: u8, t: &Template, a: u32, b: u32) -> Result<u32> {
    let relative = if t.minimum_level == t.maximum_level {
        Single::ZERO
    } else {
        div(
            n(u32::from(level - t.minimum_level))?,
            n(u32::from(t.maximum_level - t.minimum_level))?,
        )?
    };
    a.min(b)
        .checked_add(uint(mul(relative, n(a.abs_diff(b))?)?)?)
        .ok_or(InitError::Numeric("legacy interpolation overflow"))
}
pub(super) fn stats(t: &Template, row: &ClassLevelRow, rates: Rates) -> Result<Stats> {
    for v in row.attributes.into_iter().chain([
        row.base_health,
        row.base_mana,
        row.base_armor,
        t.legacy.minimum_health,
        t.legacy.maximum_health,
        t.legacy.minimum_mana,
        t.legacy.maximum_mana,
        t.legacy.armor,
        t.legacy.melee_attack_power,
        t.legacy.ranged_attack_power,
    ]) {
        bounded(v)?;
    }
    for v in [
        row.base_melee_attack_power,
        row.base_ranged_attack_power,
        row.base_damage,
        t.damage_multiplier,
        t.damage_variance,
        t.legacy.minimum_melee_damage,
        t.legacy.maximum_melee_damage,
        rates.normal_health,
        rates.normal_damage,
    ] {
        nonnegative(v, "negative damage or configuration dependency")?;
    }
    if (row.base_health == 0 && row.attributes[2] == 0)
        || sf(row.base_damage) <= Single::from_bits(0x3dcccccd)
    {
        return Err(InitError::Unsupported(
            "classlevel row rejected by native getter",
        ));
    }
    if sf(t.damage_variance) > n(2)? {
        return Err(InitError::Unsupported(
            "negative classlevel minimum weapon damage",
        ));
    }
    // Native loader preprocessing occurs before multiplier application.
    if t.resistances.iter().any(|n| n.unsigned_abs() > MAX_AMOUNT) {
        return Err(InitError::Invalid("bounded create resistance"));
    }
    let raw_mana_bonus = uint(bonus(n(row.attributes[3])?, 15)?)?;
    let loaded_mana = row.base_mana - row.base_mana.min(raw_mana_bonus);
    let agility_armor = row.attributes[1]
        .checked_mul(2)
        .ok_or(InitError::Numeric("agility armor"))?;
    let loaded_armor = row.base_armor - row.base_armor.min(agility_armor);
    let rap_bonus = n(row.attributes[1].saturating_sub(10))?;
    let loaded_rap = sub(
        sf(row.base_ranged_attack_power),
        sf(row.base_ranged_attack_power).min(rap_bonus),
    )?;
    let mut create = [SpellFloat::ZERO; 5];
    let mut attrs = [0; 5];
    for index in 0..5 {
        if sf(t.attribute_multipliers[index]) >= Single::ZERO {
            create[index] = out(mul(
                n(row.attributes[index])?,
                sf(t.attribute_multipliers[index]),
            )?)?;
        }
        attrs[index] = uint(sf(create[index]))?;
    }
    let health_multiplier = if is_positive(t.health_multiplier) {
        sf(t.health_multiplier)
    } else {
        n(1)?
    };
    let power_multiplier = if is_positive(t.power_multiplier) {
        sf(t.power_multiplier)
    } else {
        n(1)?
    };
    let damage_multiplier = mul(sf(t.damage_multiplier), sf(rates.normal_damage))?;
    let modern = sf(t.armor_multiplier) >= Single::ZERO;
    let (health, mana, armor, minimum, maximum, base_ap) = if modern {
        if sf(t.health_multiplier) < Single::ZERO || sf(t.power_multiplier) < Single::ZERO {
            return Err(InitError::Unsupported(
                "SelectLevel health/mana sentinel without legacy armor branch",
            ));
        }
        let spread = mul(sf(row.base_damage), div(sf(t.damage_variance), n(2)?)?)?;
        (
            row.base_health,
            loaded_mana,
            round(mul(n(loaded_armor)?, sf(t.armor_multiplier))?)?,
            sub(sf(row.base_damage), spread)?,
            add(sf(row.base_damage), spread)?,
            sf(row.base_melee_attack_power),
        )
    } else {
        // The native legacy path applies the same damage multiplier here AND in
        // UpdateDamagePhysical. Do not silently replace that with one multiply.
        (
            interpolate(
                row.level,
                t,
                t.legacy.minimum_health,
                t.legacy.maximum_health,
            )?,
            interpolate(row.level, t, t.legacy.minimum_mana, t.legacy.maximum_mana)?,
            n(t.legacy.armor)?,
            mul(sf(t.legacy.minimum_melee_damage), damage_multiplier)?,
            mul(sf(t.legacy.maximum_melee_damage), damage_multiplier)?,
            n(t.legacy.melee_attack_power)?,
        )
    };
    let create_health = uint(mul(n(health)?, sf(rates.normal_health))?)?.max(1);
    let stamina_bonus = sub(bonus(n(attrs[2])?, 10)?, bonus(sf(create[2]), 10)?)?;
    let max_health = uint(round(
        add(
            mul(n(create_health)?, health_multiplier)?,
            mul(stamina_bonus, health_multiplier)?,
        )?
        .max(n(1)?),
    )?)?;
    let intellect_bonus = if mana > 0 {
        bonus(n(attrs[3])?, 15)?
    } else {
        Single::ZERO
    };
    // TOTAL_PCT is set for the active power type only, not all creature mana.
    let mana_multiplier = if power_type(t.unit_class)? == PowerType::Mana {
        power_multiplier
    } else {
        n(1)?
    };
    let max_mana = uint(round(mul(
        add(n(mana)?, intellect_bonus)?,
        mana_multiplier,
    )?)?)?;
    let effective_armor = uint(add(armor, mul(n(attrs[1])?, n(2)?)?)?)?;
    // UpdateAllStats first visits strength while a fresh creature's agility is
    // still zero. Reject a negative/overflowed transient rogue AP cast even if
    // the second UpdateEntry pass would have produced a representable result.
    if t.unit_class == 4 {
        uint(add(
            sub(sub(n(attrs[0])?, sf(create[0]))?, sf(create[1]))?,
            mul(base_ap, damage_multiplier)?,
        )?)?;
    }
    // The ranged update executes even for our mainhand-only creature subset.
    let base_rap = if modern {
        loaded_rap
    } else {
        n(t.legacy.ranged_attack_power)?
    };
    uint(add(
        sub(n(attrs[1])?, sf(create[1]))?,
        mul(base_rap, damage_multiplier)?,
    )?)?;
    let damage = creature_damage_range(CreatureDamageInputs {
        class: stat_class(t.unit_class)?,
        strength: attrs[0],
        agility: attrs[1],
        create_strength: create[0],
        create_agility: create[1],
        base_attack_power: out(base_ap)?,
        damage_multiplier: out(damage_multiplier)?,
        base_weapon_minimum: out(minimum)?,
        base_weapon_maximum: out(maximum)?,
        weapon_unusable: false,
    })
    .map_err(|_| InitError::Numeric("creature mainhand damage/AP"))?;
    let mut resistances = [0; 7];
    resistances[0] = effective_armor as i32;
    resistances[1..].copy_from_slice(&t.resistances);
    Ok(Stats {
        create_attributes: create,
        attributes: attrs,
        create_health,
        create_mana: mana,
        base_armor: out(armor)?,
        armor: effective_armor as i32,
        resistances,
        base_melee_attack_power: out(base_ap)?,
        loaded_base_ranged_attack_power: out(loaded_rap)?,
        damage_multiplier: out(damage_multiplier)?,
        base_weapon_minimum: out(minimum)?,
        base_weapon_maximum: out(maximum)?,
        damage,
        resources: Resources {
            health: max_health,
            max_health,
            mana: max_mana,
            max_mana,
        },
    })
}
pub(super) fn model(t: &Template, row: &ModelRow, scale: SpellFloat) -> Result<Model> {
    if row.gender > 2 {
        return Err(InitError::Unsupported("model gender loader repair"));
    }
    for value in [
        row.bounding_radius,
        row.combat_reach,
        row.speed_walk,
        row.speed_run,
        t.speed_walk,
        t.speed_run,
    ] {
        nonnegative(value, "negative model/speed input")?;
    }
    // The native model loader may repair zero player-race radii using ChrRaces.
    // This raw-row boundary has no race binding, so do not silently skip it.
    positive(
        row.bounding_radius,
        "zero model radius may require ChrRaces repair",
    )?;
    let walk_rate = if t.speed_walk.bits() & 0x7fff_ffff != 0 {
        t.speed_walk
    } else {
        row.speed_walk
    };
    let run_rate = if t.speed_run.bits() & 0x7fff_ffff != 0 {
        t.speed_run
    } else {
        row.speed_run
    };
    positive(walk_rate, "zero native walk speed")?;
    positive(run_rate, "zero native run speed")?;
    // Unit::SetSpeedRate clamps the selected rate before GetSpeed multiplies it.
    let minimum_rate = Single::from_bits(0x3c23d70a); // 0.01f
    let walk_rate = sf(walk_rate).max(minimum_rate);
    let run_rate = sf(run_rate).max(minimum_rate);
    Ok(Model {
        display_id: row.id,
        gender: row.gender,
        scale,
        bounding_radius: row.bounding_radius,
        combat_reach: row.combat_reach,
        walk_speed: out(mul(walk_rate, Single::from_bits(0x40200000))?)?,
        run_speed: out(mul(run_rate, n(7)?)?)?,
    })
}
pub(super) fn mana_regen(maximum: u32) -> Result<u32> {
    uint(div(n(maximum)?, n(3)?)?)
}
