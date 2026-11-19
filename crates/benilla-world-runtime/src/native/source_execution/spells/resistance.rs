// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! The resistance groups of Unit.cpp:367-386,8970-9034,9079-9101.
use super::*;

#[derive(Clone, Copy, Debug)]
pub enum ResistanceModifier {
    BaseValue,
    BasePercent,
    TotalValue,
    TotalPercent,
}

#[derive(Clone, Copy, Debug)]
struct ResistanceGroup {
    base_value: f32,
    base_percent: f32,
    total_value: f32,
    total_percent: f32,
}

impl ResistanceGroup {
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
}

/// Live Unit resistance operands. Only Creature::UpdateArmor writes an update field here.
#[derive(Clone, Debug)]
pub struct SourceUnitResistances {
    create: [i32; 7],
    groups: [ResistanceGroup; 7],
    agility: u32,
    armor: i32,
}

impl Default for SourceUnitResistances {
    fn default() -> Self {
        Self {
            create: [0; 7],
            groups: [ResistanceGroup {
                base_value: 0.0,
                base_percent: 1.0,
                total_value: 0.0,
                total_percent: 1.0,
            }; 7],
            agility: 0,
            armor: 0,
        }
    }
}

impl SourceUnitResistances {
    /// Creature.cpp:1459,1475,1485; StatSystem.cpp:623,628.
    pub fn select_level(&mut self, base_armor: f32, agility: i32) -> Result<()> {
        self.groups[0].base_value = base_armor;
        self.agility = agility as u32; // Unit.h:1321 reads the UInt32 update field.
        self.update_armor()
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
    pub fn update_armor(&mut self) -> Result<()> {
        let dynamic = (self.agility as f32) * 2.0;
        self.groups[0].total_value += dynamic;
        self.armor = armor_int(self.total(0))?;
        self.groups[0].total_value -= dynamic;
        Ok(())
    }

    /// SpellAuras.cpp:3367-3387.
    pub fn handle_aura(
        &mut self,
        amount: i32,
        mask: i32,
        apply: bool,
        can_modify: bool,
        player: bool,
    ) -> Result<()> {
        if amount == 0 {
            return Ok(());
        }
        for school in 0..self.groups.len() {
            if mask & (1 << school) == 0 {
                continue;
            }
            let old = self.total(school);
            self.groups[school].modify(ResistanceModifier::TotalValue, amount as f32, apply);
            if can_modify {
                if school != 0 {
                    return Err(refuse("execution.spells.Unit_9033.UpdateResistances_owner"));
                }
                require(
                    !player,
                    "execution.spells.Unit_9019.Player_UpdateArmor_owner",
                )?;
                self.update_armor()?;
            }
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
