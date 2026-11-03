// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, StatSystem.cpp:203-386; Player.cpp:7107-7173.
// Upstream GPL-2.0-or-later, Copyright CMaNGOS Project contributors.
//! Bind captured starter operands to the existing mainhand host, never world admission.
use super::*;
use benilla_sim::{player_melee as melee, starter_init as init};
mod attack;

impl Profile {
    /// Assemble item 35 from the initializer populated by captured learning/outfit adapters.
    /// Returns weapon-only DPS; live AP damage, skills and timers remain source-recomputed.
    pub fn initialize_captured_player_melee(
        &mut self,
        starter: &init::StarterInput,
        weapon_skill_gain: u32,
    ) -> Result<spell::SpellFloat, NativeError> {
        if starter.race != 1 || starter.class != 8 {
            return Err(NativeError::UnsupportedMechanic(
                "captured melee Human Mage identity",
            ));
        }
        let staff = starter.items.iter().find(|row| row.id == 35).ok_or(
            NativeError::UnsupportedMechanic("captured melee item_template[35]"),
        )?;
        if staff.class != 2 || staff.subclass != 10 || staff.inventory_type != 17 {
            return Err(NativeError::UnsupportedMechanic(
                "captured melee two-hand staff type",
            ));
        }
        if staff.delay_ms == 0 || staff.delay_ms > 1_000_000 {
            return Err(NativeError::UnsupportedMechanic(
                "captured melee positive bounded staff delay",
            ));
        }
        let damage = &staff.damage;
        if damage[0].school != 0
            || damage[0].max_bits & 0x7fff_ffff == 0
            || damage[1..].iter().any(|line| {
                *line
                    != init::ItemDamage {
                        min_bits: 0,
                        max_bits: 0,
                        school: 0,
                    }
            })
        {
            return Err(NativeError::UnsupportedMechanic(
                "captured melee one physical damage line",
            ));
        }
        let minimum = spell::SpellFloat::from_bits(damage[0].min_bits)?;
        let maximum = spell::SpellFloat::from_bits(damage[0].max_bits)?;
        let dps = melee::weapon_dps(minimum, maximum, staff.delay_ms)
            .map_err(|_| NativeError::UnsupportedMechanic("captured melee staff damage range"))?;
        let combat = self
            .combat
            .as_ref()
            .ok_or(NativeError::UnsupportedMechanic(
                "captured melee initialized learning/equipment/stats",
            ))?;
        for id in [136, 162] {
            if !starter
                .skill_lines
                .iter()
                .any(|row| row.id == id && row.category == 6)
            {
                return Err(NativeError::UnsupportedMechanic(
                    "captured melee staff/unarmed SkillLine",
                ));
            }
            if !combat.skills.contains_key(&id) {
                return Err(NativeError::UnsupportedMechanic(
                    "captured melee initialized staff/unarmed skill",
                ));
            }
        }
        attack::validate(self, starter)?;
        if combat.input_identity != starter.source_identity() {
            return Err(NativeError::UnsupportedMechanic(
                "captured melee starter content binding",
            ));
        }
        if combat
            .equipment_weapons
            .iter()
            .any(|row| matches!(row.slot, 16 | 17))
        {
            return Err(NativeError::UnsupportedMechanic(
                "captured melee offhand/ranged equipment",
            ));
        }
        let weapon = combat
            .equipment_weapons
            .iter()
            .find(|row| row.slot == 15)
            .ok_or(NativeError::UnsupportedMechanic(
                "captured melee equipped starting staff",
            ))?;
        if weapon.item_id != staff.id
            || weapon.class != staff.class
            || weapon.subclass != staff.subclass
            || weapon.inventory_type != staff.inventory_type
            || weapon.skill != 136
            || weapon.delay_ms != staff.delay_ms
            || weapon.damage.iter().copied().ne(damage
                .iter()
                .map(|line| (line.min_bits, line.max_bits, line.school)))
        {
            return Err(NativeError::UnsupportedMechanic(
                "captured melee retained staff agreement",
            ));
        }
        self.initialize_player_melee(weapon_skill_gain)?;
        Ok(dps)
    }
}

#[cfg(test)]
mod tests;
