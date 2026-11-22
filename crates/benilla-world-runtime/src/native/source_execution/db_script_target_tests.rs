use super::*;

#[test]
fn self_targets_form_source_major_cartesian_pairs_without_short_circuiting() {
    let r = DbScriptRow {
        buddy: 902,
        radius_or_guid: 10,
        flags: 0x200 | 4,
        ..row(1, 7, 0)
    };
    let definitions = maps(vec![r]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    f.host.geometry = true;
    f.host.visitor = vec![f.h[2], f.h[1]];
    f.host.entries.insert(f.h[2], 902);
    f.host.entries.insert(f.h[1], 902);
    start(&mut e, f.h[0], None, 0, &mut f).unwrap();
    assert_eq!(
        f.host
            .effects
            .iter()
            .map(|e| (e.1.unwrap(), e.2.unwrap()))
            .collect::<Vec<_>>(),
        [
            (f.h[2], f.h[2]),
            (f.h[2], f.h[1]),
            (f.h[1], f.h[2]),
            (f.h[1], f.h[1])
        ]
    );
}
#[test]
fn dead_entry_last_search_keeps_alive_candidates_but_all_buddies_require_corpse_state() {
    let r = DbScriptRow {
        buddy: 902,
        radius_or_guid: 10,
        flags: 0x40,
        ..row(1, 7, 0)
    };
    let definitions = maps(vec![r]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    f.host.geometry = true;
    f.host.visitor = vec![f.h[1], f.h[2]];
    f.host.entries.insert(f.h[1], 902);
    f.host.entries.insert(f.h[2], 902);
    start(&mut e, f.h[0], None, 0, &mut f).unwrap();
    assert_eq!(f.host.effects[0].1, Some(f.h[2]));
    let r = DbScriptRow {
        buddy: 902,
        radius_or_guid: 10,
        flags: 0x40 | 0x200,
        ..row(1, 9, 0)
    };
    let definitions = maps(vec![r]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    f.host.effects.clear();
    start(&mut e, f.h[0], None, 0, &mut f).unwrap();
    assert!(f.host.effects.is_empty());
}
#[test]
fn null_buddy_quirk_only_refuses_commands_that_dereference_it() {
    let definitions = maps(vec![row(1, 1, 0)]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    f.host.geometry = true;
    let missing = DbScriptRow {
        buddy: 901,
        radius_or_guid: 10,
        ..row(1, 7, 0)
    };
    let invocation = DbScriptInvocation {
        kind: Some(DbScriptType::Relay),
        id: 100,
        source: f.objects.get(f.h[0]).unwrap().guid,
        target: None,
        item_owner: None,
    };
    let mut context = DbScriptContext {
        objects: &mut f.objects,
        units: &mut f.units,
        owner: &mut f.host,
    };
    assert!(!e
        .handle_step(
            Step {
                invocation,
                definition: Definition::Loaded(&missing)
            },
            &mut context
        )
        .unwrap());
    assert!(context.owner.effects.is_empty());
    let active = DbScriptRow {
        command: 21,
        ..missing
    };
    assert_eq!(
        e.handle_step(
            Step {
                invocation,
                definition: Definition::Loaded(&active)
            },
            &mut context
        )
        .unwrap_err()
        .id,
        "execution.db_script_executor.null_active_source_undefined"
    );
    for flags in [0x100, 0x800] {
        let group = DbScriptRow {
            flags,
            ..active.clone()
        };
        assert_eq!(
            e.buddies(&group, None, None, &mut context).unwrap_err().id,
            "execution.db_script_executor.null_buddy_origin_undefined"
        );
    }
    assert!(e
        .buddies(&active, None, None, &mut context)
        .unwrap()
        .is_none());
}
#[test]
fn queued_relay_uses_header_default_none_and_explicit_uniqueness_still_suppresses() {
    let mut relay = row(45, 100, 0);
    relay.delay = 5;
    let mut next = row(1, 7, 0);
    next.delay = 10;
    let definitions = maps(vec![relay, next]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    start(&mut e, f.h[0], None, 3, &mut f).unwrap();
    f.host.now = 5;
    process(&mut e, &mut f).unwrap();
    assert_eq!(e.pending().count(), 3);
    start(&mut e, f.h[0], None, 3, &mut f).unwrap();
    assert_eq!(e.pending().count(), 3);
    f.host.now = 10;
    process(&mut e, &mut f).unwrap();
    assert_eq!(f.host.effects.len(), 1);
}
#[test]
fn signed_clock_addition_checks_both_sides_of_the_deadline_boundary() {
    let definitions = maps(vec![DbScriptRow {
        delay: 1,
        ..row(1, 7, 0)
    }]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    f.host.now = i64::MAX;
    assert_eq!(
        start(&mut e, f.h[0], None, 0, &mut f).unwrap_err().id,
        "execution.db_script_executor.clock_add_overflow_undefined"
    );
    assert_eq!(e.pending().count(), 0);
    f.host.now = i64::MAX - 1;
    start(&mut e, f.h[0], None, 0, &mut f).unwrap();
    assert_eq!(e.pending().next().unwrap().0, i64::MAX);
    f.host.now = i64::MAX;
    process(&mut e, &mut f).unwrap();
    assert_eq!(f.host.effects.len(), 1);
}
#[test]
fn deferred_source_guid_resolves_a_replacement_construction() {
    let definitions = maps(vec![DbScriptRow {
        delay: 1,
        ..row(1, 7, 0)
    }]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    start(&mut e, f.h[0], None, 0, &mut f).unwrap();
    let guid = f.objects.get(f.h[0]).unwrap().guid.unwrap();
    f.objects.remove_from_world(f.h[0]).unwrap();
    let replacement = f.objects.construct(1, SourceObjectKind::Creature).unwrap();
    f.objects.assign_guid(replacement, 1, guid).unwrap();
    f.objects.get_mut(replacement).unwrap().map = Some(map_id());
    f.objects.add_to_world(replacement).unwrap();
    f.units
        .register(
            &f.objects,
            replacement,
            SourceUnitFields::creature(
                &crate::native::source_execution::motion_master_tests::template(),
            ),
        )
        .unwrap();
    f.host.now = 1;
    process(&mut e, &mut f).unwrap();
    assert_eq!(f.host.effects[0].1, Some(replacement));
}
#[test]
fn unsupported_elevator_and_dynamic_guids_skip_but_motransport_and_corpse_require_real_lookup() {
    let definitions = maps(vec![DbScriptRow {
        text: [1, 0, 0, 0],
        ..row(0, 0, 0)
    }]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    for (high, kind, error) in [
        (HighGuid::Transport, SourceObjectKind::Transport, None),
        (
            HighGuid::DynamicObject,
            SourceObjectKind::DynamicObject,
            None,
        ),
        (
            HighGuid::MoTransport,
            SourceObjectKind::Transport,
            Some("f4_transport_lookup"),
        ),
        (
            HighGuid::Corpse,
            SourceObjectKind::Corpse,
            Some("f4_global_corpse_lookup"),
        ),
    ] {
        let h = f.objects.construct(1, kind).unwrap();
        let guid = if high == HighGuid::MoTransport || high == HighGuid::Corpse {
            SourceGuid::without_entry(high, h.construction as u32)
        } else {
            SourceGuid::with_entry(high, 1901, h.construction as u32)
        };
        f.objects
            .assign_guid(h, h.construction as u32, guid)
            .unwrap();
        f.objects.get_mut(h).unwrap().map = Some(map_id());
        f.objects.add_to_world(h).unwrap();
        let result = start(&mut e, h, None, 0, &mut f);
        if let Some(error) = error {
            assert_eq!(result.unwrap_err(), script_missing(error));
        } else {
            assert!(result.unwrap());
        }
    }
    assert!(f.host.effects.is_empty());
}
#[test]
fn direct_ai_event_uses_null_invoker_and_preserves_all_u32_payload_bits() {
    let definitions = maps(vec![row(1, 1, 0)]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    let mut r = row(35, u32::MAX, 0);
    r.data[2] = 0xfedc_ba98;
    run(&mut e, &r, Some(f.h[0]), Some(f.h[1]), &mut f).unwrap();
    assert_eq!(
        f.host.received,
        [(f.h[1], f.h[0], None, u32::MAX, 0xfedc_ba98)]
    );
}

#[test]
fn item_owner_is_required_and_field_commands_use_the_preserved_item_pointer() {
    let definitions = maps(vec![row(2, 10, 55)]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    let item = f.objects.construct(1, SourceObjectKind::Item).unwrap();
    f.objects
        .assign_guid(item, 0, SourceGuid::without_entry(HighGuid::Item, 9))
        .unwrap();
    assert_eq!(
        start(&mut e, item, Some(f.h[1]), 0, &mut f).unwrap_err(),
        script_missing("f5_26_item")
    );
    e.execute_command(
        &row(2, 10, 55),
        None,
        Some(f.h[1]),
        false,
        Some(item),
        &mut DbScriptContext {
            objects: &mut f.objects,
            units: &mut f.units,
            owner: &mut f.host,
        },
    )
    .unwrap();
    assert_eq!(f.host.fields[&(item, 10)], 55);
}

#[test]
fn close_door_refuses_only_after_its_current_state_guard_and_stand_narrows_to_byte() {
    let definitions = maps(vec![row(1, 1, 0)]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    assert!(!run(&mut e, &row(12, 0, 0), Some(f.h[4]), None, &mut f).unwrap());
    f.host.go_state = 0;
    assert_eq!(
        run(&mut e, &row(12, 0, 0), Some(f.h[4]), None, &mut f).unwrap_err(),
        script_missing("f4_gameobject_lifecycle")
    );
    run(&mut e, &row(28, 300, 0), Some(f.h[0]), None, &mut f).unwrap();
    assert_eq!(f.host.effects.last().unwrap().0, DbScriptCall::Stand(44));
    for value in [4, 7, 8, u32::MAX] {
        assert_eq!(
            run(&mut e, &row(47, value, 0), Some(f.h[0]), None, &mut f)
                .unwrap_err()
                .id,
            "execution.db_script_executor.interrupt_spell_index_undefined"
        );
    }
}

#[test]
fn absent_string_list_skips_termination_but_absent_group_entry_is_undefined() {
    struct Absent;
    impl AssistanceExecutionOwner for Absent {}
    impl DbScriptExecutionOwner for Absent {
        fn members(
            &mut self,
            _: MapIdentity,
            _: DbScriptMembers,
        ) -> Result<Option<Vec<ObjectHandle>>> {
            Ok(None)
        }
    }
    let definitions = maps(vec![row(1, 1, 0)]);
    let e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    let mut absent = Absent;
    let mut context = DbScriptContext {
        objects: &mut f.objects,
        units: &mut f.units,
        owner: &mut absent,
    };
    let string = DbScriptRow {
        buddy: 1,
        flags: 0x800,
        ..row(31, 0, 0)
    };
    assert!(e
        .buddies(&string, Some(f.h[0]), None, &mut context)
        .unwrap()
        .is_none());
    let group = DbScriptRow {
        flags: 0x100,
        ..string.clone()
    };
    assert_eq!(
        e.buddies(&group, Some(f.h[0]), None, &mut context)
            .unwrap_err()
            .id,
        "execution.db_script_executor.null_spawn_group_entry_undefined"
    );
    let mut context = DbScriptContext {
        objects: &mut f.objects,
        units: &mut f.units,
        owner: &mut f.host,
    };
    assert_eq!(
        e.buddies(&group, Some(f.h[0]), None, &mut context)
            .unwrap()
            .unwrap(),
        []
    );
    assert_eq!(
        e.buddies(&string, Some(f.h[0]), None, &mut context)
            .unwrap()
            .unwrap(),
        []
    );
}

#[test]
fn map_end_adapter_processes_only_scripts_and_delegates_other_owners() {
    struct Other;
    impl WeatherPacketOwner for Other {
        fn zone_id(&mut self, _: ObjectHandle, _: &SourceObjects) -> Result<u32> {
            Err(refuse("authored.weather_owner"))
        }
        fn send_weather(&mut self, _: ObjectHandle, _: [u8; 13]) -> Result<()> {
            Err(refuse("authored.weather_owner"))
        }
    }
    impl SourceMapEndWork for Other {
        fn map_end_phase(
            &mut self,
            _: MapEndPhase,
            _: MapIdentity,
            _: u32,
            _: &mut SourceObjects,
        ) -> Result<()> {
            Err(refuse("authored.other_map_phase"))
        }
    }
    let definitions = maps(vec![DbScriptRow {
        delay: 1,
        ..row(1, 7, 0)
    }]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    start(&mut e, f.h[0], None, 0, &mut f).unwrap();
    f.host.now = 1;
    let mut other = Other;
    let mut adapter = SourceDbScriptMapEnd {
        scripts: &mut e,
        units: &mut f.units,
        owner: &mut f.host,
        other_phases: &mut other,
    };
    assert_eq!(
        adapter
            .map_end_phase(MapEndPhase::ObjectFields, map_id(), 1, &mut f.objects)
            .unwrap_err()
            .id,
        "authored.other_map_phase"
    );
    adapter
        .map_end_phase(MapEndPhase::Scripts, map_id(), 1, &mut f.objects)
        .unwrap();
    assert_eq!(adapter.owner.effects.len(), 1);
    let wrong = MapIdentity { map: 1, ..map_id() };
    assert_eq!(
        adapter
            .map_end_phase(MapEndPhase::Scripts, wrong, 1, &mut f.objects)
            .unwrap_err()
            .id,
        "execution.db_script_executor.map_phase_binding"
    );
}

#[test]
fn delayed_internal_commands_copy_the_definition_not_its_callers_lifetime() {
    let definitions = maps(vec![row(1, 1, 0)]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    {
        let mut command = row(1, 7, 0);
        e.command_start(
            &command,
            10,
            f.h[0],
            None,
            &mut DbScriptContext {
                objects: &mut f.objects,
                units: &mut f.units,
                owner: &mut f.host,
            },
        )
        .unwrap();
        command.data[0] = 99;
        assert_eq!(e.inventory()["pending"][0]["definition"]["data"][0], 7);
        assert_eq!(command.data[0], 99);
    }
    f.host.now = 10;
    process(&mut e, &mut f).unwrap();
    assert_eq!(f.host.effects[0].0, DbScriptCall::Emote(7));
    e.validate().unwrap();
}

#[test]
fn nonfixed_enum_casts_refuse_only_at_the_reached_cast_and_cover_both_boundaries() {
    let definitions = maps(vec![row(1, 1, 0)]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    let source = Some(f.h[0]);
    let target = Some(f.h[1]);
    let mut home = row(3, 0, 0);
    home.text[0] = 1;
    home.data[2] = 3;
    assert_eq!(
        run(&mut e, &home, source, target, &mut f).unwrap_err(),
        script_missing("f5_05_motion")
    );
    home.data[2] = 4;
    assert_eq!(
        run(&mut e, &home, source, target, &mut f).unwrap_err().id,
        "execution.db_script_executor.forced_movement_enum_undefined"
    );
    let mut movement = row(20, 2, 0);
    movement.text[0] = 3;
    assert_eq!(
        run(&mut e, &movement, source, target, &mut f).unwrap_err(),
        script_missing("f5_05_motion")
    );
    movement.text[0] = -1;
    assert_eq!(
        run(&mut e, &movement, source, target, &mut f)
            .unwrap_err()
            .id,
        "execution.db_script_executor.forced_movement_enum_undefined"
    );
    movement.text[0] = 0;
    movement.data[0] = 31;
    f.host.formation = Some(true);
    assert_eq!(
        run(&mut e, &movement, source, target, &mut f).unwrap_err(),
        script_missing("f4_formation")
    );
    movement.data[0] = 32;
    assert_eq!(
        run(&mut e, &movement, source, target, &mut f)
            .unwrap_err()
            .id,
        "execution.db_script_executor.movement_type_enum_undefined"
    );
    f.host.formation = None;
    assert!(!run(&mut e, &movement, source, target, &mut f).unwrap());
    f.host.geometry = true;
    let mut dynamic = row(37, 0, 0);
    dynamic.text[0] = 3;
    assert_eq!(
        run(&mut e, &dynamic, source, target, &mut f).unwrap_err(),
        script_missing("f5_05_motion")
    );
    dynamic.text[0] = 4;
    assert_eq!(
        run(&mut e, &dynamic, source, target, &mut f)
            .unwrap_err()
            .id,
        "execution.db_script_executor.forced_movement_enum_undefined"
    );
    run(&mut e, &row(54, 3, 0), source, target, &mut f).unwrap();
    assert_eq!(f.host.effects.last().unwrap().0, DbScriptCall::Sheathe(3));
    assert_eq!(
        run(&mut e, &row(54, 4, 0), source, target, &mut f)
            .unwrap_err()
            .id,
        "execution.db_script_executor.sheathe_enum_undefined"
    );
}

#[test]
fn dependency_arguments_preserve_quest_failure_door_minimum_and_sound_precedence() {
    let definitions = maps(vec![row(1, 1, 0)]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    let source = Some(f.h[0]);
    let player = Some(f.h[3]);
    f.objects.mark_dead(f.h[0]).unwrap();
    assert_eq!(
        run(&mut e, &row(7, 123, 99), source, player, &mut f).unwrap_err(),
        script_missing("f5_24_quest")
    );
    assert_eq!(
        f.host.calls.last(),
        Some(&DbScriptCall::QuestExplored {
            quest: 123,
            fail: true
        })
    );
    f.objects.legacy_respawn(f.h[0]).unwrap();
    assert_eq!(
        run(&mut e, &row(7, 123, 99), source, player, &mut f).unwrap_err(),
        script_missing("f6_distance")
    );
    assert_eq!(
        run(&mut e, &row(11, 0, 1), Some(f.h[4]), None, &mut f).unwrap_err(),
        script_missing("f4_gameobject_lifecycle")
    );
    assert_eq!(
        f.host.calls.last(),
        Some(&DbScriptCall::Door {
            open: true,
            reset: 15,
            alternate: false
        })
    );
    assert_eq!(
        run(&mut e, &row(16, 7, 8), source, player, &mut f).unwrap_err(),
        script_missing("f6_zone")
    );
    let mut sound = row(16, 7, 1 | 2 | 4 | 8 | 16);
    sound.data[2] = 123;
    assert_eq!(
        run(&mut e, &sound, source, player, &mut f).unwrap_err(),
        script_missing("presentation_sound")
    );
    assert_eq!(
        f.host.calls.last(),
        Some(&DbScriptCall::Sound {
            id: 7,
            kind: DbScriptSoundKind::Distance,
            audience: DbScriptSoundAudience::Area(123)
        })
    );
    sound.flags = 8;
    assert_eq!(
        run(&mut e, &sound, source, player, &mut f).unwrap_err(),
        script_missing("presentation_sound")
    );
    assert_eq!(
        f.host.calls.last(),
        Some(&DbScriptCall::Sound {
            id: 7,
            kind: DbScriptSoundKind::Music,
            audience: DbScriptSoundAudience::Area(123)
        })
    );
    assert_eq!(
        run(&mut e, &row(8, 0, 0), source, player, &mut f).unwrap_err(),
        script_missing("f5_24_quest")
    );
    assert_eq!(
        f.host.calls.last(),
        Some(&DbScriptCall::KillCredit {
            entry: 901,
            group: false
        })
    );
}

#[test]
fn source_ai_null_refuses_before_direct_or_around_receivers() {
    let definitions = maps(vec![row(1, 1, 0)]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    f.units.get_mut(&f.objects, f.h[0]).unwrap().has_ai = false;
    for radius in [0, 10] {
        assert_eq!(
            run(
                &mut e,
                &row(35, 7166, radius),
                Some(f.h[0]),
                Some(f.h[1]),
                &mut f
            )
            .unwrap_err()
            .id,
            "execution.db_script_executor.source_ai_null_undefined"
        );
    }
    assert!(f.host.received.is_empty());
    f.units.get_mut(&f.objects, f.h[0]).unwrap().has_ai = true;
    run(
        &mut e,
        &row(35, 7166, 0),
        Some(f.h[0]),
        Some(f.h[1]),
        &mut f,
    )
    .unwrap();
    assert_eq!(f.host.received[0].3, 7166);
}

#[test]
fn gossip_loaded_step_resolves_the_registered_player_before_ai_delivery() {
    let script = DbScriptRow {
        id: 7166,
        ..row(35, 7166, 0)
    };
    let definitions = SourceDbScriptMaps::load(
        vec![(DbScriptType::Gossip, vec![script])],
        SourceDbScriptTemplates::default(),
        &mut Loader,
    )
    .unwrap();
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    let player_guid = f.objects.get(f.h[3]).unwrap().guid.unwrap();
    assert_eq!(
        f.objects
            .lookup(map_id(), LiveLookupKind::Player, player_guid),
        Some(f.h[3])
    );
    e.start(
        DbScriptType::Gossip,
        7166,
        f.h[0],
        Some(f.h[3]),
        0,
        &mut DbScriptContext {
            objects: &mut f.objects,
            units: &mut f.units,
            owner: &mut f.host,
        },
    )
    .unwrap();
    assert_eq!(f.host.received, [(f.h[0], f.h[3], Some(f.h[3]), 7166, 0)]);
}

#[test]
fn all_dead_entry_buddies_distinguish_corpses_from_other_nonlive_states() {
    let r = DbScriptRow {
        buddy: 902,
        radius_or_guid: 10,
        flags: 0x40 | 0x200,
        ..row(1, 7, 0)
    };
    let definitions = maps(vec![r]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    f.host.geometry = true;
    f.host.visitor = vec![f.h[1], f.h[2]];
    for h in [f.h[1], f.h[2]] {
        f.objects.mark_dead(h).unwrap();
        f.host.entries.insert(h, 902);
    }
    f.host.corpses.insert(f.h[1]);
    start(&mut e, f.h[0], None, 0, &mut f).unwrap();
    assert_eq!(
        f.host.effects.iter().map(|r| r.1).collect::<Vec<_>>(),
        [Some(f.h[1])]
    );
    struct WithoutCorpse<'a>(&'a mut Host);
    impl AssistanceExecutionOwner for WithoutCorpse<'_> {}
    impl DbScriptExecutionOwner for WithoutCorpse<'_> {
        fn entry(&mut self, h: ObjectHandle) -> Result<u32> {
            DbScriptExecutionOwner::entry(self.0, h)
        }
        fn visit(
            &mut self,
            h: ObjectHandle,
            radius: u32,
            kind: DbScriptVisit,
        ) -> Result<Vec<ObjectHandle>> {
            DbScriptExecutionOwner::visit(self.0, h, radius, kind)
        }
    }
    let mut owner = WithoutCorpse(&mut f.host);
    let step = &definitions.map(DbScriptType::Relay).unwrap().scripts()[&100][0];
    assert_eq!(
        e.buddies(
            step,
            Some(f.h[0]),
            None,
            &mut DbScriptContext {
                objects: &mut f.objects,
                units: &mut f.units,
                owner: &mut owner
            }
        )
        .unwrap_err(),
        script_missing("f4_creature_death_state")
    );
}

#[test]
fn scheduling_uniqueness_enum_refuses_above_its_defined_domain() {
    let definitions = maps(vec![row(1, 7, 0)]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    start(&mut e, f.h[0], None, 3, &mut f).unwrap();
    let delivered = f.host.effects.len();
    for unique in [4, u8::MAX] {
        assert_eq!(
            start(&mut e, f.h[0], None, unique, &mut f).unwrap_err().id,
            "execution.db_script_executor.uniqueness_enum_undefined"
        );
    }
    assert_eq!(f.host.effects.len(), delivered);
}
