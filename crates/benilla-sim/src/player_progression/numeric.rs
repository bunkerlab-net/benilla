// SPDX-License-Identifier: GPL-3.0-or-later
// Source StatSystem.cpp / Unit.cpp: original binary32 operation order.
use super::{Error, Event, Host, Result, Staged, StatHook};
use crate::{
    classic_spell::SpellFloat,
    mage_resource::{ManaRegen, ManaRegenModifiers, SpellCritModifiers},
    starter_init::{Attributes, ModifierSnapshot},
};
use rustc_apfloat::{ieee::Single, Float, Round, Status, StatusAnd};
const R: Round = Round::NearestTiesToEven;
fn check<T>(v: StatusAnd<T>) -> Result<T> {
    if !(v.status & !Status::INEXACT).is_empty() {
        return Err(Error::Numeric("binary32 exception"));
    }
    Ok(v.value)
}
fn n(v: i64) -> Result<Single> {
    check(Single::from_i128_r(v.into(), R))
}
fn add(a: Single, b: Single) -> Result<Single> {
    check(a.add_r(b, R))
}
fn mul(a: Single, b: Single) -> Result<Single> {
    check(a.mul_r(b, R))
}
fn trunc(v: Single) -> Result<i32> {
    i32::try_from(check(v.to_i128_r(32, Round::TowardZero, &mut false))?)
        .map_err(|_| Error::Numeric("int32 conversion"))
}
fn rounded(v: Single) -> Result<u32> {
    let out = u32::try_from(check(v.to_u128_r(
        32,
        Round::NearestTiesToAway,
        &mut false,
    ))?)
    .map_err(|_| Error::Numeric("uint32 conversion"))?;
    if out > 1_000_000 {
        return Err(Error::Numeric("resource bound"));
    }
    Ok(out)
}
fn cells(bits: [u32; 4]) -> Result<[Single; 4]> {
    let mut out = [Single::ZERO; 4];
    for (i, b) in bits.into_iter().enumerate() {
        if b & 0x7f800000 == 0x7f800000 || (b & 0x7f800000 == 0 && b & 0x7fffff != 0) {
            return Err(Error::Numeric("nonfinite/subnormal modifier"));
        }
        let v = Single::from_bits(b.into());
        if v < n(-1_000_000)? || v > n(1_000_000)? {
            return Err(Error::Numeric("modifier bound"));
        }
        out[i] = v;
    }
    Ok(out)
}
fn total(bits: [u32; 4], create: i32, dynamic: Single) -> Result<Single> {
    let m = cells(bits)?;
    // GetModifierValue(TOTAL_PCT), GetTotalStat/Resistance clamp this cell.
    if m[3] <= Single::ZERO {
        return Ok(Single::ZERO);
    }
    mul(
        add(
            mul(add(m[0], n(create.into())?)?, m[1])?,
            add(m[2], dynamic)?,
        )?,
        m[3],
    )
}
fn bonus(stat: i32, mult: i64) -> Result<Single> {
    let base = stat.min(20);
    add(n(base.into())?, mul(n((stat - base).into())?, n(mult)?)?)
}
fn attribute(m: &ModifierSnapshot, index: usize) -> Result<i32> {
    let v = trunc(total(m.attributes[index], m.create.0[index], Single::ZERO)?)?.max(0);
    if v > 1_000_000 {
        return Err(Error::Numeric("effective attribute bound"));
    }
    Ok(v)
}
fn max_health(m: &ModifierSnapshot, stamina: i32) -> Result<u32> {
    rounded(total(m.health, m.create_health as i32, bonus(stamina, 10)?)?.max(n(1)?))
}
fn max_mana(m: &ModifierSnapshot, intellect: i32) -> Result<u32> {
    rounded(total(m.mana, m.create_mana as i32, bonus(intellect, 15)?)?)
}
fn resistance(m: &ModifierSnapshot, index: usize, agility: i32) -> Result<i32> {
    let dynamic = if index == 0 {
        mul(n(agility.into())?, n(2)?)?
    } else {
        Single::ZERO
    };
    if index == 0 {
        // UpdateArmor temporarily adds agility into TOTAL_VALUE, then subtracts
        // it again. Binary32 cancellation can change that supposedly static cell.
        // This fixed-cell reducer must reject rather than conceal that mutation.
        let original = cells(m.resistances[0])?[2];
        let temporary = add(original, dynamic)?;
        let restored = check(temporary.sub_r(dynamic, R))?;
        if restored.to_bits() != original.to_bits() {
            return Err(Error::Numeric("non-reversible armor modifier"));
        }
    }
    let value = trunc(total(m.resistances[index], 0, dynamic)?.max(Single::ZERO))?;
    if value > 1_000_000 {
        return Err(Error::Numeric("resistance bound"));
    }
    Ok(value)
}
fn regen(spirit: i32) -> Result<ManaRegen> {
    crate::mage_resource::update_mana_regen(
        spirit as u32,
        ManaRegenModifiers {
            spirit_multiplier: SpellFloat::from_bits(0x3f800000)
                .map_err(|_| Error::Numeric("one"))?,
            flat_mana_per_five: 0,
            interrupted_percent: 0,
        },
    )
    .map_err(|_| Error::Numeric("mage regeneration"))
}
fn crit(level: u32, intellect: i32) -> Result<SpellFloat> {
    crate::mage_resource::mage_spell_crit(
        level,
        intellect as u32,
        SpellCritModifiers {
            all_schools: 0,
            school: 0,
        },
    )
    .map_err(|_| Error::Numeric("mage spell crit"))
}
/// Supported numeric fields only. Attack/weapon/defense/spell-damage/hit fields
/// live in the mandatory audited host, not implied zero by their omission here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumericState {
    pub attributes: Attributes,
    pub resistances: [i32; 7],
    pub health: u32,
    pub max_health: u32,
    /// Native order: mana, rage, focus, energy, happiness.
    pub powers: [u32; 5],
    pub max_powers: [u32; 5],
    pub mana_regeneration: ManaRegen,
    pub spell_crit_percent: SpellFloat,
}
impl NumericState {
    /// Compute the admitted static modifier state at an actual row's level.
    /// Creation rows/provenance must still pass State::restore and host preflight.
    pub fn from_modifiers(
        level: u32,
        m: &ModifierSnapshot,
        health: u32,
        powers: [u32; 5],
    ) -> Result<Self> {
        if !(1..=super::MAX_SUPPORTED_LEVEL).contains(&level)
            || m.create.0.iter().any(|v| !(0..=255).contains(v))
            || m.create_health == 0
            || m.create_health > 1_000_000
            || m.create_mana == 0
            || m.create_mana > 1_000_000
        {
            return Err(Error::Numeric("create rows"));
        }
        let mut attributes = Attributes([0; 5]);
        for i in 0..5 {
            attributes.0[i] = attribute(m, i)?;
        }
        let mut resistances = [0; 7];
        for (i, v) in resistances.iter_mut().enumerate() {
            *v = resistance(m, i, attributes.0[1])?;
        }
        let max_health = max_health(m, attributes.0[2])?;
        let max_powers = [max_mana(m, attributes.0[3])?, 1000, 0, 100, 0];
        if health > max_health || powers.iter().zip(max_powers).any(|(a, b)| *a > b) {
            return Err(Error::Invalid("current resources above maxima"));
        }
        Ok(Self {
            attributes,
            resistances,
            health,
            max_health,
            powers,
            max_powers,
            mana_regeneration: regen(attributes.0[4])?,
            spell_crit_percent: crit(level, attributes.0[3])?,
        })
    }
    pub(super) fn validate(&self, level: u32, m: &ModifierSnapshot, alive: bool) -> Result<()> {
        let expected = Self::from_modifiers(level, m, self.health, self.powers)?;
        if *self != expected || alive != (self.health != 0) {
            return Err(Error::Invalid("numeric checkpoint"));
        }
        Ok(())
    }
}
impl<H: Host> Staged<'_, H> {
    pub(super) fn set_health(&mut self, value: u32) -> Result<()> {
        // In the admitted Alive/CORPSE states, SetHealth clamps to the current max.
        let value = value.min(self.state.0.numeric.max_health);
        self.state.0.numeric.health = value;
        self.emit(Event::Health(value))
    }
    pub(super) fn set_power(&mut self, power: usize, value: u32) -> Result<()> {
        // Source early exit compares BEFORE clamping to the max.
        if self.state.0.numeric.powers[power] == value {
            return Ok(());
        }
        let value = value.min(self.state.0.numeric.max_powers[power]);
        self.state.0.numeric.powers[power] = value;
        self.emit(Event::Power {
            power: power as u8,
            value,
        })
    }
    fn armor(&mut self) -> Result<()> {
        let value = resistance(
            &self.state.0.modifiers,
            0,
            self.state.0.numeric.attributes.0[1],
        )?;
        self.state.0.numeric.resistances[0] = value;
        self.emit(Event::Armor(value))
    }
    pub(super) fn update_all_stats(&mut self) -> Result<()> {
        for i in 0..5 {
            let value = attribute(&self.state.0.modifiers, i)?;
            self.state.0.numeric.attributes.0[i] = value;
            self.emit(Event::Stat {
                index: i as u8,
                value,
            })?;
        }
        self.emit(Event::Hook(StatHook::AttackPowerAndDamage {
            ranged: false,
        }))?;
        self.emit(Event::Hook(StatHook::AttackPowerAndDamage { ranged: true }))?;
        self.armor()?;
        let value = max_health(
            &self.state.0.modifiers,
            self.state.0.numeric.attributes.0[2],
        )?;
        self.state.0.numeric.max_health = value;
        self.emit(Event::MaxHealth(value))?;
        if value < self.state.0.numeric.health {
            self.set_health(value)?;
        }
        let maxima = [
            max_mana(
                &self.state.0.modifiers,
                self.state.0.numeric.attributes.0[3],
            )?,
            1000,
            0,
            100,
            0,
        ];
        for (power, value) in maxima.into_iter().enumerate() {
            self.state.0.numeric.max_powers[power] = value;
            self.emit(Event::MaxPower {
                power: power as u8,
                value,
            })?;
            if value < self.state.0.numeric.powers[power] {
                self.set_power(power, value)?;
            }
        }
        self.emit(Event::Hook(StatHook::AllCritPercentages))?;
        self.state.0.numeric.spell_crit_percent =
            crit(self.state.0.level, self.state.0.numeric.attributes.0[3])?;
        self.emit(Event::Hook(StatHook::AllSpellCritChances))?;
        self.emit(Event::Hook(StatHook::DefenseBonuses))?;
        self.emit(Event::Hook(StatHook::SpellDamageBonus))?;
        self.state.0.numeric.mana_regeneration = regen(self.state.0.numeric.attributes.0[4])?;
        self.emit(Event::Hook(StatHook::ManaRegen))?;
        for attack in 0..3 {
            self.emit(Event::Hook(StatHook::WeaponHitChance { attack }))?;
        }
        self.armor()?;
        for school in 1..7 {
            let value = resistance(
                &self.state.0.modifiers,
                school,
                self.state.0.numeric.attributes.0[1],
            )?;
            self.state.0.numeric.resistances[school] = value;
            self.emit(Event::Resistance {
                school: school as u8,
                value,
            })?;
        }
        Ok(())
    }
}
