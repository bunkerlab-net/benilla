// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Spell.cpp:6311-6401. Missing Unit/aura state refuses at its first read.
use super::*;
use rustc_apfloat::{ieee::Single, Float, Round, Status};
use SpellCastResult as R;

pub const SPELL_POWER_HEALTH: u32 = 0xffff_fffe;

/// Borrow current resources from the existing Unit owner, not initial maxima.
pub struct SpellPowerContext<'a> {
    pub objects: &'a SourceObjects,
    pub units: &'a SourceUnits,
}
impl SpellPowerContext<'_> {
    pub fn health(&self, caster: ObjectHandle) -> Result<u32> {
        Ok(self.vitals(caster)?.health)
    }
    pub fn current(&self, caster: ObjectHandle, ty: u32) -> Result<u32> {
        let v = self.vitals(caster)?;
        match ty {
            0 => Ok(v.mana),
            3 => Ok(v.energy),
            // SourceUnits has no live rage/focus/happiness field. Do not infer
            // current power from a constructor's maximum or regeneration flag.
            _ => Err(refuse("execution.spells.Unit_GetPower_type_owner")),
        }
    }
    pub fn maximum(&self, caster: ObjectHandle, ty: u32) -> Result<u32> {
        let v = self.vitals(caster)?;
        match ty {
            0 => Ok(v.max_mana),
            3 => Ok(v.max_energy),
            _ => Err(refuse("execution.spells.Unit_GetMaxPower_type_owner")),
        }
    }
    fn vitals(&self, caster: ObjectHandle) -> Result<SourceUnitVitals> {
        self.units
            .get(self.objects, caster)?
            .vitals
            .ok_or_else(|| refuse("execution.spells.CreatureStats_current_resources_owner"))
    }
}

impl SourceSpell {
    /// Spell.cpp:6373-6401. The cached cost is assigned before either comparison.
    pub fn check_power(
        &mut self,
        t: &SpellTemplate,
        strict: bool,
        owner: &mut (impl SpellExecutionOwner + ?Sized),
    ) -> Result<R> {
        if self.item_cast || !owner.power_is_unit(self.caster)? {
            return Ok(R::Ok);
        }
        self.power_cost = self.calculate_power_cost(t, !strict, owner)?;
        if t.powertype == SPELL_POWER_HEALTH {
            return Ok(if owner.power_health(self.caster)? <= self.power_cost {
                R::CasterAuraState
            } else {
                R::Ok
            });
        }
        if t.powertype >= 5 {
            return Ok(R::Unknown);
        }
        Ok(
            if owner.power_current(self.caster, t.powertype)? < self.power_cost {
                R::NoPower
            } else {
                R::Ok
            },
        )
    }

    /// Spell.cpp:6311-6370. SQL costs and levels are uint32 (DBCStructure.h:745-751).
    pub fn calculate_power_cost(
        &self,
        t: &SpellTemplate,
        final_use: bool,
        owner: &mut (impl SpellExecutionOwner + ?Sized),
    ) -> Result<u32> {
        if self.item_cast {
            return Ok(0);
        }
        if t.attributesex & 2 != 0 {
            return match t.powertype {
                SPELL_POWER_HEALTH => owner.power_health(self.caster),
                0..5 => owner.power_current(self.caster, t.powertype),
                _ => Ok(0),
            };
        }
        // :6331 uses unsigned usual arithmetic conversions, then the pinned
        // int32 bit conversion. These wraps are defined, not signed-overflow UB.
        let rank = owner.power_rank(self.caster, t)? as i32;
        let delta = ((rank / 5) as u32).wrapping_sub(t.baselevel);
        let mut cost = t
            .manacost
            .wrapping_add(t.manacostperlevel.wrapping_mul(delta)) as i32;
        if t.manacostpercentage != 0 {
            let base = match t.powertype {
                SPELL_POWER_HEALTH => owner.power_create_health(self.caster)?,
                0 => owner.power_create_mana(self.caster)?,
                1..5 => owner.power_maximum(self.caster, t.powertype)?,
                _ => return Ok(0),
            };
            cost = (cost as u32).wrapping_add(t.manacostpercentage.wrapping_mul(base) / 100) as i32;
        }
        // SharedDefines.h:238-244 also returns physical for an empty/high-only mask.
        let school = (0..7)
            .find(|i| self.school_mask & (1 << i) != 0)
            .unwrap_or(0);
        let flat = owner.power_flat_modifier(self.caster, school)?;
        cost = cost
            .checked_add(flat)
            .ok_or_else(|| refuse("execution.spells.CalculatePowerCost_flat_add_undefined"))?;
        if let Some(player) = owner.power_mod_owner(self.caster)? {
            cost = owner.apply_cost_mod(player, t.id, cost, final_use)?;
        }
        if t.attributes & 0x0008_0000 != 0 {
            let level = owner.power_level(self.caster)?;
            // :6364, binary32 after each operation. No host libm or FMA rewrite.
            let denominator =
                ((single(1.117f32.to_bits()) * integer(t.spelllevel)).value / integer(level)).value;
            let denominator = (denominator - single(0.1327f32.to_bits())).value;
            cost = power_signed(
                (signed_integer(cost) / denominator).value,
                "execution.spells.CalculatePowerCost_scaling_conversion_undefined",
            )?;
        }
        let multiplier = owner.power_multiplier(self.caster, school)?;
        let multiplier = (single(1f32.to_bits()) + single(multiplier)).value;
        cost = power_signed(
            (signed_integer(cost) * multiplier).value,
            "execution.spells.CalculatePowerCost_multiplier_conversion_undefined",
        )?;
        Ok(cost.max(0) as u32)
    }
}
fn single(bits: u32) -> Single {
    Single::from_bits(u128::from(bits))
}
fn integer(value: u32) -> Single {
    Single::from_u128(u128::from(value)).value
}
fn signed_integer(value: i32) -> Single {
    Single::from_i128(i128::from(value)).value
}
fn power_signed(value: Single, id: &str) -> Result<i32> {
    let converted = value.to_i128_r(32, Round::TowardZero, &mut false);
    require(!converted.status.contains(Status::INVALID_OP), id)?;
    i32::try_from(converted.value).map_err(|_| refuse(id))
}

/// Unit.cpp:11876-11881. The creature rank is level, not level times five.
pub fn creature_power_rank(level: u32, t: &SpellTemplate) -> u32 {
    let maximum = t.maxlevel.wrapping_mul(5);
    if t.maxlevel > 0 && level >= maximum {
        maximum
    } else {
        level
    }
}
