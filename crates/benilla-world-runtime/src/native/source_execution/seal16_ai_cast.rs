// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! CreatureEventAI.cpp:782-804; UnitAI.cpp:143-223. Live creation-time casting.
use super::super::addon_cast;
use super::*;
use crate::native::source_execution::main_spell::UnitPowerOwner;

pub(in super::super) struct Binding<'a> {
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
        let result = self.run_with_base(units, &mut ai.base, objects, run)?;
        let object = objects.get(self.object)?;
        let live = units.get(objects, self.object)?;
        let (starts, events) = (&live.spell_starts, &live.unit_events);
        if self.object.thread == 6
            && object.database_guid == 40520
            && starts
                .spells
                .get(&4)
                .is_some_and(|spell| spell.state == SourceSpellState::Finished)
        {
            assert_guid40520_queue(starts, events);
        }
        Ok(result)
    }

    pub(in super::super) fn run_with_base<T>(
        &mut self,
        units: &mut SourceUnits,
        base: &mut SourceUnitAi,
        objects: &SourceObjects,
        run: impl FnOnce(&mut SpellStartContext<'_>) -> Result<T>,
    ) -> Result<T> {
        units.get(objects, self.object)?;
        run(&mut SpellStartContext {
            stores: self.stores,
            storage: SpellStartStorage::Unit,
            owner: &mut Owner {
                object: self.object,
                objects,
                units,
                base,
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
        })
    }
}

struct Owner<'a> {
    object: ObjectHandle,
    objects: &'a SourceObjects,
    units: &'a mut SourceUnits,
    base: &'a mut SourceUnitAi,
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
        // Unit.cpp:4304 reads this unit's current Spell slot.
        let live = self.units.get(self.objects, unit)?;
        // Unit.cpp:4304. Empty slots return before cancellation or callbacks.
        require(
            live.spell_slots.current(slot).is_none(),
            "execution.spells.Unit_4304.InterruptSpell_live_cancel_owner",
        )
    }
}

impl SpellSelfCastOwner for Owner<'_> {
    fn cast_objects(&self) -> &SourceObjects {
        self.objects
    }
    fn cast_mod_owner(&mut self, caster: ObjectHandle) -> Result<Option<ObjectHandle>> {
        self.range_mod_owner(caster)
    }
    fn cast_chain(&mut self) -> Result<(&mut SourceUnitAuras, u32)> {
        let live = self.units.get_mut(self.objects, self.object)?;
        Ok((&mut live.auras, self.max_casts))
    }
    fn cast_checked(&mut self, spell: &mut SourceSpell, t: &SpellTemplate) -> Result<()> {
        let unit = self.unit(spell.caster)?;
        let level = self.power_owner().power_level(spell.caster)?;
        let base = &*self.base;
        let mut effects = addon_cast::Owner {
            object: self.object,
            objects: self.objects,
            terrain: self.terrain,
            stores: self.stores,
            targets: self.targets,
            units: self.units,
            unit,
            level,
            random: self.random,
            creature_spells: self.creature_spells,
            clocks: self.clocks,
            world_clock: self.world_clock,
            broadcast: self.broadcast,
            max_casts: self.max_casts,
        };
        spell.cast_checked(
            t,
            self.stores,
            Some(base),
            &mut effects,
            |effects, spell| {
                // UnitAI.cpp:352. A different Spell instance leaves the target alone.
                if spell.flags.notify_ai() && base.current_spell == Some(spell.id) {
                    let live = effects.units.get_mut(effects.objects, spell.caster)?;
                    finish_ai_cast(
                        base,
                        spell,
                        t,
                        live.victim,
                        &mut live.target,
                        effects.objects,
                    )?;
                }
                Ok(())
            },
        )
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
    objects: &SourceObjects,
) -> Result<()> {
    // UnitAI.cpp:355-357,367-391,414-429. Finishing does not clear the pointer.
    if t.attributesex4 & 0x80 != 0 || t.attributes & 0x404 != 0 || ignores_cast_target(spell, t) {
        return Ok(());
    }
    require(
        t.id != 31306,
        "execution.spells.UnitAI_416.HandleDelayedInstantAnimation_owner",
    )?;
    objects.set_unit_target(spell.caster, target, victim.filter(|_| !base.combat_script))
}

impl SpellLocationOwner for Owner<'_> {}
impl SpellExecutionOwner for Owner<'_> {
    fn spell_storage(
        &mut self,
        caster: ObjectHandle,
    ) -> Result<(&mut SourceSpellStarts, &mut SourceUnitEvents)> {
        let live = self.units.get_mut(self.objects, caster)?;
        Ok((&mut live.spell_starts, &mut live.unit_events))
    }
    fn unit(&mut self, unit: ObjectHandle) -> Result<SpellUnitState> {
        // Unit.cpp:1495 and Spell.cpp:4830-4869 read the target's own state.
        self.require_unit(unit)?;
        let object = self.objects.get(unit)?;
        require(
            object.kind == SourceObjectKind::Creature,
            "execution.spells.ai_cast.non_generic_creature_owner",
        )?;
        let live = self.units.get(self.objects, unit)?;
        let movement = live
            .movement
            .as_ref()
            .and_then(|movement| movement.flags)
            .ok_or_else(|| refuse("execution.spells.ai_cast.movement_flags_owner"))?;
        Ok(SpellUnitState {
            player: false,
            gameobject: false,
            alive: object.alive,
            standing: live.stand_state == 0,
            player_controlled: live.flags & 8 != 0,
            combat: live.flags & UNIT_FLAG_COMBAT != 0,
            battleground_wait_leave: false,
            gm: live.gm,
            moving: movement & 0x0400_60cf != 0,
            falling_far: movement & 0x4000 != 0,
            stealth: live.auras.has_aura_type(16),
            form: live.shapeshift_form,
            class: live
                .unit_class
                .ok_or_else(|| refuse("execution.spells.ai_cast.unit_class_owner"))?,
            combo_target: live.combo_target,
            mounted: live.is_mounted(),
            taxi: live.flags & 0x0010_0000 != 0,
            totem: live.totem,
            untargetable: live.flags & 0x0001_0000 != 0,
            has_ai: live.has_ai,
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
        starts: Option<&SourceSpellStarts>,
        cast_flags: u32,
    ) -> Result<(bool, bool)> {
        self.require_unit(original)?;
        // UnitAI.cpp:175 reads AI order, not Unit's evade flags.
        let evade = self.base.order == 4;
        if evade && cast_flags & 2 == 0 {
            return Ok((true, false));
        }
        // UnitAI.cpp:178 reads the selected caster's current cast.
        let live = self.units.get(self.objects, caster)?;
        Ok((
            evade,
            live.spell_slots.is_non_melee_spell_casted(
                starts.unwrap_or(&live.spell_starts),
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
        // UnitAI.cpp:188-190 checks the selected target's auras.
        let live = self.units.get(self.objects, unit)?;
        Ok(live.auras.has_aura(spell))
    }
    fn active_auto_spell(
        &mut self,
        unit: ObjectHandle,
        starts: Option<&SourceSpellStarts>,
    ) -> Result<Option<u32>> {
        // UnitAI.cpp:204-208 reads the caster's active autorepeat Spell.
        let live = self.units.get(self.objects, unit)?;
        live.spell_slots
            .current(SpellSlot::AutoRepeat)
            .map(|id| {
                starts
                    .unwrap_or(&live.spell_starts)
                    .current_spell(id)
                    .map(|spell| spell.spell)
            })
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
            let live = self.units.get(self.objects, spell.caster)?;
            let mut clock = self.world_clock;
            spell.cooldown_lookups[0] += 1;
            if self.object.thread == 6 && self.objects.get(self.object)?.database_guid == 40520 {
                assert_eq!(
                    (spell.id, spell.spell),
                    if spell.id == 3 { (3, 10848) } else { (4, 9036) }
                );
                assert!(live.cooldowns.is_empty());
            }
            return Ok(
                if live.cooldowns.is_spell_ready(
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
            let live = self.units.get(self.objects, spell.caster)?;
            spell.cooldown_lookups[1] += 1;
            return Ok(
                if live
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
            // Spell.cpp:4958-4965. Examine each holder in multimap order.
            for (id, existing) in self.live()?.auras.aura_holders() {
                let old = self
                    .stores
                    .template(id.spell())
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
            let live = self.live()?;
            let positive =
                positive_unit_spell(self.stores, t, 7, Some((spell.caster, self.object)))?;
            return Ok(
                if positive
                    && mask != 0
                    && live.immunities.immune_to_self_spell(
                        self.stores,
                        t,
                        mask,
                        self.object,
                        &live.auras,
                    )?
                {
                    SpellCastResult::TargetAuraState
                } else {
                    SpellCastResult::Ok
                },
            );
        }
        if let SpellCheck::TargetFilter(index) = check {
            let live = self.live()?;
            let relations = SourceCasterRelations {
                caster: self.object,
                objects: self.objects,
                flags: &live.flags,
                control_guids: &live.control_guids,
            };
            let result =
                spell.check_target_filter(t, usize::from(index), self.objects, &relations)?;
            if self.object.thread == 7
                && t.id == 11966
                && self.objects.get(self.object)?.database_guid == 33193
            {
                // SQL :154000, :426295, :847002; trace :60881926 and :60881944.
                assert_eq!(index, 0);
                assert_eq!(t.effects().map(|effect| effect.target_a), [21, 0, 0]);
                assert_eq!(t.effects().map(|effect| effect.target_b), [0; 3]);
                let data = SPELL_TARGET_INFO_TABLE[21];
                assert_eq!(
                    (data.target_type, data.filter, data.enumerator),
                    (
                        SpellTargetImplicitType::Unit,
                        SpellTargetFilter::Helpful,
                        SpellTargetEnumerator::Single,
                    )
                );
                assert_eq!(spell.targets.unit, Some(self.object));
                assert_eq!(
                    (spell.caster, spell.original_caster),
                    (self.object, self.object)
                );
                assert_eq!(live.flags & 0x0200_0008, 0);
                assert_eq!(result, SpellCastResult::Ok);
            }
            return Ok(result);
        }
        if let SpellCheck::AllowedInLocation(location) = check {
            let player = spell_beneficiary_from_guids(
                self.objects,
                self.object,
                self.live()?.control_guids,
            )?;
            let map = self
                .objects
                .get(self.object)?
                .map
                .ok_or_else(|| refuse("execution.spells.location.WorldObject_GetMapId_owner"))?
                .map;
            let result =
                self.stores
                    .allowed_in_location(t, map, location[0], location[1], player, self)?;
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
        _: Option<&mut BTreeMap<u64, SourceSpell>>,
    ) -> Result<()> {
        self.require_unit(spell.caster)?;
        self.units
            .set_current_spell(self.objects, self.stores, spell, t)
    }

    fn add_gcd(&mut self, spell: &SourceSpell, t: &SpellTemplate) -> Result<()> {
        self.require_unit(spell.caster)?;
        let clock = self.world_clock;
        self.units
            .get_mut(self.objects, spell.caster)?
            .cooldowns
            .add_gcd(t, 0, || Ok(clock.current_ms()))
    }
    fn power_objects(&mut self) -> Result<&SourceObjects> {
        Ok(self.objects)
    }
    fn power_health(&mut self, caster: ObjectHandle) -> Result<u32> {
        // Spell.cpp:6321,6380 read the true caster's current health.
        self.power_owner().power_health(caster)
    }
    fn caster_aura_flags(&mut self, caster: ObjectHandle) -> Result<u32> {
        // Spell.cpp:6029 reads UNIT_FIELD_FLAGS from the true caster.
        self.require_unit(caster)?;
        Ok(self.units.get(self.objects, caster)?.flags)
    }
    fn caster_charmer(&mut self, caster: ObjectHandle) -> Result<SourceControlLookup> {
        // Spell.cpp:6034 reads the true caster's charmer.
        self.require_unit(caster)?;
        self.units
            .get(self.objects, caster)?
            .control_guids
            .get_charmer(self.objects, caster, None, unbound_pet_loading)
    }
    fn power_mod_owner(&mut self, caster: ObjectHandle) -> Result<Option<ObjectHandle>> {
        self.range_mod_owner(caster)
    }
    fn power_level(&mut self, caster: ObjectHandle) -> Result<u32> {
        self.power_owner().power_level(caster)
    }
    fn power_create_health(&mut self, caster: ObjectHandle) -> Result<u32> {
        self.power_owner().power_create_health(caster)
    }
    fn power_create_mana(&mut self, caster: ObjectHandle) -> Result<u32> {
        self.power_owner().power_create_mana(caster)
    }
    fn power_current(&mut self, caster: ObjectHandle, ty: u32) -> Result<u32> {
        self.power_owner().power_current(caster, ty)
    }
    fn power_maximum(&mut self, caster: ObjectHandle, ty: u32) -> Result<u32> {
        self.power_owner().power_maximum(caster, ty)
    }
    fn power_flat_modifier(&mut self, caster: ObjectHandle, school: u32) -> Result<i32> {
        self.power_owner().power_flat_modifier(caster, school)
    }
    fn power_multiplier(&mut self, caster: ObjectHandle, school: u32) -> Result<u32> {
        self.power_owner().power_multiplier(caster, school)
    }
    fn cast_time(&mut self, spell: &mut SourceSpell, t: &SpellTemplate) -> Result<i32> {
        let caster = spell.caster;
        let mod_owner = self.range_mod_owner(caster)?;
        let time = spell.creature_cast_time(
            t,
            self.stores,
            mod_owner,
            || self.power_owner().power_level(caster),
            || Ok(self.units.get(self.objects, caster)?.cast_speed),
        )?;
        spell.prepare_lookups[1] += 1;
        Ok(time)
    }
    fn cast_scheduled(&mut self, spell: &mut SourceSpell, skip: bool) -> Result<()> {
        spell.cast_self(skip, SelfCastCaller::UnitAi, self.stores, self)
    }
    fn call(&mut self, spell: &mut SourceSpell, call: SpellCall) -> Result<()> {
        match call {
            SpellCall::SendResult(_) => {
                self.require_unit(spell.caster)?;
                // Spell.cpp:3815-3830. This owner supports an ordinary Creature cast.
                require(
                    self.objects.get(spell.caster)?.kind == SourceObjectKind::Creature,
                    "execution.spells.Spell_3823.cast_result_recipient_owner",
                )
            }
            SpellCall::PrepareTriggerData => {
                spell.prepare_trigger_data(self.stores)?;
                spell.prepare_lookups[0] += 1;
                Ok(())
            }
            SpellCall::RemoveCastAuras(flag) => {
                let cast = self
                    .stores
                    .template(spell.spell)
                    .ok_or_else(|| refuse("execution.spells.missing_template"))?;
                let object = self.objects.get(self.object)?;
                let fields = self.units.get_mut(self.objects, self.object)?;
                let live = SelfAuraRemoval {
                    unit: SelfAuraTarget {
                        target: self.object,
                        level: fields.power_fields.as_ref().map(|fields| fields.level),
                        alive: object.alive,
                        player: object.kind == SourceObjectKind::Player,
                        player_group: None,
                        totem: fields.totem,
                        objects: self.objects,
                        control_guids: &fields.control_guids,
                    },
                    visibility: &mut fields.visibility,
                    damage: DamageAuraTarget::new(
                        &mut fields.creature_damage,
                        &fields.loaded_creature,
                        fields.flags,
                        fields.shapeshift_form,
                    ),
                };
                check_cast_aura_removals(self.stores, &mut fields.auras, cast, live, flag, || {
                    spell.prepare_lookups[2] += 1
                })
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
            SpellCall::AiCastState(state) => {
                self.require_unit(spell.caster)?;
                // UnitAI.cpp:352 compares the live Spell identity before reading its template.
                if !state && self.base.current_spell != Some(spell.id) {
                    return Ok(());
                }
                let t = self
                    .stores
                    .template(spell.spell)
                    .ok_or_else(|| refuse("execution.spells.missing_template"))?;
                if !state {
                    let live = self.units.get_mut(self.objects, self.object)?;
                    return finish_ai_cast(
                        self.base,
                        spell,
                        t,
                        live.victim,
                        &mut live.target,
                        self.objects,
                    );
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
                self.base.current_spell = Some(spell.id);
                Ok(())
            }
            SpellCall::AiChannelState(state) => {
                self.require_unit(spell.caster)?;
                if state {
                    // UnitAI.cpp:435-436 clears even when the channel allows another cast.
                    self.base.current_spell = None;
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
                let live = self.units.get_mut(self.objects, self.object)?;
                let victim = live.victim.filter(|_| !self.base.combat_script);
                self.objects
                    .set_unit_target(self.object, &mut live.target, victim)
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
        // Spell.cpp:6196 checks both caster and target movement.
        let live = self.units.get(self.objects, unit)?;
        let flags = live
            .movement
            .as_ref()
            .and_then(|m| m.flags)
            .ok_or_else(|| refuse("execution.spells.ai_cast.movement_flags_owner"))?;
        Ok(flags & 0x200d != 0)
    }
    fn walking(&mut self, unit: ObjectHandle) -> Result<bool> {
        // Spell.cpp:6196 checks both caster and target movement.
        let live = self.units.get(self.objects, unit)?;
        let flags = live
            .movement
            .as_ref()
            .and_then(|m| m.flags)
            .ok_or_else(|| refuse("execution.spells.ai_cast.movement_flags_owner"))?;
        Ok(flags & 0x100 != 0)
    }
    fn range_mod_owner(&mut self, unit: ObjectHandle) -> Result<Option<ObjectHandle>> {
        // Spell.cpp:6207 reads the selected caster's mod owner.
        self.require_unit(unit)?;
        world_object_spell_mod_owner(
            self.objects,
            unit,
            self.units.get(self.objects, unit)?.totem,
        )
    }
    fn combat_reach(&mut self, unit: ObjectHandle) -> Result<f32> {
        // Spell.cpp:6173,6181,6189 read both units' combat reach.
        let live = self.units.get(self.objects, unit)?;
        live.combat_reach
            .map(f32::from_bits)
            .ok_or_else(|| refuse("execution.spells.Unit_1095.UNIT_FIELD_COMBATREACH_owner"))
    }
}

#[cfg(test)]
#[path = "seal16_ai_cast_tests.rs"]
mod tests;
