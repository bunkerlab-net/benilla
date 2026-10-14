// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
// Original GPL-2.0-or-later grant; Rust translation GPL-3.0-or-later.
//! Source-derived ordinary physical NPC mainhand -> unmodified player mage.
//! This is a validated arithmetic/health projection, not complete DealDamage.
//! Native admission and required callbacks remain separate: ../../NPC-MELEE.md.
//! No host floating-point arithmetic or hidden/random-default resolved stats.
mod numeric;
mod skill;
mod skill_log;
use crate::classic::software_armor_reduced_damage;
use crate::classic_spell::SpellFloat;
pub use numeric::{
    creature_damage_range, mage_base_dodge, player_avoidance_chance, CreatureDamageInputs,
    CreatureStatClass, DamageRange,
};
pub use skill::{defense_gain_chance, resolve_defense_gain, DefenseSkill, PercentRoll};

pub const NPC_MELEE_POLICY_ID: &str = "cmangos-8ec338a1-npc-mage-melee-softfloat-v2";
/// Unit.h REACTIVE_TIMER_START, used by the mage dodge callback.
pub const DEFENSE_REACTIVE_DURATION_MS: u32 = 4_000;
const MAX_AMOUNT: u32 = 1_000_000;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeleeError {
    Domain(&'static str),
    Unsupported(&'static str),
    Roll(&'static str),
    State(&'static str),
}
impl std::fmt::Display for MeleeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unsupported/invalid NPC melee: {self:?}")
    }
}
impl std::error::Error for MeleeError {}

/// Audited ordinary, uncontrolled, non-totem, living NPC with one physical
/// mainhand damage line and no modifiers, immunities, scripts or damage procs.
/// Values are UNIT_FIELD_MINDAMAGE/MAXDAMAGE, not raw creature_template values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreatureAttack {
    /// GetLevelForTarget(player); ordinary creatures use their selected level.
    pub level: u8,
    pub minimum_damage: SpellFloat,
    pub maximum_damage: SpellFloat,
    pub no_crushing_blows: bool,
    pub creature_is_critter: bool,
}
/// Audited unmodified living, player-controlled mage, on the ground, no evade,
/// taxi, crowd control, duel, death prevention, absorb, immunity or damage shield.
/// CanDodge=true; CanParry=false/CanBlock=false from constructor and no enabling
/// spell effect. Other classes or enabling effects must not use this type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MageDefense {
    pub level: u8,
    pub agility: u32,
    /// Live GetSkillValue(SKILL_DEFENSE), including bonuses; not pure/max skill.
    pub defense_skill: u16,
    pub armor: i32,
    pub standing: bool,
    /// Source IsNonMeleeSpellCasted(false), not merely an active traveling spell.
    pub casting_non_melee: bool,
    /// Presence of CURRENT_CHANNELED_SPELL, including triggered/finished slots.
    /// This cannot be inferred from IsNonMeleeSpellCasted(false).
    pub has_channeled_spell: bool,
    /// Presence of CURRENT_AUTOREPEAT_SPELL. Its response is outside this kernel.
    pub has_auto_repeat_spell: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeleeInput {
    pub attacker: CreatureAttack,
    pub defender: MageDefense,
    pub health: u32,
    /// Object::IsFacingTargetsFront requires BOTH units' PI arcs. False is
    /// unavailable until the source's rear daze spell/aura callback is ported.
    pub front_facing: bool,
}
/// Percentages converted with source chance_u: binary32 *100, roundf ties away.
/// The die consumes ordered cumulative chances, not independent hit checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeleeChances {
    pub miss: u16,
    pub dodge: u16,
    pub parry: u16,
    pub block: u16,
    pub critical: u16,
    pub crushing: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeleeOutcome {
    Miss,
    Dodge,
    Critical,
    Crushing,
    Normal,
}
/// Explicit semantic draws in source order, including damage on a miss/dodge.
/// Always draw the inclusive damage range even when minimum == maximum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeleeRolls {
    pub damage: u32,
    /// Inclusive urand(1,10000), compared <= cumulative chance.
    pub outcome: u16,
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
    pub fn input(self) -> MeleeInput {
        self.input
    }
}
/// Required downstream source calls, deliberately not executed by this module.
/// Consume these inside the same runtime transaction, or reject the attack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeleeCallbacks {
    pub stand_up: bool,
    pub critical_wound_emote: bool,
    /// DealMeleeDamage's AttackedBy/EngageInCombatWithAggressor runs even on a
    /// miss, conditioned again on each unit's CanEnterCombat AFTER health/death.
    pub attacked_by_and_combat: bool,
    /// HandleDamageDealt on positive surviving hit. Player AddThreat is a no-op
    /// (CanHaveThreatList=false); combat flags and combat timer still need calls.
    pub surviving_damage_combat: bool,
    /// HandleDamageDealt consumes rand_chance even for configured rate=0; if it
    /// succeeds, consumes urand(0,EQUIPMENT_SLOT_END-1) and reduces durability.
    pub durability_hit_attempt: bool,
    /// Source call after hit durability on every positive nonlethal hit. The
    /// adapter must inspect the current generic slot; absent/traveling spells
    /// consume no pushback draw. Channeled/auto-repeat slots are rejected.
    pub interrupt_or_delay_spell: bool,
    /// ProcSkillsAndReactives AFTER DealMeleeDamage; includes misses and dodges,
    /// excludes dead victims and creature-type critter attackers.
    pub defense_skill_attempt: bool,
    /// Mage dodge sets AURA_STATE_DEFENSE and starts REACTIVE_DEFENSE timer.
    pub defense_reactive: bool,
    /// Full Kill path: death state/combat stop/corpse/AI callbacks and durability.
    pub killed: bool,
}
/// Immutable, single-use health projection. Read-only getters expose results;
/// callers cannot forge damage or remove required callbacks before applying.
///
/// ```compile_fail
/// use benilla_sim::npc_melee::MeleeResolution;
/// fn tamper(mut result: MeleeResolution) {
///     result.health_after = 0;
/// }
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct MeleeResolution {
    outcome: MeleeOutcome,
    rolled_damage: u32,
    armored_damage: u32,
    total_damage: u32,
    clean_damage: u32,
    health_before: u32,
    health_after: u32,
    effective_damage: u32,
    overkill: u32,
    callbacks: MeleeCallbacks,
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
    pub fn health_before(&self) -> u32 {
        self.health_before
    }
    pub fn health_after(&self) -> u32 {
        self.health_after
    }
    pub fn effective_damage(&self) -> u32 {
        self.effective_damage
    }
    pub fn overkill(&self) -> u32 {
        self.overkill
    }
    pub fn callbacks(&self) -> MeleeCallbacks {
        self.callbacks
    }
}
/// Validate before requesting/consuming RNG. Never infer any missing live field.
pub fn prepare(input: MeleeInput) -> Result<MeleePlan, MeleeError> {
    let a = input.attacker;
    let d = input.defender;
    if !(1..=63).contains(&a.level) || !(1..=60).contains(&d.level) {
        return Err(MeleeError::Domain("level"));
    }
    if input.health == 0 || input.health > MAX_AMOUNT {
        return Err(MeleeError::State("living health"));
    }
    if d.agility > MAX_AMOUNT || d.armor.unsigned_abs() > MAX_AMOUNT {
        return Err(MeleeError::Domain("effective player stats"));
    }
    if d.has_channeled_spell || d.has_auto_repeat_spell {
        return Err(MeleeError::Unsupported(
            "channeled/auto-repeat current spell slot",
        ));
    }
    if !input.front_facing {
        return Err(MeleeError::Unsupported(
            "rear/non-facing melee and daze callback",
        ));
    }
    let range = numeric::roll_range(a.minimum_damage, a.maximum_damage)?;
    let chances = numeric::chances(a, d)?;
    Ok(MeleePlan {
        input,
        range,
        chances,
    })
}
/// CalculateMeleeDamage plus the bounded health projection from DealDamage.
/// The source's armor minimum is applied BEFORE the die, including roll=0.
/// This function never mutates HP, timers, skills, RNG or callback state.
pub fn resolve(plan: MeleePlan, rolls: MeleeRolls) -> Result<MeleeResolution, MeleeError> {
    if !(plan.range.0..=plan.range.1).contains(&rolls.damage) {
        return Err(MeleeError::Roll("inclusive damage range"));
    }
    if !(1..=10000).contains(&rolls.outcome) {
        return Err(MeleeError::Roll("inclusive outcome die"));
    }
    let input = plan.input;
    let armored =
        software_armor_reduced_damage(input.attacker.level, input.defender.armor, 0, rolls.damage)
            .map_err(|_| MeleeError::Domain("armor"))?;
    let c = plan.chances;
    let mut cumulative = u32::from(c.miss);
    let roll = u32::from(rolls.outcome);
    let outcome = if roll <= cumulative {
        MeleeOutcome::Miss
    } else {
        cumulative += u32::from(c.dodge);
        if roll <= cumulative {
            MeleeOutcome::Dodge
        } else {
            cumulative += u32::from(c.critical);
            if roll <= cumulative {
                MeleeOutcome::Critical
            } else {
                cumulative += u32::from(c.crushing);
                if roll <= cumulative {
                    MeleeOutcome::Crushing
                } else {
                    MeleeOutcome::Normal
                }
            }
        }
    };
    let (total, clean) = match outcome {
        MeleeOutcome::Miss => (0, 0),
        MeleeOutcome::Dodge => (0, armored * 2),
        MeleeOutcome::Critical => {
            let n = numeric::critical_damage(armored)?;
            (n, n)
        }
        MeleeOutcome::Crushing => (armored + armored / 2, armored),
        MeleeOutcome::Normal => (armored, armored),
    };
    let effective = total.min(input.health);
    let after = input.health - effective;
    let surviving_damage = total != 0 && after != 0;
    Ok(MeleeResolution {
        outcome,
        rolled_damage: rolls.damage,
        armored_damage: armored,
        total_damage: total,
        clean_damage: clean,
        health_before: input.health,
        health_after: after,
        effective_damage: effective,
        overkill: total - effective,
        callbacks: MeleeCallbacks {
            stand_up: !input.defender.standing,
            critical_wound_emote: outcome == MeleeOutcome::Critical,
            attacked_by_and_combat: true,
            surviving_damage_combat: surviving_damage,
            durability_hit_attempt: surviving_damage,
            interrupt_or_delay_spell: surviving_damage,
            defense_skill_attempt: after != 0 && !input.attacker.creature_is_critter,
            defense_reactive: after != 0 && outcome == MeleeOutcome::Dodge,
            killed: after == 0,
        },
    })
}
/// Health-only convenience, explicitly NOT full DealMeleeDamage. Revalidates
/// expected live health before applying. Callers must stage required callbacks
/// transactionally before committing this projection; errors do not alter HP.
/// This consumes the projection so an unchanged-health miss cannot accidentally
/// be applied twice. Re-resolving a plan is still pure and allowed: the runtime
/// must reject repeated action IDs and bind owner/entity/tick/world revision.
/// Comparing scalar health cannot detect stale same-health world/callback state.
///
/// ```compile_fail
/// use benilla_sim::npc_melee::{apply_health, MeleeResolution};
/// fn replay(health: &mut u32, result: MeleeResolution) {
///     let _ = apply_health(health, result);
///     let _ = apply_health(health, result);
/// }
/// ```
pub fn apply_health(health: &mut u32, result: MeleeResolution) -> Result<(), MeleeError> {
    if *health != result.health_before
        || result.health_after > result.health_before
        || result.effective_damage != result.health_before - result.health_after
        || result.effective_damage != result.total_damage.min(result.health_before)
        || result.overkill != result.total_damage - result.effective_damage
    {
        return Err(MeleeError::State("stale or inconsistent health projection"));
    }
    *health = result.health_after;
    Ok(())
}
