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
    pub units: &'a SourceUnits,
}
#[cfg(test)]
impl UnitPowerOwner<'_> {
    fn fields(&self, caster: ObjectHandle) -> Result<&SourceUnitPowerFields> {
        self.units
            .get(self.objects, caster)?
            .power_fields
            .as_ref()
            .ok_or_else(|| refuse("execution.spells.CreatureStats_power_fields_owner"))
    }
}
#[cfg(test)]
impl SpellExecutionOwner for UnitPowerOwner<'_> {
    fn power_objects(&mut self) -> Result<&SourceObjects> {
        Ok(self.objects)
    }
    fn power_context(&mut self) -> Result<SpellPowerContext<'_>> {
        Ok(SpellPowerContext {
            objects: self.objects,
            units: self.units,
        })
    }
    fn power_level(&mut self, caster: ObjectHandle) -> Result<u32> {
        Ok(self.fields(caster)?.level)
    }
    fn power_create_health(&mut self, caster: ObjectHandle) -> Result<u32> {
        Ok(self.fields(caster)?.create_health)
    }
    fn power_create_mana(&mut self, caster: ObjectHandle) -> Result<u32> {
        Ok(self.fields(caster)?.create_mana)
    }
    fn power_current(&mut self, caster: ObjectHandle, ty: u32) -> Result<u32> {
        if matches!(ty, 0 | 3) {
            return self.power_context()?.current(caster, ty);
        }
        let index = match ty {
            1 => 0,
            2 => 1,
            4 => 2,
            _ => return Err(refuse("execution.spells.Unit_GetPower_type_owner")),
        };
        Ok(self
            .fields(caster)?
            .auxiliary_current
            .ok_or_else(|| refuse("execution.spells.Unit_GetPower_type_owner"))?[index])
    }
    fn power_maximum(&mut self, caster: ObjectHandle, ty: u32) -> Result<u32> {
        if matches!(ty, 0 | 3) {
            return self.power_context()?.maximum(caster, ty);
        }
        let index = match ty {
            1 => 0,
            2 => 1,
            4 => 2,
            _ => return Err(refuse("execution.spells.Unit_GetMaxPower_type_owner")),
        };
        Ok(self
            .fields(caster)?
            .auxiliary_maximum
            .ok_or_else(|| refuse("execution.spells.Unit_GetMaxPower_type_owner"))?[index])
    }
    fn power_flat_modifier(&mut self, caster: ObjectHandle, school: u32) -> Result<i32> {
        Ok(self.units.get(self.objects, caster)?.power_cost_modifier[school as usize])
    }
    fn power_multiplier(&mut self, caster: ObjectHandle, school: u32) -> Result<u32> {
        Ok(self.units.get(self.objects, caster)?.power_cost_multiplier[school as usize])
    }
}
