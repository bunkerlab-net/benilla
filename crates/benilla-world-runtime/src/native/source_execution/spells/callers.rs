// SPDX-License-Identifier: GPL-3.0-or-later
//! Unit.cpp:1469-1512,1595-1631; UnitAI.cpp:143-224.
use super::*;

#[derive(Clone, Debug, Default)]
pub struct SourceSpellStarts {
    next_id: u64,
    pub spells: BTreeMap<u64, SourceSpell>,
}
impl SourceSpellStarts {
    pub(super) fn allocate(&mut self) -> Result<u64> {
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| refuse("execution.spells.identity_exhausted"))?;
        Ok(self.next_id)
    }
}
pub struct SpellStartContext<'a> {
    pub stores: &'a SourceSpellStores,
    pub starts: &'a mut SourceSpellStarts,
    pub events: &'a mut SourceUnitEvents,
    pub owner: &'a mut dyn SpellExecutionOwner,
}
impl SpellStartContext<'_> {
    pub fn contains(&self, spell: u32) -> bool {
        self.stores.template(spell).is_some()
    }
    pub fn cast(
        &mut self,
        caster: ObjectHandle,
        target: Option<ObjectHandle>,
        spell: u32,
        flags: SpellTriggerFlags,
        xyz: bool,
    ) -> Result<SpellCastResult> {
        let Some(t) = self.stores.template(spell) else {
            return Ok(SpellCastResult::NotFound);
        };
        // UnitAI.cpp:221 evaluates these arguments before Unit::CastSpell.
        let xyz_pose = if xyz {
            let target = target.ok_or_else(|| {
                refuse("execution.spells.UnitAI_CAST_ONLY_XYZ_null_target_undefined")
            })?;
            Some(self.owner.unit(target)?.position)
        } else {
            None
        };
        let id = self
            .starts
            .next_id
            .checked_add(1)
            .ok_or_else(|| refuse("execution.spells.identity_exhausted"))?;
        self.starts.next_id = id;
        let mut value =
            SourceSpell::construct(id, self.stores, spell, caster, None, flags, self.owner)?;
        let mut targets = SpellTargets::default();
        if let Some(p) = xyz_pose {
            if t.targets & 0x20 != 0 {
                targets.source = Some([p[0], p[1], p[2]]);
                targets.mask |= 0x20;
            }
            if t.targets & 0x40 != 0 || t.targets & 0x60 == 0 {
                targets.destination = Some([p[0], p[1], p[2]]);
                targets.mask |= 0x40;
            }
        } else {
            if let Some(target) = target {
                let p = self.owner.unit(target)?.position;
                targets.unit = Some(target);
                targets.destination = Some([p[0], p[1], p[2]]);
                targets.mask |= 2;
            }
            if t.targets & 0x40 != 0 {
                if target.is_none() {
                    self.starts.spells.insert(id, value);
                    return Ok(SpellCastResult::BadTargets);
                }
                targets.mask |= 0x40;
            }
            if t.targets & 0x20 != 0 {
                targets.source = self.owner.casting_source(&value)?;
                if targets.source.is_some() {
                    targets.mask |= 0x20;
                }
            }
        }
        // Refusals retain the real event and partially executed object; never retry it.
        let result = value.start(t, targets, None, self.events, self.owner);
        self.starts.spells.insert(id, value);
        result
    }
    pub fn ai_cast(
        &mut self,
        original: ObjectHandle,
        mut target: Option<ObjectHandle>,
        spell: u32,
        cast_flags: u32,
    ) -> Result<bool> {
        let mut caster = original;
        if let Some(mut unit) = target {
            if cast_flags & 0x80 != 0 {
                std::mem::swap(&mut caster, &mut unit);
                target = Some(unit);
            }
            if cast_flags & 0x10 != 0 {
                caster = unit;
            }
        } else if cast_flags & 0x90 != 0 {
            return Ok(false);
        }
        let (evade, casting) = self.owner.ai_cast_state(original, caster)?;
        if evade && cast_flags & 2 == 0 {
            return Ok(false);
        }
        if casting && cast_flags & 3 == 0 {
            return Ok(false);
        }
        let Some(t) = self.stores.template(spell) else {
            return Ok(false);
        };
        if cast_flags & 0x20 != 0 && self.owner.has_aura(target.unwrap_or(caster), spell)? {
            return Ok(false);
        }
        if cast_flags & 0x14 == 0 && cast_flags & 2 == 0 {
            if let Some(target) = target {
                if t.targetcreaturetype != 0
                    && !matches!(t.id, 2641 | 23356)
                    && !self.owner.ai_target_creature_type(target, t)?
                {
                    return Ok(false);
                }
            }
            if is_melee(t) && self.owner.active_auto_spell(caster)? == Some(spell) {
                return Ok(false);
            }
        }
        if cast_flags & 1 != 0 && casting {
            self.owner.interrupt_for_ai(caster, false)?;
        }
        self.owner.interrupt_for_ai(caster, true)?;
        let mut flags = (if cast_flags & 2 != 0 { 1 } else { 0 })
            | (if cast_flags & 0x40 != 0 { 4 } else { 0 });
        if flags == 0 {
            flags = 0x100;
        }
        Ok(self.cast(
            caster,
            target,
            spell,
            SpellTriggerFlags(flags),
            cast_flags & 0x1000 != 0,
        )? == SpellCastResult::Ok)
    }
}
