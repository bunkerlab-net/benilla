// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Spell.cpp:6005-6154 and SpellMgr.h:398-440. Unowned aura lists never mean empty.
use super::*;
use SpellCastResult as R;

/// Definition operands of an actual aura effect. No immunity result is an input.
#[derive(Clone, Copy, Debug)]
pub struct SpellCasterAura {
    pub attributes: u32,
    pub attributesex2: u32,
    pub school: u32,
    pub dispel: u32,
    pub mechanic: u32,
    pub effect_mechanic: u32,
    pub aura_type: u32,
}
impl SourceSpell {
    pub fn check_caster_auras(
        &mut self,
        t: &SpellTemplate,
        owner: &mut (impl SpellExecutionOwner + ?Sized),
    ) -> Result<R> {
        if self.flags.ignore_caster_aura()
            || !owner.power_is_unit(self.caster)?
            || t.attributesex3 & 0x1000_0000 != 0
        {
            return Ok(R::Ok);
        }
        // :6029 precedes the client charmer check, even for a flag-free caster.
        let flags = owner.caster_aura_flags(self.caster)?;
        let mut result = R::Ok;
        if self.client_cast
            && owner.caster_charmer(self.caster)?.is_some()
            && !self.check_cancels_aura_types(t, &[6, 177, 2], owner)?
        {
            result = R::Charmed;
        }
        // :6019-6023 initializes all usableWhile* booleans to false. The
        // mechanicCheck lambda at :6039-6076 is never called at this pin.
        for (flag, prevention, types, failure) in [
            (0x0004_0000, None, &[12][..], R::Stunned),
            (0x0000_2000, Some(1), &[27, 60][..], R::Silenced),
            (0x0002_0000, Some(2), &[25, 60][..], R::Pacified),
            (0x0080_0000, None, &[7][..], R::Fleeing),
            (0x0040_0000, None, &[5][..], R::Confused),
        ] {
            if flags & flag != 0
                && prevention.is_none_or(|p| t.preventiontype == p)
                && !self.check_cancels_aura_types(t, types, owner)?
            {
                result = failure;
                break;
            }
        }
        // A successful earlier cancellation still permits the next else-if.
        Ok(if result != R::Ok && self.param1 != 0 {
            R::PreventedByMechanic
        } else {
            result
        })
    }

    fn check_cancels_aura_types(
        &mut self,
        t: &SpellTemplate,
        types: &[u32],
        owner: &mut (impl SpellExecutionOwner + ?Sized),
    ) -> Result<bool> {
        for &ty in types {
            // :6098-6105. CheckCasterAuras already proved a Unit true caster.
            let auras = owner.caster_auras(self.caster, ty)?;
            for aura in auras {
                if spell_cancels_aura_effect(t, aura)? {
                    continue;
                }
                self.param1 = if aura.effect_mechanic != 0 {
                    aura.effect_mechanic
                } else {
                    aura.mechanic
                };
                return Ok(false);
            }
        }
        Ok(true)
    }
}
/// SpellMgr.h:399-439. School shifts refuse only after both attribute guards.
pub fn spell_cancels_aura_effect(t: &SpellTemplate, aura: &SpellCasterAura) -> Result<bool> {
    if t.attributesex & 0x8000 == 0 || aura.attributes & 0x2000_0000 != 0 {
        return Ok(false);
    }
    for (effect, ty, misc) in [
        (t.effect1, t.effectapplyauraname1, t.effectmiscvalue1),
        (t.effect2, t.effectapplyauraname2, t.effectmiscvalue2),
        (t.effect3, t.effectapplyauraname3, t.effectmiscvalue3),
    ] {
        if effect != 6 {
            continue;
        }
        let misc = misc as u32;
        let cancels = match ty {
            38 => misc == aura.aura_type,
            39 => {
                if aura.attributesex2 & 0x0400_0000 != 0 {
                    continue;
                }
                require(
                    aura.school < 32,
                    "execution.spells.SpellCancelsAuraEffect_school_shift_undefined",
                )?;
                (1u32 << aura.school) & misc != 0
            }
            41 => misc == aura.dispel,
            77 => misc == aura.mechanic || misc == aura.effect_mechanic,
            _ => false,
        };
        if cancels {
            return Ok(true);
        }
    }
    Ok(false)
}
