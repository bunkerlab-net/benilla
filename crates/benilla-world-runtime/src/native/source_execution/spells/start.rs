// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Spell.cpp:2981-3098,3101-3157,3725-3742.
use super::*;
use rustc_apfloat::{ieee::Single, Float, Round, Status};

pub trait SpellEventQueue {
    fn add_spell(&mut self, spell: u64) -> Result<u64>;
}

#[must_use = "dispatch the immediate cast before returning from SpellStart"]
pub(in crate::native::source_execution) struct ScheduledSpellStart {
    result: SpellCastResult,
    immediate: Option<bool>,
}
impl ScheduledSpellStart {
    pub(in crate::native::source_execution) fn dispatch(
        self,
        cast: impl FnOnce(bool) -> Result<()>,
    ) -> Result<SpellCastResult> {
        if let Some(skip) = self.immediate {
            cast(skip)?;
        }
        Ok(self.result)
    }
}

impl SourceSpell {
    /// Loaded script lookup precedes source constructor initialization.
    pub fn construct(
        id: u64,
        stores: &SourceSpellStores,
        spell: u32,
        caster: ObjectHandle,
        original: Option<ObjectHandle>,
        flags: SpellTriggerFlags,
        owner: &mut (impl SpellExecutionOwner + ?Sized),
    ) -> Result<Self> {
        // Spell.cpp:388. Load the two bases independently before OnInit.
        let untyped = "execution.spells.Spell_388.script_binding_owner";
        let spell_script = stores.typed_script(SpellScriptKind::Spell, spell, untyped)?;
        let aura_script = stores.typed_script(SpellScriptKind::Aura, spell, untyped)?;
        let t = stores
            .template(spell)
            .ok_or_else(|| refuse("execution.spells.missing_template"))?;
        require(
            t.school < 32,
            "execution.spells.GetSchoolMask_shift_undefined",
        )?;
        let c = owner.unit(caster)?;
        let mut school = 1u32 << t.school;
        if attack_type(t) == 2 && !c.gameobject {
            // Spell.cpp:414 evaluates getClassMask before IsPlayer.
            // Unit.h:1318; FlagConvertibleEnum.h:56-58.
            require(
                (1..=32).contains(&c.class),
                "execution.spells.getClassMask_shift_undefined",
            )?;
            if matches!(c.class, 5 | 8 | 9) && c.player {
                school = owner.wand_school(caster)?;
            }
        }
        let original = original.unwrap_or(caster);
        let affective = if original == caster && !c.gameobject {
            Some(caster)
        } else {
            owner.original_caster(caster, original)?
        };
        let mut value = Self::new(id, t, caster, flags, 0)?;
        value.original_caster = original;
        value.affective_caster = affective;
        value.school_mask = school;
        value.creation_time = owner.map_time(caster)?;
        value.spell_script = spell_script;
        value.aura_script = aura_script;
        value.reflectable = Some(
            t.dmgclass == 1
                && t.attributes & 0x20000050 == 0
                && t.attributesex & 0x80 == 0
                && !owner.positive_definition(t)?,
        );
        // SpellTargets.cpp:503-505 returns exactly 10; only the loaded map is missing.
        value.filtering_scheme = Some(owner.targeting_data(spell)?);
        require(
            value.spell_script.is_none(),
            "execution.spells.Spell_504.OnInit_owner",
        )?;
        Ok(value)
    }

    /// Spell.cpp:795-805. Compute trigger flags and effect polarity for an itemless cast.
    pub fn prepare_trigger_data(&mut self, stores: &SourceSpellStores) -> Result<()> {
        let t = stores
            .template(self.spell)
            .ok_or_else(|| refuse("execution.spells.missing_template"))?;
        let mut negative_effect_mask = 0;
        for i in 0..3 {
            if !positive_unit_effect(stores, t, i, None)? {
                negative_effect_mask |= 1 << i;
            }
        }
        self.trigger_data = Some(SpellTriggerData {
            can_trigger: !self.flags.do_not_proc(),
            negative_effect_mask,
        });
        Ok(())
    }

    /// Spell.cpp:3033,7105-7108; SpellMgr.cpp:104-172. Read creature cast-time operands lazily.
    pub fn creature_cast_time(
        &self,
        t: &SpellTemplate,
        stores: &SourceSpellStores,
        mod_owner: Option<ObjectHandle>,
        level: impl FnOnce() -> Result<u32>,
        speed: impl FnOnce() -> Result<u32>,
    ) -> Result<i32> {
        require(
            mod_owner.is_none(),
            "execution.spells.Spell_3033.SpellModRAII_owner",
        )?;
        fresh_cast_time(
            |index| stores.dbc("SpellCastTimes.dbc", index),
            self,
            t,
            level,
            speed,
        )
    }

    pub fn start(
        &mut self,
        t: &SpellTemplate,
        targets: SpellTargets,
        aura: Option<u32>,
        queue: &mut impl SpellEventQueue,
        spells: &mut BTreeMap<u64, SourceSpell>,
        owner: &mut (impl SpellExecutionOwner + ?Sized),
    ) -> Result<SpellCastResult> {
        self.schedule_start(targets, aura, queue)?;
        // Spell.cpp:3018-3020,3083,3094. Dispatch the tail cast before returning.
        self.start_scheduled(t, spells, owner)?
            .dispatch(|skip| owner.call(self, SpellCall::Cast(skip)))
    }
    // Split at AddEvent so a synchronous cast can start another spell on the same queue.
    pub(in crate::native::source_execution) fn schedule_start(
        &mut self,
        targets: SpellTargets,
        aura: Option<u32>,
        queue: &mut impl SpellEventQueue,
    ) -> Result<()> {
        require(
            self.state == SourceSpellState::Created,
            "execution.spells.SpellStart_state",
        )?;
        self.state = SourceSpellState::Targeting;
        self.targets = targets;
        self.triggered_by_aura = aura;
        // :3007-3009. A failed CheckCast still owns the queued SpellEvent.
        self.event = Some(queue.add_spell(self.id)?);
        Ok(())
    }
    pub(in crate::native::source_execution) fn start_scheduled(
        &mut self,
        t: &SpellTemplate,
        spells: &mut BTreeMap<u64, SourceSpell>,
        owner: &mut (impl SpellExecutionOwner + ?Sized),
    ) -> Result<ScheduledSpellStart> {
        let result = self.check_cast(t, true, owner)?;
        if result != SpellCastResult::Ok && (!self.auto_repeat || self.flags.autorepeat()) {
            owner.call(self, SpellCall::SendResult(result))?;
            self.finish_failed(owner)?;
            return Ok(ScheduledSpellStart {
                result,
                immediate: None,
            });
        }
        // Spell.cpp:3018-3020. The caller dispatches Prepare's tail cast.
        Ok(ScheduledSpellStart {
            result: SpellCastResult::Ok,
            immediate: self.prepare_cast(t, spells, owner)?,
        })
    }
    pub fn prepare(
        &mut self,
        t: &SpellTemplate,
        spells: &mut BTreeMap<u64, SourceSpell>,
        owner: &mut (impl SpellExecutionOwner + ?Sized),
    ) -> Result<()> {
        if let Some(skip) = self.prepare_cast(t, spells, owner)? {
            owner.call(self, SpellCall::Cast(skip))?;
        }
        Ok(())
    }

    // Spell.cpp:3083,3094. Defer only the tail cast so it can borrow the
    // same live store after SetCurrentCastedSpell.
    fn prepare_cast(
        &mut self,
        t: &SpellTemplate,
        spells: &mut BTreeMap<u64, SourceSpell>,
        owner: &mut (impl SpellExecutionOwner + ?Sized),
    ) -> Result<Option<bool>> {
        self.state = SourceSpellState::Casting;
        owner.call(self, SpellCall::PrepareTriggerData)?;
        if !self.flags.instant() {
            self.cast_time = owner.cast_time(self, t)?;
        }
        self.timer = self.cast_time.max(0) as u32;
        let c = owner.unit(self.caster)?;
        if !self.flags.old_triggered() && !c.gameobject {
            if self.client_cast && self.item_cast {
                owner.call(self, SpellCall::RemoveCastAuras(0x2000))?;
            }
            if t.attributesex2 & 0x10000000 == 0 {
                owner.call(self, SpellCall::RemoveCastAuras(4))?;
            }
            if self.flags.notify_ai() && owner.unit(self.caster)?.has_ai {
                owner.call(self, SpellCall::AiCastState(true))?;
            }
        }
        self.cast_position = owner.unit(self.caster)?.position;
        require(
            self.spell_script.is_none(),
            "execution.spells.Spell_3058.OnSuccessfulStart_owner",
        )?;
        if !c.gameobject && t.attributesex & 1 != 0 {
            owner.call(self, SpellCall::UnsummonPet)?;
        }
        if !self.flags.old_triggered() {
            if !c.gameobject {
                if (!self.flags.ignore_current(t) || is_channel(t)) && !self.flags.autorepeat() {
                    owner.set_current(self, t, spells)?;
                }
                owner.add_gcd(self, t)?;
            }
            owner.call(self, SpellCall::SendStart)?;
            if self.timer == 0 && !is_melee(t) && (!self.auto_repeat || self.flags.autorepeat()) {
                return Ok(Some(false));
            }
        } else {
            if is_channel(t) {
                require(
                    !c.gameobject,
                    "execution.spells.triggered_GO_channel_null_caster_undefined",
                )?;
                owner.set_current(self, t, spells)?;
            }
            if self.timer == 0 {
                return Ok(Some(true));
            }
        }
        Ok(None)
    }
    pub fn cancel(
        &mut self,
        t: &SpellTemplate,
        owner: &mut (impl SpellExecutionOwner + ?Sized),
    ) -> Result<()> {
        if self.state == SourceSpellState::Finished {
            return Ok(());
        }
        // :3109 is reached before the state switch, even for traveling spells.
        require(
            !owner.unit(self.caster)?.gameobject,
            "execution.spells.cancel_null_unit_caster_undefined",
        )?;
        owner.call(self, SpellCall::ResetMods)?;
        let send_result = !(is_channel(t) || self.auto_repeat);
        self.auto_repeat = false;
        match self.state {
            SourceSpellState::Created | SourceSpellState::Targeting | SourceSpellState::Casting => {
                owner.reset_gcd(self, t)?;
                owner.call(self, SpellCall::Interrupted)?;
                if send_result {
                    owner.call(self, SpellCall::SendResult(SpellCastResult::Interrupted))?;
                }
            }
            SourceSpellState::Traveling => {
                owner.call(self, SpellCall::Interrupted)?;
                if send_result {
                    owner.call(self, SpellCall::SendResult(SpellCastResult::Interrupted))?;
                }
            }
            SourceSpellState::Landing | SourceSpellState::Channeling => {
                owner.call(self, SpellCall::ChannelCancel)?;
                owner.call(self, SpellCall::Interrupted)?;
                if send_result {
                    owner.call(self, SpellCall::SendResult(SpellCastResult::Interrupted))?;
                }
            }
            SourceSpellState::Delayed | SourceSpellState::Finished => {}
        }
        self.finish_failed(owner)?;
        owner.call(self, SpellCall::RemoveOwnedObjects)
    }
    /// Spell.cpp:6803-6839,6842-6882; Spell.h:826. The RNG owner is called once.
    pub fn pushback(
        &mut self,
        t: &SpellTemplate,
        channel: bool,
        owner: &mut (impl SpellExecutionOwner + ?Sized),
    ) -> Result<u32> {
        if !owner.unit(self.caster)?.player {
            return Ok(0);
        }
        if channel {
            if self.state != SourceSpellState::Channeling {
                return Ok(0);
            }
        } else if self.state == SourceSpellState::Traveling || t.interruptflags & 0x2 == 0 {
            return Ok(0);
        }
        if owner.resist_pushback(self)? {
            return Ok(0);
        }
        let delay = if self.damage_delay_count < 5 {
            let value = 1000 - u32::from(self.damage_delay_count) * 200;
            self.damage_delay_count += 1;
            value
        } else {
            200
        };
        let applied;
        if channel {
            if (self.timer as i32) < delay as i32 {
                applied = self.timer;
                self.timer = 0;
            } else {
                applied = delay;
                self.timer -= delay;
            }
            owner.call(self, SpellCall::ChannelPushback(applied))?;
        } else {
            let sum = (self.timer as i32)
                .checked_add(delay as i32)
                .ok_or_else(|| refuse("execution.spells.pushback_signed_add_undefined"))?;
            if sum > self.cast_time {
                applied = (self.cast_time as u32).wrapping_sub(self.timer);
                self.timer = self.cast_time as u32;
            } else {
                applied = delay;
                self.timer = self.timer.wrapping_add(delay);
            }
            owner.call(self, SpellCall::Pushback(applied))?;
        }
        Ok(applied)
    }
}

impl SourceUnitAuras {
    /// Object.cpp:3091-3099. A refused continuation retains the live depth.
    pub fn try_enter_cast(&mut self, max_casts: u32) -> bool {
        if max_casts != 0 && self.cast_counter >= max_casts {
            return false;
        }
        self.cast_counter = self.cast_counter.wrapping_add(1);
        true
    }

    /// Object.h:1153. The cast counter never decrements below zero.
    pub fn leave_cast(&mut self) {
        self.cast_counter = self.cast_counter.saturating_sub(1);
    }
}

/// Spell.cpp:7105-7108; SpellMgr.cpp:104-172. Calculate the itemless creature cast time.
pub fn fresh_cast_time<'a>(
    lookup: impl FnOnce(u32) -> Result<Option<&'a SpellDbcRecord>>,
    spell: &SourceSpell,
    t: &SpellTemplate,
    level: impl FnOnce() -> Result<u32>,
    speed: impl FnOnce() -> Result<u32>,
) -> Result<i32> {
    if spell.flags.old_triggered() && (t.manacost != 0 || t.manacostpercentage != 0) {
        return Ok(0); // Spell.cpp:7105-7108.
    }
    let Some(row) = lookup(t.castingtimeindex)? else {
        return Ok(0);
    };
    let word = |index| {
        row.words
            .get(&index)
            .copied()
            .ok_or_else(|| refuse("execution.creature_addon.SpellCastTimes_layout"))
    };
    // Mixed signed/unsigned source arithmetic converts to uint32 before the
    // implementation-defined int32 assignment on the pinned GCC target.
    let rank = creature_power_rank(level()?, t) as i32;
    let mut time = word(1)?
        .wrapping_add(word(2)?.wrapping_mul(((rank / 5) as u32).wrapping_sub(t.baselevel)))
        as i32;
    time = time.max(word(3)? as i32);
    if t.attributes & 2 != 0 && !spell.auto_repeat {
        time = add_ranged_delay(time)?;
    }
    if t.attributes & 0x0000_0030 == 0 && t.attributesex3 & 0x2000_0000 == 0 {
        time = scale_cast_time(time, speed()?)?;
    } else if t.attributes & 2 != 0 && !spell.auto_repeat {
        return Err(refuse(
            "execution.spells.SpellMgr_162.ranged_attack_speed_owner",
        ));
    }
    if t.id == 19968 {
        time = 0;
    }
    Ok(time.max(0))
}

fn add_ranged_delay(time: i32) -> Result<i32> {
    time.checked_add(500)
        .ok_or_else(|| refuse("execution.spells.SpellMgr_145.cast_time_add_overflow_undefined"))
}

fn scale_cast_time(time: i32, speed: u32) -> Result<i32> {
    let value = Single::from_i128(i128::from(time))
        .value
        .mul_r(
            Single::from_bits(u128::from(speed)),
            Round::NearestTiesToEven,
        )
        .value;
    let converted = value.to_i128_r(32, Round::TowardZero, &mut false);
    require(
        !converted.status.contains(Status::INVALID_OP),
        "execution.spells.SpellMgr_160.cast_time_float_to_int_undefined",
    )?;
    Ok(converted.value as i32)
}

/// Spell.cpp:6925-6934. A flying spell retains event ownership after slot replacement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpellSlot {
    Melee,
    Generic,
    Channel,
    AutoRepeat,
}
impl SpellSlot {
    pub fn for_spell(t: &SpellTemplate, s: &SourceSpell) -> Self {
        if is_melee(t) {
            Self::Melee
        } else if s.auto_repeat {
            Self::AutoRepeat
        } else if is_channel(t) {
            Self::Channel
        } else {
            Self::Generic
        }
    }
    fn index(self) -> usize {
        match self {
            Self::Melee => 0,
            Self::Generic => 1,
            Self::Channel => 2,
            Self::AutoRepeat => 3,
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct SourceSpellSlots {
    current: [Option<u64>; 4],
}
impl SourceSpellSlots {
    pub fn current(&self, slot: SpellSlot) -> Option<u64> {
        self.current[slot.index()]
    }
    pub fn interrupt(
        &mut self,
        slot: SpellSlot,
        delayed: bool,
        spells: &mut BTreeMap<u64, SourceSpell>,
        stores: &SourceSpellStores,
        owner: &mut (impl SpellExecutionOwner + ?Sized),
    ) -> Result<()> {
        if let Some(id) = self.current(slot) {
            let s = spells
                .get_mut(&id)
                .ok_or_else(|| refuse("execution.spells.current_identity"))?;
            if !delayed && s.state == SourceSpellState::Traveling {
                return Ok(());
            }
            if slot == SpellSlot::AutoRepeat && owner.unit(s.caster)?.player {
                owner.call(s, SpellCall::AutoRepeatCancel)?;
            }
            if !matches!(
                s.state,
                SourceSpellState::Created
                    | SourceSpellState::Targeting
                    | SourceSpellState::Casting
                    | SourceSpellState::Delayed
                    | SourceSpellState::Channeling
            ) {
                return Ok(());
            }
            let t = stores
                .template(s.spell)
                .ok_or_else(|| refuse("execution.spells.missing_template"))?;
            s.cancel(t, owner)?;
            if owner.unit(s.caster)?.has_ai {
                owner.call(s, SpellCall::AiInterrupt)?;
            }
            s.referenced = false;
            self.current[slot.index()] = None;
        }
        Ok(())
    }
    /// Unit.cpp:4221-4299. The in-flight Spell is separate from the other Spells.
    pub fn set(
        &mut self,
        spell: &mut SourceSpell,
        t: &SpellTemplate,
        spells: &mut BTreeMap<u64, SourceSpell>,
        stores: &SourceSpellStores,
        owner: &mut (impl SpellExecutionOwner + ?Sized),
    ) -> Result<()> {
        let slot = SpellSlot::for_spell(t, spell);
        if self.current(slot) == Some(spell.id) {
            return Ok(());
        }
        let parallel = t.attributesex4 & 0x80 != 0;
        let triggered = spell.flags.old_triggered();
        let cancels_auto = t.attributesex3 & 0x400000 != 0;
        self.interrupt(slot, false, spells, stores, owner)?;
        let attr = |slot: SpellSlot,
                    mask: u32,
                    ex4: bool,
                    slots: &Self,
                    spells: &BTreeMap<u64, SourceSpell>|
         -> bool {
            slots
                .current(slot)
                .and_then(|id| spells.get(&id))
                .and_then(|s| stores.template(s.spell))
                .is_some_and(|t| {
                    if ex4 {
                        t.attributesex4 & mask != 0
                    } else {
                        t.attributesex3 & mask != 0
                    }
                })
        };
        match slot {
            SpellSlot::Generic => {
                if self.current(SpellSlot::Channel).is_some()
                    && !parallel
                    && !attr(SpellSlot::Channel, 0x80, true, self, spells)
                {
                    self.interrupt(SpellSlot::Channel, false, spells, stores, owner)?;
                }
                if attr(SpellSlot::AutoRepeat, 0x400000, false, self, spells) {
                    self.interrupt(SpellSlot::AutoRepeat, true, spells, stores, owner)?;
                }
            }
            SpellSlot::Channel => {
                self.interrupt(SpellSlot::Channel, true, spells, stores, owner)?;
                if !parallel && !triggered {
                    // Unit.cpp:4259 reads AUTOREPEAT, not GENERIC. Preserve this quirk.
                    if self.current(SpellSlot::AutoRepeat).is_some()
                        && !attr(SpellSlot::AutoRepeat, 0x80, true, self, spells)
                    {
                        self.interrupt(SpellSlot::Generic, false, spells, stores, owner)?;
                    }
                    if attr(SpellSlot::AutoRepeat, 0x400000, false, self, spells) {
                        self.interrupt(SpellSlot::AutoRepeat, true, spells, stores, owner)?;
                    }
                }
            }
            SpellSlot::AutoRepeat => {
                if cancels_auto {
                    self.interrupt(SpellSlot::Generic, false, spells, stores, owner)?;
                    if !parallel {
                        self.interrupt(SpellSlot::Channel, false, spells, stores, owner)?;
                    }
                }
                owner.reset_auto_repeat()?;
            }
            SpellSlot::Melee => {}
        }
        if let Some(old) = self.current(slot) {
            spells
                .get_mut(&old)
                .expect("validated current spell")
                .referenced = false;
        }
        self.current[slot.index()] = Some(spell.id);
        spell.referenced = true;
        Ok(())
    }
    pub fn clear_finished(&mut self, spells: &mut BTreeMap<u64, SourceSpell>) -> Result<()> {
        for slot in &mut self.current {
            if let Some(id) = *slot {
                let s = spells
                    .get_mut(&id)
                    .ok_or_else(|| refuse("execution.spells.current_identity"))?;
                if s.state == SourceSpellState::Finished {
                    s.referenced = false;
                    *slot = None;
                }
            }
        }
        Ok(())
    }
}

#[test]
fn cast_time_float_conversion_refuses_both_undefined_edges() {
    assert_eq!(scale_cast_time(i32::MIN, 1f32.to_bits()).unwrap(), i32::MIN);
    assert_eq!(
        scale_cast_time(2147483520, 1f32.to_bits()).unwrap(),
        2147483520
    );
    for (time, speed) in [(i32::MAX, 1f32), (i32::MIN, 2f32), (1, f32::NAN)] {
        assert_eq!(
            scale_cast_time(time, speed.to_bits()).unwrap_err().id,
            "execution.spells.SpellMgr_160.cast_time_float_to_int_undefined"
        );
    }
    assert_eq!(scale_cast_time(3, 0.5f32.to_bits()).unwrap(), 1);
}

#[test]
fn ranged_delay_refuses_at_first_signed_overflow() {
    assert_eq!(add_ranged_delay(i32::MAX - 500).unwrap(), i32::MAX);
    assert_eq!(
        add_ranged_delay(i32::MAX - 499).unwrap_err().id,
        "execution.spells.SpellMgr_145.cast_time_add_overflow_undefined"
    );
}

#[test]
fn cast_chain_refuses_at_limit_and_only_wraps_when_unlimited() {
    let mut auras = SourceUnitAuras::default();
    assert!(auras.try_enter_cast(1));
    assert!(!auras.try_enter_cast(1));
    assert_eq!(auras.cast_counter, 1);
    auras.leave_cast();
    auras.leave_cast();
    assert_eq!(auras.cast_counter, 0);
    auras.cast_counter = u32::MAX;
    assert!(auras.try_enter_cast(0));
    assert_eq!(auras.cast_counter, 0);
}

#[test]
fn trigger_data_keeps_the_proc_flag_separate_from_negative_effects() {
    let mut objects = SourceObjects::default();
    let caster = objects.construct(6, SourceObjectKind::Creature).unwrap();
    let mut stores = SourceSpellStores::default();
    stores
        .insert_template(SpellTemplate {
            id: 42,
            effect2: 129,
            ..SpellTemplate::default()
        })
        .unwrap();
    let t = stores.template(42).unwrap();
    let mut spell = SourceSpell::new(1, t, caster, SpellTriggerFlags(0x40), 0).unwrap();
    spell.prepare_trigger_data(&stores).unwrap();
    assert_eq!(
        spell.trigger_data,
        Some(SpellTriggerData {
            can_trigger: false,
            negative_effect_mask: 2,
        })
    );
}

#[cfg(test)]
#[path = "cast_query_tests.rs"]
mod cast_query_tests;
