// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;

// UpdateFields.h:221-223.
const PLAYER_BLOCK_PERCENTAGE: usize = 0x452;
const PLAYER_DODGE_PERCENTAGE: usize = 0x453;
const PLAYER_PARRY_PERCENTAGE: usize = 0x454;

// StatSystem.cpp:474-488. Indexed by the live Unit class byte.
const BASE_DODGE: [f32; 12] = [
    0.0, 0.0, 0.75, 0.64, 0.0, 3.0, 0.0, 1.75, 3.25, 2.0, 0.0, 0.75,
];

impl SourcePlayer {
    // StatSystem.cpp:390-395.
    pub(super) fn update_defense_bonuses(
        &mut self,
        objects: &SourceObjects,
        unit: &mut SourceUnitFields,
    ) -> Result<()> {
        self.update_block_percentage(objects, unit)?;
        self.update_parry_percentage(objects, unit)?;
        self.update_dodge_percentage(objects, unit)
    }

    // StatSystem.cpp:397-413.
    fn update_block_percentage(
        &mut self,
        objects: &SourceObjects,
        unit: &mut SourceUnitFields,
    ) -> Result<()> {
        require(
            !unit.can_block,
            "execution.player.StatSystem_401.block_enabled_owner",
        )?;
        unit.mod_block_chance = 0.0;
        objects.set_update_value(
            self.object,
            PLAYER_BLOCK_PERCENTAGE,
            &mut self.block_percentage,
            0.0,
        )?;
        Ok(())
    }

    // StatSystem.cpp:454-471.
    fn update_parry_percentage(
        &mut self,
        objects: &SourceObjects,
        unit: &mut SourceUnitFields,
    ) -> Result<()> {
        require(
            !unit.can_parry,
            "execution.player.StatSystem_458.parry_enabled_owner",
        )?;
        unit.mod_parry_chance = 0.0;
        objects.set_update_value(
            self.object,
            PLAYER_PARRY_PERCENTAGE,
            &mut self.parry_percentage,
            0.0,
        )?;
        Ok(())
    }

    // StatSystem.cpp:490-503.
    fn update_dodge_percentage(
        &mut self,
        objects: &SourceObjects,
        unit: &mut SourceUnitFields,
    ) -> Result<()> {
        let class = unit.unit_class.unwrap_or(0) as usize;
        let mut value = BASE_DODGE.get(class).copied().unwrap_or(0.0);
        let (agility_dodge, level) = dodge_from_agility(unit, class)?;
        value += agility_dodge;
        // The empty aura list contributes zero.
        require(
            !unit.auras.has_aura_type(49),
            "execution.player.StatSystem_497.dodge_aura_modifier_owner",
        )?;
        unit.mod_dodge_chance = value;
        // Unit.cpp:2984-2992 reads the current skill with no target.
        // Unit.h:1454 multiplies the level and narrows the cap to uint16.
        let cap = level.wrapping_mul(5) as u16;
        let skill = self.skill_value(player_skills::SKILL_DEFENSE);
        value += (i32::from(skill) - i32::from(cap)) as f32 * 0.04;
        objects.set_update_value(
            self.object,
            PLAYER_DODGE_PERCENTAGE,
            &mut self.dodge_percentage,
            value.clamp(0.0, 100.0),
        )?;
        Ok(())
    }
}

// Player.cpp:4937-4979.
fn dodge_from_agility(unit: &SourceUnitFields, class: usize) -> Result<(f32, u32)> {
    let unported = match class {
        8 => None,
        2 | 7 | 11 => Some("execution.player.Player_4947.dodge_agility_class_owner"),
        4 => Some("execution.player.Player_4955.dodge_agility_class_owner"),
        3 => Some("execution.player.Player_4959.dodge_agility_class_owner"),
        5 => Some("execution.player.Player_4963.dodge_agility_class_owner"),
        9 => Some("execution.player.Player_4967.dodge_agility_class_owner"),
        1 => Some("execution.player.Player_4971.dodge_agility_class_owner"),
        _ => Some("execution.player.Player_4975.dodge_agility_class_owner"),
    };
    if let Some(id) = unported {
        return Err(refuse(id));
    }
    let level = unit
        .power_fields
        .as_ref()
        .ok_or_else(|| refuse("execution.player.Player_4977.dodge_level_owner"))?
        .level;
    let float_level = level as f32;
    let rate = 12.9_f32 * (60.0 - float_level) / 59.0 + 20.0_f32 * (float_level - 1.0) / 59.0;
    Ok((unit.resistances.agility() as f32 / rate, level))
}
