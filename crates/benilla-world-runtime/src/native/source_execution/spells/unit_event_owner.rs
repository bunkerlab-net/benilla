// SPDX-License-Identifier: GPL-3.0-or-later
//! Spell.cpp:7127-7244. A shared Unit queue dispatches spell and ordinary events.
use super::*;
use benilla_sim::spell_event::SpellEventPlan;

pub struct SpellUnitEventOwner<'a, O> {
    pub starts: &'a mut SourceSpellStarts,
    pub stores: &'a SourceSpellStores,
    pub spell_owner: &'a mut dyn SpellExecutionOwner,
    pub ordinary: &'a mut O,
}
impl<O: UnitEventOwner> UnitEventOwner for SpellUnitEventOwner<'_, O> {
    fn forced_despawn(&mut self, only_alive: bool) -> Result<()> {
        self.ordinary.forced_despawn(only_alive)
    }
    fn assistance(
        &mut self,
        capture: &AssistanceCapture,
        queue: &mut impl AssistanceQueue,
    ) -> Result<()> {
        self.ordinary.assistance(capture, queue)
    }
    fn spell_event(&mut self, id: u64, now: u64, diff: u32) -> Result<SpellEventPlan> {
        let s = self
            .starts
            .spells
            .get_mut(&id)
            .ok_or_else(|| refuse("execution.spells.event_identity"))?;
        let t = self
            .stores
            .template(s.spell)
            .ok_or_else(|| refuse("execution.spells.missing_template"))?;
        s.execute_event(t, now, diff, self.spell_owner)
    }
    fn abort_spell(&mut self, id: u64) -> Result<bool> {
        let s = self
            .starts
            .spells
            .get_mut(&id)
            .ok_or_else(|| refuse("execution.spells.event_identity"))?;
        let t = self
            .stores
            .template(s.spell)
            .ok_or_else(|| refuse("execution.spells.missing_template"))?;
        s.abort_event(t, self.spell_owner)
    }
    fn delete_spell(&mut self, id: u64) -> Result<()> {
        let s = self
            .starts
            .spells
            .get_mut(&id)
            .ok_or_else(|| refuse("execution.spells.event_identity"))?;
        if s.state != SourceSpellState::Finished {
            let t = self
                .stores
                .template(s.spell)
                .ok_or_else(|| refuse("execution.spells.missing_template"))?;
            s.cancel(t, self.spell_owner)?;
        }
        s.event = None;
        // :7131-7138 deliberately leaks a still-referenced Spell, not its event.
        if s.deletable() {
            self.starts.spells.remove(&id);
        }
        Ok(())
    }
}
