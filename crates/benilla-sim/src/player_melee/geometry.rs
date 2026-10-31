// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Unit::CanReachWithMeleeAttack/DealMeleeDamage,
// Creature::GetShieldBlockValue. Source operands, not admission evidence.
use super::{numeric, MeleeError};
use crate::classic_spell::SpellFloat;
use crate::creature_ai::{combat_reach, AiError, Point, Real};
use rustc_apfloat::{ieee::Single, Float, Round};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerMeleeGeometry {
    pub player_position: Point,
    pub target_position: Point,
    pub player_reach: Real,
    pub target_reach: Real,
    pub player_moving: bool,
    pub player_walking: bool,
    pub target_moving: bool,
    pub target_walking: bool,
}

/// Player-controlled branch: strict 3D range, unlike the ordinary NPC's 2D
/// branch. Neither bounding radii nor renderer-axis coordinates belong here.
pub fn player_in_melee_range(input: PlayerMeleeGeometry) -> Result<bool, AiError> {
    let mut reach = combat_reach(input.player_reach, input.target_reach)?;
    if input.player_moving && !input.player_walking && input.target_moving && !input.target_walking
    {
        reach = reach.add(Real::constant(0x402a_aaab))?; // MELEE_LEEWAY = 8.0f / 3.0f
    }
    Ok(input
        .player_position
        .distance_squared(input.target_position)?
        .less(reach.mul(reach)?))
}

fn uint(value: Single) -> Result<u32, MeleeError> {
    let value = numeric::checked(value.to_u128_r(32, Round::TowardZero, &mut false))?;
    u32::try_from(value).map_err(|_| MeleeError::Domain("melee uint32 conversion"))
}

/// BASE_ATTACK branch only. Caller must establish a genuine parry outcome,
/// no active offhand, and absence of CREATURE_EXTRA_FLAG_NO_PARRY_HASTEN.
/// No timer reset or callback occurs when the source branch does not write.
pub fn mainhand_parry_haste(remaining_ms: u32, attack_time_ms: u32) -> Result<u32, MeleeError> {
    if remaining_ms > 1_000_000 || !(1..=1_000_000).contains(&attack_time_ms) {
        return Err(MeleeError::Domain("bounded mainhand parry timer"));
    }
    let remaining = numeric::integer(i64::from(remaining_ms))?;
    let interval = numeric::integer(i64::from(attack_time_ms))?;
    let twenty =
        numeric::checked(interval.mul_r(Single::from_bits(0x3e4c_cccd), Round::NearestTiesToEven))?;
    let sixty = numeric::checked(numeric::integer(3)?.mul_r(twenty, Round::NearestTiesToEven))?;
    if remaining > twenty && remaining <= sixty {
        uint(twenty)
    } else if remaining > sixty {
        let forty = numeric::checked(numeric::integer(2)?.mul_r(twenty, Round::NearestTiesToEven))?;
        uint(numeric::checked(
            remaining.sub_r(forty, Round::NearestTiesToEven),
        )?)
    } else {
        Ok(remaining_ms)
    }
}

/// Creature.h: integer level/2 plus truncation of current float strength/20.
/// Strength must be the retained source-derived NPC stat, not a guessed level
/// multiplier or a player stat. Ordinary creature levels are bounded to 1..63.
pub fn npc_shield_block(level: u8, strength: SpellFloat) -> Result<u32, MeleeError> {
    let strength = numeric::sf(strength);
    if !(1..=63).contains(&level)
        || strength < Single::ZERO
        || strength > numeric::integer(1_000_000)?
    {
        return Err(MeleeError::Domain("NPC block level/strength"));
    }
    let bonus = uint(numeric::checked(
        strength.div_r(numeric::integer(20)?, Round::NearestTiesToEven),
    )?)?;
    Ok(u32::from(level) / 2 + bonus)
}
