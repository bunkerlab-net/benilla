// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Spell.cpp prepare/cast/TakePower/SendSpellCooldown.
// Copyright CMaNGOS contributors; upstream GPL-2.0-or-later.
use super::*;

/// Effective instant-aura cost and cooldown operands for the existing caster owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InstantAuraResources {
    pub spell_id: u32,
    pub mana_cost: u32,
    pub global_category: u32,
    pub global_recovery_ms: u32,
    pub recovery_ms: u32,
    pub category: u32,
    pub category_recovery_ms: u32,
}
impl From<&MageProjectileSpec> for InstantAuraResources {
    fn from(spec: &MageProjectileSpec) -> Self {
        Self {
            spell_id: spec.spell_id,
            mana_cost: spec.mana_cost,
            global_category: spec.global_category,
            global_recovery_ms: spec.global_recovery_ms,
            recovery_ms: spec.recovery_ms,
            category: spec.category,
            category_recovery_ms: spec.category_recovery_ms,
        }
    }
}
impl CasterResources {
    /// The host resolves CheckCast, target and hooks before this mutation.
    /// Ordinary refusals leave resources unchanged.
    pub fn cast_instant_aura(
        &mut self,
        spec: InstantAuraResources,
        now_ms: u64,
        checks: CastChecks,
    ) -> Result<Result<(), CastFailure>, SpellError> {
        if spec.spell_id == 0
            || spec.mana_cost > MAX_AMOUNT
            || [
                spec.global_recovery_ms,
                spec.recovery_ms,
                spec.category_recovery_ms,
            ]
            .into_iter()
            .any(|ms| ms > MAX_TIME)
            || self.mana > MAX_AMOUNT
        {
            return Err(SpellError::Domain("instant aura resources"));
        }
        let failure = if !self.ready_at(
            spec.spell_id,
            spec.category,
            spec.global_category,
            now_ms,
            true,
        ) {
            Some(CastFailure::NotReady)
        } else if !checks.caster_alive {
            Some(CastFailure::CasterDead)
        } else if let Some(reason) = checks.other_failure {
            Some(CastFailure::Rejected(reason))
        } else if self.mana < spec.mana_cost {
            Some(CastFailure::NoPower)
        } else if !checks.target_exists {
            Some(CastFailure::TargetLost)
        } else if !checks.target_alive {
            Some(CastFailure::TargetDead)
        } else {
            None
        };
        if let Some(failure) = failure {
            return Ok(Err(failure));
        }
        now_ms
            .checked_add(u64::from(
                spec.global_recovery_ms
                    .max(spec.recovery_ms)
                    .max(spec.category_recovery_ms),
            ))
            .ok_or(SpellError::Domain("instant aura cooldown overflow"))?;
        self.start_global(spec.global_category, spec.global_recovery_ms, now_ms)?;
        self.start_recovery(
            spec.spell_id,
            spec.recovery_ms,
            spec.category,
            spec.category_recovery_ms,
            now_ms,
        )?;
        self.mana -= spec.mana_cost;
        if spec.mana_cost != 0 {
            self.last_mana_use_ms = Some(now_ms);
        }
        Ok(Ok(()))
    }
}
