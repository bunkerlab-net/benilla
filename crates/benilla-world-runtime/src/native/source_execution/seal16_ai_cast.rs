// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! CreatureEventAI.cpp:782-804; UnitAI.cpp:143-223. Live creation-time casting.
use super::super::addon_cast::{self, fresh_cast_time, prepare_trigger_data};
use super::*;
use crate::native::source_execution::main_spell::UnitPowerOwner;

pub(super) struct Binding<'a> {
    pub object: ObjectHandle,
    pub stores: &'a SourceSpellStores,
    pub targets: &'a SourceSpellTargetMgr,
    pub terrain: &'a mut SourceTerrainManager,
    pub world_clock: &'a SourceWorldClock,
    pub clocks: &'a mut ActiveClocks,
    pub random: &'a mut dyn CreatureRandom,
    pub creature_spells: Option<&'a CreatureSpellMapData>,
    pub max_casts: u32,
    pub broadcast: Option<(&'a SourceCells, u32)>,
}
impl Binding<'_> {
    pub(super) fn run<T>(
        &mut self,
        units: &mut SourceUnits,
        ais: &mut SourceCreatureAis,
        objects: &SourceObjects,
        run: impl FnOnce(&mut SpellStartContext<'_>) -> Result<T>,
    ) -> Result<T> {
        let ai = ais.get_mut(objects, self.object)?;
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
                units,
                ai,
                stores: self.stores,
                targets: self.targets,
                terrain: self.terrain,
                world_clock: self.world_clock,
                clocks: self.clocks,
                random: self.random,
                creature_spells: self.creature_spells,
                max_casts: self.max_casts,
                broadcast: self.broadcast,
            },
        });
        let live = units.get_mut(objects, self.object)?;
        live.spell_starts = starts;
        live.unit_events = events;
        result
    }
}
struct Owner<'a> {
    object: ObjectHandle,
    objects: &'a SourceObjects,
    units: &'a mut SourceUnits,
    ai: &'a mut SourceCreatureAi,
    stores: &'a SourceSpellStores,
    targets: &'a SourceSpellTargetMgr,
    terrain: &'a mut SourceTerrainManager,
    world_clock: &'a SourceWorldClock,
    clocks: &'a mut ActiveClocks,
    random: &'a mut dyn CreatureRandom,
    creature_spells: Option<&'a CreatureSpellMapData>,
    max_casts: u32,
    broadcast: Option<(&'a SourceCells, u32)>,
}
impl Owner<'_> {
    fn live(&self) -> Result<&SourceUnitFields> {
        self.units.get(self.objects, self.object)
    }
    fn power_owner(&self) -> UnitPowerOwner<'_> {
        UnitPowerOwner {
            objects: self.objects,
            units: self.units,
        }
    }
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
            self.live()?.spell_slots.current(slot).is_none(),
            "execution.spells.Unit_4304.InterruptSpell_live_cancel_owner",
        )
    }
    fn cast_self(&mut self, spell: &mut SourceSpell, skip: bool) -> Result<()> {
        spell.executing = true; // Spell.cpp:3162-3165.
        require(
            self.range_mod_owner(spell.caster)?.is_none(),
            "execution.spells.Spell_3163.SpellModRAII_owner",
        )?;
        let live = self.units.get_mut(self.objects, self.object)?;
        if !addon_cast::enter_cast(&mut live.auras.cast_counter, self.max_casts) {
            spell.finish_failed(self)?; // SendCastResult returns for a creature.
            spell.executing = false;
            return Ok(());
        }
        addon_cast::require_self_pointers(spell, self.objects)?;
        let t = self
            .stores
            .template(spell.spell)
            .ok_or_else(|| refuse("execution.spells.missing_template"))?;
        if !skip && spell.check_cast(t, false, self)? != SpellCastResult::Ok {
            // Spell.cpp:7857-7863; non-player SendCastResult is skipped.
            return Err(refuse("execution.spells.Spell_7860.SendInterrupted_owner"));
        }
        let unit = self.unit(spell.caster)?;
        let level = self.power_owner().power_level(spell.caster)?;
        let live = self.units.get_mut(self.objects, self.object)?;
        let base = &self.ai.base;
        addon_cast::Owner {
            object: self.object,
            objects: self.objects,
            terrain: self.terrain,
            stores: self.stores,
            targets: self.targets,
            flags: &live.flags,
            immunities: &live.immunities,
            auras: &mut live.auras,
            cooldowns: &mut live.cooldowns,
            stand: live.stand_state,
            mount: u32::from(live.mounted),
            unit,
            level,
            cast_speed: &live.cast_speed,
            creature_spell_list: live.spell_list.as_ref(),
            entry: &live.entry,
            random: self.random,
            creature_spells: self.creature_spells,
            clocks: self.clocks,
            world_clock: self.world_clock,
            broadcast: self.broadcast,
            bounding_radius: live.bounding_radius,
            max_casts: self.max_casts,
        }
        .cast_checked(spell, t, Some(base), |spell| {
            // UnitAI.cpp:352. A different Spell instance leaves the target alone.
            if spell.flags.notify_ai() && base.current_spell == Some(spell.id) {
                finish_ai_cast(base, spell, t, live.victim, &mut live.target)?;
            }
            Ok(())
        })
    }
}

fn ignores_cast_target(spell: &SourceSpell, t: &SpellTemplate) -> bool {
    let (a, b) = (t.effectimplicittargeta1, t.effectimplicittargetb1);
    a == 24 || (matches!(a, 18 | 22) && matches!(b, 15 | 16) && spell.cast_time == 0)
}

fn finish_ai_cast(
    base: &SourceUnitAi,
    spell: &SourceSpell,
    t: &SpellTemplate,
    victim: Option<ObjectHandle>,
    target: &mut Option<ObjectHandle>,
) -> Result<()> {
    // UnitAI.cpp:355-357,367-391,414-429. Finishing does not clear the pointer.
    if t.attributesex4 & 0x80 != 0 || t.attributes & 0x404 != 0 || ignores_cast_target(spell, t) {
        return Ok(());
    }
    require(
        t.id != 31306,
        "execution.spells.UnitAI_416.HandleDelayedInstantAnimation_owner",
    )?;
    *target = victim.filter(|_| !base.combat_script);
    Ok(())
}

impl SpellLocationOwner for Owner<'_> {}
impl SpellExecutionOwner for Owner<'_> {
    fn unit(&mut self, unit: ObjectHandle) -> Result<SpellUnitState> {
        self.require_unit(unit)?;
        let object = self.objects.get(unit)?;
        require(
            object.kind == SourceObjectKind::Creature,
            "execution.spells.ai_cast.non_generic_creature_owner",
        )?;
        let movement = self
            .live()?
            .movement
            .as_ref()
            .and_then(|movement| movement.flags)
            .ok_or_else(|| refuse("execution.spells.ai_cast.movement_flags_owner"))?;
        Ok(SpellUnitState {
            player: false,
            gameobject: false,
            alive: object.alive,
            standing: self.live()?.stand_state == 0,
            player_controlled: self.live()?.flags & 8 != 0,
            combat: self.live()?.in_combat(),
            battleground_wait_leave: false,
            gm: self.live()?.gm,
            moving: movement & 0x0400_60cf != 0,
            falling_far: movement & 0x4000 != 0,
            stealth: self.live()?.auras.has_aura_type(16),
            form: self.live()?.shapeshift_form,
            class: self
                .live()?
                .unit_class
                .ok_or_else(|| refuse("execution.spells.ai_cast.unit_class_owner"))?,
            combo_target: self.live()?.combo_target,
            mounted: self.live()?.mounted,
            taxi: self.live()?.flags & 0x0010_0000 != 0,
            totem: self.live()?.totem,
            untargetable: self.live()?.flags & 0x0001_0000 != 0,
            has_ai: self.live()?.has_ai,
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
        self.require_unit(original)?;
        // UnitAI.cpp:175 reads AI order, not Unit's evade flags.
        let evade = self.ai.base.order == 4;
        if evade && cast_flags & 2 == 0 {
            return Ok((true, false));
        }
        self.require_unit(caster)?;
        Ok((
            evade,
            self.live()?.spell_slots.is_non_melee_spell_casted(
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
        Ok(self.live()?.auras.has_aura(spell))
    }
    fn active_auto_spell(
        &mut self,
        unit: ObjectHandle,
        starts: &SourceSpellStarts,
    ) -> Result<Option<u32>> {
        self.require_unit(unit)?;
        self.live()?
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
        if check == SpellCheck::IsSpellReady {
            self.require_unit(spell.caster)?;
            let mut clock = self.world_clock;
            spell.cooldown_lookups[0] += 1;
            if self.object.thread == 6 && self.objects.get(self.object)?.database_guid == 40520 {
                assert_eq!((spell.id, spell.spell), (3, 10848));
                assert!(self.live()?.cooldowns.is_empty());
            }
            return Ok(
                if self.live()?.cooldowns.is_spell_ready(
                    t,
                    spell.item_cast,
                    self.objects.get(spell.caster)?.in_world,
                    &mut clock,
                )? {
                    SpellCastResult::Ok
                } else {
                    SpellCastResult::NotReady
                },
            );
        }
        if check == SpellCheck::HasGcd {
            self.require_unit(spell.caster)?;
            spell.cooldown_lookups[1] += 1;
            return Ok(
                if self
                    .live()?
                    .cooldowns
                    .gcd
                    .has(t.startrecoverycategory, self.world_clock.current_ms())
                {
                    SpellCastResult::NotReady
                } else {
                    SpellCastResult::Ok
                },
            );
        }
        if check == SpellCheck::Shapeshift {
            return self
                .stores
                .shapeshift_error(t, self.live()?.shapeshift_form);
        }
        if check == SpellCheck::SpiritOfRedemption {
            return Ok(
                if self.live()?.auras.has_aura_type(176) && t.attributesex3 & 0x0010_0000 == 0 {
                    SpellCastResult::BadTargets
                } else {
                    SpellCastResult::Ok
                },
            );
        }
        if let SpellCheck::StrongerAura(_) = check {
            for (&id, existing) in &self.live()?.auras.aura_holders {
                let old = self
                    .stores
                    .template(id)
                    .ok_or_else(|| refuse("execution.spells.missing_template"))?;
                require(
                    existing.caster == spell.caster,
                    "execution.spells.Spell_4965.different_caster_stacker_owner",
                )?;
                spell.stacker_lookups += 1;
                if spell_stackable(Some(t), Some(old))? {
                    spell.stacker_attribute_exits += 1;
                    continue;
                }
                return Err(refuse(
                    "execution.spells.Spell_4977.CalculateSpellEffectValue_owner",
                ));
            }
            return Ok(SpellCastResult::Ok);
        }
        if let SpellCheck::PositiveImmunity(mask) = check {
            let positive =
                positive_unit_spell(self.stores, t, 7, Some((spell.caster, self.object)))?;
            return Ok(
                if positive
                    && mask != 0
                    && self.live()?.immunities.immune_to_self_spell(
                        self.stores,
                        t,
                        mask,
                        self.object,
                        &self.live()?.auras,
                    )?
                {
                    SpellCastResult::TargetAuraState
                } else {
                    SpellCastResult::Ok
                },
            );
        }
        if let SpellCheck::AllowedInLocation(location) = check {
            require(
                self.live()?.owner.is_none() && self.live()?.charmer.is_none(),
                "execution.spells.Unit_6484.GetMaster_owner",
            )?;
            let map = self
                .objects
                .get(self.object)?
                .map
                .ok_or_else(|| refuse("execution.spells.location.WorldObject_GetMapId_owner"))?
                .map;
            let result =
                self.stores
                    .allowed_in_location(t, map, location[0], location[1], None, self)?;
            spell.check_suffix_lookups[0] += 1;
            return Ok(result);
        }
        if check == SpellCheck::Items {
            let result = spell.check_items(t, false, self)?;
            spell.check_suffix_lookups[1] += 1;
            return Ok(result);
        }
        if let SpellCheck::Range(strict) = check {
            spell.check_suffix_lookups[2] += 1;
            return spell.check_range(t, strict, self);
        }
        if let SpellCheck::Power(strict) = check {
            let result = spell.check_power(t, strict, self)?;
            spell.power_lookups[0] += 1;
            return Ok(result);
        }
        if check == SpellCheck::CasterAuras {
            let result = spell.check_caster_auras(t, self)?;
            spell.power_lookups[1] += 1;
            return Ok(result);
        }
        Err(check.missing_owner())
    }
    fn set_current(
        &mut self,
        spell: &mut SourceSpell,
        t: &SpellTemplate,
        spells: &mut BTreeMap<u64, SourceSpell>,
    ) -> Result<()> {
        self.require_unit(spell.caster)?;
        // Interruption callbacks also use this owner. Put the slot state back
        // after dispatch, including on a refusal.
        let mut slots =
            std::mem::take(&mut self.units.get_mut(self.objects, self.object)?.spell_slots);
        let result = slots.set(spell, t, spells, self.stores, self);
        self.units.get_mut(self.objects, self.object)?.spell_slots = slots;
        result
    }

    fn add_gcd(&mut self, spell: &SourceSpell, t: &SpellTemplate) -> Result<()> {
        self.require_unit(spell.caster)?;
        let clock = self.world_clock;
        self.units
            .get_mut(self.objects, self.object)?
            .cooldowns
            .add_gcd(t, 0, || Ok(clock.current_ms()))
    }
    fn power_objects(&mut self) -> Result<&SourceObjects> {
        Ok(self.objects)
    }
    fn power_context(&mut self) -> Result<SpellPowerContext<'_>> {
        Ok(SpellPowerContext {
            objects: self.objects,
            units: self.units,
        })
    }
    fn power_mod_owner(&mut self, caster: ObjectHandle) -> Result<Option<ObjectHandle>> {
        self.range_mod_owner(caster)
    }
    fn power_level(&mut self, caster: ObjectHandle) -> Result<u32> {
        self.require_unit(caster)?;
        self.power_owner().power_level(caster)
    }
    fn power_create_health(&mut self, caster: ObjectHandle) -> Result<u32> {
        self.require_unit(caster)?;
        self.power_owner().power_create_health(caster)
    }
    fn power_create_mana(&mut self, caster: ObjectHandle) -> Result<u32> {
        self.require_unit(caster)?;
        self.power_owner().power_create_mana(caster)
    }
    fn power_current(&mut self, caster: ObjectHandle, ty: u32) -> Result<u32> {
        self.require_unit(caster)?;
        self.power_owner().power_current(caster, ty)
    }
    fn power_maximum(&mut self, caster: ObjectHandle, ty: u32) -> Result<u32> {
        self.require_unit(caster)?;
        self.power_owner().power_maximum(caster, ty)
    }
    fn power_flat_modifier(&mut self, caster: ObjectHandle, school: u32) -> Result<i32> {
        self.require_unit(caster)?;
        self.power_owner().power_flat_modifier(caster, school)
    }
    fn power_multiplier(&mut self, caster: ObjectHandle, school: u32) -> Result<u32> {
        self.require_unit(caster)?;
        self.power_owner().power_multiplier(caster, school)
    }
    fn cast_time(&mut self, spell: &mut SourceSpell, t: &SpellTemplate) -> Result<i32> {
        require(
            self.range_mod_owner(spell.caster)?.is_none(),
            "execution.spells.Spell_3033.SpellModRAII_owner",
        )?;
        let caster = spell.caster;
        let time = fresh_cast_time(
            |index| self.stores.dbc("SpellCastTimes.dbc", index),
            spell,
            t,
            || self.power_owner().power_level(caster),
            || Ok(self.live()?.cast_speed),
        )?;
        spell.prepare_lookups[1] += 1;
        Ok(time)
    }
    fn call(&mut self, spell: &mut SourceSpell, call: SpellCall) -> Result<()> {
        match call {
            SpellCall::PrepareTriggerData => {
                prepare_trigger_data(self.stores, spell)?;
                spell.prepare_lookups[0] += 1;
                Ok(())
            }
            SpellCall::RemoveCastAuras(flag) => {
                let cast = self
                    .stores
                    .template(spell.spell)
                    .ok_or_else(|| refuse("execution.spells.missing_template"))?;
                addon_cast::check_cast_aura_removals(
                    self.stores,
                    &self.live()?.auras,
                    cast,
                    self.object,
                    flag,
                    &mut spell.prepare_lookups[2],
                )
            }
            SpellCall::SendStart => {
                self.require_unit(spell.caster)?;
                let t = self
                    .stores
                    .template(spell.spell)
                    .ok_or_else(|| refuse("execution.spells.missing_template"))?;
                spell.spell_start = addon_cast::spell_start_packet(spell, t, self.objects)?;
                if spell.spell_start.is_some() {
                    spell.start_broadcast = Some(addon_cast::broadcast(
                        self.objects,
                        spell.caster,
                        self.broadcast,
                        self.live()?.bounding_radius,
                    )?);
                }
                Ok(())
            }
            SpellCall::Cast(skip) => self.cast_self(spell, skip),
            SpellCall::AiCastState(state) => {
                self.require_unit(spell.caster)?;
                // UnitAI.cpp:352 compares the live Spell identity before reading its template.
                if !state && self.ai.base.current_spell != Some(spell.id) {
                    return Ok(());
                }
                let t = self
                    .stores
                    .template(spell.spell)
                    .ok_or_else(|| refuse("execution.spells.missing_template"))?;
                if !state {
                    let live = self.units.get_mut(self.objects, self.object)?;
                    return finish_ai_cast(&self.ai.base, spell, t, live.victim, &mut live.target);
                }
                spell.prepare_lookups[3] += 1;
                if t.attributesex4 & 0x80 != 0 || t.attributes & 0x404 != 0 {
                    return Ok(());
                }
                require(
                    !((spell.cast_time != 0 && t.interruptflags & 1 != 0)
                        || (t.attributesex & 0x44 != 0 && t.channelinterruptflags & 8 != 0)),
                    "execution.spells.UnitAI_362.InterruptMoving_owner",
                )?;
                if ignores_cast_target(spell, t) {
                    return Ok(());
                }
                let force_target = matches!(t.effectimplicittargeta1, 6 | 21);
                spell.prepare_lookups[4] += 1;
                require(
                    !spell
                        .targets
                        .unit
                        .is_some_and(|target| target != self.object || force_target),
                    "execution.spells.UnitAI_397.SetTarget_owner",
                )?;
                spell.prepare_lookups[5] += 1;
                require(
                    self.live()?.target.is_none(),
                    "execution.spells.UnitAI_407.SetFacingTo_owner",
                )?;
                self.ai.base.current_spell = Some(spell.id);
                Ok(())
            }
            SpellCall::AiChannelState(state) => {
                self.require_unit(spell.caster)?;
                if state {
                    // UnitAI.cpp:435-436 clears even when the channel allows another cast.
                    self.ai.base.current_spell = None;
                }
                let t = self
                    .stores
                    .template(spell.spell)
                    .ok_or_else(|| refuse("execution.spells.missing_template"))?;
                let track = t.attributesex & 0x4000 != 0;
                if !track && t.attributesex4 & 0x80 != 0 {
                    return Ok(());
                }
                if state {
                    let force = track || !matches!(t.effectimplicittargeta1, 25 | 38);
                    let untargeted = !force || (!track && t.channelinterruptflags & 0x10 != 0);
                    let other = spell
                        .targets
                        .unit
                        .is_some_and(|target| target != self.object);
                    return Err(refuse(if !untargeted && other {
                        "execution.spells.UnitAI_469.SetTarget_owner"
                    } else if untargeted {
                        "execution.spells.UnitAI_464.SetFacingTo_owner"
                    } else {
                        "execution.spells.UnitAI_474.SetFacingTo_owner"
                    }));
                }
                let victim = self.live()?.victim.filter(|_| !self.ai.base.combat_script);
                self.units.get_mut(self.objects, self.object)?.target = victim;
                Ok(())
            }
            _ => Err(call.missing_owner()),
        }
    }
    fn range_objects(&mut self) -> Result<&SourceObjects> {
        Ok(self.objects)
    }
    fn range_stores(&mut self) -> Result<&SourceSpellStores> {
        Ok(self.stores)
    }
    fn moving_forward(&mut self, unit: ObjectHandle) -> Result<bool> {
        self.require_unit(unit)?;
        let flags = self
            .live()?
            .movement
            .as_ref()
            .and_then(|m| m.flags)
            .ok_or_else(|| refuse("execution.spells.ai_cast.movement_flags_owner"))?;
        Ok(flags & 0x200d != 0)
    }
    fn walking(&mut self, unit: ObjectHandle) -> Result<bool> {
        self.require_unit(unit)?;
        let flags = self
            .live()?
            .movement
            .as_ref()
            .and_then(|m| m.flags)
            .ok_or_else(|| refuse("execution.spells.ai_cast.movement_flags_owner"))?;
        Ok(flags & 0x100 != 0)
    }
    fn range_mod_owner(&mut self, unit: ObjectHandle) -> Result<Option<ObjectHandle>> {
        self.require_unit(unit)?;
        world_object_spell_mod_owner(self.objects, unit, self.live()?.totem)
    }
    fn combat_reach(&mut self, unit: ObjectHandle) -> Result<f32> {
        self.require_unit(unit)?;
        self.live()?
            .combat_reach
            .map(f32::from_bits)
            .ok_or_else(|| refuse("execution.spells.Unit_1095.UNIT_FIELD_COMBATREACH_owner"))
    }
}

#[cfg(test)]
#[path = "seal16_ai_cast_tests.rs"]
mod tests;
