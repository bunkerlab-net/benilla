// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! UnitAI.cpp:960-972 uses live unit resources and the loaded spell stores.
use super::*;
use std::collections::BTreeSet;

impl SourceUnitAi {
    /// UnitAI.cpp:962-971 initializes only the first spell and inserts every ID.
    pub fn add_main_spell(
        &mut self,
        spell: u32,
        spells: &mut BTreeSet<u32>,
        stores: &SourceSpellStores,
        owner: &mut (impl SpellExecutionOwner + ?Sized),
    ) -> Result<()> {
        if self.main_spell_id == 0 {
            self.main_spell_id = spell;
            let t = stores
                .template(self.main_spell_id)
                .ok_or_else(|| refuse("execution.unit_ai.AddMainSpell_966.null_spell_undefined"))?;
            self.main_spell_cost = unit_power_cost(t, self.unit, owner)?;
            self.main_spell_min_range = stores
                .range_entry(t.rangeindex)?
                .map_or(0., |range| f32::from_bits(range.minimum.0));
            require(
                t.school < 32,
                "execution.unit_ai.AddMainSpell_968.school_shift_undefined",
            )?;
            self.main_attack_mask = self.main_attack_mask.wrapping_add(1u32 << t.school);
            self.main_spell_info = Some(t.id);
        }
        spells.insert(spell);
        Ok(())
    }
}

#[cfg(test)]
/// Creature.cpp:1426-1475 and Object.cpp:105-108 own these live update fields.
pub(super) struct UnitPowerOwner<'a> {
    pub objects: &'a SourceObjects,
    pub units: UnitPowerSource<'a>,
}
#[cfg(test)]
pub(super) enum UnitPowerSource<'a> {
    Units(&'a SourceUnits),
    Caster(ObjectHandle, UnitPowerOperands<'a>),
}
#[cfg(test)]
#[derive(Clone, Copy)]
pub(super) struct UnitPowerOperands<'a> {
    pub vitals: &'a Option<SourceUnitVitals>,
    pub fields: &'a Option<SourceUnitPowerFields>,
    pub flat: &'a [i32; 7],
    pub multiplier: &'a [u32; 7],
}
#[cfg(test)]
impl UnitPowerOperands<'_> {
    fn vitals(&self) -> Result<&SourceUnitVitals> {
        self.vitals
            .as_ref()
            .ok_or_else(|| refuse("execution.spells.CreatureStats_current_resources_owner"))
    }

    fn fields(&self) -> Result<&SourceUnitPowerFields> {
        self.fields
            .as_ref()
            .ok_or_else(|| refuse("execution.spells.CreatureStats_power_fields_owner"))
    }
}
#[cfg(test)]
impl UnitPowerOwner<'_> {
    fn operands(&self, caster: ObjectHandle) -> Result<UnitPowerOperands<'_>> {
        match &self.units {
            UnitPowerSource::Units(units) => {
                let live = units.get(self.objects, caster)?;
                Ok(UnitPowerOperands {
                    vitals: &live.vitals,
                    fields: &live.power_fields,
                    flat: &live.power_cost_modifier,
                    multiplier: &live.power_cost_multiplier,
                })
            }
            UnitPowerSource::Caster(unit, operands) => {
                self.objects.get(caster)?;
                require(*unit == caster, "execution.spells.ai_cast.other_unit_owner")?;
                Ok(*operands)
            }
        }
    }
}
#[cfg(test)]
impl SpellExecutionOwner for UnitPowerOwner<'_> {
    fn power_objects(&mut self) -> Result<&SourceObjects> {
        Ok(self.objects)
    }
    fn power_context(&mut self) -> Result<SpellPowerContext<'_>> {
        let UnitPowerSource::Units(units) = self.units else {
            return Err(refuse("execution.spells.Unit_GetPower_fields_owner"));
        };
        Ok(SpellPowerContext {
            objects: self.objects,
            units,
        })
    }
    fn power_health(&mut self, caster: ObjectHandle) -> Result<u32> {
        Ok(self.operands(caster)?.vitals()?.health)
    }
    fn power_level(&mut self, caster: ObjectHandle) -> Result<u32> {
        Ok(self.operands(caster)?.fields()?.level)
    }
    fn power_create_health(&mut self, caster: ObjectHandle) -> Result<u32> {
        Ok(self.operands(caster)?.fields()?.create_health)
    }
    fn power_create_mana(&mut self, caster: ObjectHandle) -> Result<u32> {
        Ok(self.operands(caster)?.fields()?.create_mana)
    }
    fn power_current(&mut self, caster: ObjectHandle, ty: u32) -> Result<u32> {
        if matches!(ty, 0 | 3) {
            return self.operands(caster)?.vitals()?.spell_power_current(ty);
        }
        let index = match ty {
            1 => 0,
            2 => 1,
            4 => 2,
            _ => return Err(refuse("execution.spells.Unit_GetPower_type_owner")),
        };
        Ok(self
            .operands(caster)?
            .fields()?
            .auxiliary_current
            .ok_or_else(|| refuse("execution.spells.Unit_GetPower_type_owner"))?[index])
    }
    fn power_maximum(&mut self, caster: ObjectHandle, ty: u32) -> Result<u32> {
        if matches!(ty, 0 | 3) {
            return self.operands(caster)?.vitals()?.spell_power_maximum(ty);
        }
        let index = match ty {
            1 => 0,
            2 => 1,
            4 => 2,
            _ => return Err(refuse("execution.spells.Unit_GetMaxPower_type_owner")),
        };
        Ok(self
            .operands(caster)?
            .fields()?
            .auxiliary_maximum
            .ok_or_else(|| refuse("execution.spells.Unit_GetMaxPower_type_owner"))?[index])
    }
    fn power_flat_modifier(&mut self, caster: ObjectHandle, school: u32) -> Result<i32> {
        Ok(self.operands(caster)?.flat[school as usize])
    }
    fn power_multiplier(&mut self, caster: ObjectHandle, school: u32) -> Result<u32> {
        Ok(self.operands(caster)?.multiplier[school as usize])
    }
}
