// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Unit.cpp:1469-1512,1595-1631; UnitAI.cpp:143-224.
use super::*;

#[derive(Clone, Debug, Default)]
pub struct SourceSpellStarts {
    next_id: u64,
    pub spells: BTreeMap<u64, SourceSpell>,
}
impl SourceSpellStarts {
    pub(in crate::native::source_execution) fn allocate(&mut self) -> Result<u64> {
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
        self.cast_with_aura(caster, target, spell, flags, xyz, None)
    }

    /// Unit.cpp:1453-1512. Start a triggered spell on behalf of this caster's aura.
    pub fn cast_triggered(
        &mut self,
        caster: ObjectHandle,
        target: Option<ObjectHandle>,
        spell: u32,
        aura: Option<u32>,
    ) -> Result<SpellCastResult> {
        self.cast_with_aura(caster, target, spell, SpellTriggerFlags(1), false, aura)
    }

    fn cast_with_aura(
        &mut self,
        caster: ObjectHandle,
        target: Option<ObjectHandle>,
        spell: u32,
        flags: SpellTriggerFlags,
        xyz: bool,
        aura: Option<u32>,
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
        let id = self.starts.allocate()?;
        let mut value =
            SourceSpell::construct(id, self.stores, spell, caster, None, flags, self.owner)?;
        value.triggered_by_spell_reagent = aura
            .and_then(|id| self.stores.template(id))
            .map(|template| template.reagent1);
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
        } else if let Some(unit_targets) = value.unit_cast_targets(t, target, self.owner)? {
            targets = unit_targets;
        } else {
            self.starts.spells.insert(id, value);
            return Ok(SpellCastResult::BadTargets);
        }
        // Refusals retain the real event and partially executed object; never retry it.
        let result = (|| {
            value.schedule_start(targets, aura, self.events)?;
            value
                .start_scheduled(t, &mut self.starts.spells, self.owner)?
                .dispatch(|skip| {
                    self.owner
                        .cast_scheduled(&mut value, skip, self.starts, self.events)
                })
        })();
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
        let (evade, casting) =
            self.owner
                .ai_cast_state(original, caster, self.starts, cast_flags)?;
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
            if is_melee(t) && self.owner.active_auto_spell(caster, self.starts)? == Some(spell) {
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

impl SourceSpell {
    /// Unit.cpp:1494-1509. No explicit unit target is valid for implicit self spells.
    pub(in crate::native::source_execution) fn unit_cast_targets(
        &self,
        t: &SpellTemplate,
        target: Option<ObjectHandle>,
        owner: &mut (impl SpellExecutionOwner + ?Sized),
    ) -> Result<Option<SpellTargets>> {
        let mut targets = SpellTargets::default();
        if let Some(target) = target {
            let p = owner.unit(target)?.position;
            targets.unit = Some(target);
            targets.destination = Some([p[0], p[1], p[2]]);
            targets.mask |= 2;
        }
        if t.targets & 0x40 != 0 {
            if target.is_none() {
                return Ok(None);
            }
            targets.mask |= 0x40;
        }
        if t.targets & 0x20 != 0 {
            targets.source = owner.casting_source(self)?;
            if targets.source.is_some() {
                targets.mask |= 0x20;
            }
        }
        Ok(Some(targets))
    }
}
