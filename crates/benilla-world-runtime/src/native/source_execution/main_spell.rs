// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! UnitAI.cpp:960-972 uses live unit resources and the loaded spell stores.
use super::*;
use std::collections::BTreeSet;

impl SourceUnitAi {
    /// UnitAI.cpp:701-708. The range row read precedes the spell modifier owner.
    pub(super) fn calculate_spell_range(
        &self,
        objects: &SourceObjects,
        units: &SourceUnits,
        stores: &SourceSpellStores,
        spell: &SpellTemplate,
    ) -> Result<f32> {
        let max = stores
            .range_entry(spell.rangeindex)?
            .map_or(0., |range| f32::from_bits(range.maximum.0));
        let live = units.get(objects, self.unit)?;
        require(
            world_object_spell_mod_owner(objects, self.unit, live.totem)?.is_none(),
            "execution.ai.UnitAI_707.ApplySpellMod_owner.player",
        )?;
        Ok(max)
    }

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

    fn power_fields(&self, caster: ObjectHandle) -> Result<SpellPowerFields<'_>> {
        let operands = self.operands(caster)?;
        Ok(SpellPowerFields {
            vitals: operands.vitals,
            power: operands.fields,
        })
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
        Ok(self.power_fields(caster)?.vitals()?.health)
    }
    fn power_level(&mut self, caster: ObjectHandle) -> Result<u32> {
        Ok(self.power_fields(caster)?.power()?.level)
    }
    fn power_create_health(&mut self, caster: ObjectHandle) -> Result<u32> {
        Ok(self.power_fields(caster)?.power()?.create_health)
    }
    fn power_create_mana(&mut self, caster: ObjectHandle) -> Result<u32> {
        Ok(self.power_fields(caster)?.power()?.create_mana)
    }
    fn power_current(&mut self, caster: ObjectHandle, ty: u32) -> Result<u32> {
        SpellPowerRead::Current.read(ty, || self.power_fields(caster))
    }
    fn power_maximum(&mut self, caster: ObjectHandle, ty: u32) -> Result<u32> {
        SpellPowerRead::Maximum.read(ty, || self.power_fields(caster))
    }
    fn power_flat_modifier(&mut self, caster: ObjectHandle, school: u32) -> Result<i32> {
        Ok(self.operands(caster)?.flat[school as usize])
    }
    fn power_multiplier(&mut self, caster: ObjectHandle, school: u32) -> Result<u32> {
        Ok(self.operands(caster)?.multiplier[school as usize])
    }
}
