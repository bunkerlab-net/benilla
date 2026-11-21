// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::native::production_compiler::VerifiedCatalog;

const MAP: MapIdentity = MapIdentity {
    map: 0,
    instance: 0,
    creation_generation: 1,
};
const POSE: SourcePose = SourcePose([0xc60b_d7cd, 0xc304_7e35, 0x42a7_0ff9, 0]);
const CONFIG: SourcePlayerCreateConfig = SourcePlayerCreateConfig {
    save_interval: 900_000,
    start_level: 1,
    start_money: 0,
    max_primary_professions: 2,
};

fn request() -> SourcePlayerCreation {
    SourcePlayerCreation {
        guid_low: 1,
        name: "Testmage".into(),
        race: 1,
        class: 8,
        gender: 0,
        appearance: [0; 5],
    }
}

fn inputs() -> PlayerCreateRows {
    let mut rows = PlayerCreateRows::default();
    rows.races.insert(
        1,
        PlayerCreateRace {
            faction: 1,
            display: [49, 50],
            team: 7,
            taxi_mask: 2,
        },
    );
    rows.classes.insert(8, 0);
    rows.starts.insert(
        (1, 8),
        PlayerCreateStart {
            map: 0,
            zone: 12,
            pose: POSE,
        },
    );
    rows.class_levels.insert((8, 1), [31, 100]);
    rows.levels.insert((1, 8, 1), [20, 20, 20, 23, 22]);
    rows.xp.insert(1, 400);
    rows.display_scales.insert(49, [1.0, 1.0]);
    rows
}

fn construct(objects: &mut SourceObjects, units: &mut SourceUnits) -> SourcePlayer {
    SourcePlayer::construct(
        objects,
        units,
        1,
        CONFIG.save_interval,
        |min, max| {
            assert_eq!((min, max), (450_000, 1_350_000));
            Ok(723_456)
        },
        || Ok(101),
    )
    .unwrap()
}

#[test]
fn captured_appearance_gate_accepts_only_the_recorded_tuple() {
    SourcePlayerCreation::check_appearance(1, 8, 0, [0; 5]).unwrap();
    let error = SourcePlayerCreation::check_appearance(1, 8, 1, [0; 5]).unwrap_err();
    assert_eq!(error.id, "execution.player.create_appearance_domain");
}

#[test]
fn human_mage_initializes_level_one_before_learning() {
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let cells = SourceCells::new(MAP).unwrap();
    let mut player = construct(&mut objects, &mut units);
    assert_eq!(player.login_time, 101);
    assert_eq!(player.last_tick, 101);
    assert_eq!(player.next_save_ms, 723_456);
    let models =
        LoadedCreatureModels::load(Vec::new(), &Default::default(), &Default::default()).unwrap();
    let error = player
        .create(
            &mut objects,
            &mut units,
            request(),
            &inputs(),
            &models,
            &cells,
            CONFIG,
            || Ok(107),
        )
        .unwrap_err();
    assert_eq!(
        error.id,
        "execution.player.Player_850.learn_default_skills_owner"
    );
    let handle = player.object();
    let object = objects.get(handle).unwrap();
    let unit = units.get(&objects, handle).unwrap();
    assert_eq!(object.kind, SourceObjectKind::Player);
    assert!(object.active);
    assert_eq!(object.guid.unwrap().raw(), 1);
    assert_eq!(object.database_guid, 1);
    assert_eq!(object.map, Some(MAP));
    assert_eq!(object.pose, Some(POSE));
    assert_eq!(object.zone_update_id, 0);
    assert!(!object.in_world);
    assert_eq!(cells.registered_cell(handle), None);
    assert_eq!(objects.map_players(MAP).count(), 0);
    assert_eq!(objects.find_player(object.guid.unwrap()).unwrap(), None);
    assert_eq!(player.name, "Testmage");
    assert_eq!(player.race, 1);
    assert_eq!(unit.unit_class, Some(8));
    assert_eq!(player.gender, 0);
    assert_eq!(unit.power_type, Some(0));
    assert_eq!(unit.faction, 1);
    assert_eq!(unit.flags, 8);
    assert_eq!(player.native_display, 49);
    assert_eq!(player.display, 49);
    assert_eq!(player.bytes_2, [0, 0, 0, 2]);
    assert_eq!(player.spell_family, 3);
    assert_eq!(player.create_stats, [20, 20, 20, 23, 22]);
    assert_eq!(unit.resistances.armor(), 40);
    let power = unit.power_fields.as_ref().unwrap();
    assert_eq!(
        (power.level, power.create_health, power.create_mana),
        (1, 31, 100)
    );
    let vitals = unit.vitals.unwrap();
    assert_eq!((vitals.health, vitals.max_health), (51, 51));
    assert_eq!((vitals.mana, vitals.max_mana), (165, 165));
    assert_eq!((vitals.energy, vitals.max_energy), (100, 100));
    assert_eq!(player.next_level_xp, 400);
    assert_eq!(player.last_tick, 107);
    assert_eq!(player.free_primary_professions, 2);
    assert!(object.field_changed(UNIT_FIELD_BYTES_0).unwrap());
    assert!(object.field_changed(UNIT_FIELD_HEALTH).unwrap());
    assert!(!object.field_changed(PLAYER_BYTES).unwrap());
}

#[test]
fn invalid_race_class_and_gender_stop_before_map_binding() {
    for (race, class, gender, expected) in [
        (9, 8, 0, "execution.player.Player_755.race"),
        (1, 10, 0, "execution.player.Player_755.class"),
        (1, 8, 2, "execution.player.Player_770.gender"),
    ] {
        let mut objects = SourceObjects::default();
        let mut units = SourceUnits::default();
        let cells = SourceCells::new(MAP).unwrap();
        let mut player = construct(&mut objects, &mut units);
        let models =
            LoadedCreatureModels::load(Vec::new(), &Default::default(), &Default::default())
                .unwrap();
        let request = SourcePlayerCreation {
            race,
            class,
            gender,
            ..request()
        };
        let error = player
            .create(
                &mut objects,
                &mut units,
                request,
                &inputs(),
                &models,
                &cells,
                CONFIG,
                || panic!("clock after validation"),
            )
            .unwrap_err();
        assert_eq!(error.id, expected);
        let object = objects.get(player.object()).unwrap();
        assert_eq!(object.guid.unwrap().raw(), 1);
        assert_eq!(object.map, None);
        assert_eq!(object.pose, None);
    }
}

#[test]
fn legal_pair_without_playercreateinfo_refuses() {
    let mut rows = inputs();
    rows.starts.clear();
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let cells = SourceCells::new(MAP).unwrap();
    let mut player = construct(&mut objects, &mut units);
    let models =
        LoadedCreatureModels::load(Vec::new(), &Default::default(), &Default::default()).unwrap();
    let error = player
        .create(
            &mut objects,
            &mut units,
            request(),
            &rows,
            &models,
            &cells,
            CONFIG,
            || panic!("clock after validation"),
        )
        .unwrap_err();
    assert_eq!(error.id, "execution.player.Player_755.playercreateinfo");
    assert_eq!(objects.get(player.object()).unwrap().pose, None);
}

#[test]
fn private_catalog_human_mage_matches_seal16_packets() {
    let Some(root) = std::env::var_os("BENILLA_F2_CATALOG") else {
        println!("SKIP Seal16 player creation: set BENILLA_F2_CATALOG");
        return;
    };
    let mut catalog = VerifiedCatalog::open(Path::new(&root), false).unwrap();
    let rows = PlayerCreateRows::load(&mut catalog).unwrap();
    let models = LoadedCreatureModels::catalog_diagnostic(&mut catalog).unwrap();
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let cells = SourceCells::new(MAP).unwrap();
    let mut player = construct(&mut objects, &mut units);
    let error = player
        .create(
            &mut objects,
            &mut units,
            request(),
            &rows,
            &models,
            &cells,
            CONFIG,
            || Ok(107),
        )
        .unwrap_err();
    assert_eq!(
        error.id,
        "execution.player.Player_850.learn_default_skills_owner"
    );
    // Seal16 client-packets.jsonl:5,9,16-17. The pose words are from opcode 566.
    let object = objects.get(player.object()).unwrap();
    let unit = units.get(&objects, player.object()).unwrap();
    assert_eq!(object.guid.unwrap().raw(), 1);
    assert_eq!(
        (player.race, unit.unit_class, player.gender),
        (1, Some(8), 0)
    );
    assert_eq!(object.map.unwrap().map, 0);
    assert_eq!(object.pose, Some(POSE));
    assert_eq!(rows.start(1, 8).unwrap().zone, 12);
    assert_eq!(player.display, 49);
    assert_eq!(unit.faction, 1);
    assert_eq!(unit.power_type, Some(0));
    assert_eq!(player.create_stats, [20, 20, 20, 23, 22]);
    assert_eq!(unit.creature_damage.attack_power_words(), [10, 10]);
    assert!(rows.spells(1, 8).contains(&133));
    assert!(rows
        .actions(1, 8)
        .iter()
        .any(|row| row.button == 1 && row.action == 133));
    assert!(rows.items(1, 8).is_empty());
}

#[test]
fn attack_power_uses_the_hunter_formulas_and_refuses_undefined_mage_cast() {
    let mut objects = SourceObjects::default();
    let handle = objects.construct(1, SourceObjectKind::Player).unwrap();
    let mut damage = SourceCreatureDamage::default();
    damage.set_strength(&objects, handle, 20).unwrap();
    let input = PlayerAttackPowerInputs {
        class: 3,
        level: 1,
        agility: 23,
        form: 0,
        form_agility_bonus: None,
        attack_times: [2000.0; 3],
        unarmed: true,
    };
    damage
        .update_player_attack_power_and_damage(&objects, handle, input, false)
        .unwrap();
    damage
        .update_player_attack_power_and_damage(&objects, handle, input, true)
        .unwrap();
    assert_eq!(damage.attack_power_words(), [25, 38]);

    damage.set_strength(&objects, handle, 1).unwrap();
    let error = damage
        .update_player_attack_power_and_damage(
            &objects,
            handle,
            PlayerAttackPowerInputs { class: 8, ..input },
            false,
        )
        .unwrap_err();
    assert_eq!(
        error.id,
        "execution.stats.StatSystem_273.attack_power_float_to_uint32_undefined"
    );
    assert_eq!(damage.attack_power_words(), [25, 38]);
}
