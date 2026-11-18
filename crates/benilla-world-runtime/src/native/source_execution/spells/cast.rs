// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Spell.cpp:3160-3377,3442-3484,3725-3793. Immediate creature self casts.
use super::*;

#[derive(Clone, Copy)]
pub enum SelfCastCaller {
    TriggeredUnit,
    UnitAi,
}

/// Live operands and effects of Spell.cpp:3160-3207.
pub trait SpellSelfCastOwner: SpellExecutionOwner {
    fn cast_objects(&self) -> &SourceObjects;
    fn cast_mod_owner(&mut self, caster: ObjectHandle) -> Result<Option<ObjectHandle>>;
    fn cast_chain(&mut self) -> (&mut SourceUnitAuras, u32);
    fn cast_checked(
        &mut self,
        spell: &mut SourceSpell,
        t: &SpellTemplate,
        starts: &mut SourceSpellStarts,
        events: &mut SourceUnitEvents,
    ) -> Result<()>;
}

/// Live continuations of the checked cast in Spell.cpp:3258-3353.
pub trait SpellCastEffects {
    fn cast_unit(&self) -> &SpellUnitState;
    fn cast_auras(&mut self) -> &mut SourceUnitAuras;
    fn duration(&self, t: &SpellTemplate) -> Result<i32>;
    fn fill_self_targets(&mut self, spell: &mut SourceSpell, t: &SpellTemplate) -> Result<()>;
    fn send_cooldown(&mut self, spell: &mut SourceSpell, t: &SpellTemplate) -> Result<()>;
    fn send_spell_go(&mut self, spell: &mut SourceSpell, t: &SpellTemplate) -> Result<()>;
    fn remove_cast_auras(
        &mut self,
        t: &SpellTemplate,
        caster: ObjectHandle,
        flag: u32,
    ) -> Result<()>;
    fn hit_self(
        &mut self,
        spell: &mut SourceSpell,
        t: &SpellTemplate,
        nested: Option<(&mut SourceSpellStarts, &mut SourceUnitEvents)>,
    ) -> Result<()>;
    fn proc_cast_end(&mut self, spell: &mut SourceSpell) -> Result<()>;
}

impl SourceSpell {
    /// Spell.cpp:3160-3207. A refused continuation keeps its in-flight Spell and cast depth.
    pub fn cast_self(
        &mut self,
        skip: bool,
        caller: SelfCastCaller,
        stores: &SourceSpellStores,
        starts: &mut SourceSpellStarts,
        events: &mut SourceUnitEvents,
        owner: &mut impl SpellSelfCastOwner,
    ) -> Result<()> {
        self.executing = true;
        require(
            owner.cast_mod_owner(self.caster)?.is_none(),
            "execution.spells.Spell_3163.SpellModRAII_owner",
        )?;
        let (auras, max_casts) = owner.cast_chain();
        if !auras.try_enter_cast(max_casts) {
            self.finish_failed(owner)?;
            self.executing = false;
            return Ok(());
        }
        self.update_self_pointers(caller, owner.cast_objects())?;
        let t = stores
            .template(self.spell)
            .ok_or_else(|| refuse("execution.spells.missing_template"))?;
        if !skip && self.check_cast(t, false, owner)? != SpellCastResult::Ok {
            return Err(refuse(match caller {
                SelfCastCaller::TriggeredUnit => "execution.spells.Spell_3206.StopCast_owner",
                // Spell.cpp:7857-7863. SendCastResult returns for a creature.
                SelfCastCaller::UnitAi => "execution.spells.Spell_7860.SendInterrupted_owner",
            }));
        }
        owner.cast_checked(self, t, starts, events)
    }

    /// Spell.cpp:3180,6901-6906. Only the self-target pointer update is bound.
    fn update_self_pointers(&self, caller: SelfCastCaller, objects: &SourceObjects) -> Result<()> {
        objects.get(self.caster)?;
        if self.targets.unit.is_none() && matches!(caller, SelfCastCaller::TriggeredUnit) {
            require(
                self.original_caster == self.caster,
                "execution.spells.Spell_3180.UpdatePointers_nonself_owner",
            )
        } else {
            require(
                self.targets.unit == Some(self.caster) && self.original_caster == self.caster,
                "execution.spells.Spell_6905.UpdatePointers_nonself_owner",
            )
        }
    }

    /// Spell.cpp:3211-3377,3442-3484,3725-3793. Execute a checked immediate creature cast.
    pub fn cast_checked(
        &mut self,
        t: &SpellTemplate,
        stores: &SourceSpellStores,
        base: Option<&SourceUnitAi>,
        nested: Option<(&mut SourceSpellStarts, &mut SourceUnitEvents)>,
        effects: &mut impl SpellCastEffects,
        finish: impl FnOnce(&mut SourceSpell) -> Result<()>,
    ) -> Result<()> {
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
        // Spell.h:158-165 checks TARGET_FLAG_TRADE_ITEM before changing the item GUID.
        require(
            self.targets.mask & 0x1000 == 0,
            "execution.spells.Spell_3256.updateTradeSlotItem_owner",
        )?;
        self.duration = effects.duration(t)?;
        effects.fill_self_targets(self, t)?;
        effects.send_cooldown(self, t)?;
        if effects.cast_unit().has_ai && self.flags.notify_ai() {
            let base = base.ok_or_else(|| refuse("execution.spells.Spell_3274.active_ai_owner"))?;
            // UnitAI.cpp:956 short-circuits before reading EventAI's main-spell set.
            require(
                base.ranged_setting != 1,
                "execution.spells.UnitAI_956.main_spell_membership_owner",
            )?;
        }
        // TakePower calls ModifyPower(0), which returns for zero.
        require(
            self.power_cost == 0 && t.powertype < 5,
            "execution.spells.Spell_4399.ModifyPower_owner",
        )?;
        // TakeReagents, TakeAmmo and SendCastResult return for a non-player.
        require(
            !effects.cast_unit().player && !effects.cast_unit().gameobject,
            "execution.spells.Spell_4439.player_cast_owner",
        )?;
        require(
            t.attributesex & 0x44 == 0,
            "execution.spells.Spell_3942.channel_spell_go_owner",
        )?;
        if !self.flags.channel_only()
            && (t.spellvisual != 0
                || f32::from_bits(t.speed.0) > 0.
                || (!self.flags.old_triggered() && self.triggered_by_aura.is_none()))
        {
            effects.send_spell_go(self, t)?;
        }
        for (i, effect) in t.effects().iter().enumerate() {
            if effect.effect != 0 {
                self.damage_multipliers[i] = 1f32.to_bits();
            }
        }
        require(
            !self.script_present,
            "execution.spells.Spell_8059.OnCast_owner",
        )?;
        if !self.flags.old_triggered() && t.attributesex2 & 0x10000000 == 0 {
            effects.remove_cast_auras(t, self.caster, 0x10000)?;
        }
        self.state = SourceSpellState::Landing;
        require(
            self.flags.old_triggered()
                || t.interruptflags & 8 == 0
                || t.attributesex2 & 0x20000 != 0,
            "execution.spells.Spell_3450.resetAttackTimer_owner",
        )?;
        if let Some(threat) = stores.threat(t.id) {
            require(
                threat.threat == 0 && threat.ap_bonus.0 == 0,
                "execution.spells.Spell_4497.HandleThreatSpells_owner",
            )?;
        }
        // Spell.cpp:3462 is the first delayed-spell read in this immediate phase.
        require(
            f32::from_bits(t.speed.0).partial_cmp(&0.) != Some(std::cmp::Ordering::Greater),
            "execution.spells.Spell_3462.delayed_spell_owner",
        )?;
        // Spell.cpp:1612-1652 computes no damage or healing and rolls no crit here.
        for target in &mut self.unique_targets {
            target.processed_mask = 0;
        }
        effects.hit_self(self, t, nested)?;
        require(
            !effects.cast_auras().has_aura_type(109),
            "execution.spells.Spell_7442.Aura_isAffectedOnSpell_owner",
        )?;
        effects.proc_cast_end(self)?;
        // HandleImmediate revisits processed targets, then finishes the cast.
        self.state = SourceSpellState::Finished;
        finish(self)?;
        require(
            t.attributes & 0x0010_0000 == 0,
            "execution.spells.Spell_3793.AttackStop_owner",
        )?;
        effects.cast_auras().leave_cast();
        self.executing = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CooldownOwner<'a> {
        objects: &'a SourceObjects,
        auras: SourceUnitAuras,
    }

    impl SpellExecutionOwner for CooldownOwner<'_> {
        fn check(
            &mut self,
            _: &mut SourceSpell,
            _: &SpellTemplate,
            check: SpellCheck,
        ) -> Result<SpellCastResult> {
            assert_eq!(check, SpellCheck::IsSpellReady);
            Ok(SpellCastResult::NotReady)
        }
    }

    impl SpellSelfCastOwner for CooldownOwner<'_> {
        fn cast_objects(&self) -> &SourceObjects {
            self.objects
        }
        fn cast_mod_owner(&mut self, caster: ObjectHandle) -> Result<Option<ObjectHandle>> {
            world_object_spell_mod_owner(self.objects, caster, false)
        }
        fn cast_chain(&mut self) -> (&mut SourceUnitAuras, u32) {
            (&mut self.auras, 1)
        }
        fn cast_checked(
            &mut self,
            _: &mut SourceSpell,
            _: &SpellTemplate,
            _: &mut SourceSpellStarts,
            _: &mut SourceUnitEvents,
        ) -> Result<()> {
            panic!("CheckCast failure must stop before effects");
        }
    }

    #[test]
    fn non_strict_cast_keeps_the_spell_event_and_depth_at_each_stop() {
        let mut objects = SourceObjects::default();
        let caster = objects.construct(6, SourceObjectKind::Creature).unwrap();
        let mut stores = SourceSpellStores::default();
        stores
            .insert_template(SpellTemplate {
                id: 42,
                ..SpellTemplate::default()
            })
            .unwrap();
        let t = stores.template(42).unwrap();
        for (caller, refusal) in [
            (
                SelfCastCaller::TriggeredUnit,
                "execution.spells.Spell_3206.StopCast_owner",
            ),
            (
                SelfCastCaller::UnitAi,
                "execution.spells.Spell_7860.SendInterrupted_owner",
            ),
        ] {
            let mut starts = SourceSpellStarts::default();
            let mut events = SourceUnitEvents::default();
            let mut owner = CooldownOwner {
                objects: &objects,
                auras: SourceUnitAuras::default(),
            };
            let id = starts.allocate().unwrap();
            let mut spell = SourceSpell::new(id, t, caster, SpellTriggerFlags(0), 0).unwrap();
            spell
                .schedule_start(
                    SpellTargets {
                        unit: Some(caster),
                        ..SpellTargets::default()
                    },
                    None,
                    &mut events,
                )
                .unwrap();
            spell.state = SourceSpellState::Casting;
            assert_eq!(
                spell
                    .cast_self(false, caller, &stores, &mut starts, &mut events, &mut owner)
                    .unwrap_err()
                    .id,
                refusal,
            );
            assert!(spell.executing);
            assert_eq!(spell.state, SourceSpellState::Casting);
            assert_eq!(owner.auras.cast_counter, 1);
            assert!(events.owns_spell_event(spell.event.unwrap(), id));
        }
    }
}
