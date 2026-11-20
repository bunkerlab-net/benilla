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
    fn finish_operands<'a>(
        &'a mut self,
        starts: Option<&'a SourceSpellStarts>,
    ) -> Result<SpellFinishOperands<'a>>;
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

pub struct SpellFinishOperands<'a> {
    pub(crate) attack: CastAttackStop<'a>,
    pub(crate) map_types: Option<&'a BTreeMap<u32, u32>>,
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
            // Spell.cpp:3172,3815-3835. The result precedes finish and skips aura-triggered casts.
            if self.triggered_by_aura.is_none() {
                require(
                    !owner.unit(self.caster)?.player,
                    "execution.spells.Spell_3172.SendCastResult_player_owner",
                )?;
                require(
                    !self.flags.pet_cast(),
                    "execution.spells.Spell_3823.GetControllingPlayer_owner",
                )?;
            }
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
        mut nested: Option<(&mut SourceSpellStarts, &mut SourceUnitEvents)>,
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
        effects.hit_self(
            self,
            t,
            nested
                .as_mut()
                .map(|(starts, events)| (&mut **starts, &mut **events)),
        )?;
        require(
            !effects.cast_auras().has_aura_type(109),
            "execution.spells.Spell_7442.Aura_isAffectedOnSpell_owner",
        )?;
        effects.proc_cast_end(self)?;
        // HandleImmediate revisits processed targets, then finishes the cast.
        self.state = SourceSpellState::Finished;
        finish(self)?;
        finish_tail(
            t,
            effects.finish_operands(nested.map(|(starts, _)| &*starts))?,
        )?;
        effects.cast_auras().leave_cast();
        self.executing = false;
        Ok(())
    }
}

fn finish_tail(t: &SpellTemplate, live: SpellFinishOperands<'_>) -> Result<()> {
    let SpellFinishOperands {
        mut attack,
        map_types,
    } = live;
    let caster = attack.object;
    // Spell.cpp:3793-3802. Spell.h:584-587 tests the ranged slot attribute.
    if t.attributes & 0x0010_0000 != 0 {
        if t.attributes & 2 != 0 {
            attack.interrupt_autorepeat()?;
        }
        attack.attack_stop(false, false, false, false)?;
        require(
            !attack
                .control_guids
                .is_client_controlled(*attack.flags, caster, None),
            "execution.spells.Spell_3801.SendAttackSwingCancelAttack_owner",
        )?;
    }
    // Spell.cpp:3805-3809. Map.h:201 checks the map entry if present.
    if caster.in_world {
        let map = caster
            .map
            .ok_or_else(|| refuse("execution.spells.Spell_3807.GetMap_owner"))?;
        let map_types =
            map_types.ok_or_else(|| refuse("execution.spells.Spell_3808.IsDungeon_owner"))?;
        let dungeon = map_types.get(&map.map).is_some_and(|&map_type| {
            PersistentMapEntry {
                id: map.map,
                map_type,
            }
            .dungeon()
        });
        require(
            !dungeon,
            "execution.spells.Spell_3809.UpdateEncounterState_owner",
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CooldownOwner<'a> {
        objects: &'a SourceObjects,
        auras: SourceUnitAuras,
    }

    impl SpellExecutionOwner for CooldownOwner<'_> {
        fn unit(&mut self, caster: ObjectHandle) -> Result<SpellUnitState> {
            Ok(SpellUnitState {
                player: self.objects.get(caster)?.kind == SourceObjectKind::Player,
                gameobject: false,
                alive: true,
                standing: true,
                player_controlled: false,
                combat: false,
                battleground_wait_leave: false,
                gm: false,
                moving: false,
                falling_far: false,
                stealth: false,
                form: 0,
                class: 1,
                combo_target: None,
                mounted: false,
                taxi: false,
                totem: false,
                untargetable: false,
                has_ai: false,
                position: [0; 4],
            })
        }

        fn reset_auto_repeat(&mut self) -> Result<()> {
            Ok(())
        }
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
        fn cast_mod_owner(&mut self, _: ObjectHandle) -> Result<Option<ObjectHandle>> {
            Ok(None)
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

    #[test]
    fn cast_limit_preserves_finish_and_refuses_unbound_result_recipients() {
        use SourceObjectKind::{Creature, Pet, Player};

        let stores = SourceSpellStores::default();
        let t = SpellTemplate {
            id: 42,
            ..SpellTemplate::default()
        };
        for (kind, flags, aura, refusal) in [
            (Creature, 0, None, None),
            (
                Player,
                0,
                None,
                Some("execution.spells.Spell_3172.SendCastResult_player_owner"),
            ),
            (Player, 0, Some(17), None),
            (
                Pet,
                0x80,
                None,
                Some("execution.spells.Spell_3823.GetControllingPlayer_owner"),
            ),
            (Pet, 0x80, Some(17), None),
        ] {
            let mut objects = SourceObjects::default();
            let caster = objects.construct(6, kind).unwrap();
            // Bind the earlier SpellMod lookup so every case reaches the depth limit.
            let mut owner = CooldownOwner {
                objects: &objects,
                auras: SourceUnitAuras::default(),
            };
            owner.auras.cast_counter = 1;
            let mut spell = SourceSpell::new(1, &t, caster, SpellTriggerFlags(flags), 0).unwrap();
            spell.state = SourceSpellState::Casting;
            spell.triggered_by_aura = aura;
            let result = spell.cast_self(
                false,
                SelfCastCaller::UnitAi,
                &stores,
                &mut SourceSpellStarts::default(),
                &mut SourceUnitEvents::default(),
                &mut owner,
            );
            assert_eq!(owner.auras.cast_counter, 1, "{kind:?}");
            if let Some(id) = refusal {
                assert_eq!(result.unwrap_err().id, id);
                assert_eq!(spell.state, SourceSpellState::Casting);
                assert!(spell.executing);
            } else {
                assert_eq!(result, Ok(()));
                assert_eq!(spell.state, SourceSpellState::Finished);
                assert!(!spell.executing);
            }
        }
    }

    fn attack<'a>(
        object: &'a SourceObject,
        live: &'a mut SourceUnitFields,
        stores: &'a SourceSpellStores,
    ) -> CastAttackStop<'a> {
        let (cast, starts, _) = live.borrow_cast();
        CastAttackStop {
            object,
            flags: cast.flags,
            control_guids: cast.control_guids,
            state: cast.state,
            victim: cast.victim,
            slots: cast.spell_slots,
            starts: Some(starts),
            stores,
        }
    }

    #[test]
    fn finish_attack_stop_preserves_callback_and_write_order() {
        let mut objects = SourceObjects::default();
        let caster = objects.construct(1, SourceObjectKind::Creature).unwrap();
        let victim = objects.construct(1, SourceObjectKind::Creature).unwrap();
        let player = objects.construct(1, SourceObjectKind::Player).unwrap();
        let stores = SourceSpellStores::default();
        for (state, victim, target_switch, including_combo, caster, result, new_state) in [
            (8, None, false, false, caster, Ok(false), 8),
            (9, None, false, false, caster, Ok(false), 8),
            (
                8,
                Some(victim),
                false,
                false,
                caster,
                Err("execution.spells.Unit_6358._removeAttacker_owner"),
                8,
            ),
            (
                9,
                Some(victim),
                false,
                false,
                caster,
                Err("execution.spells.Unit_6405.SendMeleeAttackStop_owner"),
                8,
            ),
            (
                9,
                Some(victim),
                true,
                false,
                caster,
                Err("execution.spells.Unit_6350.SetTargetGuid_owner"),
                9,
            ),
            (
                9,
                Some(victim),
                true,
                true,
                caster,
                Err("execution.spells.Unit_6342.ClearComboPoints_owner"),
                9,
            ),
            (
                9,
                None,
                false,
                false,
                player,
                Err("execution.spells.Unit_6353.SendAttackSwingCancelAttack_owner"),
                9,
            ),
        ] {
            let mut live = SourceUnitFields::constructed_creature();
            live.state = state;
            live.victim = victim;
            live.target = victim;
            assert_eq!(
                attack(objects.get(caster).unwrap(), &mut live, &stores)
                    .attack_stop(target_switch, true, including_combo, false)
                    .map_err(|error| error.id),
                result.map_err(str::to_owned)
            );
            assert_eq!(
                (live.state, live.victim, live.target),
                (new_state, victim, victim)
            );
        }
    }

    #[test]
    fn finish_attack_stop_reads_live_slots_before_clearing_melee() {
        let mut objects = SourceObjects::default();
        let caster = objects.construct(1, SourceObjectKind::Creature).unwrap();
        let player = objects.construct(1, SourceObjectKind::Player).unwrap();
        let mut slot_owner = CooldownOwner {
            objects: &objects,
            auras: SourceUnitAuras::default(),
        };
        let stores = SourceSpellStores::default();
        for (attributes, slot, kind, expected, new_state) in [
            (0, Some(SpellSlot::Melee), caster, Ok(()), 9),
            (0x0010_0000, None, caster, Ok(()), 8),
            (
                0x0010_0000,
                Some(SpellSlot::Melee),
                caster,
                Err("execution.spells.Unit_4304.InterruptSpell_live_cancel_owner"),
                8,
            ),
            (0x0010_0002, None, caster, Ok(()), 8),
            (
                0x0010_0002,
                Some(SpellSlot::AutoRepeat),
                caster,
                Err("execution.spells.Spell_3796.InterruptSpell_autorepeat_owner"),
                9,
            ),
            (
                0x0010_0002,
                Some(SpellSlot::AutoRepeat),
                player,
                Err("execution.spells.Unit_4310.SendAutoRepeatCancel_owner"),
                9,
            ),
        ] {
            let mut live = SourceUnitFields::constructed_creature();
            live.state = 9;
            live.melee_spell = false;
            if let Some(slot) = slot {
                let t = SpellTemplate {
                    id: 7,
                    attributes: if slot == SpellSlot::Melee { 4 } else { 2 },
                    ..Default::default()
                };
                let mut s = SourceSpell::new(1, &t, kind, SpellTriggerFlags(0), 0).unwrap();
                s.auto_repeat = slot == SpellSlot::AutoRepeat;
                live.spell_slots
                    .set(
                        &mut s,
                        &t,
                        &mut live.spell_starts.spells,
                        &stores,
                        &mut slot_owner,
                    )
                    .unwrap();
                live.spell_starts.spells.insert(s.id, s);
            }
            let object = objects.get(kind).unwrap();
            let t = SpellTemplate {
                attributes,
                ..Default::default()
            };
            assert_eq!(
                finish_tail(
                    &t,
                    SpellFinishOperands {
                        attack: attack(object, &mut live, &stores),
                        map_types: None,
                    }
                )
                .map_err(|error| error.id),
                expected.map_err(str::to_owned)
            );
            assert_eq!(live.state, new_state);
            if let Some(slot) = slot {
                assert_eq!(live.spell_slots.current(slot), Some(1));
                assert!(live.spell_starts.spells[&1].referenced);
            }
            if slot == Some(SpellSlot::AutoRepeat) {
                assert_eq!(
                    attack(object, &mut live, &stores)
                        .attack_stop(false, true, false, true)
                        .unwrap_err()
                        .id,
                    "execution.spells.Unit_6346.InterruptNonMeleeSpells_owner"
                );
            }
        }
    }

    #[test]
    fn finish_attack_stop_reaches_control_and_encounter_after_melee() {
        let mut objects = SourceObjects::default();
        let caster = objects.construct(1, SourceObjectKind::Creature).unwrap();
        let map = MapIdentity {
            map: 33,
            instance: 1,
            creation_generation: 1,
        };
        let cells = SourceCells::new(map).unwrap();
        objects.bind_map(caster, map, &cells).unwrap();
        objects
            .assign_guid(caster, 9, SourceGuid::with_entry(HighGuid::Creature, 7, 9))
            .unwrap();
        objects.add_to_world(caster).unwrap();
        let object = objects.get(caster).unwrap();
        let stores = SourceSpellStores::default();
        let t = SpellTemplate {
            attributes: 0x0010_0000,
            ..Default::default()
        };
        let mut live = SourceUnitFields::constructed_creature();
        live.state = UNIT_STATE_MELEE;
        live.flags = 0x0100_0008;
        live.control_guids.charmer = SourceGuid::without_entry(HighGuid::Player, 42);
        let types = BTreeMap::from([(33, 1)]);
        assert_eq!(
            finish_tail(
                &t,
                SpellFinishOperands {
                    attack: attack(object, &mut live, &stores),
                    map_types: Some(&types),
                }
            )
            .unwrap_err()
            .id,
            "execution.spells.Spell_3801.SendAttackSwingCancelAttack_owner"
        );
        assert_eq!(live.state, 0);
        live.flags = 0;
        assert_eq!(
            finish_tail(
                &t,
                SpellFinishOperands {
                    attack: attack(object, &mut live, &stores),
                    map_types: Some(&types),
                }
            )
            .unwrap_err()
            .id,
            "execution.spells.Spell_3809.UpdateEncounterState_owner"
        );
        let types = BTreeMap::from([(33, 0)]);
        finish_tail(
            &t,
            SpellFinishOperands {
                attack: attack(object, &mut live, &stores),
                map_types: Some(&types),
            },
        )
        .unwrap();
    }

    #[test]
    fn finish_attack_stop_client_control_uses_flags_charmer_and_kind() {
        let mut objects = SourceObjects::default();
        let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
        let owner = objects.construct(1, SourceObjectKind::Player).unwrap();
        let other = objects.construct(1, SourceObjectKind::Player).unwrap();
        let player_guid = SourceGuid::without_entry(HighGuid::Player, 42);
        objects.assign_guid(owner, 42, player_guid).unwrap();
        objects
            .assign_guid(other, 43, SourceGuid::without_entry(HighGuid::Player, 43))
            .unwrap();
        let possessed = SourceControlGuids {
            charmer: player_guid,
            ..Default::default()
        };
        let non_player_charmer = SourceControlGuids {
            charmer: SourceGuid::with_entry(HighGuid::Creature, 1, 1),
            ..Default::default()
        };
        let uncharmed = SourceControlGuids::default();
        let player = objects.get(owner).unwrap();
        let creature = objects.get(unit).unwrap();
        let other = objects.get(other).unwrap();
        for (guids, flags, unit, exact, expected) in [
            (possessed, 0x0100_0000, player, None, false),
            (possessed, 0x0100_000c, player, None, false),
            (possessed, 0x0140_0008, player, None, false),
            (possessed, 0x0180_0008, player, None, false),
            (possessed, 8, player, None, false),
            (non_player_charmer, 0x0100_0008, player, None, false),
            (possessed, 0x0100_0008, creature, None, true),
            (possessed, 0x0100_0008, creature, Some(player), true),
            (possessed, 0x0100_0008, creature, Some(other), false),
            (uncharmed, 8, creature, None, false),
            (uncharmed, 8, player, None, true),
            (uncharmed, 8, player, Some(player), true),
            (uncharmed, 8, player, Some(other), false),
        ] {
            assert_eq!(guids.is_client_controlled(flags, unit, exact), expected);
        }
    }
}
