use super::*;

struct NoDraw;
impl UnitAiExecutionOwner for NoDraw {}
impl SpellExecutionOwner for NoDraw {}
impl CreatureRandom for NoDraw {
    fn creature_irand(&mut self, min: i32, _: i32) -> Result<i32> {
        Ok(min)
    }
    fn creature_urand(&mut self, min: u32, _: u32) -> Result<u32> {
        Ok(min)
    }
}
fn setup() -> (SourceObjects, SourceUnits, SourceCreatureAis, ObjectHandle) {
    let mut objects = SourceObjects::default();
    let object = objects.construct(6, SourceObjectKind::Creature).unwrap();
    objects.set_pose(object, SourcePose([0; 4])).unwrap();
    let template = CreatureAiTemplateOperands {
        ordinal: 1,
        entry: 6491,
        faction: 35,
        extra_flags: 0,
        unit_flags: 0,
        type_flags: 0,
        static_flags: [0; 4],
        rank: 0,
        inhabit_type: 1,
        stats_multipliers: [1f32.to_bits(); 3],
        civilian: false,
        pursuit_ms: 15000,
        call_for_help: 0,
        creature_type: 0,
        ai_name: "EventAI".into(),
        script_name: String::new(),
    };
    let mut live = SourceUnitFields::creature(&template);
    live.unit_class = Some(1);
    let mut movement = SourceCreatureMovement::construct();
    movement.flags = Some(0);
    live.movement = Some(movement);
    let mut units = SourceUnits::default();
    units.register(&objects, object, live).unwrap();
    let bytes = b"LogFilter_EventAiDev=1\nGuarderSight=50\nCreatureCheckForHelpRadius=5\n";
    let config = EventAiLoaderConfig::parse(bytes, Sha256Id::of(bytes)).unwrap();
    let ai = SourceCreatureAi::construct(
        &objects,
        object,
        &mut units,
        &template,
        false,
        0,
        &config,
        &mut NoDraw,
    )
    .unwrap();
    let mut ais = SourceCreatureAis::default();
    ais.install(&objects, &mut units, ai, &mut SourceCombat::default())
        .unwrap();
    (objects, units, ais, object)
}
fn stores() -> SourceSpellStores {
    let mut stores = SourceSpellStores::default();
    stores
        .insert_template(SpellTemplate {
            id: 10848,
            ..SpellTemplate::default()
        })
        .unwrap();
    stores
}
#[test]
fn live_binding_runs_once_restores_event_owner_and_preserves_saved_bytes() {
    let (objects, mut units, ais, object) = setup();
    let stores = stores();
    let targets = SourceSpellTargetMgr::initialize(&stores);
    let mut terrain = SourceTerrainManager::default();
    let before = serde_json::to_vec(units.get(&objects, object).unwrap()).unwrap();
    let mut binding = Binding {
        object,
        stores: &stores,
        targets: &targets,
        terrain: &mut terrain,
        world_clock: &SourceWorldClock::default(),
    };
    let mut calls = 0;
    let expected = refuse("authored.callback.refusal");
    let result: Result<()> = binding.run(&mut units, &ais, &objects, |context| {
        calls += 1;
        assert!(context.owner.unit(object)?.has_ai);
        context.events.schedule_ai_notify(5000, false)?;
        Err(expected.clone())
    });
    assert_eq!(calls, 1);
    assert_eq!(result, Err(expected));
    let live = units.get(&objects, object).unwrap();
    assert!(live.unit_events.is_ai_notify_scheduled());
    assert_eq!(before, serde_json::to_vec(live).unwrap());
    assert_eq!(
        binding
            .run(&mut units, &ais, &objects, |_| {
                calls += 1;
                Ok(17)
            })
            .unwrap(),
        17
    );
    assert_eq!(calls, 2);
}
#[test]
fn live_ai_cast_keeps_its_spell_after_cooldowns_before_missing_geometry() {
    let (objects, mut units, mut ais, object) = setup();
    let stores = stores();
    let targets = SourceSpellTargetMgr::initialize(&stores);
    let mut terrain = SourceTerrainManager::default();
    let mut binding = Binding {
        object,
        stores: &stores,
        targets: &targets,
        terrain: &mut terrain,
        world_clock: &SourceWorldClock::default(),
    };
    // UnitAI.cpp:175 short-circuits the current-spell read during evade.
    ais.get_mut(&objects, object).unwrap().base.order = 4;
    assert!(!binding
        .run(&mut units, &ais, &objects, |c| c.ai_cast(
            object,
            Some(object),
            10848,
            0
        ))
        .unwrap());
    assert!(units
        .get(&objects, object)
        .unwrap()
        .spell_starts
        .spells
        .is_empty());
    ais.get_mut(&objects, object).unwrap().base.order = 0;
    // The triggered branch bypasses order but still reads current-cast slots.
    ais.get_mut(&objects, object).unwrap().base.order = 4;
    binding
        .run(&mut units, &ais, &objects, |c| {
            assert_eq!(
                c.owner.ai_cast_state(object, object, c.starts, 2)?,
                (true, false)
            );
            Ok(())
        })
        .unwrap();
    ais.get_mut(&objects, object).unwrap().base.order = 0;
    let before = serde_json::to_vec(units.get(&objects, object).unwrap()).unwrap();
    assert_eq!(
        binding
            .run(&mut units, &ais, &objects, |c| c.ai_cast(
                object,
                Some(object),
                10848,
                0
            ))
            .unwrap_err()
            .id,
        "execution.terrain.live_object_map_owner"
    );
    let live = units.get(&objects, object).unwrap();
    let spell = &live.spell_starts.spells[&1];
    assert_eq!(spell.cooldown_lookups, [1, 1]);
    assert_eq!(spell.state, SourceSpellState::Targeting);
    assert_eq!(spell.flags, SpellTriggerFlags(0x100));
    assert_eq!((spell.targets.unit, spell.targets.mask), (Some(object), 2));
    assert_eq!(live.unit_events.inventory().queued.len(), 1);
    assert_eq!(before, serde_json::to_vec(live).unwrap());
    assert_eq!(
        live.spell_starts
            .require_load_completion(&live.unit_events)
            .unwrap_err()
            .id,
        "execution.spells.queued_spell_event_owner"
    );
    // A second unit starts with independent owners. Replacing the comparison
    // record cannot remove the first unit's Spell or its event.
    let other = SourceUnitFields::creature(&CreatureAiTemplateOperands {
        ordinal: 2,
        entry: 6491,
        faction: 35,
        extra_flags: 0,
        unit_flags: 0,
        type_flags: 0,
        static_flags: [0; 4],
        rank: 0,
        inhabit_type: 1,
        stats_multipliers: [1f32.to_bits(); 3],
        civilian: false,
        pursuit_ms: 15000,
        call_for_help: 0,
        creature_type: 0,
        ai_name: "EventAI".into(),
        script_name: String::new(),
    });
    assert!(other.spell_starts.spells.is_empty());
    assert!(other.unit_events.inventory().queued.is_empty());
    assert_eq!(
        units
            .get(&objects, object)
            .unwrap()
            .spell_starts
            .spells
            .len(),
        1
    );
}
#[test]
fn live_unit_reads_current_flags_pose_auras_control_and_interrupt_slots() {
    let (mut objects, mut units, ais, object) = setup();
    let stores = stores();
    let targets = SourceSpellTargetMgr::initialize(&stores);
    let mut terrain = SourceTerrainManager::default();
    let live = units.get_mut(&objects, object).unwrap();
    live.flags = 8 | UNIT_FLAG_COMBAT | 0x0001_0000;
    live.stand_state = 1;
    live.shapeshift_form = 5;
    live.mounted = true;
    live.auras.aura_types.insert(16, vec![1]);
    objects
        .set_pose(object, SourcePose([1f32.to_bits(), 0, 0, 0]))
        .unwrap();
    let mut binding = Binding {
        object,
        stores: &stores,
        targets: &targets,
        terrain: &mut terrain,
        world_clock: &SourceWorldClock::default(),
    };
    binding
        .run(&mut units, &ais, &objects, |c| {
            let value = c.owner.unit(object)?;
            assert!(
                value.has_ai
                    && value.player_controlled
                    && value.combat
                    && value.untargetable
                    && value.mounted
                    && value.stealth
            );
            assert!(!value.standing);
            assert_eq!(value.form, 5);
            assert_eq!(value.position[0], 1f32.to_bits());
            assert!(!c.owner.has_aura(object, 10848)?);
            assert_eq!(c.owner.active_auto_spell(object, c.starts)?, None);
            c.owner.interrupt_for_ai(object, true)?;
            c.owner.interrupt_for_ai(object, false)?;
            for slot in 0..4 {
                c.owner.interrupt_slot(object, slot)?;
            }
            assert_eq!(
                c.owner.interrupt_slot(object, 4).unwrap_err().id,
                "execution.spells.EventAI_interrupt_slot_index_undefined"
            );
            Ok(())
        })
        .unwrap();
    let live = units.get_mut(&objects, object).unwrap();
    live.charmer = Some(object);
    live.combo_target = Some(object);
    binding
        .run(&mut units, &ais, &objects, |c| {
            assert_eq!(c.owner.unit(object)?.combo_target, Some(object));
            Ok(())
        })
        .unwrap();
}

#[test]
fn live_slots_and_ai_order_use_persistent_spells_in_source_order() {
    struct NoSpell;
    impl SpellExecutionOwner for NoSpell {}
    for melee in [false, true] {
        let (objects, mut units, mut ais, object) = setup();
        let mut stores = SourceSpellStores::default();
        let t = SpellTemplate {
            id: 10848,
            attributes: if melee { 4 } else { 0 },
            ..Default::default()
        };
        stores.insert_template(t.clone()).unwrap();
        let live = units.get_mut(&objects, object).unwrap();
        let mut spell = SourceSpell::new(1, &t, object, SpellTriggerFlags(0), 0).unwrap();
        spell.state = SourceSpellState::Casting;
        live.spell_starts.spells.insert(1, spell);
        live.spell_slots
            .set(1, &mut live.spell_starts.spells, &stores, &mut NoSpell)
            .unwrap();
        let targets = SourceSpellTargetMgr::initialize(&stores);
        let mut terrain = SourceTerrainManager::default();
        let mut binding = Binding {
            object,
            stores: &stores,
            targets: &targets,
            terrain: &mut terrain,
            world_clock: &SourceWorldClock::default(),
        };
        binding
            .run(&mut units, &ais, &objects, |c| {
                assert_eq!(
                    c.owner.ai_cast_state(object, object, c.starts, 0)?,
                    (false, !melee)
                );
                assert_eq!(
                    c.owner.interrupt_for_ai(object, melee).unwrap_err().id,
                    "execution.spells.Unit_4304.InterruptSpell_live_cancel_owner"
                );
                Ok(())
            })
            .unwrap();
        ais.get_mut(&objects, object).unwrap().base.order = 4;
        binding
            .run(&mut units, &ais, &objects, |c| {
                assert_eq!(
                    c.owner.ai_cast_state(object, object, c.starts, 0)?,
                    (true, false)
                );
                assert_eq!(
                    c.owner.ai_cast_state(object, object, c.starts, 2)?,
                    (true, !melee)
                );
                assert!(!c.ai_cast(object, Some(object), 10848, 0)?);
                Ok(())
            })
            .unwrap();
        assert_eq!(
            units
                .get(&objects, object)
                .unwrap()
                .spell_starts
                .spells
                .len(),
            1
        );
    }
}

#[test]
fn default_context_callback_runs_once_and_preserves_refusal() {
    struct DefaultOwner<'a> {
        stores: &'a SourceSpellStores,
        starts: &'a mut SourceSpellStarts,
        events: &'a mut SourceUnitEvents,
        spell_owner: &'a mut NoDraw,
    }
    impl CreatureRandom for DefaultOwner<'_> {
        fn creature_irand(&mut self, _: i32, _: i32) -> Result<i32> {
            panic!("no cast draw")
        }
        fn creature_urand(&mut self, _: u32, _: u32) -> Result<u32> {
            panic!("no cast draw")
        }
    }
    impl EventAiExecutionOwner for DefaultOwner<'_> {
        fn spell_context(&mut self) -> Result<SpellStartContext<'_>> {
            Ok(SpellStartContext {
                stores: self.stores,
                starts: self.starts,
                events: self.events,
                owner: self.spell_owner,
            })
        }
    }
    let stores = stores();
    let mut starts = SourceSpellStarts::default();
    let mut events = SourceUnitEvents::default();
    let mut owner = DefaultOwner {
        stores: &stores,
        starts: &mut starts,
        events: &mut events,
        spell_owner: &mut NoDraw,
    };
    let mut count = 0;
    assert_eq!(
        owner
            .with_spell_context(|context| {
                count += 1;
                assert!(context.contains(10848));
                Ok(7)
            })
            .unwrap(),
        7
    );
    assert_eq!(count, 1);
    let expected = refuse("authored.default.callback.refusal");
    let result: Result<()> = owner.with_spell_context(|_| {
        count += 1;
        Err(expected.clone())
    });
    assert_eq!(count, 2);
    assert_eq!(result, Err(expected));
}
#[test]
fn live_cooldowns_preserve_saved_bytes_and_the_f7_refusal() {
    let (objects, mut units, ais, object) = setup();
    let stores = stores();
    let t = stores.template(10848).unwrap();
    let before = serde_json::to_vec(units.get(&objects, object).unwrap()).unwrap();
    let live = units.get_mut(&objects, object).unwrap();
    live.cooldowns
        .add(0, t, 10, 0, false, |_| unreachable!())
        .unwrap();
    live.cooldowns.add_gcd(t, 10, || Ok(0)).unwrap();
    live.cooldowns.lockouts.insert(2, 10);
    assert_eq!(before, serde_json::to_vec(&*live).unwrap());
    assert_eq!(
        units.inventory(&objects).unwrap()["restore"],
        "execution.unit.f7_restore_owner"
    );
    assert_eq!(
        SourceManagers::default()
            .require_durable_checkpoint()
            .unwrap_err()
            .id,
        "execution.managers.f7_continuation_owner"
    );
    let targets = SourceSpellTargetMgr::initialize(&stores);
    let mut terrain = SourceTerrainManager::default();
    let mut binding = Binding {
        object,
        stores: &stores,
        targets: &targets,
        terrain: &mut terrain,
        world_clock: &SourceWorldClock::default(),
    };
    binding
        .run(&mut units, &ais, &objects, |c| {
            let mut s = SourceSpell::new(1, t, object, SpellTriggerFlags(0), 0)?;
            assert_eq!(
                c.owner.check(&mut s, t, SpellCheck::IsSpellReady)?,
                SpellCastResult::NotReady
            );
            assert_eq!(
                c.owner.check(&mut s, t, SpellCheck::HasGcd)?,
                SpellCastResult::NotReady
            );
            Ok(())
        })
        .unwrap();
    units
        .get_mut(&objects, object)
        .unwrap()
        .cooldowns
        .update(10);
    binding
        .run(&mut units, &ais, &objects, |c| {
            let mut s = SourceSpell::new(1, t, object, SpellTriggerFlags(0), 0)?;
            assert_eq!(
                c.owner.check(&mut s, t, SpellCheck::IsSpellReady)?,
                SpellCastResult::Ok
            );
            assert_eq!(
                c.owner.check(&mut s, t, SpellCheck::HasGcd)?,
                SpellCastResult::Ok
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn prepare_walk_preserves_holders_and_reaches_the_actual_ai_write() {
    let (objects, mut units, ais, object) = setup();
    let mut stores = stores();
    stores
        .insert_template(SpellTemplate {
            id: 9036,
            ..Default::default()
        })
        .unwrap();
    for id in [9036, 10848] {
        let t = stores.template(id).unwrap();
        units
            .get_mut(&objects, object)
            .unwrap()
            .auras
            .aura_holders
            .insert(id, SourceAuraHolder::new(t, object, 0, 0, -1).unwrap());
    }
    let before = units.get(&objects, object).unwrap().auras.clone();
    let targets = SourceSpellTargetMgr::initialize(&stores);
    let mut terrain = SourceTerrainManager::default();
    let mut binding = Binding {
        object,
        stores: &stores,
        targets: &targets,
        terrain: &mut terrain,
        world_clock: &SourceWorldClock::default(),
    };
    binding
        .run(&mut units, &ais, &objects, |c| {
            let t = stores.template(10848).unwrap();
            let mut s = SourceSpell::new(3, t, object, SpellTriggerFlags(0x100), 0)?;
            s.targets.unit = Some(object);
            c.owner.call(&mut s, SpellCall::RemoveCastAuras(4))?;
            assert_eq!(s.prepare_lookups[2], 2);
            assert_eq!(
                c.owner
                    .call(&mut s, SpellCall::AiCastState(true))
                    .unwrap_err()
                    .id,
                "execution.spells.UnitAI_429.current_spell_write_owner"
            );
            assert_eq!(&s.prepare_lookups[3..], &[1, 1, 1]);
            c.owner.call(&mut s, SpellCall::AiCastState(false))?;
            Ok(())
        })
        .unwrap();
    assert_eq!(units.get(&objects, object).unwrap().auras, before);
}

#[test]
fn casting_time_reads_live_speed_and_refuses_only_the_ranged_operand() {
    use std::cell::Cell;
    let (_, _, _, object) = setup();
    let row = SpellDbcRecord {
        ordinal: 0,
        words: BTreeMap::from([(1, 500), (2, 0), (3, 0)]),
    };
    let lookup = |index| Ok((index == 42).then_some(&row));
    let mut t = SpellTemplate {
        id: 10848,
        castingtimeindex: 42,
        ..Default::default()
    };
    let s = SourceSpell::new(3, &t, object, SpellTriggerFlags(0), 0).unwrap();
    let reads = Cell::new(0);
    let time = fresh_cast_time(
        lookup,
        &s,
        &t,
        || Ok(1),
        || {
            reads.set(reads.get() + 1);
            Ok(0.5f32.to_bits())
        },
    )
    .unwrap();
    assert_eq!(time, 250);
    assert_eq!(reads.get(), 1);
    t.attributes = 0x12;
    assert_eq!(
        fresh_cast_time(lookup, &s, &t, || Ok(1), || unreachable!())
            .unwrap_err()
            .id,
        "execution.spells.SpellMgr_162.ranged_attack_speed_owner"
    );
    t.castingtimeindex = 99;
    assert_eq!(
        fresh_cast_time(lookup, &s, &t, || unreachable!(), || unreachable!()).unwrap(),
        0
    );
}

#[test]
fn aura_removal_and_ai_movement_refuse_at_their_source_calls() {
    let (objects, mut units, ais, object) = setup();
    let mut stores = stores();
    stores
        .insert_template(SpellTemplate {
            id: 9036,
            aurainterruptflags: 4,
            ..Default::default()
        })
        .unwrap();
    let ghost = stores.template(9036).unwrap();
    units
        .get_mut(&objects, object)
        .unwrap()
        .auras
        .aura_holders
        .insert(
            9036,
            SourceAuraHolder::new(ghost, object, 0, 0, -1).unwrap(),
        );
    let mut shroud = stores.template(10848).unwrap().clone();
    shroud.interruptflags = 1;
    stores.insert_template(shroud).unwrap();
    let targets = SourceSpellTargetMgr::initialize(&stores);
    let mut terrain = SourceTerrainManager::default();
    let mut binding = Binding {
        object,
        stores: &stores,
        targets: &targets,
        terrain: &mut terrain,
        world_clock: &SourceWorldClock::default(),
    };
    binding
        .run(&mut units, &ais, &objects, |c| {
            let t = stores.template(10848).unwrap();
            let mut s = SourceSpell::new(3, t, object, SpellTriggerFlags(0), 0)?;
            assert_eq!(
                c.owner
                    .call(&mut s, SpellCall::RemoveCastAuras(4))
                    .unwrap_err()
                    .id,
                "execution.spells.Unit_5270.RemoveSpellAuraHolder_owner"
            );
            assert_eq!(s.prepare_lookups[2], 1);
            s.cast_time = 1;
            assert_eq!(
                c.owner
                    .call(&mut s, SpellCall::AiCastState(true))
                    .unwrap_err()
                    .id,
                "execution.spells.UnitAI_362.InterruptMoving_owner"
            );
            assert_eq!(s.prepare_lookups[4], 0);
            Ok(())
        })
        .unwrap();
    assert_eq!(
        units
            .get(&objects, object)
            .unwrap()
            .auras
            .aura_holders
            .len(),
        1
    );
}

#[test]
fn spell_mod_owner_dispatch_refuses_pet_and_totem_for_range_power_and_cast_time() {
    for (kind, totem, expected) in [
        (
            SourceObjectKind::Pet,
            false,
            "execution.spells.Pet_596.GetSpellModOwner_owner",
        ),
        (
            SourceObjectKind::Creature,
            true,
            "execution.spells.Totem_193.GetSpellModOwner_owner",
        ),
    ] {
        let mut objects = SourceObjects::default();
        let object = objects.construct(6, kind).unwrap();
        let mut fields = SourceUnitFields::constructed_creature();
        fields.totem = totem;
        let mut units = SourceUnits::default();
        units.register(&objects, object, fields).unwrap();
        let ais = SourceCreatureAis::default();
        let stores = stores();
        let targets = SourceSpellTargetMgr::initialize(&stores);
        let mut terrain = SourceTerrainManager::default();
        let mut binding = Binding {
            object,
            stores: &stores,
            targets: &targets,
            terrain: &mut terrain,
            world_clock: &SourceWorldClock::default(),
        };
        binding
            .run(&mut units, &ais, &objects, |c| {
                assert_eq!(c.owner.range_mod_owner(object).unwrap_err().id, expected);
                assert_eq!(c.owner.power_mod_owner(object).unwrap_err().id, expected);
                let t = stores.template(10848).unwrap();
                let mut spell = SourceSpell::new(1, t, object, SpellTriggerFlags(0), 0)?;
                assert_eq!(c.owner.cast_time(&mut spell, t).unwrap_err().id, expected);
                Ok(())
            })
            .unwrap();
    }
}
