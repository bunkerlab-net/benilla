// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_world_data::*;
use serde_json::{json, Value};
use std::io::{self, Read};

const FIXTURE: &[u8] = include_bytes!("fixtures/original-camp.json");
fn fixture() -> Value {
    serde_json::from_slice(FIXTURE).unwrap()
}
fn load(value: &Value) -> Result<WorldData, Error> {
    load_json(&serde_json::to_vec(value).unwrap(), Limits::default())
}

#[test]
fn original_fixture_is_indexed() {
    let world = load_json(FIXTURE, Limits::default()).unwrap();
    assert_eq!(world.creature(900002).unwrap().max_health, 30);
    assert_eq!(world.spawn(2).unwrap().creature_entry, 900002);
    assert_eq!(world.quest(910001).unwrap().objectives[0].count, 2);
    assert_eq!(world.item(920001).unwrap().max_stack, 20);
    assert_eq!(world.ability(930001).unwrap().effect, DirectEffect::Damage);
    assert_eq!(world.ability(930002).unwrap().effect, DirectEffect::Heal);
    assert!(world.creature(1).is_none());
}
#[test]
fn row_order_and_whitespace_do_not_change_digest() {
    let first = load_json(FIXTURE, Limits::default()).unwrap();
    let mut value = fixture();
    for table in [
        "creatures",
        "spawns",
        "quests",
        "quest_relations",
        "items",
        "abilities",
    ] {
        value[table].as_array_mut().unwrap().reverse();
    }
    let second = load(&value).unwrap();
    assert_eq!(first.content_digest(), second.content_digest());
    let encoded = serde_json::to_vec(first.export()).unwrap();
    assert_eq!(
        load_json(&encoded, Limits::default())
            .unwrap()
            .content_digest(),
        first.content_digest()
    );
}
#[test]
fn gameplay_data_changes_digest() {
    let first = load_json(FIXTURE, Limits::default()).unwrap();
    let mut value = fixture();
    value["abilities"][0]["amount"] = json!(6);
    assert_ne!(
        load(&value).unwrap().content_digest(),
        first.content_digest()
    );
}
#[test]
fn dangling_references_are_rejected_at_each_boundary() {
    for (table, row, field, expected) in [
        ("spawns", 0, "creature_entry", "spawns.creature_entry"),
        (
            "quest_relations",
            0,
            "creature_entry",
            "quest_relations.creature_entry",
        ),
        (
            "quest_relations",
            0,
            "quest_entry",
            "quest_relations.quest_entry",
        ),
    ] {
        let mut value = fixture();
        value[table][row][field] = json!(123);
        assert!(
            matches!(load(&value), Err(Error::MissingReference { field, .. }) if field == expected)
        );
    }
    let mut value = fixture();
    value["quests"][0]["objectives"][0]["creature_entry"] = json!(123);
    assert!(matches!(
        load(&value),
        Err(Error::MissingReference {
            field: "quests.objectives.creature_entry",
            ..
        })
    ));
    let mut value = fixture();
    value["quests"][0]["reward_items"][0]["item_entry"] = json!(123);
    assert!(matches!(
        load(&value),
        Err(Error::MissingReference {
            field: "quests.reward_items.item_entry",
            ..
        })
    ));
}
#[test]
fn duplicate_identifiers_and_relations_are_rejected() {
    for table in [
        "creatures",
        "spawns",
        "quests",
        "quest_relations",
        "items",
        "abilities",
    ] {
        let mut value = fixture();
        let row = value[table][0].clone();
        value[table].as_array_mut().unwrap().push(row);
        assert!(matches!(load(&value), Err(Error::Duplicate { table: t, .. }) if t == table));
    }
}
#[test]
fn missing_quest_turnin_is_rejected() {
    let mut value = fixture();
    value["quest_relations"].as_array_mut().unwrap().pop();
    assert!(matches!(
        load(&value),
        Err(Error::Invalid {
            field: "quests.start_and_finish_relations",
            ..
        })
    ));
}
#[test]
fn unknown_fields_cannot_sneak_in_mechanics() {
    for table in [
        "creatures",
        "spawns",
        "quests",
        "quest_relations",
        "items",
        "abilities",
    ] {
        let mut value = fixture();
        value[table][0]["ScriptName"] = json!("arbitrary-script");
        assert!(matches!(load(&value), Err(Error::Json { .. })));
    }
    let mut value = fixture();
    value["source"]["download_url"] = json!("https://example.invalid/assets");
    assert!(matches!(load(&value), Err(Error::Json { .. })));
}
#[test]
fn nonempty_unsupported_features_fail_closed() {
    let mut value = fixture();
    value["unsupported_features"] = json!(["creature 900002 requires EventAI"]);
    assert_eq!(load(&value).unwrap_err(), Error::UnsupportedFeatures);
}
#[test]
fn unknown_versions_and_profiles_fail_closed() {
    let mut value = fixture();
    value["format_version"] = json!(2);
    assert!(matches!(
        load(&value),
        Err(Error::Invalid {
            field: "format_version",
            ..
        })
    ));
    let mut value = fixture();
    value["profile"] = json!("all-of-classic");
    assert!(matches!(
        load(&value),
        Err(Error::Invalid {
            field: "profile",
            ..
        })
    ));
}
#[test]
fn local_source_pins_are_metadata_validation_not_a_real_db_fixture() {
    let mut value = fixture();
    value["source"] = json!({"kind":"local_cmangos_classic", "core_revision":REVIEWED_CORE_REVISION,
        "database_revision":REVIEWED_DATABASE_REVISION,"client_build":CLASSIC_CLIENT_BUILD,
        "schema_sha256":"0".repeat(64)});
    assert!(load(&value).is_ok());
    for (field, invalid) in [
        ("core_revision", json!("master")),
        ("database_revision", json!("master")),
        ("client_build", json!(6005)),
        ("schema_sha256", json!("not-a-sha")),
    ] {
        let mut changed = value.clone();
        changed["source"][field] = invalid;
        assert!(matches!(load(&changed), Err(Error::Invalid { .. })));
    }
}
#[test]
fn bounds_and_float_coordinates_are_rejected() {
    for (table, field, value) in [
        ("creatures", "max_health", json!(0)),
        ("creatures", "melee_period_ms", json!(0)),
        ("spawns", "respawn_ms", json!(0)),
        ("spawns", "orientation_milliradians", json!(6284)),
        ("items", "max_stack", json!(0)),
        ("abilities", "amount", json!(0)),
        ("abilities", "max_range_milliunits", json!(100001)),
    ] {
        let mut data = fixture();
        data[table][0][field] = value;
        assert!(matches!(load(&data), Err(Error::Invalid { .. })));
    }
    for position in [
        json!([1.5, 0, 0]),
        json!([-2147483648_i64, 0, 0]),
        json!([100000001, 0, 0]),
    ] {
        let mut value = fixture();
        value["spawns"][0]["position_milliunits"] = position;
        assert!(load(&value).is_err());
    }
}
#[test]
fn unsupported_ability_effects_and_invalid_range_are_rejected() {
    let mut value = fixture();
    value["abilities"][0]["effect"] = json!("summon");
    assert!(matches!(load(&value), Err(Error::Json { .. })));
    let mut value = fixture();
    value["abilities"][0]["min_range_milliunits"] = json!(10001);
    assert!(matches!(
        load(&value),
        Err(Error::Invalid {
            field: "abilities.range",
            ..
        })
    ));
}
#[test]
fn zero_negative_and_overflow_ids_fail() {
    for bad in [json!(0), json!(-1), json!(4294967296_u64)] {
        let mut value = fixture();
        value["items"][0]["entry"] = bad;
        assert!(load(&value).is_err());
    }
}
#[test]
fn string_limits_and_control_characters_fail() {
    for name in [
        "".to_owned(),
        " ".to_owned(),
        "x".repeat(257),
        "name\0tail".to_owned(),
    ] {
        let mut value = fixture();
        value["creatures"][0]["name"] = json!(name);
        assert!(matches!(
            load(&value),
            Err(Error::Invalid {
                field: "creatures.name",
                ..
            })
        ));
    }
    assert!(load_json(
        FIXTURE,
        Limits {
            max_string_bytes: 4,
            ..Limits::default()
        }
    )
    .is_err());
}
#[test]
fn malformed_duplicate_json_and_sql_are_not_accepted() {
    for input in [
        b"DROP TABLE creature;".as_slice(),
        b"{\"format_version\":1,\"format_version\":1}",
        b"[]",
        b"null",
        b"{",
    ] {
        assert!(matches!(
            load_json(input, Limits::default()),
            Err(Error::Json { .. })
        ));
    }
    let mut trailing = FIXTURE.to_vec();
    trailing.extend_from_slice(b"{} ");
    assert!(matches!(
        load_json(&trailing, Limits::default()),
        Err(Error::Json { .. })
    ));
}
#[test]
fn configurable_limits_cannot_exceed_hard_caps() {
    assert_eq!(
        load_json(
            FIXTURE,
            Limits {
                max_bytes: HARD_MAX_BYTES + 1,
                ..Limits::default()
            }
        )
        .unwrap_err(),
        Error::InvalidLimits
    );
    assert_eq!(
        load_json(
            FIXTURE,
            Limits {
                max_bytes: 0,
                ..Limits::default()
            }
        )
        .unwrap_err(),
        Error::InvalidLimits
    );
    assert_eq!(
        load_json(
            FIXTURE,
            Limits {
                max_bytes: FIXTURE.len() - 1,
                ..Limits::default()
            }
        )
        .unwrap_err(),
        Error::TooLarge
    );
    assert_eq!(
        load_json(
            FIXTURE,
            Limits {
                max_records: 1,
                ..Limits::default()
            }
        )
        .unwrap_err(),
        Error::TooManyRecords
    );
}
#[test]
fn reader_consumes_no_more_than_limit_plus_one() {
    let mut reader = io::repeat(b'x');
    assert_eq!(
        load_reader(
            &mut reader,
            Limits {
                max_bytes: 10,
                ..Limits::default()
            }
        )
        .unwrap_err(),
        Error::TooLarge
    );
    let mut cursor = io::Cursor::new(vec![b'x'; 100]);
    assert_eq!(
        load_reader(
            &mut cursor,
            Limits {
                max_bytes: 10,
                ..Limits::default()
            }
        )
        .unwrap_err(),
        Error::TooLarge
    );
    assert_eq!(cursor.position(), 11);
    assert!(load_reader(io::Cursor::new(FIXTURE), Limits::default()).is_ok());
}
#[test]
fn io_errors_are_reported_without_partial_data() {
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("private path"))
        }
    }
    assert_eq!(
        load_reader(Broken, Limits::default()).unwrap_err(),
        Error::Io
    );
}
#[test]
fn oversized_objective_sets_and_duplicate_rewards_fail() {
    let mut value = fixture();
    value["quests"][0]["objectives"] = json!([]);
    assert!(load(&value).is_err());
    let mut value = fixture();
    let reward = value["quests"][0]["reward_items"][0].clone();
    value["quests"][0]["reward_items"]
        .as_array_mut()
        .unwrap()
        .push(reward);
    assert!(matches!(
        load(&value),
        Err(Error::Invalid {
            field: "quests.reward_items.duplicate",
            ..
        })
    ));
}

#[test]
fn capability_manifest_matches_parser_contract() {
    let manifest: Value = serde_json::from_str(include_str!("../CAPABILITIES.json")).unwrap();
    assert_eq!(manifest["format_version"], FORMAT_VERSION);
    assert_eq!(manifest["profile"], PROFILE);
    assert_eq!(manifest["client_build"], CLASSIC_CLIENT_BUILD);
    assert_eq!(manifest["reviewed_core_revision"], REVIEWED_CORE_REVISION);
    assert_eq!(
        manifest["reviewed_database_revision"],
        REVIEWED_DATABASE_REVISION
    );
    assert_eq!(manifest["runtime_integrated"], false);
    assert_eq!(manifest["sql_importer_available"], false);
    assert_eq!(manifest["safety"]["max_bytes"], HARD_MAX_BYTES);
    assert_eq!(manifest["safety"]["max_top_level_rows"], HARD_MAX_RECORDS);
    assert_eq!(
        manifest["safety"]["max_string_bytes"],
        HARD_MAX_STRING_BYTES
    );
}

#[test]
fn v1_fixture_digest_is_stable() {
    let data = load_json(FIXTURE, Limits::default()).unwrap();
    let actual: String = data
        .content_digest()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(
        actual,
        "9c61bc986dff10f48f00329fb033e053b3a9d7d91865d5064e2371d1a3016e56"
    );
}

#[test]
fn operational_looking_content_is_only_inert_text() {
    let mut value = fixture();
    value["quests"][0]["details"] =
        json!("SELECT * FROM example; https://example.invalid/script\n<lua>run()</lua>");
    let world = load(&value).unwrap();
    assert!(world.quest(910001).unwrap().details.starts_with("SELECT"));
}

#[test]
fn positional_record_arrays_normalize_to_the_same_object_hash() {
    let original = load_json(FIXTURE, Limits::default()).unwrap();
    let mut value = fixture();
    value["items"][0] = json!([920001, "Brass Practice Token", 0, 20]);
    assert_eq!(
        load(&value).unwrap().content_digest(),
        original.content_digest()
    );
}
