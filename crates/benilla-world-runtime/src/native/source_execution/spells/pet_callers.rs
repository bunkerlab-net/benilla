// SPDX-License-Identifier: GPL-3.0-or-later
//! PetAI.cpp:564-587. The actual Spell exists before facing and pet learning.
use super::*;

impl SpellStartContext<'_> {
    pub fn pet_cast(
        &mut self,
        caster: ObjectHandle,
        spell: u32,
        target: Option<ObjectHandle>,
        controlled_opener: bool,
        pet: bool,
    ) -> Result<bool> {
        let flags = SpellTriggerFlags(0x100 | if controlled_opener { 0x80 } else { 0 });
        let id = self.starts.allocate()?;
        let mut value =
            SourceSpell::construct(id, self.stores, spell, caster, None, flags, self.owner)?;
        let result = (|| {
            let mut targets = SpellTargets::default();
            if let Some(target) = target {
                // Spell.cpp:138-146, setUnitTarget copies the target position.
                let p = self.owner.unit(target)?.position;
                targets.unit = Some(target);
                targets.destination = Some([p[0], p[1], p[2]]);
                targets.mask |= 2;
                if !self.owner.pet_has_in_arc(caster, target)? {
                    self.owner.pet_face_target(caster, target)?;
                }
            }
            if pet {
                self.owner.pet_check_learning(caster, spell)?;
            }
            let template = self
                .stores
                .template(spell)
                .ok_or_else(|| refuse("execution.spells.missing_template"))?;
            value
                .start(template, targets, None, self.events, self.owner)
                .map(|result| result == SpellCastResult::Ok)
        })();
        // A reached missing dependency must not discard the constructed Spell.
        self.starts.spells.insert(id, value);
        result
    }
}
