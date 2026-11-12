// SPDX-License-Identifier: GPL-3.0-or-later
//! Spell.cpp:2981-3098,3101-3157,3725-3742.
use super::*;

pub trait SpellEventQueue {
    fn add_spell(&mut self, spell: u64) -> Result<u64>;
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
        let script = stores
            .script_binding(SpellScriptKind::Spell, spell)?
            .is_some();
        stores.script_binding(SpellScriptKind::Aura, spell)?;
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
        value.script_present = script;
        value.reflectable = Some(
            t.dmgclass == 1
                && t.attributes & 0x20000050 == 0
                && t.attributesex & 0x80 == 0
                && !owner.positive_definition(t)?,
        );
        // SpellTargets.cpp:503-505 returns exactly 10; only the loaded map is missing.
        value.filtering_scheme = Some(owner.targeting_data(spell)?);
        if script {
            owner.call(&mut value, SpellCall::Init)?;
        }
        Ok(value)
    }
    pub fn start(
        &mut self,
        t: &SpellTemplate,
        targets: SpellTargets,
        aura: Option<u32>,
        queue: &mut impl SpellEventQueue,
        owner: &mut (impl SpellExecutionOwner + ?Sized),
    ) -> Result<SpellCastResult> {
        require(
            self.state == SourceSpellState::Created,
            "execution.spells.SpellStart_state",
        )?;
        self.state = SourceSpellState::Targeting;
        self.targets = targets;
        self.triggered_by_aura = aura;
        // :3007-3009. A failed CheckCast still owns the queued SpellEvent.
        self.event = Some(queue.add_spell(self.id)?);
        let result = self.check_cast(t, true, owner)?;
        if result != SpellCastResult::Ok && (!self.auto_repeat || self.flags.autorepeat()) {
            owner.call(self, SpellCall::SendResult(result))?;
            self.finish_failed(owner)?;
            return Ok(result);
        }
        self.prepare(t, owner)?;
        Ok(SpellCastResult::Ok)
    }
    pub fn prepare(
        &mut self,
        t: &SpellTemplate,
        owner: &mut (impl SpellExecutionOwner + ?Sized),
    ) -> Result<()> {
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
        if self.script_present {
            owner.call(self, SpellCall::SuccessfulStart)?;
        }
        if !c.gameobject && t.attributesex & 1 != 0 {
            owner.call(self, SpellCall::UnsummonPet)?;
        }
        if !self.flags.old_triggered() {
            if !c.gameobject {
                if (!self.flags.ignore_current(t) || is_channel(t)) && !self.flags.autorepeat() {
                    owner.set_current(self, t)?;
                }
                owner.add_gcd(self, t)?;
            }
            owner.call(self, SpellCall::SendStart)?;
            if self.timer == 0 && !is_melee(t) && (!self.auto_repeat || self.flags.autorepeat()) {
                owner.call(self, SpellCall::Cast(false))?;
            }
        } else {
            if is_channel(t) {
                require(
                    !c.gameobject,
                    "execution.spells.triggered_GO_channel_null_caster_undefined",
                )?;
                owner.set_current(self, t)?;
            }
            if self.timer == 0 {
                owner.call(self, SpellCall::Cast(true))?;
            }
        }
        Ok(())
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
    pub fn set(
        &mut self,
        id: u64,
        spells: &mut BTreeMap<u64, SourceSpell>,
        stores: &SourceSpellStores,
        owner: &mut (impl SpellExecutionOwner + ?Sized),
    ) -> Result<()> {
        let s = spells
            .get(&id)
            .ok_or_else(|| refuse("execution.spells.current_identity"))?;
        let t = stores
            .template(s.spell)
            .ok_or_else(|| refuse("execution.spells.missing_template"))?;
        let slot = SpellSlot::for_spell(t, s);
        if self.current(slot) == Some(id) {
            return Ok(());
        }
        let parallel = t.attributesex4 & 0x80 != 0;
        let triggered = s.flags.old_triggered();
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
        self.current[slot.index()] = Some(id);
        spells.get_mut(&id).expect("validated new spell").referenced = true;
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
