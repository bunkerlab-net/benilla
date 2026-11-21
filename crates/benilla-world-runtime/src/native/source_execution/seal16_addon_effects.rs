// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Spell.cpp:3160-3370,533-1043,1163-1502. Triggered generic creature self casts.
use super::*;

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
    fn cast_chain(&mut self) -> Result<(&mut SourceUnitAuras, u32)> {
        let live = self.units.get_mut(self.objects, self.object)?;
        Ok((&mut live.auras, self.max_casts))
    }
    fn cast_checked(&mut self, spell: &mut SourceSpell, t: &SpellTemplate) -> Result<()> {
        spell.cast_checked(t, self.stores, None, self, |_, _| Ok(()))
    }
}

impl SpellCastEffects for Owner<'_> {
    fn cast_unit(&mut self) -> Result<SpellUnitState> {
        Ok(self.unit)
    }

    fn cast_auras(&mut self) -> Result<&mut SourceUnitAuras> {
        Ok(&mut self.units.get_mut(self.objects, self.object)?.auras)
    }

    fn finish_operands(&mut self) -> Result<SpellFinishOperands<'_>> {
        let live = self.units.get_mut(self.objects, self.object)?;
        Ok(SpellFinishOperands {
            attack: CastAttackStop {
                object: self.objects.get(self.object)?,
                flags: &live.flags,
                control_guids: &live.control_guids,
                state: &mut live.state,
                victim: &live.victim,
                slots: &live.spell_slots,
                starts: Some(&live.spell_starts),
                stores: self.stores,
            },
            map_types: self.terrain.pathfinder_map_types.as_ref(),
        })
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
            self.live()?.bounding_radius,
        )?);
        Ok(())
    }

    fn remove_cast_auras(
        &mut self,
        t: &SpellTemplate,
        caster: ObjectHandle,
        flag: u32,
    ) -> Result<()> {
        let fields = self.units.get_mut(self.objects, caster)?;
        let live = SelfAuraRemoval {
            unit: Self::aura_operands(
                self.objects,
                caster,
                self.level,
                fields.totem,
                &fields.control_guids,
            )?,
            visibility: &mut fields.visibility,
            damage: DamageAuraTarget::new(
                &mut fields.creature_damage,
                &fields.loaded_creature,
                fields.flags,
                fields.shapeshift_form,
            ),
        };
        check_cast_aura_removals(self.stores, &mut fields.auras, t, live, flag, || {})
    }

    fn proc_cast_end(&mut self, spell: &mut SourceSpell) -> Result<()> {
        let raw = self.random.creature_raw_position()?;
        let mut lookups = 0;
        let bolvar = self.object.thread == 6
            && self.objects.get(self.object)?.database_guid == 10495
            && spell.spell == 18943;
        let expected = [
            (8990, ProcTriggerExit::ZeroFlags),
            (18943, ProcTriggerExit::SelfTrigger),
        ];
        let live = self.units.get_mut(self.objects, self.object)?;
        live.auras.proc_creature_self_cast_end(
            self.stores,
            spell,
            self.objects,
            live.stand_state,
            |id, exit| {
                if bolvar {
                    assert_eq!(expected[lookups as usize], (id, exit));
                }
                lookups += 1;
            },
        )?;
        if bolvar {
            // No draw before the recorded completion at :61351308.
            assert_eq!(raw, 32034);
            assert_eq!(lookups as usize, expected.len());
        }
        assert_eq!(self.random.creature_raw_position()?, raw);
        spell.proc_event_lookups += lookups;
        Ok(())
    }

    fn duration(&self, t: &SpellTemplate) -> Result<i32> {
        creature_self_aura_duration(self.stores, t)
    }

    fn fill_self_targets(&mut self, s: &mut SourceSpell, t: &SpellTemplate) -> Result<()> {
        let observe_area = self.object.thread == 7
            && t.id == 1032
            && self.objects.get(self.object)?.database_guid == 35891;
        let mut selections = Vec::new();
        let live = self.live()?;
        SelfSpellTargeting {
            stores: self.stores,
            targets: self.targets,
            objects: self.objects,
            unit: &self.unit,
            level: self.level,
            immunities: &live.immunities,
            auras: &live.auras,
            relations: &SourceCasterRelations {
                caster: self.object,
                objects: self.objects,
                flags: &live.flags,
                control_guids: &live.control_guids,
            },
        }
        .fill(s, t, |selection| {
            if observe_area {
                selections.push(selection);
            }
        })?;
        if observe_area {
            // initial-classicmangos.sql:838502; Spell.cpp:552-573,2159-2161.
            assert_eq!(
                selections,
                [SelfTargetSelection {
                    effect: 0,
                    implicit_type: SpellTargetImplicitType::SpecialUnit,
                    modes: [1, 0],
                    dispatched: [true, false],
                    had_single: true,
                    caster_default: false,
                    has_candidate: [true, false],
                }]
            );
            assert_eq!(
                s.unique_targets,
                [SourceSpellTarget {
                    unit: self.object,
                    effect_mask: 1,
                    hit_mask: 1,
                    processed_mask: 0,
                    processed: false,
                    duration: -1,
                    missed: false,
                }]
            );
        }
        Ok(())
    }

    fn hit_self(&mut self, s: &mut SourceSpell, t: &SpellTemplate) -> Result<()> {
        s.hit_self_auras(t, self.stores, self)
    }
}

impl SpellAuraHitOwner for Owner<'_> {
    fn aura_holder_seconds(&mut self, s: &SourceSpell) -> Result<i64> {
        let raw = self.random.creature_raw_position()?;
        let clock = self.clocks.aura_holder(self.object.thread, raw)?;
        self.assert_event_ai_clock(
            s.caster,
            s.id,
            &clock,
            [(60478082, 60478084), (60478530, 60478532)],
        );
        Ok(clock.seconds)
    }

    fn aura_effect_seconds(&mut self, s: &SourceSpell) -> Result<i64> {
        let raw = self.random.creature_raw_position()?;
        let clock = self.clocks.aura(self.object.thread, raw)?;
        self.assert_event_ai_clock(
            s.caster,
            s.id,
            &clock,
            [(60478159, 60478161), (60478579, 60478581)],
        );
        Ok(clock.seconds)
    }

    fn aura_target(&self, target: ObjectHandle) -> Result<SelfAuraTarget<'_>> {
        let live = self.units.get(self.objects, target)?;
        Self::aura_operands(
            self.objects,
            target,
            self.level,
            live.totem,
            &live.control_guids,
        )
    }

    fn aura_hit_target(&mut self, target: ObjectHandle) -> Result<SelfAuraHitTarget<'_>> {
        let live = self.units.get_mut(self.objects, target)?;
        Ok(SelfAuraHitTarget {
            unit: Self::aura_operands(
                self.objects,
                target,
                self.level,
                live.totem,
                &live.control_guids,
            )?,
            stand: live.stand_state,
            auras: &mut live.auras,
            visibility: &mut live.visibility,
            damage: DamageAuraTarget::new(
                &mut live.creature_damage,
                &live.loaded_creature,
                live.flags,
                live.shapeshift_form,
            ),
        })
    }

    fn apply_inserted_aura_holder(
        &mut self,
        t: &SpellTemplate,
        holder: SourceAuraHolderId,
    ) -> Result<()> {
        let stores = self.stores;
        apply_self_aura_holder(self, stores, t, holder)
    }

    fn proc_aura_hit(&mut self, s: &mut SourceSpell) -> Result<()> {
        let raw = self.random.creature_raw_position()?;
        let mut lookups = 0;
        let alive = self.objects.get(self.object)?.alive;
        let live = self.units.get_mut(self.objects, self.object)?;
        live.auras.proc_creature_self_hit(
            self.stores,
            s,
            alive,
            self.objects,
            live.stand_state,
            |_, _| lookups += 1,
        )?;
        assert_eq!(self.random.creature_raw_position()?, raw);
        s.proc_event_lookups += lookups;
        Ok(())
    }
}

impl Owner<'_> {
    fn assert_event_ai_clock(
        &self,
        caster: ObjectHandle,
        id: u64,
        clock: &ActiveClock,
        lines: [(u64, u64); 2],
    ) {
        if self.object.thread == 6
            && self.objects.get(caster).unwrap().database_guid == 40520
            && matches!(id, 3 | 4)
        {
            // seal16-full-01/run/trace.tsv. Both time inputs follow raw word 27086.
            assert_eq!((clock.site_line, clock.time_line), lines[id as usize - 3]);
            assert_eq!(
                (clock.thread, clock.raw, clock.seconds),
                (6, 27087, 1794314190)
            );
        }
    }
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

#[test]
fn finish_attack_stop_nested_stance_runs_before_the_parent_tail() {
    use super::super::super::ActiveClock;
    use benilla_sim::source_rng::{DrawCursor, Mt19937, SourceRandom};

    struct AutoRepeatReset(bool);
    impl SpellExecutionOwner for AutoRepeatReset {
        fn reset_auto_repeat(&mut self) -> Result<()> {
            assert!(!self.0);
            self.0 = true;
            Ok(())
        }
    }

    let mut objects = SourceObjects::default();
    let object = objects.construct(1, SourceObjectKind::Creature).unwrap();
    let other = objects.construct(1, SourceObjectKind::Creature).unwrap();
    let map = MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    };
    objects
        .bind_map(object, map, &SourceCells::new(map).unwrap())
        .unwrap();
    objects.set_pose(object, SourcePose([0; 4])).unwrap();
    objects
        .assign_guid(object, 1, SourceGuid::with_entry(HighGuid::Creature, 1, 1))
        .unwrap();
    let stance = SpellTemplate {
        id: 2457,
        attributes: 0x0010_0042,
        effect1: 6,
        effectapplyauraname1: 36,
        effectmiscvalue1: 17,
        effectimplicittargeta1: 1,
        ..Default::default()
    };
    let boost = SpellTemplate {
        id: 21156,
        attributes: 0x0010_0040,
        ..Default::default()
    };
    let auto = SpellTemplate {
        id: 75,
        attributes: 2,
        ..Default::default()
    };
    let mut stores = SourceSpellStores::default();
    for t in [stance, boost, auto] {
        stores.insert_template(t).unwrap();
    }
    stores.bind_empty_duration_fixture();
    stores.bind_test_dbc("SpellCastTimes.dbc", []);
    stores.bind_test_dbc(
        "SpellShapeshiftForm.dbc",
        [(
            17,
            SpellDbcRecord {
                ordinal: 0,
                words: Default::default(),
            },
        )],
    );
    stores.bind_test_dbc("SpellRange.dbc", []);
    let targets = SourceSpellTargetMgr::initialize(&stores);
    for victim in [None, Some(other)] {
        let mut live = SourceUnitFields::constructed_creature();
        live.state = UNIT_STATE_MELEE;
        live.victim = victim;
        live.power_type = Some(1);
        live.power_fields = Some(SourceUnitPowerFields {
            level: 19,
            create_health: 100,
            create_mana: 0,
            auxiliary_current: Some([0; 3]),
            auxiliary_maximum: None,
        });
        let auto = stores.template(75).unwrap();
        let mut s = SourceSpell::new(
            live.spell_starts.allocate().unwrap(),
            auto,
            object,
            SpellTriggerFlags(0),
            0,
        )
        .unwrap();
        s.auto_repeat = true;
        let mut reset = AutoRepeatReset(false);
        live.spell_slots
            .set(
                &mut s,
                auto,
                &mut live.spell_starts.spells,
                &stores,
                &mut reset,
            )
            .unwrap();
        assert!(reset.0);
        live.spell_starts.spells.insert(s.id, s);
        let mut geometry = SourceTerrainManager::default();
        geometry.files.retain_source_absence(0, 32, 32);
        let mut mt = Mt19937::from_seed(123);
        let mut cursor = DrawCursor::default();
        let mut sink = |_| {};
        let mut random = SourceRandom::new(1, &mut mt, &mut cursor, &mut sink).unwrap();
        geometry.load_terrain(0, &mut random).unwrap();
        let clocks = [8250845, 8249833]
            .into_iter()
            .enumerate()
            .map(|(i, site)| ActiveClock {
                site_line: (2 * i + 1) as u64,
                time_line: (2 * i + 2) as u64,
                thread: 1,
                site: [1, 3678789157770927477, site],
                module: true,
                raw: 1,
                seconds: 42,
            });
        let mut clocks = ActiveClocks(clocks.collect());
        let unit = SpellUnitState {
            player: false,
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
        };
        let mut units = SourceUnits::default();
        units.register(&objects, object, live).unwrap();
        let result = Execution {
            terrain: &mut geometry,
            stores: &stores,
            targets: &targets,
            unit,
            level: 19,
            clocks: &mut clocks,
            world_clock: &SourceWorldClock::default(),
            random: &mut random,
            creature_spells: None,
            raw: 1,
            max_casts: 4,
        }
        .cast_spell(
            &mut CreatureAddonState::default(),
            &mut units,
            object,
            &objects,
            2457,
        );
        let live = units.get(&objects, object).unwrap();
        let expected = if victim.is_some() {
            "execution.spells.Unit_6405.SendMeleeAttackStop_owner"
        } else {
            "execution.spells.Spell_3796.InterruptSpell_autorepeat_owner"
        };
        assert_eq!(result.unwrap_err().id, expected);
        let parent = &live.spell_starts.spells[&2];
        let child = &live.spell_starts.spells[&3];
        assert_eq!(
            (parent.spell, child.spell, child.triggered_by_aura),
            (2457, 21156, Some(2457))
        );
        assert!(parent.executing);
        assert_eq!(child.state, SourceSpellState::Finished);
        assert_eq!(live.state & UNIT_STATE_MELEE, 0);
        assert_eq!(live.spell_slots.current(SpellSlot::AutoRepeat), Some(1));
        assert!(clocks.0.is_empty());
        if victim.is_some() {
            assert_eq!(parent.state, SourceSpellState::Landing);
            assert!(child.executing);
            assert_eq!(live.auras.cast_counter, 2);
            assert!(!live.auras.first_aura_holder(2457).unwrap().ready);
        } else {
            assert_eq!(parent.state, SourceSpellState::Finished);
            assert!(!child.executing);
            assert_eq!(live.auras.cast_counter, 1);
            assert!(live.auras.first_aura_holder(2457).unwrap().ready);
        }
    }
}
