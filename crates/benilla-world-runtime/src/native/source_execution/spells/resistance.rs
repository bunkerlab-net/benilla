// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! The resistance groups of Unit.cpp:367-386,8970-9034,9079-9101.
use super::damage::{CreatureWeaponState, DamageAttack};
use super::*;

#[derive(Clone, Copy, Debug)]
pub enum ResistanceModifier {
    BaseValue,
    BasePercent,
    TotalValue,
    TotalPercent,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::native::source_execution) struct StatModifierGroup {
    pub(super) base_value: f32,
    pub(super) base_percent: f32,
    pub(super) total_value: f32,
    pub(in crate::native::source_execution) total_percent: f32,
}

impl Default for StatModifierGroup {
    fn default() -> Self {
        Self {
            base_value: 0.0,
            base_percent: 1.0,
            total_value: 0.0,
            total_percent: 1.0,
        }
    }
}

impl StatModifierGroup {
    fn modify(&mut self, modifier: ResistanceModifier, amount: f32, apply: bool) {
        let field = match modifier {
            ResistanceModifier::BaseValue => &mut self.base_value,
            ResistanceModifier::BasePercent => &mut self.base_percent,
            ResistanceModifier::TotalValue => &mut self.total_value,
            ResistanceModifier::TotalPercent => &mut self.total_percent,
        };
        if matches!(
            modifier,
            ResistanceModifier::BaseValue | ResistanceModifier::TotalValue
        ) {
            *field += if apply { amount } else { -amount };
        } else {
            let amount = if amount <= -100.0 { -200.0 } else { amount };
            let factor = (100.0 + amount) / 100.0;
            *field *= if apply { factor } else { 1.0 / factor };
        }
    }

    // Unit.cpp:9049-9060. Leave the raw value intact for unapply.
    pub(super) fn total_percent(&self) -> f32 {
        if self.total_percent <= 0.0 {
            0.0
        } else {
            self.total_percent
        }
    }
}

pub(super) enum UnitMod<'a> {
    Stat(&'a mut StatModifierGroup),
    Resistance(&'a mut SourceUnitResistances, usize),
    Damage(
        &'a mut SourceCreatureDamage,
        DamageAttack,
        CreatureWeaponState<'a>,
    ),
}

#[derive(Clone, Copy)]
pub(super) struct StatModifierUnit<'a> {
    pub objects: &'a SourceObjects,
    pub unit: ObjectHandle,
    pub can_modify: bool,
    pub player: bool,
    pub totem: bool,
}

impl StatModifierUnit<'_> {
    // Unit.cpp:8970-9040. The modifier write precedes the CanModifyStats test.
    pub(super) fn handle_stat_modifier(
        self,
        mut group: UnitMod<'_>,
        modifier: ResistanceModifier,
        amount: f32,
        apply: bool,
    ) -> Result<()> {
        match &mut group {
            UnitMod::Stat(group) => group,
            UnitMod::Resistance(state, school) => &mut state.groups[*school],
            UnitMod::Damage(state, attack, _) => state.group(*attack),
        }
        .modify(modifier, amount, apply);
        if !self.can_modify {
            return Ok(());
        }
        match group {
            UnitMod::Stat(_) => Err(refuse("execution.spells.Unit_9017.UpdateStats_owner")),
            UnitMod::Resistance(state, 0) => {
                require(
                    !self.player,
                    "execution.spells.Unit_9019.Player_UpdateArmor_owner",
                )?;
                state.update_armor(self.objects, self.unit)
            }
            UnitMod::Resistance(..) => {
                Err(refuse("execution.spells.Unit_9033.UpdateResistances_owner"))
            }
            UnitMod::Damage(state, attack, weapon) => {
                require(
                    !self.player,
                    match attack {
                        DamageAttack::Mainhand => {
                            "execution.spells.Unit_9038.Player_UpdateDamagePhysical_owner"
                        }
                        DamageAttack::Offhand => {
                            "execution.spells.Unit_9039.Player_UpdateDamagePhysical_owner"
                        }
                        DamageAttack::Ranged => {
                            "execution.spells.Unit_9040.Player_UpdateDamagePhysical_owner"
                        }
                    },
                )?;
                if self.totem {
                    // Totem.h:64 has an empty virtual override.
                    return Ok(());
                }
                state.update_damage_physical(self.objects, self.unit, weapon, attack)
            }
        }
    }
}

/// Live resistance operands, including the agility and armor update fields.
#[derive(Clone, Debug, Default)]
pub struct SourceUnitResistances {
    create: [i32; 7],
    groups: [StatModifierGroup; 7],
    agility: u32,
    armor: i32,
}

impl SourceUnitResistances {
    /// Creature.cpp:1459,1475,1485; StatSystem.cpp:623,628.
    pub fn select_level(
        &mut self,
        objects: &SourceObjects,
        unit: ObjectHandle,
        base_armor: f32,
        agility: i32,
    ) -> Result<()> {
        self.set_base_armor(base_armor);
        self.set_agility(objects, unit, agility)?;
        self.update_armor(objects, unit)
    }

    pub(in crate::native::source_execution) fn set_base_armor(&mut self, value: f32) {
        self.groups[0].base_value = value;
    }

    pub(in crate::native::source_execution) fn agility(&self) -> u32 {
        self.agility
    }

    pub(in crate::native::source_execution) fn set_agility(
        &mut self,
        objects: &SourceObjects,
        unit: ObjectHandle,
        agility: i32,
    ) -> Result<()> {
        objects.set_update_value(
            unit,
            UNIT_FIELD_STAT1,
            &mut self.agility,
            agility.max(0) as u32,
        )?;
        Ok(())
    }

    /// Creature.cpp:585-590.
    pub fn set_creature_resistances(&mut self, values: [u32; 6]) {
        for (out, value) in self.create[1..].iter_mut().zip(values) {
            *out = value as i32;
        }
    }

    pub fn create_nonphysical(&self) -> [u32; 6] {
        std::array::from_fn(|i| self.create[i + 1] as u32)
    }

    pub fn armor(&self) -> i32 {
        self.armor
    }

    fn total(&self, school: usize) -> f32 {
        let group = &self.groups[school];
        if group.total_percent <= 0.0 {
            return 0.0;
        }
        let create = self.create[school] as f32;
        let mut value = create;
        value += group.base_value;
        value *= group.base_percent;
        value += group.total_value;
        value *= group.total_percent;
        if value < 0.0 && create >= 0.0 {
            return 0.0;
        }
        value
    }

    // StatSystem.cpp:664-672. The two TOTAL_VALUE writes may round differently.
    pub fn update_armor(&mut self, objects: &SourceObjects, unit: ObjectHandle) -> Result<()> {
        let dynamic = (self.agility as f32) * 2.0;
        self.groups[0].total_value += dynamic;
        let value = armor_int(self.total(0))?;
        // Unit.h:1325 uses SetStatInt32Value (Object.cpp:930-936).
        objects.set_update_value(unit, UNIT_FIELD_RESISTANCES, &mut self.armor, value.max(0))?;
        self.groups[0].total_value -= dynamic;
        Ok(())
    }

    /// SpellAuras.cpp:3367-3387.
    #[allow(clippy::too_many_arguments)]
    pub fn handle_aura(
        &mut self,
        objects: &SourceObjects,
        unit: ObjectHandle,
        amount: i32,
        mask: i32,
        apply: bool,
        can_modify: bool,
        player: bool,
    ) -> Result<()> {
        if amount == 0 {
            return Ok(());
        }
        let target = StatModifierUnit {
            objects,
            unit,
            can_modify,
            player,
            totem: false,
        };
        for school in 0..self.groups.len() {
            if mask & (1 << school) == 0 {
                continue;
            }
            let old = self.total(school);
            target.handle_stat_modifier(
                UnitMod::Resistance(self, school),
                ResistanceModifier::TotalValue,
                amount as f32,
                apply,
            )?;
            let _change = self.total(school) - old;
            require(
                !player,
                "execution.spells.Aura_3384.ApplyResistanceBuffModsMod_owner",
            )?;
        }
        Ok(())
    }
}

fn armor_int(value: f32) -> Result<i32> {
    require(
        (i32::MIN as f32..2_147_483_648.0).contains(&value),
        "execution.spells.StatSystem_669.armor_float_to_int_undefined",
    )?;
    Ok(value as i32)
}

#[cfg(test)]
#[path = "resistance_tests.rs"]
mod tests;
