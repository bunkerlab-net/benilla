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
        let live = SelfAuraRemoval {
            target: caster,
            ..self.aura_removal()
        };
        check_cast_aura_removals(self.stores, self.auras, t, live, flag, || {})
    }

    fn proc_cast_end(&mut self, spell: &mut SourceSpell) -> Result<()> {
        let raw = self.random.creature_raw_position()?;
        let mut lookups = 0;
        self.auras
            .proc_creature_self_cast_end(self.stores, spell, || lookups += 1)?;
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
        SelfSpellTargeting {
            stores: self.stores,
            targets: self.targets,
            objects: self.objects,
            unit: &self.unit,
            level: self.level,
            immunities: self.immunities,
            auras: self.auras,
            relations: self,
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

    fn hit_self(
        &mut self,
        s: &mut SourceSpell,
        t: &SpellTemplate,
        nested: Option<(&mut SourceSpellStarts, &mut SourceUnitEvents)>,
    ) -> Result<()> {
        s.hit_self_auras(
            t,
            self.stores,
            self.aura_removal(),
            self.stand,
            nested,
            self,
        )
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

    fn aura_hit_target(&mut self, target: ObjectHandle) -> Result<&mut SourceUnitAuras> {
        require(
            target == self.object,
            "execution.spells.Spell_1490.AddSpellAuraHolder_target_owner",
        )?;
        Ok(self.auras)
    }

    fn apply_inserted_aura_holder(
        &mut self,
        t: &SpellTemplate,
        holder: SourceAuraHolderId,
        nested: Option<(&mut SourceSpellStarts, &mut SourceUnitEvents)>,
    ) -> Result<()> {
        let stores = self.stores;
        apply_self_aura_holder(self, stores, t, holder, nested)
    }

    fn proc_aura_hit(&mut self, s: &mut SourceSpell) -> Result<()> {
        let raw = self.random.creature_raw_position()?;
        let mut lookups = 0;
        let alive = self.objects.get(self.object)?.alive;
        self.auras
            .proc_creature_self_hit(self.stores, s, alive, || lookups += 1)?;
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
