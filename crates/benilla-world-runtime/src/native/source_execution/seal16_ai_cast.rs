// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! CreatureEventAI.cpp:782-804; UnitAI.cpp:143-223. Live creation-time casting.
use super::*;

pub(super) struct Binding<'a> {
    pub object: ObjectHandle,
    pub stores: &'a SourceSpellStores,
    pub targets: &'a SourceSpellTargetMgr,
    pub terrain: &'a mut SourceTerrainManager,
}
impl Binding<'_> {
    pub(super) fn run<T>(
        &mut self,
        units: &mut SourceUnits,
        ais: &SourceCreatureAis,
        objects: &SourceObjects,
        run: impl FnOnce(&mut SpellStartContext<'_>) -> Result<T>,
    ) -> Result<T> {
        let live = units.get_mut(objects, self.object)?;
        // Move the owners for the borrow only. Restore them on success and refusal.
        let mut starts = std::mem::take(&mut live.spell_starts);
        let mut events = std::mem::take(&mut live.unit_events);
        let result = run(&mut SpellStartContext {
            stores: self.stores,
            starts: &mut starts,
            events: &mut events,
            owner: &mut Owner {
                object: self.object,
                objects,
                live,
                ais,
                stores: self.stores,
                targets: self.targets,
                terrain: self.terrain,
            },
        });
        live.spell_starts = starts;
        live.unit_events = events;
        result
    }
}
struct Owner<'a> {
    object: ObjectHandle,
    objects: &'a SourceObjects,
    live: &'a mut SourceUnitFields,
    ais: &'a SourceCreatureAis,
    stores: &'a SourceSpellStores,
    targets: &'a SourceSpellTargetMgr,
    terrain: &'a mut SourceTerrainManager,
}
impl Owner<'_> {
    fn require_unit(&self, unit: ObjectHandle) -> Result<()> {
        self.objects.get(unit)?;
        require(
            unit == self.object,
            "execution.spells.ai_cast.other_unit_owner",
        )
    }
    fn interrupt(&self, unit: ObjectHandle, slot: SpellSlot) -> Result<()> {
        self.require_unit(unit)?;
        // Unit.cpp:4304. Empty slots return before cancellation or callbacks.
        require(
            self.live.spell_slots.current(slot).is_none(),
            "execution.spells.Unit_4304.InterruptSpell_live_cancel_owner",
        )
    }
}
impl SpellExecutionOwner for Owner<'_> {
    fn unit(&mut self, unit: ObjectHandle) -> Result<SpellUnitState> {
        self.require_unit(unit)?;
        let object = self.objects.get(unit)?;
        require(
            object.kind == SourceObjectKind::Creature,
            "execution.spells.ai_cast.non_generic_creature_owner",
        )?;
        let movement = self
            .live
            .movement
            .as_ref()
            .and_then(|movement| movement.flags)
            .ok_or_else(|| refuse("execution.spells.ai_cast.movement_flags_owner"))?;
        Ok(SpellUnitState {
            player: false,
            gameobject: false,
            alive: object.alive,
            standing: self.live.stand_state == 0,
            player_controlled: self.live.flags & 8 != 0,
            combat: self.live.in_combat(),
            battleground_wait_leave: false,
            gm: self.live.gm,
            moving: movement & 0x0400_60cf != 0,
            falling_far: movement & 0x4000 != 0,
            stealth: self.live.auras.has_aura_type(16),
            form: self.live.shapeshift_form,
            class: self
                .live
                .unit_class
                .ok_or_else(|| refuse("execution.spells.ai_cast.unit_class_owner"))?,
            combo_target: self.live.combo_target,
            mounted: self.live.mounted,
            taxi: self.live.flags & 0x0010_0000 != 0,
            totem: self.live.totem,
            untargetable: self.live.flags & 0x0001_0000 != 0,
            has_ai: self.live.has_ai,
            position: object
                .pose
                .ok_or_else(|| refuse("execution.spells.WorldObject_position_owner"))?
                .0,
        })
    }
    fn ai_cast_state(
        &mut self,
        original: ObjectHandle,
        caster: ObjectHandle,
        starts: &SourceSpellStarts,
        cast_flags: u32,
    ) -> Result<(bool, bool)> {
        // UnitAI.cpp:175 reads AI order, not Unit's evade flags.
        let evade = self.ais.get(self.objects, original)?.base.order == 4;
        if evade && cast_flags & 2 == 0 {
            return Ok((true, false));
        }
        self.require_unit(caster)?;
        Ok((
            evade,
            self.live.spell_slots.is_non_melee_spell_casted(
                starts,
                self.stores,
                false,
                false,
                false,
                false,
                false,
            )?,
        ))
    }
    fn has_aura(&mut self, unit: ObjectHandle, spell: u32) -> Result<bool> {
        self.require_unit(unit)?;
        Ok(self.live.auras.has_aura(spell))
    }
    fn active_auto_spell(
        &mut self,
        unit: ObjectHandle,
        starts: &SourceSpellStarts,
    ) -> Result<Option<u32>> {
        self.require_unit(unit)?;
        self.live
            .spell_slots
            .current(SpellSlot::AutoRepeat)
            .map(|id| starts.current_spell(id).map(|spell| spell.spell))
            .transpose()
    }
    fn interrupt_for_ai(&mut self, caster: ObjectHandle, melee: bool) -> Result<()> {
        if melee {
            return self.interrupt(caster, SpellSlot::Melee);
        }
        // Unit.cpp:4387-4396 preserves generic, autorepeat, channel order.
        for slot in [
            SpellSlot::Generic,
            SpellSlot::AutoRepeat,
            SpellSlot::Channel,
        ] {
            self.interrupt(caster, slot)?;
        }
        Ok(())
    }
    fn interrupt_slot(&mut self, caster: ObjectHandle, slot: u32) -> Result<()> {
        self.interrupt(
            caster,
            match slot {
                0 => SpellSlot::Melee,
                1 => SpellSlot::Generic,
                2 => SpellSlot::AutoRepeat,
                3 => SpellSlot::Channel,
                _ => {
                    return Err(refuse(
                        "execution.spells.EventAI_interrupt_slot_index_undefined",
                    ))
                }
            },
        )
    }
    fn map_time(&mut self, caster: ObjectHandle) -> Result<u32> {
        self.require_unit(caster)?;
        // Map.cpp:78-80; World.cpp:107,1389,1511. No World::Update before creation.
        Ok(0)
    }
    fn positive_definition(&mut self, t: &SpellTemplate) -> Result<bool> {
        positive_unit_spell(self.stores, t, 7, None)
    }
    fn targeting_data(&mut self, spell: u32) -> Result<[[u32; 2]; 3]> {
        self.targets
            .get_spell_targeting_data(spell)
            .map(|data| {
                data.filtering_scheme
                    .map(|row| row.map(|scheme| scheme as u32))
            })
            .map_err(|error| refuse(&format!("execution.spells.targeting.{error}")))
    }
    fn terrain_geometry(&mut self) -> Result<(&SourceObjects, &mut SourceTerrainManager)> {
        Ok((self.objects, self.terrain))
    }
    fn check(
        &mut self,
        spell: &mut SourceSpell,
        t: &SpellTemplate,
        check: SpellCheck,
    ) -> Result<SpellCastResult> {
        if let SpellCheck::PositiveImmunity(mask) = check {
            let positive =
                positive_unit_spell(self.stores, t, 7, Some((spell.caster, self.object)))?;
            return Ok(
                if positive
                    && mask != 0
                    && self.live.immunities.immune_to_self_spell(
                        self.stores,
                        t,
                        mask,
                        self.object,
                        &self.live.auras,
                    )?
                {
                    SpellCastResult::TargetAuraState
                } else {
                    SpellCastResult::Ok
                },
            );
        }
        Err(check.missing_owner())
    }
}

#[cfg(test)]
#[path = "seal16_ai_cast_tests.rs"]
mod tests;
