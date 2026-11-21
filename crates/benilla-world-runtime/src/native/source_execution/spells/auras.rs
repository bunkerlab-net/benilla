// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! SpellAuras.cpp:257-290,5155-5212. State belongs to the target Unit.
use super::super::relations::unbound_pet_loading;
use super::procs::heartbeat_aura;
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
    pub area: Option<SourceAreaAura>,
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
    script: Option<SourceSpellScript>,
}
impl SourceAuraHolder {
    pub(super) fn for_self_spell(
        stores: &SourceSpellStores,
        t: &SpellTemplate,
        target: ObjectHandle,
        triggered_by: Option<u32>,
        seconds: impl FnOnce() -> Result<i64>,
    ) -> Result<Self> {
        // SpellAuras.cpp:5164,5174,5185,5211. The script read precedes the clock.
        let script = stores.typed_script(
            SpellScriptKind::Aura,
            t.id,
            "execution.spells.SpellAuras_5164.script_binding_owner",
        )?;
        let time = seconds()?;
        let duration = SourceSpellScript::on_duration_calculate(
            script,
            creature_self_aura_duration(stores, t)?,
        );
        let mut holder = Self::new(t, target, time, 0, duration)?;
        holder.triggered_by = triggered_by;
        holder.script = script;
        if let Some(script) = script {
            script.on_holder_init(&mut holder);
        }
        Ok(holder)
    }

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
            script: None,
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

/// Immutable operands shared by the creature self-hit and removal paths.
#[derive(Clone, Copy)]
pub struct SelfAuraTarget<'a> {
    pub target: ObjectHandle,
    pub level: Option<u32>,
    pub alive: bool,
    pub player: bool,
    pub player_group: Option<bool>,
    pub totem: bool,
    pub objects: &'a SourceObjects,
    pub control_guids: &'a SourceControlGuids,
}

/// Live operands of the bound creature self-holder removal.
pub struct SelfAuraRemoval<'a> {
    pub unit: SelfAuraTarget<'a>,
    pub visibility: &'a mut SourceUnitVisibility,
    pub damage: DamageAuraTarget<'a>,
}

/// Unit.h:242-254.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum AuraRemoveMode {
    Default = 0,
    Stack = 1,
    Death = 4,
    Delete = 5,
    Expire = 7,
}

// Unit.cpp:5253-5276. Only RemoveAurasOnCast has the cast filters.
pub fn check_cast_aura_removals(
    stores: &SourceSpellStores,
    auras: &mut SourceUnitAuras,
    cast: &SpellTemplate,
    mut live: SelfAuraRemoval<'_>,
    flag: u32,
    observe_lookup: impl FnMut(),
) -> Result<()> {
    let target = live.unit.target;
    auras.remove_interrupting_auras(
        stores,
        &mut live,
        flag,
        |old, holder| {
            (old.id != cast.id || holder.caster != target)
                && (cast.attributesex & 0x20 == 0 || old.dispel != 5)
                && (cast.attributesex2 & 0x4000 == 0 || old.dispel != 6)
        },
        observe_lookup,
    )
}

impl SourceUnitAuras {
    // Unit.cpp:5211-5223. The flag-only overload has no exceptions.
    pub(in crate::native::source_execution) fn remove_auras_with_interrupt_flags(
        &mut self,
        stores: &SourceSpellStores,
        live: &mut SelfAuraRemoval<'_>,
        flag: u32,
    ) -> Result<()> {
        self.remove_interrupting_auras(stores, live, flag, |_, _| true, || {})
    }

    // Unit.cpp:5218,5271. Restart at begin() after each removal.
    fn remove_interrupting_auras(
        &mut self,
        stores: &SourceSpellStores,
        live: &mut SelfAuraRemoval<'_>,
        flag: u32,
        mut can_remove: impl FnMut(&SpellTemplate, &SourceAuraHolder) -> bool,
        mut observe_lookup: impl FnMut(),
    ) -> Result<()> {
        loop {
            let mut remove = None;
            for (id, holder) in self.aura_holders() {
                let old = stores
                    .template(id.spell())
                    .ok_or_else(|| refuse("execution.spells.missing_template"))?;
                observe_lookup();
                if old.aurainterruptflags & flag != 0 && can_remove(old, holder) {
                    remove = Some(id);
                    break;
                }
            }
            let Some(id) = remove else { return Ok(()) };
            self.remove_self_aura_holder(stores, id, AuraRemoveMode::Default, live)?;
        }
    }
}

fn spell_duration_word(row: Option<&SpellDbcRecord>, index: usize) -> Result<i32> {
    row.map_or(Ok(0), |row| {
        row.words
            .get(&index)
            .map(|v| *v as i32)
            .ok_or_else(|| refuse("execution.spells.SpellDuration_layout"))
    })
}

// SpellMgr.cpp:56-64. Unlike CalculateSpellDuration, this never calls a script.
fn template_duration(stores: &SourceSpellStores, t: &SpellTemplate) -> Result<i32> {
    source_spell_duration(spell_duration_word(
        stores.dbc("SpellDuration.dbc", t.durationindex)?,
        1,
    )?)
}

// SpellMgr.cpp:56-101. The bound creature path has no player SpellMod owner.
pub fn creature_self_aura_duration(stores: &SourceSpellStores, t: &SpellTemplate) -> Result<i32> {
    let row = stores.dbc("SpellDuration.dbc", t.durationindex)?;
    let value = source_spell_duration(spell_duration_word(row, 1)?)?;
    if value != -1 && t.attributesex3 & 0x2000_0000 == 0 {
        source_spell_max_duration(spell_duration_word(row, 3)?)?;
    }
    Ok(value)
}

pub struct SelfAuraHitTarget<'a> {
    pub unit: SelfAuraTarget<'a>,
    pub stand: u8,
    pub auras: &'a mut SourceUnitAuras,
    pub visibility: &'a mut SourceUnitVisibility,
    pub damage: DamageAuraTarget<'a>,
}

/// Operands and continuations of an immediate self cast's effects.
pub trait SpellAuraHitOwner {
    fn targetless_proc(&mut self, _: &SourceSpell, _: &SpellTemplate, _: u8) -> Result<()> {
        Err(refuse(
            "execution.spells.Spell_1551.ProcDamageAndSpell_owner",
        ))
    }

    fn aura_holder_seconds(&mut self, _spell: &SourceSpell) -> Result<i64> {
        Err(refuse("execution.spells.SpellAuraHolder_5174.time_owner"))
    }

    fn aura_effect_seconds(&mut self, _spell: &SourceSpell) -> Result<i64> {
        Err(refuse("execution.spells.Aura_270.time_owner"))
    }

    fn aura_target(&self, _target: ObjectHandle) -> Result<SelfAuraTarget<'_>> {
        Err(refuse("execution.spells.Spell_1445.aura_target_owner"))
    }

    fn aura_hit_target(&mut self, _target: ObjectHandle) -> Result<SelfAuraHitTarget<'_>> {
        Err(refuse(
            "execution.spells.Spell_1490.AddSpellAuraHolder_target_owner",
        ))
    }

    fn apply_inserted_aura_holder(
        &mut self,
        _t: &SpellTemplate,
        _holder: SourceAuraHolderId,
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

// SpellEffects.cpp:1731-1748,2331-2338.
pub(crate) fn self_aura_effect(effect: u32) -> Result<Option<SourceAreaAuraKind>> {
    let area = SourceAreaAuraKind::for_effect(effect);
    if area.is_some() || effect == 6 {
        return Ok(area);
    }
    Err(refuse(&format!(
        "execution.spells.SpellEffects.effect_{effect}_owner"
    )))
}

#[test]
fn targetless_empty_effects_calculate_value_before_procs() {
    struct Procs(u8);
    impl SpellAuraHitOwner for Procs {
        fn targetless_proc(
            &mut self,
            spell: &SourceSpell,
            _: &SpellTemplate,
            mask: u8,
        ) -> Result<()> {
            assert_eq!((mask, spell.damage_per_effect[0]), (1, 15));
            self.0 += 1;
            Ok(())
        }
    }
    let caster = ObjectHandle {
        thread: 12,
        construction: 1,
    };
    let mut procs = Procs(0);
    for (effect, class, player, refusal) in [
        (20, -1, true, None),
        (25, -1, true, None),
        (26, -1, true, None),
        (37, -1, true, None),
        (39, -1, true, None),
        (60, 2, false, None),
        (60, -1, true, None),
        (
            60,
            2,
            true,
            Some("execution.spells.SpellEffects_2319.proficiency_owner"),
        ),
        (
            60,
            4,
            true,
            Some("execution.spells.SpellEffects_2324.proficiency_owner"),
        ),
        (
            21,
            -1,
            true,
            Some("execution.spells.Spell_1527.targetless_effect_owner"),
        ),
    ] {
        let t = SpellTemplate {
            effect1: effect,
            equippeditemclass: class,
            effectbasepoints1: 14,
            effectbasedice1: 1,
            ..SpellTemplate::default()
        };
        let mut spell = SourceSpell::new(1, &t, caster, SpellTriggerFlags(1), 0).unwrap();
        spell.targetless_mask = 1;
        let result = spell.hit_targetless(&t, &mut procs, Some(1), player);
        if let Some(id) = refusal {
            assert_eq!(result.unwrap_err().id, id);
            assert_eq!(spell.damage_per_effect[0], 15);
        } else {
            result.unwrap();
        }
    }
    assert_eq!(procs.0, 7);
}

impl SourceSpell {
    /// Spell.cpp:1504-1528,1544-1552. Destination targeting is unbound.
    pub(super) fn hit_targetless(
        &mut self,
        t: &SpellTemplate,
        owner: &mut impl SpellAuraHitOwner,
        level: Option<u32>,
        player: bool,
    ) -> Result<()> {
        let mask = self.targetless_mask;
        for i in 0..3 {
            if mask & (1 << i) != 0 {
                require(
                    self.handle_bound_effect(t, i, level, player)?,
                    "execution.spells.Spell_1527.targetless_effect_owner",
                )?;
            }
        }
        if mask != 0 {
            owner.targetless_proc(self, t, mask)?;
        }
        Ok(())
    }

    // Spell.cpp:4593-4612. Calculate the value before the shared dispatch table.
    fn handle_bound_effect(
        &mut self,
        t: &SpellTemplate,
        index: usize,
        level: Option<u32>,
        player: bool,
    ) -> Result<bool> {
        let amount =
            super::effect_value::effect_value(t, index, self.base_points[index], level, player)?;
        require(
            self.spell_script.is_none(),
            "execution.spells.Spell_4610.OnEffectExecute_owner",
        )?;
        self.damage_per_effect[index] = amount;
        // SpellEffects.cpp:78,83-84,95,97,190-193. Empty effects write nothing.
        match t.effects()[index].effect {
            20 | 25 | 26 | 37 | 39 => Ok(true),
            // SpellEffects.cpp:2313-2327. Only player weapon or armor can reach the masks.
            60 if player && t.equippeditemclass == 2 => Err(refuse(
                "execution.spells.SpellEffects_2319.proficiency_owner",
            )),
            60 if player && t.equippeditemclass == 4 => Err(refuse(
                "execution.spells.SpellEffects_2324.proficiency_owner",
            )),
            60 => Ok(true),
            _ => Ok(false),
        }
    }

    /// Spell.cpp:1163-1184,1445-1494. Apply the immediate creature self hit.
    pub fn hit_self_auras(
        &mut self,
        t: &SpellTemplate,
        stores: &SourceSpellStores,
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
            self.pending_aura = Some(SourceAuraHolder::for_self_spell(
                stores,
                t,
                target_unit,
                self.triggered_by_aura,
                || owner.aura_holder_seconds(self),
            )?);
        }
        for (i, effect) in t.effects().iter().enumerate() {
            if mask & (1 << i) == 0 {
                continue;
            }
            let SelfAuraTarget {
                level,
                player,
                alive,
                ..
            } = owner.aura_target(target_unit)?;
            if self.handle_bound_effect(t, i, level, player)? {
                continue;
            }
            let amount = self.damage_per_effect[i];
            let area = self_aura_effect(effect.effect)?;
            if area.is_some() && !alive {
                continue; // SpellEffects.cpp:2333-2334.
            }
            // SpellEffects.cpp:1731-1748; SpellAuras.cpp:266-290 binds effect time separately.
            let time = owner.aura_effect_seconds(self)?;
            let holder = self.pending_aura.as_mut().ok_or_else(|| {
                refuse("execution.spells.SpellEffects_1731.pending_aura_holder_owner")
            })?;
            // SpellAuras.cpp:275. The bound unit caster is present.
            let amount = holder
                .script
                .map_or(amount, |script| script.on_aura_value_calculate(amount));
            let mut aura = SourceAura::self_spell_effect(
                t,
                i,
                effect.aura,
                self.base_points[i],
                amount,
                holder.stack,
                time,
            )?;
            if let Some(kind) = area {
                let live = owner.aura_target(target_unit)?;
                SourceAreaAura::attach(
                    &mut aura,
                    kind,
                    stores,
                    [
                        t.effectradiusindex1,
                        t.effectradiusindex2,
                        t.effectradiusindex3,
                    ][i],
                    || world_object_spell_mod_owner(live.objects, self.caster, live.totem),
                    live.totem,
                )?;
            }
            // SpellAuras.cpp:376-377. Set the value before dispatching OnAuraInit.
            aura.script_value = self.script_value;
            if let Some(script) = holder.script {
                script.on_aura_init(&mut aura);
            }
            holder.effects[i] = Some(aura);
        }
        // Spell.cpp:1460,1496-1500. A dead target can leave an empty holder.
        if self
            .pending_aura
            .as_ref()
            .is_some_and(|holder| holder.effects.iter().all(Option::is_none))
        {
            self.pending_aura = None;
        }
        // Spell.cpp:1490. Transfer pending ownership before modifier callbacks.
        let inserted = if self.pending_aura.is_some() {
            let target = owner.aura_hit_target(target_unit)?;
            let holder = target.auras.insert_self_aura_holder(
                stores,
                t,
                &mut self.pending_aura,
                &mut SelfAuraRemoval {
                    unit: target.unit,
                    visibility: target.visibility,
                    damage: target.damage,
                },
                target.stand,
            )?;
            owner.apply_inserted_aura_holder(t, holder)?;
            Some(holder)
        } else {
            None
        };
        // Spell.cpp:1329-1331. Run the hit procs for both sides.
        owner.proc_aura_hit(self)?;
        require(
            self.spell_script.is_none(),
            "execution.spells.Spell_1334.OnAfterHit_owner",
        )?;
        // Spell.cpp:1353-1358. Only the holder created by this hit can become ready.
        if let Some(id) = inserted {
            let auras = owner.aura_hit_target(target_unit)?.auras;
            require(
                !auras.spell_procs_happening,
                "execution.spells.Spell_1356.AddDelayedHolderDueToProc_owner",
            )?;
            let holder = auras
                .retained_aura_holder_mut(id)
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
        // SpellMgr.h:1067-1069,1087-1089,1212-1215 on aura-apply effects.
        let e = t.effects()[i];
        let hostile = |mode: u32| {
            SPELL_TARGET_INFO_TABLE
                .get(mode as usize)
                .is_some_and(|info| info.filter == SpellTargetFilter::Harmful)
        };
        let positive =
            t.attributes & 0x0400_0000 == 0 && !hostile(e.target_a) && !hostile(e.target_b);
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
            area: None,
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
        live: &mut SelfAuraRemoval<'_>,
        stand: u8,
    ) -> Result<SourceAuraHolderId> {
        let holder = pending
            .as_mut()
            .ok_or_else(|| refuse("execution.spells.Unit_4687.pending_aura_holder_owner"))?;
        let area = holder
            .effects
            .iter()
            .flatten()
            .any(|aura| aura.area.is_some());
        let persistent = holder.effects.iter().flatten().any(|aura| aura.persistent);
        // Unit.cpp:4708-4773. The same-caster removal breaks the equal-key scan.
        let same_spell = ((!holder.passive && !persistent) || area)
            .then(|| self.aura_holders_for_spell(t.id).next())
            .flatten();
        if let Some((id, old)) = same_spell {
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
            self.remove_self_aura_holder(stores, id, AuraRemoveMode::Stack, live)?;
        }
        // Unit.cpp:4778-4783,4930-5045; SpellMgr.h:351-356.
        let party = t.effects().iter().any(|effect| effect.effect == 35);
        if !holder.passive || party || t.effects().iter().any(|effect| effect.effect == 6) {
            self.check_aura_stacking(stores, t, holder, party)?;
        }
        require(
            !holder.tracked,
            "execution.spells.Unit_4793.GetTrackedAuraTargets_owner",
        )?;
        self.aura_fields
            .assign_self_aura_slot(t, holder, live.unit, stand)?;
        // Unit.cpp:4847-4853. Set creation delay before indexing effects.
        holder.skip_update = true;
        self.store_aura_holder(t.id, pending)
    }

    fn check_aura_stacking(
        &self,
        stores: &SourceSpellStores,
        t: &SpellTemplate,
        holder: &SourceAuraHolder,
        party: bool,
    ) -> Result<()> {
        let data = stores.stack(t.id)?;
        for (id, existing) in self.aura_holders() {
            let id = id.spell();
            let existing_data = stores.stack(id)?;
            let old = stores
                .template(id)
                .ok_or_else(|| refuse("execution.spells.missing_template"))?;
            let own = existing.caster == holder.caster;
            let another_rank = id != t.id && stores.first_rank(id) == stores.first_rank(t.id);
            if old.attributes & 0x40 != 0
                && (id != t.id || t.attributes & 0x10 == 0)
                && ((!own && !party) || !another_rank)
            {
                continue;
            }
            // Unit.cpp:4974-4977 lets a triggering aura coexist with its triggered boost.
            if holder.triggered_by == Some(id) || existing.triggered_by == Some(t.id) {
                continue;
            }
            let mut unique = id == t.id && t.attributesex & 0x800 != 0;
            if let Some((data, _)) = data
                .zip(existing_data)
                .filter(|(data, existing)| data.mask & existing.mask != 0)
            {
                unique = matches!(data.rule, 1 | 2);
            }
            // SharedDefines.h:1649-1675; LIMITONLY never conflicts.
            if !unique && !matches!(holder.diminish_group, 0 | 18) {
                unique = holder.diminish_group == existing.diminish_group;
            }
            require(
                own,
                "execution.spells.Unit_5000.different_caster_stacker_owner",
            )?;
            let stackable = spell_stackable(Some(t), Some(old))?;
            require(
                !unique && stackable && !another_rank,
                "execution.spells.Unit_5003.aura_stack_resolution_owner",
            )?;
        }
        Ok(())
    }

    // Unit.cpp:5326-5390; the bound caster and target are the same live unit.
    pub fn remove_self_aura_holder(
        &mut self,
        stores: &SourceSpellStores,
        id: SourceAuraHolderId,
        mode: AuraRemoveMode,
        live: &mut SelfAuraRemoval<'_>,
    ) -> Result<()> {
        let holder = self
            .aura_holder(id)
            .ok_or_else(|| refuse("execution.spells.Unit_5328.live_aura_holder_owner"))?;
        require(
            !holder.deleted,
            "execution.spells.Unit_5328.deleted_aura_holder_assert",
        )?;
        let t = stores
            .template(id.spell())
            .ok_or_else(|| refuse("execution.spells.missing_template"))?;
        require(
            holder.caster == holder.target && holder.target == live.unit.target,
            "execution.spells.Unit_5333.GetCaster_owner",
        )?;
        let channeled = t.attributesex & 0x44 != 0;
        require(
            !channeled || !live.unit.totem,
            "execution.spells.Unit_5335.GetTotemType_owner",
        )?;
        let mut holder = self
            .unlink_aura_holder(id)
            .ok_or_else(|| refuse("execution.spells.Unit_5346.aura_holder_map_owner"))?;
        holder.remove_mode = mode as u32;
        let result = self.remove_holder_contents(stores, t, id, &mut holder, mode, live);
        if result.is_ok() {
            holder.deleted = true;
        }
        self.defer_aura_holder(id, holder);
        result?;
        // Unit.cpp:5373-5378. The proc store is read only with a nonzero counter.
        if self.heartbeat_proc_counter != 0 && heartbeat_aura(stores, t) {
            self.heartbeat_proc_counter -= 1;
        }
        if mode != AuraRemoveMode::Expire && channeled && !checks::area_spell(t)? {
            return Err(refuse(
                "execution.spells.Unit_5381.GetChannelObjectGuid_owner",
            ));
        }
        if matches!(t.id, 28682 | 11129) {
            let sibling = if t.id == 28682 { 11129 } else { 28682 };
            self.remove_self_auras_by_spell(stores, sibling, None, live)?;
        }
        Ok(())
    }

    fn remove_holder_contents(
        &mut self,
        stores: &SourceSpellStores,
        t: &SpellTemplate,
        id: SourceAuraHolderId,
        holder: &mut SourceAuraHolder,
        mode: AuraRemoveMode,
        live: &mut SelfAuraRemoval<'_>,
    ) -> Result<()> {
        // SpellAuras.cpp:5906-5919. A tracked self holder would access the caster's map.
        require(
            !holder.tracked,
            "execution.spells.SpellAuraHolder_5915.GetTrackedAuraTargets_owner",
        )?;
        for index in 0..holder.effects.len() {
            self.remove_aura(
                stores,
                t,
                SourceAuraEffectRef {
                    holder: id,
                    effect: index as u8,
                },
                holder,
                mode,
                live,
            )?;
        }
        self.remove_aura_slot(stores, t, holder, mode, live)?;
        if mode != AuraRemoveMode::Delete {
            self.handle_spell_specific_boosts(stores, t, Some(live))?;
        }
        Ok(())
    }

    // Unit.cpp:5404-5446. The holder and type list no longer expose the aura
    // when its modifier runs.
    fn remove_aura(
        &mut self,
        stores: &SourceSpellStores,
        t: &SpellTemplate,
        reference: SourceAuraEffectRef,
        holder: &mut SourceAuraHolder,
        mode: AuraRemoveMode,
        live: &mut SelfAuraRemoval<'_>,
    ) -> Result<()> {
        let effect = holder
            .effects
            .get_mut(usize::from(reference.effect))
            .ok_or_else(|| refuse("execution.spells.Unit_5425.holder_effect_owner"))?;
        let Some(aura) = effect.as_mut() else {
            return Ok(());
        };
        if aura.kind < 192 {
            self.unlink_aura_effect(aura.kind, reference);
        }
        aura.remove_mode = mode as u32;
        let Some(mut aura) = effect.take() else {
            return Ok(());
        };
        let result = if mode != AuraRemoveMode::Delete || matches!(aura.kind, 2 | 128) {
            let result =
                self.unapply_aura(stores, t, &mut aura, mode, live, (reference.holder, holder));
            if result.is_ok() {
                aura.applied = false;
            }
            result
        } else {
            Ok(())
        };
        self.defer_aura(aura);
        result
    }

    // SpellAuras.cpp:679,684,686,690. Unapply reverses the hook order.
    fn unapply_aura(
        &mut self,
        stores: &SourceSpellStores,
        t: &SpellTemplate,
        aura: &mut SourceAura,
        mode: AuraRemoveMode,
        live: &mut SelfAuraRemoval<'_>,
        (id, holder): (SourceAuraHolderId, &SourceAuraHolder),
    ) -> Result<()> {
        let kind = aura.kind;
        if let Some(script) = holder.script {
            script.on_after_apply(aura, false);
        }
        match SelfAuraApplyHandler::for_kind(kind) {
            Some(SelfAuraApplyHandler::Dummy) => handle_creature_dummy_aura(
                stores,
                t,
                live.unit.player,
                Some((mode, live.unit.alive)),
            ),
            Some(SelfAuraApplyHandler::Stealth) => {
                self.handle_stealth(stores, live, aura.misc, aura.amount, false, true)
            }
            Some(SelfAuraApplyHandler::Invisibility) => {
                self.handle_invisibility(stores, live, aura.misc, aura.amount, false, true)
            }
            Some(SelfAuraApplyHandler::InvisibilityDetect) => self.handle_invisibility_detect(
                live,
                aura.misc,
                aura.amount,
                false,
                true,
                Some((id, &holder.effects)),
            ),
            Some(SelfAuraApplyHandler::StealthDetect) => {
                handle_mod_stealth_detect(live.visibility, aura.misc, aura.amount, false)
            }
            Some(SelfAuraApplyHandler::DamagePercent) => {
                handle_damage_percent_done(live.unit, &mut live.damage, t, aura, Some(mode), true)
            }
            Some(SelfAuraApplyHandler::Ghost) => {
                self.handle_aura_ghost(live.unit.objects, live.unit.target, false, live.unit.player)
            }
            Some(SelfAuraApplyHandler::ProcTrigger | SelfAuraApplyHandler::NoImmediateEffect) => {
                Ok(())
            }
            Some(SelfAuraApplyHandler::Null) if kind >= 192 => Ok(()),
            _ => Err(refuse(&format!(
                "execution.spells.AuraHandler.type_{kind}_unapply_owner"
            ))),
        }?;
        if let Some(script) = holder.script {
            script.on_apply(aura, false);
        }
        Ok(())
    }

    // SpellAuras.cpp:5306-5393.
    fn remove_aura_slot(
        &mut self,
        stores: &SourceSpellStores,
        t: &SpellTemplate,
        holder: &mut SourceAuraHolder,
        mode: AuraRemoveMode,
        live: &mut SelfAuraRemoval<'_>,
    ) -> Result<()> {
        if mode != AuraRemoveMode::Stack {
            self.cleanup_aura_triggered_spells(stores, t, live)?;
        }
        // IsPersistent scans m_auras (SpellAuras.cpp:5833-5839), now empty.
        let slot = usize::from(holder.slot);
        if self
            .aura_fields
            .spells
            .get(slot)
            .is_none_or(|spell| *spell == 0)
        {
            return Ok(());
        }
        require(
            holder.diminish_group == 0,
            "execution.spells.SpellAuraHolder_5337.ApplyDiminishingAura_owner",
        )?;
        let unit = live.unit;
        self.aura_fields.set_aura_spell(unit, slot, 0)?;
        self.aura_fields.set_aura_flags(unit, slot, 0)?;
        let level = unit
            .level
            .ok_or_else(|| refuse("execution.spells.SpellAuraHolder_5341.GetLevel_owner"))?;
        self.aura_fields.set_aura_level(unit, slot, level)?;
        holder.charges = 0;
        holder.stack = 1;
        self.aura_fields.update_aura_application(
            unit.objects,
            unit.target,
            holder.slot,
            holder.charges,
            holder.stack,
        )?;
        if mode == AuraRemoveMode::Delete {
            return Ok(());
        }
        unit.update_aura_for_group()?;
        if t.spellfamilyname == 10 && t.spellfamilyflags & 0x0a00_0200 != 0 {
            let mut sibling = false;
            for id in self.aura_spell_ids() {
                let other = stores
                    .template(id)
                    .ok_or_else(|| refuse("execution.spells.missing_template"))?;
                if other.spellfamilyname == 10 && other.spellfamilyflags & t.spellfamilyflags != 0 {
                    sibling = true;
                    break;
                }
            }
            require(
                sibling,
                "execution.spells.SpellAuraHolder_5375.ModifyAuraState_owner",
            )?;
        }
        if t.attributesex & 0x0004_0000 != 0 {
            let mut sibling = false;
            for id in self.aura_spell_ids() {
                let other = stores
                    .template(id)
                    .ok_or_else(|| refuse("execution.spells.missing_template"))?;
                if other.attributesex & 0x0004_0000 != 0 {
                    sibling = true;
                    break;
                }
            }
            require(
                sibling,
                "execution.spells.SpellAuraHolder_5384.RemoveFlag_owner",
            )?;
        }
        require(
            t.attributes & 0x0200_0000 == 0,
            "execution.spells.SpellAuraHolder_5390.AddCooldown_owner",
        )
    }

    // SpellAuras.cpp:5395-5421.
    fn cleanup_aura_triggered_spells(
        &mut self,
        stores: &SourceSpellStores,
        t: &SpellTemplate,
        live: &mut SelfAuraRemoval<'_>,
    ) -> Result<()> {
        for (index, effect) in t.effects().into_iter().enumerate() {
            if effect.aura == 0 || effect.trigger == 0 {
                continue;
            }
            let Some(triggered) = stores.template(effect.trigger) else {
                continue;
            };
            if template_duration(stores, triggered)? != -1
                || effect.aura == 23
                    && template_duration(stores, t)?
                        == [t.effectamplitude1, t.effectamplitude2, t.effectamplitude3][index]
                            as i32
            {
                continue;
            }
            self.remove_self_auras_by_spell(stores, effect.trigger, None, live)?;
        }
        Ok(())
    }

    // Unit.cpp:5049-5063,5179-5193. Restart the selected spell's range.
    fn remove_self_auras_by_spell(
        &mut self,
        stores: &SourceSpellStores,
        spell: u32,
        caster: Option<ObjectHandle>,
        live: &mut SelfAuraRemoval<'_>,
    ) -> Result<()> {
        loop {
            let id = self
                .aura_holders_for_spell(spell)
                .find(|(_, holder)| caster.is_none_or(|caster| caster == holder.caster))
                .map(|(id, _)| id);
            let Some(id) = id else { return Ok(()) };
            self.remove_self_aura_holder(stores, id, AuraRemoveMode::Default, live)?;
        }
    }
}

impl SelfAuraTarget<'_> {
    // Unit.cpp:10501-10523.
    fn update_aura_for_group(self) -> Result<()> {
        if self.player {
            return match self.player_group {
                Some(false) => Ok(()),
                Some(true) => Err(refuse("execution.spells.Unit_10508.aura_slot_group_owner")),
                None => Err(refuse("execution.spells.Unit_10506.GetGroup_owner")),
            };
        }
        if self.control_guids.has_charmer() {
            if let Some(charmer) = self
                .control_guids
                .get_charmer(self.objects, self.target, None, unbound_pet_loading)?
                .unit()
            {
                require(
                    self.objects.get(charmer)?.kind != SourceObjectKind::Player,
                    "execution.spells.Unit_10516.charmer_GetGroup_owner",
                )?;
            }
        }
        Ok(())
    }
}

impl SourceAuraFields {
    // SpellAuras.cpp:5231-5283,5921-5974. Creature group/duration sends exit at Unit.cpp:10501-10523.
    fn assign_self_aura_slot(
        &mut self,
        t: &SpellTemplate,
        holder: &mut SourceAuraHolder,
        live: SelfAuraTarget<'_>,
        stand: u8,
    ) -> Result<()> {
        holder.slot = 255;
        if !holder.passive {
            let positive = holder.effects.iter().flatten().all(|a| a.positive);
            let range = if positive { 0..32 } else { 32..48 };
            if let Some(slot) = range.into_iter().find(|&i| self.spells[i] == 0) {
                holder.slot = slot as u8;
                self.set_aura_spell(live, slot, t.id)?;
                let flags = if positive {
                    4 | u32::from(t.attributes & 0x8000_0000 == 0)
                } else {
                    8
                };
                self.set_aura_flags(live, slot, flags)?;
                let level = live.level.ok_or_else(|| {
                    refuse("execution.spells.SpellAuraHolder_5277.GetLevel_owner")
                })?;
                self.set_aura_level(live, slot, level)?;
                self.update_aura_application(
                    live.objects,
                    live.target,
                    holder.slot,
                    holder.charges,
                    holder.stack,
                )?;
                live.update_aura_for_group()?;
                require(
                    !live.player || holder.passive,
                    "execution.spells.SpellAuraHolder_5986.aura_duration_session_owner",
                )?;
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

    // SpellAuras.h:195.
    fn set_aura_spell(&mut self, live: SelfAuraTarget<'_>, slot: usize, spell: u32) -> Result<()> {
        live.objects.set_update_value(
            live.target,
            UNIT_FIELD_AURA + slot,
            &mut self.spells[slot],
            spell,
        )?;
        Ok(())
    }

    // SpellAuras.cpp:5921-5942.
    fn set_aura_flags(&mut self, live: SelfAuraTarget<'_>, slot: usize, flags: u32) -> Result<()> {
        let index = slot >> 3;
        let shift = (slot & 7) * 4;
        let value = (self.flags[index] & !(15 << shift)) | flags << shift;
        live.objects.set_update_value(
            live.target,
            UNIT_FIELD_AURAFLAGS + index,
            &mut self.flags[index],
            value,
        )?;
        Ok(())
    }

    // SpellAuras.cpp:5945-5953.
    fn set_aura_level(&mut self, live: SelfAuraTarget<'_>, slot: usize, level: u32) -> Result<()> {
        let index = slot / 4;
        let shift = (slot % 4) * 8;
        let value = (self.levels[index] & !(255 << shift)) | level << shift;
        live.objects.set_update_value(
            live.target,
            UNIT_FIELD_AURALEVELS + index,
            &mut self.levels[index],
            value,
        )?;
        Ok(())
    }

    // SpellAuras.cpp:5960-5974.
    fn update_aura_application(
        &mut self,
        objects: &SourceObjects,
        target: ObjectHandle,
        slot: u8,
        charges: u32,
        stack: u32,
    ) -> Result<()> {
        let slot = usize::from(slot);
        if slot >= self.spells.len() {
            return Ok(());
        }
        let count = if charges > 0 {
            charges.wrapping_mul(stack)
        } else {
            stack
        };
        let count = if count <= 255 {
            count.wrapping_sub(1) as u8
        } else {
            254
        };
        let index = slot / 4;
        let shift = (slot % 4) * 8;
        let value = (self.applications[index] & !(255 << shift)) | u32::from(count) << shift;
        objects.set_update_value(
            target,
            UNIT_FIELD_AURAAPPLICATIONS + index,
            &mut self.applications[index],
            value,
        )?;
        Ok(())
    }
}

/// Live operands of Unit.cpp:4855 for the creature self-aura path.
pub struct SelfAuraApplication<'a> {
    pub unit: SelfAuraTarget<'a>,
    pub auras: &'a mut SourceUnitAuras,
    pub visibility: &'a mut SourceUnitVisibility,
    pub resistances: &'a mut SourceUnitResistances,
    pub damage: DamageAuraTarget<'a>,
    pub can_modify_stats: bool,
    pub threat_modifier: &'a mut [f32; 7],
    pub power_fields: &'a mut Option<SourceUnitPowerFields>,
    pub power_type: Option<&'a mut u8>,
    pub vitals: &'a mut Option<SourceUnitVitals>,
    pub form: &'a mut u32,
}

pub trait SelfAuraApplicationOwner: SpellExecutionOwner {
    fn self_aura_application(&mut self) -> Result<SelfAuraApplication<'_>>;

    fn cast_aura_boost(&mut self, _caster: ObjectHandle, _spell: u32, _aura: u32) -> Result<()> {
        Err(refuse("execution.spells.Aura_4291.CastSpell_owner"))
    }
}

#[derive(Clone, Copy)]
pub(crate) enum SelfAuraApplyHandler {
    Null,
    Dummy,
    Threat,
    Stealth,
    StealthDetect,
    Invisibility,
    InvisibilityDetect,
    Resistance,
    PeriodicTrigger,
    Shapeshift,
    ProcTrigger,
    DamagePercent,
    NoImmediateEffect,
    Ghost,
}

impl SelfAuraApplyHandler {
    // SpellAuras.cpp:61-252. Only types with a bound apply path appear here.
    pub(crate) fn for_kind(kind: u32) -> Option<Self> {
        Some(match kind {
            0 | 192..=255 => Self::Null,
            4 => Self::Dummy,
            10 => Self::Threat,
            15 => Self::NoImmediateEffect, // SpellAuras.cpp:76.
            16 => Self::Stealth,           // SpellAuras.cpp:77.
            17 => Self::StealthDetect,     // SpellAuras.cpp:78.
            18 => Self::Invisibility,
            19 => Self::InvisibilityDetect, // SpellAuras.cpp:80.
            22 => Self::Resistance,
            23 => Self::PeriodicTrigger,
            36 => Self::Shapeshift,
            42 => Self::ProcTrigger,
            79 => Self::DamagePercent,
            87 => Self::NoImmediateEffect, // SpellAuras.cpp:148.
            95 => Self::Ghost,
            126 => Self::NoImmediateEffect, // SpellAuras.cpp:187.
            _ => return None,
        })
    }
}

/// Unit.cpp:4855-4865; SpellAuras.cpp:5224-5228. Use the inserted holder for
/// every callback, and borrow it again after a nested cast.
pub fn apply_self_aura_holder(
    owner: &mut impl SelfAuraApplicationOwner,
    stores: &SourceSpellStores,
    t: &SpellTemplate,
    holder: SourceAuraHolderId,
) -> Result<()> {
    for i in 0..3 {
        let mut live = owner.self_aura_application()?;
        let aura_holder = live.auras.applied_holder_mut(holder)?;
        if aura_holder.deleted {
            return Ok(());
        }
        let Some(aura) = aura_holder.effects[i].as_mut() else {
            continue;
        };
        // SpellAuras.cpp:679,682. Read the handler type before dispatching OnApply.
        let kind = aura.kind;
        let script = aura_holder.script;
        if let Some(script) = script {
            script.on_apply(aura, true);
        }
        let unit = live.unit;
        match SelfAuraApplyHandler::for_kind(kind) {
            Some(SelfAuraApplyHandler::Null) => {} // SpellAuras.cpp:61 dispatches type 0 to HandleNULL.
            // SpellAuras.h:283-286. Later damage calculations inspect the aura list.
            Some(SelfAuraApplyHandler::NoImmediateEffect) => {}
            Some(SelfAuraApplyHandler::Stealth) => {
                let (misc, amount) = (aura.misc, aura.amount);
                live.auras.handle_stealth(
                    stores,
                    &mut SelfAuraRemoval {
                        unit,
                        visibility: live.visibility,
                        damage: live.damage.reborrow(*live.form),
                    },
                    misc,
                    amount,
                    true,
                    true,
                )?;
                live.auras.mark_effect_applied(holder, i)?;
                continue;
            }
            Some(SelfAuraApplyHandler::Invisibility) => {
                let (misc, amount) = (aura.misc, aura.amount);
                live.auras.handle_invisibility(
                    stores,
                    &mut SelfAuraRemoval {
                        unit,
                        visibility: live.visibility,
                        damage: live.damage.reborrow(*live.form),
                    },
                    misc,
                    amount,
                    true,
                    true,
                )?;
                live.auras.mark_effect_applied(holder, i)?;
                continue;
            }
            Some(SelfAuraApplyHandler::StealthDetect) => {
                handle_mod_stealth_detect(live.visibility, aura.misc, aura.amount, true)?;
            }
            Some(SelfAuraApplyHandler::InvisibilityDetect) => {
                let (misc, amount) = (aura.misc, aura.amount);
                live.auras.handle_invisibility_detect(
                    &mut SelfAuraRemoval {
                        unit,
                        visibility: live.visibility,
                        damage: live.damage.reborrow(*live.form),
                    },
                    misc,
                    amount,
                    true,
                    true,
                    None,
                )?;
                live.auras.mark_effect_applied(holder, i)?;
                continue;
            }
            Some(SelfAuraApplyHandler::Resistance) => live.resistances.handle_aura(
                unit.objects,
                unit.target,
                aura.amount,
                aura.misc,
                true,
                live.can_modify_stats,
                unit.player,
            )?,
            Some(SelfAuraApplyHandler::DamagePercent) => {
                handle_damage_percent_done(unit, &mut live.damage, t, aura, None, true)?
            }
            Some(SelfAuraApplyHandler::Ghost) => {
                live.auras
                    .handle_aura_ghost(unit.objects, unit.target, true, unit.player)?;
                live.auras.mark_effect_applied(holder, i)?;
                continue;
            }
            Some(SelfAuraApplyHandler::Shapeshift) => {
                let misc = aura.misc;
                if let Some(spell) = live.apply_shape(stores, holder, misc)? {
                    // SpellAuras.cpp:4291-4295. The outer holder is still unready.
                    let caster = unit.target;
                    owner.cast_aura_boost(caster, spell, holder.spell())?;
                }
                owner
                    .self_aura_application()?
                    .auras
                    .mark_effect_applied(holder, i)?;
                continue;
            }
            Some(SelfAuraApplyHandler::Threat) => {
                // SpellAuras.cpp:2811-2843; Util.h:156-160.
                if unit.objects.get(unit.target)?.alive {
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
            Some(SelfAuraApplyHandler::PeriodicTrigger) => {
                // SpellAuras.cpp:3097-3117 sets periodic and skips removal-only casts.
                aura.periodic = true;
            }
            None if kind == 65 => {
                return Err(refuse(if aura.amount > 0 {
                    "execution.spells.Unit_10496.ApplyPercentModFloatValue_owner"
                } else {
                    "execution.spells.Unit_10498.ApplyPercentModFloatValue_owner"
                }));
            }
            Some(SelfAuraApplyHandler::ProcTrigger) => {
                live.auras
                    .handle_aura_proc_trigger_spell(unit.objects, holder, true, true)?;
                live.auras.mark_effect_applied(holder, i)?;
                continue;
            }
            Some(SelfAuraApplyHandler::Dummy) => {
                handle_creature_dummy_aura(stores, t, unit.player, None)?
            }
            None => {
                return Err(refuse(&format!(
                    "execution.spells.AuraHandler.type_{kind}_owner"
                )))
            }
        }
        // SpellAuras.cpp:688. The handler may have changed the aura.
        if let Some(script) = script {
            script.on_after_apply(aura, true);
        }
        aura.applied = true;
    }
    owner
        .self_aura_application()?
        .auras
        .apply_holder_boosts(stores, t, holder)
}
// SpellAuras.cpp:2621-2624; ObjectVisibility.h:80 adds an unsigned value.
fn handle_mod_stealth_detect(
    visibility: &mut SourceUnitVisibility,
    misc: i32,
    amount: i32,
    apply: bool,
) -> Result<()> {
    let delta = if apply {
        amount
    } else {
        amount
            .checked_neg()
            .ok_or_else(|| refuse("execution.spells.Aura_2623.stealth_detect_negation_undefined"))?
    };
    let value = visibility
        .stealth_detect
        .get_mut(misc as u32 as usize)
        .ok_or_else(|| {
            refuse("execution.spells.ObjectVisibility_80.stealth_detect_index_undefined")
        })?;
    *value = value.wrapping_add(delta as u32);
    Ok(())
}

// ObjectVisibility.cpp:133-146; Relations.cpp:72-91; ObjectAccessor.cpp:77-88.
fn effective_invisibility_detect_value(
    unit: SelfAuraTarget<'_>,
    flags: u32,
    visibility: &SourceUnitVisibility,
    index: usize,
) -> Result<i32> {
    if flags & 8 != 0 {
        let master = unit.control_guids.master_guid();
        let controller = if master.raw() == 0 {
            unit.player
        } else {
            master.is_player() && unit.objects.find_player(master)?.is_some()
        };
        require(
            !controller,
            "execution.spells.ObjectVisibility_142.controller_detect_value_owner",
        )?;
    }
    Ok(visibility.detect_values[index])
}

// SpellAuras.cpp:4114-4177. The magic multiplier is read later from the aura list.
fn handle_damage_percent_done(
    unit: SelfAuraTarget<'_>,
    damage: &mut DamageAuraTarget<'_>,
    spell: &SpellTemplate,
    aura: &SourceAura,
    remove: Option<AuraRemoveMode>,
    real: bool,
) -> Result<()> {
    use super::damage::DamageAttack;
    use super::resistance::{StatModifierUnit, UnitMod};

    require(
        !real || !unit.player,
        "execution.spells.Aura_4123.GetWeaponForAttack_owner",
    )?;
    if aura.misc & 1 != 0 && (spell.equippeditemclass == -1 || !unit.player) {
        let target = StatModifierUnit {
            objects: unit.objects,
            unit: unit.target,
            can_modify: damage.can_modify_stats,
            player: unit.player,
            totem: unit.totem,
        };
        for attack in [
            DamageAttack::Mainhand,
            DamageAttack::Offhand,
            DamageAttack::Ranged,
        ] {
            target.handle_stat_modifier(
                UnitMod::Damage(damage.damage, attack, damage.weapon),
                ResistanceModifier::TotalPercent,
                aura.amount as f32,
                remove.is_none(),
            )?;
        }
        require(
            !unit.player,
            "execution.spells.Aura_4147.ApplyModSignedFloatValue_owner",
        )?;
    }
    if aura.misc & 0x7e == 0 {
        return Ok(());
    }
    if spell.equippeditemclass != -1 || spell.equippediteminventorytypemask != 0 {
        return Ok(());
    }
    require(
        !unit.player,
        "execution.spells.Aura_4173.ApplyModSignedFloatValue_owner",
    )?;
    require(
        remove != Some(AuraRemoveMode::Expire) || spell.id != 30423,
        "execution.spells.Aura_4177.CastSpell_38639_owner",
    )
}

impl SourceUnitAuras {
    fn applied_holder_mut(&mut self, holder: SourceAuraHolderId) -> Result<&mut SourceAuraHolder> {
        self.retained_aura_holder_mut(holder)
            .ok_or_else(|| refuse("execution.spells.Unit_4855.aura_holder_owner"))
    }

    fn mark_effect_applied(&mut self, holder: SourceAuraHolderId, index: usize) -> Result<()> {
        let holder = self.applied_holder_mut(holder)?;
        if let Some(aura) = holder.effects[index].as_mut() {
            if let Some(script) = holder.script {
                script.on_after_apply(aura, true);
            }
            aura.applied = true;
        }
        Ok(())
    }

    // SpellAuras.cpp:3067-3082,5740-5753. Creature effects have no player spell
    // modifiers (SpellAuras.cpp:901-904).
    fn handle_aura_proc_trigger_spell(
        &mut self,
        objects: &SourceObjects,
        id: SourceAuraHolderId,
        apply: bool,
        real: bool,
    ) -> Result<()> {
        if !real || id.spell() != 28200 || !apply {
            return Ok(());
        }
        let holder = self.applied_holder_mut(id)?;
        if holder.charges != 6 {
            holder.charges = 6;
            let (target, slot, stack) = (holder.target, holder.slot, holder.stack);
            self.aura_fields
                .update_aura_application(objects, target, slot, 6, stack)?;
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
        // Unit.cpp:4860-4865.
        self.handle_spell_specific_boosts(stores, t, None)?;
        if heartbeat_aura(stores, t) {
            self.heartbeat_proc_counter = self.heartbeat_proc_counter.wrapping_add(1);
        }
        Ok(())
    }

    // SpellAuras.cpp:5572-5652.
    fn handle_spell_specific_boosts(
        &mut self,
        stores: &SourceSpellStores,
        t: &SpellTemplate,
        mut remove: Option<&mut SelfAuraRemoval<'_>>,
    ) -> Result<()> {
        for spell in spell_specific_boosts(t) {
            let Some(live) = remove.as_deref_mut() else {
                return Err(refuse(
                    "execution.spells.SpellAuraHolder_5648.boost_CastSpell_owner",
                ));
            };
            require(
                stores.template(spell).is_some(),
                "execution.spells.SpellAuraHolder_5633.boost_template_null_undefined",
            )?;
            // Either argument of the caster-GUID choice names the self caster.
            self.remove_self_auras_by_spell(stores, spell, Some(live.unit.target), live)?;
        }
        Ok(())
    }

    // SpellAuras.cpp:2568-2619. Strength changes before the other aura effects.
    fn handle_stealth(
        &mut self,
        stores: &SourceSpellStores,
        live: &mut SelfAuraRemoval<'_>,
        misc: i32,
        amount: i32,
        apply: bool,
        real: bool,
    ) -> Result<()> {
        use SourceUnitVisibilityMode::*;
        let delta = if apply {
            amount
        } else {
            amount.checked_neg().ok_or_else(|| {
                refuse("execution.spells.Aura_2572.stealth_amount_negation_undefined")
            })?
        };
        let strength = live
            .visibility
            .stealth_strength
            .get_mut(misc as u32 as usize)
            .ok_or_else(|| {
                refuse("execution.spells.ObjectVisibility_79.stealth_index_undefined")
            })?;
        // ObjectVisibility.h:79 adds unsigned strength modulo 2^32.
        *strength = strength.wrapping_add(delta as u32);

        let unit = live.unit;
        if apply {
            // SpellDefines.h:382.
            self.remove_auras_with_interrupt_flags(stores, live, 0x0010_0000)?;
            if real {
                // Unit.h:124,1443 sets creep through the update field.
                let flags = self.vis_flags | 2;
                unit.objects.set_update_value(
                    unit.target,
                    UNIT_FIELD_BYTES_1,
                    &mut self.vis_flags,
                    flags,
                )?;
                require(!unit.player, "execution.spells.Aura_2585.SetByteFlag_owner")?;
                if live.visibility.mode != Off {
                    live.visibility.set_visibility(unit, GroupNoDetect)?;
                    live.visibility.set_visibility(unit, GroupStealth)?;
                }
            }
        } else if real && !self.has_aura_type(16) && live.visibility.mode != Off {
            // Unit.h:124,1444 clears creep through the update field.
            let flags = self.vis_flags & !2;
            unit.objects.set_update_value(
                unit.target,
                UNIT_FIELD_BYTES_1,
                &mut self.vis_flags,
                flags,
            )?;
            require(
                !unit.player,
                "execution.spells.Aura_2606.RemoveByteFlag_owner",
            )?;
            if self.has_aura_type(18) {
                live.visibility.set_visibility(unit, GroupNoDetect)?;
                live.visibility.set_visibility(unit, GroupInvisibility)?;
            } else {
                live.visibility.set_visibility(unit, On)?;
            }
        }
        Ok(())
    }

    // SpellAuras.cpp:2631-2679. The running sum, not `apply`, selects the branch.
    fn handle_invisibility(
        &mut self,
        stores: &SourceSpellStores,
        live: &mut SelfAuraRemoval<'_>,
        misc: i32,
        amount: i32,
        apply: bool,
        real: bool,
    ) -> Result<()> {
        let delta = if apply {
            amount
        } else {
            amount.checked_neg().ok_or_else(|| {
                refuse("execution.spells.Aura_2635.invisibility_amount_negation_undefined")
            })?
        };
        let index = misc as u32 as usize;
        let value = live.visibility.values.get_mut(index).ok_or_else(|| {
            refuse("execution.spells.ObjectVisibility_71.invisibility_index_undefined")
        })?;
        *value = value.checked_add(delta).ok_or_else(|| {
            refuse("execution.spells.ObjectVisibility_71.invisibility_addition_undefined")
        })?;
        let bit = 1u32 << index;
        if *value > 0 {
            live.visibility.mask |= bit;
            // SpellDefines.h:382.
            self.remove_auras_with_interrupt_flags(stores, live, 0x0010_0000)?;
            require(
                !real || !live.unit.player,
                "execution.spells.Aura_2647.GetMover_owner",
            )?;
            if live.visibility.mode == SourceUnitVisibilityMode::On {
                live.visibility.mode = SourceUnitVisibilityMode::GroupInvisibility;
            }
        } else {
            live.visibility.mask &= !bit;
            if real && live.visibility.mask == 0 {
                require(
                    !live.unit.player,
                    "execution.spells.Aura_2666.RemoveByteFlag_owner",
                )?;
                // SpellAuraDefines.h:111: SPELL_AURA_MOD_STEALTH = 16.
                if live.visibility.mode != SourceUnitVisibilityMode::Off && !self.has_aura_type(16)
                {
                    live.visibility.mode = SourceUnitVisibilityMode::On;
                }
            }
        }
        require(
            !live.unit.objects.get(live.unit.target)?.in_world,
            "execution.spells.Aura_2679.UpdateVisibilityAndView_owner",
        )
    }

    // SpellAuras.cpp:2682-2722.
    fn handle_invisibility_detect(
        &self,
        live: &mut SelfAuraRemoval<'_>,
        misc: i32,
        amount: i32,
        apply: bool,
        real: bool,
        detached: Option<(SourceAuraHolderId, &[Option<SourceAura>; 3])>,
    ) -> Result<()> {
        let index = misc as u32 as usize;
        require(
            index < u32::BITS as usize && index < live.visibility.detect_values.len(),
            "execution.spells.ObjectVisibility_110.invisibility_detect_index_undefined",
        )?;
        let bit = 1u32 << index;
        if apply {
            live.visibility.detect_mask |= bit;
            if effective_invisibility_detect_value(
                live.unit,
                live.damage.weapon.flags,
                live.visibility,
                index,
            )? <= amount
            {
                live.visibility.detect_values[index] = amount;
            }
        } else {
            let mut highest = 0;
            for effect in self.auras_of_type(19) {
                let effects = match detached {
                    Some((id, effects)) if id == effect.holder => Some(effects),
                    _ => self
                        .aura_holder(effect.holder)
                        .map(|holder| &holder.effects),
                };
                let remaining = effects
                    .and_then(|effects| effects.get(usize::from(effect.effect)))
                    .and_then(Option::as_ref)
                    .ok_or_else(|| refuse("execution.spells.Aura_2704.GetModifier_owner"))?;
                if remaining.misc == misc
                    && remaining.amount != amount
                    && remaining.amount > highest
                {
                    highest = remaining.amount;
                }
            }
            if highest == 0 {
                live.visibility.detect_mask &= !bit;
            }
            live.visibility.detect_values[index] = highest;
        }
        require(
            !real || !live.unit.player,
            "execution.spells.Aura_2721.UpdateVisibilityForOwner_owner",
        )
    }

    // SpellAuras.cpp:4401-4429.
    fn handle_aura_ghost(
        &mut self,
        objects: &SourceObjects,
        target: ObjectHandle,
        apply: bool,
        player: bool,
    ) -> Result<()> {
        // Unit.h:1443-1444; Object.cpp:1008,1026 test one fixed bit.
        let old = self.vis_flags;
        let flags = if apply { old | 1 } else { old & !1 };
        objects.set_update_value(target, UNIT_FIELD_BYTES_1, &mut self.vis_flags, flags)?;
        require(
            !player,
            if apply {
                "execution.spells.Aura_4411.player_ghost_flag_owner"
            } else {
                "execution.spells.Aura_4421.player_ghost_flag_owner"
            },
        )
    }
}

impl SourceUnitVisibility {
    // Unit.cpp:8329-8335 writes the visibility mode before any in-world update.
    fn set_visibility(
        &mut self,
        unit: SelfAuraTarget<'_>,
        mode: SourceUnitVisibilityMode,
    ) -> Result<()> {
        self.mode = mode;
        require(
            !unit.objects.get(unit.target)?.in_world,
            "execution.spells.Unit_8334.UpdateVisibilityAndView_owner",
        )
    }
}

// SpellAuras.cpp:5572-5624. Default families and IDs exit before polymorph.
fn spell_specific_boosts(t: &SpellTemplate) -> impl Iterator<Item = u32> + '_ {
    let (boosts, eligible): (&[u32], _) = match (t.spellfamilyname, t.id) {
        (0, 20594) => (&[20612], true),
        (3, _) => (&[], true),
        (9, 19574) => (&[24395, 24396, 24397, 26592], true),
        _ => (&[], false),
    };
    boosts
        .iter()
        .copied()
        .chain((eligible && t.mechanic == 17).then_some(12939))
}

// SpellAuras.cpp:1231-1629.
fn handle_creature_dummy_aura(
    stores: &SourceSpellStores,
    t: &SpellTemplate,
    player: bool,
    remove: Option<(AuraRemoveMode, bool)>,
) -> Result<()> {
    if let Some((mode, alive)) = remove {
        if t.effect1 == 63 && t.effect2 == 6 && t.effectapplyauraname2 == 4 && alive {
            // Spell.cpp:106-114; SpellAuras.cpp:1359-1385.
            return require(
                !matches!(t.id, 19548 | 19674 | 19687..=19689 | 19692..=19694 | 19696..=19697 | 19699 | 19700),
                "execution.spells.Aura_1383.quest_tame_CastSpell_owner",
            );
        }
        let site = match t.id {
            11129 => Some("execution.spells.Aura_1391.RemoveAurasDueToSpell_owner"),
            11826 if mode == AuraRemoveMode::Expire && player => {
                Some("execution.spells.Aura_1400.CastSpell_owner")
            }
            12774 if mode != AuraRemoveMode::Death => {
                Some("execution.spells.Aura_1410.CastSpell_owner")
            }
            17189 => Some("execution.spells.Aura_1417.ModifyHealth_owner"),
            25185 => Some("execution.spells.Aura_1421.CastSpell_owner"),
            26077 => Some("execution.spells.Aura_1426.CastSpell_owner"),
            29104 if mode == AuraRemoveMode::Death && player => {
                Some("execution.spells.Aura_1432.CastSpell_owner")
            }
            30238 => Some("execution.spells.Aura_1437.RemoveAurasDueToSpell_owner"),
            11826 | 12774 | 29104 => return Ok(()),
            _ if mode == AuraRemoveMode::Death
                && t.spellfamilyname == 3
                && t.spellfamilyflags & 0x800 != 0 =>
            {
                Some("execution.spells.Aura_1448.InterruptSpell_owner")
            }
            _ => None,
        };
        if let Some(site) = site {
            return Err(refuse(site));
        }
    } else {
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
            ),
            4 => t.id == 23427,
            6 => t.id == 30238,
            _ => false,
        };
        require(
            !specific,
            "execution.spells.Aura_1231.HandleAuraDummy_specific_owner",
        )?;
    }
    if let Some(site) = match (t.spellfamilyname, t.id) {
        (0, 6606) => Some("execution.spells.Aura_1472.clearUnitState_owner"),
        (0, 16093) => Some("execution.spells.Aura_1487.clearUnitState_owner"),
        (0, 24658) => Some("execution.spells.Aura_1504.RemoveAurasDueToSpell_owner"),
        (0, 24661) => Some("execution.spells.Aura_1517.RemoveAurasDueToSpell_owner"),
        (0, 28819) => Some("execution.spells.Aura_1530.SetStandState_owner"),
        (0, 29266) => Some("execution.spells.Aura_1540.SetFeignDeath_owner"),
        (7, _) if player && t.spelliconid == 1563 => {
            Some("execution.spells.Aura_1552.UpdateAttackPowerAndDamage_owner")
        }
        (10, 21082 | 20162 | 20305..=20308) => {
            Some("execution.spells.Aura_1571.ApplyPercentModFloatVar_owner")
        }
        _ => None,
    } {
        return Err(refuse(if remove.is_some() {
            site
        } else {
            "execution.spells.Aura_1231.HandleAuraDummy_specific_owner"
        }));
    }
    if stores.pet_aura(t.id).is_some() {
        return Err(refuse(if remove.is_some() {
            "execution.spells.Aura_1585.RemovePetAura_owner"
        } else {
            "execution.spells.Aura_1583.AddPetAura_owner"
        }));
    }
    require(
        !player,
        "execution.spells.Aura_1591.GetSpellAreaForAuraMapBounds_owner",
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
            !self.unit.player,
            "execution.spells.Aura_1890.tactical_mastery_owner",
        )?;
        self.clear_stance_rage()?;
        // SpellAuras.cpp:1920-1936; SpellDefines.h:377.
        self.unit.objects.set_update_value(
            self.unit.target,
            UNIT_FIELD_BYTES_1,
            self.form,
            form,
        )?;
        self.auras.remove_auras_with_interrupt_flags(
            stores,
            &mut SelfAuraRemoval {
                unit: self.unit,
                visibility: self.visibility,
                damage: self.damage.reborrow(*self.form),
            },
            0x0000_8000,
        )?;
        Ok(Some(boost)) // SpellAuras.cpp:4263-4271.
    }

    fn set_stance_power_type(&mut self) -> Result<()> {
        let power_type = self
            .power_type
            .as_deref_mut()
            .ok_or_else(|| refuse("execution.spells.Aura_1885.GetPowerType_owner"))?;
        let unit = self.unit;
        if unit
            .objects
            .set_update_value(unit.target, UNIT_FIELD_BYTES_0, power_type, 1)?
        {
            if let Some(vitals) = self.vitals {
                vitals.has_mana = false;
            }
            self.require_no_power_group("execution.spells.Unit_6184.GetGroup_owner")?;
            require(
                unit.objects.get(unit.target)?.kind == SourceObjectKind::Creature,
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
        let unit = self.unit;
        if unit
            .objects
            .set_update_value(unit.target, UNIT_FIELD_POWER2, rage, 0)?
        {
            self.require_no_power_group("execution.spells.Unit_9294.GetGroup_owner")?;
        }
        Ok(())
    }

    fn require_no_power_group(&self, refusal: &str) -> Result<()> {
        let live = self.unit;
        require(!live.player, refusal)?;
        if live.control_guids.has_charmer() {
            if let Some(unit) = live
                .control_guids
                .get_charmer(live.objects, live.target, None, unbound_pet_loading)?
                .unit()
            {
                require(
                    live.objects.get(unit)?.kind != SourceObjectKind::Player,
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
            &mut SelfAuraRemoval {
                unit: SelfAuraTarget {
                    target: ObjectHandle {
                        thread: 1,
                        construction: 1,
                    },
                    level: Some(1),
                    alive: true,
                    player: false,
                    player_group: None,
                    totem: false,
                    objects: &SourceObjects::default(),
                    control_guids: &SourceControlGuids::default(),
                },
                visibility: &mut SourceUnitVisibility::default(),
                damage: DamageAuraTarget::new(
                    &mut SourceCreatureDamage::default(),
                    &SourceLoadedCreatureFields::default(),
                    0,
                    0,
                ),
            },
            0,
        )
        .unwrap_err()
        .id;
    assert_eq!(id, "execution.spells.Unit_4687.pending_aura_holder_owner");
    assert_eq!(auras, SourceUnitAuras::default());
}

#[test]
fn proc_trigger_aura_sets_ascendance_charges_only_on_real_apply() {
    let mut auras = SourceUnitAuras::default();
    let mut objects = SourceObjects::default();
    let target = objects.construct(1, SourceObjectKind::Creature).unwrap();
    for id in [7276, 28200] {
        let t = SpellTemplate {
            id,
            ..Default::default()
        };
        let mut holder = SourceAuraHolder::new(&t, target, 0, 0, -1).unwrap();
        holder.charges = 1;
        holder.stack = 2;
        holder.slot = 1;
        let holder = auras.store_aura_holder(id, &mut Some(holder)).unwrap();
        auras.aura_fields.applications[0] = 0x3412_0156;
        for (apply, real) in [(false, false), (true, false), (false, true), (true, true)] {
            auras
                .handle_aura_proc_trigger_spell(&objects, holder, apply, real)
                .unwrap();
            if id == 28200 && apply && real {
                assert_eq!(auras.aura_holder(holder).unwrap().charges, 6);
                assert_eq!(auras.aura_fields.applications[0], 0x3412_0b56);
            } else {
                assert_eq!(auras.aura_holder(holder).unwrap().charges, 1);
                assert_eq!(auras.aura_fields.applications[0], 0x3412_0156);
            }
        }
    }
}

#[cfg(test)]
#[path = "aura_mark_tests.rs"]
mod aura_mark_tests;

#[cfg(test)]
#[path = "visibility_aura_tests.rs"]
mod visibility_aura_tests;

#[cfg(test)]
#[path = "damage_aura_tests.rs"]
mod damage_aura_tests;
