// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::native::production_compiler::VerifiedCatalog;
use benilla_sim::source_rng::{DrawCursor, Mt19937, SourceRandom};

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

pub(super) fn construct(objects: &mut SourceObjects, units: &mut SourceUnits) -> SourcePlayer {
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

pub(super) fn test_items(ids: impl IntoIterator<Item = u32>) -> SourceStartupItems {
    let mut items = SourceStartupItems::default();
    let mut mt = Mt19937::from_seed(1);
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    let mut random = SourceRandom::new(1, &mut mt, &mut cursor, &mut sink).unwrap();
    items
        .validate(
            ids.into_iter()
                .map(|id| (id, StartupItemPrototype::default()))
                .collect(),
            &Default::default(),
            &mut random,
        )
        .unwrap();
    assert_eq!(random.cursor().raw, 0);
    items
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
            &SourceSpellStores::default(),
            &SourceStartupItems::default(),
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
                &SourceSpellStores::default(),
                &SourceStartupItems::default(),
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
            &SourceSpellStores::default(),
            &SourceStartupItems::default(),
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
    let spells = SourceSpellStores::load(&mut catalog, &mut SourceSpellScriptRegistry).unwrap();
    let mut mt = Mt19937::from_seed(1);
    let mut cursor = DrawCursor::default();
    let items = SourceStartupItems::catalog_prefix(
        &mut catalog,
        &mut SourceRandom::new(1, &mut mt, &mut cursor, &mut |_| {}).unwrap(),
    )
    .unwrap();
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
            &spells,
            &items,
            &models,
            &cells,
            CONFIG,
            || Ok(107),
        )
        .unwrap_err();
    assert_eq!(error.id, "execution.player.Player_3400.passive_cast_owner");
    assert_eq!(
        player.spellbook(),
        &BTreeMap::from([(
            204,
            SourcePlayerSpell {
                state: SourcePlayerSpellState::New,
                active: true,
                dependent: true,
                disabled: false,
            }
        )])
    );
    assert!(player.action_buttons().is_empty());
    // Northshire-01 A proxy-requests.jsonl:534-535,1647-1648.
    assert_eq!(
        rows.skills(1, 8)
            .iter()
            .map(|row| row.id)
            .collect::<Vec<_>>(),
        [95, 162, 183, 415, 6, 8, 136, 228, 237, 754, 98]
    );
    assert_eq!(
        player.skills(),
        &[SourcePlayerSkill {
            id_step: [95, 0],
            value_max: [1, 5]
        }]
    );
    let object = objects.get(player.object()).unwrap();
    assert!(object.field_changed(0x2ce).unwrap());
    assert!(object.field_changed(0x2cf).unwrap());
    assert!(!object.field_changed(0x2d0).unwrap());
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
    // Northshire-01 A client-packets.jsonl:26 and trace:68539472.
    // Compare the SQL grant order to the packet's sorted spell IDs. The live
    // prefix above stops at the first passive and has not applied the grants.
    assert_eq!(
        rows.spells(1, 8),
        [
            81, 133, 168, 203, 204, 227, 522, 668, 2382, 2479, 3050, 3365, 5009, 5019, 6233, 6246,
            6247, 6477, 6478, 6603, 7266, 7267, 7355, 8386, 9078, 9125, 20597, 20598, 20599, 20600,
            20864, 21651, 21652, 22027, 22810
        ]
    );
    // Northshire-01 A client-packets.jsonl:27 and trace:68539567.
    assert_eq!(
        rows.actions(1, 8)
            .iter()
            .map(|row| (row.button, row.action | (u32::from(row.kind) << 24)))
            .collect::<Vec<_>>(),
        [
            (0, 6603),
            (1, 133),
            (2, 168),
            (10, 0x8000_009f),
            (11, 0x8000_0816)
        ]
    );
    assert!(rows.items(1, 8).is_empty());

    // A second live player tests rank/talent branches outside the creation route.
    let mut rank_objects = SourceObjects::default();
    let mut rank_units = SourceUnits::default();
    let mut rank_player = construct(&mut rank_objects, &mut rank_units);
    rank_player.race = 1;
    let live = PlayerLearning {
        objects: &rank_objects,
        unit: rank_units.get(&rank_objects, rank_player.object()).unwrap(),
        skills: rows.skill_inputs.as_ref().unwrap(),
        spells: &spells,
        items: &items,
        class: 8,
    };
    assert_eq!(
        rank_player
            .add_spell(&live, 143, true, true, true, false)
            .unwrap_err()
            .id,
        "execution.player.Player_3284.talent_tab_owner"
    );
    assert_eq!(
        rank_player
            .add_spell(&live, 11069, true, true, true, false)
            .unwrap_err()
            .id,
        "execution.player.Player_3259.talent_rank_removal_owner"
    );
    assert!(rank_player.spellbook().is_empty());
    let disabled = SourcePlayerSpell {
        state: SourcePlayerSpellState::New,
        active: false,
        dependent: false,
        disabled: true,
    };
    rank_player.spells.insert(11069, disabled);
    assert_eq!(
        rank_player
            .learn_spell(&live, 11069, false, false)
            .unwrap_err()
            .id,
        "execution.player.Player_3379.used_talent_points_owner"
    );
    assert!(!rank_player.spellbook()[&11069].disabled);
    // The disabled ranks belong to a Mage who already has Fire.
    // Northshire-01 A client-packets.jsonl:31, fields 721-722.
    rank_player.skills.push(SourcePlayerSkill {
        id_step: [8, 0],
        value_max: [5, 5],
    });
    for id in [133, 143] {
        rank_player.spells.insert(id, disabled);
    }
    rank_player.learn_spell(&live, 133, false, false).unwrap();
    for id in [133, 143] {
        assert_eq!(
            rank_player.spellbook()[&id],
            SourcePlayerSpell {
                disabled: false,
                ..disabled
            }
        );
    }
}

#[test]
fn detached_create_orders_shuffled_grants_and_sets_buttons_before_outfit() {
    let mut rows = inputs();
    rows.skill_inputs = Some(Default::default());
    rows.test_grants(
        vec![9, 5, 6],
        vec![
            PlayerCreateAction {
                button: 11,
                action: 19,
                kind: 128,
            },
            PlayerCreateAction {
                button: 4,
                action: 5,
                kind: 0,
            },
            PlayerCreateAction {
                button: 2,
                action: 9,
                kind: 0,
            },
        ],
    );
    assert_eq!(rows.spells(1, 8), [5, 6, 9]);
    assert_eq!(
        rows.actions(1, 8)
            .iter()
            .map(|r| r.button)
            .collect::<Vec<_>>(),
        [2, 4, 11]
    );
    let mut spells = SourceSpellStores::default();
    for &id in &[5, 6, 9] {
        spells
            .insert_template(SpellTemplate {
                id,
                ..Default::default()
            })
            .unwrap();
    }
    let items = test_items([19]);
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
            &spells,
            &items,
            &models,
            &cells,
            CONFIG,
            || Ok(107),
        )
        .unwrap_err();
    assert_eq!(error.id, "execution.player.Player_861.outfit_owner");
    assert_eq!(
        player.spellbook().keys().copied().collect::<Vec<_>>(),
        [5, 6, 9]
    );
    assert_eq!(
        player
            .action_buttons()
            .iter()
            .map(|(&button, row)| (button, row.packed_data))
            .collect::<Vec<_>>(),
        [(2, 9), (4, 5), (11, 0x8000_0013)]
    );
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
