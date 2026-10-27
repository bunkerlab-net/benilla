// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
// Original GPL-2.0-or-later grant; Rust translation GPL-3.0-or-later.
//! Mage mainhand arithmetic against an ordinary, living, uncontrolled NPC.
//! One physical damage line, no immunity/evade, auras, disarm, damage/crit
//! modifiers, absorbs, procs or scripts. Inputs are resolved source operands,
//! NOT admission evidence. This does not apply health, timers or callbacks.
//! A runtime must additionally execute parry haste, skills/reactives,
//! durability, threat, death/loot and combat callbacks transactionally.
//! Unit.cpp CalculateMeleeDamage/RollMeleeOutcomeAgainst supply the arithmetic;
//! DealMeleeDamage/ProcSkillsAndReactives are deliberately not implemented here.
//! In particular, parry haste precedes damage and attacker weapon-skill attempts
//! can follow a lethal hit. A missing callback must refuse before committing.
//! The planned native consumer derives these operands from current retained
//! weapon/skill/derived-stat state and raw creature initialization, not clients.
mod geometry;
mod numeric;
mod skill;
pub use geometry::{
    mainhand_parry_haste, npc_shield_block, player_in_melee_range, PlayerMeleeGeometry,
};
pub use skill::{resolve_weapon_gain, weapon_gain_chance, WeaponSkill, WeaponSkillResolution};
use crate::classic::software_armor_reduced_damage;
use crate::classic_spell::SpellFloat;

pub const PLAYER_MELEE_POLICY_ID: &str = "cmangos-8ec338a1-mage-npc-mainhand-softfloat-v1";
const MAX_AMOUNT: u32 = 1_000_000;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeleeError {
    Domain(&'static str),
    Roll(&'static str),
}
impl std::fmt::Display for MeleeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid player mainhand arithmetic: {self:?}")
    }
}
impl std::error::Error for MeleeError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MageAttack {
    pub level: u8,
    /// Effective live skill, including source bonuses, not its level cap.
    pub weapon_skill: u16,
    /// Current UNIT_FIELD_MINDAMAGE/MAXDAMAGE, including AP, not item damage.
    pub minimum_damage: SpellFloat,
    pub maximum_damage: SpellFloat,
    /// m_modCritChance, NOT the skill-adjusted UI percentage.
    pub critical_chance: SpellFloat,
    pub hit_chance: SpellFloat,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreatureDefense {
    /// Ordinary GetLevelForTarget; defense is level * 5 in this domain.
    pub level: u8,
    pub armor: i32,
    pub can_react: bool,
    pub casting_non_melee: bool,
    pub can_dodge: bool,
    pub can_parry: bool,
    pub can_block: bool,
    /// IsFacingTargetsFront: BOTH units' PI arcs, not scheduler's 120° arc.
    pub front_facing: bool,
    pub shield_block_value: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeleeInput {
    pub attacker: MageAttack,
    pub defender: CreatureDefense,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeleeChances {
    pub miss: u16,
    pub dodge: u16,
    pub parry: u16,
    pub block: u16,
    pub glance: u16,
    pub critical: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeleeOutcome {
    Miss,
    Dodge,
    Parry,
    Block,
    Glancing,
    Critical,
    Normal,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeleePlan {
    input: MeleeInput,
    range: (u32, u32),
    chances: MeleeChances,
}
impl MeleePlan {
    pub fn damage_range(self) -> (u32, u32) {
        self.range
    }
    pub fn chances(self) -> MeleeChances {
        self.chances
    }
    pub fn outcome(self, roll: u16) -> Result<MeleeOutcome, MeleeError> {
        if !(1..=10000).contains(&roll) {
            return Err(MeleeError::Roll("outcome die"));
        }
        let c = self.chances;
        let mut cumulative = 0_u32;
        for (chance, outcome) in [
            (c.miss, MeleeOutcome::Miss),
            (c.dodge, MeleeOutcome::Dodge),
            (c.parry, MeleeOutcome::Parry),
            (c.block, MeleeOutcome::Block),
            (c.glance, MeleeOutcome::Glancing),
            (c.critical, MeleeOutcome::Critical),
        ] {
            cumulative += u32::from(chance);
            if u32::from(roll) <= cumulative {
                return Ok(outcome);
            }
        }
        Ok(MeleeOutcome::Normal)
    }
    /// Query only after consuming damage then outcome draws. None means no
    /// third draw, including a glancing result with negative skill difference.
    pub fn glance_range(self, outcome_roll: u16) -> Result<Option<(u32, u32)>, MeleeError> {
        if self.outcome(outcome_roll)? == MeleeOutcome::Glancing {
            numeric::glance_range(self.input.attacker, self.input.defender)
        } else {
            Ok(None)
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeleeRolls {
    /// Inclusive draw, even when range endpoints are equal; consumed FIRST.
    pub damage: u32,
    /// Inclusive urand(1,10000), consumed SECOND.
    pub outcome: u16,
    /// Exactly the conditional third draw requested by glance_range.
    pub glance: Option<u32>,
}
/// Numeric output only; no health or capability mutation API is provided.
#[derive(Debug, PartialEq, Eq)]
pub struct MeleeResolution {
    outcome: MeleeOutcome,
    rolled_damage: u32,
    armored_damage: u32,
    total_damage: u32,
    clean_damage: u32,
    blocked_amount: u32,
}
impl MeleeResolution {
    pub fn outcome(&self) -> MeleeOutcome {
        self.outcome
    }
    pub fn rolled_damage(&self) -> u32 {
        self.rolled_damage
    }
    pub fn armored_damage(&self) -> u32 {
        self.armored_damage
    }
    pub fn total_damage(&self) -> u32 {
        self.total_damage
    }
    pub fn clean_damage(&self) -> u32 {
        self.clean_damage
    }
    pub fn blocked_amount(&self) -> u32 {
        self.blocked_amount
    }
    pub fn sub_damage(&self) -> u32 {
        self.total_damage
    }
}

pub fn prepare(input: MeleeInput) -> Result<MeleePlan, MeleeError> {
    let a = input.attacker;
    let d = input.defender;
    if !(1..=60).contains(&a.level) || !(1..=63).contains(&d.level) || a.weapon_skill > 1000 {
        return Err(MeleeError::Domain("level/weapon skill"));
    }
    if d.armor.unsigned_abs() > MAX_AMOUNT || d.shield_block_value > MAX_AMOUNT {
        return Err(MeleeError::Domain("defense stats"));
    }
    let range = numeric::roll_range(a.minimum_damage, a.maximum_damage)?;
    if range.1 > MAX_AMOUNT {
        return Err(MeleeError::Domain("damage magnitude"));
    }
    let chances = numeric::chances(a, d)?;
    Ok(MeleePlan {
        input,
        range,
        chances,
    })
}

pub fn resolve(plan: MeleePlan, rolls: MeleeRolls) -> Result<MeleeResolution, MeleeError> {
    if !(plan.range.0..=plan.range.1).contains(&rolls.damage) {
        return Err(MeleeError::Roll("inclusive damage range"));
    }
    let outcome = plan.outcome(rolls.outcome)?;
    match (plan.glance_range(rolls.outcome)?, rolls.glance) {
        (Some((low, high)), Some(roll)) if (low..=high).contains(&roll) => {}
        (None, None) => {}
        _ => return Err(MeleeError::Roll("conditional glancing draw")),
    }
    let armored = software_armor_reduced_damage(
        plan.input.attacker.level,
        plan.input.defender.armor,
        0,
        rolls.damage,
    )
    .map_err(|_| MeleeError::Domain("armor"))?;
    let (total, clean, blocked) = match outcome {
        MeleeOutcome::Miss => (0, 0, 0),
        MeleeOutcome::Dodge | MeleeOutcome::Parry => (0, armored * 2, 0),
        MeleeOutcome::Block => {
            let blocked = armored.min(plan.input.defender.shield_block_value);
            (armored - blocked, armored + blocked, blocked)
        }
        MeleeOutcome::Glancing => {
            let total = match rolls.glance {
                Some(roll) => numeric::glancing_damage(armored, roll)?,
                None => armored,
            };
            (total, armored + (armored - total), 0)
        }
        MeleeOutcome::Critical => {
            let total = numeric::critical_damage(armored)?;
            (total, total, 0)
        }
        MeleeOutcome::Normal => (armored, armored, 0),
    };
    Ok(MeleeResolution {
        outcome,
        rolled_damage: rolls.damage,
        armored_damage: armored,
        total_damage: total,
        clean_damage: clean,
        blocked_amount: blocked,
    })
}
