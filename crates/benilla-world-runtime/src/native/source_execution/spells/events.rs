// SPDX-License-Identifier: GPL-3.0-or-later
//! Spell.cpp:3519-3570,7149-7244. Reuse the shared scheduling kernels.
use super::*;
use benilla_sim::spell_event::{
    plan_after_delayed, plan_after_update, SpellEventPlan, SpellEventState,
};

impl SourceSpell {
    pub fn update(
        &mut self,
        t: &SpellTemplate,
        diff: u32,
        owner: &mut (impl SpellExecutionOwner + ?Sized),
    ) -> Result<()> {
        if !self.updated {
            self.updated = true;
            if self.creation_time == owner.map_time(self.caster)? {
                return Ok(());
            }
        }
        if !owner.update_pointers(self)? {
            self.cancel(t, owner)?;
            return Ok(());
        }
        let c = owner.unit(self.caster)?;
        if !c.gameobject
            && self.timer != 0
            && (0..3)
                .any(|i| f32::from_bits(c.position[i]) != f32::from_bits(self.cast_position[i]))
            && (t.effect1 != 84 || !c.falling_far)
            && (self.state == SourceSpellState::Channeling && t.channelinterruptflags & 8 != 0
                || !is_melee(t)
                    && !self.auto_repeat
                    && !self.flags.old_triggered()
                    && t.interruptflags & 1 != 0)
        {
            self.cancel(t, owner)?;
        }
        match self.state {
            SourceSpellState::Casting => {
                if self.timer != 0 {
                    if let Some(target) = self.targets.unit {
                        // SpellMgr.h:260-263 is a pure attribute/mask predicate.
                        let allow_dead = t.attributesex2 & 1 != 0
                            || t.attributes & 0x40 != 0
                            || t.targets & 0x8600 != 0;
                        if !self.flags.old_triggered() && !allow_dead && !owner.unit(target)?.alive
                        {
                            self.cancel(t, owner)?;
                            return Ok(());
                        }
                    }
                    self.timer = self.timer.saturating_sub(diff);
                }
                if self.timer == 0 && !is_melee(t) && !self.auto_repeat {
                    owner.call(self, SpellCall::Cast(false))?;
                }
            }
            SourceSpellState::Channeling => owner.call(self, SpellCall::ChannelUpdate)?,
            _ => {}
        }
        Ok(())
    }
    pub fn execute_event(
        &mut self,
        t: &SpellTemplate,
        now: u64,
        diff: u32,
        owner: &mut (impl SpellExecutionOwner + ?Sized),
    ) -> Result<SpellEventPlan> {
        if self.state != SourceSpellState::Finished {
            self.update(t, diff, owner)?;
        }
        let state = match self.state {
            SourceSpellState::Finished => SpellEventState::Finished {
                deletable: self.deletable(),
            },
            SourceSpellState::Traveling if is_channel(t) && self.delay_start != 0 => {
                if owner.other_non_melee_cast(self)? {
                    self.cancel(t, owner)?;
                } else {
                    owner.call(self, SpellCall::HandleImmediate)?;
                }
                SpellEventState::Casting
            }
            SourceSpellState::Traveling => SpellEventState::Traveling {
                delay_start_ms: Some(self.delay_start),
                delay_moment_ms: self.delay_moment,
            },
            _ => SpellEventState::Casting,
        };
        let plan = plan_after_update(now, state)
            .map_err(|e| refuse(&format!("execution.spells.event.{e:?}")))?;
        match plan {
            SpellEventPlan::HandleDelayed { elapsed_ms } => {
                let next = owner.handle_delayed(self, elapsed_ms)?;
                plan_after_delayed(now, self.delay_start, next)
                    .map_err(|e| refuse(&format!("execution.spells.event.{e:?}")))
            }
            SpellEventPlan::ReAdd {
                set_delay_start_ms: Some(start),
                ..
            } => {
                self.delay_start = start;
                Ok(plan)
            }
            _ => Ok(plan),
        }
    }
    pub fn abort_event(
        &mut self,
        t: &SpellTemplate,
        owner: &mut (impl SpellExecutionOwner + ?Sized),
    ) -> Result<bool> {
        if self.state != SourceSpellState::Finished {
            self.cancel(t, owner)?;
        }
        Ok(self.deletable())
    }
}

/// Ordinary explicit-unit arithmetic is the existing bit-exact kernel, not F6 geometry.
pub fn spell_unit_range(
    origin: [u32; 3],
    target: [u32; 3],
    minimum: u32,
    maximum: u32,
    caster_reach: u32,
    target_reach: u32,
    strict: bool,
) -> Result<benilla_sim::spell_range::Outcome> {
    benilla_sim::spell_range::check_bits(
        origin,
        target,
        minimum,
        maximum,
        caster_reach,
        target_reach,
        strict,
    )
    .map_err(|e| refuse(&format!("execution.spells.range.{e:?}")))
}
