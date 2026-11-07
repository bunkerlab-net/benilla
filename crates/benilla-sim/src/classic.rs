// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from CMaNGOS Classic StatSystem.cpp and Unit.cpp at
// 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f. See ../PROVENANCE.md.
// Copyright: CMaNGOS Project contributors; see ../licenses/AUTHORS.md.
// Upstream grants GPL version 2 or (at your option) any later version.
// This modified Rust translation is distributed under GPL version 3 or later,
// WITHOUT ANY WARRANTY, including MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
//! Bounded, callable Classic stat and physical-armor formulas, not a combat engine.
//!
//! Inputs are resolved stats/modifiers, not native rows. Callers must establish
//! class/level table selection, modifier provenance, physical damage school and
//! eligibility. No class values, database rows, aura execution or hit rolls live here.
//!
//! Reference helpers preserve the upstream sequence of IEEE-754 binary32
//! operations, including rounding at each operation. Do not reassociate, use
//! `mul_add`, enable fast math, or replace these expressions by rational algebra.
//! Finite input bounds exclude NaNs, infinities, subnormal coefficients and
//! out-of-range integer conversions. Integer results are checked before casting.
//! Committed independent C++ oracle vectors must pass on every supported target.
//! `armor_reduced_damage` is the versioned rational authority.
//! `software_armor_reduction` and `software_armor_reduced_damage` reproduce the
//! source binary32 operations using integer software arithmetic, never host floats.
//! The other helpers are reference/import-validation kernels, not runtime authority.

mod softfloat;
pub use softfloat::{
    software_armor_reduced_damage, software_armor_reduction, SOFTWARE_ARMOR_POLICY_ID,
};

/// Version of the rational arithmetic/validation contract; include it in rules hashes.
pub const NUMERIC_POLICY_VERSION: u32 = 1;
const MAX_VALUE: u32 = 1_000_000_000;
const MAX_STAT: i32 = 1_000_000;

/// An unsupported numeric domain is an error, never a clipped native value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassicError {
    pub field: &'static str,
}
impl std::fmt::Display for ClassicError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Classic formula input/result outside supported domain: {}",
            self.field
        )
    }
}
impl std::error::Error for ClassicError {}

/// Already-resolved UnitMods slots; percentages are factors (1.0 means unchanged).
/// This type does not apply, stack, remove or infer auras/equipment modifiers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StatModifiers {
    pub base_value: f32,
    pub base_percent: f32,
    pub total_value: f32,
    pub total_percent: f32,
}
impl Default for StatModifiers {
    fn default() -> Self {
        Self {
            base_value: 0.0,
            base_percent: 1.0,
            total_value: 0.0,
            total_percent: 1.0,
        }
    }
}
impl StatModifiers {
    fn validate(self) -> Result<Self, ClassicError> {
        finite(
            self.base_value,
            -(MAX_VALUE as f32),
            MAX_VALUE as f32,
            "base_value",
        )?;
        finite(
            self.total_value,
            -(MAX_VALUE as f32),
            MAX_VALUE as f32,
            "total_value",
        )?;
        finite(self.base_percent, 0.0, 1000.0, "base_percent")?;
        finite(self.total_percent, 0.0, 1000.0, "total_percent")?;
        Ok(self)
    }
}

fn finite(value: f32, min: f32, max: f32, field: &'static str) -> Result<f32, ClassicError> {
    if !value.is_finite() || value.is_subnormal() || value < min || value > max {
        Err(ClassicError { field })
    } else {
        Ok(value)
    }
}
fn bounded(value: u32, field: &'static str) -> Result<f32, ClassicError> {
    if value > MAX_VALUE {
        Err(ClassicError { field })
    } else {
        Ok(value as f32)
    }
}
fn stat(value: i32, field: &'static str) -> Result<f32, ClassicError> {
    if !(-MAX_STAT..=MAX_STAT).contains(&value) {
        Err(ClassicError { field })
    } else {
        Ok(value as f32)
    }
}
fn rounded(value: f32) -> Result<u32, ClassicError> {
    let value = finite(value, 0.0, MAX_VALUE as f32, "result")?;
    // std::round in CMaNGOS rounds halfway cases away from zero; this is not XP's policy.
    let value = value.round();
    Ok(value as u32)
}
fn stamina_bonus(stamina: f32) -> f32 {
    let base = if stamina < 20.0 { stamina } else { 20.0 };
    let more = stamina - base;
    base + more * 10.0
}
fn intellect_bonus(intellect: f32) -> f32 {
    let base = if intellect < 20.0 { intellect } else { 20.0 };
    let more = intellect - base;
    base + more * 15.0
}

/// Unit::GetHealthBonusFromStamina over integer resolved stats, including negatives.
pub fn health_bonus_from_stamina(stamina: i32) -> Result<f32, ClassicError> {
    Ok(stamina_bonus(stat(stamina, "stamina")?))
}
/// Unit::GetManaBonusFromIntellect over integer resolved stats, including negatives.
pub fn mana_bonus_from_intellect(intellect: i32) -> Result<f32, ClassicError> {
    Ok(intellect_bonus(stat(intellect, "intellect")?))
}

/// Unit::UpdateMaxHealth (the player path), including its minimum-one clamp.
/// `create_health` must come from the selected local class/level row, not a default.
pub fn player_max_health(
    create_health: u32,
    stamina: i32,
    modifiers: StatModifiers,
) -> Result<u32, ClassicError> {
    let create_health = bounded(create_health, "create_health")?;
    let bonus = health_bonus_from_stamina(stamina)?;
    let m = modifiers.validate()?;
    let mut value = m.base_value + create_health;
    value *= m.base_percent;
    value += m.total_value + bonus;
    value *= m.total_percent;
    rounded(value.max(1.0))
}

/// Unit::UpdateMaxPower for POWER_MANA. A zero create-mana suppresses the
/// intellect bonus; it does not suppress explicit resolved mana modifiers.
pub fn maximum_mana(
    create_mana: u32,
    intellect: i32,
    modifiers: StatModifiers,
) -> Result<u32, ClassicError> {
    let create_mana = bounded(create_mana, "create_mana")?;
    let intellect = stat(intellect, "intellect")?;
    let bonus = if create_mana > 0.0 {
        intellect_bonus(intellect)
    } else {
        0.0
    };
    let m = modifiers.validate()?;
    let mut value = m.base_value + create_mana;
    value *= m.base_percent;
    value += m.total_value + bonus;
    value *= m.total_percent;
    rounded(value)
}

/// Creature::UpdateMaxHealth and Creature::GetHealthBonusFromStamina.
/// Native creature stamina contributes only its bonus difference from the
/// floating-point create-stat, unlike player stamina. No pet scaling is provided.
/// SelectLevel's separate rank-rate adjustment to create health belongs to the caller.
pub fn creature_max_health(
    create_health: u32,
    stamina: i32,
    create_stamina: f32,
    health_multiplier: f32,
    modifiers: StatModifiers,
) -> Result<u32, ClassicError> {
    let create_health = bounded(create_health, "create_health")?;
    let stamina = stat(stamina, "stamina")?;
    let create_stamina = finite(create_stamina, 0.0, MAX_STAT as f32, "create_stamina")?;
    let health_multiplier = finite(health_multiplier, 0.0, 1000.0, "health_multiplier")?;
    let bonus = stamina_bonus(stamina) - stamina_bonus(create_stamina);
    let m = modifiers.validate()?;
    let mut value = m.base_value + create_health;
    value *= m.base_percent * health_multiplier;
    value += m.total_value + bonus * health_multiplier;
    value *= m.total_percent;
    rounded(value.max(1.0))
}

/// Result before and after CalculateMeleeDamage's uint32 assignment. Retaining
/// bits allows architecture-independent oracle checks without a float tolerance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArmorReduction {
    pub remaining_damage_bits: u32,
    pub melee_damage: u32,
}

/// Unit::CalcArmorReducedDamage followed by the physical-melee integer cast.
/// Level is bounded to 1..=63, signed armor/penetration to +/-1e9 and damage to
/// 0..=1e9. Negative effective armor becomes zero; reduction caps at 75%.
/// The upstream helper returns one even for zero incoming damage. Callers must
/// skip misses/immunity/no-attack before invoking this helper, as appropriate.
/// A non-unit attacker is represented by zero armor penetration.
pub fn cmangos_armor_reduction(
    attacker_level: u8,
    armor: i32,
    armor_penetration: i32,
    damage: u32,
) -> Result<ArmorReduction, ClassicError> {
    if !(1..=63).contains(&attacker_level) {
        return Err(ClassicError {
            field: "attacker_level",
        });
    }
    if !(-(MAX_VALUE as i32)..=MAX_VALUE as i32).contains(&armor) {
        return Err(ClassicError { field: "armor" });
    }
    if !(-(MAX_VALUE as i32)..=MAX_VALUE as i32).contains(&armor_penetration) {
        return Err(ClassicError {
            field: "armor_penetration",
        });
    }
    let damage = bounded(damage, "damage")?;
    let mut armor = armor as f32;
    armor -= armor_penetration as f32;
    if armor < 0.0 {
        armor = 0.0;
    }
    let mut reduction = 0.1_f32 * armor / (8.5_f32 * attacker_level as f32 + 40.0);
    reduction = reduction / (1.0 + reduction);
    reduction = reduction.clamp(0.0, 0.75);
    let remaining = damage - damage * reduction;
    let remaining = if remaining > 1.0 { remaining } else { 1.0 };
    finite(remaining, 1.0, MAX_VALUE as f32, "result")?;
    Ok(ArmorReduction {
        remaining_damage_bits: remaining.to_bits(),
        melee_damage: remaining as u32,
    })
}

/// Integer-only rational reformulation of the pinned physical-armor formula.
/// All intermediate operations use integers. Round down after the 75% reduction
/// cap, then enforce the source's one-damage minimum. This intentionally differs
/// from the source's intermediate float rounding near some integer boundaries.
/// See `tests/fixtures/armor_rounding_differences.tsv` and `CLASSIC.md`.
/// This function provides armor mitigation only, never hit/miss, immunity or damage rolls.
pub fn armor_reduced_damage(
    attacker_level: u8,
    armor: i32,
    armor_penetration: i32,
    damage: u32,
) -> Result<u32, ClassicError> {
    if !(1..=63).contains(&attacker_level) {
        return Err(ClassicError {
            field: "attacker_level",
        });
    }
    if !(-(MAX_VALUE as i32)..=MAX_VALUE as i32).contains(&armor) {
        return Err(ClassicError { field: "armor" });
    }
    if !(-(MAX_VALUE as i32)..=MAX_VALUE as i32).contains(&armor_penetration) {
        return Err(ClassicError {
            field: "armor_penetration",
        });
    }
    if damage > MAX_VALUE {
        return Err(ClassicError { field: "damage" });
    }
    let effective_armor = (i64::from(armor) - i64::from(armor_penetration)).max(0) as u64;
    let k = 85 * u64::from(attacker_level) + 400;
    let remaining = if effective_armor >= 3 * k {
        u64::from(damage) / 4
    } else {
        u64::from(damage) * k / (effective_armor + k)
    };
    // The preceding input bounds prove 1 <= result <= MAX_VALUE, even at damage=0.
    Ok(remaining.max(1) as u32)
}
