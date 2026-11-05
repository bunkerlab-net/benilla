// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn temp() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "benilla-event-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn ids() -> Ids {
    [
        "creature",
        "gameobject",
        "creature_template",
        "creature_equip_template",
        "spell_template",
        "quest_template",
        "mail_template",
    ]
    .into_iter()
    .map(|name| (name, BTreeSet::from([7])))
    .collect()
}
fn record(table: &str, fields: &[(&str, &str)]) -> Value {
    let cols: Vec<_> = fields
        .iter()
        .map(|(name, _)| json!({"COLUMN_NAME":name}))
        .collect();
    let cells: Vec<_> = fields
        .iter()
        .map(|(_, value)| Some(value.as_bytes()))
        .collect();
    disposition(
        table,
        0,
        &cols,
        &cells,
        &ids(),
        &BTreeMap::from([(1, (1, 5)), (2, (0, 0))]),
    )
    .unwrap()
}
#[test]
fn signed_spawn_memberships_are_distinct_from_unsigned_mail_events() {
    let r = record("game_event_creature", &[("guid", "7"), ("event", "-1")]);
    assert_eq!(r["values"]["event"], -1);
    assert_eq!(r["values"]["active_when"], "event_inactive");
    assert_eq!(r["accepted"], true);
    let r = record(
        "game_event_mail",
        &[
            ("event", "-1"),
            ("racemask", "256"),
            ("quest", "99"),
            ("mailtemplateid", "99"),
            ("senderentry", "99"),
        ],
    );
    assert_eq!(r["values"]["event"], 65535);
    assert_eq!(
        r["diagnostics"],
        json!([
            "invalid_race_mask",
            "missing_event",
            "missing_mail_template",
            "missing_quest",
            "missing_sender"
        ])
    );
    assert_eq!(r["accepted"], false);
}
#[test]
fn event_substitutions_clear_only_missing_dependencies() {
    let r = record(
        "game_event_creature_data",
        &[
            ("guid", "7"),
            ("event", "1"),
            ("modelid", "99"),
            ("equipment_id", "99"),
            ("entry_id", "99"),
            ("spell_start", "7"),
            ("spell_end", "99"),
        ],
    );
    assert_eq!(r["values"]["entry_id"], 0);
    assert_eq!(r["values"]["equipment_id"], 0);
    assert_eq!(r["values"]["spell_end"], 0);
    assert_eq!(r["values"]["spell_start"], 7);
    assert_eq!(r["values"]["modelid"], 99);
    assert_eq!(
        r["diagnostics"],
        json!([
            "entry_id_cleared",
            "equipment_id_cleared",
            "spell_end_cleared"
        ])
    );
}
#[test]
fn linked_calendars_disabled_events_and_quest_links_are_not_invented() {
    let r = record(
        "game_event",
        &[
            ("entry", "1"),
            ("schedule_type", "1"),
            ("occurence", "0"),
            ("length", "5"),
            ("linkedto", "99"),
            ("holiday", "0"),
        ],
    );
    assert_eq!(r["values"]["linkedto"], 0);
    assert_eq!(
        r["diagnostics"],
        json!(["disabled_occurrence", "invalid_link_cleared"])
    );
    let r = record(
        "game_event_time",
        &[
            ("entry", "2"),
            ("start_time", "2020-01-01 00:00:00"),
            ("end_time", "2030-01-01 00:00:00"),
        ],
    );
    assert_eq!(r["diagnostics"], json!(["calendar_ignored"]));
    let r = record("game_event_quest", &[("quest", "99"), ("event", "1")]);
    assert_eq!(r["accepted"], false);
    assert_eq!(r["diagnostics"], json!(["missing_quest"]));
}
#[test]
fn weather_retains_four_seasons_and_resets_only_chances_above_100() {
    let r = record(
        "game_weather",
        &[
            ("zone", "1"),
            ("spring_rain_chance", "101"),
            ("spring_snow_chance", "0"),
            ("spring_storm_chance", "0"),
            ("summer_rain_chance", "0"),
            ("summer_snow_chance", "100"),
            ("summer_storm_chance", "0"),
            ("fall_rain_chance", "0"),
            ("fall_snow_chance", "0"),
            ("fall_storm_chance", "99"),
            ("winter_rain_chance", "80"),
            ("winter_snow_chance", "80"),
            ("winter_storm_chance", "0"),
        ],
    );
    assert_eq!(
        r["values"]["seasons"],
        json!([[25, 0, 0], [0, 100, 0], [0, 0, 99], [80, 80, 0]])
    );
    assert_eq!(r["diagnostics"], json!(["spring_rain_chance_reset"]));
}
fn frame(ordinal: u64, value: u8) -> Vec<u8> {
    let mut raw = vec![0; 16];
    raw[4..12].copy_from_slice(&ordinal.to_le_bytes());
    raw[12..14].copy_from_slice(&1u16.to_le_bytes());
    raw[14..16].copy_from_slice(&1u16.to_le_bytes());
    raw.push(0);
    raw.extend_from_slice(&1u32.to_le_bytes());
    raw.push(value);
    let size = raw.len() as u32;
    raw[..4].copy_from_slice(&size.to_le_bytes());
    raw
}
fn stream(raw: &[u8]) -> Result<Value> {
    let path = temp();
    std::fs::write(&path, raw).unwrap();
    let d = json!({"rows":1,"bytes":raw.len(),"sha256":hex(&Sha256::digest(raw))});
    let result = verify_stream(&path, 1, &[hash_cells([Some(b"7".as_slice())])], &d);
    std::fs::remove_file(path).unwrap();
    result
}
#[test]
fn rehashed_occurrence_forgery_bad_ordinals_and_stream_bounds_refuse() {
    assert_eq!(stream(&frame(0, b'7')).unwrap()["rows"], 1);
    assert_eq!(
        stream(&frame(0, b'8')).unwrap_err().id,
        "events.view_mismatch"
    );
    assert_eq!(
        stream(&frame(1, b'7')).unwrap_err().id,
        "catalog.duplicate_row"
    );
    assert_eq!(stream(&[]).unwrap_err().id, "events.exhausted");
    assert_eq!(
        stream(&[frame(0, b'7'), vec![0]].concat()).unwrap_err().id,
        "events.trailing"
    );
}
#[test]
fn immutable_initial_state_distinguishes_empty_absent_and_invalid_timezone() {
    let path = temp();
    let env = temp();
    let raw = b"CREATE TABLE `game_event_status` (\n  `event` smallint(6) unsigned NOT NULL DEFAULT 0,\n  PRIMARY KEY (`event`)\n);\nLOCK TABLES `game_event_status` WRITE;\n/*!40000 ALTER TABLE `game_event_status` DISABLE KEYS */;\n/*!40000 ALTER TABLE `game_event_status` ENABLE KEYS */;\nUNLOCK TABLES;\n";
    std::fs::write(&path, raw).unwrap();
    let mut evidence = json!({"environment":{"TZ":"UTC","LC_ALL":"C"},"tzdata_utc":{"path":"/usr/share/zoneinfo/Etc/UTC","bytes":114,"sha256":"8b85846791ab2c8a5463c83a5be3c043e2570d7448434d41398969ed47e3e6f2"}});
    std::fs::write(&env, json::canonical(&evidence)).unwrap();
    let state = initial_state(&path, &env).unwrap();
    assert_eq!(state["events"], json!([]));
    assert_eq!(state["status_rows"], 0);
    assert_eq!(state["runtime_status"], "not_evaluated");
    assert_eq!(state["snapshot"]["sha256"], hex(&Sha256::digest(raw)));
    evidence["environment"]["TZ"] = json!("Europe/London");
    std::fs::write(&env, json::canonical(&evidence)).unwrap();
    assert_eq!(
        initial_state(&path, &env).unwrap_err().id,
        "events.calendar"
    );
    std::fs::write(&path, b"").unwrap();
    assert_eq!(
        initial_state(&path, &env).unwrap_err().id,
        "events.initial_state"
    );
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(env).unwrap();
}
