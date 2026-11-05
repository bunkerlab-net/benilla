// SPDX-License-Identifier: GPL-3.0-or-later
//! Pinned event/weather loader views, not an active-event or weather manager.
use super::*;
use std::{collections::BTreeSet, io::Write};
const LOADERS: &[&str] = &[
    "GameEvents/GameEventMgr:92",
    "GameEvents/GameEventMgr:107",
    "GameEvents/GameEventMgr:196",
    "GameEvents/GameEventMgr:237",
    "GameEvents/GameEventMgr:319",
    "GameEvents/GameEventMgr:410",
    "GameEvents/GameEventMgr:499",
    "GameEvents/GameEventMgr:557",
    "Weather/Weather:436",
];
const TABLES: &[&str] = &[
    "game_event",
    "game_event_time",
    "game_event_creature",
    "game_event_gameobject",
    "game_event_creature_data",
    "game_event_quest",
    "game_event_mail",
    "game_weather",
];
fn columns<'a>(schema: &'a Value, table: &str) -> Result<&'a [Value]> {
    schema["tables"][table]["columns"]
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| fail("catalog.schema"))
}
fn position(cols: &[Value], name: &str) -> Result<usize> {
    cols.iter()
        .position(|c| {
            c["COLUMN_NAME"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case(name))
        })
        .ok_or_else(|| fail("catalog.schema"))
}
fn cells<'a>(row: &RowView<'a>) -> Result<[Option<&'a [u8]>; 256]> {
    let mut result = [None; 256];
    for (i, c) in row.cells().enumerate() {
        result[i] = c?;
    }
    Ok(result)
}
fn hash_cells(values: impl IntoIterator<Item = Option<impl AsRef<[u8]>>>) -> [u8; 32] {
    let mut hash = Sha256::new();
    for cell in values {
        hash.update(
            cell.as_ref()
                .map_or(u32::MAX, |v| v.as_ref().len() as u32)
                .to_le_bytes(),
        );
        if let Some(v) = cell {
            hash.update(v.as_ref());
        }
    }
    hash.finalize().into()
}
fn expected(
    catalog: &mut VerifiedCatalog,
    schema: &Value,
    spec: &Value,
) -> Result<(usize, Vec<[u8; 32]>)> {
    let table = spec["relations"][0]
        .as_str()
        .ok_or_else(|| fail("events.query"))?;
    let query = spec["query"].as_str().ok_or_else(|| fail("events.query"))?;
    let projection = query
        .strip_prefix("SELECT ")
        .and_then(|s| s.split_once(" FROM "))
        .ok_or_else(|| fail("events.query"))?
        .0;
    let cols = columns(schema, table)?;
    let names: Vec<_> = projection.split(',').map(str::trim).collect();
    let mut hashes = Vec::new();
    if projection == "MAX(entry)" {
        let index = position(cols, "entry")?;
        let mut max: Option<(u32, Vec<u8>)> = None;
        catalog.scan(table, 0, 256, |row| {
            let values = cells(&row)?;
            if let Some(v) = values[index] {
                let n = super::spawns::uint(Some(v))?;
                if max.as_ref().is_none_or(|(old, _)| n > *old) {
                    max = Some((n, v.to_vec()));
                }
            }
            Ok(())
        })?;
        hashes.push(hash_cells([max.as_ref().map(|(_, v)| v.as_slice())]));
    } else if query.contains("LEFT OUTER JOIN") {
        let target = spec["relations"][1]
            .as_str()
            .ok_or_else(|| fail("events.query"))?;
        check(
            catalog.relations[target].rows * 192 <= DECODED as u64,
            "events.view_limit",
        )?;
        let guid = position(columns(schema, target)?, "guid")?;
        let mut matches = BTreeMap::<u32, Vec<Vec<u8>>>::new();
        catalog.scan(target, 0, 256, |row| {
            if let Some(v) = cells(&row)?[guid] {
                matches
                    .entry(super::spawns::uint(Some(v))?)
                    .or_default()
                    .push(v.to_vec());
            }
            Ok(())
        })?;
        let guid = position(cols, "guid")?;
        let indexes: Vec<_> = names
            .iter()
            .map(|n| {
                let (owner, col) = n.split_once('.').ok_or_else(|| fail("events.query"))?;
                Ok(if owner == target {
                    None
                } else {
                    Some(position(cols, col)?)
                })
            })
            .collect::<Result<_>>()?;
        catalog.scan(table, 0, 256, |row| {
            let values = cells(&row)?;
            let key = super::spawns::uint(values[guid])?;
            let found = matches.get(&key);
            for i in 0..found.map_or(1, Vec::len) {
                check(hashes.len() < DECODED / 128, "events.view_limit")?;
                hashes.push(hash_cells(indexes.iter().map(|index| {
                    index.map_or_else(|| found.map(|v| v[i].as_slice()), |i| values[i])
                })));
            }
            Ok(())
        })?;
    } else {
        let indexes: Vec<_> = names
            .iter()
            .map(|n| position(cols, n))
            .collect::<Result<_>>()?;
        catalog.scan(table, 0, 256, |row| {
            let values = cells(&row)?;
            check(hashes.len() < DECODED / 128, "events.view_limit")?;
            hashes.push(hash_cells(indexes.iter().map(|&i| values[i])));
            Ok(())
        })?;
    }
    hashes.sort_unstable();
    Ok((names.len(), hashes))
}
fn verify_stream(path: &Path, count: usize, expected: &[[u8; 32]], d: &Value) -> Result<Value> {
    let held = reader::Held::open(path, MAX_RAW)?;
    let mut buffer = vec![0; FRAME];
    let mut at = 0;
    let mut hash = Sha256::new();
    let mut seen = Vec::with_capacity(expected.len());
    for ordinal in 0..expected.len() {
        check(at + 4 <= held.len(), "events.exhausted")?;
        held.read_into(at, &mut buffer[..4])?;
        let size = le(&buffer, 0, 4)? as usize;
        check((16..=FRAME).contains(&size), "catalog.frame_limit")?;
        held.read_into(at + 4, &mut buffer[4..size])?;
        let row = RowView::new(&buffer[..size], ordinal as u64, count)?;
        row.validate()?;
        seen.push(hash_cells(cells(&row)?[..count].iter().copied()));
        hash.update(&buffer[..size]);
        at += size as u64;
    }
    check(at == held.len(), "events.trailing")?;
    let occurrence = hex(&hash.finalize());
    check(
        d["bytes"] == held.len() && d["rows"] == seen.len() as u64 && d["sha256"] == occurrence,
        "events.hash",
    )?;
    seen.sort_unstable();
    check(seen == expected, "events.view_mismatch")?;
    let mut multiset = Sha256::new();
    for h in seen {
        multiset.update(h);
    }
    Ok(
        json!({"rows":expected.len(),"occurrence_sha256":occurrence,"multiset_sha256":hex(&multiset.finalize())}),
    )
}
fn input_bytes(path: &Path) -> Result<Vec<u8>> {
    let held = reader::Held::open(path, CHUNK as u64)?;
    held.read(0, held.len() as usize, &mut Metrics::default(), CACHE)
}
pub(super) fn initial_state(snapshot: &Path, environment: &Path) -> Result<Value> {
    let raw = input_bytes(snapshot)?;
    let text = std::str::from_utf8(&raw).map_err(|_| fail("events.initial_state"))?;
    let marker = "LOCK TABLES `game_event_status` WRITE;\n";
    check(
        text.matches(marker).count() == 1
            && text.matches("CREATE TABLE `game_event_status` (").count() == 1,
        "events.initial_state",
    )?;
    let body = text
        .split_once(marker)
        .and_then(|(_, s)| s.split_once("UNLOCK TABLES;"))
        .ok_or_else(|| fail("events.initial_state"))?
        .0;
    check(text.matches("INSERT INTO `game_event_status`").count() == body.matches("INSERT INTO `game_event_status`").count()
        && text.contains("CREATE TABLE `game_event_status` (\n  `event` smallint(6) unsigned NOT NULL DEFAULT 0,\n  PRIMARY KEY (`event`)\n)"),"events.initial_state")?;
    let mut lines = body.lines();
    check(
        lines.next() == Some("/*!40000 ALTER TABLE `game_event_status` DISABLE KEYS */;"),
        "events.initial_state",
    )?;
    let mut values = Vec::<u16>::new();
    let mut unique = BTreeSet::new();
    let mut finished = false;
    for line in lines {
        check(!finished, "events.initial_state")?;
        if line == "/*!40000 ALTER TABLE `game_event_status` ENABLE KEYS */;" {
            finished = true;
            continue;
        }
        let payload = line
            .strip_prefix("INSERT INTO `game_event_status` VALUES ")
            .and_then(|s| s.strip_suffix(';'))
            .ok_or_else(|| fail("events.initial_state"))?;
        for entry in payload.split(',') {
            let n = entry
                .strip_prefix('(')
                .and_then(|s| s.strip_suffix(')'))
                .filter(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()))
                .ok_or_else(|| fail("events.initial_state"))?;
            let n = n.parse::<u16>().map_err(|_| fail("events.initial_state"))?;
            check(
                values.len() < 65536 && unique.insert(n),
                "events.initial_state",
            )?;
            values.push(n);
        }
    }
    check(finished, "events.initial_state")?;
    let evidence_raw = input_bytes(environment)?;
    let evidence = json::decode_with_limits(&evidence_raw, CHUNK, 32, 100000)?;
    check(
        evidence["environment"]["TZ"] == "UTC" && evidence["environment"]["LC_ALL"] == "C",
        "events.calendar",
    )?;
    let tzdata = &evidence["tzdata_utc"];
    catalog_schema::fields(tzdata, &["path", "bytes", "sha256"], "events.calendar")?;
    check(
        tzdata["bytes"] == 114
            && tzdata["sha256"]
                == "8b85846791ab2c8a5463c83a5be3c043e2570d7448434d41398969ed47e3e6f2",
        "events.calendar",
    )?;
    Ok(
        json!({"snapshot":{"sha256":hex(&Sha256::digest(&raw)),"bytes":raw.len()},"database":"characters","table":"game_event_status","query":"SELECT event FROM game_event_status","source":"src/game/GameEvents/GameEventMgr.cpp:640","events":values,"status_rows":values.len(),"calendar":{"policy":"recorded_samples_paused_server_v1","timezone":"UTC","locale":"C","calendar":"Gregorian","tzdata":tzdata,"evidence":{"sha256":hex(&Sha256::digest(&evidence_raw)),"bytes":evidence_raw.len()},"runtime_clock_coverage":false},"runtime_status":"not_evaluated"}),
    )
}
type Ids = BTreeMap<&'static str, BTreeSet<u32>>;
type Events = BTreeMap<u32, (u32, u32)>;
fn valid(events: &Events, event: u32) -> bool {
    events
        .get(&event)
        .is_some_and(|&(schedule, length)| schedule == 0 || length > 0)
}
fn disposition(
    table: &str,
    ordinal: u64,
    cols: &[Value],
    row: &[Option<&[u8]>],
    ids: &Ids,
    events: &Events,
) -> Result<Value> {
    let mut values = serde_json::Map::new();
    for (i, c) in cols.iter().enumerate() {
        let name = c["COLUMN_NAME"]
            .as_str()
            .ok_or_else(|| fail("catalog.schema"))?
            .to_ascii_lowercase();
        if !matches!(name.as_str(), "description" | "start_time" | "end_time") {
            values.insert(name, Value::from(super::spawns::uint(row[i])?));
        }
    }
    let mut flags = Vec::<String>::new();
    let n = |name: &str| values[name].as_u64().unwrap_or(0) as u32;
    match table {
        "game_event" => {
            let entry = n("entry") & 65535;
            if entry == 0 {
                flags.push("reserved_event".into());
            }
            let clear = n("linkedto") != 0 && !valid(events, n("linkedto"));
            if clear {
                flags.push("invalid_link_cleared".into());
            }
            if ![0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 13].contains(&n("schedule_type")) {
                flags.push("unknown_schedule".into());
            }
            if n("occurence") == 0 {
                flags.push("disabled_occurrence".into());
            }
            if n("length") == 0 && n("schedule_type") != 0 {
                flags.push("invalid_length".into());
            }
            if n("occurence") != 0 && n("occurence") < n("length") {
                flags.push("invalid_occurrence".into());
            }
            if n("occurence") == 0 {
                let length = n("length");
                values.insert("occurence".into(), Value::from(length));
            }
            if clear {
                values.insert("linkedto".into(), Value::from(0));
            }
        }
        "game_event_time" => {
            let event = n("entry") & 65535;
            if !events.contains_key(&event) {
                flags.push("missing_event".into());
            }
            if events.get(&event).is_some_and(|&(s, _)| s != 1 && s != 11) {
                flags.push("calendar_ignored".into());
            }
            for name in ["start_time", "end_time"] {
                values.insert(
                    format!("{name}_hex"),
                    row[position(cols, name)?].map_or(Value::Null, |v| Value::String(hex(v))),
                );
            }
        }
        "game_weather" => {
            for (name, value) in &mut values {
                if name != "zone" && value.as_u64().unwrap_or(0) > 100 {
                    flags.push(format!("{name}_reset"));
                    *value = Value::from(25);
                }
            }
            let seasons: Vec<Vec<_>> = ["spring", "summer", "fall", "winter"]
                .iter()
                .map(|s| {
                    ["rain", "snow", "storm"]
                        .iter()
                        .map(|w| values[&format!("{s}_{w}_chance")].clone())
                        .collect()
                })
                .collect();
            values.insert("seasons".into(), json!(seasons));
        }
        _ => {
            let event = n("event") & 65535;
            let signed = event as i16 as i32;
            let membership = matches!(table, "game_event_creature" | "game_event_gameobject");
            let checked = if membership {
                signed.unsigned_abs()
            } else {
                event
            };
            if checked == 0 {
                flags.push("reserved_event".into());
            }
            if !valid(events, checked) {
                flags.push("missing_event".into());
            }
            if membership {
                let target = if table == "game_event_creature" {
                    "creature"
                } else {
                    "gameobject"
                };
                if !ids[target].contains(&n("guid")) {
                    flags.push("missing_spawn".into());
                }
                values.insert(
                    "active_when".into(),
                    Value::from(if signed < 0 {
                        "event_inactive"
                    } else {
                        "event_active"
                    }),
                );
                values.insert("event".into(), Value::from(signed));
            } else if table == "game_event_creature_data" {
                if !ids["creature"].contains(&n("guid")) {
                    flags.push("missing_spawn".into());
                }
                for (col, target) in [
                    ("equipment_id", "creature_equip_template"),
                    ("entry_id", "creature_template"),
                    ("spell_start", "spell_template"),
                    ("spell_end", "spell_template"),
                ] {
                    let value = values[col].as_u64().unwrap_or(0) as u32;
                    if value != 0 && !ids[target].contains(&value) {
                        flags.push(format!("{col}_cleared"));
                        values.insert(col.into(), Value::from(0));
                    }
                }
                values.insert("event".into(), Value::from(event));
            } else if table == "game_event_quest" {
                if !ids["quest_template"].contains(&n("quest")) {
                    flags.push("missing_quest".into());
                }
                values.insert("event".into(), Value::from(event));
            } else if table == "game_event_mail" {
                if n("racemask") & 0xff == 0 {
                    flags.push("invalid_race_mask".into());
                }
                if n("quest") != 0 && !ids["quest_template"].contains(&n("quest")) {
                    flags.push("missing_quest".into());
                }
                if !ids["creature_template"].contains(&n("senderentry")) {
                    flags.push("missing_sender".into());
                }
                if !ids["mail_template"].contains(&n("mailtemplateid")) {
                    flags.push("missing_mail_template".into());
                }
                values.insert("event".into(), Value::from(event));
            }
        }
    }
    // Diagnostic order is independent of SQL schema column order.
    flags.sort_unstable();
    let accepted = !flags.iter().any(|f| {
        [
            "reserved_event",
            "missing_event",
            "missing_spawn",
            "missing_quest",
            "missing_sender",
            "missing_mail_template",
            "invalid_race_mask",
            "calendar_ignored",
        ]
        .contains(&f.as_str())
    });
    Ok(json!({"ordinal":ordinal,"values":values,"diagnostics":flags,"accepted":accepted}))
}
fn plans(catalog: &mut VerifiedCatalog, schema: &Value, records: &Path) -> Result<Value> {
    let dependencies = [
        ("creature", "guid"),
        ("gameobject", "guid"),
        ("creature_template", "entry"),
        ("creature_equip_template", "entry"),
        ("quest_template", "entry"),
        ("spell_template", "id"),
    ];
    let charge = dependencies
        .iter()
        .map(|(t, _)| catalog.relations[*t].rows * 128)
        .sum::<u64>()
        + catalog.relations["game_event"].rows * 256;
    check(charge <= DECODED as u64, "events.view_limit")?;
    let mut ids = Ids::new();
    for (table, name) in dependencies {
        let i = position(columns(schema, table)?, name)?;
        let mut set = BTreeSet::new();
        catalog.scan(table, 0, 256, |row| {
            set.insert(super::spawns::uint(cells(&row)?[i])?);
            Ok(())
        })?;
        ids.insert(table, set);
    }
    let dbcs = bounded_json(&catalog.object(&descriptor(&catalog.manifest["dbc_root"])?, 0)?)?;
    check(!dbcs["MailTemplate.dbc"].is_null(), "events.mail_dbc")?;
    let blob = catalog.object(&descriptor(&dbcs["MailTemplate.dbc"])?, 0)?;
    check(blob.len() >= 20 && &blob[..4] == b"WDBC", "events.mail_dbc")?;
    let count = le(&blob, 4, 4)? as usize;
    let fields = le(&blob, 8, 4)?;
    let size = le(&blob, 12, 4)? as usize;
    let strings = le(&blob, 16, 4)? as usize;
    check(
        count <= 100000
            && fields == 10
            && size == 40
            && strings > 0
            && blob.len() == 20 + count * size + strings,
        "events.mail_dbc",
    )?;
    let mut mail = BTreeSet::new();
    for i in 0..count {
        mail.insert(le(&blob, 20 + i * size, 4)? as u32);
    }
    ids.insert("mail_template", mail);
    let cols = columns(schema, "game_event")?;
    let entry = position(cols, "entry")?;
    let schedule = position(cols, "schedule_type")?;
    let length = position(cols, "length")?;
    let mut events = Events::new();
    catalog.scan("game_event", 0, 256, |row| {
        let c = cells(&row)?;
        events.insert(
            super::spawns::uint(c[entry])? & 65535,
            (
                super::spawns::uint(c[schedule])?,
                super::spawns::uint(c[length])?,
            ),
        );
        Ok(())
    })?;
    std::fs::create_dir(records).map_err(|_| fail("catalog.receipt"))?;
    let mut result = serde_json::Map::new();
    for &table in TABLES {
        let cols = columns(schema, table)?;
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(records.join(format!("{table}.jsonl")))
            .map_err(|_| fail("catalog.receipt"))?;
        let mut hash = Sha256::new();
        let mut count = 0u64;
        let mut size = 0u64;
        let mut diagnostics = BTreeMap::<String, u64>::new();
        catalog.scan(table, 0, 256, |row| {
            let record = disposition(table, row.ordinal, cols, &cells(&row)?, &ids, &events)?;
            for flag in record["diagnostics"]
                .as_array()
                .ok_or_else(|| fail("events.plan"))?
            {
                *diagnostics
                    .entry(flag.as_str().unwrap_or_default().to_owned())
                    .or_default() += 1;
            }
            let mut raw = json::canonical(&record);
            raw.push(b'\n');
            check(
                raw.len() <= FRAME && size + raw.len() as u64 <= MAX_RAW,
                "events.byte_limit",
            )?;
            output
                .write_all(&raw)
                .map_err(|_| fail("catalog.receipt"))?;
            hash.update(&raw);
            size += raw.len() as u64;
            count += 1;
            Ok(())
        })?;
        output.sync_all().map_err(|_| fail("catalog.receipt"))?;
        result.insert(table.into(),json!({"rows":count,"bytes":size,"sha256":hex(&hash.finalize()),"diagnostics":diagnostics}));
    }
    Ok(Value::Object(result))
}
/// Independently verifies complete event/weather source input views and initial DB reference.
pub fn diagnose_catalog_events(
    root: &Path,
    occurrences: &Path,
    snapshot: &Path,
    environment: &Path,
    records: &Path,
) -> Result<Value> {
    let state = initial_state(snapshot, environment)?;
    let mut catalog = VerifiedCatalog::open(root, false)?;
    prepare(&mut catalog, occurrences, state, records)
}
pub(super) fn prepare(
    catalog: &mut VerifiedCatalog,
    occurrences: &Path,
    state: Value,
    records: &Path,
) -> Result<Value> {
    let schema = bounded_json(&catalog.object(&descriptor(&catalog.manifest["schema_root"])?, 0)?)?;
    let meta = bounded_json(&input_bytes(&occurrences.join("occurrences.json"))?)?;
    catalog_schema::fields(
        &meta,
        &["format", "streams", "complete", "source_snapshot"],
        "events.occurrences",
    )?;
    check(
        meta["format"] == "benilla-event-occurrences-v1"
            && meta["complete"] == false
            && meta["source_snapshot"] == "phase0-sequential-diagnostic-not-F1",
        "events.occurrences",
    )?;
    let keys: Vec<_> = (0..LOADERS.len()).map(|i| i.to_string()).collect();
    catalog_schema::fields(
        &meta["streams"],
        &keys.iter().map(String::as_str).collect::<Vec<_>>(),
        "events.occurrences",
    )?;
    let mut loaders = serde_json::Map::new();
    for (i, id) in LOADERS.iter().enumerate() {
        let spec = &catalog_schema::POLICY["loaders"][id];
        let key = i.to_string();
        let d = &meta["streams"][&key];
        catalog_schema::fields(d, &["query", "rows", "bytes", "sha256"], "events.query")?;
        check(d["query"] == spec["query"], "events.query")?;
        check(
            d["rows"].as_u64().is_some_and(|n| n <= 2000000)
                && d["bytes"].as_u64().is_some_and(|n| n <= MAX_RAW),
            "events.hash",
        )?;
        let (columns, wanted) = expected(catalog, &schema, spec)?;
        let mut report = verify_stream(
            &occurrences.join(format!("{key}.rows")),
            columns,
            &wanted,
            d,
        )?;
        report["query"] = spec["query"].clone();
        report["source"] = spec["source"].clone();
        loaders.insert(key, report);
    }
    let mut families = serde_json::Map::new();
    for &table in TABLES {
        let mut hash = Sha256::new();
        let mut count = 0u64;
        catalog.scan(table, 0, 256, |row| {
            hash.update(row.frame);
            count += 1;
            Ok(())
        })?;
        families.insert(
            table.into(),
            json!({"rows":count,"raw_sha256":hex(&hash.finalize())}),
        );
    }
    let plans = plans(catalog, &schema, records)?;
    let mut report = json!({"format":"benilla-event-views-v1","catalog_sha256":catalog.manifest["catalog_sha256"],"families":families,"loaders":loaders,"initial_state":state,"plans":plans,"complete":false,"refusals":["events.reference_execution_missing","events.calendar_execution_missing","events.pool_execution_missing","events.gameplay_not_admitted"]});
    report["weather_calendar"] = json!({"source":"src/game/Weather/Weather.cpp:112",
        "season_by_yday":(0..366).map(|day|((day+365-78)/91)%4).collect::<Vec<u32>>()});
    report["view_sha256"] = Value::String(json::digest(&report));
    Ok(report)
}
#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
