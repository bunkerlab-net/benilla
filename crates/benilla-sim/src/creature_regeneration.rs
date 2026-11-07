// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
// Source grant GPL-2.0-or-later; Rust translation GPL-3.0-or-later.
//! Ordinary, uncontrolled Creature::RegenerateAll health path. See ../CREATURE-REGENERATION.md.
use serde::{Deserialize, Serialize};

pub const CREATURE_REGENERATION_POLICY_ID: &str = "cmangos-8ec338a1-ordinary-creature-health-v1";
pub const REGEN_INTERVAL_MS: u32 = 5_000;
const HEALTH: u32 = 0x2;
const POWER: u32 = 0xc;
const INTERACTION_IN_COMBAT: u32 = 0x40000;
const PLAYER_CONTROLLED: u32 = 0x8;
const MAX_HEALTH: u32 = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CreatureKind {
    Unknown,
    Ordinary,
    PetOrRegenerationOverride,
}
/// Actual active power kind and maximum, including a captured zero. No default:
/// lack of a mana field is not evidence that RegeneratePower is irrelevant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum PowerEvidence {
    Unknown,
    Known { power_type: u32, max_power: u32 },
}
/// Immutable captured inputs. Their presence is not proof of capture completeness.
/// Any entry/flag/control/power-max mutation requires a separately implemented path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub kind: CreatureKind,
    pub regenerate_stats: u32,
    pub creature_type_flags: u32,
    /// Effective UNIT_FIELD_FLAGS, after creation overrides.
    pub unit_flags: u32,
    pub power: PowerEvidence,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid(&'static str),
    Unsupported(&'static str),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ordinary creature regeneration: {self:?}")
    }
}
impl std::error::Error for Error {}
impl Policy {
    pub fn validate(self) -> Result<(), Error> {
        if self.kind != CreatureKind::Ordinary {
            return Err(Error::Unsupported(
                "unknown creature kind or regeneration override",
            ));
        }
        if self.unit_flags & PLAYER_CONTROLLED != 0 {
            return Err(Error::Unsupported(
                "player-controlled creature regeneration",
            ));
        }
        if self.regenerate_stats & !0xf != 0 {
            return Err(Error::Invalid("RegenerateStats bits"));
        }
        let PowerEvidence::Known {
            power_type,
            max_power,
        } = self.power
        else {
            return Err(Error::Unsupported("unknown active power regeneration"));
        };
        if power_type > 4 || max_power > MAX_HEALTH {
            return Err(Error::Invalid("active power kind or maximum"));
        }
        // IsRegeneratingPower gates both combat states; zero maxima return before
        // calculation; rage/happiness hit the switch default before aura modifiers.
        if self.regenerate_stats & POWER != 0 && max_power != 0 && matches!(power_type, 0 | 2 | 3) {
            return Err(Error::Unsupported("active mana/focus/energy regeneration"));
        }
        Ok(())
    }
    pub fn health_regenerates(self) -> bool {
        self.regenerate_stats & HEALTH != 0 && self.creature_type_flags & INTERACTION_IN_COMBAT == 0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct State {
    pub elapsed_ms: u32,
    pub health: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub diff_ms: u32,
    /// Creature::Update entered ALIVE, and remains alive after Unit::Update.
    /// False for corpse/dead and the DEAD -> JUST_ALIVED transition update.
    pub alive_update: bool,
    /// Both facts must be sampled after AI and CombatManager::Update.
    pub in_combat: bool,
    pub evade_regeneration: bool,
}
/// Constructor timer is zero. Death, evade, home and respawn never reset it.
/// Only the host knows whether this update entered ALIVE and survived Unit::Update.
pub fn update(
    policy: Policy,
    max_health: u32,
    before: State,
    frame: Frame,
) -> Result<State, Error> {
    policy.validate()?;
    if max_health == 0 || max_health > MAX_HEALTH || before.health > max_health {
        return Err(Error::Invalid("health bounds"));
    }
    if !frame.alive_update {
        return Ok(before);
    }
    if before.health == 0 {
        return Err(Error::Invalid("alive update with zero health"));
    }
    let mut next = before;
    // Native uint32 addition wraps; runtime fixed steps keep this accumulator <5000.
    next.elapsed_ms = before.elapsed_ms.wrapping_add(frame.diff_ms);
    if next.elapsed_ms < REGEN_INTERVAL_MS {
        return Ok(next);
    }
    if (!frame.in_combat || frame.evade_regeneration) && policy.health_regenerates() {
        // RegenerateHealth uint32 max/3 -> ModifyHealth(int32), safe in this bound.
        next.health = max_health.min(before.health + max_health / 3);
    }
    // Exactly one subtraction even when health is full, disabled or combat-blocked.
    next.elapsed_ms -= REGEN_INTERVAL_MS;
    Ok(next)
}
