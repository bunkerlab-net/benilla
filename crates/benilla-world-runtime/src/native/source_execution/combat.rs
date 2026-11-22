// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Unit.cpp:865-1016,1203-1220. Missing Unit/spell/lifecycle work stays refused.
use super::*;
use benilla_sim::creature_ai::{melee_decision, SwingConditions, SwingDecision};
use std::collections::BTreeSet;

mod live;
pub use live::SourceLiveDamage;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceDamageType {
    Direct,
    SpellDirect,
    Dot,
    SelfDamage,
    Shield,
    NoDamage,
    Split,
    Instakill,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceDamageAttack {
    MainHand,
    OffHand,
    Ranged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceDamageOutcome {
    Normal,
    Dodge,
    Parry,
    Critical,
}

/// Unit.h:632-641. The damage used for rage can differ from the landed amount.
#[derive(Clone, Copy, Debug)]
pub struct SourceCleanDamage {
    pub damage: u32,
    pub attack: SourceDamageAttack,
    pub outcome: SourceDamageOutcome,
    pub taken_or_absorbed: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct SourceDamage {
    pub dealer: Option<ObjectHandle>,
    pub victim: ObjectHandle,
    pub amount: u32,
    pub kind: SourceDamageType,
    pub school: u32,
    pub spell: Option<ThreatSpell>,
    pub clean_damage: Option<SourceCleanDamage>,
    pub travel_started_ms: Option<u32>,
    pub durability_loss: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct CombatUnit {
    pub health: u32,
    pub alive: bool,
    pub taxi: bool,
    pub evade: bool,
    pub has_ai: bool,
    pub unkillable: bool,
    pub preventing_death: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CombatCall {
    DamageDeal,
    DamageTaken,
    JustPreventedDeath,
    /// Stand state, rage, aura interrupts, then the zero-damage rage branch.
    DealDamagePrelude,
    /// Duel, loot-recipient assignment and damage-by-others, before death prevention.
    DealDamageNonzeroPrelude,
    /// Kill starts at beneficiary lookup, not SetHealth(0).
    Kill,
    /// Durability, rage, aura interruption and duel completion after threat/combat.
    DamageDealtSuffix,
}
pub trait CombatExecutionOwner: ThreatExecutionOwner {
    fn combat_unit(&mut self, _unit: ObjectHandle) -> Result<CombatUnit> {
        Err(refuse("execution.combat.f5_04b_unit_fields_owner"))
    }
    fn combat_health(&mut self, unit: ObjectHandle) -> Result<u32> {
        Ok(self.combat_unit(unit)?.health)
    }
    fn preventing_death(&mut self, unit: ObjectHandle) -> Result<bool> {
        Ok(self.combat_unit(unit)?.preventing_death)
    }
    fn can_apply_damage_mods(&mut self, unit: ObjectHandle) -> Result<bool> {
        let unit = self.combat_unit(unit)?;
        Ok(unit.alive && !unit.taxi && !unit.evade)
    }
    fn has_damage_ai(&mut self, unit: ObjectHandle) -> Result<bool> {
        Ok(self.combat_unit(unit)?.has_ai)
    }
    fn combat_call(&mut self, call: CombatCall, _damage: &mut SourceDamage) -> Result<()> {
        Err(refuse(match call {
            CombatCall::DamageDeal | CombatCall::DamageTaken | CombatCall::JustPreventedDeath => {
                "execution.combat.f5_04b_ai_callback_owner"
            }
            CombatCall::Kill => "execution.combat.f5_04b_kill_owner",
            CombatCall::DealDamagePrelude
            | CombatCall::DealDamageNonzeroPrelude
            | CombatCall::DamageDealtSuffix => "execution.combat.f5_04b_unit_damage_owner",
        }))
    }
    fn damage_taken(
        &mut self,
        _combat: &mut SourceCombat,
        damage: &mut SourceDamage,
    ) -> Result<()> {
        self.combat_call(CombatCall::DamageTaken, damage)
    }
    fn modify_health(&mut self, _unit: ObjectHandle, _delta: i32) -> Result<()> {
        Err(refuse("execution.combat.f5_04b_health_owner"))
    }
    fn damage_threat_multiplier(&mut self, _spell: Option<ThreatSpell>) -> Result<ThreatValue> {
        Err(refuse("execution.combat.f5_07_spell_threat_owner"))
    }
    fn set_in_combat(
        &mut self,
        _unit: ObjectHandle,
        _enemy: ObjectHandle,
        _threats: &mut SourceThreats,
    ) -> Result<()> {
        Err(refuse("execution.combat.f5_04b_combat_owner"))
    }
}
#[derive(Clone, Debug, Default)]
pub struct SourceCombat {
    death_prevented: BTreeSet<ObjectHandle>,
}
impl SourceCombat {
    pub const INVENTORY_VERSION: u32 = 1;
    pub fn death_prevented_units(&self) -> impl Iterator<Item = ObjectHandle> + '_ {
        self.death_prevented.iter().copied()
    }
    pub fn validate(&self, owner: &mut impl CombatExecutionOwner) -> Result<()> {
        for unit in &self.death_prevented {
            owner.combat_unit(*unit)?;
        }
        Ok(())
    }
    /// The same mainhand decision kernel as the bounded melee path. Damage is not recalculated.
    pub fn melee_decision(timer: u32, conditions: SwingConditions) -> SwingDecision {
        melee_decision(timer, conditions)
    }
    pub fn death_prevented(&self, unit: ObjectHandle) -> bool {
        self.death_prevented.contains(&unit)
    }
    pub fn reset_death_prevented(&mut self, unit: ObjectHandle) {
        self.death_prevented.remove(&unit);
    }
    /// Base CreatureAI::DamageTaken, CreatureAI.cpp:102-114. Does not reduce damage itself.
    pub fn creature_damage_taken(
        &mut self,
        damage: &mut SourceDamage,
        owner: &mut impl CombatExecutionOwner,
    ) -> Result<()> {
        let victim = owner.combat_unit(damage.victim)?;
        if victim.unkillable
            && damage.kind != SourceDamageType::Instakill
            && victim.health <= damage.amount
            && self.death_prevented.insert(damage.victim)
        {
            owner.combat_call(CombatCall::JustPreventedDeath, damage)?;
        }
        Ok(())
    }
    /// Unit::DealDamageMods preserves both mutable callback sides and absorbed accounting.
    pub fn damage_mods(
        &mut self,
        damage: &mut SourceDamage,
        absorb: Option<&mut u32>,
        owner: &mut impl CombatExecutionOwner,
    ) -> Result<()> {
        if !owner.can_apply_damage_mods(damage.victim)? {
            if let Some(absorb) = absorb {
                *absorb = absorb.wrapping_add(damage.amount);
            }
            damage.amount = 0;
            return Ok(());
        }
        let original = damage.amount;
        if let Some(dealer) = damage.dealer {
            if owner.has_damage_ai(dealer)? {
                owner.combat_call(CombatCall::DamageDeal, damage)?;
            }
        }
        if owner.has_damage_ai(damage.victim)? {
            owner.damage_taken(self, damage)?;
        }
        if let Some(absorb) = absorb {
            if original > damage.amount {
                *absorb = absorb.wrapping_add(original - damage.amount);
            }
        }
        Ok(())
    }
    pub fn deal_damage(
        &mut self,
        damage: &mut SourceDamage,
        threats: &mut SourceThreats,
        owner: &mut impl CombatExecutionOwner,
    ) -> Result<u32> {
        owner.combat_call(CombatCall::DealDamagePrelude, damage)?;
        if damage.amount == 0 {
            return Ok(0);
        }
        // health is captured before duel/loot work, Unit.cpp:969-1007.
        let health = owner.combat_health(damage.victim)?;
        owner.combat_call(CombatCall::DealDamageNonzeroPrelude, damage)?;
        if owner.preventing_death(damage.victim)?
            && damage.kind != SourceDamageType::Instakill
            && health <= damage.amount
        {
            damage.amount = health.wrapping_sub(1);
        }
        if health <= damage.amount {
            owner.combat_call(CombatCall::Kill, damage)?;
        } else {
            self.handle_damage_dealt(damage, threats, owner)?;
        }
        Ok(damage.amount)
    }
    /// Entry is HandleDamageDealt, not a replacement for its caller's preceding work.
    pub fn handle_damage_dealt(
        &mut self,
        damage: &mut SourceDamage,
        threats: &mut SourceThreats,
        owner: &mut impl CombatExecutionOwner,
    ) -> Result<()> {
        // -(int32)damage is undefined at INT32_MIN, Unit.cpp:1207.
        let delta = (damage.amount as i32)
            .checked_neg()
            .ok_or_else(|| refuse("execution.combat.negate_int_min_damage_undefined"))?;
        owner.modify_health(damage.victim, delta)?;
        if let Some(dealer) = damage.dealer {
            if owner.can_attack(damage.victim, dealer)?
                && !damage.spell.is_some_and(|s| s.no_threat)
                && owner.can_enter_combat(dealer)?
                && owner.can_enter_combat(damage.victim)?
            {
                let value = ThreatValue::from_u32(damage.amount)
                    .multiply(owner.damage_threat_multiplier(damage.spell)?);
                threats.unit_add(
                    damage.victim,
                    Some(dealer),
                    value,
                    damage
                        .clean_damage
                        .is_some_and(|clean| clean.outcome == SourceDamageOutcome::Critical),
                    damage.school,
                    damage.spell,
                    owner,
                )?;
                if !matches!(
                    damage.kind,
                    SourceDamageType::Dot | SourceDamageType::Shield
                ) {
                    owner.set_in_combat(dealer, damage.victim, threats)?;
                    owner.set_in_combat(damage.victim, dealer, threats)?;
                    owner.combat_timer(dealer, damage.victim)?;
                }
            }
        }
        owner.combat_call(CombatCall::DamageDealtSuffix, damage)
    }
    /// Unit::Suicide calls DealDamage directly. It does not call DealDamageMods.
    pub fn suicide(
        &mut self,
        unit: ObjectHandle,
        threats: &mut SourceThreats,
        owner: &mut impl CombatExecutionOwner,
    ) -> Result<u32> {
        let amount = owner.combat_health(unit)?;
        self.deal_damage(
            &mut SourceDamage {
                dealer: Some(unit),
                victim: unit,
                amount,
                kind: SourceDamageType::Instakill,
                school: 1,
                spell: None,
                clean_damage: None,
                travel_started_ms: None,
                durability_loss: false,
            },
            threats,
            owner,
        )
    }
    /// Linking action adapter. All remaining AI/motion operations keep their own refusal.
    pub fn linking_action(
        &mut self,
        threats: &mut SourceThreats,
        unit: ObjectHandle,
        action: LinkingAction,
        owner: &mut impl CombatExecutionOwner,
    ) -> Result<()> {
        match action {
            LinkingAction::Suicide => {
                self.suicide(unit, threats, owner)?;
                Ok(())
            }
            LinkingAction::AddThreat(target) => {
                threats.unit_add(unit, Some(target), ThreatValue::ZERO, false, 0, None, owner)
            }
            LinkingAction::SetInCombatWith(target) => owner.set_in_combat(unit, target, threats),
            LinkingAction::TriggerCombatTimer(target) => owner.combat_timer(unit, target),
            _ => Err(action.missing_owner()),
        }
    }
}
