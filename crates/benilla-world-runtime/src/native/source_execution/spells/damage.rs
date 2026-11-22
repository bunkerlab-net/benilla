// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::{resistance::StatModifierGroup, *};

// UpdateFields.h:122-125,138,153-160.
const UNIT_FIELD_STAT0: usize = 0x96;
const ATTACK_POWER_FIELDS: [[usize; 3]; 2] = [[0xa5, 0xa6, 0xa7], [0xa8, 0xa9, 0xaa]];
const DAMAGE_FIELDS: [[usize; 2]; 3] = [[0x86, 0x87], [0x88, 0x89], [0xab, 0xac]];

#[derive(Clone, Copy)]
pub(super) enum DamageAttack {
    Mainhand,
    Offhand,
    Ranged,
}

pub struct DamageAuraTarget<'a> {
    pub(super) damage: &'a mut SourceCreatureDamage,
    pub(super) weapon: CreatureWeaponState<'a>,
    pub(super) can_modify_stats: bool,
}

impl<'a> DamageAuraTarget<'a> {
    pub fn new(
        damage: &'a mut SourceCreatureDamage,
        creature: &'a SourceLoadedCreatureFields,
        flags: u32,
        form: u32,
    ) -> Self {
        Self {
            damage,
            weapon: CreatureWeaponState {
                equipment: &creature.equipment,
                can_dual_wield: creature.can_dual_wield,
                flags,
                form,
            },
            can_modify_stats: creature.can_modify_stats,
        }
    }

    pub(super) fn reborrow(&mut self, form: u32) -> DamageAuraTarget<'_> {
        DamageAuraTarget {
            damage: self.damage,
            weapon: CreatureWeaponState {
                form,
                ..self.weapon
            },
            can_modify_stats: self.can_modify_stats,
        }
    }

    #[cfg(test)]
    pub fn unmodified(damage: &'a mut SourceCreatureDamage) -> Self {
        static CREATURE: std::sync::LazyLock<SourceLoadedCreatureFields> =
            std::sync::LazyLock::new(SourceLoadedCreatureFields::default);
        Self::new(damage, &CREATURE, 0, 0)
    }
}

#[derive(Clone, Copy)]
pub(in crate::native::source_execution) struct PlayerAttackPowerInputs {
    pub class: u32,
    pub level: u32,
    pub agility: u32,
    pub form: u32,
    pub form_agility_bonus: Option<bool>,
    pub attack_times: [f32; 3],
    pub unarmed: bool,
}

#[derive(Clone, Copy)]
pub(in crate::native::source_execution) struct CreatureWeaponState<'a> {
    pub equipment: &'a SourceCreatureEquipment,
    pub can_dual_wield: bool,
    pub flags: u32,
    pub form: u32,
}

impl CreatureWeaponState<'_> {
    // Creature.cpp:2773-2778.
    fn has_weapon(self, attack: usize) -> bool {
        self.equipment.slots[attack].info[0] as u8 == 2 || (attack == 1 && self.can_dual_wield)
    }

    // Creature.h:854; Unit.h:1191-1204; Unit.cpp:10889-10904.
    fn has_weapon_for_attack(self, attack: usize) -> bool {
        !matches!(self.form, 1 | 5 | 8)
            && !(attack == 0 && self.flags & 0x0020_0000 != 0)
            && self.has_weapon(attack)
    }
}

/// Creature.cpp:149,1462-1471,1482; Unit.cpp:367-380; Unit.h:2003,2455-2469.
#[derive(Clone, Debug)]
pub struct SourceCreatureDamage {
    groups: [StatModifierGroup; 3],
    ap_groups: [StatModifierGroup; 2],
    ap_modifiers: [[f32; 2]; 2],
    weapon: [[f32; 2]; 3],
    dps_percent: [f32; 3],
    base_ap: [f32; 2],
    multiplier: f32,
    create_strength: f32,
    create_agility: f32,
    strength: u32,
    ap: [i32; 2],
    ap_mod_words: [u32; 2],
    ap_multipliers: [f32; 2],
    damage: [[f32; 2]; 3],
}

impl Default for SourceCreatureDamage {
    fn default() -> Self {
        let mut groups = [StatModifierGroup::default(); 3];
        groups[1].total_percent = 0.5;
        Self {
            groups,
            ap_groups: [StatModifierGroup::default(); 2],
            ap_modifiers: [[0.0; 2]; 2],
            weapon: [[1.0, 2.0]; 3],
            dps_percent: [1.0; 3],
            base_ap: [0.0; 2],
            multiplier: 1.0,
            create_strength: 0.0,
            create_agility: 0.0,
            strength: 0,
            ap: [0; 2],
            ap_mod_words: [0; 2],
            ap_multipliers: [0.0; 2],
            damage: [[0.0; 2]; 3],
        }
    }
}

impl SourceCreatureDamage {
    pub(super) fn group(&mut self, attack: DamageAttack) -> &mut StatModifierGroup {
        &mut self.groups[attack as usize]
    }

    // Player.cpp:7150-7151 and Unit.h:2034 write the live first damage line.
    pub(in crate::native::source_execution) fn set_mainhand_base_weapon(
        &mut self,
        value: [f32; 2],
    ) {
        self.weapon[0] = value;
    }

    #[cfg(test)]
    pub(in crate::native::source_execution) fn mainhand_base_weapon(&self) -> [f32; 2] {
        self.weapon[0]
    }
    pub(in crate::native::source_execution) fn load_level_operands(
        &mut self,
        stats: &benilla_sim::creature_init::SourceLevelStats,
    ) {
        self.weapon = stats
            .weapon_damage_bits
            .map(|range| range.map(f32::from_bits));
        self.base_ap = stats.base_attack_power_bits.map(f32::from_bits);
        self.create_strength = f32::from_bits(stats.create_attributes_bits[0]);
        self.create_agility = f32::from_bits(stats.create_attributes_bits[1]);
        self.multiplier = f32::from_bits(stats.damage_multiplier_bits);
    }

    pub(in crate::native::source_execution) fn set_strength(
        &mut self,
        objects: &SourceObjects,
        unit: ObjectHandle,
        value: i32,
    ) -> Result<()> {
        objects.set_update_value(
            unit,
            UNIT_FIELD_STAT0,
            &mut self.strength,
            value.max(0) as u32,
        )?;
        Ok(())
    }

    // StatSystem.cpp:675-736. Strength updates melee before agility is written.
    pub(in crate::native::source_execution) fn update_attack_power_and_damage(
        &mut self,
        objects: &SourceObjects,
        unit: ObjectHandle,
        class: u32,
        agility: u32,
        weapon: CreatureWeaponState<'_>,
        ranged: bool,
    ) -> Result<()> {
        let kind = usize::from(ranged);
        let agility = agility as f32;
        let mut base = if ranged {
            agility - self.create_agility
        } else {
            let strength = self.strength as f32;
            match class {
                8 => strength,
                4 => (strength - self.create_strength) + agility - self.create_agility,
                _ => (strength - self.create_strength) * 2.0,
            }
        };
        base += self.base_ap[kind] * self.multiplier;
        let group = &mut self.ap_groups[kind];
        group.base_value = base;
        let base = group.base_value * group.base_percent;
        let multiplier = group.total_percent() - 1.0;
        let [field, field_mod, field_multiplier] = ATTACK_POWER_FIELDS[kind];
        require(
            base > -1.0 && base < 4_294_967_296.0,
            if ranged {
                "execution.creature.UpdateAttackPowerAndDamage_ranged_conversion_undefined"
            } else {
                "execution.creature.UpdateAttackPowerAndDamage_melee_conversion_undefined"
            },
        )?;
        objects.set_update_value(unit, field, &mut self.ap[kind], base as u32 as i32)?;
        for (half, amount) in self.ap_modifiers[kind].into_iter().enumerate() {
            require(
                amount > -32_769.0 && amount < 32_768.0,
                if half == 0 {
                    "execution.spells.StatSystem_725.attack_power_mod_float_to_int16_undefined"
                } else {
                    "execution.spells.StatSystem_726.attack_power_mod_float_to_int16_undefined"
                },
            )?;
            let shift = half * 16;
            let packed = (self.ap_mod_words[kind] & !(0xffff << shift))
                | (u32::from(amount as i16 as u16) << shift);
            objects.set_update_value(unit, field_mod, &mut self.ap_mod_words[kind], packed)?;
        }
        objects.set_update_value(
            unit,
            field_multiplier,
            &mut self.ap_multipliers[kind],
            multiplier,
        )?;
        if ranged {
            self.update_damage_physical(objects, unit, weapon, DamageAttack::Ranged)
        } else {
            self.update_damage_physical(objects, unit, weapon, DamageAttack::Mainhand)?;
            self.update_damage_physical(objects, unit, weapon, DamageAttack::Offhand)
        }
    }

    // StatSystem.cpp:203-286. Player and creature AP use the same live field storage.
    pub(in crate::native::source_execution) fn update_player_attack_power_and_damage(
        &mut self,
        objects: &SourceObjects,
        unit: ObjectHandle,
        input: PlayerAttackPowerInputs,
        ranged: bool,
    ) -> Result<()> {
        let level = input.level as f32;
        let agility = input.agility as f32;
        let strength = self.strength as f32;
        let base = if ranged {
            match input.class {
                3 => level * 2.0 + agility * 2.0 - 10.0,
                1 | 4 => level + agility - 10.0,
                11 if matches!(input.form, 1 | 5 | 8) => 0.0,
                _ => agility - 10.0,
            }
        } else {
            match input.class {
                1 | 2 => level * 3.0 + strength * 2.0 - 20.0,
                3 | 4 => level * 2.0 + strength + agility - 20.0,
                7 => level * 2.0 + strength * 2.0 - 20.0,
                11 => {
                    let mut base = level * 3.0 + strength * 2.0 - 20.0;
                    if input.form != 0
                        && input.form_agility_bonus.ok_or_else(|| {
                            refuse("execution.stats.StatSystem_255.shapeshift_form_owner")
                        })?
                    {
                        base += agility;
                    }
                    base
                }
                5 | 8 | 9 => strength - 10.0,
                _ => 0.0,
            }
        };
        let kind = usize::from(ranged);
        let group = &mut self.ap_groups[kind];
        group.base_value = base;
        let base = group.base_value * group.base_percent;
        let multiplier = group.total_percent() - 1.0;
        let [field, field_mod, field_multiplier] = ATTACK_POWER_FIELDS[kind];
        require(
            (-1.0..4_294_967_296.0).contains(&base) && base != -1.0,
            "execution.stats.StatSystem_273.attack_power_float_to_uint32_undefined",
        )?;
        objects.set_update_value(unit, field, &mut self.ap[kind], base as u32 as i32)?;
        for (half, amount) in self.ap_modifiers[kind].into_iter().enumerate() {
            let (amount, id) = if half == 0 {
                (
                    amount + 0.0,
                    "execution.stats.StatSystem_274.attack_power_mod_conversion_undefined",
                )
            } else {
                (
                    amount,
                    "execution.stats.StatSystem_275.attack_power_mod_conversion_undefined",
                )
            };
            let word = if half == 0 {
                // The positive half converts through int32 before narrowing.
                require(amount >= i32::MIN as f32 && amount < 2_147_483_648.0, id)?;
                amount as i32 as u16
            } else {
                require(amount > -32_769.0 && amount < 32_768.0, id)?;
                amount as i16 as u16
            };
            let shift = half * 16;
            let packed =
                (self.ap_mod_words[kind] & !(0xffff << shift)) | (u32::from(word) << shift);
            objects.set_update_value(unit, field_mod, &mut self.ap_mod_words[kind], packed)?;
        }
        objects.set_update_value(
            unit,
            field_multiplier,
            &mut self.ap_multipliers[kind],
            multiplier,
        )?;

        require(
            input.unarmed && input.form == 0,
            if ranged {
                "execution.stats.StatSystem_280.player_weapon_damage_owner"
            } else {
                "execution.stats.StatSystem_283.player_weapon_damage_owner"
            },
        )?;
        self.update_unarmed_player_damage(
            objects,
            unit,
            if ranged { 2 } else { 0 },
            input.attack_times,
        )
    }

    // StatSystem.cpp:302-387. Form zero, no equipped weapons, enchantments or ammo.
    fn update_unarmed_player_damage(
        &mut self,
        objects: &SourceObjects,
        unit: ObjectHandle,
        attack: usize,
        attack_times: [f32; 3],
    ) -> Result<()> {
        let speed = attack_times[attack] / 1000.0;
        let group = &self.groups[attack];
        let base = group.base_value + self.total_attack_power(attack)? / 14.0 * speed;
        let damage = self.weapon[attack].map(|value| {
            let weapon = value * self.dps_percent[attack];
            ((base + weapon) * group.base_percent + group.total_value) * group.total_percent()
        });
        for ((field, current), value) in DAMAGE_FIELDS[attack]
            .into_iter()
            .zip(&mut self.damage[attack])
            .zip(damage)
        {
            objects.set_update_value(
                unit,
                field,
                current,
                if value < 0.0 { 0.0 } else { value },
            )?;
        }
        Ok(())
    }

    // Unit.cpp:9175-9187 reads ranged mods as one signed word.
    fn total_attack_power(&self, attack: usize) -> Result<f32> {
        let kind = usize::from(attack == 2);
        let word = self.ap_mod_words[kind];
        let ap = if kind == 1 {
            self.ap[kind].checked_add(word as i32).ok_or_else(|| {
                refuse("execution.spells.Unit_9179.ranged_attack_power_add_undefined")
            })?
        } else {
            self.ap[kind]
                .checked_add(i32::from(word as i16))
                .and_then(|ap| ap.checked_add(i32::from((word >> 16) as i16)))
                .ok_or_else(|| {
                    refuse("execution.spells.Unit_9184.melee_attack_power_add_undefined")
                })?
        };
        if ap < 0 {
            return Ok(0.0);
        }
        Ok((ap as f32) * (1.0 + self.ap_multipliers[kind]))
    }

    // StatSystem.cpp:739-806; Unit.cpp:9190-9195.
    pub(super) fn update_damage_physical(
        &mut self,
        objects: &SourceObjects,
        unit: ObjectHandle,
        weapon: CreatureWeaponState<'_>,
        attack: DamageAttack,
    ) -> Result<()> {
        let attack = attack as usize;
        let group = &self.groups[attack];
        let base = group.base_value + (self.total_attack_power(attack)? / 14.0);
        let usable = weapon.has_weapon_for_attack(attack);
        let base_weapon = if attack == 1 && !usable {
            [0.0; 2]
        } else {
            self.weapon[attack].map(|value| value * self.dps_percent[attack])
        };
        let scaled = base_weapon.map(|value| value * self.multiplier);
        let conditional = if weapon.has_weapon(attack) && !usable {
            0.5
        } else {
            1.0
        };
        let damage = scaled.map(|value| {
            (((base + value) * group.base_percent + group.total_value) * group.total_percent())
                * conditional
        });
        for ((field, current), value) in DAMAGE_FIELDS[attack]
            .into_iter()
            .zip(&mut self.damage[attack])
            .zip(damage)
        {
            // Object.cpp:872-881,922-927 uses a numeric float comparison.
            objects.set_update_value(
                unit,
                field,
                current,
                if value < 0.0 { 0.0 } else { value },
            )?;
        }
        Ok(())
    }

    #[cfg(test)]
    pub(in crate::native::source_execution) fn damage_bits(&self) -> [[u32; 2]; 3] {
        self.damage.map(|range| range.map(f32::to_bits))
    }

    #[cfg(test)]
    pub(in crate::native::source_execution) fn attack_power_words(&self) -> [u32; 2] {
        self.ap.map(|value| value as u32)
    }
}

#[cfg(test)]
#[path = "damage_tests.rs"]
mod tests;
