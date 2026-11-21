// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::native::source_execution::creature_tests::{chicken_binding, Random};

fn live(stand_state: u8, health: u32, max_health: u32) -> SourceUnitFields {
    let mut live = CreatureConstruction::construct_loaded_unit();
    live.stand_state = stand_state;
    live.flags = 2;
    live.vitals = Some(SourceUnitVitals {
        health,
        max_health,
        mana: 0,
        max_mana: 0,
        has_mana: false,
        energy: 0,
        max_energy: 0,
    });
    live
}

#[test]
fn garments_constructor_and_respawn_reset_the_installed_ai() {
    let map = MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    };
    let cells = SourceCells::new(map).unwrap();
    let mut binding = chicken_binding();
    binding.name = "npc_garments_of_quests".into();
    binding.script_id = 285; // Trace :61479761.
    let binding = Arc::new(binding);
    let template = CreatureAiTemplateOperands {
        ordinal: 0,
        entry: 12423,
        faction: 12,
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
        creature_type: 7,
        ai_name: String::new(),
        script_name: binding.name.clone(),
    };
    let bytes = b"LogFilter_EventAiDev=1\nGuarderSight=50\n";
    let config = EventAiLoaderConfig::parse(bytes, Sha256Id::of(bytes)).unwrap();
    // SQL :429338/:437897/:448247, with an authored standing arm.
    for stand in [8, 0] {
        let mut objects = SourceObjects::default();
        let unit = objects.construct(6, SourceObjectKind::Creature).unwrap();
        objects.bind_map(unit, map, &cells).unwrap();
        let guid = SourceGuid::with_entry(HighGuid::Creature, 12423, 81085);
        objects.assign_guid(unit, 81085, guid).unwrap();
        objects.add_to_world(unit).unwrap();
        let mut units = SourceUnits::default();
        let mut fields = live(stand, 137, 137);
        fields.entry = template.entry;
        units.register(&objects, unit, fields).unwrap();
        let mut random = Random::new();
        let mut installed = SourceCreatureAi::construct(
            &objects,
            unit,
            &mut units,
            &template,
            false,
            0,
            &config,
            &mut random,
        )
        .unwrap();
        assert_eq!(random.cursor.raw, 2); // Trace :61479774/:61479784.
        let script = SourceScriptedAi::construct(
            SourceScriptedAiKind::for_registered_name(&binding.name).unwrap(),
            &binding,
            &mut objects,
            &mut units,
            &mut installed.base,
            &mut random,
            &mut (),
        )
        .unwrap();
        let SourceScriptedAi::GarmentsOfQuests(mut ai) = script else {
            panic!("expected Garments");
        };
        assert_eq!(
            ai.escort,
            EscortAi {
                evade_check_cooldown_ms: 2500,
                quest: None,
                player_guid: None,
                player_check_timer_ms: 1000,
                state: 0,
                running: false,
                can_instant_respawn: false,
                can_return_to_start: false,
                waypoint_path_id: 0,
                current_escort_waypoint_path: 0,
            }
        );
        assert_eq!(
            (
                ai.quest.player_guid,
                ai.quest.healed,
                ai.quest.can_run,
                ai.quest.run_away_timer_ms
            ),
            (None, false, false, 5000)
        );
        let fields = units.get(&objects, unit).unwrap();
        assert_eq!(
            (
                fields.stand_state,
                fields.flags,
                fields.vitals.unwrap().health
            ),
            (8, 0x1002, 95)
        );
        let object = objects.get(unit).unwrap();
        assert_eq!(
            object.field_changed(UNIT_FIELD_BYTES_1).unwrap(),
            stand != 8
        );
        assert!(object.field_changed(UNIT_FIELD_FLAGS).unwrap());
        assert!(object.field_changed(UNIT_FIELD_HEALTH).unwrap());
        assert!(objects.client_update_pending(map, unit));

        ai.quest.player_guid = Some(guid);
        ai.quest.healed = true;
        ai.quest.can_run = true;
        ai.quest.run_away_timer_ms = 23;
        installed.base.generic_timers[0].remaining = 123;
        installed.base.generic_timers[1].disabled = false;
        // Clear the update marks before testing both the equal and changed respawn paths.
        objects.remove_from_world(unit).unwrap();
        objects.add_to_world(unit).unwrap();
        if stand == 0 {
            let fields = units.get_mut(&objects, unit).unwrap();
            fields.stand_state = 0;
            fields.flags &= !0x1000;
            fields.vitals.as_mut().unwrap().health = 137;
        }
        let mut ais = SourceCreatureAis::default();
        installed.loaded_class =
            LoadedCreatureAiClass::Scripted(SourceScriptedAi::GarmentsOfQuests(ai));
        ais.install(
            &objects,
            &mut units,
            installed,
            &mut SourceCombat::default(),
        )
        .unwrap();
        ais.loaded_just_respawned(&mut objects, &mut units, unit, &mut random, None, &mut ())
            .unwrap();
        assert_eq!(random.cursor.raw, 4); // Trace :61479814/:61479824.
        let installed = ais.get(&objects, unit).unwrap();
        assert_eq!(
            installed
                .base
                .generic_timers
                .map(|timer| (timer.id, timer.remaining, timer.disabled)),
            [(2000, 0, true), (2001, 0, true)]
        );
        let LoadedCreatureAiClass::Scripted(SourceScriptedAi::GarmentsOfQuests(ai)) =
            &installed.loaded_class
        else {
            panic!("expected installed Garments");
        };
        assert_eq!(
            (
                ai.quest.player_guid,
                ai.quest.healed,
                ai.quest.can_run,
                ai.quest.run_away_timer_ms
            ),
            (None, false, false, 5000)
        );
        let fields = units.get(&objects, unit).unwrap();
        assert_eq!(
            (
                fields.stand_state,
                fields.flags,
                fields.vitals.unwrap().health
            ),
            (8, 0x1002, 95)
        );
        let object = objects.get(unit).unwrap();
        for field in [UNIT_FIELD_BYTES_1, UNIT_FIELD_FLAGS, UNIT_FIELD_HEALTH] {
            assert_eq!(object.field_changed(field).unwrap(), stand == 0);
        }
        assert_eq!(objects.client_update_pending(map, unit), stand == 0);
    }
}

#[test]
fn garments_health_uses_binary64_and_refuses_an_out_of_range_int_before_writing() {
    for (max_health, expected) in [
        (90, Ok(62)),
        (3_067_833_782, Ok(2_147_483_647)),
        (
            3_067_833_783,
            Err("execution.ai.npcs_special_592.health_int_out_of_range_undefined"),
        ),
    ] {
        let mut objects = SourceObjects::default();
        let unit = objects.construct(6, SourceObjectKind::Creature).unwrap();
        let mut units = SourceUnits::default();
        units
            .register(&objects, unit, live(0, max_health, max_health))
            .unwrap();
        let result =
            GarmentsOfQuestsAi::construct(&Arc::new(chicken_binding()), unit, &objects, &mut units);
        let fields = units.get(&objects, unit).unwrap();
        assert_eq!(
            result
                .as_ref()
                .map(|_| fields.vitals.unwrap().health)
                .map_err(|error| error.id.as_str()),
            expected
        );
        assert_eq!((fields.stand_state, fields.flags), (8, 0x1002));
        if expected.is_err() {
            assert_eq!(fields.vitals.unwrap().health, max_health);
            assert!(!objects
                .get(unit)
                .unwrap()
                .field_changed(UNIT_FIELD_HEALTH)
                .unwrap());
        }
    }
}

#[test]
fn garments_reset_keeps_unit_writes_before_player_and_charmer_refusals() {
    let mut objects = SourceObjects::default();
    let map = MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    };
    let cells = SourceCells::new(map).unwrap();
    let player = objects.construct(6, SourceObjectKind::Player).unwrap();
    let player_guid = SourceGuid::without_entry(HighGuid::Player, 7);
    objects.assign_guid(player, 0, player_guid).unwrap();
    objects.bind_map(player, map, &cells).unwrap();
    objects.add_to_world(player).unwrap();
    objects.register_player_lookup(player).unwrap();
    let creature = objects.construct(6, SourceObjectKind::Creature).unwrap();
    objects
        .assign_guid(
            creature,
            81085,
            SourceGuid::with_entry(HighGuid::Creature, 12423, 81085),
        )
        .unwrap();
    objects.bind_map(creature, map, &cells).unwrap();
    objects.add_to_world(creature).unwrap();

    let mut units = SourceUnits::default();
    let binding = Arc::new(chicken_binding());
    units.register(&objects, player, live(0, 137, 137)).unwrap();
    let mut charmed = live(8, 137, 137);
    charmed.control_guids.charmer = player_guid;
    units.register(&objects, creature, charmed).unwrap();
    assert_eq!(
        GarmentsOfQuestsAi::construct(&binding, player, &objects, &mut units)
            .unwrap_err()
            .id,
        "execution.unit.Unit_10190.SendPacket_owner"
    );
    let fields = units.get(&objects, player).unwrap();
    assert_eq!(
        (
            fields.stand_state,
            fields.flags,
            fields.vitals.unwrap().health
        ),
        (8, 2, 137)
    );
    for (unit, owner) in [
        (player, "execution.unit.Unit_9235.GetGroup_owner"),
        (creature, "execution.unit.Unit_9241.GetGroup_owner"),
    ] {
        assert_eq!(
            GarmentsOfQuestsAi::construct(&binding, unit, &objects, &mut units)
                .unwrap_err()
                .id,
            owner
        );
        let fields = units.get(&objects, unit).unwrap();
        assert_eq!((fields.flags, fields.vitals.unwrap().health), (0x1002, 95));
        assert!(objects
            .get(unit)
            .unwrap()
            .field_changed(UNIT_FIELD_HEALTH)
            .unwrap());
    }
    objects.remove_from_world(player).unwrap();
    GarmentsOfQuestsAi::construct(&binding, creature, &objects, &mut units).unwrap();
}

#[test]
fn garments_unported_callbacks_refuse_by_source_name() {
    let mut objects = SourceObjects::default();
    let unit = objects.construct(6, SourceObjectKind::Creature).unwrap();
    let mut units = SourceUnits::default();
    units.register(&objects, unit, live(8, 137, 137)).unwrap();
    let ai =
        GarmentsOfQuestsAi::construct(&Arc::new(chicken_binding()), unit, &objects, &mut units)
            .unwrap();
    let mut script = SourceScriptedAi::GarmentsOfQuests(ai);
    assert_eq!(
        script.update_ai(1).unwrap_err().id,
        "execution.ai.escort_ai_129.UpdateAI_owner"
    );
    let SourceScriptedAi::GarmentsOfQuests(mut ai) = script else {
        panic!("expected Garments");
    };
    assert_eq!(
        ai.escort.movement_inform(0, 0).unwrap_err().id,
        "execution.ai.escort_ai_182.MovementInform_owner"
    );
    assert_eq!(
        ai.escort.set_current_waypoint(0).unwrap_err().id,
        "execution.ai.escort_ai_205.SetCurrentWaypoint_owner"
    );
    assert_eq!(
        ai.escort.get_player_for_escort().unwrap_err().id,
        "execution.ai.escort_ai_62.GetPlayerForEscort_owner"
    );
    assert_eq!(
        ai.spell_hit(unit, 0).unwrap_err().id,
        "execution.ai.npcs_special_595.SpellHit_owner"
    );
}
