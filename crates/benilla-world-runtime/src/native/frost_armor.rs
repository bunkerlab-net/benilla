// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f.
// SpellAuras.cpp:3367-3386,3934-3941; UnitAuraProcHandler.cpp:343-406;
// Unit.cpp:2659-2661,8337-8431,10474-10490. Upstream GPL-2.0-or-later.
//! Source-bound Frost Armor and its victim-side Chilled proc. No general aura dispatcher.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrostArmorHolder {
    pub aura: spell::TimedAura,
    pub applied_tick: u64,
    pub last_update_tick: u64,
    pub cast_id: u64,
    pub level: u8,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChilledHolder {
    pub slot: u8,
    pub aura: spell::TimedAura,
    pub applied_tick: u64,
    pub last_update_tick: u64,
    pub action: u64,
    pub target: crate::NpcId,
    pub caster_level: u8,
    pub target_level: u8,
    pub armor: FrostArmorHolder,
    pub draw: Draw,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum FrostArmorEvent {
    Cast {
        cast_id: u64,
        failure: Option<spell::CastFailure>,
        mana_spent: u32,
    },
    Applied,
    Removed,
    Chilled {
        target: crate::NpcId,
        action: u64,
        missed: bool,
    },
    ChilledRemoved {
        target: crate::NpcId,
    },
    NpcSpeeds {
        target: crate::NpcId,
        speeds: [ai::Real; 3],
    },
}

pub(in crate::native) fn source(
    profile: &Profile,
    id: u32,
) -> Result<&SpellAuthoritySource, NativeError> {
    profile
        .spell_location_catalog()
        .and_then(|c| c.frost_source(id))
        .ok_or(NativeError::AuthorityUnavailable(
            "Frost Armor source closure",
        ))
}
pub(in crate::native) fn resources(raw: &SpellAuthoritySource) -> spell::InstantAuraResources {
    let w = &raw.words;
    spell::InstantAuraResources {
        spell_id: w[0],
        mana_cost: w[32],
        global_category: w[157],
        global_recovery_ms: w[158],
        recovery_ms: w[19],
        category: w[2],
        category_recovery_ms: w[20],
    }
}
pub(in crate::native) fn spec(
    raw: &SpellAuthoritySource,
    level: u8,
) -> Result<spell::AuraLifecycleSpec, NativeError> {
    let amount = |i: usize| -> Result<i32, NativeError> {
        let w = &raw.words;
        Ok(spell::EffectValue {
            base_points: w[76 + i] as i32,
            base_dice: w[67 + i] as i32,
            die_sides: w[64 + i] as i32,
            points_per_level: spell::SpellFloat::ZERO,
            dice_per_level: spell::SpellFloat::ZERO,
            base_level: w[28] as u8,
            spell_level: w[29] as u8,
            max_level: w[27] as u8,
        }
        .evaluate(u16::from(level), None)?)
    };
    let result = match raw.words[0] {
        168 => spell::AuraLifecycleSpec::ArmorProc {
            armor: amount(0)?,
            trigger_spell: 6136,
        },
        6136 => spell::AuraLifecycleSpec::Chilled {
            melee_haste: amount(0)?,
            movement_slow: amount(1)?,
        },
        _ => return Err(NativeError::InvalidProfile),
    };
    result.validate()?;
    Ok(result)
}
pub(in crate::native) fn duration(raw: &SpellAuthoritySource) -> Result<u32, NativeError> {
    raw.duration
        .and_then(|d| u32::try_from(d.base_ms).ok())
        .filter(|d| *d > 0 && *d <= 3_600_000)
        .ok_or(NativeError::InvalidProfile)
}

impl FrostArmorHolder {
    pub(in crate::native) fn validate(
        &self,
        profile: &Profile,
        tick: u64,
        next_cast: u64,
    ) -> Result<(), NativeError> {
        if self.cast_id == 0
            || self.cast_id >= next_cast
            || !(1..=60).contains(&self.level)
            || self.applied_tick == 0
            || self.applied_tick > tick
            || self.last_update_tick != tick
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        let raw = source(profile, 168)?;
        let elapsed = (tick - self.applied_tick + 1)
            .checked_mul(u64::from(profile.tick_ms))
            .and_then(|v| u32::try_from(v).ok())
            .ok_or(NativeError::InvalidCheckpoint)?;
        let mut expected = spell::TimedAura::new(spec(raw, self.level)?, duration(raw)?)?;
        expected.update(elapsed)?;
        if expected != self.aura || self.aura.remaining_ms() == 0 {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok(())
    }
}
impl ChilledHolder {
    pub(in crate::native) fn validate(
        &self,
        profile: &Profile,
        state: &State,
        npc: &Npc,
    ) -> Result<(), NativeError> {
        self.armor
            .validate(profile, self.applied_tick, state.next_cast_id)?;
        if self.target != npc.id
            || self.applied_tick > state.tick
            || self.last_update_tick != state.tick
            || self.action == 0
            || self.action
                >= state
                    .combat
                    .as_ref()
                    .ok_or(NativeError::InvalidCheckpoint)?
                    .next_action
            || !(1..=60).contains(&self.caster_level)
            || !(1..=60).contains(&self.target_level)
            || self.caster_level < self.armor.level
            || npc.health == 0
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        let raw = source(profile, 6136)?;
        let elapsed = (state.tick - self.applied_tick)
            .checked_mul(u64::from(profile.tick_ms))
            .and_then(|v| u32::try_from(v).ok())
            .ok_or(NativeError::InvalidCheckpoint)?;
        let mut expected = spell::TimedAura::new(spec(raw, self.caster_level)?, duration(raw)?)?;
        expected.update(elapsed)?;
        let mut counter = self.draw.first_counter;
        let sample = rng::sample_inclusive_v2(state.world, state.seed, &mut counter, 0, 9999)?;
        let miss = spell::zero_resistance_binary_npc_miss(
            self.caster_level,
            self.target_level,
            spell::SpellFloat::from_bits(0x4080_0000)?,
        )?;
        if expected != self.aura
            || self.aura.remaining_ms() == 0
            || self.draw.cast_id != 0
            || self.draw.purpose
                != (DrawPurpose::FrostArmorHit {
                    action: self.action,
                    npc: self.target,
                })
            || self.draw.low != 0
            || self.draw.high != 9999
            || self.draw.value != sample.value
            || self.draw.consumed != sample.consumed_words
            || counter > state.rng_counter
            || sample.value < i64::from(miss)
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok(())
    }
}
impl State {
    pub fn frost_armor(&self) -> Option<&FrostArmorHolder> {
        self.frost_armor.as_ref()
    }
    pub fn effective_armor(&self) -> Result<i32, NativeError> {
        self.combat
            .as_ref()
            .ok_or(NativeError::InvalidState)?
            .stats
            .resistances[0]
            .checked_add(self.frost_armor.as_ref().map_or(0, |h| h.aura.armor()))
            .ok_or(NativeError::InvalidState)
    }
    pub(in crate::native) fn validate_frost_armor(
        &self,
        profile: &Profile,
    ) -> Result<(), NativeError> {
        if let Some(holder) = &self.frost_armor {
            holder.validate(profile, self.tick, self.next_cast_id)?;
        }
        for npc in self.npcs.values() {
            if let Some(holder) = &npc.chilled {
                holder.validate(profile, self, npc)?;
                if npc.ai.effective_profile() != &npc.effective_profile(profile)?.ai {
                    return Err(NativeError::InvalidCheckpoint);
                }
            }
            if !matches!(npc.fireball_aura_slot, 32 | 33)
                || npc.chilled.as_ref().is_some_and(|h| {
                    !matches!(h.slot, 32 | 33)
                        || npc.auras.contains_key(&133) && h.slot == npc.fireball_aura_slot
                })
            {
                return Err(NativeError::InvalidCheckpoint);
            }
        }
        Ok(())
    }
}

/// Raw model rate is multiplied before the source minimum-rate clamp.
pub(in crate::native) fn modify_profile(
    profile: &Profile,
    spawn: u64,
    base: &mut NpcProfile,
    holder: &ChilledHolder,
) -> Result<(), NativeError> {
    base.ai.mainhand_interval_ms = holder.aura.attack_time(base.ai.mainhand_interval_ms)?;
    let input = &profile
        .creature_generations
        .as_ref()
        .and_then(|c| c.source(spawn))
        .ok_or(NativeError::InvalidProfile)?
        .input;
    let benilla_sim::creature_init::Rows::Complete(models) = &input.models else {
        return Err(NativeError::InvalidProfile);
    };
    let model = models
        .iter()
        .find(|row| row.id == base.display_id)
        .ok_or(NativeError::InvalidProfile)?;
    let rate = if input.template.speed_run.bits() & 0x7fff_ffff != 0 {
        input.template.speed_run
    } else {
        model.speed_run
    };
    let spell::AuraLifecycleSpec::Chilled { movement_slow, .. } = holder.aura.spec() else {
        return Err(NativeError::InvalidState);
    };
    base.ai.run_speed = benilla_sim::player_slow::creature_run_speed(
        movement_slow,
        ai::Real::from_bits(rate.bits())?,
    )?;
    base.ai.home_speed = base.ai.run_speed;
    Ok(())
}

impl Npc {
    pub fn chilled_speeds(&self, profile: &Profile) -> Result<[ai::Real; 3], NativeError> {
        use benilla_sim::player_slow::{MoveType, SpeedRates};
        let rates = self
            .chilled
            .as_ref()
            .map(|h| h.aura.movement_rates())
            .transpose()?
            .unwrap_or(SpeedRates::normal());
        Ok([
            self.effective_profile(profile)?.ai.run_speed,
            rates.speed(MoveType::RunBack)?,
            rates.speed(MoveType::Swim)?,
        ])
    }
}
