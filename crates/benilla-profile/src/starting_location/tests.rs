use super::*;
use crate::{
    tests::{directory, input},
    valid_classes, ProfileStore, SpawnPolicy,
};
use serde_json::{json, Value};
use std::fs;

// Entirely authored geometry, deliberately unlike any actual racial start.
fn fixture(race: u8, class: u8) -> Value {
    let mut value = json!({
        "format": "benilla-classic-source-slice-v2",
        "core_revision": CORE, "database_revision": DATABASE, "client_build": 5875,
        "revision_claims_verified_against_installation": false,
        "selection": {"format_version": 2, "starter": {"race": race, "class": class, "level_cap": 1}},
        "consistency": "transactional_snapshot",
        "tables": {"playercreateinfo": [{"race": race, "class": class,
            "map": u32::from(race % 2), "zone": u32::from(race) + 900,
            "position_x": {"decimal": format!("{}", 100 + u32::from(race))},
            "position_y": {"decimal": format!("{}", 200 + u32::from(class))},
            "position_z": {"decimal": "31.125"}, "orientation": {"decimal": "1.25"}}]},
        "table_sha256": {"playercreateinfo": ""},
        "report": {"runnable": false, "capture_complete": false}
    });
    rehash(&mut value);
    value
}
fn rehash(value: &mut Value) {
    value.sort_all_objects();
    value["table_sha256"]["playercreateinfo"] = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&value["tables"]["playercreateinfo"]).unwrap())
    )
    .into();
}
fn load(value: &Value) -> Result<ClassicStart, Error> {
    ClassicStart::from_json(&serde_json::to_vec(value).unwrap())
}
#[test]
fn all_forty_combinations_create_persist_move_reopen_and_recover_without_reset() {
    let mut count = 0;
    for race in 1..=8 {
        for &class in valid_classes(race) {
            count += 1;
            let start = load(&fixture(race, class)).unwrap();
            let (_temp, path) = directory();
            let mut store = ProfileStore::create(&path).unwrap();
            let mut new = input("Fixture");
            new.race = race;
            new.class = class;
            let id = store.create_character_from_start(new, &start).unwrap();
            let created = store.character(id).unwrap().clone();
            assert_eq!(
                created.position,
                Position {
                    map_id: u32::from(race % 2),
                    zone_id: u32::from(race) + 900,
                    position: [100.0 + f32::from(race), 200.0 + f32::from(class), 31.125],
                    orientation: 1.25
                }
            );
            assert_eq!(
                created.spawn_policy,
                SpawnPolicy::ClassicSourceV1 {
                    capture_sha256: start.capture_sha256()
                }
            );
            assert!(created.spawn_policy.position().is_err());
            drop(store);
            let mut store = ProfileStore::open(&path).unwrap();
            assert_eq!(store.selected_character(), Some(&created));
            // A different map, zone, height and facing must survive; no source lookup on resume.
            let moved = Position {
                map_id: 1 - created.position.map_id,
                zone_id: 950,
                position: [-54.25, 67.5, 91.75],
                orientation: 5.75,
            };
            store.save_position(id, moved).unwrap();
            store.archive_character(store.owner_id(), id).unwrap();
            drop(store);
            let mut store = ProfileStore::open(&path).unwrap();
            store.restore_character(store.owner_id(), id).unwrap();
            store.select_character(id).unwrap();
            assert_eq!(store.character(id).unwrap().position, moved);
            drop(store);
            assert_eq!(
                ProfileStore::open(&path)
                    .unwrap()
                    .selected_character()
                    .unwrap()
                    .position,
                moved
            );
        }
    }
    assert_eq!(count, 40);
}
#[test]
fn mismatched_and_invalid_combinations_never_write_or_fall_back() {
    let start = load(&fixture(6, 11)).unwrap();
    let (_temp, path) = directory();
    let mut store = ProfileStore::create(&path).unwrap();
    let bytes = fs::read(path.join("profile.json")).unwrap();
    for (race, class) in [(1, 1), (6, 1), (0, 1), (9, 1), (6, 4), (7, 5), (4, 8)] {
        let mut new = input("Fixture");
        new.race = race;
        new.class = class;
        assert!(store.create_character_from_start(new, &start).is_err());
        assert!(store.characters().is_empty());
        assert_eq!(fs::read(path.join("profile.json")).unwrap(), bytes);
    }
}
#[test]
fn legacy_positions_and_policy_are_unchanged_by_new_source_support() {
    let (_temp, path) = directory();
    let mut store = ProfileStore::create(&path).unwrap();
    let mut new = input("Legacy");
    new.race = 6;
    new.class = 11;
    let old = store.create_character(new).unwrap();
    let moved = Position {
        map_id: 0,
        zone_id: 42,
        position: [19.0, 23.0, 47.0],
        orientation: 3.0,
    };
    store.save_position(old, moved).unwrap();
    let old_record = store.character(old).unwrap().clone();
    let start = load(&fixture(1, 1)).unwrap();
    store
        .create_character_from_start(input("Source"), &start)
        .unwrap();
    drop(store);
    let mut store = ProfileStore::open(&path).unwrap();
    assert_eq!(store.character(old), Some(&old_record));
    assert_eq!(old_record.spawn_policy, SpawnPolicy::NorthshireMeetingV1);
    store.archive_character(store.owner_id(), old).unwrap();
    store.restore_character(store.owner_id(), old).unwrap();
    assert_eq!(store.character(old), Some(&old_record));
}
#[test]
fn malformed_missing_duplicate_wrong_revision_and_unsupported_rows_fail_closed() {
    for case in 0..18 {
        let mut value = fixture(1, 1);
        match case {
            0 => value["tables"]["playercreateinfo"] = json!([]),
            1 => {
                let row = value["tables"]["playercreateinfo"][0].clone();
                value["tables"]["playercreateinfo"] = json!([row, row]);
            }
            2 => value["tables"]["playercreateinfo"][0]["class"] = json!(7),
            3 => value["tables"]["playercreateinfo"][0]["map"] = json!(2),
            4 => value["tables"]["playercreateinfo"][0]["zone"] = json!(10001),
            5 => value["tables"]["playercreateinfo"][0]["position_x"] = json!({"decimal": "NaN"}),
            6 => value["tables"]["playercreateinfo"][0]["position_y"] = json!({"decimal": "20001"}),
            7 => value["tables"]["playercreateinfo"][0]["orientation"] = json!({"decimal": "101"}),
            8 => value["selection"]["starter"]["race"] = json!(2),
            9 => value["core_revision"] = json!("unreviewed"),
            10 => value["database_revision"] = json!("unreviewed"),
            11 => value["format"] = json!("benilla-classic-source-slice-v1"),
            12 => value["client_build"] = json!(12340),
            13 => value["revision_claims_verified_against_installation"] = json!(true),
            14 => value["report"]["runnable"] = json!(true),
            15 => value["tables"]["playercreateinfo"][0]["extra"] = json!(1),
            16 => {
                value["tables"]["playercreateinfo"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("orientation");
            }
            17 => value["tables"]["playercreateinfo"][0]["position_z"] = json!(31),
            _ => unreachable!(),
        }
        rehash(&mut value);
        assert!(load(&value).is_err(), "case {case}");
    }
    let mut value = fixture(1, 1);
    value["tables"]["playercreateinfo"][0]["zone"] = json!(9010);
    assert!(load(&value).is_err(), "row hash must be verified");
    let bytes = serde_json::to_string(&fixture(1, 1)).unwrap();
    assert!(ClassicStart::from_json(
        bytes
            .replace("\"race\":1", "\"race\":1,\"race\":1")
            .as_bytes()
    )
    .is_err());
    assert!(ClassicStart::from_json(b"{}").is_err());
    assert!(ClassicStart::from_json(b"{broken").is_err());
    let mut repeated = std::io::repeat(b' ');
    assert!(ClassicStart::read(&mut repeated).is_err());
}
#[test]
fn only_canonical_finite_decimal_strings_are_accepted() {
    for text in [
        "", "-", "+1", "01", "-0", "1.", ".5", "1.00", "1e2", "Infinity", "1.2.3", " 1", "1\n",
    ] {
        let mut value = fixture(1, 1);
        value["tables"]["playercreateinfo"][0]["position_x"] = json!({"decimal": text});
        rehash(&mut value);
        assert!(load(&value).is_err(), "{text:?}");
    }
}

#[test]
fn captured_table_hash_matches_existing_python_canonicalizer() {
    // Generated independently with classic_export.core.digest on this authored row.
    let value = fixture(1, 1);
    assert_eq!(
        value["table_sha256"]["playercreateinfo"],
        "4eb096fc95ad47794a28301edd3f3854ea8ab2bb3e6deeebf07059f58fa74288"
    );
    let compact = serde_json::to_vec(&value).unwrap();
    let pretty = serde_json::to_vec_pretty(&value).unwrap();
    let a = ClassicStart::read(compact.as_slice()).unwrap();
    let b = ClassicStart::read(pretty.as_slice()).unwrap();
    assert_eq!(a.position_for(1, 1).unwrap(), b.position_for(1, 1).unwrap());
    assert_ne!(
        a.capture_sha256(),
        b.capture_sha256(),
        "digest tracks exact input bytes"
    );
}
