use super::*;

#[test]
fn pool_group_and_string_members_obey_their_distinct_filters() {
    let definitions = maps(vec![row(1, 1, 0)]);
    let e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    f.host.geometry = true;
    f.host.members = vec![f.h[2], f.h[1]];
    f.objects.mark_dead(f.h[2]).unwrap();
    let mut context = DbScriptContext {
        objects: &mut f.objects,
        units: &mut f.units,
        owner: &mut f.host,
    };
    let pool = DbScriptRow {
        buddy: 901,
        radius_or_guid: 1,
        flags: 0x80,
        ..row(1, 1, 0)
    };
    assert_eq!(
        e.buddies(&pool, Some(f.h[0]), None, &mut context)
            .unwrap()
            .unwrap(),
        [Some(f.h[1])]
    );
    let group = DbScriptRow {
        buddy: 1,
        flags: 0x100 | 0x200,
        ..row(1, 1, 0)
    };
    assert_eq!(
        e.buddies(&group, Some(f.h[0]), None, &mut context)
            .unwrap()
            .unwrap(),
        [Some(f.h[2]), Some(f.h[1])]
    );
    let string = DbScriptRow {
        buddy: 1,
        flags: 0x800 | 0x200,
        ..row(1, 1, 0)
    };
    assert_eq!(
        e.buddies(&string, Some(f.h[0]), None, &mut context)
            .unwrap()
            .unwrap(),
        [Some(f.h[1])]
    );
    let bounded = DbScriptRow {
        radius_or_guid: 1,
        ..string
    };
    assert!(e
        .buddies(&bounded, Some(f.h[0]), None, &mut context)
        .unwrap()
        .is_none());
}
#[test]
fn command_fields_text_emote_and_flag_toggle_have_observable_effects() {
    let definitions = maps(vec![row(1, 1, 0)]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    let source = Some(f.h[0]);
    let target = Some(f.h[1]);
    for (command, a, b, value) in [
        (2, 10, 0x55, 0x55),
        (4, 10, 0x80, 0xd5),
        (5, 10, 0x50, 0x85),
    ] {
        run(&mut e, &row(command, a, b), source, target, &mut f).unwrap();
        assert_eq!(f.host.fields[&(f.h[0], 10)], value);
    }
    let mut talk = row(0, 0, 0);
    talk.text = [10, 20, 30, 0];
    f.host.words.push_back(2);
    run(&mut e, &talk, source, target, &mut f).unwrap();
    assert_eq!(
        f.host.effects.last().unwrap().0,
        DbScriptCall::DisplayText(30)
    );
    let mut emote = row(1, 4, 0);
    emote.text = [5, 6, 0, 7];
    f.host.words.push_back(2);
    run(&mut e, &emote, source, None, &mut f).unwrap();
    assert_eq!(f.host.effects.last().unwrap().0, DbScriptCall::Emote(6));
    run(&mut e, &row(1, 4, 0), source, None, &mut f).unwrap();
    assert_eq!(f.host.draws.last(), Some(&(0, 0)));
    for command in [29, 48] {
        run(&mut e, &row(command, 3, 1), source, None, &mut f).unwrap();
        run(&mut e, &row(command, 1, 2), source, None, &mut f).unwrap();
        let field = if command == 29 { 0x93 } else { 0x2e };
        assert_eq!(f.host.fields[&(f.h[0], field)], 2);
    }
    run(&mut e, &row(27, 5, 0), Some(f.h[4]), None, &mut f).unwrap();
    assert_eq!(f.host.fields[&(f.h[4], 9)], 0x12);
    run(&mut e, &row(27, 10, 0), Some(f.h[4]), None, &mut f).unwrap();
    assert_eq!(f.host.fields[&(f.h[4], 9)], 0);
}
#[test]
fn scalar_commands_and_source_empty_cases_preserve_exact_calls() {
    let definitions = maps(vec![row(1, 1, 0)]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    for (command, a, b, expected) in [
        (25, 0, 0, DbScriptCall::SetWalk(true)),
        (28, 7, 0, DbScriptCall::Stand(7)),
        (54, 2, 0, DbScriptCall::Sheathe(2)),
        (55, 99, 1, DbScriptCall::StringId(99, true)),
    ] {
        run(&mut e, &row(command, a, b), Some(f.h[0]), None, &mut f).unwrap();
        assert_eq!(f.host.effects.last().unwrap().0, expected);
    }
    let mut world = row(53, 0, 0);
    world.text = [-1, -2, 0, 0];
    run(&mut e, &world, Some(f.h[0]), None, &mut f).unwrap();
    assert_eq!(
        f.host.effects.last().unwrap().0,
        DbScriptCall::WorldState(u32::MAX, u32::MAX - 1)
    );
    let count = f.host.effects.len();
    for command in [19, 33, 57, 999] {
        assert!(!run(&mut e, &row(command, 0, 0), Some(f.h[0]), None, &mut f).unwrap());
    }
    assert_eq!(f.host.effects.len(), count);
}
#[test]
fn conditions_gate_effects_and_conditional_termination_uses_player_precedence() {
    let definitions = maps(vec![row(1, 1, 0)]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    let guarded = DbScriptRow {
        condition: 99,
        ..row(1, 1, 0)
    };
    f.host.condition = Some(false);
    run(&mut e, &guarded, Some(f.h[0]), None, &mut f).unwrap();
    assert!(f.host.effects.is_empty());
    f.host.condition = Some(true);
    run(&mut e, &guarded, Some(f.h[0]), None, &mut f).unwrap();
    assert_eq!(f.host.effects.len(), 1);
    assert!(run(&mut e, &row(34, 99, 0), Some(f.h[0]), Some(f.h[3]), &mut f).unwrap());
    let inverse = DbScriptRow {
        flags: 8,
        ..row(34, 99, 0)
    };
    assert!(!run(&mut e, &inverse, Some(f.h[0]), Some(f.h[3]), &mut f).unwrap());
    let refusal = run(&mut e, &row(34, 99, 1), Some(f.h[0]), Some(f.h[3]), &mut f).unwrap_err();
    assert_eq!(refusal, script_missing("f5_24_quest"));
}
#[test]
fn random_templates_try_explicit_chances_then_equal_fallback_and_empty_has_no_draw() {
    let template = SourceDbScriptTemplates::load([
        DbScriptRandomRow {
            ordinal: 0,
            id: 7,
            kind: 0,
            target: 12,
            chance: 20,
        },
        DbScriptRandomRow {
            ordinal: 1,
            id: 7,
            kind: 0,
            target: 13,
            chance: 0,
        },
    ])
    .unwrap();
    let definitions = SourceDbScriptMaps::load(
        vec![(DbScriptType::Relay, vec![row(0, 7, 0)])],
        template,
        &mut Loader,
    )
    .unwrap();
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    run(&mut e, &row(0, 7, 0), Some(f.h[0]), None, &mut f).unwrap();
    assert_eq!(f.host.effects[0].0, DbScriptCall::DisplayText(13));
    assert_eq!(f.host.draws, [(0, 0)]);
    run(&mut e, &row(0, 999, 0), Some(f.h[0]), None, &mut f).unwrap();
    assert_eq!(f.host.effects.len(), 1);
    assert_eq!(f.host.draws.len(), 1);
}
#[test]
fn every_dependent_command_refuses_at_its_reached_owner_not_at_opcode_dispatch() {
    let definitions = maps(vec![row(1, 1, 0)]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    let cases = [
        (3, 0, 0, 0, 1, "f5_05_facing"),
        (6, 0, 0, 0, 3, "f5_24_player_teleport"),
        (7, 1, 0, 0, 3, "f5_24_quest"),
        (8, 1, 0, 0, 3, "f5_24_quest"),
        (9, 0, 0, 4, 1, "f4_gameobject_lifecycle"),
        (10, 1, 0, 0, 1, "f5_15_summon"),
        (11, 0, 0, 4, 1, "f4_gameobject_lifecycle"),
        (13, 0, 0, 0, 4, "f4_gameobject_lifecycle"),
        (14, 1, 0, 0, 1, "f5_12_aura"),
        (15, 1, 0, 0, 1, "f5_08_spell"),
        (16, 1, 0, 0, 1, "presentation_sound"),
        (17, 1, 1, 0, 3, "f5_26_inventory"),
        (18, 0, 0, 0, 1, "f4_creature_lifecycle"),
        (20, 2, 0, 0, 1, "f5_05_motion"),
        (21, 1, 0, 0, 1, "f4_active_object"),
        (22, 1, 0, 0, 1, "temporary_faction"),
        (23, 0, 0, 0, 1, "unit_fields"),
        (24, 0, 0, 0, 1, "unit_fields"),
        (26, 0, 0, 0, 1, "f5_04_attack_eligibility"),
        (30, 1, 0, 0, 3, "f5_24_player_taxi"),
        (32, 1, 0, 0, 1, "f5_05_motion"),
        (36, 1, 0, 0, 1, "f5_05_motion"),
        (37, 1, 0, 0, 1, "f6_geometry"),
        (38, 1, 0, 0, 3, "f5_25_mail"),
        (39, 1, 0, 0, 1, "f5_05_movement_flags"),
        (40, 0, 0, 0, 4, "f4_gameobject_lifecycle"),
        (41, 0, 0, 0, 1, "f4_creature_lifecycle"),
        (42, 1, 0, 0, 1, "equipment"),
        (43, 0, 0, 0, 4, "f4_gameobject_lifecycle"),
        (44, 902, 0, 0, 1, "f4_update_entry"),
        (46, 1, 0, 0, 1, "f5_08_spell"),
        (47, 3, 0, 0, 1, "f5_08_spell"),
        (50, 0, 0, 0, 1, "f5_04_combat"),
        (51, 150, 0, 0, 1, "f4_formation"),
        (52, 1, 0, 0, 1, "f5_25_gossip"),
        (58, 1, 0, 0, 1, "f4_spawn_group"),
        (200, 1, 0, 0, 3, "f5_25_lfg"),
    ];
    for (command, a, b, s, t, owner) in cases {
        assert_eq!(
            run(
                &mut e,
                &row(command, a, b),
                Some(f.h[s]),
                Some(f.h[t]),
                &mut f
            )
            .unwrap_err(),
            script_missing(owner),
            "command {command}"
        );
    }
    assert!(!run(&mut e, &row(12, 0, 0), Some(f.h[4]), None, &mut f).unwrap());
    assert!(!run(&mut e, &row(3, 0, 0), Some(f.h[4]), None, &mut f).unwrap());
}
#[test]
fn undefined_domains_refuse_on_both_sides_without_repairing_source_operands() {
    let definitions = maps(vec![row(1, 1, 0)]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    assert_eq!(
        run(&mut e, &row(3, 0, 0), Some(f.h[3]), None, &mut f)
            .unwrap_err()
            .id,
        "execution.db_script_executor.move_to_creature_downcast_undefined"
    );
    assert_eq!(
        run(&mut e, &row(3, 0, 0), Some(f.h[0]), None, &mut f).unwrap_err(),
        script_missing("f5_05_facing")
    );
    let dynamic = DbScriptRow {
        flags: 8,
        ..row(37, 0, 0)
    };
    assert_eq!(
        run(&mut e, &dynamic, Some(f.h[0]), Some(f.h[1]), &mut f)
            .unwrap_err()
            .id,
        "execution.db_script_executor.move_dynamic_uninitialized_z_undefined"
    );
    assert_eq!(
        run(&mut e, &row(37, 0, 0), Some(f.h[0]), Some(f.h[1]), &mut f).unwrap_err(),
        script_missing("f6_geometry")
    );
    assert!(!run(&mut e, &dynamic, Some(f.h[0]), Some(f.h[0]), &mut f).unwrap());
    for command in [21, 49, 55, 58] {
        assert!(run(&mut e, &row(command, 1, 0), None, Some(f.h[1]), &mut f)
            .unwrap_err()
            .id
            .ends_with("_undefined"));
    }
    f.units.get_mut(&f.objects, f.h[1]).unwrap().has_ai = false;
    assert_eq!(
        run(&mut e, &row(35, 1, 0), Some(f.h[0]), Some(f.h[1]), &mut f)
            .unwrap_err()
            .id,
        "execution.db_script_executor.receiver_ai_null_undefined"
    );
    f.units.get_mut(&f.objects, f.h[1]).unwrap().has_ai = true;
    run(&mut e, &row(35, 1, 0), Some(f.h[0]), Some(f.h[1]), &mut f).unwrap();
}
#[test]
fn command35_raw_gossip7166_reaches_existing_eventai_receiver_unchanged() {
    let definitions = maps(vec![row(1, 1, 0)]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    let mut ai = crate::native::source_execution::event_ai_dispatch_tests::state();
    ai.holders[0].event.kind = 30;
    ai.holders[0].event.params = [7166, 0, 0, 0, 0, 0];
    ai.holders[0].event.chance = 100;
    ai.holders[0].event.actions[0] = EventAiAction {
        kind: 22,
        params: Some([9, 0, 0]),
    };
    f.host.event_ai = Some(ai);
    let script = DbScriptRow {
        id: 7166,
        ..row(35, 7166, 0)
    };
    run(&mut e, &script, Some(f.h[0]), Some(f.h[3]), &mut f).unwrap();
    assert_eq!(f.host.received, [(f.h[0], f.h[3], Some(f.h[3]), 7166, 0)]);
    assert_eq!(f.host.event_ai.as_ref().unwrap().phase, 9);
    assert_eq!(f.host.event_words, 2);
    f.host.event_ai = None;
    f.host.received.clear();
    f.host.visitor = vec![f.h[1], f.h[2]];
    f.host.geometry = true;
    run(
        &mut e,
        &row(35, 7166, 10),
        Some(f.h[0]),
        Some(f.h[3]),
        &mut f,
    )
    .unwrap();
    assert_eq!(
        f.host
            .received
            .iter()
            .map(|r| (r.0, r.3))
            .collect::<Vec<_>>(),
        [(f.h[1], 7166), (f.h[2], 7166)]
    );
}
