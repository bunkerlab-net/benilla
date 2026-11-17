// SPDX-License-Identifier: GPL-3.0-or-later
//! Unit.cpp:4343-4381, pinned CMaNGOS Classic 8ec338a1.
use super::*;

/// Live Spell identities must resolve independently of a comparison record.
pub trait SourceCurrentSpells {
    fn current_spell(&self, _id: u64) -> Result<&SourceSpell> {
        Err(refuse(
            "execution.spells.IsNonMeleeSpellCasted_live_spell_owner",
        ))
    }
}
impl SourceCurrentSpells for () {}

#[cfg(test)]
impl SourceCurrentSpells for BTreeMap<u64, SourceSpell> {
    fn current_spell(&self, id: u64) -> Result<&SourceSpell> {
        self.get(&id)
            .ok_or_else(|| refuse("execution.spells.IsNonMeleeSpellCasted_current_spell_owner"))
    }
}

impl SourceSpellSlots {
    /// Reads the existing current-spell identities in source short-circuit order.
    #[allow(clippy::too_many_arguments)] // Preserve Unit.cpp:4343's five query operands.
    pub fn is_non_melee_spell_casted(
        &self,
        spells: &impl SourceCurrentSpells,
        stores: &SourceSpellStores,
        with_delayed: bool,
        skip_channeled: bool,
        skip_autorepeat: bool,
        for_movement: bool,
        for_auto_ignore: bool,
    ) -> Result<bool> {
        let spell = |id| spells.current_spell(id);
        let template = |s: &SourceSpell| {
            stores
                .template(s.spell)
                .ok_or_else(|| refuse("execution.spells.IsNonMeleeSpellCasted_spell_info_owner"))
        };
        if let Some(id) = self.current(SpellSlot::Generic) {
            let s = spell(id)?;
            if s.state != SourceSpellState::Finished {
                let special = !for_movement || template(s)?.interruptflags & 1 != 0;
                let auto = for_auto_ignore && template(s)?.attributesex2 & 0x20000 != 0;
                if !auto && special && (with_delayed || s.state != SourceSpellState::Traveling) {
                    return Ok(true);
                }
            }
        }
        if !skip_channeled {
            if let Some(id) = self.current(SpellSlot::Channel) {
                let s = spell(id)?;
                let attribute = !for_movement && template(s)?.attributesex4 & 0x80 != 0;
                let auto = for_auto_ignore && template(s)?.attributesex2 & 0x20000 != 0;
                if !auto
                    && !attribute
                    && !s.flags.old_triggered()
                    && s.state != SourceSpellState::Finished
                {
                    return Ok(true);
                }
            }
        }
        // :4378 checks the pointer only, even for a finished autorepeat spell.
        if !skip_autorepeat && self.current(SpellSlot::AutoRepeat).is_some() {
            return Ok(true);
        }
        Ok(for_auto_ignore)
    }
}
