// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Spell.cpp:3160-3370,533-1043,1163-1502. Triggered generic creature self casts.
use super::*;

impl Owner<'_> {
    pub(super) fn cast_self(&mut self, s: &mut SourceSpell, skip: bool) -> Result<()> {
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
        s.executing = true;
        // Object.cpp:3091-3099. A refused continuation retains the live depth.
        if self.max_casts != 0 && self.auras.cast_counter >= self.max_casts {
            s.finish_failed(self)?;
            s.executing = false;
            return Ok(());
        }
        self.auras.cast_counter = self.auras.cast_counter.wrapping_add(1);
        self.objects.get(s.caster)?;
        require(
            s.targets.unit == Some(s.caster) && s.original_caster == s.caster,
            "execution.spells.Spell_3180.UpdatePointers_nonself_owner",
        )?;
        let t = self
            .stores
            .template(s.spell)
            .ok_or_else(|| refuse("execution.spells.missing_template"))?;
        if !skip && s.check_cast(t, false, self)? != SpellCastResult::Ok {
            return Err(refuse("execution.spells.Spell_3206.StopCast_owner"));
        }
        let precast = match t.spellfamilyname {
            0 => matches!(t.mechanic, 16 | 25),
            6 => t.spellfamilyflags & 1 != 0,
            10 => t.mechanic == 25 && t.id != 25771,
            _ => false,
        };
        require(
            !precast,
            "execution.spells.Spell_3218.AddPrecastSpell_owner",
        )?;
        require(
            t.spellfamilyname != 6
                || !matches!(t.id, 15237 | 15430 | 15431 | 27799 | 27800 | 27801 | 25331),
            "execution.spells.Spell_3240.AddTriggeredSpell_owner",
        )?;
        // updateTradeSlotItem has no item target, Spell.h:158-165.
        s.duration = self.duration(t)?;
        self.fill_self_targets(s, t)?;
        // Spell.cpp:3510-3516; Object.cpp:2632-2635 skips zero-duration writes.
        self.cooldowns.send_spell_cooldown(
            t,
            s.flags.channel_only(),
            || Ok(self.world_clock.current_ms()),
            |category| {
                Ok(self
                    .stores
                    .dbc("SpellCategory.dbc", category)?
                    .map(|row| row.words[&1]))
            },
        )?;
        require(
            !self.unit.has_ai,
            "execution.spells.Spell_3275.OnSpellCooldownAdded_owner",
        )?;
        // TakePower calls ModifyPower(0), whose first operation returns for zero.
        require(
            s.power_cost == 0 && t.powertype < 5,
            "execution.spells.Spell_4399.ModifyPower_owner",
        )?;
        // TakeReagents, TakeAmmo and SendCastResult return for a non-player.
        require(
            !self.unit.player && !self.unit.gameobject,
            "execution.spells.Spell_4439.player_cast_owner",
        )?;
        require(
            t.attributesex & 0x44 == 0,
            "execution.spells.Spell_3942.channel_spell_go_owner",
        )?;
        if t.spellvisual != 0 || f32::from_bits(t.speed.0) > 0. {
            let guid = self
                .objects
                .get(s.caster)?
                .guid
                .ok_or_else(|| refuse("execution.spells.Spell_3967.GetPackGUID_owner"))?
                .raw();
            s.spell_go = Some(self_spell_go(s, t, guid)?);
            // Object.cpp:1916-1921, before AddToWorld there is no broadcast.
            require(
                !self.objects.get(s.caster)?.in_world,
                "execution.spells.WorldObject_1920.MessageBroadcast_owner",
            )?;
        }
        for (i, effect) in t.effects().iter().enumerate() {
            if effect.effect != 0 {
                s.damage_multipliers[i] = 1f32.to_bits();
            }
        }
        require(
            !s.script_present,
            "execution.spells.Spell_8059.OnCast_owner",
        )?;
        require(
            s.flags.old_triggered(),
            "execution.spells.Spell_3296.RemoveAurasOnCast_owner",
        )?;
        s.state = SourceSpellState::Landing;
        if let Some(threat) = self.stores.threat(t.id) {
            require(
                threat.threat == 0 && threat.ap_bonus.0 == 0,
                "execution.spells.Spell_4497.HandleThreatSpells_owner",
            )?;
        }
        // No targetless, destination, item or launch-handled effects in this fill.
        // Spell.cpp:1612-1652 computes no damage or healing and rolls no crit.
        for target in &mut s.unique_targets {
            target.processed_mask = 0;
        }
        self.hit_self(s, t)?;
        require(
            !self.auras.has_aura_type(109),
            "execution.spells.Spell_7442.Aura_isAffectedOnSpell_owner",
        )?;
        s.proc_event_lookups += self.proc_self(false)?; // Spell.cpp:3357, cast end.
                                                        // handle_immediate revisits processed targets, then finish(true).
        require(
            t.attributes & 0x0010_0000 == 0,
            "execution.spells.Spell_3793.AttackStop_owner",
        )?;
        s.state = SourceSpellState::Finished;
        if self.auras.cast_counter != 0 {
            self.auras.cast_counter -= 1;
        }
        s.executing = false;
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

    fn hit_self(&mut self, s: &mut SourceSpell, t: &SpellTemplate) -> Result<()> {
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
        let mut holder = None;
        if t.effects()
            .iter()
            .enumerate()
            .any(|(i, effect)| mask & (1 << i) != 0 && matches!(effect.effect, 6 | 27 | 35 | 119))
        {
            // SpellAuras.cpp:5174 and :270 are independent libc time calls.
            let time = self
                .clocks
                .aura_holder(self.object.thread, self.raw)?
                .seconds;
            let duration = self.duration(t)?;
            holder = Some(SourceAuraHolder::new(t, self.object, time, 0, duration)?);
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
            let time = self.clocks.aura(self.object.thread, self.raw)?.seconds;
            // SpellTargets.cpp:34,54,90 gives these self targets the HELPFUL filter.
            let positive = t.attributes & 0x0400_0000 == 0;
            let period = [t.effectamplitude1, t.effectamplitude2, t.effectamplitude3][i];
            // SpellAuras.cpp:389 multiplies in uint32 before pinned int32 assignment.
            let stack = holder
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
            holder
                .as_mut()
                .expect("IsSpellAppliesAura includes ApplyAura")
                .effects[i] = Some(aura);
        }
        if let Some(holder) = holder {
            self.add_holder(t, holder)?;
        }
        s.proc_event_lookups += self.proc_self(true)?; // Spell.cpp:1329-1331, both sides.
        if let Some(holder) = self.auras.aura_holders.get_mut(&t.id) {
            holder.ready = true;
        }
        Ok(())
    }

    fn add_holder(&mut self, t: &SpellTemplate, mut holder: SourceAuraHolder) -> Result<()> {
        require(
            !self.auras.has_aura(t.id),
            "execution.spells.Unit_4719.same_spell_stack_owner",
        )?;
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
        let seated = matches!(self.addon.stand, 1 | 2 | 4 | 5 | 6);
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
        self.auras.aura_holders.insert(t.id, holder);
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

fn self_spell_go(s: &mut SourceSpell, t: &SpellTemplate, guid: u64) -> Result<SourceSpellGo> {
    require(
        s.targets.mask == 2
            && s.targets.unit == Some(s.caster)
            && s.unique_targets.len() <= 1
            && t.attributes & 2 == 0,
        "execution.spells.Spell_3976.nonself_packet_owner",
    )?;
    let mut body = SourceSpellGo {
        bytes: [0; 53],
        length: 0,
    };
    fn append(body: &mut SourceSpellGo, bytes: &[u8]) {
        body.bytes[body.length..body.length + bytes.len()].copy_from_slice(bytes);
        body.length += bytes.len();
    }
    fn packed(body: &mut SourceSpellGo, guid: u64) {
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
    packed(&mut body, guid);
    packed(&mut body, guid);
    append(&mut body, &t.id.to_le_bytes());
    append(&mut body, &0x101u16.to_le_bytes()); // Spell.h:48,56; Spell.cpp:471.
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
    s.unique_targets[0].hit_mask = 0;
    let body = self_spell_go(&mut s, &t, 1).unwrap();
    assert!(s.unique_targets[0].missed);
    assert_eq!(
        &body.bytes[..body.length],
        &[1, 1, 1, 1, 76, 35, 0, 0, 1, 1, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 8, 2, 0, 1, 1]
    );
}
