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

#[test]
fn avoidance_capabilities_keep_unit_and_player_constructor_defaults() {
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let creature = SourceUnitFields::constructed_creature();
    assert!(creature.can_dodge && creature.can_parry && creature.can_block);

    let player = construct(&mut objects, &mut units);
    let object = objects.get(player.object()).unwrap();
    let unit = units.get_mut(&objects, player.object()).unwrap();
    assert!(unit.can_dodge && !unit.can_parry && !unit.can_block);
    unit.set_can_dodge(object, true).unwrap();
    unit.set_can_parry(object, false).unwrap();
    unit.set_can_block(object, false).unwrap();
    assert_eq!(
        unit.set_can_dodge(object, false).unwrap_err().id,
        "execution.unit.Unit_3026.update_dodge_percentage_owner"
    );
    assert_eq!(
        unit.set_can_parry(object, true).unwrap_err().id,
        "execution.unit.Unit_3036.update_parry_percentage_owner"
    );
    assert_eq!(
        unit.set_can_block(object, true).unwrap_err().id,
        "execution.unit.Unit_3046.update_block_percentage_owner"
    );
    assert!(!unit.can_dodge && unit.can_parry && unit.can_block);
}

#[test]
fn skill_bonus_writes_only_the_learned_slot_and_selected_half() {
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let mut player = construct(&mut objects, &mut units);
    let handle = player.object();
    let unit = units.get_mut(&objects, handle).unwrap();
    let object = objects.get(handle).unwrap();
    player
        .modify_skill_bonus(&objects, unit, 43, 5, true)
        .unwrap();
    assert!(player.skills().is_empty());
    assert!(!object.field_changed(0x2d0).unwrap());
    player.skills.push(SourcePlayerSkill {
        id_step: [43, 0],
        value_max: [1, 5],
        bonus: [-3, 7],
    });
    player
        .modify_skill_bonus(&objects, unit, 43, 5, true)
        .unwrap();
    assert_eq!(player.skills()[0].bonus, [-3, 12]);
    assert!(object.field_changed(0x2d0).unwrap());
    player
        .modify_skill_bonus(&objects, unit, 43, -1, false)
        .unwrap();
    assert_eq!(player.skills()[0].bonus, [-4, 12]);
    assert!(!object.field_changed(0x2cf).unwrap());
}

#[test]
fn percent_stat_buff_marks_both_live_words_and_can_be_removed() {
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let mut player = construct(&mut objects, &mut units);
    let object = objects.get(player.object()).unwrap();
    for amount in [25.0, -100.0] {
        player.positive_stats[4] = 16.0;
        player.negative_stats[4] = -8.0;
        player
            .apply_stat_percent_buff_mod(&objects, 4, amount, true)
            .unwrap();
        if amount == 25.0 {
            assert_eq!(
                (player.positive_stats[4], player.negative_stats[4]),
                (20.0, -10.0)
            );
        } else {
            assert!(player.positive_stats[4] > 0.0 && player.positive_stats[4] < 0.017);
            assert!(player.negative_stats[4] < 0.0 && player.negative_stats[4] > -0.009);
        }
        player
            .apply_stat_percent_buff_mod(&objects, 4, amount, false)
            .unwrap();
        assert_eq!(
            (player.positive_stats[4], player.negative_stats[4]),
            (16.0, -8.0)
        );
    }
    assert!(object.field_changed(0x49d).unwrap());
    assert!(object.field_changed(0x4a2).unwrap());
    assert!(!object.field_changed(0x49c).unwrap());
    assert!(!object.field_changed(0x4a1).unwrap());
    assert_eq!(
        player
            .apply_stat_percent_buff_mod(&objects, 5, 25.0, true)
            .unwrap_err()
            .id,
        "execution.player.Player_1824.stat_buff_index"
    );
}

#[test]
fn defense_refresh_keeps_real_dodge_and_marks_only_changed_ui() {
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let mut player = construct(&mut objects, &mut units);
    let handle = player.object();
    let unit = units.get_mut(&objects, handle).unwrap();
    unit.unit_class = Some(8);
    unit.power_fields = Some(SourceUnitPowerFields {
        level: 1,
        create_health: 0,
        create_mana: 0,
        auxiliary_current: None,
        auxiliary_maximum: None,
    });
    unit.resistances.set_agility(&objects, handle, 20).unwrap();
    player.skills.push(SourcePlayerSkill {
        id_step: [95, 0],
        value_max: [1, 5],
        bonus: [0; 2],
    });
    player.update_defense_bonuses(&objects, unit).unwrap();
    assert_eq!(unit.mod_dodge_chance.to_bits(), 0x4099_9cc6);
    assert_eq!(player.dodge_percentage.to_bits(), 0x4094_7e0e);
    let object = objects.get(handle).unwrap();
    assert!(!object.field_changed(0x452).unwrap());
    assert!(object.field_changed(0x453).unwrap());
    assert!(!object.field_changed(0x454).unwrap());

    objects.remove_from_world(handle).unwrap();
    player.update_defense_bonuses(&objects, unit).unwrap();
    assert!(!objects.get(handle).unwrap().field_changed(0x453).unwrap());

    // Both signed bonus halves participate. Effective Defense is now 5.
    player.skills[0].bonus = [-3, 7];
    player.update_defense_bonuses(&objects, unit).unwrap();
    assert_eq!(player.dodge_percentage.to_bits(), 0x4099_9cc6);
    assert!(objects.get(handle).unwrap().field_changed(0x453).unwrap());

    // The upper clamp does not cap the live Unit modifier.
    unit.resistances
        .set_agility(&objects, handle, 1_290)
        .unwrap();
    player.update_defense_bonuses(&objects, unit).unwrap();
    assert_eq!(unit.mod_dodge_chance, 103.25);
    assert_eq!(player.dodge_percentage, 100.0);

    player.skills[0].bonus = [-10, 0];
    assert_eq!(player.skill_value(95), 0);
    unit.power_fields.as_mut().unwrap().level = 60;
    unit.resistances.set_agility(&objects, handle, 0).unwrap();
    player.update_defense_bonuses(&objects, unit).unwrap();
    assert_eq!(unit.mod_dodge_chance, 3.25);
    assert_eq!(player.dodge_percentage, 0.0);
}

#[test]
fn defense_refresh_refuses_enabled_block_and_parry_in_source_order() {
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let mut player = construct(&mut objects, &mut units);
    let unit = units.get_mut(&objects, player.object()).unwrap();
    unit.can_parry = true;
    unit.can_block = true;
    assert_eq!(
        player
            .update_defense_bonuses(&objects, unit)
            .unwrap_err()
            .id,
        "execution.player.StatSystem_401.block_enabled_owner"
    );
    unit.can_block = false;
    unit.mod_block_chance = 7.0;
    assert_eq!(
        player
            .update_defense_bonuses(&objects, unit)
            .unwrap_err()
            .id,
        "execution.player.StatSystem_458.parry_enabled_owner"
    );
    assert_eq!(unit.mod_block_chance, 0.0);
}

#[test]
fn defense_refresh_names_each_unported_dodge_class() {
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let mut player = construct(&mut objects, &mut units);
    let unit = units.get_mut(&objects, player.object()).unwrap();
    for (class, line) in [
        (2, 4947),
        (7, 4947),
        (11, 4947),
        (4, 4955),
        (3, 4959),
        (5, 4963),
        (9, 4967),
        (1, 4971),
        (0, 4975),
        (12, 4975),
    ] {
        unit.unit_class = Some(class);
        assert_eq!(
            player
                .update_defense_bonuses(&objects, unit)
                .unwrap_err()
                .id,
            format!("execution.player.Player_{line}.dodge_agility_class_owner")
        );
    }
    unit.unit_class = Some(8);
    assert_eq!(
        player
            .update_defense_bonuses(&objects, unit)
            .unwrap_err()
            .id,
        "execution.player.Player_4977.dodge_level_owner"
    );
}

#[test]
fn defense_refresh_refuses_a_live_dodge_aura_before_writing_dodge() {
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let mut player = construct(&mut objects, &mut units);
    let handle = player.object();
    let unit = units.get_mut(&objects, handle).unwrap();
    unit.unit_class = Some(8);
    unit.power_fields = Some(SourceUnitPowerFields {
        level: 1,
        create_health: 0,
        create_mana: 0,
        auxiliary_current: None,
        auxiliary_maximum: None,
    });
    let template = SpellTemplate {
        id: 1,
        ..Default::default()
    };
    let mut holder = SourceAuraHolder::new(&template, handle, 0, 0, -1).unwrap();
    holder.effects[0] = Some(SourceAura::self_spell_effect(&template, 0, 49, 0, 0, 1, 0).unwrap());
    unit.auras.store_aura_holder(1, &mut Some(holder)).unwrap();
    unit.mod_dodge_chance = 7.0;
    assert_eq!(
        player
            .update_defense_bonuses(&objects, unit)
            .unwrap_err()
            .id,
        "execution.player.StatSystem_497.dodge_aura_modifier_owner"
    );
    assert_eq!(unit.mod_dodge_chance, 7.0);
    assert!(!objects.get(handle).unwrap().field_changed(0x453).unwrap());
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
            None,
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
                None,
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
            None,
            || panic!("clock after validation"),
        )
        .unwrap_err();
    assert_eq!(error.id, "execution.player.Player_755.playercreateinfo");
    assert_eq!(objects.get(player.object()).unwrap().pose, None);
}

fn northshire_creation_trace(path: &Path) -> (Mt19937, [(SourcePlayerAuraClock, i64); 10]) {
    use std::fs::File;
    use std::io::{BufRead, BufReader, Seek, SeekFrom};

    let mut trace = BufReader::new(File::open(path).unwrap());
    // Northshire-01 A trace:68133280-68133281. Seek to the captured seed and state.
    trace.seek(SeekFrom::Start(4_371_307_944)).unwrap();
    let mut line = String::new();
    trace.read_line(&mut line).unwrap();
    assert_eq!(
        line.trim_end(),
        "12\t13807\tmt_seed\t0x201800037ec0 1795246040 236208755"
    );
    line.clear();
    trace.read_line(&mut line).unwrap();
    let mut data = line
        .strip_prefix("12\t13808\tmt_state\t0x201800037ec0 ")
        .unwrap()
        .split_ascii_whitespace();
    let rng = Mt19937::from_seed_seq(&[1_795_246_040, 236_208_755]);
    let state = rng.state();
    for &word in &state.words {
        assert_eq!(Some(word), data.next().map(|v| v.parse().unwrap()));
    }
    assert_eq!(Some(state.index), data.next().map(|v| v.parse().unwrap()));
    assert_eq!(data.next(), None);
    (rng, read_northshire_aura_clocks(&mut trace))
}

fn read_northshire_aura_clocks(
    trace: &mut impl std::io::BufRead,
) -> [(SourcePlayerAuraClock, i64); 10] {
    use SourcePlayerAuraClock::{Effect, Holder};
    let mut at = 68_133_281;
    let mut line = String::new();
    [
        (68_133_905, Holder),
        (68_133_907, Effect),
        (68_133_982, Holder),
        (68_133_990, Effect),
        (68_133_992, Effect),
        (68_134_067, Holder),
        (68_134_072, Effect),
        (68_134_169, Holder),
        (68_134_189, Effect),
        (68_134_197, Effect),
    ]
    .map(|(callsite, kind)| {
        while at < callsite {
            line.clear();
            assert_ne!(trace.read_line(&mut line).unwrap(), 0);
            at += 1;
        }
        let mut row = line.trim_end().split('\t');
        assert_eq!(row.next(), Some("12"));
        let sequence = row.next().unwrap().parse::<u64>().unwrap();
        assert_eq!(row.next(), Some("clock_callsite"));
        let mut payload = row.next().unwrap().split_ascii_whitespace();
        assert_eq!(
            payload.nth(2),
            Some(match kind {
                Holder => "8250845",
                Effect => "8249833",
            })
        );
        line.clear();
        assert_ne!(trace.read_line(&mut line).unwrap(), 0);
        at += 1;
        let mut row = line.trim_end().split('\t');
        assert_eq!(row.next(), Some("12"));
        assert_eq!(row.next().unwrap().parse::<u64>().unwrap(), sequence + 1);
        assert_eq!(row.next(), Some("clock_time"));
        let seconds = row
            .next()
            .unwrap()
            .split_ascii_whitespace()
            .next()
            .unwrap()
            .parse::<i64>()
            .unwrap();
        assert_eq!(seconds, 1_795_246_040);
        (kind, seconds)
    })
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
    let f1 = std::env::var_os("BENILLA_F6_MMAP_F1").unwrap();
    let mut files = SourceTerrainFiles::default();
    files
        .retain_present(
            0,
            48,
            32,
            &Path::new(&f1).join("products/maps/0004832.map"),
            &Sha256Id::parse("9924cc69ac739017c39dcd5294286c6abbe9817d9e1808efa6a5bd07ae5d3f63")
                .unwrap(),
        )
        .unwrap();
    let mut terrain =
        SourceTerrainManager::new(files, SourceTerrainDbc::catalog(&mut catalog).unwrap());
    terrain
        .load_terrain(
            0,
            &mut SourceRandom::new(1, &mut mt, &mut cursor, &mut |_| {}).unwrap(),
        )
        .unwrap();
    let targets = SourceSpellTargetMgr::initialize(&spells);
    let mut session = SourceWorldSession::construct(true);
    // Northshire-01 A client-packets.jsonl:4-7. WorldSession.cpp:479-499 runs before create.
    session.set_in_char_selection();
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let cells = SourceCells::new(MAP).unwrap();
    let trace = std::env::var_os("BENILLA_NORTHSHIRE_A_TRACE").unwrap();
    let (mut engine, aura_clocks) = northshire_creation_trace(Path::new(&trace));
    let mut draws = DrawCursor::default();
    let mut raw = Vec::new();
    let mut observe = |event: benilla_sim::source_rng::TraceEvent| {
        assert_eq!(event.stream, 12);
        if let benilla_sim::source_rng::EventKind::Raw { position, word, .. } = event.kind {
            raw.push((position, word));
        }
    };
    let mut rng = SourceRandom::new(12, &mut engine, &mut draws, &mut observe).unwrap();
    let mut player = SourcePlayer::construct(
        &mut objects,
        &mut units,
        12,
        CONFIG.save_interval,
        |min, max| {
            assert_eq!((min, max), (450_000, 1_350_000)); // Trace :68133268.
            let delay = rng.urand(min, max).unwrap();
            assert_eq!(delay, 477_818); // Trace :68133283.
            Ok(delay)
        },
        || Ok(101),
    )
    .unwrap();
    let mut irand = |min, max| {
        assert_eq!((min, max), (0, 6)); // Trace :68133495.
        let result = rng.irand(min, max).unwrap();
        assert_eq!(result, 0); // Trace :68133497.
        Ok(result)
    };
    let mut clock_reads = 0;
    let mut aura_seconds = |kind| {
        let (expected, seconds) = aura_clocks[clock_reads];
        assert_eq!(kind, expected);
        clock_reads += 1;
        Ok(seconds)
    };
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
            Some(SourcePlayerCastInputs {
                targets: &targets,
                terrain: &mut terrain,
                map_time: &|map| {
                    assert_eq!(map, MAP);
                    Ok(100)
                },
                aura_seconds: &mut aura_seconds,
                irand: &mut irand,
                session: &session,
                max_casts: 20,
                indoor_policy: true,
            }),
            || Ok(107),
        )
        .unwrap_err();
    assert_eq!(error.id, "execution.player.Player_861.outfit_owner");
    // Northshire-01 A trace :68133905-68133908,68133982-68133993,
    // :68134067-68134073,68134169-68134198.
    assert_eq!(clock_reads, 10);
    // Trace :68133282,68133496. Later creation casts add no draw.
    assert_eq!(raw, [(0, 132_753_960), (1, 40_231_698)]);
    assert_eq!(draws.raw, 2);
    let unit = units.get(&objects, player.object()).unwrap();
    let queue = unit.unit_events.inventory();
    // SkillLineAbility keys 490,689,1424,1236,700,2927,11728,11730,11731,11861,590.
    // Northshire-01 A trace :68133335,68133417,68133506,68133583,68133667,
    // :68133743,68133825,68133911,68133996,68134076,68134201,68134280,
    // :68134359,68134441,68134518. Perception 20600 does not cast.
    let casts = [
        204, 81, 522, 203, 227, 5009, 20599, 20597, 20598, 20864, 668, 2382, 3050, 9078, 9125,
    ];
    assert_eq!(queue.queued.len(), casts.len());
    assert_eq!(queue.clock_ms, 0);
    for (index, (event, id)) in queue.queued.iter().zip(casts).enumerate() {
        let insertion = index as u64 + 1;
        let SourceUnitEvent::Spell { spell } = event.kind else {
            panic!("creation training must schedule a spell event");
        };
        assert_eq!((event.add_time_ms, event.exec_time_ms), (0, 1));
        let schedule = event.queue.as_ref().unwrap();
        assert_eq!(
            (schedule.deadline_ms, schedule.insertion_id),
            (1, insertion)
        );
        let cast = &unit.spell_starts.spells[&spell];
        assert_eq!((cast.spell, cast.event), (id, Some(event.event_id)));
        if id == 20599 {
            // Stock SQL :853806 and SpellDuration.dbc key 21 give a permanent self aura.
            assert_eq!(cast.duration, -1);
            assert_eq!(cast.damage_multipliers, [1f32.to_bits(), 0, 0]);
            let [target] = cast.unique_targets.as_slice() else {
                panic!("Diplomacy must select one self target");
            };
            assert_eq!(target.unit, player.object());
            assert_eq!(
                (target.effect_mask, target.hit_mask, target.duration),
                (1, 1, -1)
            );
            assert!(!target.missed && target.processed && cast.pending_aura.is_none());
        }
        assert_eq!(cast.state, SourceSpellState::Finished);
        assert!(!cast.executing);
        if insertion <= 3 {
            assert_eq!(cast.creation_time, 100);
        }
        // Stock SQL :853804-853806,854031 use fixed effect values.
        assert_eq!(
            cast.damage_per_effect,
            match id {
                20599 => [10, 0, 0],
                20597 => [5, 5, 0],
                20598 => [5, 0, 0],
                20864 => [5, 5, 0],
                _ => [0; 3],
            }
        );
    }
    // Spell.cpp:3356,3735,8109. All fifteen casts have returned.
    assert_eq!(unit.auras.cast_counter, 0);
    assert_eq!(player.active_mod_spell, None);
    assert_eq!(unit.auras.aura_holders().count(), 4);
    let diplomacy = unit.auras.first_aura_holder(20599).unwrap();
    assert!(diplomacy.ready && diplomacy.passive && diplomacy.permanent);
    assert!(diplomacy.skip_update);
    assert_eq!((diplomacy.duration, diplomacy.slot), (-1, 255));
    assert_eq!(diplomacy.apply_time, 1_795_246_040);
    assert_eq!(diplomacy.apply_ms, 100); // The authored map-time input above.
    let aura = diplomacy.effects[0].as_ref().unwrap();
    assert_eq!(
        (aura.kind, aura.amount, aura.apply_time),
        (156, 10, 1_795_246_040)
    );
    assert!(aura.applied);
    let sword = unit.auras.first_aura_holder(20597).unwrap();
    assert!(sword.ready && sword.effects[2].is_none());
    assert_eq!(sword.apply_time, 1_795_246_040);
    for (aura, skill) in sword.effects[..2].iter().zip([43, 55]) {
        let aura = aura.as_ref().unwrap();
        assert_eq!(
            (aura.kind, aura.amount, aura.apply_time),
            (98, 5, 1_795_246_040)
        );
        assert_eq!(aura.misc, skill);
        assert!(aura.applied);
        assert!(!player
            .skills()
            .iter()
            .any(|row| row.id_step[0] == skill as u16));
    }
    let spirit = unit.auras.first_aura_holder(20598).unwrap();
    assert!(spirit.ready && spirit.passive && spirit.permanent);
    assert_eq!(spirit.apply_time, 1_795_246_040);
    let aura = spirit.effects[0].as_ref().unwrap();
    assert_eq!((aura.kind, aura.misc, aura.amount), (137, 4, 5));
    assert!(aura.applied);
    // Unit.cpp:363,8996-9017. Creation writes the multiplier without UpdateStats.
    assert_eq!(
        unit.stat_modifiers
            .map(|group| group.total_percent.to_bits()),
        [
            0x3f80_0000,
            0x3f80_0000,
            0x3f80_0000,
            0x3f80_0000,
            0x3f86_6666
        ]
    );
    assert_eq!(player.mental_and_stamina[2], 22);
    assert_eq!(player.positive_stats, [0.0; 5]);
    assert_eq!(player.negative_stats, [0.0; 5]);
    let mace = unit.auras.first_aura_holder(20864).unwrap();
    assert!(mace.ready && mace.passive && mace.permanent);
    assert_eq!(mace.apply_time, 1_795_246_040);
    assert!(mace.effects[2].is_none());
    // Stock SQL :854031; trace :68134169-68134198.
    for (aura, skill) in mace.effects[..2].iter().zip([54, 160]) {
        let aura = aura.as_ref().unwrap();
        assert_eq!(
            (aura.kind, aura.misc, aura.amount, aura.apply_time),
            (98, skill, 5, 1_795_246_040)
        );
        assert!(aura.applied);
        assert!(!player
            .skills()
            .iter()
            .any(|row| row.id_step[0] == skill as u16));
    }
    // Northshire-01 A client-packets.jsonl:16-17; SQL :839555,845058,845096.
    assert_eq!(player.weapon_proficiency, 0x0008_4400);
    assert_eq!(player.armor_proficiency, 0x3);
    // Stock SQL :671412; SkillLine.dbc key 95; StatSystem.cpp:484,493-502.
    assert_eq!(unit.mod_dodge_chance.to_bits(), 0x4099_9cc6);
    assert_eq!(player.dodge_percentage.to_bits(), 0x4094_7e0e);
    assert_eq!((unit.mod_block_chance, unit.mod_parry_chance), (0.0, 0.0));
    assert_eq!(
        (player.block_percentage, player.parry_percentage),
        (0.0, 0.0)
    );
    assert!(terrain.terrain(0).unwrap().grid(48, 32).is_some());
    let learned = SourcePlayerSpell {
        state: SourcePlayerSpellState::New,
        active: true,
        dependent: true,
        disabled: false,
    };
    assert!(player.spellbook().values().all(|&spell| spell == learned));
    // Keys 11728-11730 grant Perception 20600 before Sword without a cast.
    for id in [
        81, 133, 168, 203, 204, 227, 522, 5009, 5019, 20597, 20598, 20599, 20600,
    ] {
        assert_eq!(player.spellbook().get(&id), Some(&learned));
    }
    // Northshire-01 A proxy-requests.jsonl:534-535,1647-1648.
    assert_eq!(
        rows.skills(1, 8)
            .iter()
            .map(|row| row.id)
            .collect::<Vec<_>>(),
        [95, 162, 183, 415, 6, 8, 136, 228, 237, 754, 98]
    );
    // SkillLine.dbc keys 95,162,6,8,136,228,237,754. SkillRaceClassInfo.dbc
    // keys 57,301,55,862 maximize the mono lines at Player.cpp:5865.
    assert_eq!(
        player.skills(),
        [
            ([95, 0], [1, 5]),
            ([162, 0], [1, 5]),
            ([6, 1], [5, 5]),
            ([8, 1], [5, 5]),
            ([136, 0], [1, 5]),
            ([228, 0], [1, 5]),
            ([237, 1], [5, 5]),
            ([754, 1], [5, 5]),
            ([98, 0], [300, 300]), // SkillLine.dbc key 98 and Player.cpp:5833-5835.
            // Ability key 4999, SkillRaceClassInfo key 148 and Player.cpp:5776.
            ([415, 0], [1, 1]),
        ]
        .map(|(id_step, value_max)| SourcePlayerSkill {
            id_step,
            value_max,
            bonus: [0; 2],
        })
    );
    let object = objects.get(player.object()).unwrap();
    assert!(object.field_changed(0x2ce).unwrap());
    assert!(object.field_changed(0x2cf).unwrap());
    assert!(!object.field_changed(0x2d0).unwrap());
    assert!(object.field_changed(0x2e6).unwrap());
    assert!(object.field_changed(0x2e7).unwrap());
    assert!(!object.field_changed(0x2e8).unwrap());
    assert!(object.field_changed(0x2e9).unwrap());
    assert!(object.field_changed(0x2ea).unwrap());
    assert!(!object.field_changed(0x2eb).unwrap());
    assert!(!object.field_changed(PLAYER_FIELD_POSSTAT0 + 4).unwrap());
    assert!(!object.field_changed(PLAYER_FIELD_NEGSTAT0 + 4).unwrap());
    assert!(!object.field_changed(0x452).unwrap());
    assert!(object.field_changed(0x453).unwrap());
    assert!(!object.field_changed(0x454).unwrap());
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
    // Northshire-01 A client-packets.jsonl:26 and trace :68539472.
    let packet_spells = [
        81, 133, 168, 203, 204, 227, 522, 668, 2382, 2479, 3050, 3365, 5009, 5019, 6233, 6246,
        6247, 6477, 6478, 6603, 7266, 7267, 7355, 8386, 9078, 9125, 20597, 20598, 20599, 20600,
        20864, 21651, 21652, 22027, 22810,
    ];
    assert_eq!(rows.spells(1, 8), packet_spells);
    // Northshire-01 A client-packets.jsonl:27 and trace:68539567.
    let packet_actions = [
        (0, 6603),
        (1, 133),
        (2, 168),
        (10, 0x8000_009f),
        (11, 0x8000_0816),
    ];
    // Player.cpp:2965-2979,5905-5912 selects the live rows sent on login.
    let live_spells = player
        .spellbook()
        .iter()
        .filter_map(|(&id, spell)| {
            (spell.active && !spell.disabled && spell.state != SourcePlayerSpellState::Removed)
                .then_some(id)
        })
        .collect::<Vec<_>>();
    let live_actions = player
        .action_buttons()
        .iter()
        .filter_map(|(&slot, button)| {
            (button.state != SourcePlayerActionState::Deleted && button.packed_data != 0)
                .then_some((slot, button.packed_data))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        (live_spells.as_slice(), live_actions.as_slice()),
        (packet_spells.as_slice(), packet_actions.as_slice())
    );
    assert_eq!(player.action_buttons().len(), packet_actions.len());
    assert_eq!(player.spellbook().len(), packet_spells.len());
    assert_eq!(
        rows.actions(1, 8)
            .iter()
            .map(|row| (row.button, row.action | (u32::from(row.kind) << 24)))
            .collect::<Vec<_>>(),
        packet_actions
    );
    assert!(rows.items(1, 8).is_empty());

    // A second live player tests rank/talent branches outside the creation route.
    let mut rank_objects = SourceObjects::default();
    let mut rank_units = SourceUnits::default();
    let mut rank_player = construct(&mut rank_objects, &mut rank_units);
    rank_player.race = 1;
    let mut live = PlayerLearning {
        objects: &rank_objects,
        units: &mut rank_units,
        cast: None,
        skills: rows.skill_inputs.as_ref().unwrap(),
        spells: &spells,
        items: &items,
        class: 8,
    };
    assert_eq!(
        rank_player
            .add_spell(&mut live, 143, true, true, true, false)
            .unwrap_err()
            .id,
        "execution.player.Player_3284.talent_tab_owner"
    );
    assert_eq!(
        rank_player
            .add_spell(&mut live, 11069, true, true, true, false)
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
            .learn_spell(&mut live, 11069, false, false)
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
        bonus: [0; 2],
    });
    for id in [133, 143] {
        rank_player.spells.insert(id, disabled);
    }
    rank_player
        .learn_spell(&mut live, 133, false, false)
        .unwrap();
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
            None,
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
