// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1: Unit modifiers and StatSystem.cpp, software binary32.
use super::{InitError, Result};
use rustc_apfloat::{ieee::Single, Float, Round, Status, StatusAnd};
const R: Round = Round::NearestTiesToEven;
fn check<T>(v: StatusAnd<T>) -> Result<T> {
    if !(v.status & !Status::INEXACT).is_empty() {
        return Err(InitError::Numeric("floating exception"));
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
    let x = check(v.to_i128_r(32, Round::TowardZero, &mut false))?;
    i32::try_from(x).map_err(|_| InitError::Numeric("int32 conversion"))
}
/// Spell's cached simple value is integer, but CalculateSpellEffectValue first
/// converts the recovered base points to float and then adds base dice in float.
pub(super) fn constant_effect_amount(base_points: i32, base_dice: u32) -> Result<i32> {
    let dice = i32::try_from(base_dice).map_err(|_| InitError::Numeric("base dice"))?;
    base_points
        .checked_add(dice)
        .ok_or(InitError::Numeric("cached initial effect value"))?;
    trunc(add(n(base_points.into())?, n(dice.into())?)?)
}
fn round(v: Single) -> Result<u32> {
    let x = check(v.to_u128_r(32, Round::NearestTiesToAway, &mut false))?;
    let x = u32::try_from(x).map_err(|_| InitError::Numeric("uint32 conversion"))?;
    if x > 1_000_000 {
        return Err(InitError::Numeric("resource bound"));
    }
    Ok(x)
}
#[derive(Clone, Copy, Debug)]
struct Mods([Single; 4]);
impl Mods {
    fn new() -> Self {
        Self([
            Single::ZERO,
            Single::from_bits(0x3f800000),
            Single::ZERO,
            Single::from_bits(0x3f800000),
        ])
    }
    fn apply(&mut self, kind: usize, amount: i32) -> Result<()> {
        if amount.unsigned_abs() > 1_000_000 {
            return Err(InitError::Numeric("modifier bound"));
        }
        let amount = n(amount.into())?;
        self.0[kind] = if kind == 0 || kind == 2 {
            add(self.0[kind], amount)?
        } else {
            let amount = if amount <= n(-100)? { n(-200)? } else { amount };
            mul(
                self.0[kind],
                check(add(n(100)?, amount)?.div_r(n(100)?, R))?,
            )?
        };
        // Keep additive integer modifier cells exact and order-independent under
        // item break/restore; reject aggregate domains above this bound.
        if self.0[kind] < n(-1_000_000)? || self.0[kind] > n(1_000_000)? {
            return Err(InitError::Numeric("aggregate modifier bound"));
        }
        Ok(())
    }
    fn total(self, create: i32, dynamic: Single) -> Result<Single> {
        if self.0[3] <= Single::ZERO {
            return Ok(Single::ZERO);
        }
        mul(
            add(
                mul(add(self.0[0], n(create.into())?)?, self.0[1])?,
                add(self.0[2], dynamic)?,
            )?,
            self.0[3],
        )
    }
}
/// Order: strength, agility, stamina, intellect, spirit. Values are field integers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attributes(pub [i32; 5]);
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resources {
    pub health: u32,
    pub max_health: u32,
    pub mana: u32,
    pub max_mana: u32,
}
#[derive(Clone, Debug)]
pub(super) struct Stats {
    pub create: Attributes,
    pub effective: Attributes,
    pub create_health: u32,
    pub create_mana: u32,
    pub resources: Resources,
    pub resistances: [i32; 7],
    attributes: [Mods; 5],
    health: Mods,
    mana: Mods,
    resistance: [Mods; 7],
}
impl Stats {
    pub fn new(create: Attributes, h: u32, m: u32) -> Result<Self> {
        if create.0.iter().any(|v| !(0..=255).contains(v))
            || h == 0
            || h > 1_000_000
            || m == 0
            || m > 1_000_000
        {
            return Err(InitError::Numeric("level-one SQL creation row"));
        }
        // Constructor sets modifier cells 0/1/0/1 and all create resistances zero.
        let mut s = Self {
            create,
            effective: create,
            create_health: h,
            create_mana: m,
            resources: Resources {
                health: h,
                max_health: h,
                mana: m,
                max_mana: m,
            },
            resistances: [0; 7],
            attributes: [Mods::new(); 5],
            health: Mods::new(),
            mana: Mods::new(),
            resistance: [Mods::new(); 7],
        };
        s.update()?;
        s.resources.health = s.resources.max_health;
        s.resources.mana = s.resources.max_mana;
        Ok(s)
    }
    fn bonus(v: i32, mult: i64) -> Result<Single> {
        let base = v.min(20);
        add(n(base.into())?, mul(n((v - base).into())?, n(mult)?)?)
    }
    pub fn update(&mut self) -> Result<()> {
        for i in 0..5 {
            self.effective.0[i] =
                trunc(self.attributes[i].total(self.create.0[i], Single::ZERO)?)?.max(0);
        }
        if self
            .effective
            .0
            .iter()
            .any(|v| !(0..=1_000_000).contains(v))
        {
            return Err(InitError::Numeric("effective attribute"));
        }
        // The source adds TOTAL_VALUE + bonus before adding to (BASE+create)*BASE_PCT.
        self.resources.max_health = round(
            self.health
                .total(
                    self.create_health as i32,
                    Self::bonus(self.effective.0[2], 10)?,
                )?
                .max(n(1)?),
        )?;
        self.resources.max_mana = round(self.mana.total(
            self.create_mana as i32,
            Self::bonus(self.effective.0[3], 15)?,
        )?)?;
        self.resources.health = self.resources.health.min(self.resources.max_health);
        self.resources.mana = self.resources.mana.min(self.resources.max_mana);
        for i in 0..7 {
            let dynamic = if i == 0 {
                mul(n(self.effective.0[1].into())?, n(2)?)?
            } else {
                Single::ZERO
            };
            self.resistances[i] = trunc(self.resistance[i].total(0, dynamic)?.max(Single::ZERO))?;
        }
        Ok(())
    }
    pub fn attribute(&mut self, index: usize, kind: usize, value: i32) -> Result<()> {
        self.attributes[index].apply(kind, value)
    }
    pub fn resource(&mut self, mana: bool, kind: usize, value: i32) -> Result<()> {
        if mana {
            self.mana.apply(kind, value)?
        } else {
            self.health.apply(kind, value)?
        }
        Ok(())
    }
    pub fn resistance(&mut self, index: usize, kind: usize, value: i32) -> Result<()> {
        self.resistance[index].apply(kind, value)
    }
}

/// Numeric snapshot only. Origin is validated against a fresh admitted recipe.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModifierSnapshot {
    pub create: Attributes,
    pub create_health: u32,
    pub create_mana: u32,
    pub attributes: [[u32; 4]; 5],
    pub health: [u32; 4],
    pub mana: [u32; 4],
    pub resistances: [[u32; 4]; 7],
}
impl Mods {
    fn bits(self) -> [u32; 4] {
        self.0.map(|v| v.to_bits() as u32)
    }
    fn from_bits(bits: [u32; 4]) -> Result<Self> {
        if bits.iter().any(|v| v & 0x7f800000 == 0x7f800000) {
            return Err(InitError::Numeric("nonfinite modifier snapshot"));
        }
        Ok(Self(bits.map(|v| Single::from_bits(v.into()))))
    }
}
impl Stats {
    pub(super) fn snapshot(&self) -> ModifierSnapshot {
        ModifierSnapshot {
            create: self.create,
            create_health: self.create_health,
            create_mana: self.create_mana,
            attributes: self.attributes.map(Mods::bits),
            health: self.health.bits(),
            mana: self.mana.bits(),
            resistances: self.resistance.map(Mods::bits),
        }
    }
    pub(super) fn restore(s: &ModifierSnapshot) -> Result<Self> {
        let mut out = Self::new(s.create, s.create_health, s.create_mana)?;
        for i in 0..5 {
            out.attributes[i] = Mods::from_bits(s.attributes[i])?;
        }
        for i in 0..7 {
            out.resistance[i] = Mods::from_bits(s.resistances[i])?;
        }
        out.health = Mods::from_bits(s.health)?;
        out.mana = Mods::from_bits(s.mana)?;
        out.resources.health = 0;
        out.resources.mana = 0;
        out.update()?;
        Ok(out)
    }
}
