// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! SpellAuras.cpp:257-290,5155-5212. State belongs to the target Unit.
use super::super::relations::unbound_pet_loading;
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceAura {
    pub kind: u32,
    pub amount: i32,
    pub base_points: i32,
    pub misc: i32,
    pub periodic_time: u32,
    pub periodic_timer: i32,
    pub periodic_tick: u32,
    pub positive: bool,
    pub apply_time: i64,
    pub applied: bool,
    pub periodic: bool,
    pub area: bool,
    pub persistent: bool,
    pub magnet_used: bool,
    pub remove_mode: u32,
    pub script_value: u64,
    pub affect_overridden: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceAuraHolder {
    pub caster: ObjectHandle,
    pub target: ObjectHandle,
    pub triggered_by: Option<u32>,
    pub apply_time: i64,
    pub apply_ms: u32,
    pub duration: i32,
    pub max_duration: i32,
    pub permanent: bool,
    pub passive: bool,
    pub death_persistent: bool,
    pub removed_on_shape_lost: bool,
    pub charges: u32,
    pub stack: u32,
    pub slot: u8,
    pub level: u32,
    pub ready: bool,
    pub deleted: bool,
    pub skip_update: bool,
    pub tracked: bool,
    pub time_cla: i32,
    pub remove_mode: u32,
    pub diminish_group: u32,
    pub heartbeat_chance: u32,
    pub heartbeat_timer: i32,
    pub heartbeat_interval: i32,
    pub proc_cooldown_ms: i64,
    pub effects: [Option<SourceAura>; 3],
}
impl SourceAuraHolder {
    pub fn new(
        t: &SpellTemplate,
        target: ObjectHandle,
        time: i64,
        ms: u32,
        duration: i32,
    ) -> Result<Self> {
        let passive = t.attributes & 0x40 != 0;
        let tracked = !checks::area_spell(t)?
            && (match t.mechanic {
                5 | 23 => true,
                7 | 10 | 14 | 17 | 18 | 20 => t.spellfamilyname != 0 && t.spellfamilyflags != 0,
                _ => false,
            } || (t.spellfamilyname == 9
                && t.effects().iter().any(|effect| effect.aura == 68)));
        Ok(Self {
            caster: target,
            target,
            triggered_by: None,
            apply_time: time,
            apply_ms: ms,
            duration,
            max_duration: duration,
            permanent: duration == -1 || (passive && t.durationindex == 0),
            passive,
            death_persistent: t.attributesex3 & 0x0010_0000 != 0,
            removed_on_shape_lost: t.attributes & 0x0400_0000 == 0
                && ((t.stances != 0
                    && (matches!(t.id, 11327 | 11329)
                        || (t.attributesex2 & 0x0008_0000 == 0
                            && t.attributes & 0x0001_0000 == 0)))
                    || (t.stances == 0 && t.spellfamilyname == 7 && t.effectapplyauraname1 == 49)),
            charges: t.proccharges,
            stack: match t.id {
                24575 | 24659 | 24662 | 26464 => t.stackamount,
                _ => 1,
            },
            slot: 48,
            level: 1,
            ready: false,
            deleted: false,
            skip_update: false,
            tracked,
            time_cla: 1000,
            remove_mode: 0,
            diminish_group: 0,
            heartbeat_chance: 0,
            heartbeat_timer: 0,
            heartbeat_interval: 0,
            proc_cooldown_ms: 0,
            effects: [None, None, None],
        })
    }
}

/// SpellMgr.cpp:56-64. abs(INT_MIN) is undefined in the pinned C++.
pub fn source_spell_duration(value: i32) -> Result<i32> {
    if value == -1 {
        return Ok(-1);
    }
    value
        .checked_abs()
        .ok_or_else(|| refuse("execution.spells.SpellMgr_63.duration_abs_undefined"))
}

/// SpellMgr.cpp:66-74, maximum duration has its own abs call.
pub fn source_spell_max_duration(value: i32) -> Result<i32> {
    if value == -1 {
        return Ok(-1);
    }
    value
        .checked_abs()
        .ok_or_else(|| refuse("execution.spells.SpellMgr_73.max_duration_abs_undefined"))
}

#[test]
fn duration_absolute_value_preserves_permanent_and_refuses_int_min() {
    assert_eq!(source_spell_duration(-1).unwrap(), -1);
    assert_eq!(source_spell_duration(i32::MIN + 1).unwrap(), i32::MAX);
    assert_eq!(source_spell_duration(i32::MAX).unwrap(), i32::MAX);
    assert_eq!(
        source_spell_duration(i32::MIN).unwrap_err().id,
        "execution.spells.SpellMgr_63.duration_abs_undefined"
    );
    assert_eq!(source_spell_max_duration(-1).unwrap(), -1);
    assert_eq!(source_spell_max_duration(i32::MIN + 1).unwrap(), i32::MAX);
    assert_eq!(source_spell_max_duration(i32::MAX).unwrap(), i32::MAX);
    assert_eq!(
        source_spell_max_duration(i32::MIN).unwrap_err().id,
        "execution.spells.SpellMgr_73.max_duration_abs_undefined"
    );
}

/// Unit.cpp:330-366 zero-initializes these update fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceAuraFields {
    pub spells: [u32; 48],
    pub flags: [u32; 6],
    pub levels: [u32; 12],
    pub applications: [u32; 12],
}
impl Default for SourceAuraFields {
    fn default() -> Self {
        Self {
            spells: [0; 48],
            flags: [0; 6],
            levels: [0; 12],
            applications: [0; 12],
        }
    }
}

/// SpellAuras.cpp:280; the unscoped AuraType enum has the range 0..255.
pub fn source_aura_type(value: u32) -> Result<u32> {
    require(
        value <= 255,
        "execution.spells.SpellAuras_280.AuraType_enum_undefined",
    )?;
    Ok(value)
}

#[test]
fn aura_type_cast_checks_both_sides_of_the_nonfixed_enum_boundary() {
    assert_eq!(source_aura_type(191).unwrap(), 191);
    assert_eq!(source_aura_type(192).unwrap(), 192);
    assert_eq!(source_aura_type(255).unwrap(), 255);
    assert_eq!(
        source_aura_type(256).unwrap_err().id,
        "execution.spells.SpellAuras_280.AuraType_enum_undefined"
    );
}

// Unit.cpp:5253-5276. Walk live holders in order until an unowned removal.
pub fn check_cast_aura_removals(
    stores: &SourceSpellStores,
    auras: &SourceUnitAuras,
    cast: &SpellTemplate,
    caster: ObjectHandle,
    flag: u32,
    mut observe_lookup: impl FnMut(),
) -> Result<()> {
    for (id, holder) in auras.aura_holders() {
        let id = id.spell();
        let old = stores
            .template(id)
            .ok_or_else(|| refuse("execution.spells.missing_template"))?;
        observe_lookup();
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

// SpellMgr.cpp:56-101. The bound creature path has no player SpellMod owner.
pub fn creature_self_aura_duration(stores: &SourceSpellStores, t: &SpellTemplate) -> Result<i32> {
    let row = stores.dbc("SpellDuration.dbc", t.durationindex)?;
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
    require(
        stores
            .script_binding(SpellScriptKind::Aura, t.id)?
            .is_none(),
        "execution.spells.AuraScript_OnDurationCalculate_owner",
    )?;
    Ok(value)
}

/// Operands and continuations of the immediate creature self-aura hit.
pub trait SpellAuraHitOwner {
    fn aura_holder_seconds(&mut self, _spell: &SourceSpell) -> Result<i64> {
        Err(refuse("execution.spells.SpellAuraHolder_5174.time_owner"))
    }

    fn aura_effect_seconds(&mut self, _spell: &SourceSpell) -> Result<i64> {
        Err(refuse("execution.spells.Aura_270.time_owner"))
    }

    fn aura_hit_target(&mut self, _target: ObjectHandle) -> Result<&mut SourceUnitAuras> {
        Err(refuse(
            "execution.spells.Spell_1490.AddSpellAuraHolder_target_owner",
        ))
    }

    fn apply_inserted_aura_holder(
        &mut self,
        _t: &SpellTemplate,
        _holder: SourceAuraHolderId,
        _nested: Option<(&mut SourceSpellStarts, &mut SourceUnitEvents)>,
    ) -> Result<()> {
        Err(refuse(
            "execution.spells.Unit_4855.ApplyAuraModifiers_owner",
        ))
    }

    fn proc_aura_hit(&mut self, _spell: &mut SourceSpell) -> Result<()> {
        Err(refuse(
            "execution.spells.Spell_1331.ProcDamageAndSpell_owner",
        ))
    }
}

impl SourceSpell {
    /// Spell.cpp:1163-1184,1445-1494. Apply the immediate creature self hit.
    pub fn hit_self_auras(
        &mut self,
        t: &SpellTemplate,
        stores: &SourceSpellStores,
        level: u32,
        stand: u8,
        nested: Option<(&mut SourceSpellStarts, &mut SourceUnitEvents)>,
        owner: &mut impl SpellAuraHitOwner,
    ) -> Result<()> {
        let Some(target) = self.unique_targets.first_mut() else {
            return Ok(());
        };
        if target.processed || target.effect_mask == 0 {
            return Ok(());
        }
        target.processed = true;
        require(
            target.unit == self.caster,
            "execution.spells.Spell_1172.nonself_target_owner",
        )?;
        if target.missed {
            return Ok(());
        }
        let target_unit = target.unit;
        let mask = target.hit_mask & !target.processed_mask;
        if t.effects()
            .iter()
            .enumerate()
            .any(|(i, effect)| mask & (1 << i) != 0 && matches!(effect.effect, 6 | 27 | 35 | 119))
        {
            // SpellAuras.cpp:5174-5185. Holder time precedes duration calculation.
            let time = owner.aura_holder_seconds(self)?;
            let duration = creature_self_aura_duration(stores, t)?;
            let mut holder = SourceAuraHolder::new(t, target_unit, time, 0, duration)?;
            holder.triggered_by = self.triggered_by_aura;
            self.pending_aura = Some(holder);
        }
        for (i, effect) in t.effects().iter().enumerate() {
            if mask & (1 << i) == 0 {
                continue;
            }
            // Spell.cpp:4593-4612. Calculate damage before dispatching the effect.
            let amount = super::effect_value::effect_value(t, i, self.base_points[i])?;
            self.damage_per_effect[i] = amount;
            if effect.effect != 6 {
                return Err(refuse(&format!(
                    "execution.spells.SpellEffects.effect_{}_owner",
                    effect.effect
                )));
            }
            // SpellEffects.cpp:1731-1748; SpellAuras.cpp:266-290 binds effect time separately.
            let time = owner.aura_effect_seconds(self)?;
            let holder = self.pending_aura.as_mut().ok_or_else(|| {
                refuse("execution.spells.SpellEffects_1731.pending_aura_holder_owner")
            })?;
            holder.effects[i] = Some(SourceAura::self_spell_effect(
                t,
                i,
                effect.aura,
                self.base_points[i],
                amount,
                holder.stack,
                time,
            )?);
        }
        // Spell.cpp:1490. Transfer pending ownership before modifier callbacks.
        let inserted = if self.pending_aura.is_some() {
            let holder = owner
                .aura_hit_target(target_unit)?
                .insert_self_aura_holder(stores, t, &mut self.pending_aura, level, stand)?;
            owner.apply_inserted_aura_holder(t, holder, nested)?;
            Some(holder)
        } else {
            None
        };
        // Spell.cpp:1329-1331. Run the hit procs for both sides.
        owner.proc_aura_hit(self)?;
        // Spell.cpp:1353-1358. Only the holder created by this hit can become ready.
        if let Some(id) = inserted {
            let auras = owner.aura_hit_target(target_unit)?;
            require(
                !auras.spell_procs_happening,
                "execution.spells.Spell_1356.AddDelayedHolderDueToProc_owner",
            )?;
            let holder = auras
                .aura_holder_mut(id)
                .ok_or_else(|| refuse("execution.spells.Spell_1358.aura_holder_owner"))?;
            holder.ready = true;
        }
        Ok(())
    }
}

impl SourceAura {
    // SpellAuras.cpp:257-290,389. Construct the bound ApplyAura effect.
    pub(in crate::native::source_execution) fn self_spell_effect(
        t: &SpellTemplate,
        i: usize,
        kind: u32,
        base_points: i32,
        amount: i32,
        stack: u32,
        time: i64,
    ) -> Result<Self> {
        // SpellTargets.cpp:34,54,90 gives these self targets the HELPFUL filter.
        let positive = t.attributes & 0x0400_0000 == 0;
        let period = [t.effectamplitude1, t.effectamplitude2, t.effectamplitude3][i];
        Ok(Self {
            kind: source_aura_type(kind)?,
            // SpellAuras.cpp:389 multiplies in uint32 before pinned int32 assignment.
            amount: (amount as u32).wrapping_mul(stack) as i32,
            base_points,
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
        })
    }
}

impl SourceUnitAuras {
    // Unit.cpp:4687-4853. Check conflicts, assign fields, then index the holder.
    fn insert_self_aura_holder(
        &mut self,
        stores: &SourceSpellStores,
        t: &SpellTemplate,
        pending: &mut Option<SourceAuraHolder>,
        level: u32,
        stand: u8,
    ) -> Result<SourceAuraHolderId> {
        let holder = pending
            .as_mut()
            .ok_or_else(|| refuse("execution.spells.Unit_4687.pending_aura_holder_owner"))?;
        // Unit.cpp:4708-4773. The bound path stops on the first matching holder.
        if let Some(old) = self.first_aura_holder(t.id) {
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
        stores.stack(t.id)?;
        for (id, existing) in self.aura_holders() {
            let id = id.spell();
            stores.stack(id)?;
            let old = stores
                .template(id)
                .ok_or_else(|| refuse("execution.spells.missing_template"))?;
            if old.attributes & 0x40 != 0
                && (id != t.id || t.attributes & 0x10 == 0)
                && (id == t.id || stores.first_rank(id) != stores.first_rank(t.id))
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
        self.aura_fields
            .assign_self_aura_slot(t, holder, level, stand)?;
        // Unit.cpp:4847-4853. Set creation delay before indexing effects.
        holder.skip_update = true;
        self.store_aura_holder(t.id, pending)
    }
}

impl SourceAuraFields {
    // SpellAuras.cpp:5231-5283,5921-5974. Creature group/duration sends exit at Unit.cpp:10501-10523.
    fn assign_self_aura_slot(
        &mut self,
        t: &SpellTemplate,
        holder: &mut SourceAuraHolder,
        level: u32,
        stand: u8,
    ) -> Result<()> {
        holder.slot = 255;
        if !holder.passive {
            let positive = holder.effects.iter().flatten().all(|a| a.positive);
            let range = if positive { 0..32 } else { 32..48 };
            if let Some(slot) = range.into_iter().find(|&i| self.spells[i] == 0) {
                holder.slot = slot as u8;
                self.spells[slot] = t.id;
                let shift = (slot & 7) * 4;
                let flags = if positive {
                    4 | u32::from(t.attributes & 0x8000_0000 == 0)
                } else {
                    8
                };
                self.flags[slot >> 3] = (self.flags[slot >> 3] & !(15 << shift)) | flags << shift;
                let shift = (slot % 4) * 8;
                self.levels[slot / 4] =
                    (self.levels[slot / 4] & !(255 << shift)) | level.wrapping_shl(shift as u32);
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
                self.applications[slot / 4] =
                    (self.applications[slot / 4] & !(255 << shift)) | u32::from(count) << shift;
            }
        }
        // SpellAuras.cpp:5290-5303. Refuse at unbound stand, flag, or seal updates.
        let seated = matches!(stand, 1 | 2 | 4 | 5 | 6);
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
        )
    }
}

/// Live operands of Unit.cpp:4855 for the creature self-aura path.
pub struct SelfAuraApplication<'a> {
    pub object: ObjectHandle,
    pub objects: &'a SourceObjects,
    pub control_guids: &'a SourceControlGuids,
    pub auras: &'a mut SourceUnitAuras,
    pub threat_modifier: &'a mut [f32; 7],
    pub power_fields: &'a mut Option<SourceUnitPowerFields>,
    pub power_type: Option<&'a mut u8>,
    pub vitals: &'a mut Option<SourceUnitVitals>,
    pub form: &'a mut u32,
    pub player: bool,
}

pub trait SelfAuraApplicationOwner: SpellExecutionOwner {
    fn self_aura_application(&mut self) -> SelfAuraApplication<'_>;
}

/// Unit.cpp:4855-4865; SpellAuras.cpp:5224-5228. Use the inserted holder for
/// every callback, and borrow it again after a nested cast.
pub fn apply_self_aura_holder(
    owner: &mut impl SelfAuraApplicationOwner,
    stores: &SourceSpellStores,
    t: &SpellTemplate,
    holder: SourceAuraHolderId,
    mut nested: Option<(&mut SourceSpellStarts, &mut SourceUnitEvents)>,
) -> Result<()> {
    for (i, effect) in t.effects().iter().enumerate() {
        let mut live = owner.self_aura_application();
        let aura_holder = live.auras.applied_holder_mut(holder)?;
        if aura_holder.deleted {
            return Ok(());
        }
        let Some(aura) = aura_holder.effects[i].as_mut() else {
            continue;
        };
        // SpellAuras.cpp:677-690. The holder already checked for an AuraScript.
        let kind = effect.aura;
        match kind {
            192..=255 => {}
            95 => {
                live.auras.vis_flags |= 1; // SpellAuras.cpp:4401-4429; creature.
                live.auras.mark_effect_applied(holder, i)?;
                continue;
            }
            36 => {
                let misc = aura.misc;
                if let Some(spell) = live.apply_shape(stores, holder, misc)? {
                    // SpellAuras.cpp:4291-4295. The outer holder is still unready.
                    let caster = live.object;
                    let (starts, events) = nested
                        .as_mut()
                        .ok_or_else(|| refuse("execution.spells.Aura_4291.CastSpell_owner"))?;
                    SpellStartContext {
                        stores,
                        starts,
                        events,
                        owner,
                    }
                    .cast_triggered(
                        caster,
                        None,
                        spell,
                        Some(holder.spell()),
                    )?;
                }
                owner
                    .self_aura_application()
                    .auras
                    .mark_effect_applied(holder, i)?;
                continue;
            }
            10 => {
                // SpellAuras.cpp:2811-2843; Util.h:156-160.
                if live.objects.get(live.object)?.alive {
                    require(
                        !matches!(t.id, 26400 | 28862),
                        "execution.spells.Aura_2839.mod_threat_level_scaling_owner",
                    )?;
                    let amount = if aura.amount == -100 {
                        -99.99
                    } else {
                        aura.amount as f32
                    };
                    let factor = (100.0 + amount) / 100.0;
                    for (school, modifier) in live.threat_modifier.iter_mut().enumerate() {
                        if aura.misc & (1 << school) != 0 {
                            *modifier *= factor;
                        }
                    }
                }
            }
            23 => {
                // SpellAuras.cpp:3097-3117 sets periodic and skips removal-only casts.
                aura.periodic = true;
            }
            65 => {
                return Err(refuse(if aura.amount > 0 {
                    "execution.spells.Unit_10496.ApplyPercentModFloatValue_owner"
                } else {
                    "execution.spells.Unit_10498.ApplyPercentModFloatValue_owner"
                }));
            }
            4 => apply_creature_dummy_aura(stores, t)?,
            _ => {
                return Err(refuse(&format!(
                    "execution.spells.AuraHandler.type_{kind}_owner"
                )))
            }
        }
        aura.applied = true;
    }
    owner
        .self_aura_application()
        .auras
        .apply_holder_boosts(stores, t, holder)
}

impl SourceUnitAuras {
    fn applied_holder_mut(&mut self, holder: SourceAuraHolderId) -> Result<&mut SourceAuraHolder> {
        self.aura_holder_mut(holder)
            .ok_or_else(|| refuse("execution.spells.Unit_4855.aura_holder_owner"))
    }

    fn mark_effect_applied(&mut self, holder: SourceAuraHolderId, index: usize) -> Result<()> {
        if let Some(aura) = self.applied_holder_mut(holder)?.effects[index].as_mut() {
            aura.applied = true;
        }
        Ok(())
    }

    fn apply_holder_boosts(
        &mut self,
        stores: &SourceSpellStores,
        t: &SpellTemplate,
        holder: SourceAuraHolderId,
    ) -> Result<()> {
        if self.applied_holder_mut(holder)?.deleted {
            return Ok(());
        }
        // Unit.cpp:4860-4865; SpellAuras.cpp:5572-5652.
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
        let flags = stores
            .proc_event(t.id)
            .map_or(t.procflags, |v| v.flags | t.procflags);
        if flags & 1 != 0 {
            self.heartbeat_proc_counter = self.heartbeat_proc_counter.wrapping_add(1);
        }
        Ok(())
    }
}

// SpellAuras.cpp:1231-1629. Specific handlers refuse; defaults reach PetAura.
fn apply_creature_dummy_aura(stores: &SourceSpellStores, t: &SpellTemplate) -> Result<()> {
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
        stores.pet_aura(t.id).is_none(),
        "execution.spells.Aura_1583.AddPetAura_owner",
    )
}

impl SelfAuraApplication<'_> {
    fn apply_shape(
        &mut self,
        stores: &SourceSpellStores,
        holder: SourceAuraHolderId,
        misc: i32,
    ) -> Result<Option<u32>> {
        let form = source_shape_form(misc)?;
        let Some(_) = stores.dbc("SpellShapeshiftForm.dbc", form)? else {
            return Ok(None); // SpellAuras.cpp:1716-1723.
        };
        let boost = match form {
            0x11 => 21156,
            0x12 => 7376,
            0x13 => 7381,
            _ => return Err(refuse("execution.spells.Aura_1729.shapeshift_form_owner")),
        };
        // SpellAuras.cpp:1861; Unit.cpp:820-834 excludes this holder.
        require(
            self.auras
                .auras_of_type(36)
                .iter()
                .all(|effect| effect.holder == holder),
            "execution.spells.Unit_831.RemoveAurasDueToSpell_owner",
        )?;
        self.set_stance_power_type()?;
        require(
            *self.form != 0x12 || !self.auras.has_aura_type(112),
            "execution.spells.Aura_1884.GetOverrideScript_owner",
        )?;
        require(
            !self.player,
            "execution.spells.Aura_1890.tactical_mastery_owner",
        )?;
        self.clear_stance_rage()?;
        // SpellAuras.cpp:1920-1936; Unit.cpp:5211-5223.
        *self.form = form;
        for id in self.auras.aura_spell_ids() {
            let aura = stores
                .template(id)
                .ok_or_else(|| refuse("execution.spells.missing_template"))?;
            require(
                aura.aurainterruptflags & 0x8000 == 0,
                "execution.spells.Unit_5217.RemoveSpellAuraHolder_owner",
            )?;
        }
        Ok(Some(boost)) // SpellAuras.cpp:4263-4271.
    }

    fn set_stance_power_type(&mut self) -> Result<()> {
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
        Ok(())
    }

    fn clear_stance_rage(&mut self) -> Result<()> {
        let rage = &mut self
            .power_fields
            .as_mut()
            .and_then(|power| power.auxiliary_current.as_mut())
            .ok_or_else(|| refuse("execution.spells.Aura_1911.GetPower_rage_owner"))?[0];
        if *rage != 0 {
            *rage = 0;
            self.require_no_power_group("execution.spells.Unit_9294.GetGroup_owner")?;
        }
        Ok(())
    }

    fn require_no_power_group(&self, refusal: &str) -> Result<()> {
        require(!self.player, refusal)?;
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

#[test]
fn inserting_without_a_pending_holder_refuses_before_writing_slots() {
    let mut auras = SourceUnitAuras::default();
    let id = auras
        .insert_self_aura_holder(
            &SourceSpellStores::default(),
            &SpellTemplate::default(),
            &mut None,
            1,
            0,
        )
        .unwrap_err()
        .id;
    assert_eq!(id, "execution.spells.Unit_4687.pending_aura_holder_owner");
    assert_eq!(auras, SourceUnitAuras::default());
}
