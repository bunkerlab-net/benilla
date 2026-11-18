// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Spell.cpp:3160-3370,533-1043,1163-1502. Triggered generic creature self casts.
use super::*;
use rustc_apfloat::{ieee::Single, Float, Round, Status};

// Unit.cpp:5250-5270. Walk the live aura list until an unowned removal.
pub(in super::super) fn check_cast_aura_removals(
    stores: &SourceSpellStores,
    auras: &SourceUnitAuras,
    cast: &SpellTemplate,
    caster: ObjectHandle,
    flag: u32,
    lookups: &mut u32,
) -> Result<()> {
    for (&id, holder) in &auras.aura_holders {
        let old = stores
            .template(id)
            .ok_or_else(|| refuse("execution.spells.missing_template"))?;
        *lookups += 1;
        if old.aurainterruptflags & flag != 0
            && (id != cast.id || holder.caster != caster)
            && (cast.attributesex & 0x20 == 0 || old.dispel != 5)
            && (cast.attributesex2 & 0x4000 == 0 || old.dispel != 6)
        {
            return Err(refuse(
                "execution.spells.Unit_5270.RemoveSpellAuraHolder_owner",
            ));
        }
    }
    Ok(())
}

impl Owner<'_> {
    pub(super) fn assert_cast_entry(&self, s: &SourceSpell) -> Result<()> {
        if s.spell == 9036 && self.objects.get(s.caster)?.database_guid == 40520 {
            assert_eq!(s.state, SourceSpellState::Casting);
            assert_eq!(
                (s.spell, s.caster, s.original_caster),
                (9036, self.object, self.object)
            );
            assert_eq!(s.flags, SpellTriggerFlags(1));
            assert_eq!((s.cast_time, s.timer, s.creation_time), (0, 0, 0));
            assert_eq!((s.targets.unit, s.targets.mask), (Some(self.object), 2));
            assert_eq!(s.cast_position, self.objects.get(s.caster)?.pose.unwrap().0);
            assert_eq!(
                s.trigger_data,
                Some(SpellTriggerData {
                    can_trigger: true,
                    negative_effect_mask: 0
                })
            );
            assert!(!s.referenced && !s.executing && !s.client_cast && !s.item_cast);
        }
        Ok(())
    }
}

impl SpellSelfCastOwner for Owner<'_> {
    fn cast_objects(&self) -> &SourceObjects {
        self.objects
    }
    fn cast_mod_owner(&mut self, caster: ObjectHandle) -> Result<Option<ObjectHandle>> {
        world_object_spell_mod_owner(self.objects, caster, self.unit.totem)
    }
    fn cast_chain(&mut self) -> (&mut SourceUnitAuras, u32) {
        (self.auras, self.max_casts)
    }
    fn cast_checked(
        &mut self,
        spell: &mut SourceSpell,
        t: &SpellTemplate,
        starts: &mut SourceSpellStarts,
        events: &mut SourceUnitEvents,
    ) -> Result<()> {
        spell.cast_checked(t, self.stores, None, Some((starts, events)), self, |_| {
            Ok(())
        })
    }
}

impl SpellCastEffects for Owner<'_> {
    fn cast_unit(&self) -> &SpellUnitState {
        &self.unit
    }

    fn cast_auras(&mut self) -> &mut SourceUnitAuras {
        self.auras
    }

    fn send_cooldown(&mut self, spell: &mut SourceSpell, t: &SpellTemplate) -> Result<()> {
        self.send_spell_cooldown(spell, t)
    }

    fn send_spell_go(&mut self, spell: &mut SourceSpell, t: &SpellTemplate) -> Result<()> {
        let guid = self
            .objects
            .get(spell.caster)?
            .guid
            .ok_or_else(|| refuse("execution.spells.Spell_3967.GetPackGUID_owner"))?
            .raw();
        spell.spell_go = Some(self_spell_go(spell, t, guid)?);
        spell.go_broadcast = Some(broadcast(
            self.objects,
            spell.caster,
            self.broadcast,
            self.bounding_radius,
        )?);
        Ok(())
    }

    fn remove_cast_auras(
        &mut self,
        t: &SpellTemplate,
        caster: ObjectHandle,
        flag: u32,
    ) -> Result<()> {
        check_cast_aura_removals(self.stores, self.auras, t, caster, flag, &mut 0)
    }

    fn proc_cast_end(&mut self, spell: &mut SourceSpell) -> Result<()> {
        spell.proc_event_lookups += self.proc_self(false)?;
        Ok(())
    }

    fn duration(&self, t: &SpellTemplate) -> Result<i32> {
        let row = self.stores.dbc("SpellDuration.dbc", t.durationindex)?;
        let word = |index| -> Result<i32> {
            row.map_or(Ok(0), |row| {
                row.words
                    .get(&index)
                    .map(|v| *v as i32)
                    .ok_or_else(|| refuse("execution.spells.SpellDuration_layout"))
            })
        };
        let value = source_spell_duration(word(1)?)?;
        if value != -1 && t.attributesex3 & 0x2000_0000 == 0 {
            source_spell_max_duration(word(3)?)?;
        }
        // SpellMgr.cpp:76-101, Object.h:1156. No Player spell-mod owner.
        require(
            self.stores
                .script_binding(SpellScriptKind::Aura, t.id)?
                .is_none(),
            "execution.spells.AuraScript_OnDurationCalculate_owner",
        )?;
        Ok(value)
    }

    fn fill_self_targets(&mut self, s: &mut SourceSpell, t: &SpellTemplate) -> Result<()> {
        let data = self
            .targets
            .get_spell_targeting_data(t.id)
            .map_err(|e| refuse(&format!("execution.spells.targeting.{e}")))?;
        for (i, effect) in t.effects().iter().enumerate() {
            if effect.effect == 0 {
                continue;
            }
            require(
                data.implicit_type[i] == SpellTargetImplicitType::Unit,
                "execution.spells.Spell_550.effect_target_type_owner",
            )?;
            for side in 0..2 {
                let target = [effect.target_a, effect.target_b][side];
                if target == 0 || data.ignored_targets[i][side] {
                    continue;
                }
                let radius = [
                    t.effectradiusindex1,
                    t.effectradiusindex2,
                    t.effectradiusindex3,
                ][i];
                if radius != 0 {
                    self.stores.dbc("SpellRadius.dbc", radius)?;
                } else {
                    self.stores.range_entry(t.rangeindex)?;
                }
                self.stores.cone(self.stores.first_rank(t.id));
                require(
                    t.id != 28241,
                    "execution.spells.Spell_7570.GetSpellAuraHolder_owner",
                )?;
                // Spell.cpp:1785,2159-2161,2397-2405 selects the explicit friendly target.
                match target {
                    1 => {}
                    21 | 57 => {
                        let Some(unit) = s.targets.unit else {
                            continue;
                        };
                        if !unit_can_assist_spell(s.caster, unit, self)? {
                            continue;
                        }
                        require(
                            unit == s.caster,
                            "execution.spells.SetTargetMap_nonself_owner",
                        )?;
                    }
                    _ => return Err(refuse("execution.spells.Spell_1785.SetTargetMap_owner")),
                }
                // CheckTarget, Spell.cpp:6936-7093, self skips type/LOS checks.
                if t.attributesex3 & 0x1000 != 0 && self.auras.vis_flags & 1 == 0 {
                    continue;
                }
                if t.maxtargetlevel != 0 && self.level > t.maxtargetlevel {
                    continue;
                }
                require(
                    t.attributesex3 & 0x8000_0000 == 0,
                    "execution.spells.Spell_7072.IsAOEImmune_owner",
                )?;
                require(
                    !s.script_present,
                    "execution.spells.Spell_7093.OnCheckTarget_owner",
                )?;
                // FilterTargetMap on one target has no truncation or RNG.
                let mask = data.target_mask[i][side] & !(s.partial_application_mask as u8);
                let mut hit = 0;
                for j in 0..3 {
                    if mask & (1 << j) != 0
                        && !self.immunities.immune_to_self_effect(t, j, self.auras)?
                    {
                        hit |= 1 << j;
                    }
                }
                if t.attributesex4 & 0x0000_0800 != 0 && hit != mask {
                    hit = 0;
                }
                if let Some(existing) = s.unique_targets.iter_mut().find(|v| v.unit == s.caster) {
                    existing.effect_mask |= mask;
                    existing.hit_mask |= hit;
                    continue;
                }
                let duration = self.duration(t)?;
                // Unit.cpp:2900-2925. Positive self aura: immunity, no hit roll.
                require(
                    positive_unit_spell(
                        self.stores,
                        t,
                        u32::from(mask),
                        Some((s.caster, s.caster)),
                    )?,
                    "execution.spells.Unit_2927.SpellHitResult_negative_owner",
                )?;
                let missed = self.immunities.immune_to_self_spell(
                    self.stores,
                    t,
                    u32::from(mask),
                    self.object,
                    self.auras,
                )?;
                require(
                    !self.unit.player_controlled && !matches!(t.id, 36924 | 31480),
                    "execution.spells.Spell_1027.diminishing_owner",
                )?;
                s.unique_targets.push(SourceSpellTarget {
                    unit: s.caster,
                    effect_mask: mask,
                    hit_mask: hit,
                    processed_mask: 0,
                    processed: false,
                    duration,
                    missed,
                });
            }
        }
        require(
            t.attributesex2 & 0x0020_0000 == 0 && t.attributesex & 0x0400_0000 == 0,
            "execution.spells.Spell_704.all_target_requirements_owner",
        )?;
        Ok(())
    }

    fn hit_self(
        &mut self,
        s: &mut SourceSpell,
        t: &SpellTemplate,
        nested: Option<(&mut SourceSpellStarts, &mut SourceUnitEvents)>,
    ) -> Result<()> {
        let Some(target) = s.unique_targets.first_mut() else {
            return Ok(());
        };
        if target.processed || target.effect_mask == 0 {
            return Ok(());
        }
        target.processed = true;
        if target.missed {
            return Ok(());
        }
        let mask = target.hit_mask & !target.processed_mask;
        if t.effects()
            .iter()
            .enumerate()
            .any(|(i, effect)| mask & (1 << i) != 0 && matches!(effect.effect, 6 | 27 | 35 | 119))
        {
            // SpellAuras.cpp:5174 and :270 are independent libc time calls.
            let raw = self.random.creature_raw_position()?;
            let clock = self.clocks.aura_holder(self.object.thread, raw)?;
            self.assert_event_ai_clock(s.caster, s.id, &clock, (60478082, 60478084));
            let time = clock.seconds;
            let duration = self.duration(t)?;
            let mut aura_holder = SourceAuraHolder::new(t, self.object, time, 0, duration)?;
            aura_holder.triggered_by = s.triggered_by_aura;
            s.pending_aura = Some(aura_holder);
        }
        for (i, effect) in t.effects().iter().enumerate() {
            if mask & (1 << i) == 0 {
                continue;
            }
            let amount = effect_value(t, i, s.base_points[i])?;
            s.damage_per_effect[i] = amount;
            require(
                effect.effect == 6,
                &format!(
                    "execution.spells.SpellEffects.effect_{}_owner",
                    effect.effect
                ),
            )?;
            let raw = self.random.creature_raw_position()?;
            let clock = self.clocks.aura(self.object.thread, raw)?;
            self.assert_event_ai_clock(s.caster, s.id, &clock, (60478159, 60478161));
            let time = clock.seconds;
            // SpellTargets.cpp:34,54,90 gives these self targets the HELPFUL filter.
            let positive = t.attributes & 0x0400_0000 == 0;
            let period = [t.effectamplitude1, t.effectamplitude2, t.effectamplitude3][i];
            // SpellAuras.cpp:389 multiplies in uint32 before pinned int32 assignment.
            let stack = s
                .pending_aura
                .as_ref()
                .expect("IsSpellAppliesAura includes ApplyAura")
                .stack;
            let aura = SourceAura {
                kind: source_aura_type(effect.aura)?,
                amount: (amount as u32).wrapping_mul(stack) as i32,
                base_points: s.base_points[i],
                misc: [t.effectmiscvalue1, t.effectmiscvalue2, t.effectmiscvalue3][i],
                periodic_time: period,
                periodic_timer: period as i32,
                periodic_tick: 0,
                positive,
                apply_time: time,
                applied: false,
                periodic: false,
                area: false,
                persistent: false,
                magnet_used: false,
                remove_mode: 0,
                script_value: 0,
                affect_overridden: false,
            };
            s.pending_aura
                .as_mut()
                .expect("IsSpellAppliesAura includes ApplyAura")
                .effects[i] = Some(aura);
        }
        if s.pending_aura.is_some() {
            self.add_holder(t, &mut s.pending_aura, nested)?;
        }
        s.proc_event_lookups += self.proc_self(true)?; // Spell.cpp:1329-1331, both sides.
        if let Some(holder) = self.auras.aura_holders.get_mut(&t.id) {
            holder.ready = true;
        }
        Ok(())
    }
}

impl Owner<'_> {
    fn assert_event_ai_clock(
        &self,
        caster: ObjectHandle,
        id: u64,
        clock: &ActiveClock,
        lines: (u64, u64),
    ) {
        if self.object.thread == 6
            && self.objects.get(caster).unwrap().database_guid == 40520
            && id == 3
        {
            // seal16-full-01/run/trace.tsv. Both time inputs follow raw word 27086.
            assert_eq!((clock.site_line, clock.time_line), lines);
            assert_eq!(
                (clock.thread, clock.raw, clock.seconds),
                (6, 27087, 1794314190)
            );
        }
    }

    fn add_holder(
        &mut self,
        t: &SpellTemplate,
        pending: &mut Option<SourceAuraHolder>,
        mut nested: Option<(&mut SourceSpellStarts, &mut SourceUnitEvents)>,
    ) -> Result<()> {
        let holder = pending.as_mut().expect("pending aura");
        if let Some(old) = self.auras.aura_holders.get(&t.id) {
            require(
                !holder.passive,
                "execution.spells.Unit_4708.passive_duplicate_holder_owner",
            )?;
            require(
                old.caster == holder.caster,
                "execution.spells.Unit_4763.different_caster_stacker_owner",
            )?;
            require(
                t.stackamount == 0,
                "execution.spells.Unit_4722.ModStackAmount_owner",
            )?;
            // SpellAuras.cpp:5513-5515; the creature self cast has no item.
            require(
                t.id != 12654,
                "execution.spells.Unit_4733.rolling_ignite_owner",
            )?;
            return Err(refuse(
                "execution.spells.Unit_4757.RemoveSpellAuraHolder_owner",
            ));
        }
        // Unit.cpp:4937-4971. Existing passive ranks skip before stacker calls.
        self.stores.stack(t.id)?;
        for (&id, existing) in &self.auras.aura_holders {
            self.stores.stack(id)?;
            let old = self
                .stores
                .template(id)
                .ok_or_else(|| refuse("execution.spells.missing_template"))?;
            if old.attributes & 0x40 != 0
                && (id != t.id || t.attributes & 0x10 == 0)
                && (id == t.id || self.stores.first_rank(id) != self.stores.first_rank(t.id))
            {
                continue;
            }
            // Unit.cpp:4974-4977 lets a triggering aura coexist with its triggered boost.
            if holder.triggered_by == Some(id) || existing.triggered_by == Some(t.id) {
                continue;
            }
            require(
                existing.caster == holder.caster,
                "execution.spells.Unit_5000.different_caster_stacker_owner",
            )?;
            spell_stackable(Some(t), Some(old))?;
            return Err(refuse(
                "execution.spells.Unit_5003.aura_stack_resolution_owner",
            ));
        }
        require(
            !holder.tracked,
            "execution.spells.Unit_4793.GetTrackedAuraTargets_owner",
        )?;
        // SpellAuras.cpp:5231-5283,5921-5974. Creature group updates and
        // duration packets return without sending, Unit.cpp:10501-10523.
        holder.slot = 255;
        if !holder.passive {
            let positive = holder.effects.iter().flatten().all(|a| a.positive);
            let range = if positive { 0..32 } else { 32..48 };
            if let Some(slot) = range
                .into_iter()
                .find(|&i| self.auras.aura_fields.spells[i] == 0)
            {
                holder.slot = slot as u8;
                let fields = &mut self.auras.aura_fields;
                fields.spells[slot] = t.id;
                let shift = (slot & 7) * 4;
                let flags = if positive {
                    4 | u32::from(t.attributes & 0x8000_0000 == 0)
                } else {
                    8
                };
                fields.flags[slot >> 3] =
                    (fields.flags[slot >> 3] & !(15 << shift)) | flags << shift;
                let shift = (slot % 4) * 8;
                fields.levels[slot / 4] = (fields.levels[slot / 4] & !(255 << shift))
                    | self.level.wrapping_shl(shift as u32);
                let count = if holder.charges > 0 {
                    holder.charges.wrapping_mul(holder.stack)
                } else {
                    holder.stack
                };
                let count = if count <= 255 {
                    count.wrapping_sub(1) as u8
                } else {
                    254
                };
                fields.applications[slot / 4] =
                    (fields.applications[slot / 4] & !(255 << shift)) | u32::from(count) << shift;
            }
        }
        let seated = matches!(self.stand, 1 | 2 | 4 | 5 | 6);
        require(
            t.aurainterruptflags & 0x0004_0000 == 0 || seated,
            "execution.spells.SpellAuraHolder_5292.SetStandState_owner",
        )?;
        require(
            t.attributesex & 0x0004_0000 == 0,
            "execution.spells.SpellAuraHolder_5295.SetFlag_owner",
        )?;
        require(
            t.spellfamilyname != 10 || t.spellfamilyflags & 0x0a00_0200 == 0,
            "execution.spells.SpellAuraHolder_5303.ModifyAuraState_owner",
        )?;
        holder.skip_update = true;
        self.auras
            .aura_holders
            .insert(t.id, pending.take().expect("pending aura"));
        for (i, aura) in self.auras.aura_holders[&t.id].effects.iter().enumerate() {
            if let Some(aura) = aura {
                if aura.kind < 192 {
                    self.auras
                        .aura_types
                        .entry(aura.kind)
                        .or_default()
                        .push((u64::from(t.id) << 2) | i as u64);
                }
            }
        }
        for (i, effect) in t.effects().iter().enumerate() {
            if self.auras.aura_holders[&t.id].effects[i].is_some() {
                // Aura::OnApply/OnAfterApply see the holder script already bound.
                match effect.aura {
                    192..=255 => {}
                    95 => self.auras.vis_flags |= 1, // SpellAuras.cpp:4401-4429; creature.
                    10 => {
                        let aura = self.auras.aura_holders[&t.id].effects[i].as_ref().unwrap();
                        if self.objects.get(self.object)?.alive {
                            require(
                                !matches!(t.id, 26400 | 28862),
                                "execution.spells.Aura_2839.mod_threat_level_scaling_owner",
                            )?;
                            // SpellAuras.cpp:2811-2843; Util.h:156-160.
                            let amount = if aura.amount == -100 {
                                -99.99
                            } else {
                                aura.amount as f32
                            };
                            let factor = (100.0 + amount) / 100.0;
                            for (school, modifier) in self.threat_modifier.iter_mut().enumerate() {
                                if aura.misc & (1 << school) != 0 {
                                    *modifier *= factor;
                                }
                            }
                        }
                    }
                    36 => self.apply_shape(t.id, i, nested.as_mut())?,
                    23 => {
                        // SpellAuras.cpp:3097-3117 sets periodic and skips removal-only casts.
                        self.auras.aura_holders.get_mut(&t.id).unwrap().effects[i]
                            .as_mut()
                            .unwrap()
                            .periodic = true;
                    }
                    65 => {
                        let aura = self.auras.aura_holders[&t.id].effects[i].as_ref().unwrap();
                        return Err(refuse(if aura.amount > 0 {
                            "execution.spells.Unit_10496.ApplyPercentModFloatValue_owner"
                        } else {
                            "execution.spells.Unit_10498.ApplyPercentModFloatValue_owner"
                        }));
                    }
                    4 => {
                        // SpellAuras.cpp:1231-1629. Specific handlers retain
                        // their named refusal; default branches reach PetAura.
                        let specific = match t.spellfamilyname {
                            0 => matches!(
                                t.id,
                                6946 | 41170
                                    | 7057
                                    | 7054
                                    | 13139
                                    | 21094
                                    | 23487
                                    | 23183
                                    | 25042
                                    | 25471
                                    | 26681
                                    | 26682
                                    | 6606
                                    | 16093
                                    | 24658
                                    | 24661
                                    | 28819
                                    | 29266
                            ),
                            4 => t.id == 23427,
                            6 => t.id == 30238,
                            10 => matches!(t.id, 21082 | 20162 | 20305 | 20306 | 20307 | 20308),
                            _ => false,
                        };
                        require(
                            !specific,
                            "execution.spells.Aura_1231.HandleAuraDummy_specific_owner",
                        )?;
                        require(
                            self.stores.pet_aura(t.id).is_none(),
                            "execution.spells.Aura_1583.AddPetAura_owner",
                        )?;
                    }
                    _ => {
                        return Err(refuse(&format!(
                            "execution.spells.AuraHandler.type_{}_owner",
                            effect.aura
                        )))
                    }
                }
                self.auras.aura_holders.get_mut(&t.id).unwrap().effects[i]
                    .as_mut()
                    .unwrap()
                    .applied = true;
            }
        }
        let boost = match t.spellfamilyname {
            0 => t.id == 20594,
            3 => t.mechanic == 17,
            9 => t.id == 19574,
            _ => false,
        };
        require(
            !boost,
            "execution.spells.SpellAuraHolder_5648.boost_CastSpell_owner",
        )?;
        let flags = self
            .stores
            .proc_event(t.id)
            .map_or(t.procflags, |v| v.flags | t.procflags);
        if flags & 1 != 0 {
            self.auras.heartbeat_proc_counter = self.auras.heartbeat_proc_counter.wrapping_add(1);
        }
        Ok(())
    }

    fn apply_shape(
        &mut self,
        spell: u32,
        effect: usize,
        nested: Option<&mut (&mut SourceSpellStarts, &mut SourceUnitEvents)>,
    ) -> Result<()> {
        let misc = self.auras.aura_holders[&spell].effects[effect]
            .as_ref()
            .unwrap()
            .misc;
        let form = source_shape_form(misc)?;
        let Some(_) = self.stores.dbc("SpellShapeshiftForm.dbc", form)? else {
            return Ok(()); // SpellAuras.cpp:1716-1723.
        };
        let boost = match form {
            0x11 => 21156,
            0x12 => 7376,
            0x13 => 7381,
            _ => return Err(refuse("execution.spells.Aura_1729.shapeshift_form_owner")),
        };
        // SpellAuras.cpp:1792-1796,1861-1885; Unit.cpp:820-834 excludes this holder.
        require(
            self.auras.aura_types[&36]
                .iter()
                .all(|id| (*id >> 2) == u64::from(spell)),
            "execution.spells.Unit_831.RemoveAurasDueToSpell_owner",
        )?;
        let power_type = self
            .power_type
            .as_deref_mut()
            .ok_or_else(|| refuse("execution.spells.Aura_1885.GetPowerType_owner"))?;
        if *power_type != 1 {
            *power_type = 1;
            if let Some(vitals) = self.vitals {
                vitals.has_mana = false;
            }
            self.require_no_power_group("execution.spells.Unit_6184.GetGroup_owner")?;
            require(
                self.objects.get(self.object)?.kind == SourceObjectKind::Creature,
                "execution.spells.Unit_6191.GetCreatePowers_owner",
            )?;
        }
        require(
            *self.form != 0x12 || !self.auras.has_aura_type(112),
            "execution.spells.Aura_1884.GetOverrideScript_owner",
        )?;
        require(
            !self.unit.player,
            "execution.spells.Aura_1890.tactical_mastery_owner",
        )?;
        let rage = &mut self
            .power_fields
            .as_mut()
            .and_then(|power| power.auxiliary_current.as_mut())
            .ok_or_else(|| refuse("execution.spells.Aura_1911.GetPower_rage_owner"))?[0];
        if *rage != 0 {
            *rage = 0;
            self.require_no_power_group("execution.spells.Unit_9294.GetGroup_owner")?;
        }
        // SpellAuras.cpp:1920-1936; Unit.cpp:5211-5223.
        *self.form = form;
        for &id in self.auras.aura_holders.keys() {
            let aura = self
                .stores
                .template(id)
                .ok_or_else(|| refuse("execution.spells.missing_template"))?;
            require(
                aura.aurainterruptflags & 0x8000 == 0,
                "execution.spells.Unit_5217.RemoveSpellAuraHolder_owner",
            )?;
        }
        // SpellAuras.cpp:4263-4271,4291-4295. The boost starts before this holder is ready.
        let (starts, queue) =
            nested.ok_or_else(|| refuse("execution.spells.Aura_4291.CastSpell_owner"))?;
        let caster = self.object;
        SpellStartContext {
            stores: self.stores,
            starts,
            events: queue,
            owner: self,
        }
        .cast_triggered(caster, None, boost, Some(spell))?;
        Ok(())
    }

    fn require_no_power_group(&self, refusal: &str) -> Result<()> {
        require(!self.unit.player, refusal)?;
        if self.control_guids.has_charmer() {
            if let Some(unit) = self
                .control_guids
                .get_charmer(self.objects, self.object, None, unbound_pet_loading)?
                .unit()
            {
                require(
                    self.objects.get(unit)?.kind != SourceObjectKind::Player,
                    refusal,
                )?;
            }
        }
        Ok(())
    }

    fn proc_self(&mut self, victim: bool) -> Result<u32> {
        // UnitAuraProcHandler.cpp:277-340,343-388. Helpful spell flags do
        // not intersect MELEE_BASED_TRIGGER_MASK. Creature has no item procs.
        let current = !self.auras.spell_procs_happening;
        self.auras.spell_procs_happening = true;
        let mut lookups = 0;
        for _ in 0..if victim { 2 } else { 1 } {
            for (&id, holder) in &self.auras.aura_holders {
                // :415-420 skips the just-created holder until Spell.cpp:1358.
                if !holder.ready || holder.deleted {
                    continue;
                }
                let t = self
                    .stores
                    .template(id)
                    .ok_or_else(|| refuse("execution.spells.missing_template"))?;
                let event = self.stores.proc_event(id);
                let flags = event
                    .filter(|v| v.flags != 0)
                    .map_or(t.procflags, |v| v.flags);
                lookups += 1;
                // :518-529 rejects before a chance or handler when flags are zero.
                require(
                    flags == 0,
                    "execution.spells.UnitAuraProcHandler_532.IsSpellProcEventCanTriggeredBy_owner",
                )?;
            }
        }
        if current {
            self.auras.spell_procs_happening = false;
        }
        Ok(lookups)
    }
}
// SpellDefines.h:454-475 has an unfixed enum with enumerators 0 through 32.
fn source_shape_form(misc: i32) -> Result<u32> {
    require(
        (0..=63).contains(&misc),
        "execution.spells.Aura_1716.shapeshift_enum_undefined",
    )?;
    Ok(misc as u32)
}

#[test]
fn shapeshift_enum_refuses_values_outside_the_source_range() {
    assert_eq!(source_shape_form(0).unwrap(), 0);
    assert_eq!(source_shape_form(17).unwrap(), 17);
    assert_eq!(source_shape_form(63).unwrap(), 63);
    for misc in [-1, 64] {
        assert_eq!(
            source_shape_form(misc).unwrap_err().id,
            "execution.spells.Aura_1716.shapeshift_enum_undefined"
        );
    }
}

fn effect_value(t: &SpellTemplate, i: usize, current: i32) -> Result<i32> {
    // Object.cpp:2923-2965. Port the fixed dice path, without synthetic RNG.
    let dice = [t.effectbasedice1, t.effectbasedice2, t.effectbasedice3][i] as i32;
    let base = current
        .checked_sub(dice)
        .ok_or_else(|| refuse("execution.spells.Object_2927.base_points_sub_undefined"))?;
    require(
        [
            t.effectrealpointsperlevel1,
            t.effectrealpointsperlevel2,
            t.effectrealpointsperlevel3,
        ][i]
            .0
            == 0,
        "execution.spells.Object_2934.effect_level_scaling_owner",
    )?;
    require(
        matches!(
            [t.effectdiesides1, t.effectdiesides2, t.effectdiesides3][i],
            0 | 1
        ),
        "execution.spells.Object_2955.effect_dice_rng_owner",
    )?;
    let value = Single::from_i128(i128::from(base))
        .value
        .add_r(
            Single::from_i128(i128::from(dice)).value,
            Round::NearestTiesToEven,
        )
        .value;
    require(
        t.attributes & 0x0008_0000 == 0
            || t.spelllevel == 0
            || !matches!(t.effects()[i].aura, 3 | 53 | 69 | 162 | 64),
        "execution.spells.Object_3014.GetCreatureClassLvlStats_owner",
    )?;
    let converted = value.to_i128_r(32, Round::TowardZero, &mut false);
    require(
        !converted.status.contains(Status::INVALID_OP),
        "execution.spells.Object_3024.effect_float_to_int_undefined",
    )?;
    Ok(converted.value as i32)
}

#[test]
fn fixed_effect_value_checks_signed_subtraction_before_float_conversion() {
    let t = SpellTemplate {
        effectbasedice1: 1,
        effectdiesides1: 1,
        ..Default::default()
    };
    assert_eq!(effect_value(&t, 0, i32::MIN + 1).unwrap(), i32::MIN);
    assert_eq!(
        effect_value(&t, 0, i32::MIN).unwrap_err().id,
        "execution.spells.Object_2927.base_points_sub_undefined"
    );
    assert_eq!(effect_value(&t, 0, 2_147_483_520).unwrap(), 2_147_483_520);
    assert_eq!(
        effect_value(&t, 0, i32::MAX).unwrap_err().id,
        "execution.spells.Object_3024.effect_float_to_int_undefined"
    );
}

pub(in super::super) fn spell_start_packet(
    s: &SourceSpell,
    t: &SpellTemplate,
    objects: &SourceObjects,
) -> Result<Option<SourceSpellPacket>> {
    // Spell.cpp:3904,7096-7102. This includes ordinary instant casts.
    if s.flags.channel_only()
        || !(t.spellvisual != 0
            || t.attributesex & 0x44 != 0
            || f32::from_bits(t.speed.0) > 0.
            || (!s.flags.old_triggered() && s.triggered_by_aura.is_none()))
    {
        return Ok(None);
    }
    let hidden = s.triggered_by_aura.is_some() || s.flags.hide_log(s.auto_repeat);
    let ranged = t.attributes & 2 != 0;
    require(
        !s.item_cast,
        "execution.spells.Spell_3918.cast_item_guid_owner",
    )?;
    let caster = objects.get(s.caster)?;
    let guid = caster
        .guid
        .ok_or_else(|| refuse("execution.spells.Spell_3920.GetPackGUID_owner"))?
        .raw();
    let mut body = SourceSpellPacket {
        bytes: [0; 53],
        length: 0,
    };
    packed(&mut body, guid);
    packed(
        &mut body,
        if caster.kind == SourceObjectKind::GameObject {
            0
        } else {
            guid
        },
    );
    append(&mut body, &t.id.to_le_bytes());
    let flags = 2u16 | u16::from(hidden) | if ranged { 0x20 } else { 0 };
    append(&mut body, &flags.to_le_bytes());
    append(&mut body, &s.timer.to_le_bytes());
    require(
        matches!(s.targets.mask, 0 | 2),
        "execution.spells.Spell_3931.spell_targets_packet_owner",
    )?;
    append(&mut body, &(s.targets.mask as u16).to_le_bytes());
    if s.targets.mask == 2 {
        let target = match s.targets.unit {
            Some(unit) => objects
                .get(unit)?
                .guid
                .ok_or_else(|| refuse("execution.spells.Spell_292.target_pack_guid_owner"))?
                .raw(),
            None => 0,
        };
        packed(&mut body, target);
    }
    require(
        !ranged,
        "execution.spells.Spell_3934.WriteAmmoToPacket_owner",
    )?;
    Ok(Some(body))
}

fn append(body: &mut SourceSpellPacket, bytes: &[u8]) {
    body.bytes[body.length..body.length + bytes.len()].copy_from_slice(bytes);
    body.length += bytes.len();
}

fn packed(body: &mut SourceSpellPacket, guid: u64) {
    let bytes = guid.to_le_bytes();
    let mask = bytes
        .iter()
        .enumerate()
        .fold(0u8, |m, (i, b)| m | (u8::from(*b != 0) << i));
    append(body, &[mask]);
    for b in bytes {
        if b != 0 {
            append(body, &[b]);
        }
    }
}

#[test]
fn spell_start_packet_uses_live_guids_and_the_normal_cast_send_branch() {
    let mut objects = SourceObjects::default();
    let unit = objects.construct(6, SourceObjectKind::Creature).unwrap();
    objects
        .assign_guid(unit, 10, SourceGuid::without_entry(HighGuid::Player, 1))
        .unwrap();
    let t = SpellTemplate {
        id: 10848,
        ..Default::default()
    };
    let mut s = SourceSpell::new(1, &t, unit, SpellTriggerFlags(0x100), 0).unwrap();
    s.timer = 42;
    s.targets.mask = 2;
    s.targets.unit = Some(unit);
    let start = spell_start_packet(&s, &t, &objects).unwrap().unwrap();
    assert_eq!(
        &start.bytes[..start.length],
        &[1, 1, 1, 1, 96, 42, 0, 0, 2, 0, 42, 0, 0, 0, 2, 0, 1, 1],
    );
    s.flags = SpellTriggerFlags(1);
    assert!(spell_start_packet(&s, &t, &objects).unwrap().is_none());
    s.flags = SpellTriggerFlags(0x8000);
    assert!(spell_start_packet(&s, &t, &objects).unwrap().is_none());
}

fn self_spell_go(s: &mut SourceSpell, t: &SpellTemplate, guid: u64) -> Result<SourceSpellPacket> {
    require(
        s.targets.mask == 2
            && s.targets.unit == Some(s.caster)
            && s.unique_targets.len() <= 1
            && t.attributes & 2 == 0,
        "execution.spells.Spell_3976.nonself_packet_owner",
    )?;
    let mut body = SourceSpellPacket {
        bytes: [0; 53],
        length: 0,
    };
    packed(&mut body, guid);
    packed(&mut body, guid);
    append(&mut body, &t.id.to_le_bytes());
    let hidden = s.triggered_by_aura.is_some() || s.flags.hide_log(s.auto_repeat);
    // Spell.h:48,56; Spell.cpp:471,3947-3949.
    append(&mut body, &(0x100u16 | u16::from(hidden)).to_le_bytes());
    let mut miss = None;
    if let Some(target) = s.unique_targets.first_mut() {
        if target.hit_mask == 0 && target.effect_mask != 0 {
            target.missed = true;
            miss = Some(8);
        } else if target.missed {
            miss = Some(7);
        }
        append(&mut body, &[u8::from(miss.is_none())]);
        if miss.is_none() {
            append(&mut body, &guid.to_le_bytes());
        }
    } else {
        append(&mut body, &[0]);
    }
    append(&mut body, &[u8::from(miss.is_some())]);
    if let Some(miss) = miss {
        append(&mut body, &guid.to_le_bytes());
        append(&mut body, &[miss]);
    }
    append(&mut body, &2u16.to_le_bytes());
    packed(&mut body, guid);
    Ok(body)
}

#[test]
fn self_spell_go_encodes_full_and_packed_guids_without_broadcasting() {
    // A one-byte authored GUID makes the wire layout independent of packing.
    let t = SpellTemplate {
        id: 9036,
        ..Default::default()
    };
    let unit = ObjectHandle {
        thread: 6,
        construction: 1,
    };
    let mut s = SourceSpell::new(1, &t, unit, SpellTriggerFlags(1), 0).unwrap();
    s.targets.unit = Some(unit);
    s.targets.mask = 2;
    s.unique_targets.push(SourceSpellTarget {
        unit,
        effect_mask: 1,
        hit_mask: 1,
        processed_mask: 0,
        processed: false,
        duration: -1,
        missed: false,
    });
    let body = self_spell_go(&mut s, &t, 1).unwrap();
    assert_eq!(
        &body.bytes[..body.length],
        &[1, 1, 1, 1, 76, 35, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 1, 1]
    );
    s.flags = SpellTriggerFlags(0x100);
    let normal = self_spell_go(&mut s, &t, 1).unwrap();
    assert_eq!(
        &normal.bytes[..normal.length],
        &[1, 1, 1, 1, 76, 35, 0, 0, 0, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 1, 1]
    );
    s.flags = SpellTriggerFlags(1);
    s.unique_targets[0].hit_mask = 0;
    let body = self_spell_go(&mut s, &t, 1).unwrap();
    assert!(s.unique_targets[0].missed);
    assert_eq!(
        &body.bytes[..body.length],
        &[1, 1, 1, 1, 76, 35, 0, 0, 1, 1, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 8, 2, 0, 1, 1]
    );
}
