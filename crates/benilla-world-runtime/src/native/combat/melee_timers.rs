// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
//! Unit initialization/update/reset and Player::_ApplyItemBonuses in the retained
//! mainhand-only, idle, form-zero, no-disarm, speed-one domain. No melee attacks
//! or RNG are produced here. A release witness separately authenticates resets.
use super::*;

const MAINHAND: u8 = 15;
const OFFHAND: u8 = 16;
const RANGED: u8 = 17;
const BASE_ATTACK_TIME: u32 = 2000;
const BASE_ATTACK_FIELD: u32 = 0x44fa_0000;
const SPEED_ONE: u32 = 0x3f80_0000;

/// Required source state, independent of ordinary combat membership. Private
/// fields cannot be supplied as proof of a source-initialized combat domain.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerMeleeTimers {
    attack_time_fields: [u32; 2],
    attack_speed_factors: [u32; 2],
    remaining_ms: [u32; 2],
    melee_attacking: bool,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    victim: Option<crate::NpcId>,
    shapeshift_form: u8,
    disarmed: bool,
}

// Unit::SetAttackTime converts uint32 to binary32, multiplying by speed1.
// Encode the nearest-even conversion with integer arithmetic, including delays
// above 2^24. Host floating point and an ad-hoc integer-delay shortcut are avoided.
fn attack_field(value: u32) -> u32 {
    if value == 0 {
        return 0;
    }
    let mut exponent = 31 - value.leading_zeros();
    let mut significand = if exponent <= 23 {
        u64::from(value) << (23 - exponent)
    } else {
        let shift = exponent - 23;
        let high = u64::from(value >> shift);
        let remainder = value & ((1 << shift) - 1);
        let half = 1 << (shift - 1);
        high + u64::from(remainder > half || (remainder == half && high & 1 != 0))
    };
    if significand == 1 << 24 {
        significand >>= 1;
        exponent += 1;
    }
    ((exponent + 127) << 23) | (significand as u32 & 0x7f_ffff)
}

// Unit::GetAttackTime divides the float field by speed1 and truncates uint32;
// resetAttackTimer converts that integer to float, multiplies speed1 and truncates
// again. The first result is already binary32-exact, so both conversions agree.
fn source_attack_time(field: u32) -> Result<u32, NativeError> {
    let exponent = (field >> 23) & 0xff;
    if field >> 31 != 0 || !(127..=158).contains(&exponent) {
        return Err(NativeError::UnsupportedMechanic(
            "player melee attack time outside source uint32 conversion domain",
        ));
    }
    let exponent = exponent - 127;
    let significand = u64::from((field & 0x7f_ffff) | (1 << 23));
    let integer = if exponent >= 23 {
        significand << (exponent - 23)
    } else {
        significand >> (23 - exponent)
    };
    u32::try_from(integer).map_err(|_| NativeError::InvalidState)
}

fn source_slot(item: &InitialWeapon) -> bool {
    match item.class {
        // The retained initializer admits only mainhand placement. Ranged
        // subclasses cannot acquire melee authority from a forged inventory type.
        2 => {
            item.slot == MAINHAND
                && matches!(item.inventory_type, 13 | 17 | 21)
                && matches!(item.subclass, 0 | 1 | 4 | 5 | 6 | 7 | 8 | 10 | 13 | 15 | 20)
        }
        4 => match item.inventory_type {
            1..=10 => item.slot == (item.inventory_type - 1) as u8,
            11 => matches!(item.slot, 10 | 11),
            12 => matches!(item.slot, 12 | 13),
            16 => item.slot == 14,
            19 => item.slot == 18,
            20 => item.slot == 4,
            _ => false,
        },
        _ => false,
    }
}

fn retained_fields(
    weapons: &[InitialWeapon],
    equipment: &equipment::EquipmentSnapshot,
    recipe: &init::StatRecipeSnapshot,
) -> Result<[u32; 2], NativeError> {
    equipment::EquipmentState::from_snapshot(equipment.clone())?;
    if weapons.len() != equipment.items.len()
        || recipe.items.len() != equipment.items.len()
        || weapons.windows(2).any(|w| w[0].slot >= w[1].slot)
        || recipe.items.windows(2).any(|w| w[0].slot >= w[1].slot)
        || recipe.broken_slots.windows(2).any(|w| w[0] >= w[1])
        || recipe.broken_slots.iter().any(|slot| {
            recipe
                .items
                .iter()
                .all(|item| item.slot != *slot || item.max_durability == 0)
        })
    {
        return Err(NativeError::InvalidProfile);
    }
    let mut fields = [BASE_ATTACK_FIELD; 2];
    for ((weapon, equipped), modifier) in weapons.iter().zip(&equipment.items).zip(&recipe.items) {
        if weapon.slot != equipped.slot
            || modifier.slot != equipped.slot
            || weapon.item_id != equipped.item_id
            || modifier.item_id != equipped.item_id
            || modifier.max_durability != equipped.max_durability
            || modifier.weapon != (weapon.class == 2)
        {
            return Err(NativeError::InvalidProfile);
        }
        if matches!(weapon.slot, OFFHAND | RANGED) || !source_slot(weapon) {
            return Err(NativeError::UnsupportedMechanic(
                "player melee source class/slot outside mainhand-only domain",
            ));
        }
        // _ApplyItemBonuses uses IsWeapon and Delay, not damage. Broken-state
        // callbacks are represented by the recipe before durability is set to0.
        if weapon.class == 2 && weapon.delay_ms != 0 && !recipe.broken_slots.contains(&weapon.slot)
        {
            fields[0] = attack_field(weapon.delay_ms);
            source_attack_time(fields[0])?;
        }
    }
    Ok(fields)
}

impl PlayerMeleeTimers {
    pub(super) fn from_initializer(
        weapons: &[InitialWeapon],
        equipment: &equipment::EquipmentSnapshot,
        recipe: &init::StatRecipeSnapshot,
    ) -> Result<Self, NativeError> {
        if !recipe.broken_slots.is_empty()
            || equipment
                .items
                .iter()
                .any(|i| i.current_durability != i.max_durability)
        {
            return Err(NativeError::InvalidProfile);
        }
        Ok(Self {
            attack_time_fields: retained_fields(weapons, equipment, recipe)?,
            attack_speed_factors: [SPEED_ONE; 2],
            remaining_ms: [0; 2],
            // Unit::Unit initializes m_state0 and m_attacking=nullptr. No
            // admitted command, effect or callback is allowed to call Attack.
            melee_attacking: false,
            victim: None,
            shapeshift_form: 0,
            disarmed: false,
        })
    }

    pub fn remaining_ms(&self) -> [u32; 2] {
        self.remaining_ms
    }

    fn validate_shape(&self) -> Result<(), NativeError> {
        if self.shapeshift_form != 0
            || self.disarmed
            || self.attack_speed_factors != [SPEED_ONE; 2]
            || self.melee_attacking != self.victim.is_some()
            || self.remaining_ms[1] > 100
        {
            return Err(NativeError::InvalidState);
        }
        Ok(())
    }

    pub(in crate::native) fn victim(&self) -> Option<crate::NpcId> {
        self.victim
    }

    pub(in crate::native) fn melee_attacking(&self) -> bool {
        self.melee_attacking
    }

    /// Only the private source reducer writes these fields. Whole-owner
    /// validation independently derives every field from its joint proof.
    pub(in crate::native) fn write_source_fields(
        &mut self,
        victim: Option<crate::NpcId>,
        remaining_ms: [u32; 2],
    ) -> Result<(), NativeError> {
        let mut next = self.clone();
        next.victim = victim;
        next.melee_attacking = victim.is_some();
        next.remaining_ms = remaining_ms;
        next.validate_shape()?;
        *self = next;
        Ok(())
    }

    fn validate_idle(&self) -> Result<(), NativeError> {
        if self.melee_attacking
            || self.victim.is_some()
            || self.shapeshift_form != 0
            || self.disarmed
            || self.attack_speed_factors != [SPEED_ONE; 2]
            || self.remaining_ms[1] != 0
        {
            return Err(NativeError::UnsupportedMechanic(
                "player melee timers require idle form-zero no-disarm speed-one state",
            ));
        }
        Ok(())
    }

    pub(super) fn item_broken(
        &mut self,
        slot: u8,
        recipe: &init::StatRecipeSnapshot,
    ) -> Result<(), NativeError> {
        self.validate_shape()?;
        let item = recipe
            .items
            .iter()
            .find(|item| item.slot == slot)
            .ok_or(NativeError::InvalidState)?;
        if !recipe.broken_slots.contains(&slot) {
            return Err(NativeError::InvalidState);
        }
        if item.weapon {
            if slot != MAINHAND {
                return Err(NativeError::UnsupportedMechanic(
                    "player melee break outside mainhand-only domain",
                ));
            }
            // Delay0 did not change the initialized2000 field, so leaving the
            // field at2000 also represents its source no-write branch exactly.
            self.attack_time_fields[0] = BASE_ATTACK_FIELD;
        }
        Ok(())
    }
}

impl CombatGenesis {
    pub(in crate::native) fn validate_melee_genesis(&self) -> Result<(), NativeError> {
        if self.starter_identity.class != 8
            || self.melee_timers
                != PlayerMeleeTimers::from_initializer(
                    &self.equipment_weapons,
                    &self.equipment,
                    &self.recipe.snapshot(),
                )?
        {
            return Err(NativeError::InvalidProfile);
        }
        Ok(())
    }

    /// The caller separately authenticates the historical recipe. Immutable
    /// item identity and complete slot coverage must still agree with genesis.
    pub(in crate::native) fn source_attack_times_for_recipe(
        &self,
        recipe: &init::StatRecipeSnapshot,
    ) -> Result<[u32; 2], NativeError> {
        self.validate_melee_genesis()?;
        let fresh = self.recipe.snapshot();
        if recipe.items != fresh.items || recipe.version != fresh.version {
            return Err(NativeError::InvalidState);
        }
        let fields = retained_fields(&self.equipment_weapons, &self.equipment, recipe)?;
        Ok([
            source_attack_time(fields[0])?,
            source_attack_time(fields[1])?,
        ])
    }
}

impl PlayerCombatState {
    pub(in crate::native) fn validate_melee_timers(
        &self,
        genesis: &CombatGenesis,
    ) -> Result<(), NativeError> {
        self.melee_timers.validate_idle()?;
        self.validate_melee_timer_fields(genesis)
    }

    /// Structural component check. Active lifecycle/countdown authority is
    /// checked only by the selected whole-owner player-melee proof.
    pub(in crate::native) fn validate_melee_timer_fields(
        &self,
        genesis: &CombatGenesis,
    ) -> Result<(), NativeError> {
        self.melee_timers.validate_shape()?;
        let times = genesis.source_attack_times_for_recipe(&self.recipe.snapshot())?;
        let current = self.equipment.snapshot();
        // This private recipe is produced by live source operations or the
        // containing owner's independent history replay, never inactive flags.
        quest_progression::equipment_movement::restore_equipment(
            genesis,
            current.clone(),
            &self.recipe,
        )?;
        let broken = current
            .items
            .iter()
            .filter(|i| i.is_broken())
            .map(|i| i.slot)
            .collect::<Vec<_>>();
        if broken != self.recipe.snapshot().broken_slots
            || self.melee_timers.attack_time_fields != times.map(attack_field)
            || self.melee_timers.remaining_ms[0]
                > source_attack_time(genesis.melee_timers.attack_time_fields[0])?
                    .max(BASE_ATTACK_TIME)
        {
            return Err(NativeError::InvalidState);
        }
        Ok(())
    }

    pub(in crate::native) fn source_attack_times(
        &self,
        genesis: &CombatGenesis,
    ) -> Result<[u32; 2], NativeError> {
        self.validate_melee_timers(genesis)?;
        genesis.source_attack_times_for_recipe(&self.recipe.snapshot())
    }

    /// Spell::_handle_immediate_phase: BASE reset is unconditional even without
    /// a usable mainhand. Retained complete equipment proves no OFF weapon; a
    /// present unsupported offhand can never be converted into an absent one.
    pub(in crate::native) fn reset_melee_timers(
        &mut self,
        genesis: &CombatGenesis,
    ) -> Result<[u32; 2], NativeError> {
        let times = self.source_attack_times(genesis)?;
        self.melee_timers.remaining_ms[0] = times[0];
        Ok(times)
    }

    pub(in crate::native) fn reset_selected_melee_timer(
        &mut self,
        genesis: &CombatGenesis,
    ) -> Result<[u32; 2], NativeError> {
        self.validate_melee_timer_fields(genesis)?;
        let times = genesis.source_attack_times_for_recipe(&self.recipe.snapshot())?;
        self.melee_timers.remaining_ms[0] = times[0];
        Ok(times)
    }

    /// Unit::Update subtracts BASE/OFF after the event/spell update. A release
    /// during that update is therefore decremented by the very same diff.
    pub(in crate::native) fn advance_melee_timers(&mut self, diff: u32) {
        for timer in &mut self.melee_timers.remaining_ms {
            *timer = timer.saturating_sub(diff);
        }
    }
}

#[cfg(test)]
mod tests;
